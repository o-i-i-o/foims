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

export const DEVICE_PORT_PAGE_SIZE = 50;

const devicePortState = {
  currentDeviceId: null,
  currentDeviceName: null,
};

export function setCurrentDeviceId(id) {
  devicePortState.currentDeviceId = id;
}

export function getCurrentDeviceId() {
  return devicePortState.currentDeviceId;
}

export function setCurrentDeviceName(name) {
  devicePortState.currentDeviceName = name;
}

export function getCurrentDeviceName() {
  return devicePortState.currentDeviceName;
}

async function loadDevicePortsData(page = 1, searchTerm = "") {
  const currentDeviceName = getCurrentDeviceName();
  try {
    const url = `/api/resources/devices/device-ports?page=${page}&page_size=${DEVICE_PORT_PAGE_SIZE}&search=${encodeURIComponent(searchTerm)}`;
    const result = await apiGet(url);
    const data = result.success ? result.data : { items: [], total: 0 };
    const ports = data.items || data;

    renderTable("#device-ports-table", {
      data: ports,
      columns: [
        { field: 'device_name', render: (v, row) => v ? `${v} (${row.device_ip || ''})` : currentDeviceName },
        { field: 'port_number', render: (v) => v },
        { field: 'port_name', render: (v) => v || '-' },
        { field: 'port_type', render: (v) => v },
        { field: 'vlan_id', render: (v) => v || '-' },
        { field: 'status', render: (v) => `<span class="status-badge ${v === 'up' ? 'status-active' : 'status-inactive'}">${v}</span>` },
        { field: 'speed', render: (v) => v || '-' },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit device-port-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-delete device-port-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('device.no_ports')
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#device-ports-table", data, (p) => loadDevicePortsData(p, searchTerm));
    }
  } catch (error) {
    handleError(error, t('device.load_ports_failed'), () => {
      renderTable("#device-ports-table", { data: [], columns: [], emptyMessage: t('common.load_failed_retry') });
    });
  }
}

async function loadDevicePortsByDeviceId(deviceId) {
  try {
    const result = await apiGet(`/api/resources/devices/${deviceId}/device-ports?page_size=1000`);

    const searchContainer = document.querySelector(
      "#device-ports-list-tab .search-container",
    );

    if (!searchContainer) {
      renderTable("#device-ports-table", {
        data: result.success ? result.data : [],
        columns: [
          { field: 'port_number', render: (v) => v },
          { field: 'port_name', render: (v) => v || '-' },
          { field: 'port_type', render: (v) => v },
          { field: 'vlan_id', render: (v) => v || '-' },
          { field: 'status', render: (v) => `<span class="status-badge ${v === 'up' ? 'status-active' : 'status-inactive'}">${v}</span>` },
          { field: 'speed', render: (v) => v || '-' },
          { field: 'id', render: (v) => `
            <button class="btn btn-sm btn-edit device-port-edit" data-id="${v}">${t('common.edit')}</button>
            <button class="btn btn-sm btn-delete device-port-delete" data-id="${v}">${t('common.delete')}</button>
          ` }
        ],
        emptyMessage: t('device.no_ports')
      });
      return;
    }

    const actionButtonsContainer = searchContainer.parentElement.querySelector(".action-buttons") ||
      (() => {
        const container = document.createElement("div");
        container.className = "action-buttons";
        searchContainer.parentElement.appendChild(container);
        return container;
      })();

    let addBtn = elementCache.get("add-device-port-btn");
    if (!addBtn) {
      addBtn = document.createElement("button");
      addBtn.id = "add-device-port-btn";
      addBtn.className = "btn btn-primary btn-sm";
      addBtn.textContent = t('device.add_port');
      addBtn.addEventListener("click", () => openDevicePortModal(null, getCurrentDeviceId()));
      actionButtonsContainer.appendChild(addBtn);
    }

    let snmpPortsBtn = elementCache.get("sync-snmp-ports-btn");
    if (!snmpPortsBtn) {
      snmpPortsBtn = document.createElement("button");
      snmpPortsBtn.id = "sync-snmp-ports-btn";
      snmpPortsBtn.className = "btn btn-secondary btn-sm";
      snmpPortsBtn.textContent = t('device.sync_ports_from_snmp');
      snmpPortsBtn.addEventListener("click", () => syncPortsFromSnmp(getCurrentDeviceId()));
      actionButtonsContainer.appendChild(snmpPortsBtn);
    }

    renderTable("#device-ports-table", {
      data: result.success ? result.data : [],
      columns: [
        { field: 'port_number', render: (v) => v },
        { field: 'port_name', render: (v) => v || '-' },
        { field: 'port_type', render: (v) => v },
        { field: 'vlan_id', render: (v) => v || '-' },
        { field: 'status', render: (v) => `<span class="status-badge ${v === 'up' ? 'status-active' : 'status-inactive'}">${v}</span>` },
        { field: 'speed', render: (v) => v || '-' },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit device-port-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-delete device-port-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('device.no_ports')
    });
  } catch (error) {
    renderTable("#device-ports-table", { data: [], columns: [], emptyMessage: t('common.load_failed_retry') });
  }
}

async function manageDevicePorts(deviceId, deviceName) {
  setCurrentDeviceId(deviceId);
  setCurrentDeviceName(deviceName);

  try {
    const deviceResult = await apiGet(`/api/resources/devices/${deviceId}`);
    if (!deviceResult.success) {
      showToast(t('device.load_failed'), "error");
      return;
    }

    const deviceData = deviceResult.data;
    const hasSnmpConfig = deviceData.snmp_community || deviceData.snmp_username;
    if (!hasSnmpConfig) {
      showToast(t('device.no_snmp_config'), "warning");
    }

    const portsResult = await apiGet(`/api/resources/devices/${deviceId}/device-ports?page_size=1000`);
    if (!portsResult.success) {
      showToast(t('device.load_ports_failed'), "error");
      return;
    }

    let ports = [];
    if (portsResult.data) {
      if (Array.isArray(portsResult.data)) {
        ports = portsResult.data;
      } else if (portsResult.data.items && Array.isArray(portsResult.data.items)) {
        ports = portsResult.data.items;
      }
    }

    const portGroups = groupPorts(ports);
    await showPortGroupsModal(deviceName, portGroups, deviceId);
  } catch (error) {
    console.error("管理设备端口失败:", error);
    showToast(t('common.operation_failed_retry'), "error");
  }
}

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
    { typeName: "Ethernet", regex: /(Ethernet)(\d+)/, subGroup: true }
  ];

  ports.forEach(port => {
    const portNumber = port.port_number;
    let groupKey = t('device.port_group_other');

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
    const otherKey = t('device.port_group_other');
    if (finalGroups[otherKey]) {
      finalGroups[otherKey].push(...smallGroupPorts);
    } else {
      finalGroups[otherKey] = smallGroupPorts;
    }
  }

  return finalGroups;
}

async function showPortGroupsModal(deviceName, portGroups, deviceId) {
  await openModal("device-ports-group-modal");

  const modal = elementCache.get("device-ports-group-modal");
  const title = elementCache.get("device-ports-group-modal-title");
  const container = document.querySelector(".port-groups-container");
  const addPortBtn = elementCache.get("add-port-btn");

  if (!modal || !title || !container || !addPortBtn) {
    console.error("端口分组模态框相关DOM元素未找到", { modal, title, container, addPortBtn });
    return;
  }

  title.textContent = `${deviceName} - ${t('device.port_groups')}`;
  container.innerHTML = "";

  Object.entries(portGroups).forEach(([groupName, ports]) => {
    const groupElement = document.createElement("div");
    groupElement.className = "port-group";

    const groupTitle = document.createElement("h4");
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

      portGrid.appendChild(portItem);
    });

    groupElement.appendChild(portGrid);
    container.appendChild(groupElement);
  });

  container.onclick = (e) => {
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
        description: portItem.dataset.description
      });
    }
  };

  addPortBtn.onclick = () => {
    openPortDetailModal({
      portId: "",
      deviceId: deviceId,
      portNumber: "",
      portName: "",
      portType: "access",
      vlanId: "",
      status: "up",
      speed: "",
      description: ""
    });
  };

  const getSnmpPortsBtn = elementCache.get("get-snmp-ports-btn");
  if (getSnmpPortsBtn) {
    getSnmpPortsBtn.onclick = async () => {
      const ports = await syncPortsFromSnmp(deviceId);
      if (ports && ports.length > 0) {
        const portGroups = groupPorts(ports);
        showPortGroupsModal(getCurrentDeviceName() || "", portGroups, deviceId);
      }
    };
  }
}

