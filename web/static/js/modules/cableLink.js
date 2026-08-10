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

const tableState = createSortState('updated_at', 'desc');
let currentPage = 1;

const ENDPOINT_TYPE_LABELS = {
  net_outlet: t('cable_link.endpoint_net_outlet') || t('net_outlet.name') || '信息点',
  switch_port: t('cable_link.endpoint_switch_port') || '交换机接口',
  device_interface: t('cable_link.endpoint_device_interface') || '设备接口',
};

const LINK_TYPE_LABELS = {
  ethernet: t('cable_link.link_type_ethernet') || '网线',
  fiber: t('cable_link.link_type_fiber') || '光纤',
  console: t('cable_link.link_type_console') || 'Console',
};

function getEndpointTypeLabel(type) {
  return ENDPOINT_TYPE_LABELS[type] || type;
}

function getLinkTypeLabel(type) {
  return LINK_TYPE_LABELS[type] || type;
}

function buildEndpointDisplay(type, label) {
  const typeLabel = getEndpointTypeLabel(type);
  const lbl = label ? escapeHtml(label) : '-';
  return `${typeLabel}：<br><small>${lbl}</small>`;
}

export async function loadCableLinksData(page = 1, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);

  try {
    const result = await apiGet(`/api/resources/cable-links?page=${page}&page_size=${DEFAULT_PAGE_SIZE}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const items = data.items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#cable-links-table", {
      data: items,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'a_endpoint_type', render: (v, row) => buildEndpointDisplay(row.a_endpoint_type, row.a_endpoint_label) },
        { field: 'b_endpoint_type', render: (v, row) => buildEndpointDisplay(row.b_endpoint_type, row.b_endpoint_label) },
        { field: 'link_type', render: (v) => getLinkTypeLabel(v) },
        { field: 'cable_label', render: (v) => escapeHtml(v) || '-' },
        { field: 'length_m', render: (v) => (v != null ? `${v}m` : '-') },
        { field: 'tested', render: (v) => v ? (t('cable_link.tested_yes') || '已测') : (t('cable_link.tested_no') || '未测') },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('common.no_data')
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#cable-links-table", data, loadCableLinksData);
    }
    updateSortIcons("cable-links-table", tableState);
  } catch (error) {
    handleError(error, t('cable_link.load_failed'), () => {
      renderTable("#cable-links-table", { data: [], columns: [], emptyMessage: t('common.load_failed_retry') });
    });
  }
}

export function initCableLinkSortEvents() {
  initSortEvents("cable-links-table", tableState, loadCableLinksData);
}

export async function editCableLink(id) {
  try {
    const result = await apiGet(`/api/resources/cable-links/${id}`);
    if (result.success) {
      openCableLinkModal(result.data);
    } else {
      showToast(`${t('cable_link.load_failed')}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t('cable_link.load_failed'));
  }
}

export async function deleteCableLink(id) {
  await handleDelete(id, "/api/resources/cable-links", t('cable_link.delete_success'), loadCableLinksData);
}

// 动态加载各类端点选项
async function loadEndpointOptions(endpointType, selectId, selectedId = null) {
  const select = elementCache.get(selectId);
  if (!select) return;

  select.innerHTML = `<option value="">${t('cable_link.select_endpoint') || '选择端点'}</option>`;

  let items = [];
  try {
    if (endpointType === 'net_outlet') {
      const result = await apiGet('/api/resources/net-outlets?page_size=1000');
      const data = result.success ? result.data : {};
      items = (data.items || data || []).map(o => ({ id: o.id, label: o.name }));
    } else if (endpointType === 'switch_port') {
      const result = await apiGet('/api/resources/devices/switch-ports?page_size=1000');
      const data = result.success ? result.data : {};
      items = (data.items || data || []).map(sp => ({
        id: sp.id,
        label: `${sp.port_number || sp.port_name || sp.id}${sp.device_name ? ' @ ' + sp.device_name : ''}`,
      }));
    } else if (endpointType === 'device_interface') {
      const result = await apiGet('/api/resources/devices/interfaces?page_size=1000');
      const data = result.success ? result.data : {};
      items = (data.items || data || []).map(di => ({
        id: di.id,
        label: `${di.name || di.id}${di.device_name ? ' @ ' + di.device_name : ''}`,
      }));
    }
  } catch (e) {
    console.error('加载端点选项失败:', e);
  }

  items.forEach(item => {
    const option = document.createElement("option");
    option.value = item.id;
    option.textContent = item.label;
    select.appendChild(option);
  });

  if (selectedId) select.value = selectedId;
}

let aTypeChangeHandler = null;
let bTypeChangeHandler = null;

