// Visualization manager
import { SVGVisualization } from "./SVGVisualization.js";
import { loadNetworkRegionsForSelect, loadRoomsForSelect } from "../../utils/resources.js";
let workstationVisualization = null;
let cabinetVisualization = null;
let visualizationInitialized = false;
async function loadNetworkRegionsForVisualization(autoSelectFirst = false) {
    const select = document.getElementById("network-region-select");
    if (!select)
        return [];
    const regions = await loadNetworkRegionsForSelect(select, autoSelectFirst);
    if (autoSelectFirst && regions.length > 0 && cabinetVisualization) {
        await cabinetVisualization.autoDrawCabinetPositions(String(regions[0].id));
    }
    return regions;
}
function initTabSwitching() {
    const visualizationContainer = document.getElementById("visualization");
    if (!visualizationContainer)
        return;
    const tabBtns = visualizationContainer.querySelectorAll(".tab-btn");
    const tabContents = visualizationContainer.querySelectorAll(".tab-content");
    tabBtns.forEach((btn) => {
        btn.addEventListener("click", function () {
            const tabId = this.getAttribute("data-tab");
            tabBtns.forEach((b) => b.classList.remove("active"));
            this.classList.add("active");
            tabContents.forEach((content) => content.classList.remove("active"));
            const targetTab = document.getElementById(tabId);
            if (targetTab) {
                targetTab.classList.add("active");
                if (tabId === "cabinet-visualization" && cabinetVisualization) {
                    const networkRegionId = document.getElementById("network-region-select")?.value;
                    if (networkRegionId) {
                        cabinetVisualization.loadSavedLayout(networkRegionId);
                    }
                }
            }
        });
    });
}
function bindSelectEvents() {
    const roomSelect = document.getElementById("room-select");
    if (roomSelect) {
        roomSelect.addEventListener("change", (e) => {
            const roomId = e.target.value;
            if (roomId && workstationVisualization) {
                workstationVisualization.loadSavedLayout(roomId);
            }
        });
    }
    const networkRegionSelect = document.getElementById("network-region-select");
    if (networkRegionSelect) {
        networkRegionSelect.addEventListener("change", (e) => {
            const networkRegionId = e.target.value;
            if (networkRegionId && cabinetVisualization) {
                cabinetVisualization.loadSavedLayout(networkRegionId);
            }
        });
    }
}
function bindAutoDrawEvents() {
    const autoDrawWorkstationsBtn = document.getElementById("auto-draw-workstations");
    if (autoDrawWorkstationsBtn) {
        autoDrawWorkstationsBtn.addEventListener("click", async () => {
            const roomId = document.getElementById("room-select")?.value;
            if (roomId && workstationVisualization) {
                await workstationVisualization.autoDrawWorkstations(roomId);
                workstationVisualization.saveLayout();
            }
        });
    }
    const autoDrawCabinetPositionsBtn = document.getElementById("auto-draw-cabinet-positions");
    if (autoDrawCabinetPositionsBtn) {
        autoDrawCabinetPositionsBtn.addEventListener("click", async () => {
            const networkRegionId = document.getElementById("network-region-select")?.value;
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
            }
            else if (tabId === "cabinet-visualization" && cabinetVisualization) {
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
            }
            else if (tabId === "cabinet-visualization" && cabinetVisualization) {
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
            }
            else if (tabId === "cabinet-visualization" && cabinetVisualization) {
                cabinetVisualization.toggleSnapToGrid(!cabinetVisualization.snapToGrid);
            }
        });
    }
}
export async function initVisualization() {
    if (visualizationInitialized)
        return;
    const workstationContainer = document.getElementById("workstation-visualization-container");
    const cabinetContainer = document.getElementById("cabinet-visualization-container");
    if (workstationContainer) {
        workstationVisualization = new SVGVisualization("workstation-visualization-container", "workstation", {
            onElementSelect: (_element, data) => {
                console.log("Selected workstation:", data);
            },
            onElementMove: (_element, position) => {
                console.log("Moved workstation to:", position);
            },
        });
    }
    if (cabinetContainer) {
        cabinetVisualization = new SVGVisualization("cabinet-visualization-container", "cabinet", {
            onElementSelect: (_element, data) => {
                console.log("Selected cabinet position:", data);
            },
            onElementMove: (_element, position) => {
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
export function destroyVisualization() {
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
//# sourceMappingURL=visualizationManager.js.map