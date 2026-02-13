// ==================== 交换机管理功能 ====================

// 导入必要的模块
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
  getElementValue,
  debounce,
  handleDelete,
  handleError
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";

import {
  bindSwitchAddIpButton,
  addIpAddressFieldToSwitchForm,
  IpConfigManager,
  getManager
} from "../utils/ipconfig.js";

// 加载交换机数据
async function loadSwitchesData() {
  try {
    const result = await apiGet("/api/switches");

    renderTable(
      "#switches-table",
      result.success ? result.data : [],
      (sw) => `
                <td>${sw.name}</td>
                <td>${sw.device_type || "-"}</td>
                <td>${sw.ip_address}</td>
                <td>${sw.mac_address || "-"}</td>
                <td>${sw.model || "-"}</td>
                <td>${sw.vendor || "-"}</td>
                <td>${sw.location || "-"}</td>
                <td><span class="status-badge ${sw.snmp_community || sw.snmp_username ? "status-active" : "status-inactive"}">${sw.snmp_version || "-"}</span></td>
                <td>${sw.parent_switch_name || "-"}</td>
                <td>
                    <button class="btn btn-sm btn-edit" data-id="${sw.id}">编辑</button>
                    <button class="btn btn-sm btn-secondary btn-switch-ports" data-switch-id="${sw.id}" data-switch-name="${sw.name}">端口</button>
                    <button class="btn btn-sm btn-secondary btn-switch-arp" data-switch-id="${sw.id}">ARP表</button>
                    <button class="btn btn-sm btn-delete" data-id="${sw.id}">删除</button>
                </td>
            `,
      "暂无交换机数据",
      10,
    );

    // 绑定交换机端口和ARP表按钮的点击事件
    bindSwitchButtonsEvents();
  } catch (error) {
    handleError(error, "加载交换机数据失败", () => {
      renderTable("#switches-table", [], () => "", "加载失败", 10);
    });
  }
}

// 绑定交换机按钮事件（使用事件委托避免重复绑定）
function bindSwitchButtonsEvents() {
  const table = document.getElementById("switches-table");
  if (!table || table.dataset.eventsBound === "true") return;

  table.addEventListener("click", (e) => {
    const button = e.target.closest("button");
    if (!button) return;

    if (button.classList.contains("btn-switch-ports")) {
      const switchId = button.getAttribute("data-switch-id");
      const switchName = button.getAttribute("data-switch-name");
      manageSwitchPorts(switchId, switchName);
    }

    if (button.classList.contains("btn-switch-arp")) {
      const switchId = button.getAttribute("data-switch-id");
      viewArpTable(switchId);
    }
  });

  table.dataset.eventsBound = "true";
}

// 加载所有端口数据
async function loadAllSwitchPortsData() {
  try {
    const result = await apiGet("/api/switches/ports");

    renderTable(
      "#switch-ports-table",
      result.success ? result.data : [],
      (port) => `
                <td>${port.switch_name} (${port.switch_ip})</td>
                <td>${port.port_number}</td>
                <td>${port.port_name || "-"}</td>
                <td>${port.port_type}</td>
                <td>${port.vlan_id || "-"}</td>
                <td><span class="status-badge ${port.status === "up" ? "status-active" : "status-inactive"}">${port.status}</span></td>
                <td>${port.speed || "-"}</td>
                <td>
                    <button class="btn btn-sm btn-edit" data-id="${port.id}">编辑</button>
                    <button class="btn btn-sm btn-delete" data-id="${port.id}">删除</button>
                </td>
            `,
      "暂无端口数据",
      8,
    );
  } catch (error) {
    handleError(error, "加载端口数据失败", () => {
      renderTable("#switch-ports-table", [], () => "", "加载失败", 8);
    });
  }
}

