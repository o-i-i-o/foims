import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  handleError,
  appendPaginationToTable,
  escapeHtml,
  DEFAULT_PAGE_SIZE,
  createSortState,
  updateSortIcons,
  initSortEvents,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";
import {
  loadNetOutletsForSelect,
  loadDeviceTemplatesForSelect,
  loadWorkstationsForSelect,
  loadPositionsForSelect,
} from "../utils/resources.js";
import { getManager } from "../utils/ipconfig.js";
import {
  toggleSnmpConfig,
  testSnmpConnection,
  getDeviceInfoFromSnmp,
} from "./deviceSnmp.js";
import { manageDevicePorts } from "./devicePorts.js";
import { viewArpTable, viewLldpNeighbors } from "./deviceMacLldp.js";

const tableState = createSortState('name', 'asc');
let currentPage = 1;

const DEVICE_TYPE_LABELS = {
  pc: t('device_type.pc') || 'PC',
  laptop: t('device_type.laptop') || '笔记本',
  printer: t('device_type.printer') || '打印机',
  server: t('device_type.server') || '服务器',
  network_device: t('device_type.network_device') || '网络设备',
  switch: t('device_type.switch') || '交换机',
  camera: t('device_type.camera') || '摄像头',
  phone: t('device_type.phone') || '电话',
  other: t('device_type.other') || '其他',
};

function getDeviceTypeName(type) {
  return DEVICE_TYPE_LABELS[type] || type;
}

const SNMP_FORM_FIELDS = [
  'device-snmp-version', 'device-snmp-port', 'device-snmp-community',
  'device-snmp-username', 'device-snmp-auth-protocol',
  'device-snmp-auth-password', 'device-snmp-priv-protocol',
  'device-snmp-priv-password'
];

const PASSWORD_MASK = '••••••••';

function maskToNull(value) {
  if (!value || value === PASSWORD_MASK || value.trim() === '') {
    return null;
  }
  return value;
}

let deviceTableClickHandler = null;

export async function loadDevicesData(page = 1, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);

  try {
    const result = await apiGet(`/api/resources/devices?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const devices = data.items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#devices-table", {
      data: devices,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'device_type', render: (v) => getDeviceTypeName(v) },
        { field: 'brand', render: (v) => escapeHtml(v) || '-' },
        { field: 'model', render: (v) => escapeHtml(v) || '-' },
        { field: 'workstation_name', render: (v, row) => {
          if (v) return `${t('device.workstation')}: ${escapeHtml(v)}`;
          if (row.cabinet_name) return `${t('device.position')}: ${escapeHtml(row.cabinet_name)}${row.start_u ? ` ${row.start_u}-${row.end_u}U` : ''}`;
          return '-';
        }},
        { field: 'net_outlet_name', render: (v, row) => {
          if (v) return `${t('device.net_outlet')}: ${escapeHtml(v)}`;
          return '-';
        }},
        { field: 'room_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'id', render: (v, row) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-secondary btn-device-ports" data-device-id="${v}" data-device-name="${escapeHtml(row.name)}">${t('device.ports') || '端口'}</button>
          <button class="btn btn-sm btn-secondary btn-device-mac" data-device-id="${v}">${t('device.mac_table') || 'MAC表'}</button>
          <button class="btn btn-sm btn-secondary btn-device-lldp" data-device-id="${v}">${t('device.lldp') || 'LLDP'}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('common.no_data')
    });

    bindDeviceButtonsEvents();

    if (data.total !== undefined) {
      appendPaginationToTable("#devices-table", data, loadDevicesData);
    }
    updateSortIcons("devices-table", tableState);
  } catch (error) {
    handleError(error, t('device.load_failed'), () => {
      renderTable("#devices-table", { data: [], columns: [], emptyMessage: t('common.load_failed_retry') });
    });
  }
}

function bindDeviceButtonsEvents() {
  const table = elementCache.get("devices-table");
  if (!table) return;

  if (deviceTableClickHandler) {
    table.removeEventListener("click", deviceTableClickHandler);
  }

  deviceTableClickHandler = async (e) => {
    const target = e.target;
    const deviceId = target.dataset.deviceId;
    const deviceName = target.dataset.deviceName;
    const id = target.dataset.id;

    if (target.classList.contains("btn-device-ports") && deviceId) {
      manageDevicePorts(deviceId, deviceName || "");
    } else if (target.classList.contains("btn-device-mac") && deviceId) {
      viewArpTable(deviceId);
    } else if (target.classList.contains("btn-device-lldp") && deviceId) {
      viewLldpNeighbors(deviceId);
    }
  };

  table.addEventListener("click", deviceTableClickHandler);
}

export function initDeviceSortEvents() {
  initSortEvents("devices-table", tableState, loadDevicesData);
}

export async function editDevice(id) {
  try {
    const result = await apiGet(`/api/resources/devices/${id}`);
    if (result.success) {
      openDeviceModal(result.data);
    } else {
      showToast(`${t('device.load_failed')}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t('device.load_failed'));
  }
}

