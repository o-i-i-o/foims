import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
  getAccessToken,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  handleError
} from "../utils/ui.js";

// ====== IP管理 ======

// 提交IP表单
async function submitIpForm() {
  const token = getAccessToken();
  if (!token) return;

  const id = document.getElementById("ip-id").value;
  const workstationId = document.getElementById("ip-workstation").value;
  const cabinetPositionId = document.getElementById("ip-cabinet-position").value;
  const switchId = document.getElementById("ip-switch").value;
  const networkId = document.getElementById("ip-network").value;
  const ipAddress = document.getElementById("ip-address").value;
  const macAddress = document.getElementById("ip-mac-address").value;
  const hostname = document.getElementById("ip-hostname").value;

  // 验证网络和IP地址
  if (!networkId || !ipAddress) {
    showToast("请选择网络并输入IP地址", "warning");
    return;
  }

  // 确定设备类型和ID
  let deviceType = "";
  let deviceId = null;

  if (workstationId) {
    deviceType = "workstation";
    deviceId = workstationId;
  } else if (cabinetPositionId) {
    deviceType = "cabinet_position";
    deviceId = cabinetPositionId;
  } else if (switchId) {
    deviceType = "switch";
    deviceId = switchId;
  } else {
    showToast("请选择设备", "warning");
    return;
  }

  const ipData = {
    device_type: deviceType,
    network_id: networkId,
    ip_address: ipAddress,
    mac_address: macAddress || null,
    hostname: hostname || null,
  };

  // 根据设备类型设置对应的ID
  if (deviceType === "workstation") {
    ipData.workstation_id = deviceId;
  } else if (deviceType === "cabinet_position") {
    ipData.position_id = deviceId;
  } else if (deviceType === "switch") {
    ipData.switch_id = deviceId;
  }

  try {
    const url = id ? `/api/resources/ip/${id}` : "/api/resources/ip";
    const method = id ? "PUT" : "POST";

    const response = await fetch(url, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(ipData),
    });

    if (!response.ok) {
      throw new Error(`API请求失败: ${response.status}`);
    }

    const result = await response.json();
    if (result.success) {
      showToast("IP地址保存成功", "success");

      // 重新加载设备的IP地址列表
      if (deviceId) {
        if (deviceType === "workstation") {
          loadWorkstationIps(deviceId);
        } else if (deviceType === "cabinet_position") {
          loadCabinetPositionIps(deviceId);
        } else if (deviceType === "switch") {
          loadSwitchIps(deviceId);
        }
      }
    } else {
      showToast(`IP地址保存失败: ${result.message}`, "error");
    }
  } catch (error) {
    console.error("提交IP表单失败:", error);
    showToast(`IP地址保存失败: ${error.message}`, "error");
  }
}

// 绑定IP表单提交事件
function bindIpFormSubmit() {
  const ipForm = document.getElementById("ip-form");
  if (ipForm) {
    ipForm.onsubmit = async function(e) {
      e.preventDefault();
      await submitIpForm();
    };
  }
}

// 初始化IP管理
export function initIpManagement() {
  // 绑定IP表单提交事件
  bindIpFormSubmit();
}

// 打开IP管理模态框，并设置设备类型和ID
async function openIpManagerModalWithDevice(deviceType, deviceId) {
  // IP模态框已被移除，请在工位或机位表单中直接添加IP地址
  showToast("IP模态框已被移除，请在工位或机位表单中直接添加IP地址", "info");
}


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
  const token = getAccessToken();
  if (!token) {
    showToast("请先登录", "warning");
    return;
  }

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
    const response = await fetch("/api/resources/ip/pull", {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ switch_id: switchId, network_id: networkId }),
    });

    const result = await response.json();

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

// 获取网络可用IP列表
export async function getAvailableIps(networkId) {
  try {
    const result = await apiGet(`/api/resources/ip/available/${networkId}`);
    if (result.success) {
      return result.data;
    }
    return null;
  } catch (error) {
    console.error("获取可用IP列表失败:", error);
    return null;
  }
}

// 自动分配IP地址
export async function autoAssignIp(data) {
  try {
    const result = await apiPost("/api/resources/ip/auto-assign", data);
    if (result.success) {
      showToast(`IP地址 ${result.data.ip_address} 自动分配成功`, "success");
      return result.data;
    } else {
      showToast(`自动分配失败: ${result.message}`, "error");
      return null;
    }
  } catch (error) {
    handleError(error, "自动分配IP失败");
    return null;
  }
}

// 批量创建IP记录
export async function batchCreateIpManagers(ipList) {
  try {
    const result = await apiPost("/api/resources/ip/batch", ipList);
    if (result.success) {
      const { created_count, error_count, errors } = result.data;
      if (error_count > 0) {
        showToast(`批量创建完成：成功 ${created_count} 条，失败 ${error_count} 条`, "warning");
        console.warn("批量创建错误:", errors);
      } else {
        showToast(`批量创建成功：共 ${created_count} 条`, "success");
      }
      return result.data;
    } else {
      showToast(`批量创建失败: ${result.message}`, "error");
      return null;
    }
  } catch (error) {
    handleError(error, "批量创建IP失败");
    return null;
  }
}

// 批量删除IP记录
export async function batchDeleteIpManagers(ids) {
  if (!ids || ids.length === 0) {
    showToast("请选择要删除的记录", "warning");
    return null;
  }

  try {
    const result = await apiDelete("/api/resources/ip/batch", { ids });
    if (result.success) {
      const { deleted_count, errors } = result.data;
      if (errors && errors.length > 0) {
        showToast(`批量删除完成：成功 ${deleted_count} 条`, "warning");
      } else {
        showToast(`批量删除成功：共 ${deleted_count} 条`, "success");
      }
      return result.data;
    } else {
      showToast(`批量删除失败: ${result.message}`, "error");
      return null;
    }
  } catch (error) {
    handleError(error, "批量删除IP失败");
    return null;
  }
}

// 填充IP地址选择下拉框（可用IP）
export async function populateAvailableIpSelect(selectId, networkId) {
  const select = document.getElementById(selectId);
  if (!select) return;

  select.innerHTML = '<option value="">-- 自动分配 --</option>';

  const availableData = await getAvailableIps(networkId);
  if (availableData && availableData.available_ips) {
    // 只显示前50个可用IP，避免下拉框过长
    const displayIps = availableData.available_ips.slice(0, 50);
    displayIps.forEach(ip => {
      const option = document.createElement("option");
      option.value = ip;
      option.textContent = ip;
      select.appendChild(option);
    });

    if (availableData.available_ips.length > 50) {
      const option = document.createElement("option");
      option.value = "";
      option.textContent = `... 还有 ${availableData.available_ips.length - 50} 个可用IP`;
      option.disabled = true;
      select.appendChild(option);
    }
  }
}