async function openPortDetailModal(portData) {
  await openModal("device-port-detail-modal");

  const modal = elementCache.get("device-port-detail-modal");
  const title = elementCache.get("device-port-detail-modal-title");
  const form = elementCache.get("device-port-form-expanded");
  const saveBtn = elementCache.get("save-port-btn");
  const deleteBtn = elementCache.get("delete-port-btn");

  if (!modal || !form) {
    console.error("端口详情模态框相关DOM元素未找到");
    return;
  }

  const isNewPort = !portData.portId;
  title.textContent = isNewPort ? t('device.add_port') : `${t('device.port_detail')} - ${portData.portNumber}`;

  elementCache.setValue("device-port-id-expanded", portData.portId || "");
  elementCache.setValue("device-port-device-id-expanded", portData.deviceId || "");
  elementCache.setValue("device-port-number-expanded", portData.portNumber || "");
  elementCache.setValue("device-port-name-expanded", portData.portName || "");
  elementCache.setValue("device-port-type-expanded", portData.portType || "access");
  elementCache.setValue("device-port-vlan-expanded", portData.vlanId || "");
  elementCache.setValue("device-port-status-expanded", portData.status || "up");
  elementCache.setValue("device-port-speed-expanded", portData.speed || "");
  elementCache.setValue("device-port-description-expanded", portData.description || "");

  deleteBtn.style.display = isNewPort ? "none" : "inline-block";

  saveBtn.onclick = async () => {
    await submitDevicePortForm();
  };

  deleteBtn.onclick = async () => {
    const confirmed = await showConfirm(t('device.confirm_delete_port'));
    if (confirmed) {
      const portId = elementCache.getValue("device-port-id-expanded");
      if (portId) {
        const result = await apiDelete(`/api/resources/devices/device-ports/${portId}`);
        if (result.success) {
          showToast(t('device.port_delete_success'), "success");
          closeModal("device-port-detail-modal");
          const currentDeviceId = getCurrentDeviceId();
          if (currentDeviceId) {
            manageDevicePorts(currentDeviceId, getCurrentDeviceName() || "");
          }
        } else {
          showToast(t('device.port_delete_failed') + ': ' + result.message, "error");
        }
      }
    }
  };
}

