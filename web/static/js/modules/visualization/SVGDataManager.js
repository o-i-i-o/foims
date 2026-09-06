import { t } from "../../utils/i18n.js";
import { nextFrame } from "../../utils/helpers.js";
import { fetchAllPages, notifyLoadFailure } from "../../utils/pagedFetch.js";

// 机柜可视化布局常量
/** 画布四周预留边距：首柜距左缘 / 柜底距底缘（像素，viewBox 1:1） */
const CABINET_EDGE_PADDING = 16;
/** 相邻机柜水平间距 */
const CABINET_GAP = 50;
/** 机柜宽度 */
const CABINET_WIDTH = 150;
/** 每渲染完一批机柜统一拉取该批机位 IP 信息 */
const CABINET_BATCH_SIZE = 16;

/**
 * 机柜容量缺省值：与后端建表默认（foims-init cabinets DDL `capacity DEFAULT 42`）一致。
 * 列表接口可能缺失该字段，渲染时统一回退到此常量。
 */
export const DEFAULT_CABINET_CAPACITY = 42;

/**
 * 布局坐标合法性校验：x/y 有限且非负、宽高有限且为正。
 * 加载侧对异常坐标（NaN/负值/零宽高）按"无有效布局"处理并告警，
 * 避免 NaN 进入 setViewBox 或负尺寸元素进入画布。
 */
export function isValidLayoutPosition(position) {
  return Boolean(
    position &&
    Number.isFinite(position.x) &&
    Number.isFinite(position.y) &&
    Number.isFinite(position.width) &&
    Number.isFinite(position.height) &&
    position.x >= 0 &&
    position.y >= 0 &&
    position.width > 0 &&
    position.height > 0
  );
}

export class SVGDataManager {
  constructor(core, renderer) {
    this.core = core;
    this.renderer = renderer;
    this.apiGet = core.apiGet;
    this.apiPost = core.apiPost;
    this.apiDelete = core.apiDelete;
    this.showToast = core.showToast;
    // 机柜渲染代次：并发 loadSavedLayout/autoDraw/resize 重排时作废旧渲染
    this.cabinetRenderToken = 0;
    // 机柜分批渲染完成标志：渲染在途（画布为半成品）时 saveLayout 拒绝，
    // 避免把只画了前几批的部分布局整表落库
    this.cabinetRenderSettled = true;
    // 工位渲染代次：与机柜分支同型，防止快速切换房间时两次加载交错绘制
    this.workstationRenderToken = 0;
    // 当前房间本次加载/绘制的合法元素 id 集合（保存布局时用于归属过滤）
    this.layoutOwnerIds = new Set();
  }

  /** 数据获取失败时的统一提示（请求异常或响应形状异常） */
  _notifyLoadFailure(error, what) {
    notifyLoadFailure(this.showToast, error, what);
  }

  /**
   * 分页拉取全量列表（共享实现在 utils/pagedFetch.js）。
   * @param {string} path 不含分页参数的接口路径
   * @param {string} what 失败提示用途描述
   * @returns {Promise<Array>} 拉取到的条目（失败时为已获取的部分或空数组）
   */
  async _fetchAllPages(path, what) {
    return fetchAllPages({
      apiGet: this.apiGet,
      showToast: this.showToast,
      path,
      what
    });
  }

  async fetchWorkstationsByRoom(roomId) {
    return this._fetchAllPages(`/api/resources/workstations?room_id=${roomId}`, "获取工位数据");
  }

  async fetchIps() {
    return this._fetchAllPages("/api/resources/ip", "获取IP");
  }

  async fetchCabinetsByRoom(roomId) {
    try {
      const result = await this.apiGet(`/api/resources/layouts/room-cabinets/${roomId}`);
      if (result.success) {
        // 该接口返回裸数组（机柜含内嵌机位），非数组视为异常数据
        return Array.isArray(result.data) ? result.data : [];
      }
      this._notifyLoadFailure(result.message, "获取房间机柜数据");
      return [];
    } catch (error) {
      this._notifyLoadFailure(error, "获取房间机柜数据");
      return [];
    }
  }

