// SVG Data Manager - Handles data fetching and layout management
import type { SVGCore, Position } from "./SVGCore.js";
import type { SVGRenderer } from "./SVGRenderer.js";

interface Workstation {
  id: string;
  name: string;
  manager?: string;
  position?: Position;
  ipManager?: IpManager | null;
}

interface IpManager {
  id: string;
  ip_address: string;
  status?: string;
  switch_name?: string;
  switch_port_number?: string;
  network_name?: string;
  workstation_id?: string;
  position_id?: string;
}

interface Cabinet {
  id: string;
  name: string;
  capacity?: number;
  position?: Position;
  networks?: { network_region_id: string }[];
}

interface CabinetPosition {
  id: string;
  name: string;
  start_u: number;
  end_u: number;
  ipManager?: IpManager | null;
}

interface LayoutItem {
  id: string;
  element_type: string;
  position: Position;
}

export class SVGDataManager {
  private core: SVGCore;
  private renderer: SVGRenderer;

  constructor(core: SVGCore, renderer: SVGRenderer) {
    this.core = core;
    this.renderer = renderer;
  }

  private async apiGet<T>(url: string): Promise<{ success: boolean; data?: T; message?: string }> {
    return this.core.apiGet(url);
  }

  private async apiPost<T>(url: string, data: unknown): Promise<{ success: boolean; data?: T; message?: string }> {
    return this.core.apiPost(url, data);
  }

  private async apiDelete<T>(url: string): Promise<{ success: boolean; data?: T; message?: string }> {
    return this.core.apiDelete(url);
  }

  private showToast(message: string, type: "success" | "error" | "warning" | "info" = "info"): void {
    this.core.showToast(message, type);
  }

  async fetchWorkstationsByRoom(roomId: string): Promise<Workstation[]> {
    try {
      const result = await this.apiGet<Workstation[] | { items: Workstation[] }>(`/api/resources/workstations?room_id=${roomId}`);
      if (!result.success || !result.data) return [];
      if (Array.isArray(result.data)) return result.data;
      if ("items" in result.data && Array.isArray(result.data.items)) return result.data.items;
      return [];
    } catch (error) {
      console.error("获取工位数据失败:", error);
      return [];
    }
  }

  async fetchIpManager(): Promise<IpManager[]> {
    try {
      const result = await this.apiGet<IpManager[] | { items: IpManager[]; data: IpManager[]; ip_managers: IpManager[]; ips: IpManager[] }>("/api/resources/ip");
      if (result.success && result.data) {
        if (Array.isArray(result.data)) return result.data;
        if ("items" in result.data && Array.isArray(result.data.items)) return result.data.items;
        if ("data" in result.data && Array.isArray(result.data.data)) return result.data.data;
        if ("ip_managers" in result.data && Array.isArray(result.data.ip_managers)) return result.data.ip_managers;
        if ("ips" in result.data && Array.isArray(result.data.ips)) return result.data.ips;
      }
      return [];
    } catch (error) {
      console.error("获取IP失败:", error);
      return [];
    }
  }

  async fetchCabinetsByNetworkRegion(networkRegionId: string): Promise<Cabinet[]> {
    try {
      const cabinetsResult = await this.apiGet<Cabinet[] | { items: Cabinet[] }>("/api/resources/cabinets?page_size=1000");

      let cabinets: Cabinet[] = [];
      if (cabinetsResult.success && cabinetsResult.data) {
        if (Array.isArray(cabinetsResult.data)) {
          cabinets = cabinetsResult.data;
        } else if ("items" in cabinetsResult.data && Array.isArray(cabinetsResult.data.items)) {
          cabinets = cabinetsResult.data.items;
        }
      }

      return cabinets.filter((cabinet) => {
        return cabinet.networks && cabinet.networks.some((network) => network.network_region_id === networkRegionId);
      });
    } catch (error) {
      console.error("获取机柜数据失败:", error);
      return [];
    }
  }

  async fetchCabinetPositions(cabinetId: string): Promise<CabinetPosition[]> {
    try {
      const positionsResult = await this.apiGet<CabinetPosition[] | { items: CabinetPosition[] }>(`/api/resources/positions?cabinet_id=${cabinetId}`);
      if (!positionsResult.success || !positionsResult.data) {
        return [];
      }

      if (Array.isArray(positionsResult.data)) {
        return positionsResult.data;
      } else if ("items" in positionsResult.data && Array.isArray(positionsResult.data.items)) {
        return positionsResult.data.items;
      }
      return [];
    } catch (error) {
      console.error("获取机位数据失败:", error);
      return [];
    }
  }

