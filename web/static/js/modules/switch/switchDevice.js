import { openModal, closeModal } from "../../utils/modal.js";
import { showToast } from "../../utils/ui.js";
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
  getSwitchPortsFromSnmp
} from "./switchSnmp.js";
import {
  createNetworkRegionState,
  PositionSelector
} from "./switchPosition.js";
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
  SWITCH_PORT_PAGE_SIZE
} from "./switchPort.js";
import {
  viewArpTable,
  viewLldpNeighbors,
  loadSwitchesForLldp,
  renderMacTable,
  bindCollapseEvents,
  groupByNetwork,
  filterEntries
} from "./switchMacLldp.js";
import {
  listState,
  createPositionState,
  setCurrentSwitchPage,
  getCurrentSwitchPage,
  setCurrentSwitchId,
  getCurrentSwitchId,
  setCurrentSwitchName,
  getCurrentSwitchName,
  getSwitchPositionData,
  resetSwitchPositionData
} from "./switchState.js";

const positionData = createPositionState();
const networkRegion = createNetworkRegionState();
let positionSelector = null;
function updateNetworkRegion(id, name = '') {
  networkRegion.id = id;
    networkRegion.name = name;
}
async function loadSwitchesDataWrapper(searchTerm = "") {
    const { loadSwitchesData } = await import("./switchList.js");
    await loadSwitchesData(listState, searchTerm);
}
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
    positionSelector = new PositionSelector(positionData, networkRegion, updateNetworkRegion);
    if (sw) {
        title.textContent = "编辑交换机";
        listState.currentSwitchId = sw.id;
        if (sw.ips && sw.ips.length > 0) {
            const firstIp = sw.ips[0];
            if (firstIp.network_region_id) {
                positionData.networkRegionId = firstIp.network_region_id;
                networkRegion.id = firstIp.network_region_id;
                networkRegion.name = firstIp.network_region || '';
            }
        }
        await positionSelector.init(sw);
        await setSwitchFormValues(sw, positionData, updateNetworkRegion);
    } else {
        title.textContent = "添加交换机";
        if (form) form.reset();
        await resetSwitchForm(positionData, updateNetworkRegion);
        listState.currentSwitchId = null;
        await positionSelector.init(null);
    }
    requestAnimationFrame(() => {
        toggleSnmpConfig();
    });
}
async function editSwitch(id) {
    const { fetchSwitchById } = await import("./switchList.js");
    const sw = await fetchSwitchById(id);
    console.log('editSwitch - fetched switch data:', sw);
    if (sw) {
        await openSwitchModal(sw);
    }
}
async function submitSwitchForm() {
    const { submitSwitchForm: saveSwitchForm } = await import("./switchList.js");
    const formData = getSwitchFormValues(positionData);
    console.log('submitSwitchForm - form data:', formData);
    const success = await saveSwitchForm(formData, listState);
    if (success) {
        closeModal("switch-modal");
        const { loadSwitchesData } = await import("./switchList.js");
        await loadSwitchesData(listState);
    }
}
function onNetworkRegionChange(regionId, regionName = '') {
    updateNetworkRegion(regionId, regionName);
}
async function deleteSwitchWrapper(id) {
    const { deleteSwitch } = await import("./switchList.js");
    return deleteSwitch(id, listState);
}
function testSnmpConnectionWrapper() {
    return testSnmpConnection(() => getSwitchFormValues(positionData));
}
function getSwitchInfoFromSnmpWrapper() {
    return getSwitchInfoFromSnmp(() => getSwitchFormValues(positionData));
}
export {
    loadSwitchesDataWrapper as loadSwitchesData,
    openSwitchModal,
    editSwitch,
    deleteSwitchWrapper as deleteSwitch,
    submitSwitchForm,
    toggleSnmpConfig,
    testSnmpConnectionWrapper as testSnmpConnection,
    getSwitchInfoFromSnmpWrapper as getSwitchInfoFromSnmp,
    getSwitchPortsFromSnmp,
    onNetworkRegionChange,
    setCurrentSwitchPage,
    getCurrentSwitchPage,
    setCurrentSwitchId,
    getCurrentSwitchId,
    setCurrentSwitchName,
    getCurrentSwitchName,
};
