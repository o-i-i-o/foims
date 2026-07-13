
// 导入必要的模块
import {
  apiGet,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  handleError,
  appendPaginationToTable,
  escapeHtml,
  DEFAULT_PAGE_SIZE,
  createSortState,
  updateSortIcons,
  initSortEvents,
} from "../utils/ui.js";

import { openModal } from "../utils/modal.js";
import { elementCache } from "../utils/helpers.js";

import {
  loadDataCenterRoomsForSelect,
  loadRoomNetworksForCabinet,
  loadCabinets
} from "../utils/resources.js";

const tableState = createSortState('name', 'asc');
let currentPage = 1;

// 房间选择事件监听器引用
let roomSelectHandler = null;

// 加载机柜数据
export async function loadCabinetsData(page = 1, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);
  
  try {
    const result = await apiGet(`/api/resources/cabinets?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const cabinets = data.items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#cabinets-table", {
      data: cabinets,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'room_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'networks', render: (v) => v && v.length > 0 ? v.map(n => `${escapeHtml(n.name)} (${escapeHtml(n.network_region)})`).join("<br>") : '-' },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'created_at', render: (v) => new Date(v).toLocaleString() },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">删除</button>
        ` }
      ],
      emptyMessage: '暂无机柜数据'
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#cabinets-table", data, loadCabinetsData);
    }
    updateSortIcons("cabinets-table", tableState);
  } catch (error) {
    handleError(error, "加载机柜数据失败", () => {
      renderTable("#cabinets-table", { data: [], columns: [], emptyMessage: "加载失败，请刷新页面重试" });
    });
  }
}

export function initCabinetSortEvents() {
  initSortEvents("cabinets-table", tableState, loadCabinetsData);
}

// 编辑机柜
export async function editCabinet(id) {
  try {
    const result = await apiGet(`/api/resources/cabinets/${id}`);
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
  await openModal("cabinet-modal");

  const title = elementCache.get('cabinet-modal-title');
  const form = elementCache.get('cabinet-form');
  const capacityInput = elementCache.get('cabinet-capacity');

  // 加载机房选项
  await loadDataCenterRoomsForSelect();

  // 获取房间选择框
  const roomSelect = elementCache.get('cabinet-room');

  // 移除旧的事件监听器
  if (roomSelectHandler) {
    roomSelect.removeEventListener("change", roomSelectHandler);
  }

  // 创建新的事件监听器 - 使用箭头函数确保this指向正确
  roomSelectHandler = async (event) => {
    const roomId = event.target.value;
    await loadRoomNetworksForCabinet(roomId);
  };

  // 添加房间选择事件监听器
  if (roomSelect) {
    roomSelect.addEventListener("change", roomSelectHandler);
  }

  if (cabinet) {
    // 编辑模式
    title.textContent = "编辑机柜";
    elementCache.setValue('cabinet-id', cabinet.id);
    elementCache.setValue('cabinet-name', cabinet.name);
    // 设置房间选择
    if (cabinet.room_id) {
      elementCache.setValue('cabinet-room', cabinet.room_id);
      // 加载所选房间的网段配置
      await loadRoomNetworksForCabinet(cabinet.room_id);
    }
    if (capacityInput) capacityInput.value = cabinet.capacity || 42;
    elementCache.setValue('cabinet-description', cabinet.description || "");
  } else {
    // 添加模式
    title.textContent = "添加机柜";
    if (form) form.reset();
    elementCache.setValue('cabinet-id', '');
    // 初始化网段显示
    const inheritedNetworksContainer = elementCache.get('cabinet-inherited-networks');
    if (inheritedNetworksContainer) {
      inheritedNetworksContainer.innerHTML = '<p class="text-muted">请先选择所属房间，将自动继承房间的网段配置</p>';
    }
  }
}

// 提交机柜表单
export async function submitCabinetForm() {
// 使用通用工具函数获取表单数据
  const id = getElementValue("cabinet-id"); // 直接获取UUID字符串，不转换为数字
  const name = getElementValue("cabinet-name");
  const roomId = getElementValue("cabinet-room"); // 获取房间ID
  const capacityStr = getElementValue("cabinet-capacity");
  const capacity = parseInt(capacityStr, 10);
  const description = getElementValue("cabinet-description");
  
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

  // 后端期望的数据格式 - 机柜网络从房间继承，无需单独设置
  const cabinetData = {
    name: name.trim(),
    room_id: roomId,
    capacity,
    description: description.trim() || null,
  };

  // 使用通用表单提交处理函数
  const success = await handleFormSubmit({
    formData: cabinetData,
    id,
    baseUrl: "/api/resources/cabinets",
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
