

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
  formatDateTime,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  debounce,
  handleError
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";

import {
  loadDataCenterRoomsForSelect,
  loadRoomNetworksForCabinet,
  loadCabinets
} from "../utils/resources.js";

// 加载机柜数据
export async function loadCabinetsData() {
  const token = getAccessToken();
  if (!token) {
    return;
  }

  try {
    const response = await fetch("/api/resources/cabinets", {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const data = await response.json();
    const tbody = document.querySelector("#cabinets-table tbody");

    if (data.success && data.data.length > 0) {
      tbody.innerHTML = "";
      data.data.forEach((cabinet) => {
        const row = document.createElement("tr");
        // 处理网络信息，显示所有关联的网段
        const networkInfo =
          cabinet.networks && cabinet.networks.length > 0
            ? cabinet.networks
                .map((network) => `${network.name} (${network.network_region})`)
                .join("<br>")
            : "-";
        row.innerHTML = `
                    <td>${cabinet.name}</td>
                    <td>${networkInfo}</td>
                    <td>${cabinet.description || "-"}</td>
                    <td>${new Date(cabinet.created_at).toLocaleString()}</td>
                    <td>
                        <button class="btn btn-sm btn-edit" data-id="${cabinet.id}">编辑</button>
                        <button class="btn btn-sm btn-delete" data-id="${cabinet.id}">删除</button>
                    </td>
                `;
        tbody.appendChild(row);
      });
    } else {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="5" class="text-center">暂无机柜数据</td></tr>';
    }
  } catch (error) {
    console.error("加载机柜数据失败:", error);
    const tbody = document.querySelector("#cabinets-table tbody");
    tbody.innerHTML =
      '<tr class="empty-row"><td colspan="5" class="text-center">加载失败，请刷新页面重试</td></tr>';
  }
}

// 编辑机柜
export async function editCabinet(id) {
  const token = getAccessToken();
  if (!token) return;

  try {
    const response = await fetch(`/api/resources/cabinets/${id}`, {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const result = await response.json();
    if (result.success) {
      openCabinetModal(result.data);
    } else {
      showToast(`获取机柜数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取机柜数据失败");
  }
}

// 删除机柜
export async function deleteCabinet(id) {
  await handleDelete(id, "/api/resources/cabinets", "机柜删除成功", loadCabinetsData);
}

// ====== 机柜管理模态框 ======
export async function openCabinetModal(cabinet = null) {
  const modal = document.getElementById("cabinet-modal");
  const title = document.getElementById("cabinet-modal-title");
  const form = document.getElementById("cabinet-form");

  // 获取容量输入框
  const capacityInput = document.getElementById("cabinet-capacity");
  
  // 加载机房选项
  await loadDataCenterRoomsForSelect();
  
  // 获取房间选择框
  const roomSelect = document.getElementById("cabinet-room");
  
  // 添加房间选择事件监听器，当选择房间时，自动显示该房间的网段配置
  roomSelect.addEventListener("change", async function() {
    const roomId = this.value;
    await loadRoomNetworksForCabinet(roomId);
  });
  
  if (cabinet) {
    // 编辑模式
    title.textContent = "编辑机柜";
    document.getElementById("cabinet-id").value = cabinet.id;
    document.getElementById("cabinet-name").value = cabinet.name;
    // 设置房间选择
    if (cabinet.room_id) {
      document.getElementById("cabinet-room").value = cabinet.room_id;
      // 加载所选房间的网段配置
      await loadRoomNetworksForCabinet(cabinet.room_id);
    }
    capacityInput.value = cabinet.capacity || 42; // 设置默认值42
    document.getElementById("cabinet-description").value = cabinet.description || "";
  } else {
    // 添加模式
    title.textContent = "添加机柜";
    form.reset();
    document.getElementById("cabinet-id").value = "";
    // 初始化网段显示
    const inheritedNetworksContainer = document.getElementById("cabinet-inherited-networks");
    inheritedNetworksContainer.innerHTML = '<p class="text-muted">请先选择所属房间，将自动继承房间的网段配置</p>';
  }

  openModal("cabinet-modal");
}

// 提交机柜表单
export async function submitCabinetForm() {
  const token = getAccessToken();
  if (!token) return;

  // 使用通用工具函数获取表单数据
  const id = getElementValue("cabinet-id"); // 直接获取UUID字符串，不转换为数字
  const name = getElementValue("cabinet-name", "trimmed");
  const roomId = getElementValue("cabinet-room"); // 获取房间ID
  const capacity = getElementValue("cabinet-capacity", "number"); // capacity是数字类型，继续使用number转换
  const description = getElementValue("cabinet-description", "trimmed");
  
  // 验证必填字段
  if (!name.trim()) {
    showToast("机柜名称不能为空", "warning");
    return;
  }

  if (!roomId) {
    showToast("请选择所属机房", "warning");
    return;
  }

  // 验证容量
  if (isNaN(capacity) || capacity <= 0) {
    showToast("机柜容量必须是有效的正数", "warning");
    return;
  }

  // 获取房间详情，包括其关联的网段
  let networkIds = [];
  try {
    const roomResult = await apiGet(`/api/resources/rooms/${roomId}`);
    if (roomResult.success && roomResult.data && roomResult.data.networks) {
      networkIds = roomResult.data.networks.map(network => network.id);
    }
  } catch (error) {
    console.error("获取房间网段配置失败:", error);
    showToast("获取房间网段配置失败，请重试", "error");
    return;
  }

  if (networkIds.length === 0) {
    showToast("所选房间未配置网段，请先为房间添加网段配置", "warning");
    return;
  }

  // 后端期望network_ids是一个数组
  const cabinetData = {
    name: name.trim(),
    room_id: roomId,
    capacity,
    network_ids: networkIds,
    description: description.trim() || null,
  };

  // 使用通用表单提交处理函数
  const success = await handleFormSubmit({
    formData: cabinetData,
    id,
    baseUrl: "/api/resources/cabinets",
    token,
    successMessage: "机柜保存成功",
    modalId: "cabinet-modal",
    reloadFunction: loadCabinetsData
  });

  return success;
}

// 加载机柜选项（用于机位模态框）
export async function loadCabinetsForModalSelect() {
  try {
    const cabinets = await loadCabinets();
    const select = document.getElementById("cabinet-position-cabinet");

    if (select) {
      // 清空现有选项
      select.innerHTML = "";

      // 添加默认占位符
      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = "选择机柜";
      select.appendChild(placeholder);

      // 添加新选项
      cabinets.forEach((cabinet) => {
        const option = document.createElement("option");
        option.value = cabinet.id;
        option.textContent = cabinet.name;
        select.appendChild(option);
      });

      return cabinets;
    }
    return [];
  } catch (error) {
    console.error("加载机柜选项失败:", error);
    return [];
  }
}
