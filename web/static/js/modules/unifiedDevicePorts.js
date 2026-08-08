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
  handleDelete,
  handleError,
  appendPaginationToTable,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";
import { t } from "../utils/i18n.js";
import { getNetworkCardManager } from "../utils/networkCardManager.js";

const PORT_PAGE_SIZE = 50;

const devicePortState = {
  currentDeviceId: null,
  currentDeviceName: null,
  currentDeviceType: null,
  isNetworkDevice: false,
};

function setCurrentDevice(device) {
  devicePortState.currentDeviceId = device.id;
  devicePortState.currentDeviceName = device.name;
  devicePortState.currentDeviceType = device.device_type;
  // 判断是否为网络设备（交换机、网络安全设备等）
  devicePortState.isNetworkDevice = ['switch', 'network_device'].includes(device.device_type);
}

function getCurrentDevice() {
  return {
    id: devicePortState.currentDeviceId,
    name: devicePortState.currentDeviceName,
    type: devicePortState.currentDeviceType,
    isNetworkDevice: devicePortState.isNetworkDevice,
  };
}

/**
 * 统一的设备端口管理入口
 * @param {string} deviceId - 设备ID
 * @param {string} deviceName - 设备名称
 */
export async function manageUnifiedDevicePorts(deviceId, deviceName) {
  try {
    // 获取设备完整信息
    const deviceResult = await apiGet(`/api/resources/devices/${deviceId}`);
    if (!deviceResult.success) {
      showToast(t('device.load_failed') || "获取设备信息失败", "error");
      return;
    }

    setCurrentDevice(deviceResult.data);
    const device = getCurrentDevice();

    // 打开统一端口管理模态框
    await openModal("unified-device-ports-modal");

    const title = elementCache.get("unified-device-ports-modal-title");
    if (title) {
      title.textContent = `${deviceName} - ${t('device.unified_ports') || '设备端口'}`;
    }

    // 根据设备类型显示不同的端口结构
    if (device.isNetworkDevice) {
      await showNetworkDevicePorts(device);
    } else {
      await showNormalDevicePorts(device);
    }
  } catch (error) {
    handleError(error, t('device.port_management_failed') || "端口管理失败");
  }
}

/**
 * 显示普通设备的端口（按网卡分组）
 */
async function showNormalDevicePorts(device) {
  const normalSection = document.getElementById("normal-device-ports-section");
  const networkSection = document.getElementById("network-device-ports-section");

  if (normalSection) normalSection.style.display = "block";
  if (networkSection) networkSection.style.display = "none";

  // 加载设备的网卡和接口配置
  await loadDeviceNicInterfaces(device.id);
}

/**
 * 显示网络设备的端口（分板块显示2层和3层）
 */
async function showNetworkDevicePorts(device) {
  const normalSection = document.getElementById("normal-device-ports-section");
  const networkSection = document.getElementById("network-device-ports-section");

  if (normalSection) normalSection.style.display = "none";
  if (networkSection) networkSection.style.display = "block";

  // 并行加载3层口和2层口
  await Promise.all([
    loadDeviceNicInterfaces(device.id),
    loadSwitchPorts(device.id),
  ]);

  // 绑定按钮事件
  bindNetworkDeviceButtons(device.id);
}

/**
 * 加载设备的网卡接口（3层口）
 */
async function loadDeviceNicInterfaces(deviceId) {
  try {
    const result = await apiGet(`/api/resources/devices/${deviceId}/nics`);
    const container = document.querySelector(".nic-interfaces-container");

    if (!container) return;

    if (!result.success || !result.data) {
      container.innerHTML = `<p class="empty-message">${t('device.no_nic_interfaces') || '暂无网卡接口数据'}</p>`;
      return;
    }

    const cards = Array.isArray(result.data) ? result.data : [];

    // 使用网卡管理器渲染（按网卡分组）
    const cardManager = getNetworkCardManager();
    await cardManager.loadExisting(cards);
  } catch (error) {
    console.error("加载网卡接口失败:", error);
    const container = document.querySelector(".nic-interfaces-container");
    if (container) {
      container.innerHTML = `<p class="error-message">${t('common.load_failed_retry') || '加载失败，请重试'}</p>`;
    }
  }
}

/**
 * 加载交换机端口（2层口）
 */
