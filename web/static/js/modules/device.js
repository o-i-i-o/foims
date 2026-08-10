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
      manageUnifiedDevicePorts(deviceId, deviceName || "");
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

function setupTemplateManageBtn() {
  const btn = document.getElementById('device-template-manage-btn');
  if (!btn || btn.dataset.bound) return;
  btn.dataset.bound = 'true';
  btn.addEventListener('click', () => openDeviceTemplateModal());
}

const DEVICE_TYPE_OPTIONS = [
  { value: 'pc', label: () => t('device_type.pc') || 'PC' },
  { value: 'laptop', label: () => t('device_type.laptop') || '笔记本' },
  { value: 'printer', label: () => t('device_type.printer') || '打印机' },
  { value: 'server', label: () => t('device_type.server') || '服务器' },
  { value: 'network_device', label: () => t('device_type.network_device') || '网络设备' },
  { value: 'switch', label: () => t('device_type.switch') || '交换机' },
  { value: 'camera', label: () => t('device_type.camera') || '摄像头' },
  { value: 'phone', label: () => t('device_type.phone') || '电话' },
  { value: 'other', label: () => t('device_type.other') || '其他' },
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
      listEl.innerHTML = `<p class="empty-hint" data-i18n="device_template.empty">${t('device_template.empty') || '暂无模板数据'}</p>`;
      return;
    }

    const templates = result.data.items;
    if (templates.length === 0) {
      listEl.innerHTML = `<p class="empty-hint" data-i18n="device_template.empty">${t('device_template.empty') || '暂无模板数据'}</p>`;
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
          <button type="button" class="btn btn-secondary btn-sm dt-edit-btn" data-i18n="common.edit">${t('common.edit') || '编辑'}</button>
          <button type="button" class="btn btn-danger btn-sm dt-delete-btn" data-i18n="common.delete">${t('common.delete') || '删除'}</button>
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
    listEl.innerHTML = `<p class="empty-hint">${t('common.load_failed') || '加载失败'}</p>`;
    handleError(error, t('common.load_failed') || '加载失败');
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
            <label data-i18n="common.name">${t('common.name') || '名称'}<span class="required">*</span></label>
            <input type="text" class="dt-edit-name nc-input" value="${escapeHtml(tmpl.name)}" />
          </div>
          <div class="form-group">
            <label data-i18n="device.device_type">${t('device.device_type') || '设备类型'}</label>
            <select class="dt-edit-type nc-input">${typeOptions}</select>
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label data-i18n="device.brand">${t('device.brand') || '品牌'}</label>
            <input type="text" class="dt-edit-brand nc-input" value="${escapeHtml(tmpl.brand || '')}" />
          </div>
          <div class="form-group">
            <label data-i18n="device.model">${t('device.model') || '型号'}</label>
            <input type="text" class="dt-edit-model nc-input" value="${escapeHtml(tmpl.model || '')}" />
          </div>
        </div>
        <div class="form-group">
          <label data-i18n="common.description">${t('common.description') || '描述'}</label>
          <textarea class="dt-edit-desc nc-input" rows="2">${escapeHtml(tmpl.description || '')}</textarea>
        </div>
        <div class="device-template-edit-actions">
          <button type="button" class="btn btn-secondary btn-sm dt-cancel-btn">${t('common.cancel') || '取消'}</button>
          <button type="button" class="btn btn-primary btn-sm dt-save-btn">${t('common.save') || '保存'}</button>
        </div>
      </div>
    `;

    itemEl.querySelector('.dt-cancel-btn')?.addEventListener('click', () => loadDeviceTemplateList());
    itemEl.querySelector('.dt-save-btn')?.addEventListener('click', async () => {
      const name = itemEl.querySelector('.dt-edit-name')?.value?.trim();
      if (!name) {
        showToast(t('device_template.name_required') || '模板名称不能为空', 'warning');
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
          showToast(t('device_template.update_success') || '模板更新成功', 'success');
          await loadDeviceTemplateList();
          await loadDeviceTemplatesForSelect('device-template-id');
        } else {
          showToast(updateResult.message || t('common.failed') || '操作失败', 'error');
        }
      } catch (error) {
        handleError(error, t('common.operation_failed') || '操作失败');
      }
    });
  } catch (error) {
    handleError(error, t('common.load_failed') || '加载失败');
  }
}

async function deleteDeviceTemplate(id, name) {
  const confirmed = await import('../utils/confirm.js').then(m => m.default(
    t('common.confirm_delete', { name: name || '' }) || `确定要删除此${name || ''}吗？`
  ));
  if (!confirmed) return;

  try {
    const result = await apiDelete(`/api/resources/device-templates/${id}`);
    if (result.success) {
      showToast(t('device_template.delete_success') || '模板删除成功', 'success');
      await loadDeviceTemplateList();
      await loadDeviceTemplatesForSelect('device-template-id');
    } else {
      showToast(result.message || t('common.failed') || '操作失败', 'error');
    }
  } catch (error) {
    handleError(error, t('common.operation_failed') || '操作失败');
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
