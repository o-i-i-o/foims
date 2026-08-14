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
import { iconButton } from "../utils/icons.js";
import { elementCache } from "../utils/helpers.js";
import {
  loadRoomsForSelect,
  loadDeviceTemplatesForSelect,
  loadWorkstationsForSelect,
  loadPositionsForSelect,
} from "../utils/resources.js";
import { loadModule } from "../utils/resourceLoader.js";
import {
  toggleSnmpConfig,
  testSnmpConnection,
  getDeviceInfoFromSnmp,
} from "./deviceSnmp.js";
import { manageUnifiedDevicePorts } from "./unifiedDevicePorts.js";
import { viewArpTable, viewLldpNeighbors } from "./deviceMacLldp.js";

const tableState = createSortState('name', 'asc');
let currentPage = 1;
let currentPageSize = DEFAULT_PAGE_SIZE;

const DEVICE_TYPE_LABELS = {
  pc: t('device_type.pc'),
  laptop: t('device_type.laptop'),
  printer: t('device_type.printer'),
  server: t('device_type.server'),
  network_device: t('device_type.network_device'),
  switch: t('device_type.switch'),
  camera: t('device_type.camera'),
  phone: t('device_type.phone'),
  other: t('device_type.other'),
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

export async function loadDevicesData(page = currentPage, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);

  try {
    const result = await apiGet(`/api/resources/devices?page=${page}&page_size=${currentPageSize}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const devices = data.items || data;
    const startIndex = (page - 1) * currentPageSize;

    renderTable("#devices-table", {
      data: devices,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'device_type', render: (v) => getDeviceTypeName(v), className: 'col-center' },
        { field: 'brand', render: (v) => escapeHtml(v) || '-' },
        { field: 'model', render: (v) => escapeHtml(v) || '-' },
        { field: 'workstation_name', render: (v, row) => {
          if (v) return `${t('device.workstation')}: ${escapeHtml(v)}`;
          if (row.cabinet_name) return `${t('device.position')}: ${escapeHtml(row.cabinet_name)}${row.start_u ? ` ${row.start_u}-${row.end_u}U` : ''}`;
          return '-';
        }},
        { field: 'room_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'id', render: (v, row) => `
          ${iconButton({ icon: 'edit', label: t('common.edit'), cls: 'btn-edit', attrs: `data-id="${v}"` })}
          ${iconButton({ icon: 'list', label: t('device.ports'), cls: 'btn-primary btn-device-ports', attrs: `data-device-id="${v}" data-device-name="${escapeHtml(row.name)}"` })}
          ${iconButton({ icon: 'list', label: t('device.mac_table'), cls: 'btn-success btn-device-mac', attrs: `data-device-id="${v}"` })}
          ${iconButton({ icon: 'list', label: t('device.lldp'), cls: 'btn-warning btn-device-lldp', attrs: `data-device-id="${v}"` })}
          ${iconButton({ icon: 'trash', label: t('common.delete'), cls: 'btn-delete', attrs: `data-id="${v}"` })}
        ` }
      ],
      emptyMessage: t('common.no_data')
    });

    bindDeviceButtonsEvents();

    if (data.total !== undefined) {
      appendPaginationToTable("#devices-table", data, loadDevicesData, {
        pageSize: currentPageSize,
        onPageSizeChange: (size) => { currentPageSize = size; loadDevicesData(1); },
      });
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
    const portsBtn = e.target.closest(".btn-device-ports");
    const macBtn = e.target.closest(".btn-device-mac");
    const lldpBtn = e.target.closest(".btn-device-lldp");

    if (portsBtn) {
      const deviceId = portsBtn.dataset.deviceId;
      const deviceName = portsBtn.dataset.deviceName;
      manageUnifiedDevicePorts(deviceId, deviceName || "");
    } else if (macBtn) {
      viewArpTable(macBtn.dataset.deviceId);
    } else if (lldpBtn) {
      viewLldpNeighbors(lldpBtn.dataset.deviceId);
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

function setupTemplateManageBtn() {
  const btn = document.getElementById('device-template-manage-btn');
  if (!btn || btn.dataset.bound) return;
  btn.dataset.bound = 'true';
  btn.addEventListener('click', () => openDeviceTemplateModal());
}

const DEVICE_TYPE_OPTIONS = [
  { value: 'pc', label: () => t('device_type.pc') },
  { value: 'laptop', label: () => t('device_type.laptop') },
  { value: 'printer', label: () => t('device_type.printer') },
  { value: 'server', label: () => t('device_type.server') },
  { value: 'network_device', label: () => t('device_type.network_device') },
  { value: 'switch', label: () => t('device_type.switch') },
  { value: 'camera', label: () => t('device_type.camera') },
  { value: 'phone', label: () => t('device_type.phone') },
  { value: 'other', label: () => t('device_type.other') },
];

async function openDeviceTemplateModal() {
  const { default: modalLoader } = await import('../utils/modalLoader.js');
  const modal = await modalLoader.loadModal('device-template-modal');
  if (!modal) return;

  modal.classList.add('active');
  document.body.style.overflow = 'hidden';

  await loadDeviceTemplateList();
}

async function loadDeviceTemplateList() {
  const listEl = document.getElementById('device-template-list');
  if (!listEl) return;

  try {
    const result = await apiGet('/api/resources/device-templates');
    if (!result.success || !result.data?.items) {
      listEl.innerHTML = `<p class="empty-hint" data-i18n="device_template.empty">${t('device_template.empty')}</p>`;
      return;
    }

    const templates = result.data.items;
    if (templates.length === 0) {
      listEl.innerHTML = `<p class="empty-hint" data-i18n="device_template.empty">${t('device_template.empty')}</p>`;
      return;
    }

    listEl.innerHTML = templates.map(tmpl => `
      <div class="device-template-item" data-id="${escapeHtml(tmpl.id)}">
        <div class="device-template-info">
          <span class="device-template-name">${escapeHtml(tmpl.name)}</span>
          <span class="device-template-meta">
            <span class="device-template-type">${escapeHtml(DEVICE_TYPE_LABELS[tmpl.device_type] || tmpl.device_type)}</span>
            ${tmpl.brand ? `<span class="device-template-brand">${escapeHtml(tmpl.brand)}</span>` : ''}
            ${tmpl.model ? `<span class="device-template-model">${escapeHtml(tmpl.model)}</span>` : ''}
          </span>
        </div>
        <div class="device-template-actions">
          ${iconButton({ icon: 'edit', label: t('common.edit'), cls: 'btn-secondary dt-edit-btn' })}
          ${iconButton({ icon: 'trash', label: t('common.delete'), cls: 'btn-danger dt-delete-btn' })}
        </div>
      </div>
    `).join('');

    listEl.querySelectorAll('.dt-edit-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const item = btn.closest('.device-template-item');
        const id = item?.dataset.id;
        if (id) await editDeviceTemplate(id);
      });
    });

    listEl.querySelectorAll('.dt-delete-btn').forEach(btn => {
      btn.addEventListener('click', async () => {
        const item = btn.closest('.device-template-item');
        const id = item?.dataset.id;
        const name = item?.querySelector('.device-template-name')?.textContent;
        if (id) await deleteDeviceTemplate(id, name);
      });
    });
  } catch (error) {
    listEl.innerHTML = `<p class="empty-hint">${t('common.load_failed')}</p>`;
    handleError(error, t('common.load_failed'));
  }
}

async function editDeviceTemplate(id) {
  try {
    const result = await apiGet(`/api/resources/device-templates/${id}`);
    if (!result.success || !result.data) return;

    const tmpl = result.data;
    const listEl = document.getElementById('device-template-list');
    const itemEl = listEl?.querySelector(`.device-template-item[data-id="${id}"]`);
    if (!itemEl) return;

    const typeOptions = DEVICE_TYPE_OPTIONS.map(opt =>
      `<option value="${opt.value}" ${opt.value === tmpl.device_type ? 'selected' : ''}>${opt.label()}</option>`
    ).join('');

    itemEl.innerHTML = `
      <div class="device-template-edit-form" data-id="${escapeHtml(id)}">
        <div class="form-row">
          <div class="form-group">
            <label data-i18n="common.name">${t('common.name')}<span class="required">*</span></label>
            <input type="text" class="dt-edit-name nc-input" value="${escapeHtml(tmpl.name)}" />
          </div>
          <div class="form-group">
            <label data-i18n="device.device_type">${t('device.device_type')}</label>
            <select class="dt-edit-type nc-input">${typeOptions}</select>
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label data-i18n="device.brand">${t('device.brand')}</label>
            <input type="text" class="dt-edit-brand nc-input" value="${escapeHtml(tmpl.brand || '')}" />
          </div>
          <div class="form-group">
            <label data-i18n="device.model">${t('device.model')}</label>
            <input type="text" class="dt-edit-model nc-input" value="${escapeHtml(tmpl.model || '')}" />
          </div>
        </div>
        <div class="form-group">
          <label data-i18n="common.description">${t('common.description')}</label>
          <textarea class="dt-edit-desc nc-input" rows="2">${escapeHtml(tmpl.description || '')}</textarea>
        </div>
        <div class="device-template-edit-actions">
          ${iconButton({ icon: 'x', label: t('common.cancel'), cls: 'btn-secondary dt-cancel-btn' })}
          ${iconButton({ icon: 'check', label: t('common.save'), cls: 'btn-primary dt-save-btn' })}
        </div>
      </div>
    `;

    itemEl.querySelector('.dt-cancel-btn')?.addEventListener('click', () => loadDeviceTemplateList());
    itemEl.querySelector('.dt-save-btn')?.addEventListener('click', async () => {
      const name = itemEl.querySelector('.dt-edit-name')?.value?.trim();
      if (!name) {
        showToast(t('device_template.name_required'), 'warning');
        return;
      }
      const data = {
        name,
        device_type: itemEl.querySelector('.dt-edit-type')?.value || 'other',
        brand: itemEl.querySelector('.dt-edit-brand')?.value?.trim() || null,
        model: itemEl.querySelector('.dt-edit-model')?.value?.trim() || null,
        description: itemEl.querySelector('.dt-edit-desc')?.value?.trim() || null,
      };
      try {
        const updateResult = await apiPut(`/api/resources/device-templates/${id}`, data);
        if (updateResult.success) {
          showToast(t('device_template.update_success'), 'success');
          await loadDeviceTemplateList();
          await loadDeviceTemplatesForSelect('device-template-id');
        } else {
          showToast(updateResult.message || t('common.failed'), 'error');
        }
      } catch (error) {
        handleError(error, t('common.operation_failed'));
      }
    });
  } catch (error) {
    handleError(error, t('common.load_failed'));
  }
}

async function deleteDeviceTemplate(id, name) {
  const confirmed = await import('../utils/confirm.js').then(m => m.default(
    t('common.confirm_delete', { name: name || '' })
  ));
  if (!confirmed) return;

  try {
    const result = await apiDelete(`/api/resources/device-templates/${id}`);
    if (result.success) {
      showToast(t('device_template.delete_success'), 'success');
      await loadDeviceTemplateList();
      await loadDeviceTemplatesForSelect('device-template-id');
    } else {
      showToast(result.message || t('common.failed'), 'error');
    }
  } catch (error) {
    handleError(error, t('common.operation_failed'));
  }
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

function setupRoomCascade() {
  const roomSelect = elementCache.get('device-room-id');
  if (!roomSelect || roomSelect.dataset.bound) return;
  roomSelect.addEventListener('change', async () => {
    const roomId = roomSelect.value || null;
    await loadWorkstationsForSelect('device-workstation-id', roomId);
    await loadPositionsForSelect('device-position-id', null, roomId);
  });
  roomSelect.dataset.bound = 'true';
}

function ensureDeviceListeners() {
  if (deviceListenersBound) return;
  setupMutualExclusion();
  setupTemplateAutoFill();
  setupSaveAsTemplateToggle();
  setupTemplateManageBtn();
  setupSnmpVersionToggle();
  setupSnmpButtons();
  setupRoomCascade();
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
  const roomId = getElementValue("device-room-id");
  const workstationId = getElementValue("device-workstation-id");
  const positionId = getElementValue("device-position-id");
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

  const { getNetworkCardManager } = await loadModule('networkCardManager');
  const cardManager = getNetworkCardManager();
  const { cards, errors: cardErrors } = cardManager.collectData();
  if (cardErrors.length > 0) {
    showToast(cardErrors.join('\n'), "warning");
    return;
  }

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
    room_id: roomId || null,
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
    cards,
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
  await loadRoomsForSelect("device-room-id");
  await loadWorkstationsForSelect("device-workstation-id");
  await loadPositionsForSelect("device-position-id");

  ensureDeviceListeners();

  const { getNetworkCardManager } = await loadModule('networkCardManager');
  const cardManager = getNetworkCardManager();

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
    if (device.room_id) {
      elementCache.setValue('device-room-id', device.room_id);
      await loadWorkstationsForSelect('device-workstation-id', device.room_id);
      await loadPositionsForSelect('device-position-id', null, device.room_id);
    }
    if (device.workstation_id) elementCache.setValue('device-workstation-id', device.workstation_id);
    if (device.position_id) elementCache.setValue('device-position-id', device.position_id);

    if (cardManager) {
      cardManager.setExcludeSwitchId(device.id || null);
      await cardManager.loadExisting(device.cards || []);
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

    if (cardManager) {
      cardManager.setExcludeSwitchId(null);
      await cardManager.init();
    }
  }

  requestAnimationFrame(() => {
    toggleSnmpConfig();
  });
}
