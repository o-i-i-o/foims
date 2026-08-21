import { loadModule } from "../../utils/resourceLoader.js";
import {
  loadOrgsForSelect,
  loadVisualizationRoomsForSelect,
  getOrgSubtreeIds
} from "../../utils/resources.js";
import { elementCache, setActiveSubtab, getActiveSubtab } from "../../utils/helpers.js";
import { editWorkstation } from "../workstation.js";
import { editCabinet } from "../cabinet.js";
import { editCabinetPosition } from "../position.js";
import { showToast } from "../../utils/ui.js";
import { apiGet } from "../../utils/apiClient.js";
import { t } from "../../utils/i18n.js";
import { loadModal, openModal, closeModal } from "../../utils/modalLoader.js";

let workstationVisualization = null;
let cabinetVisualization = null;
let topologyVisualization = null;
let topologyModal = null;
let visualizationInitialized = false;

function createVisualizationCallbacks() {
  return {
    onEditWorkstation: editWorkstation,
    onEditCabinet: editCabinet,
    onEditCabinetPosition: editCabinetPosition
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
      setActiveSubtab("visualization", tabId);

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

  // 组织/房间类型筛选变化 → 重载房间列表并自动加载第一个房间
  const filterTargets = [
    ["viz-org-select", "workstation"],
    ["viz-room-type", "workstation"],
    ["cabinet-viz-org-select", "cabinet"],
    ["cabinet-room-type", "cabinet"]
  ];
  for (const [selectId, kind] of filterTargets) {
    elementCache.get(selectId).addEventListener("change", () => {
      refreshVisualizationRooms(kind);
    });
  }
}

/**
 * 按当前组织/房间类型筛选重载可视化房间选择器，并自动加载第一个房间。
 * @param {"workstation"|"cabinet"} kind 可视化视图类型
 */
async function refreshVisualizationRooms(kind) {
  const isWorkstation = kind === "workstation";
  const orgSelectId = isWorkstation ? "viz-org-select" : "cabinet-viz-org-select";
  const typeSelectId = isWorkstation ? "viz-room-type" : "cabinet-room-type";
  const roomSelectId = isWorkstation ? "room-select" : "cabinet-room-select";

  const orgId = elementCache.getValue(orgSelectId) || null;
  const selectedType = elementCache.getValue(typeSelectId) || null;
  // 未选具体类型时按视图类别全集过滤（工位=办公类、机柜=机房类，均含"其他"）
  const roomTypes = selectedType || (isWorkstation ? "office" : "datacenter");

  await loadVisualizationRoomsForSelect(roomSelectId, roomTypes, orgId);

  // 列表含空占位项，取第一个真实房间并自动加载其布局
  const roomSelect = elementCache.get(roomSelectId);
  const firstRoomId = [...(roomSelect?.options || [])].find((opt) => opt.value)?.value;
  if (firstRoomId) {
    roomSelect.value = firstRoomId;
    if (isWorkstation) {
      workstationVisualization.loadSavedLayout(firstRoomId);
    } else {
      cabinetVisualization.loadSavedLayout(firstRoomId);
    }
  }
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
  const toggleConnModeBtn = elementCache.get("toggle-connection-mode");
  const autoDiscoverBtn = elementCache.get("topology-auto-discover");
  const autoLayoutBtn = elementCache.get("topology-auto-layout");
  const saveLayoutBtn = elementCache.get("save-topology-layout");

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
    });
  }

  if (saveLayoutBtn) {
    saveLayoutBtn.addEventListener("click", () => {
      topologyVisualization.saveLayout();
    });
  }

  const openConnModalBtn = elementCache.get("open-topology-connection-modal");
  if (openConnModalBtn) {
    openConnModalBtn.addEventListener("click", openTopologyConnectionModal);
  }

  // 组织筛选：一个组织一套布局（设备位置按组织独立保存，切换即重新渲染）
  const topologyOrgSelect = elementCache.get("topology-org-select");
  if (topologyOrgSelect) {
    topologyOrgSelect.addEventListener("change", async (e) => {
      const orgId = e.target.value || null;
      const scope = orgId ? await getOrgSubtreeIds(orgId) : null;
      topologyVisualization.setOrgFilter(scope);
    });
  }
}

// ==================== 创建连线（手动物理示意 / 逻辑链路聚合） ====================

function fillTopologyConnectionDeviceOptions(selectEl, excludeId) {
  const devices = topologyVisualization.nodes.slice();
  selectEl.innerHTML = "";
  devices.forEach((d) => {
    if (excludeId && d.device_id === excludeId) return;
    const option = document.createElement("option");
    option.value = d.device_id;
    option.textContent = d.device_name || d.device_id;
    selectEl.appendChild(option);
  });
}

async function loadTopologyConnectionPorts(selectEl, deviceId) {
  selectEl.innerHTML = "";
  if (!deviceId) return;
  try {
    const result = await apiGet(`/api/resources/devices/${deviceId}/device-ports?page_size=200`);
    if (!result.success || !result.data) return;
    const ports = result.data.items || result.data || [];
    ports.forEach((p) => {
      const option = document.createElement("option");
      option.value = p.id;
      option.textContent = p.port_name
        ? `${p.port_number} (${p.port_name})`
        : p.port_number || p.id;
      selectEl.appendChild(option);
    });
  } catch (error) {
    console.error("加载设备端口失败:", error);
  }
}

function updateTopologyConnectionFormVisibility() {
  const type = elementCache.getValue("topo-conn-type");
  const isLogical = type === "logical";
  document
    .querySelectorAll(".topo-conn-ports-group")
    .forEach((el) => el.classList.toggle("hidden", !isLogical));
}

