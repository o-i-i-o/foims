import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
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

import { openModal, closeModal } from "../utils/modal.js";

import {
  loadDataCenterRoomsForSelect,
  loadRoomNetworksForCabinet,
  loadCabinets,
} from "../utils/resources.js";

import type { Cabinet } from "../types/resources.js";

const tableState = createSortState("name", "asc");
let currentPage = 1;

let roomSelectHandler: ((event: Event) => Promise<void>) | null = null;

export async function loadCabinetsData(page = 1, sortBy: string | null = null, sortOrder: string | null = null): Promise<void> {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder as "asc" | "desc" | null);

  try {
    const result = await apiGet(`/api/resources/cabinets?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const cabinets = (data as { items?: unknown[] }).items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#cabinets-table", {
      data: cabinets as Record<string, unknown>[],
      columns: [
        { field: "id", render: (_v: unknown, _row: unknown, index: number) => startIndex + index + 1, className: "index-column" },
        { field: "name", render: (v: unknown, row: unknown) => {
          const r = row as Record<string, unknown>;
          const roomName = r.room_name ? `${escapeHtml(r.room_name as string)} / ` : "";
          return `${roomName}${escapeHtml(v as string)}`;
        }},
        { field: "networks", render: (v: unknown) => v && (v as unknown[]).length > 0 ? (v as { name: string; network_region: string }[]).map(n => `${escapeHtml(n.name)} (${escapeHtml(n.network_region)})`).join("<br>") : "-" },
        { field: "description", render: (v: unknown) => escapeHtml(v as string) || "-" },
        { field: "created_at", render: (v: unknown) => new Date(v as string).toLocaleString() },
        { field: "id", render: (v: unknown) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">删除</button>
        ` },
      ],
      emptyMessage: "暂无机柜数据",
    });

    if ((data as { total?: number }).total !== undefined) {
      appendPaginationToTable("#cabinets-table", data as { total?: number; page?: number; page_size?: number }, loadCabinetsData);
    }
    updateSortIcons("cabinets-table", tableState);
  } catch (error) {
    handleError(error, "加载机柜数据失败", () => {
      renderTable("#cabinets-table", { data: [], columns: [], emptyMessage: "加载失败，请刷新页面重试" });
    });
  }
}

export function initCabinetSortEvents(): void {
  initSortEvents("cabinets-table", tableState, loadCabinetsData);
}

export async function editCabinet(id: string | number): Promise<void> {
  try {
    const result = await apiGet(`/api/resources/cabinets/${id}`);
    if (result.success) {
      openCabinetModal(result.data as Record<string, unknown> | null);
    } else {
      showToast(`获取机柜数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取机柜数据失败");
  }
}

export async function deleteCabinet(id: string | number): Promise<void> {
  await handleDelete(id, "/api/resources/cabinets", "机柜删除成功", loadCabinetsData);
}

export async function openCabinetModal(cabinet: Record<string, unknown> | null = null): Promise<void> {
  openModal("cabinet-modal");

  const title = document.getElementById("cabinet-modal-title");
  const form = document.getElementById("cabinet-form") as HTMLFormElement | null;
  const capacityInput = document.getElementById("cabinet-capacity") as HTMLInputElement | null;

  await loadDataCenterRoomsForSelect();

  const roomSelect = document.getElementById("cabinet-room") as HTMLSelectElement | null;

  if (roomSelectHandler) {
    roomSelect?.removeEventListener("change", roomSelectHandler);
  }

  roomSelectHandler = async (event: Event) => {
    const roomId = (event.target as HTMLSelectElement).value;
    await loadRoomNetworksForCabinet(roomId);
  };

  roomSelect?.addEventListener("change", roomSelectHandler);

  if (cabinet) {
    if (title) title.textContent = "编辑机柜";
    (document.getElementById("cabinet-id") as HTMLInputElement).value = cabinet.id as string;
    (document.getElementById("cabinet-name") as HTMLInputElement).value = cabinet.name as string;
    if (cabinet.room_id) {
      (document.getElementById("cabinet-room") as HTMLSelectElement).value = cabinet.room_id as string;
      await loadRoomNetworksForCabinet(cabinet.room_id as string);
    }
    if (capacityInput) capacityInput.value = String(cabinet.capacity || 42);
    (document.getElementById("cabinet-description") as HTMLTextAreaElement).value = (cabinet.description as string) || "";
  } else {
    if (title) title.textContent = "添加机柜";
    form?.reset();
    (document.getElementById("cabinet-id") as HTMLInputElement).value = "";
    const inheritedNetworksContainer = document.getElementById("cabinet-inherited-networks");
    if (inheritedNetworksContainer) {
      inheritedNetworksContainer.innerHTML = '<p class="text-muted">请先选择所属房间，将自动继承房间的网段配置</p>';
    }
  }
}

export async function submitCabinetForm(): Promise<boolean | void> {
  const id = getElementValue("cabinet-id") as string;
  const name = getElementValue("cabinet-name") as string;
  const roomId = getElementValue("cabinet-room") as string;
  const capacityStr = getElementValue("cabinet-capacity") as string;
  const capacity = parseInt(capacityStr, 10);
  const description = getElementValue("cabinet-description") as string;

  if (!name.trim()) {
    showToast("机柜名称不能为空", "warning");
    return;
  }

  if (!roomId) {
    showToast("请选择所属机房", "warning");
    return;
  }

  if (isNaN(capacity) || capacity <= 0) {
    showToast("机柜容量必须是有效的正数", "warning");
    return;
  }

  const cabinetData = {
    name: name.trim(),
    room_id: roomId,
    capacity,
    description: description.trim() || null,
  };

  const success = await handleFormSubmit({
    formData: cabinetData,
    id,
    baseUrl: "/api/resources/cabinets",
    successMessage: "机柜保存成功",
    modalId: "cabinet-modal",
    reloadFunction: loadCabinetsData,
  });

  return success;
}

export async function loadCabinetsForModalSelect(): Promise<Cabinet[]> {
  try {
    const cabinets = await loadCabinets();
    const select = document.getElementById("cabinet-position-cabinet") as HTMLSelectElement | null;

    if (select) {
      select.innerHTML = "";

      const placeholder = document.createElement("option");
      placeholder.value = "";
      placeholder.textContent = "选择机柜";
      select.appendChild(placeholder);

      cabinets.forEach((cabinet) => {
        const option = document.createElement("option");
        option.value = cabinet.id as string;
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
