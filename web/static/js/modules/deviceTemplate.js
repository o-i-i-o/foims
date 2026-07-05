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
  escapeHtml,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";

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

export async function loadDeviceTemplatesData() {
  try {
    const result = await apiGet('/api/resources/device-templates');
    const items = result.success && result.data ? (result.data.items || result.data) : [];
    const startIndex = 0;

    renderTable("#device-templates-table", {
      data: items,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'device_type', render: (v) => getDeviceTypeName(v) },
        { field: 'brand', render: (v) => escapeHtml(v) || '-' },
        { field: 'model', render: (v) => escapeHtml(v) || '-' },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('common.no_data')
    });
  } catch (error) {
    handleError(error, t('device_template.load_failed'), () => {
      renderTable("#device-templates-table", { data: [], columns: [], emptyMessage: t('common.load_failed_retry') });
    });
  }
}

export async function editDeviceTemplate(id) {
  try {
    const result = await apiGet(`/api/resources/device-templates/${id}`);
    if (result.success) {
      openDeviceTemplateModal(result.data);
    } else {
      showToast(`${t('device_template.load_failed')}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t('device_template.load_failed'));
  }
}

export async function deleteDeviceTemplate(id) {
  await handleDelete(id, "/api/resources/device-templates", t('device_template.delete_success'), loadDeviceTemplatesData);
}

export async function submitDeviceTemplateForm() {
  const id = getElementValue("device-template-id");
  const name = getElementValue("device-template-name");
  const deviceType = getElementValue("device-template-type");
  const brand = getElementValue("device-template-brand");
  const model = getElementValue("device-template-model");
  const description = getElementValue("device-template-description");

  if (!name?.trim()) {
    showToast(t('device_template.name_required'), "warning");
    return;
  }

  const templateData = {
    name: name.trim(),
    device_type: deviceType || 'other',
    brand: brand?.trim() || null,
    model: model?.trim() || null,
    description: description?.trim() || null,
  };

  const success = await handleFormSubmit({
    formData: templateData,
    id,
    baseUrl: "/api/resources/device-templates",
    successMessage: t('device_template.save_success'),
    modalId: "device-template-modal",
    reloadFunction: loadDeviceTemplatesData
  });

  return success;
}

export async function openDeviceTemplateModal(template = null) {
  openModal("device-template-modal");

  const title = elementCache.get('device-template-modal-title');
  const form = elementCache.get('device-template-form');

  if (template) {
    title.textContent = t('device_template.edit');
    elementCache.setValue('device-template-id', template.id);
    elementCache.setValue('device-template-name', template.name);
    elementCache.setValue('device-template-type', template.device_type || 'other');
    elementCache.setValue('device-template-brand', template.brand || "");
    elementCache.setValue('device-template-model', template.model || "");
    elementCache.setValue('device-template-description', template.description || "");
  } else {
    title.textContent = t('device_template.add');
    form.reset();
    elementCache.setValue('device-template-id', '');
  }
}
