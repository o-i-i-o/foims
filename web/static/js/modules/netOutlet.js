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

import { openModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";
import { loadRoomsForSelect } from "../utils/resources.js";

const tableState = createSortState('name', 'asc');
let currentPage = 1;

const OUTLET_TYPE_LABELS = {
  wall_socket: t('net_outlet.type_wall_socket') || '墙面插座',
  patch_panel: t('net_outlet.type_patch_panel') || '配线架',
  wifi_ap: t('net_outlet.type_wifi_ap') || '无线AP',
  other: t('net_outlet.type_other') || '其他',
};

const PEER_TYPE_LABELS = {
  outlet: t('net_outlet.peer_type_outlet') || '信息点',
  patch_panel: t('net_outlet.peer_type_patch_panel') || '配线架',
  switch_port: t('net_outlet.peer_type_switch_port') || '交换机接口',
};

function getOutletTypeName(type) {
  return OUTLET_TYPE_LABELS[type] || type;
}

function getPeerTypeLabel(type) {
  return PEER_TYPE_LABELS[type] || '-';
}

function buildPeerDisplay(row) {
  if (!row.peer_type) return '-';
  const label = getPeerTypeLabel(row.peer_type);
  let target = '';
  if (row.peer_type === 'outlet' || row.peer_type === 'patch_panel') {
    if (row.peer_room_name || row.peer_outlet_name) {
      target = [row.peer_room_name, row.peer_outlet_name].filter(Boolean).map(escapeHtml).join(' / ');
    }
  } else if (row.peer_type === 'switch_port') {
    if (row.peer_room_name || row.peer_switch_port_label) {
      target = [row.peer_room_name, row.peer_switch_port_label].filter(Boolean).map(escapeHtml).join(' / ');
    }
  }
  return target ? `${escapeHtml(label)}（${target}）` : escapeHtml(label);
}

export async function loadNetOutletsData(page = 1, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);

  try {
    const result = await apiGet(`/api/resources/net-outlets?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const items = data.items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#net-outlets-table", {
      data: items,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'outlet_type', render: (v) => getOutletTypeName(v) },
        { field: 'room_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'cabinet_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'peer_type', render: (v, row) => buildPeerDisplay(row) },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('common.no_data')
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#net-outlets-table", data, loadNetOutletsData);
    }
    updateSortIcons("net-outlets-table", tableState);
  } catch (error) {
    handleError(error, t('net_outlet.load_failed'), () => {
      renderTable("#net-outlets-table", { data: [], columns: [], emptyMessage: t('common.load_failed_retry') });
    });
  }
}

export function initNetOutletSortEvents() {
  initSortEvents("net-outlets-table", tableState, loadNetOutletsData);
}

export async function editNetOutlet(id) {
  try {
    const result = await apiGet(`/api/resources/net-outlets/${id}`);
    if (result.success) {
      openNetOutletModal(result.data);
    } else {
      showToast(`${t('net_outlet.load_failed')}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t('net_outlet.load_failed'));
  }
}

export async function deleteNetOutlet(id) {
  await handleDelete(id, "/api/resources/net-outlets", t('net_outlet.delete_success'), loadNetOutletsData);
}

async function loadCabinetsForRoom(roomId, selectedCabinetId = null) {
  const cabinetSelect = elementCache.get('net-outlet-cabinet-id');
  if (!cabinetSelect) return;

  cabinetSelect.innerHTML = `<option value="">${t('net_outlet.select_cabinet') || '选择机柜'}</option>`;

  if (!roomId) return;

  try {
    const result = await apiGet(`/api/resources/cabinets?room_id=${roomId}&page_size=1000`);
    if (result.success && result.data) {
      const cabinets = result.data.items || result.data;
      cabinets.forEach(cabinet => {
        const option = document.createElement('option');
        option.value = cabinet.id;
        option.textContent = cabinet.name;
        cabinetSelect.appendChild(option);
      });

      if (selectedCabinetId) {
        cabinetSelect.value = selectedCabinetId;
      }
    }
  } catch (error) {
    console.error('加载机柜选项失败:', error);
  }
}

function updatePeerFieldVisibility(peerType) {
  const outletGroup = document.getElementById('net-outlet-peer-outlet-group');
  const switchPortGroup = document.getElementById('net-outlet-peer-switch-port-group');
  if (!outletGroup || !switchPortGroup) return;

  if (peerType === 'outlet' || peerType === 'patch_panel') {
    outletGroup.style.display = '';
    switchPortGroup.style.display = 'none';
  } else if (peerType === 'switch_port') {
    outletGroup.style.display = 'none';
    switchPortGroup.style.display = '';
  } else {
    outletGroup.style.display = 'none';
    switchPortGroup.style.display = 'none';
  }
}

