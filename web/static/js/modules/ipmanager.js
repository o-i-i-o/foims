import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  handleError
} from "../utils/ui.js";

// ====== IP管理 ======

// 加载交换机列表到拉取MAC下拉框
export async function loadSwitchesForPullMac() {
  try {
    const result = await apiGet("/api/switches");
    const select = document.getElementById("pull-mac-switch-select");
    if (!select) return;

    select.innerHTML = '<option value="">-- 选择交换机 --</option>';

    if (result.success && result.data) {
      const switches = Array.isArray(result.data) ? result.data : [];
      
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
    const result = await apiGet("/api/resources/networks");
    const select = document.getElementById("pull-mac-network-select");
    if (!select) return;

    select.innerHTML = '<option value="">-- 选择网段 --</option>';

    if (result.success && result.data) {
      const networks = Array.isArray(result.data) ? result.data : (result.data.data || []);
      
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
    // 显示加载状态，添加旋转动画
    btn.innerHTML = '<span class="loading"></span> 拉取中...';
    btn.disabled = true;

    // 调用拉取IP MAC API，传入交换机ID和网段ID
    const result = await apiPost("/api/resources/ip/pull", { switch_id: switchId, network_id: networkId });

    if (result.success) {
      // 显示详细的成功信息
      const updatedCount = result.data ? result.data.length : 0;
      showToast(`MAC数据拉取成功，共更新了 ${updatedCount} 个IP地址的MAC地址`, "success");
      loadIpMacData(); // 刷新IP MAC数据
    } else {
      showToast(`MAC数据拉取失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "拉取MAC数据失败");
  } finally {
    // 恢复按钮状态
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
      
      renderTable(
        "#ip-table",
        data || [],
        (ipManager) => `
                <td>${ipManager.device_name || "-"}</td>
                <td>${getDeviceTypeName(ipManager.device_type)}</td>
                <td>${ipManager.network_name || "未知"} (${ipManager.network_region || "未知"})</td>
                <td>${ipManager.ip_address}</td>
                <td>${ipManager.mac_address || ""}</td>
                <td>${ipManager.hostname || "-"}</td>
                <td>
                    <span class="status-badge ${ipManager.status === "active" ? "status-active" : "status-inactive"}">
                        ${ipManager.status}
                    </span>
                </td>
                <td>${formatDateTime(ipManager.last_seen)}</td>
                <td>${formatDateTime(ipManager.created_at)}</td>
            `,
        "暂无IP数据",
        10,
      );
      
      return { total, page: currentPage, total_pages };
    }
    
    return null;
  } catch (error) {
    console.error("加载IP数据失败:", error);
    renderTable(
      "#ip-table",
      [],
      () => "",
      "服务器连接失败，请检查网络或联系管理员",
      10,
    );
    return null;
  }
}

// 获取设备类型名称
function getDeviceTypeName(deviceType) {
  const typeMap = {
    'workstation': '工位',
    'cabinet_position': '机位',
    'switch': '交换机'
  };
  return typeMap[deviceType] || deviceType || '-';
}

// 初始化IP相关功能
export const initIpMacFunctions = () => {
  // 刷新按钮事件
  const refreshBtn = document.getElementById("ip-refresh-btn");
  if (refreshBtn) {
    refreshBtn.addEventListener("click", () => {
      const searchInput = document.getElementById("ip-search-input");
      const deviceTypeSelect = document.getElementById("ip-device-type-filter");
      const statusSelect = document.getElementById("ip-status-filter");
      
      loadIpMacData({
        search: searchInput?.value || '',
        device_type: deviceTypeSelect?.value || '',
        status: statusSelect?.value || ''
      });
    });
  }
  
  // 搜索按钮事件
  const searchBtn = document.getElementById("ip-search-btn");
  if (searchBtn) {
    searchBtn.addEventListener("click", () => {
      const searchInput = document.getElementById("ip-search-input");
      const deviceTypeSelect = document.getElementById("ip-device-type-filter");
      const statusSelect = document.getElementById("ip-status-filter");
      
      loadIpMacData({
        search: searchInput?.value || '',
        device_type: deviceTypeSelect?.value || '',
        status: statusSelect?.value || ''
      });
    });
  }
  
  // 搜索框回车事件
  const searchInput = document.getElementById("ip-search-input");
  if (searchInput) {
    searchInput.addEventListener("keypress", (e) => {
      if (e.key === 'Enter') {
        const deviceTypeSelect = document.getElementById("ip-device-type-filter");
        const statusSelect = document.getElementById("ip-status-filter");
        
        loadIpMacData({
          search: searchInput.value,
          device_type: deviceTypeSelect?.value || '',
          status: statusSelect?.value || ''
        });
      }
    });
  }
};
