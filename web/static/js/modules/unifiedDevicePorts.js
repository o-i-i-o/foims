import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";

import {
  showToast,
  handleError,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";
import { t } from "../utils/i18n.js";

/**
 * 设备端口管理（以 2026年5月版 device-ports-group-modal 布局为准）
 *
 * 组织结构：
 *   - 设备模态框手动管理的网口（NIC 接口）→ 渲染为一个或多个端口分组（蓝色调）
 *   - 端口模态框自身管理的二层端口（Switch 端口）→ 按端口名前缀分组（橙/绿色调）
 *   - 两类分组共用 .port-group / .port-item 样式，统一显示在 .port-groups-container 内
 *
 * 功能：
 *   - 添加端口（二层）、端口详情编辑/删除
 *   - 从 SNMP 拉取端口（含端口名称冲突二次确认：覆盖/跳过/全部覆盖/全部跳过）
 */

const devicePortState = {
  currentDeviceId: null,
  currentDeviceName: null,
  currentDeviceType: null,
  isNetworkDevice: false,
};

// SNMP 同步过程中待处理的冲突信息（供冲突对话框使用）
const conflictState = {
  newPorts: [],          // SNMP 拉取到的全部端口
  existingMap: new Map(),// 已存在端口：key = port_number（小写）
  decisions: {},         // { portNumber: 'overwrite' | 'skip' }
};

function setCurrentDevice(device) {
  devicePortState.currentDeviceId = device.id;
  devicePortState.currentDeviceName = device.name;
  devicePortState.currentDeviceType = device.device_type;
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
    // /nics 同时返回 device_type 和 cards（网卡→网口→IP 树）
    const result = await apiGet(`/api/resources/devices/${deviceId}/nics`);
    if (!result.success || !result.data) {
      showToast(t('device.load_failed') || "获取设备信息失败", "error");
      return;
    }

    const { device_type, cards } = result.data;
    setCurrentDevice({ id: deviceId, name: deviceName, device_type });
    const device = getCurrentDevice();

    // 打开端口分组模态框（5月版布局）
    await openModal("unified-device-ports-modal");

    const title = elementCache.get("unified-device-ports-modal-title");
    if (title) {
      title.textContent = `${deviceName} - ${t('device.unified_ports') || '设备端口'}`;
    }

    // 渲染：先把 NIC 作为分组渲染，网络设备再追加二层端口分组
    await renderAllPortGroups(device, cards);

    // 绑定底部按钮（添加端口 / 从SNMP获取 / 端口项点击）
    bindModalButtons(device.id);
  } catch (error) {
    handleError(error, t('device.port_management_failed') || "端口管理失败");
  }
}

/**
 * 渲染所有端口分组：NIC 分组 + （网络设备）二层端口分组
 */
async function renderAllPortGroups(device, cards) {
  const container = document.querySelector(".port-groups-container");
  if (!container) return;
  container.innerHTML = "";

  // 1. NIC 分组（设备模态框管理的网口）
  renderNicPortGroups(container, cards);

  // 2. 二层端口分组（端口模态框自身管理）—— 仅网络设备
  if (device.isNetworkDevice) {
    await loadAndRenderDevicePorts(device.id, container);
  }

  // 若两类都为空，显示空提示
  if (!container.children.length) {
    container.innerHTML = `<p class="empty-message">${t('device.no_device_ports') || '暂无端口数据'}</p>`;
  }
}

/**
 * 把 NIC（网卡→网口）渲染为端口分组
 * 每张网卡 = 一个 .port-group.is-nic-group，网卡下的每个网口 = 一个 .port-item.nic-interface
 */
