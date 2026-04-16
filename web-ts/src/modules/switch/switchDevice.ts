import { openModal, closeModal } from "../../utils/modal.js";
import { debounce } from "../../utils/ui.js";
import { elementCache } from "../../utils/helpers.js";
import { t } from "../../utils/i18n.js";

import {
  getSwitchFormValues,
  setSwitchFormValues,
  resetSwitchForm,
} from "./switchForm.js";

import {
  toggleSnmpConfig,
  testSnmpConnection,
  getSwitchInfoFromSnmp,
  syncPortsFromSnmp,
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
  SWITCH_PORT_PAGE_SIZE,
} from "./switchState.js";

import {
  loadSwitchesData,
  fetchSwitchById,
  deleteSwitch,
  submitSwitchForm,
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
  showPortGroupsModal,
  extractPortNumber,
  extractPortLastNumber,
} from "./switchPort.js";

import {
  viewArpTable,
  viewLldpNeighbors,
  loadSwitchesForLldp,
  renderMacTable,
  bindCollapseEvents,
  groupByNetwork,
  filterEntries,
} from "./switchMacLldp.js";

let positionSelector: PositionSelector | null = null;

function filterSwitchesData(): void {
  const searchTerm = elementCache.getValue("switch-search");
  loadSwitchesData(searchTerm);
}

const debouncedFilterSwitchesData = debounce(filterSwitchesData, 300);

export async function openSwitchModal(sw: Record<string, unknown> | null = null): Promise<void> {
  openModal("switch-modal");
  const title = elementCache.get("switch-modal-title") as HTMLElement | null;
  const form = elementCache.get("switch-form") as HTMLFormElement | null;
  const snmpVersionSelect = elementCache.get("switch-snmp-version") as HTMLSelectElement | null;

  if (snmpVersionSelect && !snmpVersionSelect.dataset.bound) {
    snmpVersionSelect.addEventListener("change", toggleSnmpConfig);
    snmpVersionSelect.dataset.bound = "true";
  }

  if (positionSelector) {
    positionSelector.destroy();
  }

  positionSelector = new PositionSelector((regionId, regionName) => {
    updateNetworkRegion(regionId, regionName);
  });

  if (sw) {
    if (title) title.textContent = t("switch.edit_switch") || "编辑交换机";
    listState.currentSwitchId = sw.id as string;
    if (sw.ips && Array.isArray(sw.ips) && (sw.ips as unknown[]).length > 0) {
      const firstIp = (sw.ips as Record<string, unknown>[])[0];
      if (firstIp.network_region_id) {
        updateNetworkRegion(firstIp.network_region_id as string, (firstIp.network_region as string) || "");
      }
    }
    await positionSelector.init(sw as Parameters<PositionSelector["init"]>[0]);
    await setSwitchFormValues(sw);
  } else {
    if (title) title.textContent = t("switch.add_switch") || "添加交换机";
    if (form) form.reset();
    await resetSwitchForm();
    listState.currentSwitchId = null;
    await positionSelector.init(null);
  }

  requestAnimationFrame(() => {
    toggleSnmpConfig();
  });
}

export async function editSwitch(id: string | number): Promise<void> {
  const sw = await fetchSwitchById(String(id));
  if (sw) {
    await openSwitchModal(sw as unknown as Record<string, unknown>);
  }
}

async function saveSwitch(): Promise<void> {
  const formData = getSwitchFormValues();
  const success = await submitSwitchForm(formData);
  if (success) {
    closeModal("switch-modal");
    await loadSwitchesData();
  }
}

function initSwitchTabs(): void {
  const switchesContainer = elementCache.get("switches-container") as HTMLElement | null;
  if (!switchesContainer || switchesContainer.dataset.tabsInitialized === "true") return;

  const tabBtns = switchesContainer.querySelectorAll(".tab-btn");
  const tabPanes = switchesContainer.querySelectorAll(".tab-pane");

  tabBtns.forEach((btn) => {
    btn.addEventListener("click", () => {
      tabBtns.forEach(b => b.classList.remove("active"));
      tabPanes.forEach(p => p.classList.remove("active"));

      btn.classList.add("active");
      const tabId = (btn as HTMLElement).dataset.tab;
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

function initSwitchSearch(): void {
  const searchContainer = elementCache.get("switches-tab") as HTMLElement | null;
  if (!searchContainer || searchContainer.dataset.searchInitialized === "true") return;

  const searchInput = elementCache.get("switch-search") as HTMLInputElement | null;
  const refreshBtn = elementCache.get("switch-refresh-btn") as HTMLButtonElement | null;

  if (refreshBtn) {
    refreshBtn.addEventListener("click", () => loadSwitchesData(""));
  }

  if (searchInput) {
    searchInput.addEventListener("input", () => {
      debouncedFilterSwitchesData();
    });
    searchInput.addEventListener("keypress", (e) => {
      if (e.key === "Enter") {
        filterSwitchesData();
      }
    });
  }

  const portSearchInput = elementCache.get("switch-port-search") as HTMLInputElement | null;
  const portFilterBtn = elementCache.get("switch-port-filter-btn") as HTMLButtonElement | null;
  const portRefreshBtn = elementCache.get("switch-port-refresh-btn") as HTMLButtonElement | null;

  if (portFilterBtn) {
    portFilterBtn.addEventListener("click", filterSwitchPortsData);
  }

  if (portRefreshBtn) {
    portRefreshBtn.addEventListener("click", () => {
      const switchId = getCurrentSwitchId();
      if (switchId) {
        loadSwitchPortsBySwitchId(switchId);
      } else {
        loadSwitchPortsData(1, "");
      }
    });
  }

  if (portSearchInput) {
    portSearchInput.addEventListener("keypress", (e) => {
      if (e.key === "Enter") {
        filterSwitchPortsData();
      }
    });
  }

  searchContainer.dataset.searchInitialized = "true";
}

async function filterSwitchPortsData(): Promise<void> {
  const searchTerm = elementCache.getValue("switch-port-search");
  await loadSwitchPortsData(1, searchTerm);
}

function initSwitches(): void {
  initSwitchTabs();
  initSwitchSearch();

  const snmpVersionSelect = elementCache.get("switch-snmp-version") as HTMLSelectElement | null;
  if (snmpVersionSelect) {
    snmpVersionSelect.addEventListener("change", toggleSnmpConfig);
  }
}

export {
  initSwitches,
  initSwitchTabs,
  initSwitchSearch,
  loadSwitchesData,
  deleteSwitch,
  saveSwitch as submitSwitchForm,
  toggleSnmpConfig,
  testSnmpConnection,
  getSwitchInfoFromSnmp,
  syncPortsFromSnmp,
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
  setCurrentSwitchPage,
  getCurrentSwitchId,
  getCurrentSwitchName,
  SWITCH_PAGE_SIZE,
  SWITCH_PORT_PAGE_SIZE,
};