async function loadPeerOutlets(peerRoomId, peerType, selectedPeerOutletId = null) {
  const peerOutletSelect = elementCache.get('net-outlet-peer-outlet-id');
  if (!peerOutletSelect) return;

  const placeholder = t('net_outlet.select_peer_outlet') || '选择对端信息点';
  peerOutletSelect.innerHTML = `<option value="">${placeholder}</option>`;

  if (!peerRoomId || !peerType) return;

  // 根据 peer_type 决定拉取的 outlet_type
  // outlet → 排除 patch_panel（拉取 wall_socket、wifi_ap、other）
  // patch_panel → 仅拉取 patch_panel
  let outletTypeParam = '';
  if (peerType === 'outlet') {
    // 后端不支持"排除"参数，前端过滤更稳妥：拉取该房间全部信息点后过滤
    outletTypeParam = '';
  } else if (peerType === 'patch_panel') {
    outletTypeParam = '&outlet_type=patch_panel';
  } else {
    return;
  }

  try {
    const result = await apiGet(`/api/resources/net-outlets?room_id=${peerRoomId}&page_size=1000${outletTypeParam}`);
    if (result.success && result.data) {
      const outlets = result.data.items || result.data;
      const filtered = peerType === 'outlet'
        ? outlets.filter(o => o.outlet_type !== 'patch_panel')
        : outlets;
      filtered.forEach(outlet => {
        const option = document.createElement('option');
        option.value = outlet.id;
        option.textContent = `${outlet.name}（${getOutletTypeName(outlet.outlet_type)}）`;
        peerOutletSelect.appendChild(option);
      });

      if (selectedPeerOutletId) {
        peerOutletSelect.value = selectedPeerOutletId;
      }
    }
  } catch (error) {
    console.error('加载对端信息点失败:', error);
  }
}

async function loadPeerSwitchPorts(peerRoomId, selectedPeerSwitchPortId = null) {
  const peerSwitchPortSelect = elementCache.get('net-outlet-peer-switch-port-id');
  if (!peerSwitchPortSelect) return;

  const placeholder = t('net_outlet.select_peer_switch_port') || '选择对端交换机接口';
  peerSwitchPortSelect.innerHTML = `<option value="">${placeholder}</option>`;

  if (!peerRoomId) return;

  try {
    const result = await apiGet(`/api/devices/switch-ports?room_id=${peerRoomId}&page_size=1000`);
    if (result.success && result.data) {
      const ports = result.data.items || result.data;
      ports.forEach(port => {
        const option = document.createElement('option');
        option.value = port.id;
        const parts = [port.device_name, port.port_name || port.port_number];
        option.textContent = parts.filter(Boolean).map(escapeHtml).join(' / ');
        peerSwitchPortSelect.appendChild(option);
      });

      if (selectedPeerSwitchPortId) {
        peerSwitchPortSelect.value = selectedPeerSwitchPortId;
      }
    }
  } catch (error) {
    console.error('加载对端交换机接口失败:', error);
  }
}

export async function submitNetOutletForm() {
  const id = getElementValue("net-outlet-id");
  const name = getElementValue("net-outlet-name");
  const outletType = getElementValue("net-outlet-type");
  const roomId = getElementValue("net-outlet-room-id");
  const cabinetId = getElementValue("net-outlet-cabinet-id");
  const description = getElementValue("net-outlet-description");
  const peerType = getElementValue("net-outlet-peer-type");
  const peerRoomId = getElementValue("net-outlet-peer-room-id");
  const peerOutletId = getElementValue("net-outlet-peer-outlet-id");
  const peerSwitchPortId = getElementValue("net-outlet-peer-switch-port-id");

  if (!name?.trim()) {
    showToast(t('net_outlet.name_required'), "warning");
    return;
  }

  if (!roomId) {
    showToast(t('net_outlet.room_required'), "warning");
    return;
  }

  // 校验对端字段
  if (peerType === 'outlet' || peerType === 'patch_panel') {
    if (!peerOutletId) {
      showToast(t('net_outlet.peer_outlet_required'), "warning");
      return;
    }
  } else if (peerType === 'switch_port') {
    if (!peerSwitchPortId) {
      showToast(t('net_outlet.peer_switch_port_required'), "warning");
      return;
    }
  }

  const outletData = {
    name: name.trim(),
    outlet_type: outletType || 'other',
    room_id: roomId,
    cabinet_id: cabinetId || null,
    description: description?.trim() || null,
    peer_type: peerType || null,
    peer_room_id: peerRoomId || null,
    peer_outlet_id: (peerType === 'outlet' || peerType === 'patch_panel') ? (peerOutletId || null) : null,
    peer_switch_port_id: peerType === 'switch_port' ? (peerSwitchPortId || null) : null,
  };

  const success = await handleFormSubmit({
    formData: outletData,
    id,
    baseUrl: "/api/resources/net-outlets",
    successMessage: t('net_outlet.save_success'),
    modalId: "net-outlet-modal",
    reloadFunction: loadNetOutletsData
  });

  return success;
}