function renderNicPortGroups(container, cards) {
  if (!Array.isArray(cards) || cards.length === 0) return;

  cards.forEach(card => {
    const ports = Array.isArray(card.ports) ? card.ports : [];
    if (ports.length === 0) return;

    const groupElement = document.createElement("div");
    groupElement.className = "port-group is-nic-group";

    const groupTitle = document.createElement("h4");
    const cardName = card.name || t('device.network_card') || '网卡';
    groupTitle.textContent = `${cardName} (${ports.length})`;
    groupElement.appendChild(groupTitle);

    const portGrid = document.createElement("div");
    portGrid.className = "port-grid";

    ports.sort((a, b) => (a.sort_order ?? 0) - (b.sort_order ?? 0) || String(a.name).localeCompare(String(b.name)));

    ports.forEach(iface => {
      portGrid.appendChild(createNicPortItem(card, iface));
    });

    groupElement.appendChild(portGrid);
    container.appendChild(groupElement);
  });
}

/**
 * 创建 NIC 网口项（只读展示，点击提示由设备模态框管理）
 */
function createNicPortItem(card, iface) {
  const portItem = document.createElement("div");
  portItem.className = "port-item nic-interface";
  portItem.dataset.kind = "nic";
  portItem.dataset.portId = iface.id || "";
  portItem.dataset.portNumber = iface.name || "";
  portItem.dataset.portName = iface.name || "";

  // 显示网口名的末尾数字，无数字则显示首字母
  const display = extractPortLastNumber(iface.name) || (iface.name || "?").slice(0, 3);
  portItem.textContent = display;

  // tooltip：网口名 + MAC + IP
  const ipList = Array.isArray(iface.ips) ? iface.ips.map(i => i.ip_address).filter(Boolean) : [];
  const tipParts = [iface.name || ""];
  if (iface.mac_address) tipParts.push(`MAC: ${iface.mac_address}`);
  if (ipList.length) tipParts.push(`IP: ${ipList.join(', ')}`);

  const tooltip = document.createElement("div");
  tooltip.className = "port-tooltip";
  tooltip.textContent = tipParts.join(' | ');
  portItem.appendChild(tooltip);

  return portItem;
}

/**
 * 加载并渲染二层端口（按端口名前缀分组）
 */
async function loadAndRenderDevicePorts(deviceId, container) {
  try {
    const result = await apiGet(`/api/resources/devices/${deviceId}/device-ports?page_size=1000`);
    if (!result.success || !result.data) return;

    const ports = Array.isArray(result.data) ? result.data : (result.data.items || []);
    if (ports.length === 0) return;

    const portGroups = groupPorts(ports);
    renderDevicePortGroups(container, portGroups);
  } catch (error) {
    console.error("加载设备端口失败:", error);
  }
}

/**
 * 渲染二层端口分组（追加到 container）
 */
function renderDevicePortGroups(container, portGroups) {
  Object.entries(portGroups).forEach(([groupName, ports]) => {
    const groupElement = document.createElement("div");
    groupElement.className = "port-group is-switch-group";

    const groupTitle = document.createElement("h4");
    groupTitle.textContent = `${groupName} (${ports.length})`;
    groupElement.appendChild(groupTitle);

    const portGrid = document.createElement("div");
    portGrid.className = "port-grid";

    ports.sort((a, b) => extractPortNumber(a.port_number) - extractPortNumber(b.port_number));
    ports.forEach(port => portGrid.appendChild(createDevicePortItem(port)));

    groupElement.appendChild(portGrid);
    container.appendChild(groupElement);
  });
}

/**
 * 创建二层端口项（可点击编辑）
 */
function createDevicePortItem(port) {
  const portItem = document.createElement("div");
  portItem.className = `port-item status-${port.status}`;
  portItem.dataset.kind = "switch";
  portItem.dataset.portId = port.id;
  portItem.dataset.deviceId = port.device_id;
  portItem.dataset.portNumber = port.port_number;
  portItem.dataset.portName = port.port_name || "";
  portItem.dataset.portType = port.port_type || "access";
  portItem.dataset.vlanId = port.vlan_id || "";
  portItem.dataset.status = port.status || "up";
  portItem.dataset.speed = port.speed || "";
  portItem.dataset.description = port.description || "";

  portItem.textContent = extractPortLastNumber(port.port_number);

  const tooltip = document.createElement("div");
  tooltip.className = "port-tooltip";
  tooltip.textContent = port.port_number;
  portItem.appendChild(tooltip);

  return portItem;
}

