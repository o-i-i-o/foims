// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
} from "../utils/apiClient.js";

import {
  showToast,
  handleError,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";

// 编辑机位（可视化回调）
export async function editCabinetPosition(id) {
  try {
    const result = await apiGet(`/api/resources/positions/${id}`);
    if (result.success) {
      openCabinetPositionModal(result.data);
    } else {
      showToast(`获取机位数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取机位数据失败");
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
    showToast("名称不能为空", "warning");
    return;
  }

  if (!cabinetId) {
    showToast("请选择机柜", "warning");
    return;
  }

  // 验证U位是否为有效数字
  if (isNaN(startU) || startU <= 0) {
    showToast("起始U位必须是有效的正数", "warning");
    return;
  }

  if (isNaN(endU) || endU <= 0) {
    showToast("结束U位必须是有效的正数", "warning");
    return;
  }

  if (startU > endU) {
    showToast("起始U位不能大于结束U位", "warning");
    return;
  }

  const positionData = {
    name: name.trim(),
    cabinet_id: cabinetId,
    start_u: startU,
    end_u: endU,
    description: description.trim() || null,
  };

  try {
    let result;
    if (parsedId) {
      result = await apiPut(`/api/resources/positions/${parsedId}`, positionData);
    } else {
      result = await apiPost("/api/resources/positions", positionData);
    }

    if (result.success) {
      closeModal("cabinet-position-modal");
      showToast("机位保存成功", "success");
    } else {
      const errorMsg = result.message || "操作失败，请检查输入信息";
      showToast(`操作失败: ${errorMsg}`, "error");
      console.error("服务器返回错误:", result);
    }
  } catch (error) {
    console.error("提交机位表单失败:", error);
    showToast("操作失败，请重试", "error");
  }
}

// ====== 机位管理模态框 ======
export async function openCabinetPositionModal(position = null) {
  await openModal("cabinet-position-modal");

  const title = elementCache.get('cabinet-position-modal-title');
  const form = elementCache.get('cabinet-position-form');

  const roomSelect = elementCache.get('cabinet-position-room');
  const cabinetSelect = elementCache.get('cabinet-position-cabinet');

  // 从 cabinets 数据源只读加载房间选项（去重）
  const loadRoomsFromCabinets = async () => {
    roomSelect.innerHTML = `<option value="">${t('cabinet_position.select_room', '选择房间')}</option>`;

    try {
      const result = await apiGet('/api/resources/cabinets');
      if (result.success && result.data) {
        const cabinets = result.data.items || result.data;
        const roomMap = new Map();

        cabinets.forEach(cabinet => {
          if (cabinet.room_id && cabinet.room_name && !roomMap.has(cabinet.room_id)) {
            roomMap.set(cabinet.room_id, cabinet.room_name);
          }
        });

        roomMap.forEach((roomName, roomId) => {
          const option = document.createElement('option');
          option.value = roomId;
          option.textContent = roomName;
          roomSelect.appendChild(option);
        });
      }
    } catch (error) {
      console.error('从机柜数据加载房间失败:', error);
    }
  };

  // 房间选择变化时加载机柜（只读）
  const handleRoomChange = async () => {
    const roomId = roomSelect.value;
    cabinetSelect.innerHTML = `<option value="">${t('cabinet_position.select_cabinet', '选择机柜')}</option>`;

    if (roomId) {
      try {
        const result = await apiGet(`/api/resources/cabinets?room_id=${roomId}`);
        if (result.success && result.data) {
          const cabinets = result.data.items || result.data;
          cabinets.forEach(cabinet => {
            const option = document.createElement('option');
            option.value = cabinet.id;
            option.textContent = cabinet.name;
            cabinetSelect.appendChild(option);
          });
        }
      } catch (error) {
        console.error('加载机柜失败:', error);
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
      console.error('从机柜数据获取房间信息失败:', error);
    }
    return null;
  };

  // 加载房间选项（从 cabinets 数据源只读）
  await loadRoomsFromCabinets();

  if (roomSelect) {
    roomSelect.removeEventListener('change', handleRoomChange);
    roomSelect.addEventListener('change', handleRoomChange);
  }

  if (position) {
    // 编辑模式
    title.textContent = "编辑机位";
    elementCache.setValue('cabinet-position-id', position.id);
    elementCache.setValue('cabinet-position-name', position.name);
    elementCache.setValue('cabinet-position-start-u', position.start_u || 1);
    elementCache.setValue('cabinet-position-end-u', position.end_u || 1);
    elementCache.setValue('cabinet-position-description', position.description || "");

    // 通过 cabinet_id 从 cabinets 数据源只读获取房间信息
    if (position.cabinet_id) {
      const roomInfo = await loadRoomByCabinetId(position.cabinet_id);
      if (roomInfo && roomInfo.roomId) {
        elementCache.setValue('cabinet-position-room', roomInfo.roomId);
        await handleRoomChange();
        elementCache.setValue('cabinet-position-cabinet', position.cabinet_id);
      }
    }
  } else {
    // 添加模式
    title.textContent = "添加机位";
    form.reset();
    elementCache.setValue('cabinet-position-id', '');
    cabinetSelect.innerHTML = `<option value="">${t('cabinet_position.select_room_first', '请先选择房间')}</option>`;
  }
}