let roomChangeHandler = null;
let peerTypeChangeHandler = null;
let peerRoomChangeHandler = null;

export async function openNetOutletModal(netOutlet = null) {
  await openModal("net-outlet-modal");

  const title = elementCache.get('net-outlet-modal-title');
  const form = elementCache.get('net-outlet-form');

  await loadRoomsForSelect("net-outlet-room-id", { onlyOffice: false, includeVisualization: false });
  await loadRoomsForSelect("net-outlet-peer-room-id", { onlyOffice: false, includeVisualization: false });

  const roomSelect = elementCache.get('net-outlet-room-id');
  const cabinetSelect = elementCache.get('net-outlet-cabinet-id');
  const peerTypeSelect = elementCache.get('net-outlet-peer-type');
  const peerRoomSelect = elementCache.get('net-outlet-peer-room-id');

  // 清理旧事件监听
  if (roomSelect && roomChangeHandler) {
    roomSelect.removeEventListener('change', roomChangeHandler);
  }
  if (peerTypeSelect && peerTypeChangeHandler) {
    peerTypeSelect.removeEventListener('change', peerTypeChangeHandler);
  }
  if (peerRoomSelect && peerRoomChangeHandler) {
    peerRoomSelect.removeEventListener('change', peerRoomChangeHandler);
  }

  roomChangeHandler = async () => {
    const roomId = roomSelect?.value;
    await loadCabinetsForRoom(roomId);
  };

  peerTypeChangeHandler = async () => {
    const peerType = peerTypeSelect?.value;
    updatePeerFieldVisibility(peerType);
    const peerRoomId = peerRoomSelect?.value;
    if (peerType === 'outlet' || peerType === 'patch_panel') {
      await loadPeerOutlets(peerRoomId, peerType);
    } else if (peerType === 'switch_port') {
      await loadPeerSwitchPorts(peerRoomId);
    }
  };

  peerRoomChangeHandler = async () => {
    const peerType = peerTypeSelect?.value;
    const peerRoomId = peerRoomSelect?.value;
    if (peerType === 'outlet' || peerType === 'patch_panel') {
      await loadPeerOutlets(peerRoomId, peerType);
    } else if (peerType === 'switch_port') {
      await loadPeerSwitchPorts(peerRoomId);
    }
  };

  if (roomSelect) {
    roomSelect.addEventListener('change', roomChangeHandler);
  }
  if (peerTypeSelect) {
    peerTypeSelect.addEventListener('change', peerTypeChangeHandler);
  }
  if (peerRoomSelect) {
    peerRoomSelect.addEventListener('change', peerRoomChangeHandler);
  }

  if (netOutlet) {
    title.textContent = t('net_outlet.edit');
    elementCache.setValue('net-outlet-id', netOutlet.id);
    elementCache.setValue('net-outlet-name', netOutlet.name);
    elementCache.setValue('net-outlet-type', netOutlet.outlet_type || 'other');
    elementCache.setValue('net-outlet-room-id', netOutlet.room_id);
    elementCache.setValue('net-outlet-description', netOutlet.description || "");
    elementCache.setValue('net-outlet-peer-type', netOutlet.peer_type || "");
    elementCache.setValue('net-outlet-peer-room-id', netOutlet.peer_room_id || "");

    await loadCabinetsForRoom(netOutlet.room_id, netOutlet.cabinet_id);

    const peerType = netOutlet.peer_type;
    updatePeerFieldVisibility(peerType);
    if (peerType === 'outlet' || peerType === 'patch_panel') {
      await loadPeerOutlets(netOutlet.peer_room_id, peerType, netOutlet.peer_outlet_id);
    } else if (peerType === 'switch_port') {
      await loadPeerSwitchPorts(netOutlet.peer_room_id, netOutlet.peer_switch_port_id);
    }
  } else {
    title.textContent = t('net_outlet.add');
    form.reset();
    elementCache.setValue('net-outlet-id', '');
    elementCache.setValue('net-outlet-peer-type', '');
    elementCache.setValue('net-outlet-peer-room-id', '');
    cabinetSelect.innerHTML = `<option value="">${t('net_outlet.select_cabinet') || '选择机柜'}</option>`;
    updatePeerFieldVisibility('');
  }
}
