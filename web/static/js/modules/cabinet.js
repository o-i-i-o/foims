
// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  debounce,
  handleError,
  appendPaginationToTable,
  escapeHtml,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";

import {
  loadDataCenterRoomsForSelect,
  loadRoomNetworksForCabinet,
  loadCabinets
} from "../utils/resources.js";

let currentCabinetPage = 1;
const CABINET_PAGE_SIZE = 20;
let currentCabinetSort = { by: "name", order: "asc" };

// 房间选择事件监听器引用
let roomSelectHandler = null;

// 加载机柜数据
export async function loadCabinetsData(page = 1, sortBy = null, sortOrder = null) {
  currentCabinetPage = page;
  if (sortBy) currentCabinetSort.by = sortBy;
  if (sortOrder) currentCabinetSort.order = sortOrder;
  
  try {
    const result = await apiGet(`/api/resources/cabinets?page=${page}&page_size=${CABINET_PAGE_SIZE}&sort_by=${currentCabinetSort.by}&sort_order=${currentCabinetSort.order}`);
    const tbody = document.querySelector("#cabinets-table tbody");
    tbody.innerHTML = "";

    const data = result.success ? result.data : { items: [], total: 0 };
    const cabinets = data.items || data;

    if (cabinets.length > 0) {
      const startIndex = (page - 1) * CABINET_PAGE_SIZE;
      cabinets.forEach((cabinet, index) => {
        const row = document.createElement("tr");
        const networkInfo =
          cabinet.networks && cabinet.networks.length > 0
            ? cabinet.networks
                .map((network) => `${escapeHtml(network.name)} (${escapeHtml(network.network_region)})`)
                .join("<br>")
            : "-";
        row.innerHTML = `
                    <td class="index-column">${startIndex + index + 1}</td>
                    <td>${escapeHtml(cabinet.name)}</td>
                    <td>${networkInfo}</td>
                    <td>${escapeHtml(cabinet.description) || "-"}</td>
                    <td>${new Date(cabinet.created_at).toLocaleString()}</td>
                    <td>
                        <button class="btn btn-sm btn-edit" data-id="${cabinet.id}">编辑</button>
                        <button class="btn btn-sm btn-delete" data-id="${cabinet.id}">删除</button>
                    </td>
                `;
        tbody.appendChild(row);
      });

      if (data.total !== undefined) {
        appendPaginationToTable("#cabinets-table", data, loadCabinetsData);
      }
    } else {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="6" class="text-center">暂无机柜数据</td></tr>';
    }
    updateCabinetSortIcons();
  } catch (error) {
    console.error("加载机柜数据失败:", error);
    const tbody = document.querySelector("#cabinets-table tbody");
    tbody.innerHTML =
      '<tr class="empty-row"><td colspan="6" class="text-center">加载失败，请刷新页面重试</td></tr>';
  }
}

// 更新排序图标
function updateCabinetSortIcons() {
  const table = document.getElementById("cabinets-table");
  if (!table) return;
  
  table.querySelectorAll("th.sortable").forEach(th => {
    const sortKey = th.dataset.sort;
    
    if (sortKey === currentCabinetSort.by) {
      th.classList.add("sorted", currentCabinetSort.order);
      th.classList.remove(currentCabinetSort.order === "asc" ? "desc" : "asc");
    } else {
      th.classList.remove("sorted", "asc", "desc");
    }
  });
}

// 初始化机柜排序事件
export function initCabinetSortEvents() {
  const table = document.getElementById("cabinets-table");
  if (!table) return;
  
  table.querySelectorAll("th.sortable").forEach(th => {
    th.addEventListener("click", () => {
      const sortKey = th.dataset.sort;
      const newOrder = (currentCabinetSort.by === sortKey && currentCabinetSort.order === "asc") ? "desc" : "asc";
      loadCabinetsData(1, sortKey, newOrder);
    });
  });
  
  updateCabinetSortIcons();
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
  openModal("cabinet-modal");
  
  const modal = document.getElementById("cabinet-modal");
  const title = document.getElementById("cabinet-modal-title");
  const form = document.getElementById("cabinet-form");

  // 获取容量输入框
  const capacityInput = document.getElementById("cabinet-capacity");
  
  // 加载机房选项
  await loadDataCenterRoomsForSelect();
  
  // 获取房间选择框
  const roomSelect = document.getElementById("cabinet-room");
  
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
  roomSelect.addEventListener("change", roomSelectHandler);
  
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
