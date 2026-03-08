import { loadModule } from "../utils/moduleLoader.js";
import { loadNetworkRegionsForSelect, loadRoomsForSelect, loadCabinets } from "../utils/resources.js";
import { apiGet } from "../utils/apiClient.js";
import { showToast } from "../utils/ui.js";
import { editWorkstation, deleteWorkstation } from "./workstation.js";
import { editCabinet, deleteCabinet } from "./cabinet.js";
import { editCabinetPosition, deleteCabinetPosition } from "./position.js";
import { elementCache } from "../utils/helpers.js";

// 可视化管理初始化
let workstationVisualization = null;
let cabinetVisualization = null;
let visualizationInitialized = false;

// 加载网络区域选项到选择框（用于可视化）
async function loadNetworkRegionsForVisualization(autoSelectFirst = false) {
  const select = elementCache.get("network-region-select");
  if (!select) return [];
  
  const regions = await loadNetworkRegionsForSelect(select, autoSelectFirst);
  
  // 如果需要自动选择第一个网络区域并绘制
  if (autoSelectFirst && regions.length > 0 && cabinetVisualization) {
    cabinetVisualization.autoDrawCabinetPositions(regions[0].id);
  }
  
  return regions;
}

// 加载机柜数据
export async function loadCabinetsForSelect(autoSelectFirst = false) {
  try {
    const cabinets = await loadCabinets();
    return cabinets || [];
  } catch (error) {
    console.error("加载机柜选项失败:", error);
    return [];
  }
}

// 动态导入并初始化可视化模块
export async function initVisualization() {
  if (visualizationInitialized) return;
  
  try {
    // 动态导入 SVGVisualization 模块
    const { SVGVisualization } = await loadModule('svgVisualization', '/static/js/modules/svgVisualization.js');
    
    // 初始化可视化模块的 tab 切换
    const visualizationContainer = elementCache.get("visualization");
    if (visualizationContainer) {
      const tabBtns = visualizationContainer.querySelectorAll(".tab-btn");
      const tabContents = visualizationContainer.querySelectorAll(".tab-content");

      tabBtns.forEach((btn) => {
          btn.addEventListener("click", function () {
              const tabId = this.getAttribute("data-tab");

              // 更新按钮状态
              tabBtns.forEach((b) => b.classList.remove("active"));
              this.classList.add("active");

              // 更新内容显示
              tabContents.forEach((content) => content.classList.remove("active"));
              const targetTab = elementCache.get(tabId);
              if (targetTab) {
              targetTab.classList.add("active");
              
              // 当切换到机位可视化标签时，自动加载布局
              if (tabId === "cabinet-visualization" && cabinetVisualization) {
                  const networkRegionId = elementCache.getValue("network-region-select");
                  if (networkRegionId) {
                  cabinetVisualization.loadSavedLayout(networkRegionId);
                  }
              }
              }
          });
          });
    }

    // 初始化工位可视化
    workstationVisualization = new SVGVisualization(
      "workstation-visualization-container",
      "workstation",
      {
        onEditWorkstation: editWorkstation,
        onEditCabinet: editCabinet,
        onEditCabinetPosition: editCabinetPosition,
        onDeleteWorkstation: deleteWorkstation,
        onDeleteCabinetPosition: deleteCabinetPosition
      }
    );

    // 初始化机位可视化
    cabinetVisualization = new SVGVisualization(
      "cabinet-visualization-container",
      "cabinet",
      {
        onEditWorkstation: editWorkstation,
        onEditCabinet: editCabinet,
        onEditCabinetPosition: editCabinetPosition,
        onDeleteWorkstation: deleteWorkstation,
        onDeleteCabinetPosition: deleteCabinetPosition
      }
    );

    // 加载房间和网络区域选项（网络区域自动选择第一个，flase不自动绘制）
    loadRoomsForSelect();
    loadNetworkRegionsForVisualization(false);

    // 绑定事件
    elementCache.get("room-select").addEventListener("change", (e) => {
      const roomId = e.target.value;
      // 房间选择变化时，只加载保存的布局，不自动绘制
      if (roomId) {
        workstationVisualization.loadSavedLayout(roomId);
      }
    });

    elementCache.get("network-region-select").addEventListener("change", (e) => {
      const networkRegionId = e.target.value;
      // 网络区域选择变化时，只加载保存的布局，不自动绘制
      if (networkRegionId) {
        cabinetVisualization.loadSavedLayout(networkRegionId);
      }
    });

    elementCache.get("auto-draw-workstations").addEventListener("click", async () => {
      const roomId = elementCache.getValue("room-select");
      if (roomId) {
        await workstationVisualization.autoDrawWorkstations(roomId);
        // 自动保存布局到新的svg_layouts表
        workstationVisualization.saveLayout();
      }
    });

    elementCache.get("auto-draw-cabinet-positions").addEventListener("click", async () => {
      const networkRegionId = elementCache.getValue("network-region-select");
      if (networkRegionId) {
        await cabinetVisualization.autoDrawCabinetPositions(networkRegionId);
        // 自动保存布局到新的svg_layouts表
        cabinetVisualization.saveLayout();
      }
    });

    elementCache.get("save-workstation-layout").addEventListener("click", () => {
      workstationVisualization.saveLayout();
    });

    elementCache.get("delete-workstation-layout").addEventListener("click", () => {
      workstationVisualization.deleteLayout();
    });

    elementCache.get("save-cabinet-layout").addEventListener("click", () => {
      cabinetVisualization.saveLayout();
    });

    elementCache.get("delete-cabinet-layout").addEventListener("click", () => {
      cabinetVisualization.deleteLayout();
    });

    // 页面加载完成后，自动加载第一个房间的布局
    setTimeout(() => {
      const visualizationSelect = elementCache.get("room-select");
      if (visualizationSelect && visualizationSelect.options.length > 0) {
        const firstRoomId = visualizationSelect.options[0].value;
        if (firstRoomId) {
          workstationVisualization.loadSavedLayout(firstRoomId);
        }
      }
    }, 500);
    
    visualizationInitialized = true;
  } catch (error) {
    console.error("初始化可视化模块失败:", error);
    showToast("可视化模块加载失败", "error");
  }
}