async function openDevicePortModal(portData = null, deviceId = null) {
  if (portData) {
    await openPortDetailModal({
      portId: portData.id || "",
      deviceId: portData.device_id || deviceId || "",
      portNumber: portData.port_number || "",
      portName: portData.port_name || "",
      portType: portData.port_type || "access",
      vlanId: portData.vlan_id || "",
      status: portData.status || "up",
      speed: portData.speed || "",
      description: portData.description || ""
    });
  } else {
    await openPortDetailModal({
      portId: "",
      deviceId: deviceId || "",
      portNumber: "",
      portName: "",
      portType: "access",
      vlanId: "",
      status: "up",
      speed: "",
      description: ""
    });
  }
}

async function submitDevicePortForm() {
  const id = getElementValue("device-port-id-expanded");
  const deviceId = getElementValue("device-port-device-id-expanded");
  const portNumber = getElementValue("device-port-number-expanded");
  const portName = getElementValue("device-port-name-expanded");
  const portType = getElementValue("device-port-type-expanded");
  const vlanId = getElementValue("device-port-vlan-expanded");
  const status = getElementValue("device-port-status-expanded");
  const speed = getElementValue("device-port-speed-expanded");
  const description = getElementValue("device-port-description-expanded");

  if (!portNumber) {
    showToast(t('device.port_number_required'), "warning");
    return;
  }

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
    let result;
    if (id) {
      result = await apiPut(`/api/resources/devices/device-ports/${id}`, data);
    } else {
      result = await apiPost(`/api/resources/devices/${deviceId}/device-ports`, data);
    }

    if (result.success) {
      closeModal("device-port-detail-modal");
      showToast(id ? t('device.port_update_success') : t('device.port_add_success'), "success");
      const currentDeviceId = getCurrentDeviceId();
      if (currentDeviceId) {
        try {
          await loadDevicePortsByDeviceId(currentDeviceId);
          const currentDeviceName = getCurrentDeviceName();
          if (currentDeviceName) {
            const portsResult = await apiGet(`/api/resources/devices/${currentDeviceId}/device-ports?page_size=1000`);
            if (portsResult.success && portsResult.data) {
              let ports = [];
              if (Array.isArray(portsResult.data)) {
                ports = portsResult.data;
              } else if (portsResult.data.items && Array.isArray(portsResult.data.items)) {
                ports = portsResult.data.items;
              }
              const portGroups = groupPorts(ports);
              await showPortGroupsModal(currentDeviceName, portGroups, currentDeviceId);
            }
          }
        } catch (refreshError) {
          console.error("刷新端口数据失败:", refreshError);
          showToast(t('device.port_save_refresh_failed'), "warning");
        }
      }
    } else {
      const errorMsg = result.message ?? t('common.operation_failed');
      showToast(`${t('common.operation_failed')}: ${errorMsg}`, "error");
      console.error("服务器返回错误:", result);
    }
  } catch (error) {
    handleError(error, t('device.port_submit_failed'));
  }
}

