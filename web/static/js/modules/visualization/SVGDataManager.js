import { t } from "../../utils/i18n.js";
import { nextFrame } from "../../utils/helpers.js";

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
 * 机柜容量缺省值：与后端建表默认（ipma-init cabinets DDL `capacity DEFAULT 42`）一致。
 * 列表接口可能缺失该字段，渲染时统一回退到此常量。
 */
export const DEFAULT_CABINET_CAPACITY = 42;

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
  }

  /** 数据获取失败时的统一提示（请求异常或响应形状异常） */
  _notifyLoadFailure(error, what) {
    console.error(`${what}失败:`, error);
    this.showToast(t("viz.data_load_failed"), "error");
  }

  async fetchWorkstationsByRoom(roomId) {
    try {
      const result = await this.apiGet(
        `/api/resources/workstations?room_id=${roomId}&page_size=1000`
      );
      if (result.success && Array.isArray(result.data?.items)) {
        return result.data.items;
      }
      if (result.success) {
        return [];
      }
      this._notifyLoadFailure(result.message, "获取工位数据");
      return [];
    } catch (error) {
      this._notifyLoadFailure(error, "获取工位数据");
      return [];
    }
  }

  async fetchIps() {
    try {
      const result = await this.apiGet("/api/resources/ip?page_size=1000");
      // 后端分页响应形状固定为 items（paged_response），不再做多分支兜底
      if (result.success && Array.isArray(result.data?.items)) {
        return result.data.items;
      }
      if (result.success) {
        return [];
      }
      this._notifyLoadFailure(result.message, "获取IP");
      return [];
    } catch (error) {
      this._notifyLoadFailure(error, "获取IP");
      return [];
    }
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

    try {
      if (this.core.type === "workstation") {
        this.core.elementsGroup.innerHTML = "";

        const [layoutResult, workstations, ipManagers] = await Promise.all([
          this.apiGet(`/api/resources/layouts/workstation/${id}`),
          this.fetchWorkstationsByRoom(id),
          this.fetchIps()
        ]);

        const ipMap = new Map();
        if (Array.isArray(ipManagers)) {
          ipManagers.forEach((ipManager) => {
            if (!ipManager.workstation_id) {
              return;
            }
            ipMap.set(ipManager.workstation_id, ipManager);
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

        let maxX = 0;
        let maxY = 0;

        if (doorItem && doorItem.position) {
          const rect = doorElement.querySelector("rect");
          if (rect) {
            rect.setAttribute("x", doorItem.position.x);
            rect.setAttribute("y", doorItem.position.y);
            rect.setAttribute("width", doorItem.position.width);
            rect.setAttribute("height", doorItem.position.height);
          }

          const text = doorElement.querySelector("text");
          if (text) {
            text.setAttribute("x", doorItem.position.x + doorItem.position.width / 2);
            text.setAttribute("y", doorItem.position.y - 10);
            text.dataset.relY = -10;
          }

          const circle = doorElement.querySelector("circle");
          if (circle) {
            circle.setAttribute("cx", doorItem.position.x + doorItem.position.width - 10);
            circle.setAttribute("cy", doorItem.position.y + doorItem.position.height / 2);
            circle.dataset.relCx = doorItem.position.width - 10;
            circle.dataset.relCy = doorItem.position.height / 2;
          }

          maxX = Math.max(maxX, doorItem.position.x + doorItem.position.width);
          maxY = Math.max(maxY, doorItem.position.y + doorItem.position.height);
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
            if (savedItem && savedItem.position) {
              workstation.position = savedItem.position;
            } else {
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
            workstation.ipManager = ipMap.get(workstation.id);
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

        return hasSavedLayout;
      }
      if (this.core.type === "cabinet") {
        const token = this.beginCabinetRender();

        const [layoutResult, cabinets] = await Promise.all([
          this.apiGet(`/api/resources/layouts/positions/${id}`),
          this.fetchCabinetsByRoom(id)
        ]);
        if (token !== this.cabinetRenderToken) {
          return false;
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
        }

        return hasSavedLayout;
      }
    } catch (error) {
      console.error("加载布局失败:", error);
      this.showToast(t("viz.layout_load_failed"), "error");
      this.core.elementsGroup.innerHTML = "";
      return false;
    }
  }

  /**
   * 开启一次机柜渲染：清空画布并递增渲染代次。
   * 返回代次 token，渲染过程中的异步间隙需校验 token 是否仍有效，
   * 避免 resize/切换房间触发的并发渲染交错绘制。
   */
  beginCabinetRender() {
    this.core.elementsGroup.innerHTML = "";
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
      const width = (savedItem && savedItem.position?.width) || CABINET_WIDTH;

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
    return true;
  }

  /** 按机位 ID 批量拉取 IP 信息，返回 position_id → ipManager 映射。 */
  async fetchIpMapByPositions(positionIds) {
    try {
      const ids = [...new Set(positionIds)].join(",");
      // 单批机位上限 16 柜 × 48U = 768，page_size=1000 足够覆盖
      const result = await this.apiGet(
        `/api/resources/ip?page_size=1000&position_ids=${encodeURIComponent(ids)}`
      );
      const map = new Map();
      if (result.success && Array.isArray(result.data?.items)) {
        result.data.items.forEach((ipManager) => {
          if (ipManager.position_id) {
            map.set(ipManager.position_id, ipManager);
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
      position.ipManager = ipMap.get(position.id) || null;
      this.renderer.drawCabinetPosition(position, cabinet);
    });
  }

  /**
   * 保存当前画布布局（整表 upsert）。
   * @param {Object} [options]
   * @param {boolean} [options.silent] 静默模式：成功不弹提示（拖拽自动保存使用），失败仍提示
   */
  async saveLayout({ silent = false } = {}) {
    const elements = this.core.elementsGroup.querySelectorAll("[data-id]");
    const layoutData = [];

    elements.forEach((el) => {
      if (this.core.type === "cabinet" && el.classList.contains("cabinet-position-element")) {
        return;
      }

      const id = el.dataset.id;
      const rect = el.querySelector("rect");
      if (!rect) {
        return;
      }

      const position = {
        x: parseFloat(rect.getAttribute("x")),
        y: parseFloat(rect.getAttribute("y")),
        width: parseFloat(rect.getAttribute("width")),
        height: parseFloat(rect.getAttribute("height")),
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
        console.error("删除布局失败:", error);
        this.core.elementsGroup.innerHTML = "";
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
        console.error("删除布局失败:", error);
        this.core.elementsGroup.innerHTML = "";
        this.showToast(t("viz.layout_delete_failed"), "error");
      }
    }
  }
}
