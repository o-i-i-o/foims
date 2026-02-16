// ==================== 交换机管理功能 ====================

// 导入必要的模块
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
  debounce,
  handleDelete,
  handleError
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";

import {
  IpConfigManager,
  getManager
} from "../utils/ipconfig.js";

import { elementCache } from "../utils/helpers.js";

const SWITCH_FORM_FIELDS = [
  'switch-id', 'switch-name', 'switch-model', 'switch-vendor',
  'switch-location', 'switch-description', 'switch-snmp-version',
  'switch-snmp-port', 'switch-snmp-community', 'switch-snmp-username',
  'switch-snmp-auth-protocol', 'switch-snmp-auth-password',
  'switch-snmp-priv-protocol', 'switch-snmp-priv-password'
];

function getSwitchFormValues() {
  return {
    id: elementCache.getValue('switch-id'),
    name: elementCache.getValue('switch-name'),
    model: elementCache.getValue('switch-model') || null,
    vendor: elementCache.getValue('switch-vendor') || null,
    location: elementCache.getValue('switch-location') || null,
    description: elementCache.getValue('switch-description') || null,
    snmp_version: elementCache.getValue('switch-snmp-version'),
    snmp_port: parseInt(elementCache.getValue('switch-snmp-port')) || 161,
    snmp_community: elementCache.getValue('switch-snmp-community') || null,
    snmp_username: elementCache.getValue('switch-snmp-username') || null,
    snmp_auth_protocol: elementCache.getValue('switch-snmp-auth-protocol') || null,
    snmp_auth_password: elementCache.getValue('switch-snmp-auth-password') || null,
    snmp_priv_protocol: elementCache.getValue('switch-snmp-priv-protocol') || null,
    snmp_priv_password: elementCache.getValue('switch-snmp-priv-password') || null
  };
}

function setSwitchFormValues(sw) {
  elementCache.setValue('switch-id', sw.id || '');
  elementCache.setValue('switch-name', sw.name || '');
  elementCache.setValue('switch-model', sw.model || '');
  elementCache.setValue('switch-vendor', sw.vendor || '');
  elementCache.setValue('switch-location', sw.location || '');
  elementCache.setValue('switch-description', sw.description || '');
  elementCache.setValue('switch-snmp-version', sw.snmp_version || 'v2c');
  elementCache.setValue('switch-snmp-port', sw.snmp_port || 161);
  elementCache.setValue('switch-snmp-community', sw.snmp_community || '');
  elementCache.setValue('switch-snmp-username', sw.snmp_username || '');
  elementCache.setValue('switch-snmp-auth-protocol', sw.snmp_auth_protocol || '');
  elementCache.setValue('switch-snmp-auth-password', sw.snmp_auth_password || '');
  elementCache.setValue('switch-snmp-priv-protocol', sw.snmp_priv_protocol || '');
  elementCache.setValue('switch-snmp-priv-password', sw.snmp_priv_password || '');
}

