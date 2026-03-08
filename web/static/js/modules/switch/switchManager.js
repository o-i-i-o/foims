import {
  loadSwitchesData,
  openSwitchModal,
  editSwitch,
  deleteSwitch,
  submitSwitchForm,
  toggleSnmpConfig,
  testSnmpConnection,
  getSwitchInfoFromSnmp,
  getSwitchPortsFromSnmp,
  onNetworkRegionChange,
  setCurrentSwitchId,
  setCurrentSwitchName,
  getCurrentSwitchId,
  getCurrentSwitchName,
  SWITCH_PAGE_SIZE,
  loadSwitchPortsData,
  loadSwitchPortsBySwitchId,
  manageSwitchPorts,
  openSwitchPortModal,
  editSwitchPort,
  deleteSwitchPort,
  submitSwitchPortForm,
  groupPorts,
  showPortGroupsModal,
  extractPortNumber,
  extractPortLastNumber,
  SWITCH_PORT_PAGE_SIZE,
  viewArpTable,
  viewLldpNeighbors,
  loadSwitchesForLldp,
  renderMacTable,
  bindCollapseEvents,
  groupByNetwork,
  filterEntries,
} from "./switchDevice.js";

import { showToast, debounce } from "../../utils/ui.js";
import { elementCache } from "../../utils/helpers.js";

const debouncedFilterSwitchesData = debounce(filterSwitchesData, 300);

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
        loadSwitchesData(1, "");
      } else if (tabId === "switch-ports-list") {
        loadSwitchPortsData(1, "");
      }
    });
  });

  switchesContainer.dataset.tabsInitialized = "true";
}

function initSwitchSearch() {
  const searchContainer = elementCache.get("switches-tab");
  if (!searchContainer || searchContainer.dataset.searchInitialized === "true") return;

  const searchInput = elementCache.get("switch-search");
  const refreshBtn = elementCache.get("switch-refresh-btn");

  if (refreshBtn) {
    refreshBtn.addEventListener("click", () => loadSwitchesData(1, ""));
  }

  if (searchInput) {
    searchInput.addEventListener("input", function () {
      debouncedFilterSwitchesData();
    });
    searchInput.addEventListener("keypress", function (e) {
      if (e.key === "Enter") {
        filterSwitchesData();
      }
    });
  }

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

  searchContainer.dataset.searchInitialized = "true";
}

async function filterSwitchesData() {
  const searchTerm = elementCache.getValue("switch-search");
  await loadSwitchesData(1, searchTerm);
}

async function filterSwitchPortsData() {
  const searchTerm = elementCache.getValue("switch-port-search");
  await loadSwitchPortsData(1, searchTerm);
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
  initSwitches,
  initSwitchTabs,
  initSwitchSearch,
  
  loadSwitchesData,
  openSwitchModal,
  editSwitch,
  deleteSwitch,
  submitSwitchForm,
  toggleSnmpConfig,
  testSnmpConnection,
  getSwitchInfoFromSnmp,
  getSwitchPortsFromSnmp,
  onNetworkRegionChange,
  
  loadSwitchPortsData,
  loadSwitchPortsBySwitchId,
  manageSwitchPorts,
  openSwitchPortModal,
  editSwitchPort,
  deleteSwitchPort,
  submitSwitchPortForm,
  groupPorts,
  showPortGroupsModal,
  extractPortNumber,
  extractPortLastNumber,
  
  viewArpTable,
  viewLldpNeighbors,
  loadSwitchesForLldp,
  renderMacTable,
  bindCollapseEvents,
  groupByNetwork,
  filterEntries,
  
  setCurrentSwitchId,
  setCurrentSwitchName,
  getCurrentSwitchId,
  getCurrentSwitchName,
  
  SWITCH_PAGE_SIZE,
  SWITCH_PORT_PAGE_SIZE,
};