/**
 * 绑定模态框底部按钮 + 端口项点击事件
 */
function bindModalButtons(deviceId) {
  // 添加端口 → 打开端口详情（新增）
  const addPortBtn = elementCache.get("add-port-btn");
  if (addPortBtn) {
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
        description: "",
      });
    };
  }

  // 从SNMP获取端口
  const getSnmpBtn = elementCache.get("get-snmp-ports-btn");
  if (getSnmpBtn) {
    getSnmpBtn.onclick = async () => {
      await startSnmpSync(deviceId);
    };
  }

  // 端口项点击：二层端口 → 编辑；NIC → 提示由设备模态框管理
  const container = document.querySelector(".port-groups-container");
  if (container) {
    container.onclick = (e) => {
      const portItem = e.target.closest(".port-item");
      if (!portItem) return;

      if (portItem.dataset.kind === "nic") {
        showToast(t('device.nic_managed_by_device_modal') || "网口由设备编辑模态框管理", "info");
        return;
      }

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
    };
  }
}

/**
 * 打开端口详情模态框（新增 / 编辑），并绑定保存/删除事件
 */
async function openPortDetailModal(portData) {
  await openModal("device-port-detail-modal");

  const title = elementCache.get("device-port-detail-modal-title");
  const saveBtn = elementCache.get("save-port-btn");
  const deleteBtn = elementCache.get("delete-port-btn");
  const isNewPort = !portData.portId;

  if (title) {
    title.textContent = isNewPort
      ? (t('device.add_port') || "新增端口")
      : `${t('device.port_detail') || '端口详情'} - ${portData.portNumber}`;
  }

  // 填充表单
  elementCache.setValue("device-port-id-expanded", portData.portId || "");
  elementCache.setValue("device-port-device-id-expanded", portData.deviceId || "");
  elementCache.setValue("device-port-number-expanded", portData.portNumber || "");
  elementCache.setValue("device-port-name-expanded", portData.portName || "");
  elementCache.setValue("device-port-type-expanded", portData.portType || "access");
  elementCache.setValue("device-port-vlan-expanded", portData.vlanId || "");
  elementCache.setValue("device-port-status-expanded", portData.status || "up");
  elementCache.setValue("device-port-speed-expanded", portData.speed || "");
  elementCache.setValue("device-port-description-expanded", portData.description || "");

  if (deleteBtn) {
    deleteBtn.style.display = isNewPort ? "none" : "inline-block";
  }
  if (saveBtn) {
    saveBtn.onclick = async () => { await submitPortForm(); };
  }
  if (deleteBtn) {
    deleteBtn.onclick = async () => { await deletePort(); };
  }
}

/**
 * 收集端口表单数据
 */
function collectPortFormData() {
  return {
    id: elementCache.getValue("device-port-id-expanded"),
    deviceId: elementCache.getValue("device-port-device-id-expanded"),
    port_number: elementCache.getValue("device-port-number-expanded"),
    port_name: elementCache.getValue("device-port-name-expanded") || null,
    port_type: elementCache.getValue("device-port-type-expanded") || "access",
    vlan_id: elementCache.getValue("device-port-vlan-expanded") || null,
    status: elementCache.getValue("device-port-status-expanded") || "up",
    speed: elementCache.getValue("device-port-speed-expanded") || null,
    description: elementCache.getValue("device-port-description-expanded") || null,
  };
}

/**
 * 提交端口表单（新增 / 更新），成功后刷新模态框
 */