  async loadSavedLayout(id) {
    this.core.currentRoomId = id;

    // 本次加载开启的渲染代次（失败处理据此判断本次加载是否已被取代）
    let token = null;

    try {
      if (this.core.type === "workstation") {
        token = this.beginWorkstationRender();

        const [layoutResult, workstations, ipDetails] = await Promise.all([
          this.apiGet(`/api/resources/layouts/workstation/${id}`),
          this.fetchWorkstationsByRoom(id),
          this.fetchIps()
        ]);
        // await 后校验代次：已被更新的渲染取代时丢弃本次结果，不再写入画布。
        // 返回 null（≠false"无布局"）：调用方据此放弃 autoDraw + saveLayout，
        // 防止用过期 roomId 自动排布并落库覆盖新房间已保存的布局
        if (token !== this.workstationRenderToken) {
          return null;
        }
        // 布局请求失败（5xx/网络，apiClient 不抛异常）必须与"确认无布局"
        // 区分：按无布局处理会触发 autoDraw + saveLayout 整表覆盖该房间
        // 已保存的布局，一次瞬时故障即破坏数据
        if (!layoutResult.success) {
          this._notifyLoadFailure(layoutResult.message, "获取房间布局");
          return "error";
        }

        const ipMap = new Map();
        if (Array.isArray(ipDetails)) {
          ipDetails.forEach((ipDetail) => {
            if (!ipDetail.workstation_id) {
              return;
            }
            // 同一工位多条 IP 时 active 优先（与 autoDrawWorkstations 取值策略一致）
            const existing = ipMap.get(ipDetail.workstation_id);
            if (!existing || (existing.status !== "active" && ipDetail.status === "active")) {
              ipMap.set(ipDetail.workstation_id, ipDetail);
            }
          });
        }

        let layoutData = [];
        let hasSavedLayout = false;

        if (
          layoutResult.success &&
          layoutResult.data &&
          Array.isArray(layoutResult.data) &&
          layoutResult.data.length > 0
        ) {
          layoutData = layoutResult.data;
          hasSavedLayout = true;
        }

        const doorItem = layoutData.find((item) => item.element_type === "door");
        // 门布局存于 element_layouts，绘制时带上已保存记录的主键，保证可拖拽、可保存
        const doorElement = this.renderer.drawDoor(doorItem?.id);

        const doorBounds = this._positionDoorElement(doorElement, doorItem);
        let maxX = doorBounds.maxX;
        let maxY = doorBounds.maxY;

        if (hasSavedLayout) {
          // 布局项以小写 id 建 Map：工位多时避免每个工位全量 find（O(N×M)）
          const layoutById = new Map();
          layoutData.forEach((item) => {
            if (item.element_type !== "door" && item.id) {
              layoutById.set(item.id.toLowerCase(), item);
            }
          });
          workstations.forEach((workstation, index) => {
            const savedItem = layoutById.get(workstation.id.toLowerCase());
            // 异常坐标（NaN/负值/零宽高）不进画布：告警后回退默认网格排布
            if (savedItem && savedItem.position && !isValidLayoutPosition(savedItem.position)) {
              console.warn(
                "工位已保存坐标异常，已回退默认排布:",
                workstation.id,
                savedItem.position
              );
            } else if (savedItem && isValidLayoutPosition(savedItem.position)) {
              workstation.position = savedItem.position;
            }
            if (!workstation.position) {
              const gap = 20;
              const width = 160;
              const height = 160;
              const cols = 4;
              const col = index % cols;
              const row = Math.floor(index / cols);
              workstation.position = {
                x: 150 + col * (width + gap),
                y: 100 + row * (height + gap),
                width,
                height
              };
            }
            workstation.ipDetail = ipMap.get(workstation.id);
            this.renderer.drawWorkstation(workstation);

            const pos = workstation.position;
            maxX = Math.max(maxX, pos.x + pos.width);
            maxY = Math.max(maxY, pos.y + pos.height);
          });
        }

        if (maxX > 0 || maxY > 0) {
          const padding = 50;
          // viewBox 同时覆盖容器尺寸，网格背景铺满画布，
          // 内容较小时右侧/下方不会留出空白带（左上角锚定由 preserveAspectRatio 保证）
          const cw = this.core.container.clientWidth || 0;
          const ch = this.core.container.clientHeight || 0;
          this.core.setViewBox(
            0,
            0,
            Math.max(1000, maxX + padding, cw),
            Math.max(800, maxY + padding, ch)
          );
        }

        // 记录本次绘制的合法元素 id（门 + 房间工位清单）：保存布局时用于归属过滤
        this.setLayoutOwnerIds([doorElement.dataset.id, ...workstations.map((w) => w.id)]);

        return hasSavedLayout;
      }
      if (this.core.type === "cabinet") {
        token = this.beginCabinetRender();

        const [layoutResult, cabinets] = await Promise.all([
          this.apiGet(`/api/resources/layouts/positions/${id}`),
          this.fetchCabinetsByRoom(id)
        ]);
        // 代次过期返回 null（同工位分支）：调用方放弃后续自动排布动作
        if (token !== this.cabinetRenderToken) {
          return null;
        }
        // 与工位分支同口径：布局请求失败不得按"无布局"处理
        //（机柜视图的自动排布同样会落库覆盖已保存布局）
        if (!layoutResult.success) {
          this._notifyLoadFailure(layoutResult.message, "获取机柜布局");
          // 本次不会有渲染跟进:恢复完成标志,避免 saveLayout 被永久拒绝
          this.cabinetRenderSettled = true;
          return "error";
        }

        let layoutData = [];
        let hasSavedLayout = false;

        if (
          layoutResult.success &&
          layoutResult.data &&
          Array.isArray(layoutResult.data) &&
          layoutResult.data.length > 0
        ) {
          layoutData = layoutResult.data;
          hasSavedLayout = true;
        } else {
          this.showToast(t("viz.no_room_layout_data"), "info");
        }

        if (hasSavedLayout && cabinets.length > 0) {
          await this.layoutAndRenderCabinets(cabinets, layoutData, token);
        } else {
          // 无渲染跟进（无布局/无柜）:恢复完成标志,避免 saveLayout 被永久拒绝
          this.cabinetRenderSettled = true;
        }

        return hasSavedLayout;
      }
    } catch (error) {
      console.error("加载布局失败:", error);
      this.showToast(t("viz.layout_load_failed"), "error");
      // 仅当本次加载仍是最新渲染时才清空画布：
      // 过期加载的失败处理不清画布，避免抹掉并发开启的新渲染成果
      const isLatestRender =
        token !== null &&
        (token === this.workstationRenderToken || token === this.cabinetRenderToken);
      if (!isLatestRender) {
        // 已被更新的渲染取代：放弃后续动作（null 语义同代次过期）
        return null;
      }
      this.core.elementsGroup.innerHTML = "";
      // 机柜视图异常中断后不会再有渲染跟进：恢复完成标志，避免 saveLayout 被永久拒绝
      if (this.core.type === "cabinet") {
        this.cabinetRenderSettled = true;
      }
      // 加载失败与"无布局"区分，调用方不得自动排布落库
      return "error";
    }
  }

