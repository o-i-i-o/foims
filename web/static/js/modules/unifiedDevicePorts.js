import { apiGet, apiPost, apiPut, apiDelete } from "../utils/apiClient.js";

import { showToast, handleError, escapeHtml } from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modalLoader.js";
import { elementCache } from "../utils/helpers.js";
import { showConfirm } from "../utils/confirm.js";
import { t } from "../utils/i18n.js";
import { INTERFACE_ROLES, PHYSICAL_TYPES } from "../utils/networkCardManager.js";

/**
 * 统一设备端口管理（device_interfaces 单一模型）
 *
 * 数据源为 GET /devices/{id}/nics（网卡 → 网口 → IP）：
 *   - 每张网卡/板卡渲染为一个端口分组（手工网卡在前、自动板卡在后，
 *     由后端 sort_order 保证顺序）；
 *   - 设备模态框托管的网口（device_managed=1）与端口模态框/SNMP 同步
 *     生成的网口（device_managed=0）统一展示，均可点击编辑；
 *   - 端口详情模态框配置项与设备模态框网口容器一致，另含「设备管理」
 *     复选框（勾选后该网口进入设备编辑模态框管理）。
 *
 * SNMP 拉取：GET /snmp-ports 取实时端口，与现有网口按名称比对，
 * 冲突项弹窗逐项选择覆盖/跳过；覆盖时不改动 device_managed。
 */

// SNMP 同步过程中待处理的冲突信息（供冲突对话框使用）
const conflictState = {
  newPorts: [], // SNMP 拉取到的全部端口
  existingMap: new Map(), // 已存在网口：key = name（小写）
  decisions: {} // { name: 'overwrite' | 'skip' }
};

/**
 * 统一的设备端口管理入口
 * @param {string} deviceId - 设备ID
 * @param {string} deviceName - 设备名称
 */
export async function manageUnifiedDevicePorts(deviceId, deviceName) {
  try {
    // /nics 数据与模态框 HTML 互不依赖，并行加载消除串行等待
    const [result] = await Promise.all([
      apiGet(`/api/resources/devices/${deviceId}/nics`),
      openModal("unified-device-ports-modal")
    ]);
    if (!result.success || !result.data) {
      closeModal("unified-device-ports-modal");
      showToast(t("device.load_failed"), "error");
      return;
    }

    const { cards } = result.data;

    const title = elementCache.get("unified-device-ports-modal-title");
    if (title) {
      title.textContent = `${deviceName} - ${t("device.unified_ports")}`;
    }

    renderAllPortGroups(cards || []);
    bindModalButtons(deviceId);
  } catch (error) {
    handleError(error, t("device.port_management_failed"));
  }
}

/**
 * 渲染所有端口分组：每张网卡/板卡一组，组内为该卡的全部网口
 */
function renderAllPortGroups(cards) {
  const container = document.querySelector(".port-groups-container");
  if (!container) {
    return;
  }
  container.innerHTML = "";

  // fragment 一次性挂载，分组多时避免逐组 append 触发多次重排
  const fragment = document.createDocumentFragment();
  let rendered = 0;

  cards.forEach((card) => {
    const ports = Array.isArray(card.ports) ? card.ports : [];
    if (ports.length === 0) {
      return;
    }
    rendered += ports.length;
    fragment.appendChild(createPortGroup(card, ports));
  });

  if (rendered === 0) {
    container.innerHTML = `<p class="empty-message">${t("device.no_device_ports")}</p>`;
    return;
  }
  container.appendChild(fragment);
}

/**
 * 创建一个端口分组（网卡/板卡 = .port-group）
 */
function createPortGroup(card, ports) {
  const groupElement = document.createElement("div");
  // 自动板卡（SNMP 分组生成）与手工网卡以色调区分
  groupElement.className = "port-group is-switch-group";

  const groupTitle = document.createElement("h4");
  const cardName = card.name || t("device.network_card");
  groupTitle.textContent = `${cardName} (${ports.length})`;
  groupElement.appendChild(groupTitle);

  const portGrid = document.createElement("div");
  portGrid.className = "port-grid";

  ports.sort(
    (a, b) =>
      (a.sort_order ?? 0) - (b.sort_order ?? 0) || String(a.name).localeCompare(String(b.name))
  );

  const portFragment = document.createDocumentFragment();
  ports.forEach((iface) => {
    portFragment.appendChild(createPortItem(iface));
  });
  portGrid.appendChild(portFragment);

  groupElement.appendChild(portGrid);
  return groupElement;
}

