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
import { loadRoomsForSelect } from "../utils/resources.js";

const tableState = createSortState('name', 'asc');
let currentPage = 1;

const AP_TYPE_LABELS = {
  wall_socket: t('access_point.type_wall_socket') || '墙面插座',
  patch_panel: t('access_point.type_patch_panel') || '配线架',
  wifi_ap: t('access_point.type_wifi_ap') || '无线AP',
  other: t('access_point.type_other') || '其他',
};

function getApTypeName(type) {
  return AP_TYPE_LABELS[type] || type;
}

export async function loadAccessPointsData(page = 1, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);

  try {
    const result = await apiGet(`/api/resources/access-points?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const items = data.items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#access-points-table", {
      data: items,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'ap_type', render: (v) => getApTypeName(v) },
        { field: 'room_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'cabinet_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'peer_access_point_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'connected_switch_port', render: (v, row) => {
          if (v && row.connected_switch_name) return `${escapeHtml(row.connected_switch_name)}: ${escapeHtml(v)}`;
          return escapeHtml(v) || '-';
        }},
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('common.no_data')
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#access-points-table", data, loadAccessPointsData);
    }
    updateSortIcons("access-points-table", tableState);
  } catch (error) {
    handleError(error, t('access_point.load_failed'), () => {
      renderTable("#access-points-table", { data: [], columns: [], emptyMessage: t('common.load_failed_retry') });
    });
  }
}

export function initAccessPointSortEvents() {
  initSortEvents("access-points-table", tableState, loadAccessPointsData);
}

export async function editAccessPoint(id) {
  try {
    const result = await apiGet(`/api/resources/access-points/${id}`);
    if (result.success) {
      openAccessPointModal(result.data);
    } else {
      showToast(`${t('access_point.load_failed')}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t('access_point.load_failed'));
  }
}

export async function deleteAccessPoint(id) {
  await handleDelete(id, "/api/resources/access-points", t('access_point.delete_success'), loadAccessPointsData);
}

async function loadCabinetsForRoom(roomId, selectedCabinetId = null) {
  const cabinetSelect = elementCache.get('access-point-cabinet-id');
  if (!cabinetSelect) return;

  cabinetSelect.innerHTML = `<option value="">${t('access_point.select_cabinet') || '选择机柜'}</option>`;

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

async function loadPeerAccessPoints(currentId = null, selectedPeerId = null) {
  const peerSelect = elementCache.get('access-point-peer-id');
  if (!peerSelect) return;

  peerSelect.innerHTML = `<option value="">${t('access_point.select_peer') || '选择对端接入点'}</option>`;

  try {
    const result = await apiGet('/api/resources/access-points?page_size=1000');
    if (result.success && result.data) {
      const items = result.data.items || result.data;
      items.forEach(ap => {
        if (ap.id === currentId) return;
        const option = document.createElement('option');
        option.value = ap.id;
        option.textContent = ap.name;
        peerSelect.appendChild(option);
      });

      if (selectedPeerId) {
        peerSelect.value = selectedPeerId;
      }
    }
  } catch (error) {
    console.error('加载对端接入点选项失败:', error);
  }
}

async function loadSwitchPortsForSelect(selectedPortId = null) {
  const portSelect = elementCache.get('access-point-switch-port-id');
  if (!portSelect) return;

  portSelect.innerHTML = `<option value="">${t('access_point.select_switch_port') || '选择设备端口'}</option>`;

  try {
    const result = await apiGet('/api/resources/devices/ports?page_size=1000');
    if (result.success && result.data) {
      const ports = result.data.items || result.data;
      ports.forEach(port => {
        const option = document.createElement('option');
        option.value = port.id;
        const label = port.device_name ? `${port.device_name}: ${port.name || port.port_number}` : (port.name || port.port_number);
        option.textContent = label;
        portSelect.appendChild(option);
      });

      if (selectedPortId) {
        portSelect.value = selectedPortId;
      }
    }
  } catch (error) {
    console.error('加载设备端口选项失败:', error);
  }
}

export async function submitAccessPointForm() {
  const id = getElementValue("access-point-id");
  const name = getElementValue("access-point-name");
  const apType = getElementValue("access-point-type");
  const roomId = getElementValue("access-point-room-id");
  const cabinetId = getElementValue("access-point-cabinet-id");
  const peerId = getElementValue("access-point-peer-id");
  const switchPortId = getElementValue("access-point-switch-port-id");
  const description = getElementValue("access-point-description");

  if (!name?.trim()) {
    showToast(t('access_point.name_required'), "warning");
    return;
  }

  if (!roomId) {
    showToast(t('access_point.room_required'), "warning");
    return;
  }

  const apData = {
    name: name.trim(),
    ap_type: apType || 'other',
    room_id: roomId,
    cabinet_id: cabinetId || null,
    peer_access_point_id: peerId || null,
    switch_port_id: switchPortId || null,
    description: description?.trim() || null,
  };

  const success = await handleFormSubmit({
    formData: apData,
    id,
    baseUrl: "/api/resources/access-points",
    successMessage: t('access_point.save_success'),
    modalId: "access-point-modal",
    reloadFunction: loadAccessPointsData
  });

  return success;
}

let roomChangeHandler = null;

export async function openAccessPointModal(accessPoint = null) {
  await openModal("access-point-modal");

  const title = elementCache.get('access-point-modal-title');
  const form = elementCache.get('access-point-form');

  await loadRoomsForSelect("access-point-room-id", { onlyOffice: false, includeVisualization: false });

  const roomSelect = elementCache.get('access-point-room-id');
  const cabinetSelect = elementCache.get('access-point-cabinet-id');

  if (roomSelect && roomChangeHandler) {
    roomSelect.removeEventListener('change', roomChangeHandler);
  }

  roomChangeHandler = async () => {
    const roomId = roomSelect?.value;
    await loadCabinetsForRoom(roomId);
  };

  if (roomSelect) {
    roomSelect.addEventListener('change', roomChangeHandler);
  }

  await loadPeerAccessPoints(accessPoint?.id, accessPoint?.peer_access_point_id);
  await loadSwitchPortsForSelect(accessPoint?.switch_port_id);

  if (accessPoint) {
    title.textContent = t('access_point.edit');
    elementCache.setValue('access-point-id', accessPoint.id);
    elementCache.setValue('access-point-name', accessPoint.name);
    elementCache.setValue('access-point-type', accessPoint.ap_type || 'other');
    elementCache.setValue('access-point-room-id', accessPoint.room_id);
    elementCache.setValue('access-point-description', accessPoint.description || "");

    await loadCabinetsForRoom(accessPoint.room_id, accessPoint.cabinet_id);

    if (accessPoint.peer_access_point_id) {
      elementCache.setValue('access-point-peer-id', accessPoint.peer_access_point_id);
    }
    if (accessPoint.switch_port_id) {
      elementCache.setValue('access-point-switch-port-id', accessPoint.switch_port_id);
    }
  } else {
    title.textContent = t('access_point.add');
    form.reset();
    elementCache.setValue('access-point-id', '');
    cabinetSelect.innerHTML = `<option value="">${t('access_point.select_cabinet') || '选择机柜'}</option>`;
  }
}
