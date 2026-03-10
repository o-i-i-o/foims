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

import { PositionSelector } from "./switchPosition.js";

import {
  listState,
  updateNetworkRegion,
  setCurrentSwitchId,
  setCurrentSwitchPage
} from "./switchState.js";

import {
  loadSwitchesData,
  fetchSwitchById,
  deleteSwitch,
  submitSwitchForm
} from "./switchList.js";

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
  console.log('editSwitch - fetched switch data:', sw);
  if (sw) {
    await openSwitchModal(sw);
  }
}

async function saveSwitch() {
  const formData = getSwitchFormValues();
  console.log('saveSwitch - form data:', formData);
  const success = await submitSwitchForm(formData);
  if (success) {
    closeModal("switch-modal");
    await loadSwitchesData();
  }
}

function onNetworkRegionChange(regionId, regionName = '') {
  updateNetworkRegion(regionId, regionName);
}

export {
  loadSwitchesData,
  openSwitchModal,
  editSwitch,
  deleteSwitch,
  saveSwitch as submitSwitchForm,
  toggleSnmpConfig,
  testSnmpConnection,
  getSwitchInfoFromSnmp,
  getSwitchPortsFromSnmp,
  onNetworkRegionChange,
  setCurrentSwitchPage,
  setCurrentSwitchId
};
