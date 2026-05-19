import { openModal, closeModal } from "../../utils/modal.js";
import { showToast, debounce } from "../../utils/ui.js";
import { elementCache } from "../../utils/helpers.js";

import {
  SWITCH_FORM_FIELDS,
  getSwitchFormValues,
  setSwitchFormValues,
  resetSwitchForm
} from "./switchForm.js";

import {
  toggleSnmpConfig,
  testSnmpConnection,
  getSwitchInfoFromSnmp,
  syncPortsFromSnmp
} from "./switchSnmp.js";

import { PositionSelector } from "./switchPosition.js";

import {
  listState,
  updateNetworkRegion,
  setCurrentSwitchId,
  setCurrentSwitchPage,
  setCurrentSwitchName,
  getCurrentSwitchId,
  getCurrentSwitchName,
  SWITCH_PAGE_SIZE,
  SWITCH_PORT_PAGE_SIZE
} from "./switchState.js";

import {
  loadSwitchesData,
  fetchSwitchById,
  deleteSwitch,
  submitSwitchForm,
  initSwitchFilters
} from "./switchList.js";

import {
  loadSwitchPortsData,
  loadSwitchPortsBySwitchId,
  manageSwitchPorts,
  openSwitchPortModal,
  editSwitchPort,
  deleteSwitchPort,
  submitSwitchPortForm,
  groupPorts,
  showPortGroupsModal
} from "./switchPort.js";

import {
  viewArpTable,
  viewLldpNeighbors,
  loadSwitchesForLldp,
  renderMacTable,
  bindCollapseEvents
} from "./switchMacLldp.js";

let positionSelector = null;

async function openSwitchModal(sw = null) {
  openModal("switch-modal");
  const title = elementCache.get('switch-modal-title');
  const form = elementCache.get('switch-form');
  const snmpVersionSelect = elementCache.get('switch-snmp-version');

  if (snmpVersionSelect && !snmpVersionSelect.dataset.bound) {
    snmpVersionSelect.addEventListener('change', toggleSnmpConfig);
    snmpVersionSelect.dataset.bound = 'true';
  }

  if (positionSelector) {
    positionSelector.destroy();
  }

  positionSelector = new PositionSelector((regionId, regionName) => {
    updateNetworkRegion(regionId, regionName);
  });

  if (sw) {
    title.textContent = "编辑交换机";
    listState.currentSwitchId = sw.id;
    if (sw.ips && sw.ips.length > 0) {
      const firstIp = sw.ips[0];
      if (firstIp.network_region_id) {
        updateNetworkRegion(firstIp.network_region_id, firstIp.network_region || '');
      }
    }
    await positionSelector.init(sw);
    await setSwitchFormValues(sw);
  } else {
    title.textContent = "添加交换机";
    if (form) form.reset();
    await resetSwitchForm();
    listState.currentSwitchId = null;
    await positionSelector.init(null);
  }

  requestAnimationFrame(() => {
    toggleSnmpConfig();
  });
}

async function editSwitch(id) {
  const sw = await fetchSwitchById(id);
  if (sw) {
    await openSwitchModal(sw);
  }
}

async function saveSwitch() {
  const formData = getSwitchFormValues();
  const success = await submitSwitchForm(formData);
  if (success) {
    closeModal("switch-modal");
    await loadSwitchesData();
  }
}

function onNetworkRegionChange(regionId, regionName = '') {
  updateNetworkRegion(regionId, regionName);
}

function initSwitchTabs() {
  const switchesContainer = elementCache.get("switches-container");
  if (!switchesContainer || switchesContainer.dataset.tabsInitialized === "true") return;

  const tabBtns = switchesContainer.querySelectorAll(".tab-btn");
  const tabPanes = switchesContainer.querySelectorAll(".tab-pane");

  tabBtns.forEach((btn) => {
    btn.addEventListener("click", () => {
      tabBtns.forEach((b) => b.classList.remove("active"));
      tabPanes.forEach((p) => p.classList.remove("active"));

      btn.classList.add("active");
      const tabId = btn.dataset.tab;
      const targetPane = switchesContainer.querySelector(`#${tabId}`);
      if (targetPane) targetPane.classList.add("active");

      if (tabId === "switches-list") {
        loadSwitchesData("");
      } else if (tabId === "switch-ports-list") {
        loadSwitchPortsData(1, "");
      }
    });
  });

  switchesContainer.dataset.tabsInitialized = "true";
}

function initSwitchSearch() {
  initSwitchFilters();
  
  const portSearchInput = elementCache.get("switch-port-search");
  const portFilterBtn = elementCache.get("switch-port-filter-btn");
  const portRefreshBtn = elementCache.get("switch-port-refresh-btn");

  if (portFilterBtn) {
    portFilterBtn.addEventListener("click", filterSwitchPortsData);
  }

  if (portRefreshBtn) {
    portRefreshBtn.addEventListener("click", () => {
      if (getCurrentSwitchId()) {
        loadSwitchPortsBySwitchId(getCurrentSwitchId());
      } else {
        loadSwitchPortsData(1, "");
      }
    });
  }

  if (portSearchInput) {
    portSearchInput.addEventListener("keypress", function (e) {
      if (e.key === "Enter") {
        filterSwitchPortsData();
      }
    });
  }
}

async function filterSwitchPortsData() {
  const searchTerm = elementCache.getValue("switch-port-search");
  await loadSwitchPortsData(searchTerm);
}

function initSwitches() {
  initSwitchTabs();
  initSwitchSearch();

  const snmpVersionSelect = elementCache.get("switch-snmp-version");
  if (snmpVersionSelect) {
    snmpVersionSelect.addEventListener("change", toggleSnmpConfig);
  }
}

export {
  initSwitchSearch,
  loadSwitchesData,
  openSwitchModal,
  editSwitch,
  deleteSwitch,
  saveSwitch as submitSwitchForm,
  toggleSnmpConfig,
  testSnmpConnection,
  getSwitchInfoFromSnmp,
  syncPortsFromSnmp,
  manageSwitchPorts,
  editSwitchPort,
  deleteSwitchPort,
  submitSwitchPortForm,
  viewArpTable,
  viewLldpNeighbors,
  setCurrentSwitchId,
  setCurrentSwitchName,
  setCurrentSwitchPage,
  getCurrentSwitchId,
  getCurrentSwitchName,
  SWITCH_PAGE_SIZE,
  SWITCH_PORT_PAGE_SIZE,
};
