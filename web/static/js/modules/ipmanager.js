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
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";

import { getDeviceTypeName } from "../utils/formatter.js";

const IP_PAGE_SIZE = 100;

// ====== IP管理 ======

// 加载交换机列表到拉取MAC下拉框
export async function loadSwitchesForPullMac() {
  try {
    const result = await apiGet("/api/switches");
    const select = document.getElementById("pull-mac-switch-select");
    if (!select) return;

    select.innerHTML = '<option value="">-- 选择交换机 --</option>';

    if (result.success && result.data) {
      const switches = Array.isArray(result.data) ? result.data : (result.data.items || []);
      
      if (switches.length === 0) {
        select.innerHTML = '<option value="">暂无交换机数据</option>';
        return;
      }
      
      let hasSnmpSwitch = false;
      switches.forEach((sw) => {
        if (sw.snmp_community || sw.snmp_username) {
          hasSnmpSwitch = true;
          const option = document.createElement("option");
          option.value = sw.id;
          option.textContent = `${sw.name} (${sw.ip_address})`;
          select.appendChild(option);
        }
      });
      
      if (!hasSnmpSwitch) {
        select.innerHTML = '<option value="">暂无配置SNMP的交换机</option>';
      }
    }
  } catch (error) {
    console.error("加载交换机列表失败:", error);
    const select = document.getElementById("pull-mac-switch-select");
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
// 获取选中的交换机
  const switchSelect = document.getElementById("pull-mac-switch-select");
  const switchId = switchSelect ? switchSelect.value : "";

  if (!switchId) {
    showToast("请先选择一个交换机", "warning");
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

    const result = await apiPost("/api/resources/ip/pull", { switch_id: switchId, network_id: networkId });

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

// 加载IP数据（支持搜索和分页）
export async function loadIpMacData(searchParams = {}) {
  try {
    const { search = '', device_type = '', status = '', page = 1, page_size = 100 } = searchParams;
    
    const params = new URLSearchParams();
    if (search) params.append('search', search);
    if (device_type) params.append('device_type', device_type);
    if (status) params.append('status', status);
    params.append('page', page);
    params.append('page_size', page_size);
    
    const result = await apiGet(`/api/resources/ip?${params.toString()}`);

    if (result.success && result.data) {
      const { data, total, page: currentPage, total_pages } = result.data;
      const pageNum = currentPage || 1;
      const startIndex = (pageNum - 1) * page_size;
      
      renderTable("#ip-table", {
        data: data || [],
        columns: [
          { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
          { field: 'device_name', render: (v) => v || '-' },
          { field: 'device_type', render: (v) => getDeviceTypeName(v) },
          { field: 'network_name', render: (v, row) => `${v || '未知'} (${row.network_region || '未知'})` },
          { field: 'ip_address', render: (v) => v },
          { field: 'mac_address', render: (v) => v || '-' },
          { field: 'hostname', render: (v) => v || '-' },
          { field: 'status', render: (v) => `<span class="status-badge ${v === 'active' ? 'status-active' : 'status-inactive'}">${v}</span>` },
          { field: 'last_seen', render: (v) => formatDateTime(v) },
          { field: 'created_at', render: (v) => formatDateTime(v) }
        ],
        emptyMessage: '暂无IP数据'
      });
      
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

// 初始化IP相关功能 - 使用事件委托
export const initIpMacFunctions = () => {
  const ipSection = document.getElementById("ip");
  if (!ipSection) return;

  if (ipSection.dataset.initialized === "true") return;
  ipSection.dataset.initialized = "true";

  ipSection.addEventListener("click", (e) => {
    const target = e.target;
    const id = target.id || target.dataset?.action;

    switch (id) {
      case "ip-refresh-btn":
      case "refresh":
        handleSearch();
        break;
      case "ip-search-btn":
      case "search":
        handleSearch();
        break;
      case "pull-ip-btn":
        pullIpMacData();
        break;
    }
  });

  ipSection.addEventListener("keypress", (e) => {
    if (e.target.id === "ip-search-input" && e.key === "Enter") {
      handleSearch();
    }
  });
};

function handleSearch() {
  const searchInput = document.getElementById("ip-search-input");
  const deviceTypeSelect = document.getElementById("ip-device-type-filter");
  const statusSelect = document.getElementById("ip-status-filter");

  loadIpMacData({
    search: searchInput?.value || '',
    device_type: deviceTypeSelect?.value || '',
    status: statusSelect?.value || ''
  });
}
