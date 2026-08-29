// 导入必要的模块
import { apiGet, apiPost, apiPut } from "../utils/apiClient.js";

import { showToast, handleError } from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modalLoader.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";

// 编辑机位（可视化回调）
export async function editCabinetPosition(id) {
  try {
    const result = await apiGet(`/api/resources/positions/${id}`);
    if (result.success) {
      openCabinetPositionModal(result.data);
    } else {
      showToast(`${t("cabinet_position.load_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("cabinet_position.load_failed"));
  }
}

// 提交机柜机位表单
export async function submitCabinetPositionForm() {
  const id = elementCache.getValue("cabinet-position-id");
  const parsedId = id && id !== "" ? id : null;
  const name = elementCache.getValue("cabinet-position-name");
  const cabinetId = elementCache.getValue("cabinet-position-cabinet");
  const startU = parseInt(elementCache.getValue("cabinet-position-start-u"));
  const endU = parseInt(elementCache.getValue("cabinet-position-end-u"));
  const description = elementCache.getValue("cabinet-position-description");

  // 验证必填字段
  if (!name.trim()) {
    showToast(t("cabinet_position.name_required"), "warning");
    return;
  }

  if (!cabinetId) {
    showToast(t("cabinet_position.cabinet_required"), "warning");
    return;
  }

  // 验证U位是否为有效数字
  if (isNaN(startU) || startU <= 0) {
    showToast(t("cabinet_position.start_u_invalid"), "warning");
    return;
  }

  if (isNaN(endU) || endU <= 0) {
    showToast(t("cabinet_position.end_u_invalid"), "warning");
    return;
  }

  if (startU > endU) {
    showToast(t("cabinet_position.start_gt_end"), "warning");
    return;
  }

  const positionData = {
    name: name.trim(),
    cabinet_id: cabinetId,
    start_u: startU,
    end_u: endU,
    description: description.trim() || null
  };

  // 防重复提交：请求期间禁用保存按钮，结束后恢复（双击会重复提交产生两条机位）
  const saveBtn = document.querySelector("#cabinet-position-form button[type='submit']");
  const originalText = saveBtn?.textContent;
  if (saveBtn) {
    saveBtn.disabled = true;
    saveBtn.textContent = t("common.saving");
  }

  try {
    let result;
    if (parsedId) {
      result = await apiPut(`/api/resources/positions/${parsedId}`, positionData);
    } else {
      result = await apiPost("/api/resources/positions", positionData);
    }

    if (result.success) {
      closeModal("cabinet-position-modal");
      showToast(t("cabinet_position.save_success"), "success");
    } else {
      const errorMsg = result.message || t("common.check_input");
      showToast(`${t("common.operation_failed")}: ${errorMsg}`, "error");
      console.error("服务器返回错误:", result);
    }
  } catch (error) {
    console.error("提交机位表单失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  } finally {
    if (saveBtn) {
      saveBtn.disabled = false;
      saveBtn.textContent = originalText;
    }
  }
}

// ====== 机位管理模态框 ======
export async function openCabinetPositionModal(position = null) {
  const modal = await openModal("cabinet-position-modal");
  // 模板加载失败时 openModal 返回空,直接中止避免 title.textContent 空引用
  if (!modal) {
    return;
  }

  const title = elementCache.get("cabinet-position-modal-title");
  const form = elementCache.get("cabinet-position-form");

  const roomSelect = elementCache.get("cabinet-position-room");
  const cabinetSelect = elementCache.get("cabinet-position-cabinet");

  // 从 cabinets 数据源只读加载房间选项（去重）
  const loadRoomsFromCabinets = async () => {
    roomSelect.innerHTML = `<option value="">${t("cabinet_position.select_room")}</option>`;

    try {
      const result = await apiGet("/api/resources/cabinets");
      if (result.success && result.data) {
        const cabinets = result.data.items ?? [];
        const roomMap = new Map();

        cabinets.forEach((cabinet) => {
          if (cabinet.room_id && cabinet.room_name && !roomMap.has(cabinet.room_id)) {
            roomMap.set(cabinet.room_id, cabinet.room_name);
          }
        });

        roomMap.forEach((roomName, roomId) => {
          const option = document.createElement("option");
          option.value = roomId;
          option.textContent = roomName;
          roomSelect.appendChild(option);
        });
      }
    } catch (error) {
      console.error("从机柜数据加载房间失败:", error);
    }
  };

  // 房间选择变化时加载机柜（只读）
  const handleRoomChange = async () => {
    const roomId = roomSelect.value;
    cabinetSelect.innerHTML = `<option value="">${t("cabinet_position.select_cabinet")}</option>`;

    if (roomId) {
      try {
        const result = await apiGet(`/api/resources/cabinets?room_id=${roomId}`);
        if (result.success && result.data) {
          const cabinets = result.data.items ?? [];
          cabinets.forEach((cabinet) => {
            const option = document.createElement("option");
            option.value = cabinet.id;
            option.textContent = cabinet.name;
            cabinetSelect.appendChild(option);
          });
        }
      } catch (error) {
        console.error("加载机柜失败:", error);
      }
    }
  };

  // 通过 cabinet_id 从 cabinets 数据源只读获取房间信息
  const loadRoomByCabinetId = async (cabinetId) => {
    try {
      const result = await apiGet(`/api/resources/cabinets/${cabinetId}`);
      if (result.success && result.data) {
        return {
          roomId: result.data.room_id,
          roomName: result.data.room_name
        };
      }
    } catch (error) {
      console.error("从机柜数据获取房间信息失败:", error);
    }
    return null;
  };

  // 加载房间选项（从 cabinets 数据源只读）
  await loadRoomsFromCabinets();

  // 模态框每次打开均为全新 DOM（关闭即销毁），不会累积监听器，直接绑定即可
  if (roomSelect) {
    roomSelect.addEventListener("change", handleRoomChange);
  }

  if (position) {
    // 编辑模式
    title.textContent = t("cabinet_position.edit");
    elementCache.setValue("cabinet-position-id", position.id);
    elementCache.setValue("cabinet-position-name", position.name);
    elementCache.setValue("cabinet-position-start-u", position.start_u || 1);
    elementCache.setValue("cabinet-position-end-u", position.end_u || 1);
    elementCache.setValue("cabinet-position-description", position.description || "");

    // 通过 cabinet_id 从 cabinets 数据源只读获取房间信息
    if (position.cabinet_id) {
      const roomInfo = await loadRoomByCabinetId(position.cabinet_id);
      if (roomInfo && roomInfo.roomId) {
        elementCache.setValue("cabinet-position-room", roomInfo.roomId);
        await handleRoomChange();
        elementCache.setValue("cabinet-position-cabinet", position.cabinet_id);
      }
    }
  } else {
    // 添加模式
    title.textContent = t("cabinet_position.add");
    form.reset();
    elementCache.setValue("cabinet-position-id", "");
    cabinetSelect.innerHTML = `<option value="">${t("cabinet_position.select_room_first")}</option>`;
  }
}