/**
 * 创建端口项（可点击编辑）
 */
function createPortItem(iface) {
  const portItem = document.createElement("div");
  portItem.className = `port-item ${iface.device_managed ? "nic-interface" : ""} status-${iface.status || "up"}`;
  portItem.dataset.portId = iface.id || "";
  portItem.dataset.name = iface.name || "";
  portItem.dataset.physicalType = iface.physical_type || "other";
  portItem.dataset.interfaceRole = iface.interface_role || "business";
  portItem.dataset.vlanId = iface.vlan_id || "";
  portItem.dataset.macAddress = iface.mac_address || "";
  portItem.dataset.description = iface.description || "";
  portItem.dataset.deviceManaged = iface.device_managed === true ? "1" : "0";
  portItem.dataset.status = iface.status || "up";
  portItem.dataset.speed = iface.speed || "";

  // 显示端口名的末尾数字，无数字则显示首字母
  const display = extractPortLastNumber(iface.name) || (iface.name || "?").slice(0, 3);
  portItem.textContent = display;

  // tooltip：端口名 + MAC + IP（含所属网段/网络区域）+ 运行状态
  const ipList = Array.isArray(iface.ips)
    ? iface.ips
        .map((i) => {
          const netInfo = [i.network_region, i.network_name].filter(Boolean).join(" / ");
          return netInfo ? `${i.ip_address} (${netInfo})` : i.ip_address;
        })
        .filter(Boolean)
    : [];
  const tipParts = [iface.name || ""];
  if (iface.mac_address) {
    tipParts.push(`MAC: ${iface.mac_address}`);
  }
  if (ipList.length) {
    tipParts.push(`IP: ${ipList.join(", ")}`);
  }
  if (iface.status && iface.status !== "up") {
    tipParts.push(`${t("ip.status")}: ${iface.status}`);
  }
  if (iface.speed) {
    tipParts.push(`${t("device.speed")}: ${iface.speed}`);
  }

  const tooltip = document.createElement("div");
  tooltip.className = "port-tooltip";
  tooltip.textContent = tipParts.join(" | ");
  portItem.appendChild(tooltip);

  return portItem;
}

/**
 * 绑定模态框底部按钮 + 端口项点击事件
 */
function bindModalButtons(deviceId) {
  // 页脚按钮用 onclick 赋值保证幂等：refreshModalView 在模态框未销毁时
  // 重复调用本函数，addEventListener 会叠加监听导致一次点击多次触发
  const addPortBtn = elementCache.get("add-port-btn");
  if (addPortBtn) {
    addPortBtn.onclick = () => {
      // 端口模态框新建的端口默认不进设备模态框（设备管理复选框不勾选）
      openPortDetailModal({ portId: "", deviceId, name: "", deviceManaged: false });
    };
  }

  // 从SNMP获取端口
  const getSnmpBtn = elementCache.get("get-snmp-ports-btn");
  if (getSnmpBtn) {
    getSnmpBtn.onclick = async () => {
      await startSnmpSync(deviceId);
    };
  }

  // 端口项点击（事件委托）：统一打开端口详情编辑。
  // 容器为常驻节点，用 dataset.bound 防止重复绑定
  const container = document.querySelector(".port-groups-container");
  if (container && !container.dataset.bound) {
    container.dataset.bound = "true";
    container.addEventListener("click", (e) => {
      const portItem = e.target.closest(".port-item");
      if (!portItem) {
        return;
      }
      openPortDetailModal({
        portId: portItem.dataset.portId,
        deviceId,
        name: portItem.dataset.name,
        physicalType: portItem.dataset.physicalType,
        interfaceRole: portItem.dataset.interfaceRole,
        vlanId: portItem.dataset.vlanId,
        macAddress: portItem.dataset.macAddress,
        description: portItem.dataset.description,
        deviceManaged: portItem.dataset.deviceManaged === "1"
      });
    });
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
      ? t("device.add_port")
      : `${t("device.port_detail")} - ${portData.name}`;
  }

  // 物理形态/接口角色选项与设备模态框网口容器共用同一来源
  const ptypeSelect = elementCache.get("device-port-ptype-expanded");
  if (ptypeSelect) {
    ptypeSelect.innerHTML = PHYSICAL_TYPES.map(
      (opt) => `<option value="${opt.value}">${escapeHtml(opt.label())}</option>`
    ).join("");
    ptypeSelect.value = portData.physicalType || "other";
  }
  const roleSelect = elementCache.get("device-port-role-expanded");
  if (roleSelect) {
    roleSelect.innerHTML = INTERFACE_ROLES.map(
      (opt) => `<option value="${opt.value}">${escapeHtml(opt.label())}</option>`
    ).join("");
    roleSelect.value = portData.interfaceRole || "business";
  }

  // 填充表单
  elementCache.setValue("device-port-id-expanded", portData.portId || "");
  elementCache.setValue("device-port-device-id-expanded", portData.deviceId || "");
  elementCache.setValue("device-port-name-expanded", portData.name || "");
  elementCache.setValue("device-port-vlan-expanded", portData.vlanId || "");
  elementCache.setValue("device-port-mac-expanded", portData.macAddress || "");
  elementCache.setValue("device-port-description-expanded", portData.description || "");
  const managedCheckbox = elementCache.get("device-port-managed-expanded");
  if (managedCheckbox) {
    managedCheckbox.checked = portData.deviceManaged === true;
  }

  if (deleteBtn) {
    deleteBtn.style.display = isNewPort ? "none" : "inline-block";
  }
  // 模态框每次打开都会重建 DOM，直接绑定无需防重
  if (saveBtn) {
    saveBtn.addEventListener("click", () => submitPortForm());
  }
  if (deleteBtn) {
    deleteBtn.addEventListener("click", () => deletePort());
  }
}