  /**
   * 开启一次工位渲染：清空画布并递增渲染代次。
   * 返回代次 token，await 之后的 DOM 写入需校验 token，过期即中止，
   * 避免快速切换房间时两次加载交错绘制（画布混入两个房间的工位）。
   */
  beginWorkstationRender() {
    this.core.elementsGroup.innerHTML = "";
    return ++this.workstationRenderToken;
  }

  /**
   * 按已保存布局定位门元素（矩形/文字/把手同步平移），返回内容边界。
   * 无已保存位置时按门元素默认坐标计入边界。
   */
  _positionDoorElement(doorElement, doorItem) {
    let maxX = 0;
    let maxY = 0;

    // 异常坐标（NaN/负值/零宽高）按无已保存位置处理，回退门元素默认坐标
    const doorPosition = isValidLayoutPosition(doorItem?.position) ? doorItem.position : null;
    if (doorItem?.position && !doorPosition) {
      console.warn("门的已保存坐标异常，已按默认坐标渲染:", doorItem.position);
    }

    if (doorPosition) {
      const rect = doorElement.querySelector("rect");
      if (rect) {
        rect.setAttribute("x", doorPosition.x);
        rect.setAttribute("y", doorPosition.y);
        rect.setAttribute("width", doorPosition.width);
        rect.setAttribute("height", doorPosition.height);
      }

      const text = doorElement.querySelector("text");
      if (text) {
        text.setAttribute("x", doorPosition.x + doorPosition.width / 2);
        text.setAttribute("y", doorPosition.y - 10);
        text.dataset.relY = -10;
      }

      const circle = doorElement.querySelector("circle");
      if (circle) {
        circle.setAttribute("cx", doorPosition.x + doorPosition.width - 10);
        circle.setAttribute("cy", doorPosition.y + doorPosition.height / 2);
        circle.dataset.relCx = doorPosition.width - 10;
        circle.dataset.relCy = doorPosition.height / 2;
      }

      maxX = Math.max(maxX, doorPosition.x + doorPosition.width);
      maxY = Math.max(maxY, doorPosition.y + doorPosition.height);
    } else {
      const rect = doorElement.querySelector("rect");
      if (rect) {
        const x = parseFloat(rect.getAttribute("x"));
        const y = parseFloat(rect.getAttribute("y"));
        const width = parseFloat(rect.getAttribute("width"));
        const height = parseFloat(rect.getAttribute("height"));
        maxX = Math.max(maxX, x + width);
        maxY = Math.max(maxY, y + height);
      }
    }

    return { maxX, maxY };
  }