export async function deleteDevice(id) {
  await handleDelete(id, "/api/resources/devices", t('device.delete_success'), loadDevicesData);
}

let deviceListenersBound = false;

function setupMutualExclusion() {
  const workstationSelect = elementCache.get('device-workstation-id');
  const positionSelect = elementCache.get('device-position-id');

  if (workstationSelect) {
    workstationSelect.addEventListener('change', () => {
      if (workstationSelect.value) {
        positionSelect.value = '';
      }
    });
  }

  if (positionSelect) {
    positionSelect.addEventListener('change', () => {
      if (positionSelect.value) {
        workstationSelect.value = '';
      }
    });
  }
}

function setupTemplateAutoFill() {
  const templateSelect = elementCache.get('device-template-id');
  if (!templateSelect) return;

  templateSelect.addEventListener('change', async () => {
    const templateId = templateSelect.value;
    if (!templateId) return;

    try {
      const result = await apiGet(`/api/resources/device-templates/${templateId}`);
      if (result.success && result.data) {
        const tmpl = result.data;
        if (tmpl.device_type) elementCache.setValue('device-type', tmpl.device_type);
        if (tmpl.brand) elementCache.setValue('device-brand', tmpl.brand);
        if (tmpl.model) elementCache.setValue('device-model', tmpl.model);
      }
    } catch (error) {
      console.error('加载模板详情失败:', error);
    }
  });
}

function setupSaveAsTemplateToggle() {
  const checkbox = document.getElementById('device-save-as-template');
  const nameGroup = document.getElementById('device-template-name-group');
  if (!checkbox || !nameGroup) return;

  checkbox.addEventListener('change', () => {
    nameGroup.style.display = checkbox.checked ? '' : 'none';
    if (!checkbox.checked) {
      const nameInput = document.getElementById('device-template-name');
      if (nameInput) nameInput.value = '';
    }
  });
}

function setupSnmpVersionToggle() {
  const snmpVersionSelect = elementCache.get('device-snmp-version');
  if (!snmpVersionSelect || snmpVersionSelect.dataset.bound) return;
  snmpVersionSelect.addEventListener('change', toggleSnmpConfig);
  snmpVersionSelect.dataset.bound = 'true';
}

function setupSnmpButtons() {
  const testBtn = elementCache.get('test-snmp-btn');
  const getInfoBtn = elementCache.get('get-snmp-info-btn');

  if (testBtn && !testBtn.dataset.bound) {
    testBtn.addEventListener('click', () => testSnmpConnection());
    testBtn.dataset.bound = 'true';
  }

  if (getInfoBtn && !getInfoBtn.dataset.bound) {
    getInfoBtn.addEventListener('click', () => getDeviceInfoFromSnmp());
    getInfoBtn.dataset.bound = 'true';
  }
}

function ensureDeviceListeners() {
  if (deviceListenersBound) return;
  setupMutualExclusion();
  setupTemplateAutoFill();
  setupSaveAsTemplateToggle();
  setupSnmpVersionToggle();
  setupSnmpButtons();
  deviceListenersBound = true;
}

function setSnmpFieldValues(device) {
  elementCache.setValue('device-snmp-version', device.snmp_version || 'v2c');
  elementCache.setValue('device-snmp-port', device.snmp_port || 161);
  elementCache.setValue('device-snmp-community', device.snmp_community ? PASSWORD_MASK : '');
  elementCache.setValue('device-snmp-username', device.snmp_username || '');
  elementCache.setValue('device-snmp-auth-protocol', device.snmp_auth_protocol || '');
  elementCache.setValue('device-snmp-auth-password', device.snmp_auth_password ? PASSWORD_MASK : '');
  elementCache.setValue('device-snmp-priv-protocol', device.snmp_priv_protocol || '');
  elementCache.setValue('device-snmp-priv-password', device.snmp_priv_password ? PASSWORD_MASK : '');
}

function resetSnmpFields() {
  SNMP_FORM_FIELDS.forEach(field => {
    if (field === 'device-snmp-version') {
      elementCache.setValue(field, 'v2c');
    } else if (field === 'device-snmp-port') {
      elementCache.setValue(field, 161);
    } else {
      elementCache.setValue(field, '');
    }
  });
}

