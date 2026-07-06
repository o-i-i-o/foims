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
  loadAccessPointsForSelect,
  loadDeviceTemplatesForSelect,
  loadWorkstationsForSelect,
  loadPositionsForSelect,
} from "../utils/resources.js";

const tableState = createSortState('name', 'asc');
let currentPage = 1;

const DEVICE_TYPE_LABELS = {
  pc: t('device_type.pc') || 'PC',
  laptop: t('device_type.laptop') || '笔记本',
  printer: t('device_type.printer') || '打印机',
  server: t('device_type.server') || '服务器',
  network_device: t('device_type.network_device') || '网络设备',
  camera: t('device_type.camera') || '摄像头',
  phone: t('device_type.phone') || '电话',
  ap: t('device_type.ap') || '接入点',
  other: t('device_type.other') || '其他',
};

function getDeviceTypeName(type) {
  return DEVICE_TYPE_LABELS[type] || type;
}

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
        { field: 'access_point_name', render: (v, row) => {
          if (v) return `${t('device.access_point')}: ${escapeHtml(v)}`;
          if (row.connected_switch_port && row.connected_switch_name) return `${t('device.switch_port')}: ${escapeHtml(row.connected_switch_name)}:${escapeHtml(row.connected_switch_port)}`;
          return '-';
        }},
        { field: 'room_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('common.no_data')
    });

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

async function loadSwitchPortsForDeviceSelect(selectedPortId = null) {
  const portSelect = elementCache.get('device-switch-port-id');
  if (!portSelect) return;

  portSelect.innerHTML = `<option value="">${t('device.select_switch_port') || '选择交换机端口'}</option>`;

  try {
    const result = await apiGet('/api/switches/ports?page_size=1000');
    if (result.success && result.data) {
      const ports = result.data.items || result.data;
      ports.forEach(port => {
        const option = document.createElement('option');
        option.value = port.id;
        const label = port.switch_name ? `${port.switch_name}: ${port.name || port.port_number}` : (port.name || port.port_number);
        option.textContent = label;
        portSelect.appendChild(option);
      });

      if (selectedPortId) {
        portSelect.value = selectedPortId;
      }
    }
  } catch (error) {
    console.error('加载交换机端口选项失败:', error);
  }
}

let deviceListenersBound = false;

function setupMutualExclusion() {
  const workstationSelect = elementCache.get('device-workstation-id');
  const positionSelect = elementCache.get('device-position-id');
  const apSelect = elementCache.get('device-access-point-id');
  const portSelect = elementCache.get('device-switch-port-id');

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

  if (apSelect) {
    apSelect.addEventListener('change', () => {
      if (apSelect.value) {
        portSelect.value = '';
      }
    });
  }

  if (portSelect) {
    portSelect.addEventListener('change', () => {
      if (portSelect.value) {
        apSelect.value = '';
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

function ensureDeviceListeners() {
  if (deviceListenersBound) return;
  setupMutualExclusion();
  setupTemplateAutoFill();
  setupSaveAsTemplateToggle();
  deviceListenersBound = true;
}

export async function submitDeviceForm() {
  const id = getElementValue("device-id");
  const name = getElementValue("device-name");
  const deviceType = getElementValue("device-type");
  const brand = getElementValue("device-brand");
  const model = getElementValue("device-model");
  const serialNumber = getElementValue("device-serial-number");
  const templateId = getElementValue("device-template-id");
  const workstationId = getElementValue("device-workstation-id");
  const positionId = getElementValue("device-position-id");
  const accessPointId = getElementValue("device-access-point-id");
  const switchPortId = getElementValue("device-switch-port-id");
  const description = getElementValue("device-description");

  if (!name?.trim()) {
    showToast(t('device.name_required'), "warning");
    return;
  }

  if (workstationId && positionId) {
    showToast(t('device.position_mutual_exclusive'), "warning");
    return;
  }

  if (accessPointId && switchPortId) {
    showToast(t('device.connection_mutual_exclusive'), "warning");
    return;
  }

  const saveAsTemplate = document.getElementById('device-save-as-template')?.checked;
  const templateName = getElementValue('device-template-name');

  const deviceData = {
    name: name.trim(),
    device_type: deviceType || 'other',
    brand: brand?.trim() || null,
    model: model?.trim() || null,
    serial_number: serialNumber?.trim() || null,
    template_id: templateId || null,
    workstation_id: workstationId || null,
    position_id: positionId || null,
    access_point_id: accessPointId || null,
    switch_port_id: switchPortId || null,
    description: description?.trim() || null,
    save_as_template: saveAsTemplate || false,
    template_name: saveAsTemplate ? (templateName?.trim() || name.trim()) : null,
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
  openModal("device-modal");

  const title = elementCache.get('device-modal-title');
  const form = elementCache.get('device-form');

  await loadDeviceTemplatesForSelect("device-template-id");
  await loadWorkstationsForSelect("device-workstation-id");
  await loadPositionsForSelect("device-position-id");
  await loadAccessPointsForSelect("device-access-point-id");
  await loadSwitchPortsForDeviceSelect();

  ensureDeviceListeners();

  if (device) {
    title.textContent = t('device.edit');
    elementCache.setValue('device-id', device.id);
    elementCache.setValue('device-name', device.name);
    elementCache.setValue('device-type', device.device_type || 'other');
    elementCache.setValue('device-brand', device.brand || "");
    elementCache.setValue('device-model', device.model || "");
    elementCache.setValue('device-serial-number', device.serial_number || "");
    elementCache.setValue('device-description', device.description || "");

    if (device.template_id) elementCache.setValue('device-template-id', device.template_id);
    if (device.workstation_id) elementCache.setValue('device-workstation-id', device.workstation_id);
    if (device.position_id) elementCache.setValue('device-position-id', device.position_id);
    if (device.access_point_id) elementCache.setValue('device-access-point-id', device.access_point_id);
    if (device.switch_port_id) elementCache.setValue('device-switch-port-id', device.switch_port_id);
  } else {
    title.textContent = t('device.add');
    form.reset();
    elementCache.setValue('device-id', '');
    const saveAsTemplateCheckbox = document.getElementById('device-save-as-template');
    if (saveAsTemplateCheckbox) saveAsTemplateCheckbox.checked = false;
    const templateNameGroup = document.getElementById('device-template-name-group');
    if (templateNameGroup) templateNameGroup.style.display = 'none';
  }
}