  /** 记录当前房间本次加载/绘制的合法元素 id 集合（保存布局时用于归属过滤）。 */
  setLayoutOwnerIds(ids) {
    this.layoutOwnerIds = new Set(ids);
  }

  /**
   * 开启一次机柜渲染：清空画布并递增渲染代次。
   * 返回代次 token，渲染过程中的异步间隙需校验 token 是否仍有效，
   * 避免 resize/切换房间触发的并发渲染交错绘制。
   */
  beginCabinetRender() {
    this.core.elementsGroup.innerHTML = "";
    // 渲染开始:完成标志复位,由 renderCabinetBatches 全部批次完成时置回
    this.cabinetRenderSettled = false;
    return ++this.cabinetRenderToken;
  }

  /** 等待容器获得真实高度（隐藏 tab 刚切出时可能为 0）。 */
  async waitForContainerHeight() {
    let height = this.core.container.clientHeight;
    if (!height || height < 100) {
      await new Promise((resolve) => nextFrame(resolve));
      height = this.core.container.clientHeight;
    }
    return height >= 100 ? height : 600;
  }

  /**
   * 计算全部机柜位置并立即定型画布，再分批渲染。
   * 保存的 x 坐标仅保留相对排列，整体平移使最左机柜距左缘固定边距；
   * y 按容器高度底部对齐。首帧即为最终布局（无先渲染后纠正的闪变）。
   * @returns {Promise<boolean>} 渲染是否完整完成（false 表示已被新渲染取代）
   */
  async layoutAndRenderCabinets(cabinets, layoutData, token) {
    const containerHeight = await this.waitForContainerHeight();
    if (token !== this.cabinetRenderToken) {
      return false;
    }

    const padding = CABINET_EDGE_PADDING;
    const maxCapacity = Math.max(...cabinets.map((c) => c.capacity ?? DEFAULT_CABINET_CAPACITY));
    const uHeight = Math.floor((containerHeight - padding * 2 - 40) / maxCapacity);

    let minX = Infinity;
    let maxX = 0;
    cabinets.forEach((cabinet, index) => {
      cabinet.capacity = cabinet.capacity ?? DEFAULT_CABINET_CAPACITY;
      const height = cabinet.capacity * uHeight + 40;
      const savedItem = layoutData.find(
        (item) => item.id.toLowerCase() === cabinet.id.toLowerCase()
      );
      const savedX =
        savedItem && savedItem.position && Number.isFinite(savedItem.position.x)
          ? savedItem.position.x
          : null;
      const x = savedX ?? padding + index * (CABINET_WIDTH + CABINET_GAP);
      // 已保存宽度需为正的有限值，否则回退默认宽度（负/零/NaN 会使画布尺寸错乱）
      const savedWidth = savedItem?.position?.width;
      const width = Number.isFinite(savedWidth) && savedWidth > 0 ? savedWidth : CABINET_WIDTH;

      cabinet.position = {
        x,
        y: containerHeight - padding - height,
        width,
        height
      };
      minX = Math.min(minX, x);
      maxX = Math.max(maxX, x + width);
    });

    // 整体平移：最左机柜距左缘固定 CABINET_EDGE_PADDING
    const offsetX = (minX === Infinity ? 0 : minX) - padding;
    cabinets.forEach((cabinet) => {
      cabinet.position.x -= offsetX;
    });
    const totalWidth = maxX - offsetX + padding;

    // 画布定型：宽度按内容像素设置（超出容器即出现横向滚动条）
    this.core.setCabinetCanvasSize(
      Math.max(totalWidth, this.core.container.clientWidth || 800),
      containerHeight
    );

    return await this.renderCabinetBatches(cabinets, token);
  }

