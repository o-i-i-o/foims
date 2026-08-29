// 导入必要的模块
import { apiGet, apiPost, apiPut } from "../utils/apiClient.js";

import { showToast, handleError, escapeHtml } from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modalLoader.js";

import { t } from "../utils/i18n.js";

import { loadRoomsForSelect, fetchRoomsForOptions } from "../utils/resources.js";

import { elementCache } from "../utils/helpers.js";

// 房间 → 所属组织映射（弹窗打开时随房间选项一并构建，供管理人下拉取员工）
let roomOrgMap = new Map();

/** 按组织加载员工到管理人下拉：组织人员是管理人唯一数据来源 */
// 管理人下拉加载序号:快速切换房间时旧员工响应晚到,序号不符即丢弃
let managerOptionsSeq = 0;

async function loadManagerOptions(orgId, selectedEmployeeId = "") {
  const select = document.getElementById("workstation-manager");
  if (!select) {
    return;
  }
  const requestSeq = ++managerOptionsSeq;
  let employees = [];
  if (orgId) {
    try {
      const result = await apiGet(`/api/resources/employees?org_id=${encodeURIComponent(orgId)}`);
      if (requestSeq !== managerOptionsSeq) {
        return; // 已有更新的加载在途，丢弃过期结果
      }
      employees = result.success && Array.isArray(result.data) ? result.data : [];
    } catch (error) {
      console.error("加载组织员工失败:", error);
    }
  }
  select.innerHTML = [
    `<option value="">${t("workstation.manager_unassigned")}</option>`,
    ...employees.map(
      (emp) =>
        `<option value="${emp.id}" ${emp.id === selectedEmployeeId ? "selected" : ""}>${escapeHtml(emp.name)}</option>`
    )
  ].join("");
}

/** 房间切换 → 管理人选项跟随新房间的所属组织刷新（弹窗每次打开重建 DOM，需重绑） */
function bindRoomManagerSync() {
  const roomSelect = document.getElementById("workstation-room");
  if (!roomSelect) {
    return;
  }
  roomSelect.addEventListener("change", async (e) => {
    await loadManagerOptions(roomOrgMap.get(e.target.value) || "");
  });
}

// 编辑工位（可视化回调；position 为画布当前坐标，用于回填坐标输入框）
export async function editWorkstation(id, position = null) {
  try {
    const result = await apiGet(`/api/resources/workstations/${id}`);
    if (result.success) {
      openWorkstationModal(result.data, position);
    } else {
      showToast(`${t("workstation.load_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("workstation.load_failed"));
  }
}

// ====== 工位管理模态框 ======
export async function openWorkstationModal(workstation = null, position = null) {
  await openModal("workstation-modal");

  const title = elementCache.get("workstation-modal-title");
  const form = elementCache.get("workstation-form");

  // 加载房间选项（只加载办公室），并同步构建房间→组织映射供管理人下拉使用
  const rooms = await fetchRoomsForOptions(null, true);
  roomOrgMap = new Map(rooms.map((room) => [room.id, room.org_id || ""]));
  await loadRoomsForSelect("workstation-room", { onlyOffice: true });
  bindRoomManagerSync();

  if (workstation) {
    // 编辑模式
    title.textContent = t("workstation.edit");
    elementCache.setValue("workstation-id", workstation.id);
    elementCache.setValue("workstation-name", workstation.name);
    elementCache.setValue("workstation-room", workstation.room_id);
    await loadManagerOptions(
      roomOrgMap.get(workstation.room_id) || "",
      workstation.manager_employee_id || ""
    );
    elementCache.setValue("workstation-description", workstation.description || "");
    // 画布传入的当前坐标回填（房间管理入口无画布上下文，留空表示不动位置）
    elementCache.setValue("workstation-x", position ? String(Math.round(position.x)) : "");
    elementCache.setValue("workstation-y", position ? String(Math.round(position.y)) : "");
  } else {
    // 添加模式
    title.textContent = t("workstation.add");
    form.reset();
    elementCache.setValue("workstation-id", "");
    await loadManagerOptions("");
  }
}

export async function submitWorkstationForm() {
  const id = elementCache.getValue("workstation-id");
  const parsedId = id && id !== "" ? id : null;
  const name = elementCache.getValue("workstation-name");
  const roomId = elementCache.getValue("workstation-room");
  const managerEmployeeId = elementCache.getValue("workstation-manager");
  const description = elementCache.getValue("workstation-description");

  if (!name?.trim()) {
    showToast(t("workstation.name_required"), "warning");
    return;
  }

  if (!roomId) {
    showToast(t("workstation.room_required"), "warning");
    return;
  }

  const workstationData = {
    name: name.trim(),
    room_id: roomId,
    manager_employee_id: managerEmployeeId || null,
    description: description.trim() || null
  };

  // 防重复提交：请求期间禁用保存按钮，结束后恢复（双击会重复提交产生两条工位）
  const saveBtn = document.querySelector("#workstation-form button[type='submit']");
  const originalText = saveBtn?.textContent;
  if (saveBtn) {
    saveBtn.disabled = true;
    saveBtn.textContent = t("common.saving");
  }

  try {
    let result;
    if (parsedId) {
      result = await apiPut(`/api/resources/workstations/${parsedId}`, workstationData);
    } else {
      result = await apiPost("/api/resources/workstations", workstationData);
    }

    if (result.success) {
      // 坐标仅影响画布布局（element_layouts），随保存事件交给可视化层应用
      const xInput = elementCache.getValue("workstation-x");
      const yInput = elementCache.getValue("workstation-y");
      const hasPosition =
        xInput !== "" &&
        yInput !== "" &&
        !Number.isNaN(Number(xInput)) &&
        !Number.isNaN(Number(yInput));
      document.dispatchEvent(
        new CustomEvent("ipma:workstation-saved", {
          detail: {
            id: result.data?.id || parsedId,
            room_id: roomId,
            x: hasPosition ? Math.max(0, Math.round(Number(xInput))) : null,
            y: hasPosition ? Math.max(0, Math.round(Number(yInput))) : null
          }
        })
      );
      closeModal("workstation-modal");
      showToast(t("workstation.save_success"), "success");
    } else {
      const errorMsg = result.message || t("common.check_input");
      showToast(`${t("common.operation_failed")}: ${errorMsg}`, "error");
    }
  } catch (error) {
    console.error("提交工位表单失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  } finally {
    if (saveBtn) {
      saveBtn.disabled = false;
      saveBtn.textContent = originalText;
    }
  }
}
