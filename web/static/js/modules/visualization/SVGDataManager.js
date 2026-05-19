export class SVGDataManager {
  constructor(core, renderer) {
    this.core = core;
    this.renderer = renderer;
    this.apiGet = core.apiGet;
    this.apiPost = core.apiPost;
    this.apiDelete = core.apiDelete;
    this.showToast = core.showToast;
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
        if (result.data.ip_managers && Array.isArray(result.data.ip_managers)) return result.data.ip_managers;
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
    try {
      if (this.core.type === "workstation") {
        this.core.currentRoomId = id;
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
        
        if (layoutResult.success && layoutResult.data && Array.isArray(layoutResult.data) && layoutResult.data.length > 0) {
          layoutData = layoutResult.data;
          hasSavedLayout = true;
        }
        
        const doorElement = this.renderer.drawDoor();
        
        let maxX = 0;
        let maxY = 0;
        
        const doorItem = layoutData.find(item => item.element_type === "door" || item.id === "00000000-0000-0000-0000-000000000001");
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
            const savedItem = layoutData.find(item => item.id.toLowerCase() === workstation.id.toLowerCase());
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
          this.core.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, maxX + padding)} ${Math.max(800, maxY + padding)}`);
        }
        
        return hasSavedLayout;
      } else if (this.core.type === "cabinet") {
        this.core.currentRoomId = id;
        this.core.elementsGroup.innerHTML = "";
        
        const [layoutResult, cabinets] = await Promise.all([
          this.apiGet(`/api/resources/layouts/positions/${id}`),
          this.fetchCabinetsByRoom(id)
        ]);
        
        let layoutData = [];
        let hasSavedLayout = false;
        
        if (layoutResult.success && layoutResult.data && Array.isArray(layoutResult.data) && layoutResult.data.length > 0) {
          layoutData = layoutResult.data;
          hasSavedLayout = true;
        } else {
          this.showToast("数据库中没有房间布局数据", "info");
        }
        
        if (hasSavedLayout && cabinets.length > 0) {
          const containerHeight = this.core.container.clientHeight || 600;
          const padding = 5;
          const availableHeight = containerHeight - padding * 2;
          const maxCapacity = Math.max(...cabinets.map(c => c.capacity || 45));
          const uHeight = Math.floor((availableHeight - 40) / maxCapacity);
          
          let maxX = 0;
          let maxY = 0;
          
          for (let index = 0; index < cabinets.length; index++) {
            const cabinet = cabinets[index];
            const savedItem = layoutData.find(item => item.id.toLowerCase() === cabinet.id.toLowerCase());
            
            cabinet.capacity = cabinet.capacity || 45;
            const cabinetHeight = cabinet.capacity * uHeight + 40;
            
            if (savedItem && savedItem.position) {
              cabinet.position = savedItem.position;
              cabinet.position.height = cabinetHeight;
            } else {
              const cabinetWidth = 150;
              const gap = 50;
              const startX = 50;
              const svgHeight = containerHeight;
              const bottomY = svgHeight - padding;
              
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
            this.core.svg.setAttribute("viewBox", `0 0 ${Math.max(1000, maxX + padding)} ${containerHeight}`);
            this.core.svg.setAttribute("height", "100%");
          }
        }
        
        return hasSavedLayout;
      }
    } catch (error) {
      console.error("加载布局失败:", error);
      this.showToast("加载布局失败", "error");
      this.core.elementsGroup.innerHTML = "";
      return false;
    }
  }

  async drawCabinetPositionsWithIp(cabinet) {
    try {
      const positions = cabinet.positions || [];
      const ipResult = await this.apiGet("/api/resources/ip");
      const ipMap = new Map();
      
      if (ipResult.success && ipResult.data) {
        let ipList = [];
        if (Array.isArray(ipResult.data)) {
          ipList = ipResult.data;
        } else if (ipResult.data.items && Array.isArray(ipResult.data.items)) {
          ipList = ipResult.data.items;
        } else if (ipResult.data.data && Array.isArray(ipResult.data.data)) {
          ipList = ipResult.data.data;
        }
        ipList.forEach(ipManager => {
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

  async saveLayout() {
    const elements = this.core.elementsGroup.querySelectorAll("[data-id]");
    const layoutData = [];

    elements.forEach((el) => {
      const id = el.dataset.id;
      const rect = el.querySelector("rect");
      if (!rect) return;

      const position = {
        x: parseFloat(rect.getAttribute("x")),
        y: parseFloat(rect.getAttribute("y")),
        width: parseFloat(rect.getAttribute("width")),
        height: parseFloat(rect.getAttribute("height")),
        rotation: 0,
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

  async deleteLayout() {
    if (this.core.type === "workstation") {
      if (!this.core.currentRoomId) {
        this.showToast("请先选择房间", "warning");
        return;
      }

      const confirmed = await this.core.showConfirm("确定要删除当前房间的工位布局数据吗？此操作不可恢复。");
      if (!confirmed) {
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
        this.showToast("布局删除失败", "error");
      }
    } else if (this.core.type === "cabinet") {
      if (!this.core.currentRoomId) {
        this.showToast("请先选择房间", "warning");
        return;
      }

      const confirmed = await this.core.showConfirm("确定要删除当前房间的机位布局数据吗？此操作不可恢复。");
      if (!confirmed) {
        return;
      }

      try {
        const result = await this.apiDelete(`/api/resources/layouts/positions/${this.core.currentRoomId}`);
        if (result.success) {
          this.core.elementsGroup.innerHTML = "";
          this.showToast("布局删除成功", "success");
        } else {
          this.showToast("布局删除失败: " + result.message, "error");
        }
      } catch (error) {
        console.error("删除布局失败:", error);
        this.core.elementsGroup.innerHTML = "";
        this.showToast("布局删除失败", "error");
      }
    }
  }
}