// 打开交换机模态框
async function openSwitchModal(sw = null) {
  console.log("openSwitchModal called with sw:", sw);
  const modal = document.getElementById("switch-modal");
  const title = document.getElementById("switch-modal-title");
  const form = document.getElementById("switch-form");

  if (sw) {
    title.textContent = "编辑交换机";
    document.getElementById("switch-id").value = sw.id;
    document.getElementById("switch-name").value = sw.name;
    document.getElementById("switch-model").value = sw.model || "";
    document.getElementById("switch-vendor").value = sw.vendor || "";
    document.getElementById("switch-location").value = sw.location || "";
    document.getElementById("switch-description").value = sw.description || "";
    document.getElementById("switch-snmp-version").value =
      sw.snmp_version || "v2c";
    document.getElementById("switch-snmp-port").value = sw.snmp_port || 161;
    document.getElementById("switch-snmp-community").value =
      sw.snmp_community || "";
    document.getElementById("switch-snmp-username").value =
      sw.snmp_username || "";
    document.getElementById("switch-snmp-auth-protocol").value =
      sw.snmp_auth_protocol || "";
    document.getElementById("switch-snmp-auth-password").value =
      sw.snmp_auth_password || "";
    document.getElementById("switch-snmp-priv-protocol").value =
      sw.snmp_priv_protocol || "";
    document.getElementById("switch-snmp-priv-password").value =
      sw.snmp_priv_password || "";
  } else {
    title.textContent = "添加交换机";
    form.reset();
    document.getElementById("switch-id").value = "";
    document.getElementById("switch-snmp-port").value = "161";
  }

  // 使用全局缓存的IpConfigManager实例，避免重复创建
  const ipManager = getManager('switch');
  
  // 编辑模式下，设置要排除的交换机ID（不能选择自己作为上级）
  if (sw && sw.id) {
    ipManager.setExcludeSwitchId(sw.id);
  } else {
    ipManager.setExcludeSwitchId(null);
  }
  
  // 清空IP容器
  ipManager.clear();
  
  // 处理 IP 和上联信息
  // 1. 如果有 IP 列表，后端返回的 IP 对象中可能已包含 switch_id/switch_port_id（即上联信息）
  // 2. 如果没有 IP 列表，但交换机本身有 parent_switch_id，我们需要手动构造一个项来显示
  
  let ipsToLoad = [];
  if (sw && sw.ips && sw.ips.length > 0) {
    // 使用现有的 IP 列表
    // 注意：后端返回的 switch.ips 中的每一项通常包含 switch_id 和 switch_port_id
    // 如果后端没有正确填充这些字段（例如它们存储在 switches 表而不是 ip_managers 表），
    // 我们可能需要手动合并 sw.parent_switch_id 到第一个 IP 项中
    ipsToLoad = sw.ips.map((ip, index) => {
         // 如果是第一个 IP 且没有自身的交换机连接信息，尝试使用交换机层面的上联信息
         if (index === 0 && !ip.switch_id && sw.parent_switch_id) {
             return {
                 ...ip,
                 switch_id: sw.parent_switch_id,
                 switch_port_id: sw.parent_port_id
             };
         }
         return ip;
    });
  } else if (sw && sw.parent_switch_id) {
    // 无 IP 但有上联
    ipsToLoad = [{
        parent_switch_id: sw.parent_switch_id,
        parent_port_id: sw.parent_port_id,
        _is_uplink_only: true
    }];
  }
  
  if (ipsToLoad.length > 0) {
      await ipManager.loadIps(ipsToLoad);
  } else {
      // 添加模式或编辑模式但没有IP，只添加一个IP容器
      await ipManager.addIpRow();
  }

  // 编辑模式下，设置上级交换机和端口（从交换机本身的属性获取）
  // 这部分代码是为了兼容旧的逻辑，确保即使 loadIps 没有正确设置，这里也能再次设置
  if (sw && sw.parent_switch_id) {
    // 等待 loadIps 完成后的 DOM 更新
    await new Promise(resolve => setTimeout(resolve, 100));
    
    const firstIpRow = document.querySelector(".switch-ip-row");
    if (firstIpRow) {
      const switchSelect = firstIpRow.querySelector(".switch-switch-select");
      const portSelect = firstIpRow.querySelector(".switch-switch-port-select");
      
      if (switchSelect && (!switchSelect.value || switchSelect.value === "")) {
        // 查找匹配的选项并设置值
        const normalizedSwitchId = sw.parent_switch_id.toLowerCase();
        const switchOptions = switchSelect.querySelectorAll('option');
        let found = false;
        for (const opt of switchOptions) {
          if (opt.value && opt.value.toLowerCase() === normalizedSwitchId) {
            switchSelect.value = opt.value;
            found = true;
            break;
          }
        }
        
        if (found) {
            // 触发change事件加载端口
            // 注意：这里需要传递 true 给 IpConfigManager.handleSwitchChange
            // 但由于我们无法直接调用该方法，只能触发事件
            // 为了确保端口被加载，我们可以尝试手动调用 getManager('switch').handleSwitchChange
            
            await ipManager.handleSwitchChange(switchSelect, portSelect);
            
            // 等待端口加载完成后设置端口值
            if (sw.parent_port_id && portSelect) {
              const normalizedPortId = sw.parent_port_id.toLowerCase();
              const portOptions = portSelect.querySelectorAll('option');
              for (const opt of portOptions) {
                if (opt.value && opt.value.toLowerCase() === normalizedPortId) {
                  portSelect.value = opt.value;
                  break;
                }
              }
            }
        }
      }
    }
  }

  // 根据SNMP版本显示/隐藏配置
  toggleSnmpConfig();

  // 添加从SNMP获取按钮的事件监听器
  const getSnmpInfoBtn = document.getElementById("get-snmp-info");
  if (getSnmpInfoBtn) {
    // 先移除可能存在的旧监听器
    getSnmpInfoBtn.removeEventListener("click", getSwitchInfoFromSnmp);
    // 添加新监听器
    getSnmpInfoBtn.addEventListener("click", getSwitchInfoFromSnmp);
  }

  openModal("switch-modal");
}

// 切换SNMP配置显示
function toggleSnmpConfig() {
  const version = document.getElementById("switch-snmp-version").value;
  const v2cConfig = document.getElementById("snmp-v2c-config");
  const v3Config = document.getElementById("snmp-v3-config");

  if (version === "v3") {
    v2cConfig.style.display = "none";
    v3Config.style.display = "block";
  } else {
    v2cConfig.style.display = "flex";
    v3Config.style.display = "none";
  }
}

