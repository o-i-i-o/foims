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

const OUTLET_TYPE_LABELS = {
  wall_socket: t('net_outlet.type_wall_socket') || '墙面插座',
  patch_panel: t('net_outlet.type_patch_panel') || '配线架',
  wifi_ap: t('net_outlet.type_wifi_ap') || '无线AP',
  other: t('net_outlet.type_other') || '其他',
};

function getOutletTypeName(type) {
  return OUTLET_TYPE_LABELS[type] || type;
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
        { field: 'peer_net_outlet_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'connected_device_port', render: (v, row) => {
          if (v && row.connected_device_name) return `${escapeHtml(row.connected_device_name)}: ${escapeHtml(v)}`;
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

async function loadPeerNetOutlets(currentId = null, selectedPeerId = null) {
  const peerSelect = elementCache.get('net-outlet-peer-id');
  if (!peerSelect) return;

  peerSelect.innerHTML = `<option value="">${t('net_outlet.select_peer') || '选择对端信息点'}</option>`;

  try {
    const result = await apiGet('/api/resources/net-outlets?page_size=1000');
    if (result.success && result.data) {
      const items = result.data.items || result.data;
      items.forEach(outlet => {
        if (outlet.id === currentId) return;
        const option = document.createElement('option');
        option.value = outlet.id;
        option.textContent = outlet.name;
        peerSelect.appendChild(option);
      });

      if (selectedPeerId) {
        peerSelect.value = selectedPeerId;
      }
    }
  } catch (error) {
    console.error('加载对端信息点选项失败:', error);
  }
}

async function loadDevicePortsForSelect(selectedPortId = null) {
  const portSelect = elementCache.get('net-outlet-device-port-id');
  if (!portSelect) return;

  portSelect.innerHTML = `<option value="">${t('net_outlet.select_device_port') || '选择设备端口'}</option>`;

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

export async function submitNetOutletForm() {
  const id = getElementValue("net-outlet-id");
  const name = getElementValue("net-outlet-name");
  const outletType = getElementValue("net-outlet-type");
  const roomId = getElementValue("net-outlet-room-id");
  const cabinetId = getElementValue("net-outlet-cabinet-id");
  const peerId = getElementValue("net-outlet-peer-id");
  const switchPortId = getElementValue("net-outlet-device-port-id");
  const description = getElementValue("net-outlet-description");

  if (!name?.trim()) {
    showToast(t('net_outlet.name_required'), "warning");
    return;
  }

  if (!roomId) {
    showToast(t('net_outlet.room_required'), "warning");
    return;
  }

  const outletData = {
    name: name.trim(),
    outlet_type: outletType || 'other',
    room_id: roomId,
    cabinet_id: cabinetId || null,
    peer_net_outlet_id: peerId || null,
    device_port_id: switchPortId || null,
    description: description?.trim() || null,
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

export async function openNetOutletModal(netOutlet = null) {
  await openModal("net-outlet-modal");

  const title = elementCache.get('net-outlet-modal-title');
  const form = elementCache.get('net-outlet-form');

  await loadRoomsForSelect("net-outlet-room-id", { onlyOffice: false, includeVisualization: false });

  const roomSelect = elementCache.get('net-outlet-room-id');
  const cabinetSelect = elementCache.get('net-outlet-cabinet-id');

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

  await loadPeerNetOutlets(netOutlet?.id, netOutlet?.peer_net_outlet_id);
  await loadDevicePortsForSelect(netOutlet?.device_port_id);

  if (netOutlet) {
    title.textContent = t('net_outlet.edit');
    elementCache.setValue('net-outlet-id', netOutlet.id);
    elementCache.setValue('net-outlet-name', netOutlet.name);
    elementCache.setValue('net-outlet-type', netOutlet.outlet_type || 'other');
    elementCache.setValue('net-outlet-room-id', netOutlet.room_id);
    elementCache.setValue('net-outlet-description', netOutlet.description || "");

    await loadCabinetsForRoom(netOutlet.room_id, netOutlet.cabinet_id);

    if (netOutlet.peer_net_outlet_id) {
      elementCache.setValue('net-outlet-peer-id', netOutlet.peer_net_outlet_id);
    }
    if (netOutlet.device_port_id) {
      elementCache.setValue('net-outlet-device-port-id', netOutlet.device_port_id);
    }
  } else {
    title.textContent = t('net_outlet.add');
    form.reset();
    elementCache.setValue('net-outlet-id', '');
    cabinetSelect.innerHTML = `<option value="">${t('net_outlet.select_cabinet') || '选择机柜'}</option>`;
  }
}
