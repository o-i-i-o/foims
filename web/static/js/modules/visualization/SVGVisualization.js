import { SVGCore } from "./SVGCore.js";
import { SVGRenderer } from "./SVGRenderer.js";
import { SVGDataManager } from "./SVGDataManager.js";
import { t } from "../../utils/i18n.js";

export class SVGVisualization {
  constructor(containerId, type, callbacks = {}) {
    this.core = new SVGCore(containerId, type, callbacks);
    this.renderer = new SVGRenderer(this.core);
    this.dataManager = new SVGDataManager(this.core, this.renderer);

    this.elementsGroup = this.core.elementsGroup;
    this.svg = this.core.svg;
    this.container = this.core.container;
    this.gridSize = this.core.gridSize;
    this.snapToGrid = this.core.snapToGrid;
    this.showAlignmentLines = this.core.showAlignmentLines;
    this.currentRoomId = null;
  }

  async autoDrawWorkstations(roomId) {
    try {
      const hasSavedLayout = await this.dataManager.loadSavedLayout(roomId);

      if (hasSavedLayout) {
        this.currentRoomId = roomId;
        this.core.currentRoomId = roomId;
        return;
      }

      this.currentRoomId = roomId;
      this.core.currentRoomId = roomId;
      this.core.elementsGroup.innerHTML = "";

      const workstations = await this.dataManager.fetchWorkstationsByRoom(roomId);
      const ipManagers = await this.dataManager.fetchIps();

      const ipMap = new Map();
      if (Array.isArray(ipManagers)) {
        ipManagers.forEach((ipManager) => {
          if (!ipManager.workstation_id) return;
          const existing = ipMap.get(ipManager.workstation_id);
          if (!existing || (existing.status !== "active" && ipManager.status === "active")) {
            ipMap.set(ipManager.workstation_id, ipManager);
          }
        });
      }

      if (workstations.length === 0) {
        this.core.showToast(t("viz.no_workstation_data"), "info");
        return;
      }

      this.renderer.drawDoor();

      const gap = 20;
      const width = 160;
      const height = 160;
      const startX = 150;
      const startY = 100;

      let cols = Math.min(8, Math.max(3, Math.ceil(Math.sqrt(workstations.length))));
      if (workstations.length <= 5) {
        cols = 3;
      } else if (workstations.length <= 12) {
        cols = 4;
      } else if (workstations.length <= 24) {
        cols = 6;
      }

      const rows = Math.ceil(workstations.length / cols);

      workstations.forEach((workstation, index) => {
        const col = index % cols;
        const row = Math.floor(index / cols);

        workstation.ipManager = ipMap.get(workstation.id) || null;
        workstation.position = {
          x: startX + col * (width + gap),
          y: startY + row * (height + gap),
          width: width,
          height: height
        };

        this.renderer.drawWorkstation(workstation);
      });

      const totalWidth = startX + cols * (width + gap) + 50;
      const totalHeight = startY + rows * (height + gap) + 50;
      // viewBox 同时覆盖容器尺寸，网格背景铺满画布（与 loadSavedLayout 保持一致）
      const cw = this.container.clientWidth || 0;
      const ch = this.container.clientHeight || 0;
      this.core.setViewBox(
        0,
        0,
        Math.max(1000, totalWidth, cw),
        Math.max(800, totalHeight, ch)
      );

      setTimeout(() => {
        this.saveLayout();
      }, 500);
    } catch (error) {
      console.error("自动绘制工位失败:", error);
      this.core.showToast(t("viz.auto_draw_workstation_failed"), "error");
    }
  }

  async autoDrawCabinetPositions(roomId) {
    if (!roomId) {
      return;
    }

    this.currentRoomId = roomId;
    this.core.currentRoomId = roomId;

    try {
      const cabinets = await this.dataManager.fetchCabinetsByRoom(roomId);

      if (cabinets.length === 0) {
        this.core.showToast(t("viz.no_cabinet_data"), "info");
        return;
      }

      const token = this.dataManager.beginCabinetRender();
      if (token !== this.dataManager.cabinetRenderToken) return;

      // 复用与 loadSavedLayout 相同的布局与分批渲染（无保存布局，纯自动排列）
      const finished = await this.dataManager.layoutAndRenderCabinets(cabinets, [], token);
      if (!finished) return;
    } catch (error) {
      console.error("自动绘制机位图失败:", error);
      this.core.showToast(`${t("viz.draw_positions_failed")}: ${error.message}`, "error");
    }
  }

  async loadSavedLayout(id) {
    return await this.dataManager.loadSavedLayout(id);
  }

  /** @param {{silent?: boolean}} [options] 静默保存：拖拽自动保存时不弹成功提示 */
  async saveLayout(options) {
    return await this.dataManager.saveLayout(options);
  }

  async deleteLayout() {
    return await this.dataManager.deleteLayout();
  }
}