async function deleteDevicePort(id) {
  const currentDeviceId = getCurrentDeviceId();
  const successCallback = () => {
    if (currentDeviceId) {
      loadDevicePortsByDeviceId(currentDeviceId);
    } else {
      loadDevicePortsData(1, "");
    }
  };
  await handleDelete(id, "/api/resources/devices/device-ports", t('device.port_delete_success'), successCallback);
}

async function syncPortsFromSnmp(deviceId) {
  const getPortsBtn = elementCache.get("get-snmp-ports-btn");
  const originalText = getPortsBtn?.textContent || t('device.sync_ports_from_snmp');
  if (getPortsBtn) {
    getPortsBtn.disabled = true;
    getPortsBtn.textContent = t('common.syncing');
  }

  try {
    const result = await apiPost(`/api/resources/devices/${deviceId}/device-ports/sync-snmp`, {});

    if (result.success) {
      showToast(result.message || t('device.port_sync_success'), "success");
      return result.data || [];
    } else {
      showToast(t('device.port_sync_failed') + ': ' + result.message, "error");
      return [];
    }
  } catch (error) {
    showToast(t('device.port_sync_failed'), "error");
    return [];
  } finally {
    if (getPortsBtn) {
      getPortsBtn.disabled = false;
      getPortsBtn.textContent = originalText;
    }
  }
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
  loadDevicePortsData,
  loadDevicePortsByDeviceId,
  manageDevicePorts,
  openDevicePortModal,
  deleteDevicePort,
  submitDevicePortForm,
  groupPorts,
  showPortGroupsModal,
  syncPortsFromSnmp
};
