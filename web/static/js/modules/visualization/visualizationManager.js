import { loadModule } from "../../utils/moduleLoader.js";
import { loadRoomsForSelect, loadDataCenterRoomsForSelect, loadCabinets } from "../../utils/resources.js";
import { elementCache } from "../../utils/helpers.js";
import { editWorkstation, deleteWorkstation } from "../workstation.js";
import { editCabinet, deleteCabinet } from "../cabinet.js";
import { editCabinetPosition, deleteCabinetPosition } from "../position.js";

let workstationVisualization = null;
let cabinetVisualization = null;
let visualizationInitialized = false;

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
          const roomId = elementCache.getValue("cabinet-room-select");
          if (roomId) {
            cabinetVisualization.loadSavedLayout(roomId);
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

  elementCache.get("cabinet-room-select").addEventListener("change", (e) => {
    const roomId = e.target.value;
    if (roomId) {
      cabinetVisualization.loadSavedLayout(roomId);
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
    const roomId = elementCache.getValue("cabinet-room-select");
    if (roomId) {
      await cabinetVisualization.autoDrawCabinetPositions(roomId);
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

async function loadInitialData() {
  await Promise.all([
    loadRoomsForSelect(),
    loadDataCenterRoomsForSelect("cabinet-room-select")
  ]);

  await new Promise(resolve => requestAnimationFrame(resolve));
  await new Promise(resolve => requestAnimationFrame(resolve));

  const visualizationSelect = elementCache.get("room-select");
  if (visualizationSelect && visualizationSelect.options.length > 0) {
    const firstRoomId = visualizationSelect.options[0].value;
    if (firstRoomId) {
      workstationVisualization.loadSavedLayout(firstRoomId);
    }
  }

  const cabinetRoomSelect = elementCache.get("cabinet-room-select");
  if (cabinetRoomSelect && cabinetRoomSelect.options.length > 1) {
    const firstRoomId = cabinetRoomSelect.options[1].value;
    if (firstRoomId) {
      cabinetVisualization.loadSavedLayout(firstRoomId);
    }
  }
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
