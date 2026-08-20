// 导入必要的模块
import { apiGet, apiPost, apiPut } from "../utils/apiClient.js";

import { showToast, handleError } from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modalLoader.js";

import { t } from "../utils/i18n.js";

import { loadRoomsForSelect } from "../utils/resources.js";

import { elementCache } from "../utils/helpers.js";

// 编辑工位（可视化回调）
export async function editWorkstation(id) {
  try {
    const result = await apiGet(`/api/resources/workstations/${id}`);
    if (result.success) {
      openWorkstationModal(result.data);
    } else {
      showToast(`${t("workstation.load_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("workstation.load_failed"));
  }
}

// ====== 工位管理模态框 ======
export async function openWorkstationModal(workstation = null) {
  await openModal("workstation-modal");

  const title = elementCache.get("workstation-modal-title");
  const form = elementCache.get("workstation-form");

  // 加载房间选项（只加载办公室）
  await loadRoomsForSelect("workstation-room", { onlyOffice: true });

  if (workstation) {
    // 编辑模式
    title.textContent = t("workstation.edit");
    elementCache.setValue("workstation-id", workstation.id);
    elementCache.setValue("workstation-name", workstation.name);
    elementCache.setValue("workstation-room", workstation.room_id);
    elementCache.setValue("workstation-manager", workstation.manager || "");
    elementCache.setValue("workstation-description", workstation.description || "");
  } else {
    // 添加模式
    title.textContent = t("workstation.add");
    form.reset();
    elementCache.setValue("workstation-id", "");
  }
}

export async function submitWorkstationForm() {
  const id = elementCache.getValue("workstation-id");
  const parsedId = id && id !== "" ? id : null;
  const name = elementCache.getValue("workstation-name");
  const roomId = elementCache.getValue("workstation-room");
  const manager = elementCache.getValue("workstation-manager");
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
    manager: manager.trim() || null,
    description: description.trim() || null
  };

  try {
    let result;
    if (parsedId) {
      result = await apiPut(`/api/resources/workstations/${parsedId}`, workstationData);
    } else {
      result = await apiPost("/api/resources/workstations", workstationData);
    }

    if (result.success) {
      closeModal("workstation-modal");
      showToast(t("workstation.save_success"), "success");
    } else {
      const errorMsg = result.message || t("common.check_input");
      showToast(`${t("common.operation_failed")}: ${errorMsg}`, "error");
      console.error("服务器返回错误:", result);
    }
  } catch (error) {
    console.error("提交工位表单失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  }
}
