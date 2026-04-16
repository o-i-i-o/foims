// Visualization manager
import { SVGVisualization } from "./SVGVisualization.js";
import { loadNetworkRegionsForSelect, loadRoomsForSelect } from "../../utils/resources.js";
import type { NetworkRegion } from "../../types/resources.js";
import type { Position } from "./SVGCore.js";

let workstationVisualization: SVGVisualization | null = null;
let cabinetVisualization: SVGVisualization | null = null;
let visualizationInitialized = false;

async function loadNetworkRegionsForVisualization(autoSelectFirst = false): Promise<NetworkRegion[]> {
  const select = document.getElementById("network-region-select") as HTMLSelectElement | null;
  if (!select) return [];

  const regions = await loadNetworkRegionsForSelect(select, autoSelectFirst);

  if (autoSelectFirst && regions.length > 0 && cabinetVisualization) {
    await cabinetVisualization.autoDrawCabinetPositions(String(regions[0].id));
  }

  return regions;
}

function initTabSwitching(): void {
  const visualizationContainer = document.getElementById("visualization");
  if (!visualizationContainer) return;

  const tabBtns = visualizationContainer.querySelectorAll(".tab-btn");
  const tabContents = visualizationContainer.querySelectorAll(".tab-content");

  tabBtns.forEach((btn) => {
    btn.addEventListener("click", function (this: HTMLElement) {
      const tabId = this.getAttribute("data-tab");

      tabBtns.forEach((b) => b.classList.remove("active"));
      this.classList.add("active");

      tabContents.forEach((content) => content.classList.remove("active"));
      const targetTab = document.getElementById(tabId!);
      if (targetTab) {
        targetTab.classList.add("active");

        if (tabId === "cabinet-visualization" && cabinetVisualization) {
          const networkRegionId = (document.getElementById("network-region-select") as HTMLSelectElement | null)?.value;
          if (networkRegionId) {
            cabinetVisualization.loadSavedLayout(networkRegionId);
          }
        }
      }
    });
  });
}

function bindSelectEvents(): void {
  const roomSelect = document.getElementById("room-select");
  if (roomSelect) {
    roomSelect.addEventListener("change", (e) => {
      const roomId = (e.target as HTMLSelectElement).value;
      if (roomId && workstationVisualization) {
        workstationVisualization.loadSavedLayout(roomId);
      }
    });
  }

  const networkRegionSelect = document.getElementById("network-region-select");
  if (networkRegionSelect) {
    networkRegionSelect.addEventListener("change", (e) => {
      const networkRegionId = (e.target as HTMLSelectElement).value;
      if (networkRegionId && cabinetVisualization) {
        cabinetVisualization.loadSavedLayout(networkRegionId);
      }
    });
  }
}

function bindAutoDrawEvents(): void {
  const autoDrawWorkstationsBtn = document.getElementById("auto-draw-workstations");
  if (autoDrawWorkstationsBtn) {
    autoDrawWorkstationsBtn.addEventListener("click", async () => {
      const roomId = (document.getElementById("room-select") as HTMLSelectElement | null)?.value;
      if (roomId && workstationVisualization) {
        await workstationVisualization.autoDrawWorkstations(roomId);
        workstationVisualization.saveLayout();
      }
    });
  }

  const autoDrawCabinetPositionsBtn = document.getElementById("auto-draw-cabinet-positions");
  if (autoDrawCabinetPositionsBtn) {
    autoDrawCabinetPositionsBtn.addEventListener("click", async () => {
      const networkRegionId = (document.getElementById("network-region-select") as HTMLSelectElement | null)?.value;
      if (networkRegionId && cabinetVisualization) {
        await cabinetVisualization.autoDrawCabinetPositions(networkRegionId);
        cabinetVisualization.saveLayout();
      }
    });
  }

  const saveLayoutBtn = document.getElementById("save-layout-btn");
  if (saveLayoutBtn) {
    saveLayoutBtn.addEventListener("click", () => {
      const activeTab = document.querySelector("#visualization .tab-btn.active");
      const tabId = activeTab?.getAttribute("data-tab");
      if (tabId === "workstation-visualization" && workstationVisualization) {
        workstationVisualization.saveLayout();
      } else if (tabId === "cabinet-visualization" && cabinetVisualization) {
        cabinetVisualization.saveLayout();
      }
    });
  }

  const deleteLayoutBtn = document.getElementById("delete-layout-btn");
  if (deleteLayoutBtn) {
    deleteLayoutBtn.addEventListener("click", () => {
      const activeTab = document.querySelector("#visualization .tab-btn.active");
      const tabId = activeTab?.getAttribute("data-tab");
      if (tabId === "workstation-visualization" && workstationVisualization) {
        workstationVisualization.deleteLayout();
      } else if (tabId === "cabinet-visualization" && cabinetVisualization) {
        cabinetVisualization.deleteLayout();
      }
    });
  }

  const toggleGridBtn = document.getElementById("toggle-grid-btn");
  if (toggleGridBtn) {
    toggleGridBtn.addEventListener("click", () => {
      const activeTab = document.querySelector("#visualization .tab-btn.active");
      const tabId = activeTab?.getAttribute("data-tab");
      if (tabId === "workstation-visualization" && workstationVisualization) {
        workstationVisualization.toggleSnapToGrid(!workstationVisualization.snapToGrid);
      } else if (tabId === "cabinet-visualization" && cabinetVisualization) {
        cabinetVisualization.toggleSnapToGrid(!cabinetVisualization.snapToGrid);
      }
    });
  }
}

export async function initVisualization(): Promise<void> {
  if (visualizationInitialized) return;

  const workstationContainer = document.getElementById("workstation-visualization-container");
  const cabinetContainer = document.getElementById("cabinet-visualization-container");

  if (workstationContainer) {
    workstationVisualization = new SVGVisualization("workstation-visualization-container", "workstation", {
      onElementSelect: (_element: SVGElement, data: Record<string, unknown>) => {
        console.log("Selected workstation:", data);
      },
      onElementMove: (_element: SVGElement, position: Position) => {
        console.log("Moved workstation to:", position);
      },
    });
  }

  if (cabinetContainer) {
    cabinetVisualization = new SVGVisualization("cabinet-visualization-container", "cabinet", {
      onElementSelect: (_element: SVGElement, data: Record<string, unknown>) => {
        console.log("Selected cabinet position:", data);
      },
      onElementMove: (_element: SVGElement, position: Position) => {
        console.log("Moved cabinet position to:", position);
      },
    });
  }

  await loadNetworkRegionsForVisualization();
  await loadRoomsForSelect();

  initTabSwitching();
  bindSelectEvents();
  bindAutoDrawEvents();

  visualizationInitialized = true;
}

export function destroyVisualization(): void {
  if (workstationVisualization) {
    workstationVisualization.destroy();
    workstationVisualization = null;
  }
  if (cabinetVisualization) {
    cabinetVisualization.destroy();
    cabinetVisualization = null;
  }
  visualizationInitialized = false;
}

export { workstationVisualization, cabinetVisualization };
