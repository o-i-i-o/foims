import { loadModule } from "../../utils/moduleLoader.js";
import { loadNetworkRegionsForSelect, loadRoomsForSelect, loadCabinets } from "../../utils/resources.js";
import { elementCache } from "../../utils/helpers.js";
import { editWorkstation, deleteWorkstation } from "../workstation.js";
import { editCabinet, deleteCabinet } from "../cabinet.js";
import { editCabinetPosition, deleteCabinetPosition } from "../position.js";

let workstationVisualization = null;
let cabinetVisualization = null;
let visualizationInitialized = false;

async function loadNetworkRegionsForVisualization(autoSelectFirst = false) {
  const select = elementCache.get("network-region-select");
  if (!select) return [];
  
  const regions = await loadNetworkRegionsForSelect(select, autoSelectFirst);
  
  if (autoSelectFirst && regions.length > 0 && cabinetVisualization) {
    cabinetVisualization.autoDrawCabinetPositions(regions[0].id);
  }
  
  return regions;
}

export async function loadCabinetsForSelect(autoSelectFirst = false) {
  try {
    const cabinets = await loadCabinets();
    return cabinets || [];
  } catch (error) {
    console.error("加载机柜选项失败:", error);
    return [];
  }
}

function createVisualizationCallbacks() {
  return {
    onEditWorkstation: editWorkstation,
    onEditCabinet: editCabinet,
    onEditCabinetPosition: editCabinetPosition,
    onDeleteWorkstation: deleteWorkstation,
    onDeleteCabinetPosition: deleteCabinetPosition
  };
}

function initTabSwitching() {
  const visualizationContainer = elementCache.get("visualization");
  if (!visualizationContainer) return;

  const tabBtns = visualizationContainer.querySelectorAll(".tab-btn");
  const tabContents = visualizationContainer.querySelectorAll(".tab-content");

  tabBtns.forEach((btn) => {
    btn.addEventListener("click", function () {
      const tabId = this.getAttribute("data-tab");

      tabBtns.forEach((b) => b.classList.remove("active"));
      this.classList.add("active");

      tabContents.forEach((content) => content.classList.remove("active"));
      const targetTab = elementCache.get(tabId);
      if (targetTab) {
        targetTab.classList.add("active");
        
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

function bindSelectEvents() {
  elementCache.get("room-select").addEventListener("change", (e) => {
    const roomId = e.target.value;
    if (roomId) {
      workstationVisualization.loadSavedLayout(roomId);
    }
  });

  elementCache.get("network-region-select").addEventListener("change", (e) => {
    const networkRegionId = e.target.value;
    if (networkRegionId) {
      cabinetVisualization.loadSavedLayout(networkRegionId);
    }
  });
}

function bindAutoDrawEvents() {
  elementCache.get("auto-draw-workstations").addEventListener("click", async () => {
    const roomId = elementCache.getValue("room-select");
    if (roomId) {
      await workstationVisualization.autoDrawWorkstations(roomId);
      workstationVisualization.saveLayout();
    }
  });

  elementCache.get("auto-draw-cabinet-positions").addEventListener("click", async () => {
    const networkRegionId = elementCache.getValue("network-region-select");
    if (networkRegionId) {
      await cabinetVisualization.autoDrawCabinetPositions(networkRegionId);
      cabinetVisualization.saveLayout();
    }
  });
}

function bindLayoutEvents() {
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
}

function loadInitialData() {
  loadRoomsForSelect();
  loadNetworkRegionsForVisualization(false);

  setTimeout(() => {
    const visualizationSelect = elementCache.get("room-select");
    if (visualizationSelect && visualizationSelect.options.length > 0) {
      const firstRoomId = visualizationSelect.options[0].value;
      if (firstRoomId) {
        workstationVisualization.loadSavedLayout(firstRoomId);
      }
    }
  }, 500);
}

export async function initVisualization() {
  if (visualizationInitialized) return;
  
  try {
    const { SVGVisualization } = await loadModule('SVGVisualization', '/static/js/modules/visualization/SVGVisualization.js');
    
    initTabSwitching();

    const callbacks = createVisualizationCallbacks();

    workstationVisualization = new SVGVisualization(
      "workstation-visualization-container",
      "workstation",
      callbacks
    );

    cabinetVisualization = new SVGVisualization(
      "cabinet-visualization-container",
      "cabinet",
      callbacks
    );

    bindSelectEvents();
    bindAutoDrawEvents();
    bindLayoutEvents();
    loadInitialData();
    
    visualizationInitialized = true;
  } catch (error) {
    console.error("初始化可视化模块失败:", error);
  }
}