  async loadSavedLayout(id: string): Promise<boolean> {
    try {
      if (this.core.type === "workstation") {
        this.core.currentRoomId = id;
        this.core.elementsGroup.innerHTML = "";

        const [layoutResult, workstations, ipManagers] = await Promise.all([
          this.apiGet<LayoutItem[]>(`/api/resources/layouts/workstation/${id}`),
          this.fetchWorkstationsByRoom(id),
          this.fetchIpManager(),
        ]);

        const ipMap = new Map<string, IpManager>();
        if (Array.isArray(ipManagers)) {
          ipManagers.forEach((ipManager) => {
            if (!ipManager.workstation_id) return;
            ipMap.set(ipManager.workstation_id, ipManager);
          });
        }

        let layoutData: LayoutItem[] = [];
        let hasSavedLayout = false;

        if (layoutResult.success && layoutResult.data && Array.isArray(layoutResult.data) && layoutResult.data.length > 0) {
          layoutData = layoutResult.data;
          hasSavedLayout = true;
        }

        const doorElement = this.renderer.drawDoor();

        let maxX = 0;
        let maxY = 0;

        const doorItem = layoutData.find((item) => item.element_type === "door" || item.id === "00000000-0000-0000-0000-000000000001");
        if (doorItem && doorItem.position) {
          const rect = doorElement.querySelector("rect");
          if (rect) {
            rect.setAttribute("x", String(doorItem.position.x));
            rect.setAttribute("y", String(doorItem.position.y));
            rect.setAttribute("width", String(doorItem.position.width));
            rect.setAttribute("height", String(doorItem.position.height));
          }

          const text = doorElement.querySelector("text");
          if (text) {
            text.setAttribute("x", String(doorItem.position.x + doorItem.position.width / 2));
            text.setAttribute("y", String(doorItem.position.y - 10));
            (text as SVGElement).dataset.relY = "-10";
          }

          const circle = doorElement.querySelector("circle");
          if (circle) {
            circle.setAttribute("cx", String(doorItem.position.x + doorItem.position.width - 10));
            circle.setAttribute("cy", String(doorItem.position.y + doorItem.position.height / 2));
            (circle as SVGElement).dataset.relCx = String(doorItem.position.width - 10);
            (circle as SVGElement).dataset.relCy = String(doorItem.position.height / 2);
          }

          maxX = Math.max(maxX, doorItem.position.x + doorItem.position.width);
          maxY = Math.max(maxY, doorItem.position.y + doorItem.position.height);
        } else {
          const rect = doorElement.querySelector("rect");
          if (rect) {
            const x = parseFloat(rect.getAttribute("x") || "0");
            const y = parseFloat(rect.getAttribute("y") || "0");
            const width = parseFloat(rect.getAttribute("width") || "0");
            const height = parseFloat(rect.getAttribute("height") || "0");
            maxX = Math.max(maxX, x + width);
            maxY = Math.max(maxY, y + height);
          }
        }

        if (hasSavedLayout) {
          workstations.forEach((workstation, index) => {
            const savedItem = layoutData.find((item) => item.id.toLowerCase() === workstation.id.toLowerCase());
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
                height: height,
              };
            }
            workstation.ipManager = ipMap.get(workstation.id) || null;
            this.renderer.drawWorkstation(workstation);

            const pos = workstation.position;
            maxX = Math.max(maxX, pos.x + pos.width);
            maxY = Math.max(maxY, pos.y + pos.height);
          });
        }

        if (maxX > 0 || maxY > 0) {
          const padding = 50;
          this.core.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, maxX + padding)} ${Math.max(800, maxY + padding)}`);
        }

        return hasSavedLayout;
      } else if (this.core.type === "cabinet") {
        this.core.currentNetworkRegionId = id;
        this.core.elementsGroup.innerHTML = "";

        const [layoutResult, cabinets] = await Promise.all([
          this.apiGet<LayoutItem[]>(`/api/resources/layouts/positions/${id}`),
          this.fetchCabinetsByNetworkRegion(id),
        ]);

        let layoutData: LayoutItem[] = [];
        let hasSavedLayout = false;

        if (layoutResult.success && layoutResult.data && Array.isArray(layoutResult.data) && layoutResult.data.length > 0) {
          layoutData = layoutResult.data;
          hasSavedLayout = true;
        } else {
          this.showToast("数据库中没有网络区域布局数据", "info");
        }

        if (hasSavedLayout && cabinets.length > 0) {
          let maxX = 0;
          let maxY = 0;

          for (let index = 0; index < cabinets.length; index++) {
            const cabinet = cabinets[index];
            const savedItem = layoutData.find((item) => item.id.toLowerCase() === cabinet.id.toLowerCase());
            if (savedItem && savedItem.position) {
              cabinet.position = savedItem.position;
            } else {
              const cabinetWidth = 150;
              const gap = 50;
              const startX = 50;
              const bottomY = 600;
              cabinet.capacity = cabinet.capacity || 45;
              const uHeight = 20;
              const cabinetHeight = cabinet.capacity * uHeight + 40;

              cabinet.position = {
                x: startX + index * (cabinetWidth + gap),
                y: bottomY - cabinetHeight,
                width: cabinetWidth,
                height: cabinetHeight,
              };
            }

            this.renderer.drawCabinet(cabinet);
            await this.drawCabinetPositionsWithIp(cabinet);

            if (cabinet.position) {
              maxX = Math.max(maxX, cabinet.position.x + cabinet.position.width);
              maxY = Math.max(maxY, cabinet.position.y + cabinet.position.height);
            }
          }

          if (maxX > 0 || maxY > 0) {
            const padding = 50;
            this.core.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, maxX + padding)} ${Math.max(600, maxY + padding)}`);
          }
        }

        return hasSavedLayout;
      }
      return false;
    } catch (error) {
      console.error("加载布局失败:", error);
      this.showToast("加载布局失败", "error");
      this.core.elementsGroup.innerHTML = "";
      return false;
    }
  }

  async drawCabinetPositionsWithIp(cabinet: Cabinet): Promise<void> {
    try {
      const positions = await this.fetchCabinetPositions(cabinet.id);
      const ipResult = await this.apiGet<IpManager[] | { items: IpManager[]; data: IpManager[] }>("/api/resources/ip");
      const ipMap = new Map<string, IpManager>();

      if (ipResult.success && ipResult.data) {
        let ipList: IpManager[] = [];
        if (Array.isArray(ipResult.data)) {
          ipList = ipResult.data;
        } else if ("items" in ipResult.data && Array.isArray(ipResult.data.items)) {
          ipList = ipResult.data.items;
        } else if ("data" in ipResult.data && Array.isArray(ipResult.data.data)) {
          ipList = ipResult.data.data;
        }
        ipList.forEach((ipManager) => {
          if (ipManager.position_id) {
            ipMap.set(ipManager.position_id, ipManager);
          }
        });
      }

      if (positions && positions.length > 0) {
        positions.forEach((position) => {
          position.ipManager = ipMap.get(position.id) || null;
          this.renderer.drawCabinetPosition(position, cabinet);
        });
      }
    } catch (error) {
      console.error("绘制机位失败:", error);
    }
  }

  async saveLayout(): Promise<void> {
    const elements = this.core.elementsGroup.querySelectorAll("[data-id]");
    const layoutData: LayoutItem[] = [];

    elements.forEach((el) => {
      const element = el as SVGElement;
      const id = element.dataset.id;
      const rect = element.querySelector("rect");
      if (!rect || !id) return;

      const position: Position = {
        x: parseFloat(rect.getAttribute("x") || "0"),
        y: parseFloat(rect.getAttribute("y") || "0"),
        width: parseFloat(rect.getAttribute("width") || "0"),
        height: parseFloat(rect.getAttribute("height") || "0"),
        rotation: 0,
      };

      let element_type: string;
      if (this.core.type === "workstation") {
        element_type = element.classList.contains("door-element") ? "door" : "workstation";
      } else if (this.core.type === "cabinet") {
        element_type = "network_device";
      } else {
        element_type = "workstation";
      }

      layoutData.push({ id, position, element_type });
    });

    try {
      const result = await this.apiPost("/api/resources/layouts", {
        type: this.core.type === "cabinet" ? "network_region" : this.core.type,
        room_id: this.core.type === "workstation" ? this.core.currentRoomId : null,
        network_region_id: this.core.type === "cabinet" ? this.core.currentNetworkRegionId : null,
        cabinet_id: null,
        layout: layoutData,
      });

      if (result.success) {
        this.showToast("布局保存成功", "success");
      } else {
        this.showToast("布局保存失败: " + result.message, "error");
      }
    } catch (error) {
      console.error("保存布局失败:", error);
      this.showToast("布局保存失败", "error");
    }
  }

  async deleteLayout(): Promise<void> {
    if (this.core.type === "workstation") {
      if (!this.core.currentRoomId) {
        this.showToast("请先选择房间", "warning");
        return;
      }

      if (!confirm("确定要删除当前房间的工位布局数据吗？此操作不可恢复。")) {
        return;
      }

      try {
        const result = await this.apiDelete(`/api/resources/layouts/workstation/${this.core.currentRoomId}`);
        if (result.success) {
          this.core.elementsGroup.innerHTML = "";
          this.showToast("布局删除成功", "success");
        } else {
          this.showToast("布局删除失败: " + result.message, "error");
        }
      } catch (error) {
        console.error("删除布局失败:", error);
        this.core.elementsGroup.innerHTML = "";
        this.showToast("布局删除成功", "success");
      }
    } else if (this.core.type === "cabinet") {
      if (!this.core.currentNetworkRegionId) {
        this.showToast("请先选择网络区域", "warning");
        return;
      }

      if (!confirm("确定要删除当前网络区域的机位布局数据吗？此操作不可恢复。")) {
        return;
      }

      try {
        const result = await this.apiDelete(`/api/resources/layouts/positions/${this.core.currentNetworkRegionId}`);
        if (result.success) {
          this.core.elementsGroup.innerHTML = "";
          this.showToast("布局删除成功", "success");
        } else {
          this.showToast("布局删除失败: " + result.message, "error");
        }
      } catch (error) {
        console.error("删除布局失败:", error);
        this.core.elementsGroup.innerHTML = "";
        this.showToast("布局删除成功", "success");
      }
    }
  }
}