async function loadSwitchPorts(deviceId) {
  try {
    const result = await apiGet(`/api/resources/devices/${deviceId}/switch-ports?page_size=1000`);
    const container = document.querySelector(".switch-ports-container");

    if (!container) return;

    if (!result.success || !result.data) {
      container.innerHTML = `<p class="empty-message">${t('device.no_switch_ports') || '暂无交换机端口数据'}</p>`;
      return;
    }

    const ports = Array.isArray(result.data) ? result.data : (result.data.items || []);

    // 使用现有的分组逻辑
    const portGroups = groupPorts(ports);
    renderSwitchPortGroups(container, portGroups, deviceId);
  } catch (error) {
    console.error("加载交换机端口失败:", error);
    const container = document.querySelector(".switch-ports-container");
    if (container) {
      container.innerHTML = `<p class="error-message">${t('common.load_failed_retry') || '加载失败，请重试'}</p>`;
    }
  }
}

/**
 * 渲染交换机端口分组（2层口）
 */
function renderSwitchPortGroups(container, portGroups, deviceId) {
  container.innerHTML = "";

  Object.entries(portGroups).forEach(([groupName, ports]) => {
    const groupElement = document.createElement("div");
    groupElement.className = "port-group";

    const groupTitle = document.createElement("h5");
    groupTitle.className = "port-group-title";
    groupTitle.textContent = `${groupName} (${ports.length})`;
    groupElement.appendChild(groupTitle);

    const portGrid = document.createElement("div");
    portGrid.className = "port-grid";

    ports.sort((a, b) => {
      const aNum = extractPortNumber(a.port_number);
      const bNum = extractPortNumber(b.port_number);
      return aNum - bNum;
    });

    ports.forEach(port => {
      const portItem = createPortItem(port);
      portGrid.appendChild(portItem);
    });

    groupElement.appendChild(portGrid);
    container.appendChild(groupElement);
  });
}

/**
 * 创建端口项元素
 */
function createPortItem(port) {
  const portItem = document.createElement("div");
  portItem.className = `viz-port-item status-${port.status}`;
  portItem.dataset.portId = port.id;
  portItem.dataset.deviceId = port.device_id;
  portItem.dataset.portNumber = port.port_number;
  portItem.dataset.portName = port.port_name || "";
  portItem.dataset.portType = port.port_type || "access";
  portItem.dataset.vlanId = port.vlan_id || "";
  portItem.dataset.status = port.status || "up";
  portItem.dataset.speed = port.speed || "";
  portItem.dataset.description = port.description || "";

  const portDisplayNum = extractPortLastNumber(port.port_number);
  portItem.textContent = portDisplayNum;

  const tooltip = document.createElement("div");
  tooltip.className = "port-tooltip";
  tooltip.textContent = port.port_number;
  portItem.appendChild(tooltip);

  return portItem;
}

/**
 * 绑定网络设备专用按钮事件
 */
function bindNetworkDeviceButtons(deviceId) {
  // 添加网卡接口按钮
  const addNicBtn = document.getElementById("add-nic-interface-btn");
  if (addNicBtn) {
    addNicBtn.onclick = () => {
      // TODO: 打开网卡接口添加模态框
      showToast("添加网卡接口功能待实现", "info");
    };
  }

  // 添加交换机端口按钮
  const addSwitchPortBtn = document.getElementById("add-switch-port-btn");
  if (addSwitchPortBtn) {
    addSwitchPortBtn.onclick = () => {
      // TODO: 打开交换机端口添加模态框
      showToast("添加交换机端口功能待实现", "info");
    };
  }

  // 从SNMP获取端口按钮
  const syncSnmpBtn = document.getElementById("sync-snmp-ports-btn");
  if (syncSnmpBtn) {
    syncSnmpBtn.onclick = async () => {
      await syncPortsFromSnmp(deviceId);
    };
  }

  // 端口项点击事件
  const switchPortsContainer = document.querySelector(".switch-ports-container");
  if (switchPortsContainer) {
    switchPortsContainer.onclick = (e) => {
      const portItem = e.target.closest(".viz-port-item");
      if (portItem) {
        openPortDetailModal({
          portId: portItem.dataset.portId,
          deviceId: portItem.dataset.deviceId,
          portNumber: portItem.dataset.portNumber,
          portName: portItem.dataset.portName,
          portType: portItem.dataset.portType,
          vlanId: portItem.dataset.vlanId,
          status: portItem.dataset.status,
          speed: portItem.dataset.speed,
          description: portItem.dataset.description,
        });
      }
    };
  }
}

/**
 * 从SNMP同步端口
 */
async function syncPortsFromSnmp(deviceId) {
  const syncBtn = document.getElementById("sync-snmp-ports-btn");
  const originalText = syncBtn?.textContent;

  try {
    if (syncBtn) {
      syncBtn.disabled = true;
      syncBtn.textContent = t('common.syncing') || "同步中...";
    }

    const result = await apiPost(`/api/resources/devices/${deviceId}/switch-ports/sync-snmp`, {});

    if (result.success) {
      showToast(result.message || t('device.port_sync_success') || '端口同步成功', "success");
      // 刷新2层口显示
      await loadSwitchPorts(deviceId);
    } else {
      showToast((t('device.port_sync_failed') || '端口同步失败') + ': ' + result.message, "error");
    }
  } catch (error) {
    handleError(error, t('device.port_sync_failed') || '端口同步失败');
  } finally {
    if (syncBtn) {
      syncBtn.disabled = false;
      syncBtn.textContent = originalText;
    }
  }
}