/**
 * 收集端口表单数据（与设备模态框网口容器一致的配置项 + 设备管理标记）
 */
function collectPortFormData() {
  const vlanRaw = elementCache.getValue("device-port-vlan-expanded");
  return {
    id: elementCache.getValue("device-port-id-expanded"),
    deviceId: elementCache.getValue("device-port-device-id-expanded"),
    name: elementCache.getValue("device-port-name-expanded"),
    physical_type: elementCache.getValue("device-port-ptype-expanded") || "other",
    interface_role: elementCache.getValue("device-port-role-expanded") || "business",
    vlan_id: vlanRaw ? parseInt(vlanRaw, 10) : null,
    mac_address: elementCache.getValue("device-port-mac-expanded") || null,
    description: elementCache.getValue("device-port-description-expanded") || null,
    device_managed: elementCache.get("device-port-managed-expanded")?.checked === true
  };
}

/**
 * 提交端口表单（新增 / 更新），成功后刷新模态框
 */
async function submitPortForm() {
  const data = collectPortFormData();
  const { id, deviceId, name } = data;

  if (!name?.trim()) {
    showToast(t("device.port_name_required"), "warning");
    return;
  }
  if (data.vlan_id !== null && (isNaN(data.vlan_id) || data.vlan_id < 1 || data.vlan_id > 4094)) {
    showToast(t("device.vlan_invalid"), "warning");
    return;
  }

  const payload = {
    name: name.trim(),
    physical_type: data.physical_type,
    interface_role: data.interface_role,
    vlan_id: data.vlan_id,
    mac_address: data.mac_address?.trim() || null,
    description: data.description?.trim() || null,
    device_managed: data.device_managed
  };

  const saveBtn = elementCache.get("save-port-btn");
  const originalText = saveBtn?.textContent;
  if (saveBtn) {
    saveBtn.disabled = true;
    saveBtn.textContent = t("common.saving");
  }

  try {
    let result;
    if (id) {
      result = await apiPut(`/api/resources/devices/interfaces/${id}`, payload);
    } else {
      result = await apiPost(`/api/resources/devices/${deviceId}/interfaces`, payload);
    }

    if (result.success) {
      closeModal("device-port-detail-modal");
      showToast(id ? t("device.port_update_success") : t("device.port_add_success"), "success");
      await refreshModalView(deviceId);
    } else {
      showToast(`${t("device.port_save_failed")}: ${result.message || ""}`, "error");
    }
  } catch (error) {
    handleError(error, t("device.port_save_failed"));
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
  if (!id) {
    return;
  }

  const confirmed = await showConfirm(t("device.confirm_delete_port"));
  if (!confirmed) {
    return;
  }

  const deleteBtn = elementCache.get("delete-port-btn");
  const originalText = deleteBtn?.textContent;
  if (deleteBtn) {
    deleteBtn.disabled = true;
    deleteBtn.textContent = t("common.deleting");
  }

  try {
    const result = await apiDelete(`/api/resources/devices/interfaces/${id}`);
    if (result.success) {
      closeModal("device-port-detail-modal");
      showToast(t("device.port_delete_success"), "success");
      await refreshModalView(deviceId);
    } else {
      showToast(`${t("device.port_delete_failed")}: ${result.message || ""}`, "error");
    }
  } catch (error) {
    handleError(error, t("device.port_delete_failed"));
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
  const result = await apiGet(`/api/resources/devices/${deviceId}/nics`);
  const cards = result.success && result.data ? result.data.cards || [] : [];
  renderAllPortGroups(cards);
  // 重新绑定（renderAllPortGroups 重置了 container.innerHTML，click 委托需重绑）
  bindModalButtons(deviceId);
}

// ============================================================
// SNMP 拉取 + 端口名称冲突处理（覆盖 / 跳过 / 全部覆盖 / 全部跳过）
// 流程：
//   1. GET /snmp-ports 拿到 SNMP 实时端口（不落库）
//   2. GET /nics 展平现有网口（含托管与非托管）
//   3. 对比端口名称（忽略大小写），找出冲突项
//   4. 若有冲突 → 弹出 port-conflict-modal 供用户逐项选择或一键全部
//   5. 按决策走 POST（新增）/ PUT（覆盖），跳过的项不处理；
//      覆盖仅更新运行属性，不改动 device_managed 托管状态
// ============================================================

async function startSnmpSync(deviceId) {
  const syncBtn = elementCache.get("get-snmp-ports-btn");
  const originalText = syncBtn?.textContent;

  try {
    if (syncBtn) {
      syncBtn.disabled = true;
      syncBtn.textContent = t("device.fetching_snmp");
    }

    // 1. 拉取 SNMP 实时端口
    const snmpResult = await apiGet(`/api/resources/devices/${deviceId}/snmp-ports`);
    if (!snmpResult.success || !Array.isArray(snmpResult.data) || snmpResult.data.length === 0) {
      showToast(t("device.snmp_no_ports"), "warning");
      return;
    }
    const newPorts = snmpResult.data;

    // 2. 拉取现有网口（/nics 的 cards 展平）
    const existingResult = await apiGet(`/api/resources/devices/${deviceId}/nics`);
    let existingPorts = [];
    if (existingResult.success && existingResult.data) {
      existingPorts = (existingResult.data.cards || []).flatMap((c) => c.ports || []);
    }

    const existingMap = new Map();
    existingPorts.forEach((p) => existingMap.set(String(p.name).toLowerCase(), p));

    // 3. 对比分类
    const toAdd = [];
    const conflicts = [];
    newPorts.forEach((p) => {
      const key = String(p.name).toLowerCase();
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
    conflicts.forEach((c) => {
      conflictState.decisions[String(c.snmpPort.name)] = "skip";
    });

    await showConflictModal(deviceId, toAdd, conflicts);
  } catch (error) {
    handleError(error, t("device.port_sync_failed"));
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
    summaryEl.textContent = t("device.conflict_summary", {
      conflict: conflicts.length,
      add: toAdd.length
    });
  }

  if (listEl) {
    listEl.innerHTML = "";
    conflicts.forEach((c) => {
      const row = document.createElement("div");
      row.className = "port-conflict-row";

      const num = document.createElement("div");
      num.className = "conflict-port-number";
      num.textContent = c.snmpPort.name;

      const detail = document.createElement("div");
      detail.className = "conflict-port-detail";
      const oldDesc = c.existingPort.description || c.existingPort.name;
      const newDesc = c.snmpPort.description || c.snmpPort.name;
      detail.textContent = `${t("device.conflict_existing")}: ${oldDesc} → ${t("device.conflict_new")}: ${newDesc}`;

      const select = document.createElement("select");
      select.className = "conflict-select";
      select.dataset.portName = c.snmpPort.name;
      select.innerHTML = `
        <option value="skip">${t("device.conflict_skip")}</option>
        <option value="overwrite">${t("device.conflict_overwrite")}</option>
      `;
      select.value = conflictState.decisions[c.snmpPort.name] || "skip";
      select.addEventListener("change", () => {
        conflictState.decisions[c.snmpPort.name] = select.value;
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
    skipAllBtn.addEventListener("click", () => {
      conflicts.forEach((c) => {
        conflictState.decisions[String(c.snmpPort.name)] = "skip";
      });
      syncConflictSelections();
    });
  }
  if (overwriteAllBtn) {
    overwriteAllBtn.addEventListener("click", () => {
      conflicts.forEach((c) => {
        conflictState.decisions[String(c.snmpPort.name)] = "overwrite";
      });
      syncConflictSelections();
    });
  }

  // 确认 → 应用决策
  const confirmBtn = document.getElementById("conflict-confirm-btn");
  if (confirmBtn) {
    confirmBtn.addEventListener("click", async () => {
      const toOverwrite = conflicts
        .filter((c) => conflictState.decisions[String(c.snmpPort.name)] === "overwrite")
        .map((c) => c.snmpPort);
      closeModal("port-conflict-modal");
      await applySnmpResults(deviceId, toAdd, toOverwrite);
    });
  }
}

/**
 * 同步所有冲突行下拉框显示
 */
function syncConflictSelections() {
  document.querySelectorAll(".conflict-select").forEach((sel) => {
    const pn = sel.dataset.portName;
    if (pn && conflictState.decisions[pn]) {
      sel.value = conflictState.decisions[pn];
    }
  });
}

/**
 * 应用 SNMP 同步结果
 * @param {string} deviceId
 * @param {Array} toAdd - 新增的端口（device_managed=false，不进设备模态框）
 * @param {Array} toOverwrite - 覆盖（更新运行属性）的端口
 */
async function applySnmpResults(deviceId, toAdd, toOverwrite) {
  let added = 0,
    overwritten = 0,
    failed = 0;

  // 新增：网卡按端口名前缀由后端自动生成
  const buildCreatePayload = (p) => ({
    name: p.name,
    physical_type: "other",
    interface_role: "business",
    description: p.description || null,
    port_type: p.port_type || null,
    vlan_id: p.vlan_id ?? null,
    status: p.status || null,
    speed: p.speed || null,
    device_managed: false
  });

  // 覆盖：仅更新运行属性与描述，不传 device_managed（保留原托管状态）
  const buildOverwritePayload = (p) => ({
    description: p.description || null,
    port_type: p.port_type || null,
    vlan_id: p.vlan_id ?? null,
    status: p.status || null,
    speed: p.speed || null
  });

  for (const p of toAdd) {
    try {
      const r = await apiPost(
        `/api/resources/devices/${deviceId}/interfaces`,
        buildCreatePayload(p)
      );
      if (r.success) {
        added++;
      } else {
        failed++;
      }
    } catch (e) {
      console.error(`SNMP 同步新增端口 ${p.name} 失败:`, e);
      failed++;
    }
  }

  for (const p of toOverwrite) {
    const key = String(p.name).toLowerCase();
    const existing = conflictState.existingMap.get(key);
    if (!existing) {
      try {
        const r = await apiPost(
          `/api/resources/devices/${deviceId}/interfaces`,
          buildCreatePayload(p)
        );
        if (r.success) {
          added++;
        } else {
          failed++;
        }
      } catch (e) {
        console.error(`SNMP 同步新增端口 ${p.name} 失败:`, e);
        failed++;
      }
      continue;
    }
    try {
      const r = await apiPut(
        `/api/resources/devices/interfaces/${existing.id}`,
        buildOverwritePayload(p)
      );
      if (r.success) {
        overwritten++;
      } else {
        failed++;
      }
    } catch (e) {
      console.error(`SNMP 同步覆盖端口 ${p.name} 失败:`, e);
      failed++;
    }
  }

  const parts = [];
  if (added) {
    parts.push(`${t("device.sync_added")} ${added}`);
  }
  if (overwritten) {
    parts.push(`${t("device.sync_overwritten")} ${overwritten}`);
  }
  const skipped = conflictState.newPorts.length - toAdd.length - toOverwrite.length;
  if (skipped > 0) {
    parts.push(`${t("device.sync_skipped")} ${skipped}`);
  }
  if (failed) {
    parts.push(`${t("device.sync_failed")} ${failed}`);
  }

  showToast(`${t("device.sync_result")}: ${parts.join("，")}`, failed > 0 ? "warning" : "success");

  await refreshModalView(deviceId);
}

function extractPortLastNumber(portName) {
  if (typeof portName !== "string" || !portName) {
    return portName || "";
  }
  const match = portName.match(/\d+/g);
  if (match) {
    return match.at(-1);
  }
  return portName;
}