async function submitPortForm() {
  const data = collectPortFormData();
  const { id, deviceId, port_number } = data;

  if (!port_number) {
    showToast(t('device.port_number_required') || "请填写端口号", "warning");
    return;
  }

  const payload = {
    port_number: data.port_number,
    port_name: data.port_name,
    port_type: data.port_type,
    vlan_id: data.vlan_id ? parseInt(data.vlan_id, 10) : null,
    status: data.status,
    speed: data.speed,
    description: data.description,
  };

  const saveBtn = elementCache.get("save-port-btn");
  const originalText = saveBtn?.textContent;
  if (saveBtn) {
    saveBtn.disabled = true;
    saveBtn.textContent = t('common.saving') || "保存中...";
  }

  try {
    let result;
    if (id) {
      result = await apiPut(`/api/resources/devices/device-ports/${id}`, payload);
    } else {
      result = await apiPost(`/api/resources/devices/${deviceId}/device-ports`, payload);
    }

    if (result.success) {
      closeModal("device-port-detail-modal");
      showToast(id ? (t('device.port_update_success') || "端口更新成功") : (t('device.port_add_success') || "端口添加成功"), "success");
      await refreshModalView(deviceId);
    } else {
      showToast((t('device.port_save_failed') || "端口保存失败") + ": " + (result.message || ""), "error");
    }
  } catch (error) {
    handleError(error, t('device.port_save_failed') || "端口保存失败");
  } finally {
    if (saveBtn) {
      saveBtn.disabled = false;
      saveBtn.textContent = originalText;
    }
  }
}

/**
 * 删除端口
 */
async function deletePort() {
  const data = collectPortFormData();
  const { id, deviceId } = data;
  if (!id) return;

  const confirmed = await showConfirm(t('device.confirm_delete_port') || "确定删除该端口吗？");
  if (!confirmed) return;

  const deleteBtn = elementCache.get("delete-port-btn");
  const originalText = deleteBtn?.textContent;
  if (deleteBtn) {
    deleteBtn.disabled = true;
    deleteBtn.textContent = t('common.deleting') || "删除中...";
  }

  try {
    const result = await apiDelete(`/api/resources/devices/device-ports/${id}`);
    if (result.success) {
      closeModal("device-port-detail-modal");
      showToast(t('device.port_delete_success') || "端口删除成功", "success");
      await refreshModalView(deviceId);
    } else {
      showToast((t('device.port_delete_failed') || "端口删除失败") + ": " + (result.message || ""), "error");
    }
  } catch (error) {
    handleError(error, t('device.port_delete_failed') || "端口删除失败");
  } finally {
    if (deleteBtn) {
      deleteBtn.disabled = false;
      deleteBtn.textContent = originalText;
    }
  }
}

/**
 * 刷新模态框内的端口分组视图（保存/删除/SNMP 同步后调用）
 */
async function refreshModalView(deviceId) {
  const device = getCurrentDevice();
  const result = await apiGet(`/api/resources/devices/${deviceId}/nics`);
  const cards = result.success && result.data ? (result.data.cards || []) : [];
  await renderAllPortGroups(device, cards);
  // 重新绑定（renderAllPortGroups 重置了 container.innerHTML，click 委托需重绑）
  bindModalButtons(deviceId);
}

// ============================================================
// SNMP 拉取 + 端口名称冲突处理（覆盖 / 跳过 / 全部覆盖 / 全部跳过）
// 流程：
//   1. GET /snmp-ports 拿到 SNMP 实时端口（不落库）
//   2. GET /device-ports 拿到现有端口
//   3. 对比 port_number（忽略大小写），找出冲突项
//   4. 若有冲突 → 弹出 port-conflict-modal 供用户逐项选择或一键全部
//   5. 按决策走 POST（新增）/ PUT（覆盖），跳过的项不处理
// ============================================================