// 从SNMP获取交换机信息
async function getSwitchInfoFromSnmp() {
  // 收集IP地址信息
  const ipRows = document.querySelectorAll(".switch-ip-row");
  if (ipRows.length === 0) {
    showToast("请先添加IP地址", "warning");
    return;
  }

  // 使用第一个IP地址作为测试
  const firstRow = ipRows[0];
  const networkRegionSelect = firstRow.querySelector(".switch-ip-network-region-select");
  const networkSelect = firstRow.querySelector(".switch-ip-network-select");
  
  if (!networkRegionSelect || !networkSelect || !networkRegionSelect.value || !networkSelect.value) {
    showToast("请先选择网络区域和网络", "warning");
    return;
  }

  // 创建临时交换机对象用于测试SNMP
  const networkRegionId = networkRegionSelect.value;
  const networkId = networkSelect.value;
  
  console.log("SNMP测试时的网络区域ID:", networkRegionId);
  console.log("SNMP测试时的网络ID:", networkId);
  
  const switchData = {
    name: "临时测试交换机",
    network_region_id: networkRegionId,
    network_id: networkId,
    snmp_version: document.getElementById("switch-snmp-version").value,
    snmp_port: parseInt(document.getElementById("switch-snmp-port").value) || 161,
    snmp_community: document.getElementById("switch-snmp-community").value || "public",
    snmp_username: document.getElementById("switch-snmp-username").value || null,
    snmp_auth_protocol: document.getElementById("switch-snmp-auth-protocol").value || null,
    snmp_auth_password: document.getElementById("switch-snmp-auth-password").value || null,
    snmp_priv_protocol: document.getElementById("switch-snmp-priv-protocol").value || null,
    snmp_priv_password: document.getElementById("switch-snmp-priv-password").value || null
  };
  
  console.log("SNMP测试数据:", switchData);

  try {
    // 先测试SNMP连接
    const testResult = await apiPost(`/api/switches/test-snmp`, switchData);
    if (!testResult.success) {
      showToast(`SNMP连接测试失败: ${testResult.message}`, "error");
      return;
    }

    // 获取交换机ID
    const switchId = document.getElementById("switch-id").value;
    let infoResult;
    
    if (switchId) {
      // 如果是编辑模式，使用已有交换机的SNMP信息
      infoResult = await apiGet(`/api/switches/${switchId}/snmp-info`);
    } else {
      // 如果是添加模式，先创建临时交换机以获取SNMP信息
      const createResult = await apiPost(`/api/switches`, switchData);
      if (!createResult.success) {
        showToast(`创建临时交换机失败: ${createResult.message}`, "error");
        return;
      }
      
      infoResult = await apiGet(`/api/switches/${createResult.data.id}/snmp-info`);
      
      // 删除临时交换机
      await apiDelete(`/api/switches/${createResult.data.id}`);
    }

    if (infoResult.success) {
      const { vendor, model } = infoResult.data;
      
      // 检查是否已有手动添加的内容，提示是否覆盖
      const currentVendor = document.getElementById("switch-vendor").value;
      const currentModel = document.getElementById("switch-model").value;
      
      if (currentVendor || currentModel) {
        if (confirm("已存在手动添加的交换机信息，是否覆盖?")) {
          document.getElementById("switch-vendor").value = vendor;
          document.getElementById("switch-model").value = model;
        }
      } else {
        document.getElementById("switch-vendor").value = vendor;
        document.getElementById("switch-model").value = model;
      }
      
      showToast("从SNMP获取交换机信息成功", "success");
    } else {
      showToast(`获取SNMP信息失败: ${infoResult.message}`, "error");
    }
  } catch (error) {
    handleError(error, "从SNMP获取交换机信息失败");
  }
}

