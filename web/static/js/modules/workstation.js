
// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
} from "../utils/apiClient.js";

import {
  showToast,
  handleDelete,
  handleError,
  appendPaginationToTable,
  escapeHtml,
  DEFAULT_PAGE_SIZE,
  createSortState,
  updateSortIcons,
  initSortEvents,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";

import {
  loadRoomsForSelect
} from "../utils/resources.js";

import { elementCache } from "../utils/helpers.js";

const tableState = createSortState('name', 'asc');
let isLoading = false;
let currentPage = 1;

export async function editWorkstation(id) {
  try {
    const result = await apiGet(`/api/resources/workstations/${id}`);
    if (result.success) {
      openWorkstationModal(result.data);
    } else {
      showToast(`获取工位数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取工位数据失败");
  }
}

export async function deleteWorkstation(id) {
  await handleDelete(id, "/api/resources/workstations", "工位删除成功", loadWorkstationsData);
}

// 加载工位数据
export async function loadWorkstationsData(page = 1, sortBy = null, sortOrder = null) {
  if (isLoading) return;
  
  try {
    isLoading = true;
    currentPage = page;
    if (sortBy) tableState.setSort(sortBy, sortOrder);
    
    const result = await apiGet(`/api/resources/workstations?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const tbody = document.querySelector("#workstations-table tbody");

    if (!tbody) {
      console.error("未找到工位表格 tbody 元素");
      return;
    }

    tbody.innerHTML = "";

    const data = result.success ? result.data : { items: [], total: 0 };
    const workstations = data.items || data;

    if (workstations.length > 0) {
      const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;
      
      let rowIndex = 0;

      for (const workstation of workstations) {
        const roomName = escapeHtml(workstation.room_name) || "-";
        const workstationName = escapeHtml(workstation.name);

        const row = document.createElement("tr");
        row.innerHTML = `
                    <td class="index-column">${startIndex + rowIndex + 1}</td>
                    <td>${roomName}</td>
                    <td>${workstationName}</td>
                    <td>-</td>
                    <td>${escapeHtml(workstation.manager) || "-"}</td>
                    <td>-</td>
                    <td>${escapeHtml(workstation.description) || "-"}</td>
                    <td>${new Date(workstation.created_at).toLocaleString()}</td>
                    <td>
                        <button class="btn btn-sm btn-edit" data-id="${workstation.id}">编辑</button>
                        <button class="btn btn-sm btn-delete" data-id="${workstation.id}">删除</button>
                    </td>
                `;
        tbody.appendChild(row);
        rowIndex++;
      }

      if (data.total !== undefined) {
        appendPaginationToTable("#workstations-table", data, loadWorkstationsData);
      }
    } else {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="8" class="text-center">暂无工位数据</td></tr>';
    }
    updateSortIcons("workstations-table", tableState);
  } catch (error) {
    console.error("加载工位数据失败:", error);
    const tbody = document.querySelector("#workstations-table tbody");
    if (tbody) {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="8" class="text-center">加载失败，请刷新页面重试</td></tr>';
    }
  } finally {
    isLoading = false;
  }
}

export function initWorkstationSortEvents() {
  initSortEvents("workstations-table", tableState, loadWorkstationsData);
}

// ====== 工位管理模态框 ======
export async function openWorkstationModal(workstation = null) {
  openModal("workstation-modal");
  
  const modal = elementCache.get("workstation-modal");
  const title = elementCache.get("workstation-modal-title");
  const form = elementCache.get("workstation-form");

  // 加载房间选项（只加载办公室）
  await loadRoomsForSelect("workstation-room", { onlyOffice: true });

  if (workstation) {
    // 编辑模式
    title.textContent = "编辑工位";
    elementCache.setValue("workstation-id", workstation.id);
    elementCache.setValue("workstation-name", workstation.name);
    elementCache.setValue("workstation-room", workstation.room_id);
    elementCache.setValue("workstation-manager", workstation.manager || "");
    elementCache.setValue("workstation-description", workstation.description || "");
  } else {
    // 添加模式
    title.textContent = "添加工位";
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
    showToast("工位名称不能为空", "warning");
    return;
  }

  if (!roomId) {
    showToast("请选择房间", "warning");
    return;
  }

  const workstationData = {
    name: name.trim(),
    room_id: roomId,
    manager: manager.trim() || null,
    description: description.trim() || null,
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
      loadWorkstationsData();
      showToast("工位保存成功", "success");
    } else {
      const errorMsg = result.message || "操作失败，请检查输入信息";
      showToast(`操作失败: ${errorMsg}`, "error");
      console.error("服务器返回错误:", result);
    }
  } catch (error) {
    console.error("提交工位表单失败:", error);
    showToast("操作失败，请重试", "error");
  }
}
