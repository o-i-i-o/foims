import { SVGCore } from "./SVGCore.js";
import { SVGRenderer } from "./SVGRenderer.js";
import { SVGDataManager } from "./SVGDataManager.js";

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
          if (!existing ||
              (existing.status !== "active" && ipManager.status === "active")) {
            ipMap.set(ipManager.workstation_id, ipManager);
          }
        });
      }

      if (workstations.length === 0) {
        this.core.showToast("该房间下暂无工位数据", "info");
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
          height: height,
        };

        this.renderer.drawWorkstation(workstation);
      });

      const totalWidth = startX + cols * (width + gap) + 50;
      const totalHeight = startY + rows * (height + gap) + 50;
      this.core.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, totalWidth)} ${Math.max(800, totalHeight)}`);
      
      setTimeout(() => {
        this.saveLayout();
      }, 500);
    } catch (error) {
      console.error("自动绘制工位失败:", error);
      this.core.showToast("自动绘制工位失败，请重试", "error");
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
        this.core.showToast("该房间下暂无机柜数据", "info");
        return;
      }

      this.core.elementsGroup.innerHTML = "";

      const cabinetWidth = 150;
      const gap = 50;
      const startX = 50;
      const padding = 5;
      
      const containerHeight = this.core.container.clientHeight || 600;
      const availableHeight = containerHeight - padding * 2;
      
      const maxCapacity = Math.max(...cabinets.map(c => c.capacity || 45));
      const uHeight = Math.floor((availableHeight - 40) / maxCapacity);
      const svgHeight = containerHeight;
      const bottomY = svgHeight - padding;

      cabinets.forEach((cabinet, index) => {
        const x = startX + index * (cabinetWidth + gap);
        
        cabinet.capacity = cabinet.capacity || 45;
        const cabinetHeight = cabinet.capacity * uHeight + 40;

        const y = bottomY - cabinetHeight;

        cabinet.position = {
          x: x,
          y: y,
          width: cabinetWidth,
          height: cabinetHeight,
        };

        this.renderer.drawCabinet(cabinet);
        this.dataManager.drawCabinetPositionsWithIp(cabinet);
      });

      const totalWidth = startX + cabinets.length * (cabinetWidth + gap) + 50;
      this.core.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, totalWidth)} ${svgHeight}`);
      this.core.svg.setAttribute("height", "100%");
    } catch (error) {
      console.error("自动绘制机位图失败:", error);
      this.core.showToast("绘制机位图失败: " + error.message, "error");
    }
  }

  async loadSavedLayout(id) {
    return await this.dataManager.loadSavedLayout(id);
  }

  async saveLayout() {
    return await this.dataManager.saveLayout();
  }

  async deleteLayout() {
    return await this.dataManager.deleteLayout();
  }

  deleteWorkstation(id) {
    const workstation = this.core.elementsGroup.querySelector(`[data-id="${id}"]`);
    if (workstation) {
      workstation.remove();
      this.core.showToast("工位删除成功", "success");
      this.saveLayout();
    }
  }

  deleteCabinetPosition(id) {
    const cabinetPosition = this.core.elementsGroup.querySelector(`[data-id="${id}"]`);
    if (cabinetPosition) {
      cabinetPosition.remove();
      this.core.showToast("机位删除成功", "success");
      this.saveLayout();
    }
  }

  setGridSize(size) {
    this.core.setGridSize(size);
    this.gridSize = size;
  }

  toggleSnapToGrid(enabled) {
    this.core.toggleSnapToGrid(enabled);
    this.snapToGrid = enabled;
  }

  toggleAlignmentLines(enabled) {
    this.core.toggleAlignmentLines(enabled);
    this.showAlignmentLines = enabled;
  }
}