// 从SNMP获取交换机端口信息
async function getSwitchPortsFromSnmp(switchId) {
  try {
    const result = await apiGet(`/api/switches/${switchId}/snmp-ports`);
    
    if (result.success) {
      const ports = result.data;
      if (ports.length === 0) {
        showToast("未从SNMP获取到端口信息", "warning");
        return;
      }
      
      // 检查是否已有端口，提示是否覆盖
      const currentPortsResult = await apiGet(`/api/switches/${switchId}/ports`);
      if (currentPortsResult.success && currentPortsResult.data.length > 0) {
        if (!confirm(`当前交换机已有 ${currentPortsResult.data.length} 个端口，从SNMP获取到 ${ports.length} 个端口，是否覆盖?`)) {
          return;
        }
        
        // 删除现有端口
        for (const port of currentPortsResult.data) {
          await apiDelete(`/api/switches/ports/${port.id}`);
        }
      }
      
      // 添加新端口
      let successCount = 0;
      for (const port of ports) {
        const createResult = await apiPost(`/api/switches/${switchId}/ports`, port);
        if (createResult.success) {
          successCount++;
        }
      }
      
      showToast(`成功添加 ${successCount} 个端口`, "success");
      loadSwitchPortsData(switchId);
    } else {
      showToast(`获取SNMP端口信息失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "从SNMP获取端口信息失败");
  }
}



// 提交交换机表单
async function submitSwitchForm() {
  const token = getAccessToken();
  if (!token) {
    showToast("请先登录", "warning");
    return;
  }

  const id = document.getElementById("switch-id").value;
  
  // 收集IP地址信息
  const ipManager = getManager('switch');
  const ips = ipManager.getIps();
  
  // 从第一个IP行中获取上级交换机和端口信息
  let parentSwitchId = null;
  let parentPortId = null;
  
  // 即使 ips 数组为空（如果只配置了上联但没有 IP），我们也需要获取上联信息
  const firstIpRow = document.querySelector(".switch-ip-row");
  if (firstIpRow) {
    const switchSelect = firstIpRow.querySelector(".switch-switch-select");
    const portSelect = firstIpRow.querySelector(".switch-switch-port-select");
    if (switchSelect) parentSwitchId = switchSelect.value || null;
    if (portSelect) parentPortId = portSelect.value || null;
  }
  
  // 过滤掉虚拟的 uplink-only IP 项，不将其作为 IP 发送给后端
  // 但保留它们的上联信息（已经在上面提取了）
  // 注意：getIps() 返回的是纯数据对象，没有 DOM 引用或 _is_uplink_only 标记
  // 所以我们需要检查 IP 地址是否为空
  
  const validIps = ips.filter(ip => ip.ip_address && ip.ip_address.trim());
  
  const data = {
    name: document.getElementById("switch-name").value,
    model: document.getElementById("switch-model").value || null,
    vendor: document.getElementById("switch-vendor").value || null,
    location: document.getElementById("switch-location").value || null,
    description: document.getElementById("switch-description").value || null,
    snmp_version: document.getElementById("switch-snmp-version").value,
    snmp_port:
      parseInt(document.getElementById("switch-snmp-port").value) || 161,
    snmp_community:
      document.getElementById("switch-snmp-community").value || null,
    snmp_username:
      document.getElementById("switch-snmp-username").value || null,
    snmp_auth_protocol:
      document.getElementById("switch-snmp-auth-protocol").value || null,
    snmp_auth_password:
      document.getElementById("switch-snmp-auth-password").value || null,
    snmp_priv_protocol:
      document.getElementById("switch-snmp-priv-protocol").value || null,
    snmp_priv_password:
      document.getElementById("switch-snmp-priv-password").value || null,
    parent_switch_id: parentSwitchId,
    parent_port_id: parentPortId,
    ips: validIps.length > 0 ? validIps : null
  };

  if (!data.name) {
    showToast("请填写交换机名称", "warning");
    return;
  }
  
  // 如果没有 IP 也没有上联，提示至少填写一项
  // 但允许只填上联不填 IP（纯二层交换机场景）
  /* 
  if (validIps.length === 0 && !parentSwitchId) {
     showToast("请至少添加一个IP地址或配置上联交换机", "warning");
     return;
  }
  */
  
  // 验证所有IP地址行都有有效的IP地址
  /* 
  for (const ip of ips) {
    if (!ip.ip_address || !ip.ip_address.trim()) {
      showToast("请填写所有IP地址", "warning");
      return;
    }
  }
  */
  
  console.log("提交的交换机数据:", data);

  try {
    const url = id ? `/api/switches/${id}` : "/api/switches";
    const method = id ? "PUT" : "POST";

    const response = await fetch(url, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(data),
    });

    const result = await response.json();

    if (result.success) {
      closeModal("switch-modal");
      loadSwitchesData();
      showToast(id ? "交换机更新成功" : "交换机添加成功", "success");
    } else {
      showToast(`操作失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "提交交换机表单失败");
  }
}

// 编辑交换机
async function editSwitch(id) {
  try {
    const result = await apiGet(`/api/switches/${id}`);
    if (result.success) {
      openSwitchModal(result.data);
    } else {
      showToast("获取交换机信息失败", "error");
    }
  } catch (error) {
    handleError(error, "获取交换机信息失败");
  }
}

// 删除交换机
async function deleteSwitch(id) {
  if (!confirm("确定要删除该交换机吗？相关端口也将被删除。")) return;
  await handleDelete(id, "/api/switches", "交换机删除成功", loadSwitchesData);
}

// 管理交换机端口
let currentSwitchId = null;
let currentSwitchName = null;

async function manageSwitchPorts(switchId, switchName) {
  currentSwitchId = switchId;
  currentSwitchName = switchName;

  try {
    // 检查交换机SNMP配置
    const switchResult = await apiGet(`/api/switches/${switchId}`);
    if (!switchResult.success) {
      showToast("获取交换机信息失败，请检查网络连接", "error");
      return;
    }

    const switchData = switchResult.data;
    // 检查SNMP配置是否完整
    const hasSnmpConfig = switchData.snmp_community || switchData.snmp_username;
    if (!hasSnmpConfig) {
      showToast("该交换机未配置SNMP信息，无法获取端口数据", "warning");
      // 仍然允许显示端口，但显示提示
    }

    // 加载该交换机的端口
    const portsResult = await apiGet(`/api/switches/${switchId}/ports`);
    if (!portsResult.success) {
      showToast("获取端口数据失败", "error");
      return;
    }

    const ports = portsResult.data;
    
    // 分组端口
    const portGroups = groupPorts(ports);
    
    // 显示端口分组模态框
    showPortGroupsModal(switchName, portGroups, switchId);
  } catch (error) {
    console.error("管理交换机端口失败:", error);
    showToast("操作失败，请重试", "error");
  }
}

// 端口分组逻辑
function groupPorts(ports) {
  const groups = {};
  
  ports.forEach(port => {
    const portNumber = port.port_number;
    let groupKey = "其他";
    
    // 按端口类型和前缀分组
    if (portNumber.includes("Bridge-Aggregation")) {
      // Bridge-Aggregation1-Bridge-Aggregation10 分为Bridge-Aggregation组
      groupKey = "Bridge-Aggregation";
    } else if (portNumber.includes("GigabitEthernet")) {
      // GigabitEthernet1/1/0/1-GigabitEthernet1/1/0/10分为GigabitEthernet1组
      // GigabitEthernet2/1/0/1-GigabitEthernet2/1/0/10分为GigabitEthernet2组
      const match = portNumber.match(/GigabitEthernet(\d+)/);
      if (match) {
        groupKey = `GigabitEthernet${match[1]}`;
      } else {
        groupKey = "GigabitEthernet";
      }
    } else if (portNumber.includes("Ten-GigabitEthernet")) {
      // 处理带连字符的Ten-GigabitEthernet格式
      const match = portNumber.match(/Ten-GigabitEthernet(\d+)/);
      if (match) {
        groupKey = `Ten-GigabitEthernet${match[1]}`;
      } else {
        groupKey = "Ten-GigabitEthernet";
      }
    } else if (portNumber.includes("TenGigabitEthernet")) {
      // 处理不带连字符的TenGigabitEthernet格式
      const match = portNumber.match(/TenGigabitEthernet(\d+)/);
      if (match) {
        groupKey = `TenGigabitEthernet${match[1]}`;
      } else {
        groupKey = "TenGigabitEthernet";
      }
    } else if (portNumber.includes("Vlan-interface")) {
      // Vlan-interface2-Vlan-interface25分为Vlan-interface组
      groupKey = "Vlan-interface";
    } else if (portNumber.includes("FastEthernet")) {
      const match = portNumber.match(/FastEthernet(\d+)/);
      if (match) {
        groupKey = `FastEthernet${match[1]}`;
      } else {
        groupKey = "FastEthernet";
      }
    }
    
    if (!groups[groupKey]) {
      groups[groupKey] = [];
    }
    groups[groupKey].push(port);
  });
  
  return groups;
}

// 显示端口分组模态框
function showPortGroupsModal(switchName, portGroups, switchId) {
  const modal = document.getElementById("switch-ports-group-modal");
  const title = document.getElementById("switch-ports-group-modal-title");
  const container = document.querySelector(".port-groups-container");
  const addPortFormContainer = document.querySelector(".add-port-form-container");
  const addPortBtn = document.getElementById("add-port-btn");
  const cancelAddPortBtn = document.getElementById("cancel-add-port-btn");
  const cancelAddPortBtn2 = document.getElementById("cancel-add-port-btn-2");
  const switchPortFormExpanded = document.getElementById("switch-port-form-expanded");
  
  // 设置模态框标题
  title.textContent = `${switchName} - 端口分组显示`;
  
  // 清空容器
  container.innerHTML = "";
  
  // 显示分组端口
  Object.entries(portGroups).forEach(([groupName, ports]) => {
    const groupElement = document.createElement("div");
    groupElement.className = "port-group";
    
    const groupTitle = document.createElement("h4");
    groupTitle.textContent = `${groupName} (${ports.length})`;
    groupElement.appendChild(groupTitle);
    
    const portGrid = document.createElement("div");
    portGrid.className = "port-grid";
    
    // 排序端口
    ports.sort((a, b) => {
      const aNum = extractPortNumber(a.port_number);
      const bNum = extractPortNumber(b.port_number);
      return aNum - bNum;
    });
    
    // 添加端口项
    ports.forEach(port => {
      const portItem = document.createElement("div");
      portItem.className = `port-item status-${port.status}`;
      
      // 显示端口号最后一个数字
      const portDisplayNum = extractPortLastNumber(port.port_number);
      portItem.textContent = portDisplayNum;
      
      // 添加 tooltip
      const tooltip = document.createElement("div");
      tooltip.className = "port-tooltip";
      tooltip.textContent = port.port_number;
      portItem.appendChild(tooltip);
      
      // 点击端口打开编辑表单
      portItem.addEventListener("click", () => {
        // 显示扩展表单
        addPortFormContainer.style.display = "block";
        // 填充表单数据
        document.getElementById("switch-port-id-expanded").value = port.id;
        document.getElementById("switch-port-switch-id-expanded").value = port.switch_id;
        document.getElementById("switch-port-number-expanded").value = port.port_number;
        document.getElementById("switch-port-name-expanded").value = port.port_name || "";
        document.getElementById("switch-port-type-expanded").value = port.port_type || "access";
        document.getElementById("switch-port-vlan-expanded").value = port.vlan_id || "";
        document.getElementById("switch-port-status-expanded").value = port.status || "up";
        document.getElementById("switch-port-speed-expanded").value = port.speed || "";
        document.getElementById("switch-port-description-expanded").value = port.description || "";
      });
      
      portGrid.appendChild(portItem);
    });
    
    groupElement.appendChild(portGrid);
    container.appendChild(groupElement);
  });
  
  // 绑定按钮事件
  addPortBtn.addEventListener("click", () => {
    // 显示添加端口表单
    addPortFormContainer.style.display = "block";
    // 重置表单
    switchPortFormExpanded.reset();
    document.getElementById("switch-port-id-expanded").value = "";
    document.getElementById("switch-port-switch-id-expanded").value = switchId;
  });
  
  // 取消添加端口
  const cancelAddPort = () => {
    addPortFormContainer.style.display = "none";
  };
  
  cancelAddPortBtn.addEventListener("click", cancelAddPort);
  cancelAddPortBtn2.addEventListener("click", cancelAddPort);
  
  // 提交扩展表单
  switchPortFormExpanded.addEventListener("submit", async (e) => {
    e.preventDefault();
    await submitSwitchPortForm();
  });
  
  const getSnmpPortsBtn = document.getElementById("get-snmp-ports-btn");
  if (getSnmpPortsBtn) {
    getSnmpPortsBtn.addEventListener("click", () => {
      getSwitchPortsFromSnmp(switchId);
    });
  }
  
  // 打开模态框
  openModal("switch-ports-group-modal");
}

// 提取端口号中的数字部分
function extractPortNumber(portNumber) {
  const match = portNumber.match(/\d+/g);
  if (match) {
    return parseInt(match[match.length - 1]) || 0;
  }
  return 0;
}

// 提取端口号最后一个数字
function extractPortLastNumber(portNumber) {
  const match = portNumber.match(/\d+/g);
  if (match) {
    return match[match.length - 1];
  }
  return portNumber;
}

// 加载指定交换机的端口数据
async function loadSwitchPortsData(switchId) {
  try {
    const result = await apiGet(`/api/switches/${switchId}/ports`);

    // 添加"添加端口"和"从SNMP获取端口"按钮
    const searchContainer = document.querySelector(
      "#switch-ports-list-tab .search-container",
    );
    const actionButtonsContainer = searchContainer.parentElement.querySelector(".action-buttons") || 
      (() => {
        const container = document.createElement("div");
        container.className = "action-buttons";
        searchContainer.parentElement.appendChild(container);
        return container;
      })();
    
    // 确保"添加端口"按钮存在
  let addBtn = document.getElementById("add-switch-port-btn");
  if (!addBtn) {
    addBtn = document.createElement("button");
    addBtn.id = "add-switch-port-btn";
    addBtn.className = "btn btn-primary btn-sm";
    addBtn.textContent = "添加端口";
    addBtn.addEventListener("click", () => openSwitchPortModal(null, currentSwitchId));
    actionButtonsContainer.appendChild(addBtn);
  }
  
  // 添加"从SNMP获取端口"按钮
  let snmpPortsBtn = document.getElementById("get-snmp-ports-btn");
  if (!snmpPortsBtn) {
    snmpPortsBtn = document.createElement("button");
    snmpPortsBtn.id = "get-snmp-ports-btn";
    snmpPortsBtn.className = "btn btn-secondary btn-sm";
    snmpPortsBtn.textContent = "从SNMP获取端口";
    snmpPortsBtn.addEventListener("click", () => getSwitchPortsFromSnmp(currentSwitchId));
    actionButtonsContainer.appendChild(snmpPortsBtn);
  }

    renderTable(
      "#switch-ports-table",
      result.success ? result.data : [],
      (port) => `
                <td>${currentSwitchName}</td>
                <td>${port.port_number}</td>
                <td>${port.port_name || "-"}</td>
                <td>${port.port_type}</td>
                <td>${port.vlan_id || "-"}</td>
                <td><span class="status-badge ${port.status === "up" ? "status-active" : "status-inactive"}">${port.status}</span></td>
                <td>${port.speed || "-"}</td>
                <td>
                    <button class="btn btn-sm btn-edit switch-port-edit" data-id="${port.id}">编辑</button>
                    <button class="btn btn-sm btn-delete switch-port-delete" data-id="${port.id}">删除</button>
                </td>
            `,
      "暂无端口数据",
      8,
    );
  } catch (error) {
    console.error("加载端口数据失败:", error);
    renderTable("#switch-ports-table", [], () => "", "加载失败", 8);
  }
}


// 提交端口表单
async function submitSwitchPortForm() {
  const token = getAccessToken();
  if (!token) {
    showToast("请先登录", "warning");
    return;
  }

  // 使用工具函数获取扩展表单数据
  const id = getElementValue("switch-port-id-expanded");
  const switchId = getElementValue("switch-port-switch-id-expanded");
  const portNumber = getElementValue("switch-port-number-expanded", "trimmed");
  const portName = getElementValue("switch-port-name-expanded", "trimmed");
  const portType = getElementValue("switch-port-type-expanded");
  const vlanId = getElementValue("switch-port-vlan-expanded");
  const status = getElementValue("switch-port-status-expanded");
  const speed = getElementValue("switch-port-speed-expanded", "trimmed");
  const description = getElementValue("switch-port-description-expanded", "trimmed");

  // 验证必填字段
  if (!portNumber) {
    showToast("请填写端口号", "warning");
    return;
  }

  // 构建数据对象
  const data = {
    port_number: portNumber,
    port_name: portName || null,
    port_type: portType || "access",
    vlan_id: vlanId ? parseInt(vlanId) : null,
    status: status || "up",
    speed: speed || null,
    description: description || null,
  };

  try {
    // 构建URL和方法
    const url = id 
      ? `/api/switches/ports/${id}` 
      : `/api/switches/${switchId}/ports`;
    const method = id ? "PUT" : "POST";

    const response = await fetch(url, {
      method,
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify(data),
    });

    const result = await response.json();
    if (result.success) {
      // 隐藏扩展表单
      document.querySelector(".add-port-form-container").style.display = "none";
      // 重新加载当前交换机的端口数据
      if (currentSwitchId) {
        loadSwitchPortsData(currentSwitchId);
        // 刷新端口分组显示
        if (currentSwitchName) {
          const portsResult = await apiGet(`/api/switches/${currentSwitchId}/ports`);
          if (portsResult.success && portsResult.data) {
            const portGroups = groupPorts(portsResult.data);
            showPortGroupsModal(currentSwitchName, portGroups, currentSwitchId);
          }
        }
      }
      showToast(id ? "端口更新成功" : "端口添加成功", "success");
    } else {
      const errorMsg = result.message ?? "操作失败，请检查输入信息";
      showToast(`操作失败: ${errorMsg}`, "error");
      console.error("服务器返回错误:", result);
    }
  } catch (error) {
    handleError(error, "提交端口表单失败");
  }
}

// 打开交换机端口模态框
function openSwitchPortModal(portData = null, switchId = null) {
  const modal = document.getElementById("switch-port-modal");
  const title = document.getElementById("switch-port-modal-title");
  const form = document.getElementById("switch-port-form");

  if (!modal) {
    console.warn("交换机端口模态框未找到");
    return;
  }

  // 重置表单
  form.reset();

  if (portData) {
    // 编辑模式
    title.textContent = "编辑端口";
    document.getElementById("switch-port-id").value = portData.id || "";
    document.getElementById("switch-port-switch-id").value = portData.switch_id || switchId || "";
    document.getElementById("switch-port-number").value = portData.port_number || "";
    document.getElementById("switch-port-name").value = portData.port_name || "";
    document.getElementById("switch-port-type").value = portData.port_type || "access";
    document.getElementById("switch-port-vlan").value = portData.vlan_id || "";
    document.getElementById("switch-port-status").value = portData.status || "up";
    document.getElementById("switch-port-speed").value = portData.speed || "";
    document.getElementById("switch-port-description").value = portData.description || "";
  } else {
    // 添加模式
    title.textContent = "添加端口";
    document.getElementById("switch-port-id").value = "";
    document.getElementById("switch-port-switch-id").value = switchId || "";
    document.getElementById("switch-port-status").value = "up";
    document.getElementById("switch-port-type").value = "access";
  }

  openModal("switch-port-modal");
}

// 编辑端口
async function editSwitchPort(id) {
  try {
    const result = await apiGet(`/api/switches/ports/${id}`);
    if (result.success) {
      openSwitchPortModal(result.data);
    } else {
      showToast("获取端口信息失败", "error");
    }
  } catch (error) {
    handleError(error, "获取端口信息失败");
  }
}

// 删除端口
async function deleteSwitchPort(id) {
  const successCallback = () => {
    if (currentSwitchId) {
      loadSwitchPortsData(currentSwitchId);
    } else {
      loadAllSwitchPortsData();
    }
  };
  await handleDelete(id, "/api/switches/ports", "端口删除成功", successCallback);
}

// 测试SNMP连接
async function testSnmpConnection() {
  const token = getAccessToken();
  if (!token) {
    showToast("请先登录", "warning");
    return;
  }

  // 收集IP地址信息
  const ipRows = document.querySelectorAll(".switch-ip-row");
  if (ipRows.length === 0) {
    showToast("请先添加IP地址", "warning");
    return;
  }

  // 使用第一个IP地址作为测试
  const firstRow = ipRows[0];
  const networkRegionSelect = firstRow.querySelector(".switch-ip-network-region-select");
  const networkSelect = firstRow.querySelector(".switch-ip-network-select");
  
  if (!networkRegionSelect || !networkSelect || !networkRegionSelect.value || !networkSelect.value) {
    showToast("请先选择网络区域和网络", "warning");
    return;
  }

  const btn = document.getElementById("test-snmp-btn");
  const originalText = btn.textContent;
  btn.textContent = "测试中...";
  btn.disabled = true;

  const id = document.getElementById("switch-id").value;
  const networkRegionId = networkRegionSelect.value;
  const networkId = networkSelect.value;

  const data = {
    switch_id: id || null,
    network_region_id: networkRegionId,
    network_id: networkId,
    snmp_version: document.getElementById("switch-snmp-version").value,
    snmp_port:
      parseInt(document.getElementById("switch-snmp-port").value) || 161,
    snmp_community:
      document.getElementById("switch-snmp-community").value || null,
    snmp_username:
      document.getElementById("switch-snmp-username").value || null,
    snmp_auth_protocol:
      document.getElementById("switch-snmp-auth-protocol").value || null,
    snmp_auth_password:
      document.getElementById("switch-snmp-auth-password").value || null,
    snmp_priv_protocol:
      document.getElementById("switch-snmp-priv-protocol").value || null,
    snmp_priv_password:
      document.getElementById("switch-snmp-priv-password").value || null,
  };

  try {
    const response = await fetch(
      id ? `/api/switches/${id}/test-snmp` : "/api/switches/test-snmp",
      {
        method: "POST",
        headers: {
          Authorization: `Bearer ${token}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify(data),
      },
    );

    const result = await response.json();

    if (result.success) {
      showToast("SNMP连接成功", "success");
      
      // 尝试获取并更新交换机型号和厂商信息
      try {
        let infoResult;
        if (id) {
          // 如果是编辑模式，使用已有交换机的SNMP信息
          infoResult = await apiGet(`/api/switches/${id}/snmp-info`);
        } else {
          // 如果是添加模式，先创建临时交换机以获取SNMP信息
          const createResult = await apiPost(`/api/switches`, data);
          if (createResult.success) {
            infoResult = await apiGet(`/api/switches/${createResult.data.id}/snmp-info`);
            // 删除临时交换机
            await apiDelete(`/api/switches/${createResult.data.id}`);
          }
        }

        if (infoResult && infoResult.success) {
          const { vendor, model } = infoResult.data;
          
          // 检查是否已有手动添加的内容，提示是否覆盖
          const currentVendor = document.getElementById("switch-vendor").value;
          const currentModel = document.getElementById("switch-model").value;
          
          if (currentVendor || currentModel) {
            if (confirm("已存在手动添加的交换机信息，是否覆盖?")) {
              document.getElementById("switch-vendor").value = vendor;
              document.getElementById("switch-model").value = model;
              showToast("交换机信息已更新", "success");
            }
          } else {
            document.getElementById("switch-vendor").value = vendor;
            document.getElementById("switch-model").value = model;
            showToast("交换机信息已更新", "success");
          }
        }
      } catch (infoError) {
        console.error("获取交换机详细信息失败:", infoError);
        // 不影响SNMP测试结果的显示
      }
    } else {
      showToast("SNMP连接失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("SNMP测试失败:", error);
    showToast("SNMP测试失败，请检查网络连接", "error");
  } finally {
    btn.textContent = originalText;
    btn.disabled = false;
  }
}

// 查看ARP表
async function viewArpTable(switchId) {
  const token = getAccessToken();
  if (!token) {
    showToast("请先登录", "warning");
    return;
  }

  try {
    // 检查交换机SNMP配置
    const switchResult = await apiGet(`/api/switches/${switchId}`);
    if (!switchResult.success) {
      showToast("获取交换机信息失败，请检查网络连接", "error");
      return;
    }

    const switchData = switchResult.data;
    // 检查SNMP配置是否完整
    const hasSnmpConfig = switchData.snmp_community || switchData.snmp_username;
    if (!hasSnmpConfig) {
      showToast("该交换机未配置SNMP信息，无法获取ARP表数据", "warning");
      return;
    }

    const result = await apiGet(`/api/switches/${switchId}/arp-table`);

    if (result.success) {
      const entries = result.data || [];
      if (entries.length === 0) {
        showToast("ARP表为空或无法获取", "info");
        return;
      }

      let tableHtml = '<table style="width:100%; border-collapse: collapse;">';
      tableHtml +=
        '<tr><th style="border:1px solid #ccc; padding:8px;">IP地址</th><th style="border:1px solid #ccc; padding:8px;">MAC地址</th></tr>';

      entries.forEach((entry) => {
        tableHtml += `<tr><td style="border:1px solid #ccc; padding:8px;">${entry.ip_address}</td><td style="border:1px solid #ccc; padding:8px;">${entry.mac_address}</td></tr>`;
      });

      tableHtml += "</table>";

      // 创建弹窗显示
      const modal = document.createElement("div");
      modal.className = "modal active";
      modal.innerHTML = `
                <div class="modal-content" style="max-width: 600px;">
                    <div class="modal-header">
                        <h3>ARP表 (共${entries.length}条)</h3>
                        <span class="close arp-modal-close">&times;</span>
                    </div>
                    <div class="modal-body" style="max-height: 400px; overflow-y: auto;">
                        ${tableHtml}
                    </div>
                    <div class="modal-footer">
                        <button class="btn btn-secondary arp-modal-close">关闭</button>
                    </div>
                </div>
            `;
      document.body.appendChild(modal);
    } else {
      showToast("获取ARP表失败: " + (result.message || "未知错误"), "error");
    }
  } catch (error) {
    console.error("获取ARP表失败:", error);
    showToast("获取ARP表失败，请检查网络连接和SNMP配置", "error");
  }
}

// 初始化交换机管理标签页
function initSwitchTabs() {
  const switchesContainer = document.getElementById("switches");
  if (!switchesContainer) return;

  if (switchesContainer.dataset.tabsInitialized === "true") return;

  const tabBtns = switchesContainer.querySelectorAll(".tab-btn");
  const tabContents = switchesContainer.querySelectorAll(".tab-content");

  tabBtns.forEach((btn) => {
    btn.addEventListener("click", function () {
      const tabId = this.getAttribute("data-tab");

      tabBtns.forEach((b) => b.classList.remove("active"));
      this.classList.add("active");

      tabContents.forEach((content) => content.classList.remove("active"));
      document.getElementById(`${tabId}-tab`).classList.add("active");

      if (tabId === "switches-list") {
        loadSwitchesData();
      } else if (tabId === "switch-ports-list") {
        loadAllSwitchPortsData();
      }
    });
  });

  switchesContainer.dataset.tabsInitialized = "true";
}

// 防抖处理的交换机筛选函数
const debouncedFilterSwitchesData = debounce(filterSwitchesData, 300);

// 初始化交换机搜索功能
function initSwitchSearch() {
  const searchContainer = document.getElementById("switches-list-tab");
  if (!searchContainer || searchContainer.dataset.searchInitialized === "true") return;

  const searchInput = document.getElementById("switch-search");
  const filterBtn = document.getElementById("switch-filter-btn");
  const refreshBtn = document.getElementById("switch-refresh-btn");

  if (filterBtn) {
    filterBtn.addEventListener("click", filterSwitchesData);
  }

  if (refreshBtn) {
    refreshBtn.addEventListener("click", loadSwitchesData);
  }

  if (searchInput) {
    searchInput.addEventListener("input", function () {
      debouncedFilterSwitchesData();
    });
    searchInput.addEventListener("keypress", function (e) {
      if (e.key === "Enter") {
        filterSwitchesData();
      }
    });
  }

  const portSearchInput = document.getElementById("switch-port-search");
  const portFilterBtn = document.getElementById("switch-port-filter-btn");
  const portRefreshBtn = document.getElementById("switch-port-refresh-btn");

  if (portFilterBtn) {
    portFilterBtn.addEventListener("click", filterSwitchPortsData);
  }

  if (portRefreshBtn) {
    portRefreshBtn.addEventListener("click", () => {
      if (currentSwitchId) {
        loadSwitchPortsData(currentSwitchId);
      } else {
        loadAllSwitchPortsData();
      }
    });
  }

  if (portSearchInput) {
    portSearchInput.addEventListener("keypress", function (e) {
      if (e.key === "Enter") {
        filterSwitchPortsData();
      }
    });
  }

  searchContainer.dataset.searchInitialized = "true";
}

// 筛选交换机数据
async function filterSwitchesData() {
  const searchTerm = document
    .getElementById("switch-search")
    .value.toLowerCase();

  try {
    const result = await apiGet("/api/switches");
    if (!result.success) return;

    const filteredData = result.data.filter((sw) => {
      return (
        sw.name.toLowerCase().includes(searchTerm) ||
        sw.ip_address.toLowerCase().includes(searchTerm) ||
        (sw.model && sw.model.toLowerCase().includes(searchTerm)) ||
        (sw.vendor && sw.vendor.toLowerCase().includes(searchTerm)) ||
        (sw.location && sw.location.toLowerCase().includes(searchTerm))
      );
    });

    renderTable(
      "#switches-table",
      filteredData,
      (sw) => `
                <td>${sw.name}</td>
                <td>${sw.device_type || "-"}</td>
                <td>${sw.ip_address}</td>
                <td>${sw.mac_address || "-"}</td>
                <td>${sw.model || "-"}</td>
                <td>${sw.vendor || "-"}</td>
                <td>${sw.location || "-"}</td>
                <td><span class="status-badge ${sw.snmp_community || sw.snmp_username ? "status-active" : "status-inactive"}">${sw.snmp_version || "-"}</span></td>
                <td>${sw.parent_switch_name || "-"}</td>
                <td>
                    <button class="btn btn-sm btn-edit" data-id="${sw.id}">编辑</button>
                    <button class="btn btn-sm btn-secondary btn-switch-ports" data-switch-id="${sw.id}" data-switch-name="${sw.name}">端口</button>
                    <button class="btn btn-sm btn-secondary btn-switch-arp" data-switch-id="${sw.id}">ARP表</button>
                    <button class="btn btn-sm btn-delete" data-id="${sw.id}">删除</button>
                </td>
            `,
      "无匹配数据",
      10,
    );
  } catch (error) {
    console.error("筛选交换机数据失败:", error);
  }
}

// 筛选端口数据
async function filterSwitchPortsData() {
  const searchTerm = document
    .getElementById("switch-port-search")
    .value.toLowerCase();

  try {
    const result = currentSwitchId
      ? await apiGet(`/api/switches/${currentSwitchId}/ports`)
      : await apiGet("/api/switches/ports");

    if (!result.success) return;

    const filteredData = result.data.filter((port) => {
      return (
        port.port_number.toLowerCase().includes(searchTerm) ||
        (port.port_name && port.port_name.toLowerCase().includes(searchTerm)) ||
        (port.switch_name &&
          port.switch_name.toLowerCase().includes(searchTerm))
      );
    });

    renderTable(
      "#switch-ports-table",
      filteredData,
      (port) => `
                <td>${port.switch_name || currentSwitchName}</td>
                <td>${port.port_number}</td>
                <td>${port.port_name || "-"}</td>
                <td>${port.port_type}</td>
                <td>${port.vlan_id || "-"}</td>
                <td><span class="status-badge ${port.status === "up" ? "status-active" : "status-inactive"}">${port.status}</span></td>
                <td>${port.speed || "-"}</td>
                <td>
                    <button class="btn btn-sm btn-edit switch-port-edit" data-id="${port.id}">编辑</button>
                    <button class="btn btn-sm btn-delete switch-port-delete" data-id="${port.id}">删除</button>
                </td>
            `,
      "无匹配数据",
      8,
    );
  } catch (error) {
    console.error("筛选端口数据失败:", error);
  }
}

// 初始化交换机功能
function initSwitches() {
  initSwitchTabs();
  initSwitchSearch();

  const snmpVersionSelect = document.getElementById("switch-snmp-version");
  if (snmpVersionSelect) {
    snmpVersionSelect.addEventListener("change", toggleSnmpConfig);
  }

  bindSwitchAddIpButton();

  const testSnmpBtn = document.getElementById("test-snmp-btn");
  if (testSnmpBtn) {
    testSnmpBtn.addEventListener("click", testSnmpConnection);
  }

  // ARP模态框关闭按钮事件委托
  document.addEventListener("click", (e) => {
    if (e.target.classList.contains("arp-modal-close")) {
      const modal = e.target.closest(".modal");
      if (modal) {
        modal.remove();
      }
    }
  });
}

// 导出所有必要的函数
export {
  initSwitches,
  loadSwitchesData,
  bindSwitchButtonsEvents,
  loadAllSwitchPortsData,
  openSwitchModal,
  openSwitchPortModal,
  editSwitch,
  deleteSwitch,
  deleteSwitchPort,
  submitSwitchForm,
  initSwitchTabs,
  initSwitchSearch,
  manageSwitchPorts,
  viewArpTable,
  submitSwitchPortForm,
  editSwitchPort
};