  /**
   * 分批渲染机柜：每渲染完一批（16 个）统一拉取该批机位的 IP 信息再绘制机位，
   * 避免逐柜请求；机柜多时用户可先看到并滚动前几批。
   * @returns {Promise<boolean>} 是否完整完成（false 表示已被新渲染取代）
   */
  async renderCabinetBatches(cabinets, token) {
    for (let i = 0; i < cabinets.length; i += CABINET_BATCH_SIZE) {
      if (token !== this.cabinetRenderToken) {
        return false;
      }
      const batch = cabinets.slice(i, i + CABINET_BATCH_SIZE);
      batch.forEach((cabinet) => this.renderer.drawCabinet(cabinet));

      const positionIds = batch.flatMap((cabinet) =>
        (cabinet.positions || []).map((position) => position.id)
      );
      const ipMap = positionIds.length ? await this.fetchIpMapByPositions(positionIds) : new Map();
      if (token !== this.cabinetRenderToken) {
        return false;
      }

      batch.forEach((cabinet) => this.drawCabinetPositions(cabinet, ipMap));
    }
    // 全部批次完成:画布为完整成品,恢复可保存
    this.cabinetRenderSettled = true;
    return true;
  }

  /** 按机位 ID 批量拉取 IP 信息，返回 position_id → ipDetail 映射。 */
  async fetchIpMapByPositions(positionIds) {
    try {
      const ids = [...new Set(positionIds)].join(",");
      // 单批机位上限 16 柜 × 48U = 768，page_size=1000 足够覆盖
      const result = await this.apiGet(
        `/api/resources/ip?page_size=1000&position_ids=${encodeURIComponent(ids)}`
      );
      const map = new Map();
      if (result.success && Array.isArray(result.data?.items)) {
        result.data.items.forEach((ipDetail) => {
          if (ipDetail.position_id) {
            map.set(ipDetail.position_id, ipDetail);
          }
        });
      } else if (!result.success) {
        this._notifyLoadFailure(result.message, "批量获取机位IP");
      }
      return map;
    } catch (error) {
      this._notifyLoadFailure(error, "批量获取机位IP");
      return new Map();
    }
  }

  /** 绘制单个机柜下的全部机位（IP 信息已由批量拉取结果提供）。 */
  drawCabinetPositions(cabinet, ipMap) {
    (cabinet.positions || []).forEach((position) => {
      position.ipDetail = ipMap.get(position.id) || null;
      this.renderer.drawCabinetPosition(position, cabinet);
    });
  }