export async function submitDeviceForm() {
  const id = getElementValue("device-id");
  const name = getElementValue("device-name");
  const deviceType = getElementValue("device-type");
  const brand = getElementValue("device-brand");
  const model = getElementValue("device-model");
  const vendor = getElementValue("device-vendor");
  const location = getElementValue("device-location");
  const serialNumber = getElementValue("device-serial-number");
  const templateId = getElementValue("device-template-id");
  const workstationId = getElementValue("device-workstation-id");
  const positionId = getElementValue("device-position-id");
  const netOutletId = getElementValue("device-net-outlet-id");
  const description = getElementValue("device-description");

  if (!name?.trim()) {
    showToast(t('device.name_required'), "warning");
    return;
  }

  if (workstationId && positionId) {
    showToast(t('device.position_mutual_exclusive'), "warning");
    return;
  }

  const saveAsTemplate = document.getElementById('device-save-as-template')?.checked;
  const templateName = getElementValue('device-template-name');

  const manager = getManager('device');
  const ips = manager ? manager.getIps() : [];

  let networkRegionId = null;
  const processedIps = ips.map(ip => {
    if (ip.network_region_id) networkRegionId = ip.network_region_id;
    return ip;
  });

  const snmpVersion = getElementValue("device-snmp-version") || 'v2c';
  const snmpPort = parseInt(getElementValue("device-snmp-port")) || 161;

  const deviceData = {
    name: name.trim(),
    device_type: deviceType || 'other',
    brand: brand?.trim() || null,
    model: model?.trim() || null,
    vendor: vendor?.trim() || null,
    location: location?.trim() || null,
    serial_number: serialNumber?.trim() || null,
    template_id: templateId || null,
    workstation_id: workstationId || null,
    position_id: positionId || null,
    net_outlet_id: netOutletId || null,
    description: description?.trim() || null,
    save_as_template: saveAsTemplate || false,
    template_name: saveAsTemplate ? (templateName?.trim() || name.trim()) : null,
    snmp_version: snmpVersion,
    snmp_port: snmpPort,
    snmp_community: maskToNull(getElementValue("device-snmp-community")),
    snmp_username: getElementValue("device-snmp-username") || null,
    snmp_auth_protocol: getElementValue("device-snmp-auth-protocol") || null,
    snmp_auth_password: maskToNull(getElementValue("device-snmp-auth-password")),
    snmp_priv_protocol: getElementValue("device-snmp-priv-protocol") || null,
    snmp_priv_password: maskToNull(getElementValue("device-snmp-priv-password")),
    ips: processedIps,
    network_region_id: networkRegionId
  };

  const success = await handleFormSubmit({
    formData: deviceData,
    id,
    baseUrl: "/api/resources/devices",
    successMessage: t('device.save_success'),
    modalId: "device-modal",
    reloadFunction: loadDevicesData
  });

  return success;
}

export async function openDeviceModal(device = null) {
  await openModal("device-modal");

  const title = elementCache.get('device-modal-title');
  const form = elementCache.get('device-form');

  await loadDeviceTemplatesForSelect("device-template-id");
  await loadWorkstationsForSelect("device-workstation-id");
  await loadPositionsForSelect("device-position-id");
  await loadNetOutletsForSelect("device-net-outlet-id");

  ensureDeviceListeners();

  const manager = getManager('device');

  if (device) {
    title.textContent = t('device.edit');
    elementCache.setValue('device-id', device.id);
    elementCache.setValue('device-name', device.name);
    elementCache.setValue('device-type', device.device_type || 'other');
    elementCache.setValue('device-brand', device.brand || "");
    elementCache.setValue('device-model', device.model || "");
    elementCache.setValue('device-vendor', device.vendor || "");
    elementCache.setValue('device-location', device.location || "");
    elementCache.setValue('device-serial-number', device.serial_number || "");
    elementCache.setValue('device-description', device.description || "");

    setSnmpFieldValues(device);

    if (device.template_id) elementCache.setValue('device-template-id', device.template_id);
    if (device.workstation_id) elementCache.setValue('device-workstation-id', device.workstation_id);
    if (device.position_id) elementCache.setValue('device-position-id', device.position_id);
    if (device.net_outlet_id) elementCache.setValue('device-net-outlet-id', device.net_outlet_id);

    if (manager) {
      manager.setExcludeSwitchId(device.id || null);
      await manager.loadIps(device.ips || []);
    }
  } else {
    title.textContent = t('device.add');
    form.reset();
    elementCache.setValue('device-id', '');
    resetSnmpFields();
    const saveAsTemplateCheckbox = document.getElementById('device-save-as-template');
    if (saveAsTemplateCheckbox) saveAsTemplateCheckbox.checked = false;
    const templateNameGroup = document.getElementById('device-template-name-group');
    if (templateNameGroup) templateNameGroup.style.display = 'none';

    if (manager) {
      manager.setExcludeSwitchId(null);
      manager.clear();
      await manager.addIpRow();
    }
  }

  requestAnimationFrame(() => {
    toggleSnmpConfig();
  });
}