/**
 * 打开端口详情模态框
 */
async function openPortDetailModal(portData) {
  await openModal("device-port-detail-modal");

  const title = elementCache.get("device-port-detail-modal-title");
  const isNewPort = !portData.portId;

  if (title) {
    title.textContent = isNewPort
      ? (t('device.add_port') || "新增端口")
      : `${t('device.port_detail') || '端口详情'} - ${portData.portNumber}`;
  }

  // 填充表单数据
  elementCache.setValue("device-port-id-expanded", portData.portId || "");
  elementCache.setValue("device-port-device-id-expanded", portData.deviceId || "");
  elementCache.setValue("device-port-number-expanded", portData.portNumber || "");
  elementCache.setValue("device-port-name-expanded", portData.portName || "");
  elementCache.setValue("device-port-type-expanded", portData.portType || "access");
  elementCache.setValue("device-port-vlan-expanded", portData.vlanId || "");
  elementCache.setValue("device-port-status-expanded", portData.status || "up");
  elementCache.setValue("device-port-speed-expanded", portData.speed || "");
  elementCache.setValue("device-port-description-expanded", portData.description || "");

  // TODO: 绑定保存和删除事件
}

/**
 * 端口分组逻辑（从devicePorts.js复用）
 */
function groupPorts(ports) {
  const MIN_GROUP_SIZE = 3;
  const rawGroups = {};

  const portTypePatterns = [
    { typeName: "Bridge-Aggregation", regex: /(Bridge-Aggregation)/, subGroup: false },
    { typeName: "Hundred-GigabitEthernet", regex: /(Hundred-?GigabitEthernet)(\d+)/i, subGroup: true },
    { typeName: "Forty-GigabitEthernet", regex: /(Forty-?GigabitEthernet)(\d+)/i, subGroup: true },
    { typeName: "Ten-GigabitEthernet", regex: /(Ten-GigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "TenGigabitEthernet", regex: /(TenGigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "XGigabitEthernet", regex: /(XGigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "M-GigabitEthernet", regex: /(M-GigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "GigabitEthernet", regex: /(GigabitEthernet)(\d+)/, subGroup: true },
    { typeName: "Vlan-interface", regex: /(Vlan-interface)/, subGroup: false },
    { typeName: "FastEthernet", regex: /(FastEthernet)(\d+)/, subGroup: true },
    { typeName: "Ethernet", regex: /(Ethernet)(\d+)/, subGroup: true },
  ];

  ports.forEach(port => {
    const portNumber = port.port_number;
    let groupKey = t('device.port_group_other') || "其他";

    for (const { regex, subGroup } of portTypePatterns) {
      const match = portNumber.match(regex);
      if (match) {
        if (subGroup && match[2]) {
          groupKey = `${match[1]}${match[2]}`;
        } else {
          groupKey = match[1];
        }
        break;
      }
    }

    if (!rawGroups[groupKey]) {
      rawGroups[groupKey] = [];
    }
    rawGroups[groupKey].push(port);
  });

  const finalGroups = {};
  const smallGroupPorts = [];

  Object.entries(rawGroups).forEach(([groupName, groupPorts]) => {
    if (groupPorts.length >= MIN_GROUP_SIZE) {
      finalGroups[groupName] = groupPorts;
    } else {
      smallGroupPorts.push(...groupPorts);
    }
  });

  if (smallGroupPorts.length > 0) {
    const otherKey = t('device.port_group_other') || "其他";
    if (finalGroups[otherKey]) {
      finalGroups[otherKey].push(...smallGroupPorts);
    } else {
      finalGroups[otherKey] = smallGroupPorts;
    }
  }

  return finalGroups;
}

function extractPortNumber(portNumber) {
  if (typeof portNumber !== 'string' || !portNumber) return 0;
  const match = portNumber.match(/\d+/g);
  if (match) {
    return parseInt(match[match.length - 1]) || 0;
  }
  return 0;
}

function extractPortLastNumber(portNumber) {
  if (typeof portNumber !== 'string' || !portNumber) return portNumber || '';
  const match = portNumber.match(/\d+/g);
  if (match) {
    return match[match.length - 1];
  }
  return portNumber;
}

export {
  loadDeviceNicInterfaces,
  loadSwitchPorts,
  groupPorts,
};