async function startSnmpSync(deviceId) {
  const syncBtn = elementCache.get("get-snmp-ports-btn");
  const originalText = syncBtn?.textContent;

  try {
    if (syncBtn) {
      syncBtn.disabled = true;
      syncBtn.textContent = t('device.fetching_snmp') || "获取中...";
    }

    // 1. 拉取 SNMP 实时端口
    const snmpResult = await apiGet(`/api/resources/devices/${deviceId}/snmp-ports`);
    if (!snmpResult.success || !Array.isArray(snmpResult.data) || snmpResult.data.length === 0) {
      showToast(t('device.snmp_no_ports') || "未获取到端口信息", "warning");
      return;
    }
    const newPorts = snmpResult.data;

    // 2. 拉取现有端口
    const existingResult = await apiGet(`/api/resources/devices/${deviceId}/device-ports?page_size=1000`);
    const existingPorts = existingResult.success
      ? (Array.isArray(existingResult.data) ? existingResult.data : (existingResult.data?.items || []))
      : [];

    const existingMap = new Map();
    existingPorts.forEach(p => existingMap.set(String(p.port_number).toLowerCase(), p));

    // 3. 对比分类
    const toAdd = [];
    const conflicts = [];
    newPorts.forEach(p => {
      const key = String(p.port_number).toLowerCase();
      if (existingMap.has(key)) {
        conflicts.push({ snmpPort: p, existingPort: existingMap.get(key) });
      } else {
        toAdd.push(p);
      }
    });

    // 4. 无冲突 → 直接新增
    if (conflicts.length === 0) {
      await applySnmpResults(deviceId, toAdd, []);
      return;
    }

    // 5. 有冲突 → 弹出冲突对话框
    conflictState.newPorts = newPorts;
    conflictState.existingMap = existingMap;
    conflictState.decisions = {};
    conflicts.forEach(c => {
      conflictState.decisions[String(c.snmpPort.port_number)] = 'skip';
    });

    await showConflictModal(deviceId, toAdd, conflicts);
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
 * 显示端口冲突确认对话框
 */
async function showConflictModal(deviceId, toAdd, conflicts) {
  await openModal("port-conflict-modal");

  const summaryEl = document.getElementById("port-conflict-summary");
  const listEl = document.getElementById("port-conflict-list");

  if (summaryEl) {
    const tmpl = t('device.conflict_summary') || '检测到 {conflict} 个端口名称已存在，{add} 个为新端口。请选择冲突端口的处理方式。';
    summaryEl.textContent = tmpl
      .replace('{conflict}', conflicts.length)
      .replace('{add}', toAdd.length);
  }

  if (listEl) {
    listEl.innerHTML = "";
    conflicts.forEach(c => {
      const row = document.createElement("div");
      row.className = "port-conflict-row";

      const num = document.createElement("div");
      num.className = "conflict-port-number";
      num.textContent = c.snmpPort.port_number;

      const detail = document.createElement("div");
      detail.className = "conflict-port-detail";
      const oldName = c.existingPort.port_name || '-';
      const newName = c.snmpPort.port_name || '-';
      detail.textContent = `${t('device.conflict_existing') || '现名'}: ${oldName} → ${t('device.conflict_new') || '新名'}: ${newName}`;

      const select = document.createElement("select");
      select.className = "conflict-select";
      select.dataset.portNumber = c.snmpPort.port_number;
      select.innerHTML = `
        <option value="skip">${t('device.conflict_skip') || '跳过'}</option>
        <option value="overwrite">${t('device.conflict_overwrite') || '覆盖'}</option>
      `;
      select.value = conflictState.decisions[c.snmpPort.port_number] || 'skip';
      select.addEventListener("change", () => {
        conflictState.decisions[c.snmpPort.port_number] = select.value;
      });

      row.appendChild(num);
      row.appendChild(detail);
      row.appendChild(select);
      listEl.appendChild(row);
    });
  }

  // 全部跳过 / 全部覆盖
  const skipAllBtn = document.getElementById("conflict-skip-all-btn");
  const overwriteAllBtn = document.getElementById("conflict-overwrite-all-btn");
  if (skipAllBtn) {
    skipAllBtn.onclick = () => {
      conflicts.forEach(c => { conflictState.decisions[c.snmpPort.port_number] = 'skip'; });
      syncConflictSelections();
    };
  }
  if (overwriteAllBtn) {
    overwriteAllBtn.onclick = () => {
      conflicts.forEach(c => { conflictState.decisions[c.snmpPort.port_number] = 'overwrite'; });
      syncConflictSelections();
    };
  }

  // 确认 → 应用决策
  const confirmBtn = document.getElementById("conflict-confirm-btn");
  if (confirmBtn) {
    confirmBtn.onclick = async () => {
      const toOverwrite = conflicts
        .filter(c => conflictState.decisions[c.snmpPort.port_number] === 'overwrite')
        .map(c => c.snmpPort);
      closeModal("port-conflict-modal");
      await applySnmpResults(deviceId, toAdd, toOverwrite);
    };
  }
}

/**
 * 同步所有冲突行下拉框显示
 */
function syncConflictSelections() {
  document.querySelectorAll(".conflict-select").forEach(sel => {
    const pn = sel.dataset.portNumber;
    if (pn && conflictState.decisions[pn]) sel.value = conflictState.decisions[pn];
  });
}

/**
 * 应用 SNMP 同步结果
 * @param {string} deviceId
 * @param {Array} toAdd - 新增的端口
 * @param {Array} toOverwrite - 覆盖（更新）的端口
 */
async function applySnmpResults(deviceId, toAdd, toOverwrite) {
  let added = 0, overwritten = 0, failed = 0;

  const buildPayload = (p) => ({
    port_number: p.port_number,
    port_name: p.port_name || null,
    port_type: p.port_type || "access",
    vlan_id: p.vlan_id ?? null,
    status: p.status || "up",
    speed: p.speed || null,
    description: p.description || null,
  });

  for (const p of toAdd) {
    try {
      const r = await apiPost(`/api/resources/devices/${deviceId}/device-ports`, buildPayload(p));
      if (r.success) added++; else failed++;
    } catch { failed++; }
  }

  for (const p of toOverwrite) {
    const key = String(p.port_number).toLowerCase();
    const existing = conflictState.existingMap.get(key);
    if (!existing) {
      try {
        const r = await apiPost(`/api/resources/devices/${deviceId}/device-ports`, buildPayload(p));
        if (r.success) added++; else failed++;
      } catch { failed++; }
      continue;
    }
    try {
      const r = await apiPut(`/api/resources/devices/device-ports/${existing.id}`, buildPayload(p));
      if (r.success) overwritten++; else failed++;
    } catch { failed++; }
  }

  const parts = [];
  if (added) parts.push(`${t('device.sync_added') || '新增'} ${added}`);
  if (overwritten) parts.push(`${t('device.sync_overwritten') || '覆盖'} ${overwritten}`);
  const skipped = (conflictState.newPorts.length - toAdd.length - toOverwrite.length);
  if (skipped > 0) parts.push(`${t('device.sync_skipped') || '跳过'} ${skipped}`);
  if (failed) parts.push(`${t('device.sync_failed') || '失败'} ${failed}`);

  showToast((t('device.sync_result') || '同步完成') + ': ' + parts.join('，'),
    failed > 0 ? "warning" : "success");

  await refreshModalView(deviceId);
}

/**
 * 端口分组逻辑（按端口名前缀分组，小组并入"其他"）
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

    if (!rawGroups[groupKey]) rawGroups[groupKey] = [];
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
  if (match) return parseInt(match[match.length - 1]) || 0;
  return 0;
}

function extractPortLastNumber(portNumber) {
  if (typeof portNumber !== 'string' || !portNumber) return portNumber || '';
  const match = portNumber.match(/\d+/g);
  if (match) return match[match.length - 1];
  return portNumber;
}

export {
  groupPorts,
};