  /**
   * 保存当前画布布局（整表 upsert）。
   * @param {Object} [options]
   * @param {boolean} [options.silent] 静默模式：成功不弹提示（拖拽自动保存使用），失败仍提示
   */
  async saveLayout({ silent = false } = {}) {
    // 机柜分批渲染期间画布是半成品:此时保存会把尚未渲染的机柜从布局中抹掉
    //（整表 upsert 语义），提示稍后再试（工位视图一次性绘制完成,无此问题）
    if (this.core.type === "cabinet" && !this.cabinetRenderSettled) {
      this.showToast(t("viz.rendering_in_progress"), "warning");
      return;
    }

    const elements = this.core.elementsGroup.querySelectorAll("[data-id]");
    const layoutData = [];
    // 归属过滤基准（仅工位视图）：本次 loadSavedLayout/autoDraw 加载的工位集合。
    // 画布上不属于当前房间的元素（并发切换残留）直接忽略，与渲染代次一起
    // 构成双保险，防止把其他房间的工位布局行改挂到当前房间
    const ownerIds = this.core.type === "workstation" ? this.layoutOwnerIds : null;

    elements.forEach((el) => {
      if (this.core.type === "cabinet" && el.classList.contains("cabinet-position-element")) {
        return;
      }

      const id = el.dataset.id;
      if (ownerIds && !ownerIds.has(id)) {
        return;
      }

      const rect = el.querySelector("rect");
      if (!rect) {
        return;
      }

      const x = parseFloat(rect.getAttribute("x"));
      const y = parseFloat(rect.getAttribute("y"));
      const width = parseFloat(rect.getAttribute("width"));
      const height = parseFloat(rect.getAttribute("height"));

      // 坐标校验与手工录入一致（min=0）：非有限值不入库，负坐标钳制为 0；
      // 宽高必须为正数（0/负值元素无法点击与拖拽，入库即坏数据）
      if (![x, y, width, height].every(Number.isFinite)) {
        console.warn("布局元素坐标非有限值，跳过保存:", id);
        return;
      }
      if (width <= 0 || height <= 0) {
        console.warn("布局元素宽高非正，跳过保存:", id);
        return;
      }
      if (x < 0 || y < 0) {
        console.warn("布局元素坐标为负，已钳制为 0:", id);
      }

      const position = {
        x: Math.max(0, x),
        y: Math.max(0, y),
        width,
        height,
        rotation: 0
      };

      let element_type;
      if (this.core.type === "workstation") {
        element_type = el.classList.contains("door-element") ? "door" : "workstation";
      } else if (this.core.type === "cabinet") {
        element_type = "network_device";
      } else {
        element_type = "workstation";
      }

      layoutData.push({ id, position, element_type });
    });

    // 空画布（未选房间/无元素）不提交，防止把空布局写到无效 room
    if (layoutData.length === 0 || !this.core.currentRoomId) {
      if (!silent) {
        this.showToast(t("viz.no_workstation_data"), "info");
      }
      return;
    }

    try {
      const result = await this.apiPost("/api/resources/layouts", {
        type: this.core.type === "cabinet" ? "cabinet" : this.core.type,
        room_id: this.core.currentRoomId,
        layout: layoutData
      });

      if (result.success) {
        if (!silent) {
          this.showToast(t("viz.layout_save_success"), "success");
        }
      } else {
        this.showToast(`${t("viz.layout_save_failed")}: ${result.message}`, "error");
      }
    } catch (error) {
      console.error("保存布局失败:", error);
      this.showToast(t("viz.layout_save_failed"), "error");
    }
  }

  async deleteLayout() {
    if (this.core.type === "workstation") {
      if (!this.core.currentRoomId) {
        this.showToast(t("viz.select_room_first"), "warning");
        return;
      }

      const confirmed = await this.core.showConfirm(t("viz.confirm_delete_workstation_layout"));
      if (!confirmed) {
        return;
      }

      try {
        const result = await this.apiDelete(
          `/api/resources/layouts/workstation/${this.core.currentRoomId}`
        );
        if (result.success) {
          this.core.elementsGroup.innerHTML = "";
          this.showToast(t("viz.layout_delete_success"), "success");
        } else {
          this.showToast(`${t("viz.layout_delete_failed")}: ${result.message}`, "error");
        }
      } catch (error) {
        // 请求失败视为删除未发生：保留画布现场，与 success=false 分支行为一致
        console.error("删除布局失败:", error);
        this.showToast(t("viz.layout_delete_failed"), "error");
      }
    } else if (this.core.type === "cabinet") {
      if (!this.core.currentRoomId) {
        this.showToast(t("viz.select_room_first"), "warning");
        return;
      }

      const confirmed = await this.core.showConfirm(t("viz.confirm_delete_cabinet_layout"));
      if (!confirmed) {
        return;
      }

      try {
        const result = await this.apiDelete(
          `/api/resources/layouts/positions/${this.core.currentRoomId}`
        );
        if (result.success) {
          this.core.elementsGroup.innerHTML = "";
          this.showToast(t("viz.layout_delete_success"), "success");
        } else {
          this.showToast(`${t("viz.layout_delete_failed")}: ${result.message}`, "error");
        }
      } catch (error) {
        // 请求失败视为删除未发生：保留画布现场，与 success=false 分支行为一致
        console.error("删除布局失败:", error);
        this.showToast(t("viz.layout_delete_failed"), "error");
      }
    }
  }
}
