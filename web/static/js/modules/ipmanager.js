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
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";

import { getDeviceTypeName } from "../utils/formatter.js";

const IP_PAGE_SIZE = 100;

let currentFilters = {
  device_name: '',
  device_type: '',
  network: '',
  ip_address: ''
};

let currentPage = 1;

// ====== IP管理 ======

// 加载设备列表到拉取MAC下拉框
export async function loadDevicesForPullMac() {
  try {
    const result = await apiGet("/api/resources/devices?page_size=1000");
    const select = document.getElementById("pull-mac-device-select");
    if (!select) return;

    select.innerHTML = '<option value="">-- 选择设备 --</option>';

    if (result.success && result.data) {
      const devices = Array.isArray(result.data) ? result.data : (result.data.items || []);
      
      if (devices.length === 0) {
        select.innerHTML = '<option value="">暂无设备数据</option>';
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
        select.innerHTML = '<option value="">暂无配置SNMP的设备</option>';
      }
    }
  } catch (error) {
    console.error("加载设备列表失败:", error);
    const select = document.getElementById("pull-mac-device-select");
    if (select) {
      select.innerHTML = '<option value="">加载失败</option>';
    }
  }
}

// 加载网段列表到拉取MAC下拉框
export async function loadNetworksForPullMac() {
  try {
    const result = await apiGet("/api/resources/networks?page_size=1000");
    const select = document.getElementById("pull-mac-network-select");
    if (!select) return;

    select.innerHTML = '<option value="">-- 选择网段 --</option>';

    if (result.success && result.data) {
      const networks = Array.isArray(result.data) ? result.data : (result.data.items || result.data.data || []);
      
      if (networks.length === 0) {
        select.innerHTML = '<option value="">暂无网段数据</option>';
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
      select.innerHTML = '<option value="">加载失败</option>';
    }
  }
}

// 拉取IP MAC数据
export async function pullIpMacData() {
// 获取选中的设备
  const deviceSelect = document.getElementById("pull-mac-device-select");
  const deviceId = deviceSelect ? deviceSelect.value : "";

  if (!deviceId) {
    showToast("请先选择一个设备", "warning");
    return;
  }

  // 获取选中的网段
  const networkSelect = document.getElementById("pull-mac-network-select");
  const networkId = networkSelect ? networkSelect.value : "";

  if (!networkId) {
    showToast("请先选择一个网段", "warning");
    return;
  }

  const btn = document.getElementById("pull-ip-btn");
  const originalText = btn.textContent;

  try {
    btn.innerHTML = '<span class="loading"></span> 拉取中...';
    btn.disabled = true;

    const result = await apiPost("/api/resources/ip/pull", { device_id: deviceId, network_id: networkId });

    if (result.success) {
      showToast(result.message || "MAC数据拉取成功", "success");
      loadIpMacData();
    } else {
      showToast(`MAC数据拉取失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "拉取MAC数据失败");
  } finally {
    btn.innerHTML = originalText;
    btn.disabled = false;
  }
}

export async function loadIpMacData(filters = currentFilters, page = currentPage) {
  currentFilters = filters;
  currentPage = page;
  
  try {
    const { device_name = '', device_type = '', network = '', ip_address = '' } = filters;
    
    const params = new URLSearchParams();
    if (device_name) params.append('device_name', device_name);
    if (device_type) params.append('device_type', device_type);
    if (network) params.append('network', network);
    if (ip_address) params.append('ip_address', ip_address);
    params.append('page', page);
    params.append('page_size', IP_PAGE_SIZE);
    
    const result = await apiGet(`/api/resources/ip?${params.toString()}`);

    if (result.success && result.data) {
      const { data, total, page: currentPage, total_pages } = result.data;
      const pageNum = currentPage || 1;
      const startIndex = (pageNum - 1) * IP_PAGE_SIZE;
      
      renderTable("#ip-table", {
        data: data || [],
        columns: [
          { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
          { field: 'location', render: (v, row) => {
            if (row.device_type === 'workstation' && row.room_name) {
              return escapeHtml(row.room_name);
            } else if (row.device_type === 'cabinet_position' && row.cabinet_name) {
              return escapeHtml(row.cabinet_name);
            } else if (row.port_device_name) {
              return escapeHtml(row.port_device_name);
            }
            return '-';
          }},
          { field: 'device_name', render: (v) => escapeHtml(v || '-') },
          { field: 'device_type', render: (v) => escapeHtml(getDeviceTypeName(v)) },
          { field: 'network_name', render: (v, row) => `${escapeHtml(v || '未知')} (${escapeHtml(row.network_region || '未知')})` },
          { field: 'ip_address', render: (v) => escapeHtml(v) },
          { field: 'mac_address', render: (v) => escapeHtml(v || '-') },
          { field: 'hostname', render: (v) => escapeHtml(v || '-') },
          { field: 'status', render: (v) => `<span class="status-badge ${v === 'active' ? 'status-active' : 'status-inactive'}">${escapeHtml(v)}</span>` },
          { field: 'last_seen', render: (v) => formatDateTime(v) },
          { field: 'created_at', render: (v) => formatDateTime(v) }
        ],
        emptyMessage: '暂无IP数据'
      });
      
      if (total !== undefined) {
        appendPaginationToTable("#ip-table", { total, page: pageNum, total_pages }, (p) => loadIpMacData(filters, p));
      }
      
      return { total, page: currentPage, total_pages };
    }
    
    return null;
  } catch (error) {
    console.error("加载IP数据失败:", error);
    renderTable("#ip-table", {
      data: [],
      columns: [],
      emptyMessage: "服务器连接失败，请检查网络或联系管理员"
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
    'ip-device-type-filter',
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
    device_type: document.getElementById('ip-device-type-filter')?.value || '',
    network: document.getElementById('ip-network-filter')?.value || '',
    ip_address: document.getElementById('ip-address-filter')?.value || ''
  };
  
  currentPage = 1;
  loadIpMacData(filters, 1);
}
