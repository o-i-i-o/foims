import { t } from "../../utils/i18n.js";

// 机柜可视化布局常量
/** 画布四周预留边距：首柜距左缘 / 柜底距底缘（像素，viewBox 1:1） */
const CABINET_EDGE_PADDING = 16;
/** 相邻机柜水平间距 */
const CABINET_GAP = 50;
/** 机柜宽度 */
const CABINET_WIDTH = 150;
/** 每渲染完一批机柜统一拉取该批机位 IP 信息 */
const CABINET_BATCH_SIZE = 16;

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

  async fetchWorkstationsByRoom(roomId) {
    try {
      const result = await this.apiGet(`/api/resources/workstations?room_id=${roomId}`);
      if (!result.success || !result.data) return [];
      if (Array.isArray(result.data)) return result.data;
      if (result.data.items && Array.isArray(result.data.items)) return result.data.items;
      return [];
    } catch (error) {
      console.error("获取工位数据失败:", error);
      return [];
    }
  }

  async fetchIps() {
    try {
      const result = await this.apiGet("/api/resources/ip");
      if (result.success && result.data) {
        if (Array.isArray(result.data)) return result.data;
        if (result.data.items && Array.isArray(result.data.items)) return result.data.items;
        if (result.data.data && Array.isArray(result.data.data)) return result.data.data;
        if (result.data.ip_managers && Array.isArray(result.data.ip_managers))
          return result.data.ip_managers;
        if (result.data.ips && Array.isArray(result.data.ips)) return result.data.ips;
      }
      return [];
    } catch (error) {
      console.error("获取IP失败:", error);
      return [];
    }
  }

  async fetchCabinetsByRoom(roomId) {
    try {
      const result = await this.apiGet(`/api/resources/layouts/room-cabinets/${roomId}`);
      if (result.success && result.data) {
        return result.data;
      }
      return [];
    } catch (error) {
      console.error("获取房间机柜数据失败:", error);
      return [];
    }
  }

  async fetchCabinetPositions(cabinetId) {
    try {
      const positionsResult = await this.apiGet(`/api/resources/positions?cabinet_id=${cabinetId}`);
      if (!positionsResult.success || !positionsResult.data) {
        return [];
      }

      if (Array.isArray(positionsResult.data)) {
        return positionsResult.data;
      } else if (positionsResult.data.items && Array.isArray(positionsResult.data.items)) {
        return positionsResult.data.items;
      }
      return [];
    } catch (error) {
      console.error("获取机位数据失败:", error);
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
            if (!ipManager.workstation_id) return;
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
          workstations.forEach((workstation, index) => {
            const savedItem = layoutData.find(
              (item) => item.id.toLowerCase() === workstation.id.toLowerCase()
            );
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
                width: width,
                height: height
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
          this.core.svg.setAttribute(
            "viewBox",
            `0 0 ${Math.max(1000, maxX + padding)} ${Math.max(800, maxY + padding)}`
          );
        }

        return hasSavedLayout;
      } else if (this.core.type === "cabinet") {
        const token = this.beginCabinetRender();

        const [layoutResult, cabinets] = await Promise.all([
          this.apiGet(`/api/resources/layouts/positions/${id}`),
          this.fetchCabinetsByRoom(id)
        ]);
        if (token !== this.cabinetRenderToken) return false;

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
      await new Promise((resolve) => requestAnimationFrame(resolve));
      await new Promise((resolve) => requestAnimationFrame(resolve));
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
    if (token !== this.cabinetRenderToken) return false;

    const padding = CABINET_EDGE_PADDING;
    const maxCapacity = Math.max(...cabinets.map((c) => c.capacity || 45));
    const uHeight = Math.floor((containerHeight - padding * 2 - 40) / maxCapacity);

    let minX = Infinity;
    let maxX = 0;
    cabinets.forEach((cabinet, index) => {
      cabinet.capacity = cabinet.capacity || 45;
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
      if (token !== this.cabinetRenderToken) return false;
      const batch = cabinets.slice(i, i + CABINET_BATCH_SIZE);
      batch.forEach((cabinet) => this.renderer.drawCabinet(cabinet));

      const positionIds = batch.flatMap((cabinet) =>
        (cabinet.positions || []).map((position) => position.id)
      );
      const ipMap = positionIds.length
        ? await this.fetchIpMapByPositions(positionIds)
        : new Map();
      if (token !== this.cabinetRenderToken) return false;

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
      if (result.success && result.data) {
        const list = Array.isArray(result.data)
          ? result.data
          : Array.isArray(result.data.items)
            ? result.data.items
            : [];
        list.forEach((ipManager) => {
          if (ipManager.position_id) {
            map.set(ipManager.position_id, ipManager);
          }
        });
      }
      return map;
    } catch (error) {
      console.error("批量获取机位 IP 失败:", error);
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

  async saveLayout() {
    const elements = this.core.elementsGroup.querySelectorAll("[data-id]");
    const layoutData = [];

    elements.forEach((el) => {
      if (this.core.type === "cabinet" && el.classList.contains("cabinet-position-element")) {
        return;
      }

      const id = el.dataset.id;
      const rect = el.querySelector("rect");
      if (!rect) return;

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

    try {
      const result = await this.apiPost("/api/resources/layouts", {
        type: this.core.type === "cabinet" ? "cabinet" : this.core.type,
        room_id: this.core.currentRoomId || null,
        network_region_id: null,
        cabinet_id: null,
        layout: layoutData
      });

      if (result.success) {
        this.showToast(t("viz.layout_save_success"), "success");
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
