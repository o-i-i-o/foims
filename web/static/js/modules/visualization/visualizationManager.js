import { loadModule } from "../../utils/moduleLoader.js";
import { loadRoomsForSelect, loadDataCenterRoomsForSelect, loadCabinets } from "../../utils/resources.js";
import { elementCache } from "../../utils/helpers.js";
import { editWorkstation, deleteWorkstation } from "../workstation.js";
import { editCabinet, deleteCabinet } from "../cabinet.js";
import { editCabinetPosition, deleteCabinetPosition } from "../position.js";
import { showToast } from "../../utils/ui.js";
import { apiGet } from "../../utils/apiClient.js";
import { t } from "../../utils/i18n.js";

let workstationVisualization = null;
let cabinetVisualization = null;
let topologyVisualization = null;
let topologyModal = null;
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

        if (tabId === "global-visualization" && topologyVisualization) {
          topologyVisualization.loadTopology();
        }
      }

      if (tabId !== "global-visualization" && topologyModal && topologyModal.isOpen()) {
        topologyModal.close();
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

function bindTopologyEvents() {
  const deviceSelect = elementCache.get("topology-device-select");
  const addDeviceBtn = elementCache.get("add-device-to-topology");
  const toggleConnModeBtn = elementCache.get("toggle-connection-mode");
  const autoDiscoverBtn = elementCache.get("topology-auto-discover");
  const autoLayoutBtn = elementCache.get("topology-auto-layout");
  const saveLayoutBtn = elementCache.get("save-topology-layout");
  const deleteLayoutBtn = elementCache.get("delete-topology-layout");

  if (addDeviceBtn) {
    addDeviceBtn.addEventListener("click", async () => {
      const selectEl = deviceSelect;
      if (!selectEl) return;
      const deviceId = selectEl.value;
      if (!deviceId) {
        showToast(t("visualization.select_device"), "warning");
        return;
      }
      const option = selectEl.selectedOptions[0];
      const deviceName = option?.textContent || deviceId;
      const deviceType = option?.dataset.deviceType || "other";
      await topologyVisualization.addDevice(deviceId, deviceName, deviceType);
      selectEl.value = "";
    });
  }

  if (toggleConnModeBtn) {
    toggleConnModeBtn.addEventListener("click", () => {
      const isInMode = topologyVisualization.toggleConnectionMode();
      toggleConnModeBtn.classList.toggle("active-mode", isInMode);
      toggleConnModeBtn.textContent = isInMode
        ? t("visualization.exit_connection_mode")
        : t("visualization.connection_mode");
    });
  }

  if (autoLayoutBtn) {
    autoLayoutBtn.addEventListener("click", () => {
      topologyVisualization.autoLayout();
    });
  }

  if (autoDiscoverBtn) {
    autoDiscoverBtn.addEventListener("click", async () => {
      await topologyVisualization.autoDiscover();
      loadDeviceOptions();
    });
  }

  if (saveLayoutBtn) {
    saveLayoutBtn.addEventListener("click", () => {
      topologyVisualization.saveLayout();
    });
  }

  if (deleteLayoutBtn) {
    deleteLayoutBtn.addEventListener("click", () => {
      topologyVisualization.deleteLayout();
    });
  }
}

async function loadDeviceOptions() {
  const select = elementCache.get("topology-device-select");
  if (!select) return;

  try {
    const result = await apiGet("/api/resources/devices?page_size=1000");
    if (!result.success || !result.data) return;

    const devices = result.data.items || result.data || [];
    select.innerHTML = `<option value="" data-i18n="visualization.select_device">${t("visualization.select_device")}</option>`;

    const existingIds = new Set(
      topologyVisualization.nodes.map((n) => n.device_id)
    );

    devices.forEach((d) => {
      if (existingIds.has(d.id)) return;
      const option = document.createElement("option");
      option.value = d.id;
      option.textContent = d.name || d.id;
      option.dataset.deviceType = d.device_type || "other";
      select.appendChild(option);
    });
  } catch (error) {
    console.error("加载设备列表失败:", error);
  }
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
    const { TopologyVisualization } = await loadModule('TopologyVisualization', '/static/js/modules/visualization/TopologyVisualization.js');
    const { TopologyModal } = await loadModule('TopologyModal', '/static/js/modules/visualization/TopologyModal.js');
    
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

    topologyVisualization = new TopologyVisualization("global-visualization-container");

    topologyModal = new TopologyModal();
    topologyModal.onRemoveDevice = (deviceId) => {
      topologyVisualization.deleteDevice(deviceId);
      loadDeviceOptions();
    };

    topologyVisualization.callbacks = {
      onDeviceDetail: (deviceId) => {
        const node = topologyVisualization.nodes.find((n) => n.device_id === deviceId);
        const name = node?.device_name || deviceId;
        topologyModal.open(deviceId, name);
      },
    };

    bindSelectEvents();
    bindAutoDrawEvents();
    bindLayoutEvents();
    bindTopologyEvents();
    loadInitialData();
    loadDeviceOptions();
    
    visualizationInitialized = true;
  } catch (error) {
    console.error("初始化可视化模块失败:", error);
  }
}