function resetSwitchForm() {
  const form = elementCache.get('switch-form');
  if (form) form.reset();
  elementCache.setValue('switch-id', '');
  elementCache.setValue('switch-snmp-port', '161');
}

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
  elementCache.clear();
  
  const modal = elementCache.get('switch-modal');
  const title = elementCache.get('switch-modal-title');
  const form = elementCache.get('switch-form');

  if (sw) {
    title.textContent = "编辑交换机";
  } else {
    title.textContent = "添加交换机";
  }

  const ipManager = getManager('switch');
  
  if (sw && sw.id) {
    ipManager.setExcludeSwitchId(sw.id);
  } else {
    ipManager.setExcludeSwitchId(null);
  }
  
  ipManager.clear();
  
  let ipsToLoad = [];
  if (sw && sw.ips && sw.ips.length > 0) {
    ipsToLoad = sw.ips.map((ip, index) => {
         if (index === 0) {
             return {
                 ...ip,
                 parent_switch_id: sw.parent_switch_id,
                 parent_port_id: sw.parent_port_id
             };
         }
         return ip;
    });
  } else if (sw && sw.parent_switch_id) {
    ipsToLoad = [{
        parent_switch_id: sw.parent_switch_id,
        parent_port_id: sw.parent_port_id
    }];
  }
  
  if (ipsToLoad.length > 0) {
      await ipManager.loadIps(ipsToLoad);
  } else {
      await ipManager.addIpRow();
  }

  if (sw) {
    setSwitchFormValues(sw);
  } else {
    resetSwitchForm();
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
  const formValues = getSwitchFormValues();
  
  const ipManager = getManager('switch');
  const ips = ipManager.getIps();
  
  let parentSwitchId = null;
  let parentPortId = null;
  
  const firstIpRow = document.querySelector(".switch-ip-row");
  if (firstIpRow) {
    const switchSelect = firstIpRow.querySelector(".switch-switch-select");
    const portSelect = firstIpRow.querySelector(".switch-switch-port-select");
    if (switchSelect) parentSwitchId = switchSelect.value || null;
    if (portSelect) parentPortId = portSelect.value || null;
  }
  
  const validIps = ips.filter(ip => ip.ip_address && ip.ip_address.trim());
  
  const data = {
    name: formValues.name,
    model: formValues.model,
    vendor: formValues.vendor,
    location: formValues.location,
    description: formValues.description,
    snmp_version: formValues.snmp_version,
    snmp_port: formValues.snmp_port,
    snmp_community: formValues.snmp_community,
    snmp_username: formValues.snmp_username,
    snmp_auth_protocol: formValues.snmp_auth_protocol,
    snmp_auth_password: formValues.snmp_auth_password,
    snmp_priv_protocol: formValues.snmp_priv_protocol,
    snmp_priv_password: formValues.snmp_priv_password,
    parent_switch_id: parentSwitchId,
    parent_port_id: parentPortId,
    ips: validIps.length > 0 ? validIps : null
  };

  if (!data.name) {
    showToast("请填写交换机名称", "warning");
    return;
  }

  if (validIps.length === 0) {
    showToast("请至少配置一个IP地址", "warning");
    return;
  }

  try {
    if (formValues.id) {
      var result = await apiPut(`/api/switches/${formValues.id}`, data);
    } else {
      var result = await apiPost("/api/switches", data);
    }

    if (result.success) {
      closeModal("switch-modal");
      loadSwitchesData();
      showToast(formValues.id ? "交换机更新成功" : "交换机添加成功", "success");
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

    const searchContainer = document.querySelector(
      "#switch-ports-list-tab .search-container",
    );
    
    if (!searchContainer) {
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
      return;
    }
    
    const actionButtonsContainer = searchContainer.parentElement.querySelector(".action-buttons") || 
      (() => {
        const container = document.createElement("div");
        container.className = "action-buttons";
        searchContainer.parentElement.appendChild(container);
        return container;
      })();
    
  let addBtn = document.getElementById("add-switch-port-btn");
  if (!addBtn) {
    addBtn = document.createElement("button");
    addBtn.id = "add-switch-port-btn";
    addBtn.className = "btn btn-primary btn-sm";
    addBtn.textContent = "添加端口";
    addBtn.addEventListener("click", () => openSwitchPortModal(null, currentSwitchId));
    actionButtonsContainer.appendChild(addBtn);
  }
  
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
    renderTable("#switch-ports-table", [], () => "", "加载失败", 8);
  }
}


// 提交端口表单
async function submitSwitchPortForm() {
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
    if (id) {
      var result = await apiPut(`/api/switches/ports/${id}`, data);
    } else {
      var result = await apiPost(`/api/switches/${switchId}/ports`, data);
    }

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
  const ipRows = document.querySelectorAll(".switch-ip-row");
  if (ipRows.length === 0) {
    showToast("请先添加IP地址", "warning");
    return;
  }

  const firstRow = ipRows[0];
  const networkRegionSelect = firstRow.querySelector(".switch-ip-network-region-select");
  const networkSelect = firstRow.querySelector(".switch-ip-network-select");
  
  if (!networkRegionSelect || !networkSelect || !networkRegionSelect.value || !networkSelect.value) {
    showToast("请先选择网络区域和网络", "warning");
    return;
  }

  const btn = elementCache.get('test-snmp-btn');
  const originalText = btn.textContent;
  btn.textContent = "测试中...";
  btn.disabled = true;

  const formValues = getSwitchFormValues();
  const networkRegionId = networkRegionSelect.value;
  const networkId = networkSelect.value;

  const data = {
    switch_id: formValues.id || null,
    network_region_id: networkRegionId,
    network_id: networkId,
    snmp_version: formValues.snmp_version,
    snmp_port: formValues.snmp_port,
    snmp_community: formValues.snmp_community,
    snmp_username: formValues.snmp_username,
    snmp_auth_protocol: formValues.snmp_auth_protocol,
    snmp_auth_password: formValues.snmp_auth_password,
    snmp_priv_protocol: formValues.snmp_priv_protocol,
    snmp_priv_password: formValues.snmp_priv_password,
  };

  try {
    const result = await apiPost(
      formValues.id ? `/api/switches/${formValues.id}/test-snmp` : "/api/switches/test-snmp",
      data
    );

    if (result.success) {
      showToast("SNMP连接成功", "success");
      
      try {
        let infoResult;
        if (formValues.id) {
          infoResult = await apiGet(`/api/switches/${formValues.id}/snmp-info`);
        } else {
          const createResult = await apiPost(`/api/switches`, data);
          if (createResult.success) {
            infoResult = await apiGet(`/api/switches/${createResult.data.id}/snmp-info`);
            await apiDelete(`/api/switches/${createResult.data.id}`);
          }
        }

        if (infoResult && infoResult.success) {
          const { vendor, model } = infoResult.data;
          
          const currentVendor = elementCache.getValue('switch-vendor');
          const currentModel = elementCache.getValue('switch-model');
          
          if (currentVendor || currentModel) {
            if (confirm("已存在手动添加的交换机信息，是否覆盖?")) {
              elementCache.setValue('switch-vendor', vendor);
              elementCache.setValue('switch-model', model);
              showToast("交换机信息已更新", "success");
            }
          } else {
            elementCache.setValue('switch-vendor', vendor);
            elementCache.setValue('switch-model', model);
            showToast("交换机信息已更新", "success");
          }
        }
      } catch (infoError) {
      }
    } else {
      showToast("SNMP连接失败: " + result.message, "error");
    }
  } catch (error) {
    showToast("SNMP测试失败，请检查网络连接", "error");
  } finally {
    btn.textContent = originalText;
    btn.disabled = false;
  }
}

// 查看ARP表
async function viewArpTable(switchId) {
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
