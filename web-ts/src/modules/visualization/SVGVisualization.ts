// SVG Visualization - Main visualization class
import { SVGCore, type Position } from "./SVGCore.js";
import { SVGRenderer } from "./SVGRenderer.js";
import { SVGDataManager } from "./SVGDataManager.js";

interface Workstation {
  id: string;
  name: string;
  manager?: string;
  position?: Position;
  ipManager?: {
    ip_address?: string;
    status?: string;
    switch_name?: string;
    switch_port_number?: string;
    network_name?: string;
  } | null;
}

interface Cabinet {
  id: string;
  name: string;
  capacity?: number;
  position?: Position;
  networks?: { network_region_id: string }[];
}

export class SVGVisualization {
  core: SVGCore;
  renderer: SVGRenderer;
  dataManager: SVGDataManager;

  elementsGroup: SVGGElement;
  svg: SVGSVGElement;
  container: HTMLElement;
  gridSize: number;
  snapToGrid: boolean;
  showAlignmentLines: boolean;
  currentRoomId: string | null = null;
  currentNetworkRegionId: string | null = null;

  constructor(containerId: string, type: "workstation" | "cabinet", callbacks = {}) {
    this.core = new SVGCore(containerId, type, callbacks);
    this.renderer = new SVGRenderer(this.core);
    this.dataManager = new SVGDataManager(this.core, this.renderer);

    this.elementsGroup = this.core.elementsGroup;
    this.svg = this.core.svg;
    this.container = this.core.container;
    this.gridSize = this.core.gridSize;
    this.snapToGrid = this.core.snapToGrid;
    this.showAlignmentLines = this.core.showAlignmentLines;
  }

  async autoDrawWorkstations(roomId: string): Promise<void> {
    try {
      const hasSavedLayout = await this.dataManager.loadSavedLayout(roomId);

      if (hasSavedLayout) {
        this.currentRoomId = roomId;
        return;
      }

      this.currentRoomId = roomId;
      this.core.elementsGroup.innerHTML = "";

      const workstations = await this.dataManager.fetchWorkstationsByRoom(roomId);
      const ipManagers = await this.dataManager.fetchIpManager();

      const ipMap = new Map<string, { ip_address?: string; status?: string; switch_name?: string; switch_port_number?: string; network_name?: string }>();
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

        (workstation as Workstation).ipManager = ipMap.get(workstation.id) || null;
        (workstation as Workstation).position = {
          x: startX + col * (width + gap),
          y: startY + row * (height + gap),
          width: width,
          height: height,
        };

        this.renderer.drawWorkstation(workstation as Workstation);
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

  async autoDrawCabinetPositions(networkRegionId: string): Promise<void> {
    if (!networkRegionId) {
      return;
    }

    try {
      const cabinets = await this.dataManager.fetchCabinetsByNetworkRegion(networkRegionId);

      if (cabinets.length === 0) {
        this.core.showToast("该网络区域下暂无机柜数据", "info");
        return;
      }

      this.core.elementsGroup.innerHTML = "";

      const cabinetWidth = 150;
      const gap = 50;
      const startX = 50;

      const containerHeight = this.core.container.clientHeight || 800;
      const topPadding = 20;
      const bottomPadding = 20;
      const availableHeight = containerHeight - topPadding - bottomPadding;

      const svgHeight = Math.max(availableHeight + topPadding + bottomPadding, 1000);
      const bottomY = svgHeight - bottomPadding;

      cabinets.forEach((cabinet, index) => {
        const x = startX + index * (cabinetWidth + gap);

        (cabinet as Cabinet).capacity = cabinet.capacity || 45;
        const uHeight = 20;
        const cabinetHeight = (cabinet as Cabinet).capacity! * uHeight + 40;

        const y = bottomY - cabinetHeight;

        (cabinet as Cabinet).position = {
          x: x,
          y: y,
          width: cabinetWidth,
          height: cabinetHeight,
        };

        this.renderer.drawCabinet(cabinet as Cabinet);
        this.dataManager.drawCabinetPositionsWithIp(cabinet as Cabinet);
      });

      const totalWidth = startX + cabinets.length * (cabinetWidth + gap) + 50;
      this.core.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, totalWidth)} ${svgHeight}`);
      this.core.svg.setAttribute("height", String(svgHeight));

      this.currentNetworkRegionId = networkRegionId;
    } catch (error) {
      console.error("自动绘制机位图失败:", error);
      this.core.showToast("绘制机位图失败: " + (error as Error).message, "error");
    }
  }

  async loadSavedLayout(id: string): Promise<boolean> {
    return await this.dataManager.loadSavedLayout(id);
  }

  async saveLayout(): Promise<void> {
    return await this.dataManager.saveLayout();
  }

  async deleteLayout(): Promise<void> {
    return await this.dataManager.deleteLayout();
  }

  deleteWorkstation(id: string): void {
    const workstation = this.core.elementsGroup.querySelector(`[data-id="${id}"]`);
    if (workstation) {
      workstation.remove();
      this.core.showToast("工位删除成功", "success");
      this.saveLayout();
    }
  }

  deleteCabinetPosition(id: string): void {
    const cabinetPosition = this.core.elementsGroup.querySelector(`[data-id="${id}"]`);
    if (cabinetPosition) {
      cabinetPosition.remove();
      this.core.showToast("机位删除成功", "success");
      this.saveLayout();
    }
  }

  setGridSize(size: number): void {
    this.core.setGridSize(size);
    this.gridSize = size;
  }

  toggleSnapToGrid(enabled: boolean): void {
    this.core.toggleSnapToGrid(enabled);
    this.snapToGrid = enabled;
  }

  toggleAlignmentLines(enabled: boolean): void {
    this.core.toggleAlignmentLines(enabled);
    this.showAlignmentLines = enabled;
  }

  destroy(): void {
    this.core.destroy();
  }
}