async function openTopologyConnectionModal() {
  const modal = await loadModal("topology-connection-modal");
  if (!modal) return;

  const typeSelect = elementCache.get("topo-conn-type");
  const sourceSelect = elementCache.get("topo-conn-source-device");
  const targetSelect = elementCache.get("topo-conn-target-device");
  const sourcePorts = elementCache.get("topo-conn-source-ports");
  const targetPorts = elementCache.get("topo-conn-target-ports");
  const labelInput = elementCache.get("topo-conn-label");
  const form = elementCache.get("topology-connection-form");

  fillTopologyConnectionDeviceOptions(sourceSelect, null);
  fillTopologyConnectionDeviceOptions(targetSelect, null);
  updateTopologyConnectionFormVisibility();
  if (typeSelect.value === "logical") {
    await loadTopologyConnectionPorts(sourcePorts, sourceSelect.value);
    await loadTopologyConnectionPorts(targetPorts, targetSelect.value);
  }
  labelInput.value = "";

  typeSelect.onchange = async () => {
    updateTopologyConnectionFormVisibility();
    if (typeSelect.value === "logical") {
      await loadTopologyConnectionPorts(sourcePorts, sourceSelect.value);
      await loadTopologyConnectionPorts(targetPorts, targetSelect.value);
    }
  };
  sourceSelect.onchange = async () => {
    if (typeSelect.value === "logical") {
      await loadTopologyConnectionPorts(sourcePorts, sourceSelect.value);
    }
  };
  targetSelect.onchange = async () => {
    if (typeSelect.value === "logical") {
      await loadTopologyConnectionPorts(targetPorts, targetSelect.value);
    }
  };

  form.onsubmit = async (e) => {
    e.preventDefault();

    const sourceDeviceId = sourceSelect.value;
    const targetDeviceId = targetSelect.value;
    if (!sourceDeviceId || !targetDeviceId) {
      showToast(t("viz.connection_device_required"), "warning");
      return;
    }
    if (sourceDeviceId === targetDeviceId) {
      showToast(t("viz.no_self_connection"), "warning");
      return;
    }

    const body = {
      connection_type: typeSelect.value,
      source_device_id: sourceDeviceId,
      target_device_id: targetDeviceId,
      label: labelInput.value.trim() || null
    };

    if (typeSelect.value === "logical") {
      body.source_port_ids = [...sourcePorts.selectedOptions].map((o) => o.value);
      body.target_port_ids = [...targetPorts.selectedOptions].map((o) => o.value);
      if (body.source_port_ids.length === 0 || body.target_port_ids.length === 0) {
        showToast(t("viz.logical_members_required"), "warning");
        return;
      }
    }

    const result = await topologyVisualization.dataManager.createConnection(body);
    if (result) {
      closeModal("topology-connection-modal");
      await topologyVisualization.loadTopology();
    }
  };

  openModal("topology-connection-modal");
}

async function loadInitialData() {
  // 先填充组织选项，再按默认筛选（全类别）加载两个视图的房间列表
  await Promise.all([
    loadOrgsForSelect("viz-org-select"),
    loadOrgsForSelect("cabinet-viz-org-select"),
    loadOrgsForSelect("topology-org-select")
  ]);
  await Promise.all([refreshVisualizationRooms("workstation"), refreshVisualizationRooms("cabinet")]);
}

// 窗口尺寸变化时机柜视图按新容器高度重排（防抖），保证柜底始终贴近屏幕底部
let cabinetResizeTimer = null;

function bindCabinetResizeRelayout() {
  window.addEventListener("resize", () => {
    if (!cabinetVisualization) return;
    if (getActiveSubtab("visualization") !== "cabinet-visualization") return;

    clearTimeout(cabinetResizeTimer);
    cabinetResizeTimer = setTimeout(() => {
      const roomId = elementCache.getValue("cabinet-room-select");
      if (roomId) {
        cabinetVisualization.loadSavedLayout(roomId);
      }
    }, 200);
  });
}

export async function initVisualization() {
  if (visualizationInitialized) return;

  try {
    const { SVGVisualization } = await loadModule(
      "SVGVisualization",
      "/static/js/modules/visualization/SVGVisualization.js"
    );
    const { TopologyVisualization } = await loadModule(
      "TopologyVisualization",
      "/static/js/modules/visualization/TopologyVisualization.js"
    );
    const { TopologyModal } = await loadModule(
      "TopologyModal",
      "/static/js/modules/visualization/TopologyModal.js"
    );

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
    };

    topologyVisualization.callbacks = {
      onDeviceDetail: (deviceId) => {
        const node = topologyVisualization.nodes.find((n) => n.device_id === deviceId);
        const name = node?.device_name || deviceId;
        topologyModal.open(deviceId, name);
      }
    };

    bindSelectEvents();
    bindAutoDrawEvents();
    bindLayoutEvents();
    bindTopologyEvents();
    bindCabinetResizeRelayout();
    loadInitialData();

    // 刷新后恢复上次记住的子标签（在可视化对象初始化完成后切换）
    const savedVizTabId = getActiveSubtab("visualization");
    const savedVizTabBtn = savedVizTabId
      ? document.querySelector(`#visualization .tab-btn[data-tab="${CSS.escape(savedVizTabId)}"]`)
      : null;
    if (savedVizTabBtn && !savedVizTabBtn.classList.contains("active")) {
      savedVizTabBtn.click();
    }

    visualizationInitialized = true;
  } catch (error) {
    console.error("初始化可视化模块失败:", error);
  }
}
