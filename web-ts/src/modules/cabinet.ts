import {
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

import { cabinetManager } from "../utils/managers.js";
import { eventDelegator, setupTableEvents } from "../utils/eventDelegator.js";
import { templateManager, renderEmptyRow, renderActionButtons } from "../utils/templateManager.js";
import { errorHandler, wrapAsync } from "../utils/errorHandler.js";
import type { Cabinet } from "../types/resources.js";

const tableState = createSortState("name", "asc");

let roomSelectHandler: ((event: Event) => Promise<void>) | null = null;

const cabinetTemplates = {
  tableRow: `
    <tr data-id="{{id}}">
      <td class="index-column">{{index}}</td>
      <td>{{roomName}}{{name}}</td>
      <td>{{{networks}}}</td>
      <td>{{description}}</td>
      <td>{{createdAt}}</td>
      <td>{{{actions}}}</td>
    </tr>
  `,
};

function renderCabinetRow(cabinet: Record<string, unknown>, index: number): string {
  const roomName = cabinet.room_name ? `${escapeHtml(cabinet.room_name as string)} / ` : "";
  const networks = cabinet.networks && (cabinet.networks as unknown[]).length > 0
    ? (cabinet.networks as { name: string; network_region: string }[])
        .map(n => `${escapeHtml(n.name)} (${escapeHtml(n.network_region)})`)
        .join("<br>")
    : "-";

  return templateManager.render(cabinetTemplates.tableRow, {
    id: cabinet.id,
    index,
    roomName,
    name: escapeHtml(cabinet.name as string),
    networks,
    description: escapeHtml(cabinet.description as string) || "-",
    createdAt: new Date(cabinet.created_at as string).toLocaleString(),
    actions: renderActionButtons(cabinet.id as string),
  });
}

export async function loadCabinetsData(page = 1, sortBy: string | null = null, sortOrder: string | null = null): Promise<void> {
  if (sortBy) tableState.setSort(sortBy, sortOrder as "asc" | "desc" | null);

  await wrapAsync(async () => {
    const result = await cabinetManager.list({
      page,
      pageSize: DEFAULT_PAGE_SIZE,
      sort_by: tableState.sortBy,
      sort_order: tableState.sortOrder,
    });

    if (!result) return;

    const data = result.data as { items?: Cabinet[]; total?: number };
    const cabinets = data.items || [];
    const tbody = document.querySelector("#cabinets-table tbody");

    if (!tbody) {
      errorHandler.handle(errorHandler.createError("DOM_ERROR", "未找到机柜表格元素", "error"));
      return;
    }

    if (cabinets.length === 0) {
      tbody.innerHTML = renderEmptyRow(6, "暂无机柜数据");
      return;
    }

    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;
    tbody.innerHTML = cabinets
      .map((cabinet, index) => renderCabinetRow(cabinet as unknown as Record<string, unknown>, startIndex + index + 1))
      .join("");

    if (data.total !== undefined) {
      appendPaginationToTable("#cabinets-table", data as { total?: number; page?: number; page_size?: number }, loadCabinetsData);
    }
    updateSortIcons("cabinets-table", tableState);
  }, "加载机柜数据失败")();
}

export function initCabinetSortEvents(): void {
  initSortEvents("cabinets-table", tableState, loadCabinetsData);
}

export function initCabinetTableEvents(): void {
  setupTableEvents(document, "#cabinets-table", {
    onEdit: (id) => editCabinet(id),
    onDelete: (id) => deleteCabinet(id),
  });
}

export async function editCabinet(id: string | number): Promise<void> {
  await wrapAsync(async () => {
    const cabinet = await cabinetManager.get(id);
    if (cabinet) {
      openCabinetModal(cabinet as unknown as Record<string, unknown>);
    }
  }, "获取机柜数据失败")();
}

export async function deleteCabinet(id: string | number): Promise<void> {
  const result = await cabinetManager.delete(id, { confirmMessage: "确定要删除这个机柜吗？" });
  if (result.success) {
    await loadCabinetsData();
  }
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
    (document.getElementById("cabinet-id") as HTMLInputElement).value = String(cabinet.id);
    (document.getElementById("cabinet-name") as HTMLInputElement).value = cabinet.name as string;
    if (cabinet.room_id) {
      (document.getElementById("cabinet-room") as HTMLSelectElement).value = String(cabinet.room_id);
      await loadRoomNetworksForCabinet(String(cabinet.room_id));
    }
    if (capacityInput) capacityInput.value = String(cabinet.capacity || cabinet.total_units || 42);
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
  const form = document.getElementById("cabinet-form") as HTMLFormElement | null;
  if (!form) return;

  const formData = new FormData(form);
  const id = formData.get("cabinet-id") as string;
  const name = formData.get("cabinet-name") as string;
  const roomId = formData.get("cabinet-room") as string;
  const capacityStr = formData.get("cabinet-capacity") as string;
  const capacity = parseInt(capacityStr, 10);
  const description = formData.get("cabinet-description") as string;

  if (isNaN(capacity) || capacity <= 0) {
    errorHandler.handle(errorHandler.createError("VALIDATION_ERROR", "机柜容量必须是有效的正数", "warning"));
    return;
  }

  const cabinetData = {
    name: name.trim(),
    room_id: roomId,
    capacity,
    description: description.trim() || null,
  };

  await wrapAsync(async () => {
    let result;
    if (id) {
      result = await cabinetManager.update(id, cabinetData);
    } else {
      result = await cabinetManager.create(cabinetData);
    }

    if (result.success) {
      closeModal("cabinet-modal");
      await loadCabinetsData();
      return true;
    }
  }, "保存机柜数据失败")();
}

export async function loadCabinetsForModalSelect(): Promise<Cabinet[]> {
  return wrapAsync(async () => {
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
        option.value = String(cabinet.id);
        option.textContent = cabinet.name;
        select.appendChild(option);
      });

      return cabinets;
    }
    return [];
  }, "加载机柜选项失败")() || [];
}

export function cleanup(): void {
  if (roomSelectHandler) {
    const roomSelect = document.getElementById("cabinet-room") as HTMLSelectElement | null;
    roomSelect?.removeEventListener("change", roomSelectHandler);
    roomSelectHandler = null;
  }
  eventDelegator.off(document, "click", "#cabinets-table .btn-edit");
  eventDelegator.off(document, "click", "#cabinets-table .btn-delete");
}