export async function openCableLinkModal(cableLink = null) {
  await openModal("cable-link-modal");

  const title = elementCache.get('cable-link-modal-title');
  const form = elementCache.get('cable-link-form');
  const isEdit = !!cableLink;

  const aTypeSelect = elementCache.get('cable-link-a-type');
  const aIdSelect = elementCache.get('cable-link-a-id');
  const bTypeSelect = elementCache.get('cable-link-b-type');
  const bIdSelect = elementCache.get('cable-link-b-id');

  // 编辑时端点不可修改
  const endpointReadonly = isEdit;
  [aTypeSelect, aIdSelect, bTypeSelect, bIdSelect].forEach(sel => {
    if (sel) sel.disabled = endpointReadonly;
  });

  // 清理旧事件监听
  if (aTypeSelect && aTypeChangeHandler) {
    aTypeSelect.removeEventListener('change', aTypeChangeHandler);
  }
  if (bTypeSelect && bTypeChangeHandler) {
    bTypeSelect.removeEventListener('change', bTypeChangeHandler);
  }

  aTypeChangeHandler = async () => {
    const type = aTypeSelect?.value;
    if (type) await loadEndpointOptions(type, 'cable-link-a-id');
  };
  bTypeChangeHandler = async () => {
    const type = bTypeSelect?.value;
    if (type) await loadEndpointOptions(type, 'cable-link-b-id');
  };

  if (aTypeSelect) aTypeSelect.addEventListener('change', aTypeChangeHandler);
  if (bTypeSelect) bTypeSelect.addEventListener('change', bTypeChangeHandler);

  if (isEdit) {
    title.textContent = t('cable_link.edit');
    elementCache.setValue('cable-link-id', cableLink.id);
    elementCache.setValue('cable-link-a-type', cableLink.a_endpoint_type);
    elementCache.setValue('cable-link-b-type', cableLink.b_endpoint_type);
    elementCache.setValue('cable-link-link-type', cableLink.link_type || 'ethernet');
    elementCache.setValue('cable-link-cable-label', cableLink.cable_label || '');
    elementCache.setValue('cable-link-length', cableLink.length_m != null ? cableLink.length_m : '');
    elementCache.setValue('cable-link-tested', cableLink.tested ? '1' : '0');
    // 显示端点标签（只读）
    if (aIdSelect) {
      aIdSelect.innerHTML = `<option value="${escapeHtml(cableLink.a_endpoint_id)}">${escapeHtml(cableLink.a_endpoint_label || cableLink.a_endpoint_id)}</option>`;
      aIdSelect.value = cableLink.a_endpoint_id;
    }
    if (bIdSelect) {
      bIdSelect.innerHTML = `<option value="${escapeHtml(cableLink.b_endpoint_id)}">${escapeHtml(cableLink.b_endpoint_label || cableLink.b_endpoint_id)}</option>`;
      bIdSelect.value = cableLink.b_endpoint_id;
    }
  } else {
    title.textContent = t('cable_link.add');
    if (form) form.reset();
    elementCache.setValue('cable-link-id', '');
    elementCache.setValue('cable-link-link-type', 'ethernet');
    elementCache.setValue('cable-link-tested', '0');
    // 默认加载信息点选项
    await loadEndpointOptions('net_outlet', 'cable-link-a-id');
    await loadEndpointOptions('net_outlet', 'cable-link-b-id');
  }
}

export async function submitCableLinkForm() {
  const id = getElementValue("cable-link-id");
  const linkType = getElementValue("cable-link-link-type") || 'ethernet';
  const cableLabel = getElementValue("cable-link-cable-label");
  const lengthStr = getElementValue("cable-link-length");
  const testedVal = getElementValue("cable-link-tested");
  const tested = testedVal === '1' || testedVal === 'true';

  if (id) {
    // 编辑：仅更新元数据
    const payload = {
      link_type: linkType,
      cable_label: cableLabel || null,
      length_m: lengthStr ? parseFloat(lengthStr) : null,
      tested,
    };
    try {
      const result = await apiPut(`/api/resources/cable-links/${id}`, payload);
      if (result.success) {
        showToast(t('cable_link.save_success') || "线路保存成功", "success");
        closeModal("cable-link-modal");
        await loadCableLinksData();
      } else {
        showToast(result.message || (t('cable_link.save_failed') || "线路保存失败"), "error");
      }
    } catch (error) {
      handleError(error, t('cable_link.save_failed') || "线路保存失败");
    }
    return;
  }

  // 新建：需要两端点
  const aType = getElementValue("cable-link-a-type");
  const aId = getElementValue("cable-link-a-id");
  const bType = getElementValue("cable-link-b-type");
  const bId = getElementValue("cable-link-b-id");

  if (!aType || !aId || !bType || !bId) {
    showToast(t('cable_link.endpoint_required') || "请选择两端端点", "warning");
    return;
  }
  if (aType === bType && aId === bId) {
    showToast(t('cable_link.no_self_link') || "不允许自连接", "warning");
    return;
  }

  const payload = {
    a_endpoint_type: aType,
    a_endpoint_id: aId,
    b_endpoint_type: bType,
    b_endpoint_id: bId,
    link_type: linkType,
    cable_label: cableLabel || null,
    length_m: lengthStr ? parseFloat(lengthStr) : null,
    tested,
  };

  try {
    const result = await apiPost("/api/resources/cable-links", payload);
    if (result.success) {
      showToast(t('cable_link.save_success') || "线路创建成功", "success");
      closeModal("cable-link-modal");
      await loadCableLinksData();
    } else {
      showToast(result.message || (t('cable_link.save_failed') || "线路创建失败"), "error");
    }
  } catch (error) {
    handleError(error, t('cable_link.save_failed') || "线路创建失败");
  }
}
