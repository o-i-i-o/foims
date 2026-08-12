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
  // 设备端口与设备接口在前端整合为「设备接口」，列表统一显示
  device_port: t('cable_link.endpoint_device_interface') || '设备接口',
  device_interface: t('cable_link.endpoint_device_interface') || '设备接口',
  patch_panel: t('cable_link.endpoint_patch_panel') || t('net_outlet.type_patch_panel') || '配线架',
};

// 各端点类型对应的级联「范围」选择器配置
const ENDPOINT_SCOPE = {
  net_outlet: { scope: 'room', labelKey: 'cable_link.scope_room' },
  device_interface: { scope: 'device', labelKey: 'cable_link.scope_device' },
  patch_panel: { scope: 'cabinet', labelKey: 'cable_link.scope_cabinet' },
};

// 设备接口（整合）下拉值的前缀，提交时据此还原真实 endpoint_type
const MERGED_TYPE_PREFIX = {
  device_interface: 'device_interface:',
  device_port: 'device_port:',
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

// ==========================================
// 端点类型整合 + 级联范围选择器
// ==========================================
// 各端点类型对应一个「范围」选择器：信息点→房间、设备接口→设备、配线架→机柜
// 「设备接口」为整合类型：下拉中同时包含设备接口与设备端口（optgroup 分组），
// 选项值用前缀编码（device_interface:<id> / device_port:<id>），提交时还原真实类型。

function getSideIds(side) {
  return {
    typeSelect: `cable-link-${side}-type`,
    scopeGroup: `cable-link-${side}-scope-group`,
    scopeLabel: `cable-link-${side}-scope-label`,
    scopeSelect: `cable-link-${side}-scope`,
    idSelect: `cable-link-${side}-id`,
  };
}

function appendOptions(select, items) {
  items.forEach(item => {
    const option = document.createElement('option');
    option.value = item.id;
    option.textContent = item.label;
    select.appendChild(option);
  });
}

async function loadScopeOptions(scopeType, scopeSelectId) {
  const select = elementCache.get(scopeSelectId);
  if (!select) return;

  let placeholder = '';
  let items = [];
  try {
    if (scopeType === 'room') {
      placeholder = t('cable_link.select_room') || '选择房间';
      const result = await apiGet('/api/resources/rooms?page_size=1000');
      const data = result.success ? result.data : {};
      items = (data.items || data || []).map(r => ({ id: r.id, label: r.name }));
    } else if (scopeType === 'device') {
      placeholder = t('cable_link.select_device') || '选择设备';
      const result = await apiGet('/api/resources/devices?page_size=1000');
      const data = result.success ? result.data : {};
      items = (data.items || data || []).map(d => ({ id: d.id, label: d.name || d.id }));
    } else if (scopeType === 'cabinet') {
      placeholder = t('cable_link.select_cabinet') || '选择机柜';
      const result = await apiGet('/api/resources/cabinets?page_size=1000');
      const data = result.success ? result.data : {};
      items = (data.items || data || []).map(c => ({ id: c.id, label: c.name }));
    }
  } catch (e) {
    console.error('加载范围选项失败:', e);
  }

  select.innerHTML = `<option value="">${placeholder}</option>`;
  appendOptions(select, items);
}

// 动态加载端点选项（依据端点类型 + 已选范围）
async function loadEndpointOptions(endpointType, scopeValue, selectId, selectedId = null) {
  const select = elementCache.get(selectId);
  if (!select) return;

  select.innerHTML = `<option value="">${t('cable_link.select_endpoint') || '选择端点'}</option>`;
  if (!scopeValue) return;

  try {
    if (endpointType === 'net_outlet') {
      const result = await apiGet(`/api/resources/net-outlets?room_id=${scopeValue}&page_size=1000`);
      const data = result.success ? result.data : {};
      // 排除配线架（配线架已作为独立端点类型）
      const items = (data.items || data || [])
        .filter(o => o.outlet_type !== 'patch_panel')
        .map(o => ({ id: o.id, label: o.name }));
      appendOptions(select, items);
    } else if (endpointType === 'device_interface') {
      // 整合：并发拉取该设备的接口与端口，optgroup 分组，值前缀编码
      const [ifaceRes, portRes] = await Promise.all([
        apiGet(`/api/resources/devices/${scopeValue}/interfaces?page_size=1000`),
        apiGet(`/api/resources/devices/${scopeValue}/device-ports?page_size=1000`),
      ]);
      const ifaces = (ifaceRes.success ? (ifaceRes.data?.items || ifaceRes.data || []) : [])
        .filter(di => !di.interface_type || ['physical', 'wifi'].includes(di.interface_type))
        .map(di => ({ id: di.id, label: di.name || di.id }));
      const ports = (portRes.success ? (portRes.data?.items || portRes.data || []) : [])
        .map(sp => ({ id: sp.id, label: sp.port_number || sp.port_name || sp.id }));

      if (ifaces.length) {
        const og = document.createElement('optgroup');
        og.label = t('cable_link.endpoint_device_interface') || '设备接口';
        ifaces.forEach(i => {
          const o = document.createElement('option');
          o.value = `${MERGED_TYPE_PREFIX.device_interface}${i.id}`;
          o.textContent = i.label;
          og.appendChild(o);
        });
        select.appendChild(og);
      }
      if (ports.length) {
        const og = document.createElement('optgroup');
        og.label = t('cable_link.endpoint_device_port') || '设备端口';
        ports.forEach(p => {
          const o = document.createElement('option');
          o.value = `${MERGED_TYPE_PREFIX.device_port}${p.id}`;
          o.textContent = p.label;
          og.appendChild(o);
        });
        select.appendChild(og);
      }
    } else if (endpointType === 'patch_panel') {
      const result = await apiGet(`/api/resources/net-outlets?outlet_type=patch_panel&cabinet_id=${scopeValue}&page_size=1000`);
      const data = result.success ? result.data : {};
      const items = (data.items || data || []).map(o => ({ id: o.id, label: o.name }));
      appendOptions(select, items);
    }
  } catch (e) {
    console.error('加载端点选项失败:', e);
  }

  if (selectedId) select.value = selectedId;
}

// 类型切换：更新范围选择器标签/可见性，并加载范围选项
async function onTypeChange(side) {
  const ids = getSideIds(side);
  const type = elementCache.get(ids.typeSelect)?.value;
  const scopeGroup = elementCache.get(ids.scopeGroup);
  const scopeLabel = elementCache.get(ids.scopeLabel);
  const scopeSelect = elementCache.get(ids.scopeSelect);
  const idSelect = elementCache.get(ids.idSelect);

  if (idSelect) idSelect.innerHTML = `<option value="">${t('cable_link.select_endpoint') || '选择端点'}</option>`;
  if (scopeSelect) scopeSelect.value = '';

  const cfg = type ? ENDPOINT_SCOPE[type] : null;
  if (!cfg) {
    if (scopeGroup) scopeGroup.style.display = 'none';
    return;
  }
  if (scopeLabel) scopeLabel.textContent = t(cfg.labelKey) || cfg.scope;
  if (scopeGroup) scopeGroup.style.display = '';
  await loadScopeOptions(cfg.scope, ids.scopeSelect);
}

// 范围切换：依据类型 + 范围加载端点选项
async function onScopeChange(side) {
  const ids = getSideIds(side);
  const type = elementCache.get(ids.typeSelect)?.value;
  const scopeValue = elementCache.get(ids.scopeSelect)?.value;
  await loadEndpointOptions(type, scopeValue, ids.idSelect);
}

// 还原整合类型的真实 endpoint_type 与 id
function decodeEndpoint(type, rawValue) {
  if (!rawValue) return { type: '', id: '' };
  if (type === 'device_interface') {
    const idx = rawValue.indexOf(':');
    if (idx > 0) {
      return { type: rawValue.slice(0, idx), id: rawValue.slice(idx + 1) };
    }
  }
  return { type, id: rawValue };
}

let changeHandlers = {};

export async function openCableLinkModal(cableLink = null) {
  await openModal("cable-link-modal");

  const title = elementCache.get('cable-link-modal-title');
  const form = elementCache.get('cable-link-form');
  const isEdit = !!cableLink;

  const aTypeSelect = elementCache.get('cable-link-a-type');
  const aScopeSelect = elementCache.get('cable-link-a-scope');
  const aIdSelect = elementCache.get('cable-link-a-id');
  const bTypeSelect = elementCache.get('cable-link-b-type');
  const bScopeSelect = elementCache.get('cable-link-b-scope');
  const bIdSelect = elementCache.get('cable-link-b-id');

  // 清理旧事件监听
  ['a-type', 'a-scope', 'b-type', 'b-scope'].forEach(key => {
    const [side, evt] = key.split('-');
    const sel = elementCache.get(`cable-link-${key}`);
    if (sel && changeHandlers[key]) {
      sel.removeEventListener('change', changeHandlers[key]);
    }
    changeHandlers[key] = evt === 'type'
      ? async () => { await onTypeChange(side); }
      : async () => { await onScopeChange(side); };
    if (sel) sel.addEventListener('change', changeHandlers[key]);
  });

  // 编辑时端点不可修改
  const endpointReadonly = isEdit;
  [aTypeSelect, aScopeSelect, aIdSelect, bTypeSelect, bScopeSelect, bIdSelect].forEach(sel => {
    if (sel) sel.disabled = endpointReadonly;
  });
  // 编辑模式下隐藏范围选择器（端点只读，无需级联）
  ['cable-link-a-scope-group', 'cable-link-b-scope-group'].forEach(gid => {
    const g = elementCache.get(gid);
    if (g) g.style.display = isEdit ? 'none' : '';
  });

  if (isEdit) {
    title.textContent = t('cable_link.edit');
    elementCache.setValue('cable-link-id', cableLink.id);
    // device_port / device_interface 统一映射为「设备接口」选项作展示
    const aDisplay = (cableLink.a_endpoint_type === 'device_port' || cableLink.a_endpoint_type === 'device_interface') ? 'device_interface' : cableLink.a_endpoint_type;
    const bDisplay = (cableLink.b_endpoint_type === 'device_port' || cableLink.b_endpoint_type === 'device_interface') ? 'device_interface' : cableLink.b_endpoint_type;
    elementCache.setValue('cable-link-a-type', aDisplay);
    elementCache.setValue('cable-link-b-type', bDisplay);
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
    // 默认类型为信息点：初始化范围选择器（房间）
    await onTypeChange('a');
    await onTypeChange('b');
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
  const aRawId = getElementValue("cable-link-a-id");
  const bType = getElementValue("cable-link-b-type");
  const bRawId = getElementValue("cable-link-b-id");

  const a = decodeEndpoint(aType, aRawId);
  const b = decodeEndpoint(bType, bRawId);

  if (!a.type || !a.id || !b.type || !b.id) {
    showToast(t('cable_link.endpoint_required') || "请选择两端端点", "warning");
    return;
  }
  if (a.type === b.type && a.id === b.id) {
    showToast(t('cable_link.no_self_link') || "不允许自连接", "warning");
    return;
  }

  const payload = {
    a_endpoint_type: a.type,
    a_endpoint_id: a.id,
    b_endpoint_type: b.type,
    b_endpoint_id: b.id,
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
