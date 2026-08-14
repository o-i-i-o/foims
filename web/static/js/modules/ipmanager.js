import {
  apiGet,
  apiPost,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  handleError,
  appendPaginationToTable,
  debounce,
  escapeHtml,
  createSortState,
  updateSortIcons,
  initSortEvents,
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";

import { getDeviceTypeName } from "../utils/formatter.js";

const IP_PAGE_SIZE = 100;
let currentPageSize = IP_PAGE_SIZE;

let currentFilters = {
  device_name: '',
  network: '',
  ip_address: ''
};

let currentPage = 1;

const ipTableState = createSortState('updated_at', 'desc');

// ====== IP管理 ======

// 加载设备列表到拉取MAC下拉框
export async function loadDevicesForPullMac() {
  try {
    const result = await apiGet("/api/resources/devices?page_size=1000");
    const select = document.getElementById("pull-mac-device-select");
    if (!select) return;

    select.innerHTML = `<option value="">${t('ip.select_device')}</option>`;

    if (result.success && result.data) {
      const devices = Array.isArray(result.data) ? result.data : (result.data.items || []);
      
      if (devices.length === 0) {
        select.innerHTML = `<option value="">${t('ip.no_device_data')}</option>`;
        return;
      }
      
      let hasSnmpDevice = false;
      devices.forEach((dev) => {
        if (dev.snmp_community || dev.snmp_username) {
          hasSnmpDevice = true;
          const option = document.createElement("option");
          option.value = dev.id;
          option.textContent = `${dev.name} (${dev.ip_address || '-'})`;
          select.appendChild(option);
        }
      });
      
      if (!hasSnmpDevice) {
        select.innerHTML = `<option value="">${t('ip.no_snmp_device')}</option>`;
      }
    }
  } catch (error) {
    console.error("加载设备列表失败:", error);
    const select = document.getElementById("pull-mac-device-select");
    if (select) {
      select.innerHTML = `<option value="">${t('ip.load_failed_short')}</option>`;
    }
  }
}

// 加载网段列表到拉取MAC下拉框
export async function loadNetworksForPullMac() {
  try {
    const result = await apiGet("/api/resources/networks?page_size=1000");
    const select = document.getElementById("pull-mac-network-select");
    if (!select) return;

    select.innerHTML = `<option value="">${t('ip.select_network')}</option>`;

    if (result.success && result.data) {
      const networks = Array.isArray(result.data) ? result.data : (result.data.items || result.data.data || []);
      
      if (networks.length === 0) {
        select.innerHTML = `<option value="">${t('ip.no_network_data')}</option>`;
        return;
      }
      
      networks.forEach((network) => {
        const option = document.createElement("option");
        option.value = network.id;
        option.textContent = `${network.name} (${network.ipv4_cidr || network.ipv6_cidr || '-'})`;
        select.appendChild(option);
      });
    }
  } catch (error) {
    console.error("加载网段列表失败:", error);
    const select = document.getElementById("pull-mac-network-select");
    if (select) {
      select.innerHTML = `<option value="">${t('ip.load_failed_short')}</option>`;
    }
  }
}

// 拉取IP MAC数据
export async function pullIpMacData() {
// 获取选中的设备
  const deviceSelect = document.getElementById("pull-mac-device-select");
  const deviceId = deviceSelect ? deviceSelect.value : "";

  if (!deviceId) {
    showToast(t('ip.select_device_first'), "warning");
    return;
  }

  // 获取选中的网段
  const networkSelect = document.getElementById("pull-mac-network-select");
  const networkId = networkSelect ? networkSelect.value : "";

  if (!networkId) {
    showToast(t('ip.select_network_first'), "warning");
    return;
  }

  const btn = document.getElementById("pull-ip-btn");
  const originalText = btn.textContent;

  try {
    btn.innerHTML = `<span class="loading"></span> ${t('ip.pulling')}`;
    btn.disabled = true;

    const result = await apiPost("/api/resources/ip/pull", { device_id: deviceId, network_id: networkId });

    if (result.success) {
      showToast(result.message || t('ip.pull_mac_success'), "success");
      loadIpMacData();
    } else {
      showToast(`${t('ip.pull_mac_failed')}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t('ip.pull_mac_failed'));
  } finally {
    btn.innerHTML = originalText;
    btn.disabled = false;
  }
}

export async function loadIpMacData(filters = currentFilters, page = currentPage, sortBy = null, sortOrder = null) {
  currentFilters = filters;
  currentPage = page;
  if (sortBy) ipTableState.setSort(sortBy, sortOrder);

  try {
    const { device_name = '', network = '', ip_address = '' } = filters;

    const params = new URLSearchParams();
    if (device_name) params.append('device_name', device_name);
    if (network) params.append('network', network);
    if (ip_address) params.append('ip_address', ip_address);
    params.append('page', page);
    params.append('page_size', currentPageSize);
    params.append('sort_by', ipTableState.sortBy);
    params.append('sort_order', ipTableState.sortOrder);

    const result = await apiGet(`/api/resources/ip?${params.toString()}`);

    if (result.success && result.data) {
      const { data, total, page: currentPage, total_pages } = result.data;
      const pageNum = currentPage || 1;
      const startIndex = (pageNum - 1) * currentPageSize;

      renderTable("#ip-table", {
        data: data || [],
        columns: [
          { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
          { field: 'location', render: (v, row) => {
            return escapeHtml(row.workstation_name || row.cabinet_name || row.port_device_name || row.device_name || '-');
          }},
          { field: 'device_name', render: (v) => escapeHtml(v || '-') },
          { field: 'device_type', render: (v) => escapeHtml(getDeviceTypeName(v)), className: 'col-center' },
          { field: 'network_name', render: (v, row) => `${escapeHtml(v || t('common.unknown'))} (${escapeHtml(row.network_region || t('common.unknown'))})` },
          { field: 'ip_address', render: (v) => escapeHtml(v) },
          { field: 'mac_address', render: (v) => escapeHtml(v || '-') },
          { field: 'hostname', render: (v) => escapeHtml(v || '-') },
          { field: 'status', render: (v) => `<span class="status-badge ${v === 'active' ? 'status-active' : 'status-inactive'}">${escapeHtml(v)}</span>`, className: 'col-center' },
          { field: 'last_seen', render: (v) => formatDateTime(v), className: 'col-center' },
          { field: 'created_at', render: (v) => formatDateTime(v), className: 'col-center' }
        ],
        emptyMessage: t('ip.no_ip_data')
      });

      if (total !== undefined) {
        appendPaginationToTable("#ip-table", { total, page: pageNum, total_pages, page_size: currentPageSize }, (p) => loadIpMacData(filters, p), {
        pageSize: currentPageSize,
        onPageSizeChange: (size) => { currentPageSize = size; loadIpMacData(filters, 1); },
      });
      }

      updateSortIcons("ip-table", ipTableState);

      return { total, page: currentPage, total_pages };
    }

    return null;
  } catch (error) {
    console.error("加载IP数据失败:", error);
    renderTable("#ip-table", {
      data: [],
      columns: [],
      emptyMessage: t('ip.server_connection_failed')
    });
    return null;
  }
}

export const initIpMacFunctions = () => {
  const ipSection = document.getElementById("ip");
  if (!ipSection) return;

  if (ipSection.dataset.initialized === "true") return;
  ipSection.dataset.initialized = "true";
  
  initIpFilters();

  initSortEvents("ip-table", ipTableState, (page, sortBy, sortOrder) => loadIpMacData(currentFilters, page, sortBy, sortOrder));

  ipSection.addEventListener("click", (e) => {
    const target = e.target;
    const id = target.id || target.dataset?.action;

    switch (id) {
      case "pull-ip-btn":
        pullIpMacData();
        break;
    }
  });
};

export function initIpFilters() {
  const filterIds = [
    'ip-device-name-filter',
    'ip-network-filter',
    'ip-address-filter'
  ];
  
  const debouncedFilter = debounce(applyIpFilters, 300);
  
  filterIds.forEach(filterId => {
    const filterElement = document.getElementById(filterId);
    if (filterElement) {
      filterElement.addEventListener('input', debouncedFilter);
    }
  });
}

function applyIpFilters() {
  const filters = {
    device_name: document.getElementById('ip-device-name-filter')?.value || '',
    network: document.getElementById('ip-network-filter')?.value || '',
    ip_address: document.getElementById('ip-address-filter')?.value || ''
  };
  
  currentPage = 1;
  loadIpMacData(filters, 1);
}
