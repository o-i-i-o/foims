import {
  showToast,
  handleError,
  appendPaginationToTable,
  escapeHtml,
  DEFAULT_PAGE_SIZE,
  createSortState,
  updateSortIcons,
  initSortEvents,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { elementCache } from "../utils/helpers.js";
import { getManager, handleCabinetPositionCabinetChange } from "../utils/ipconfig.js";
import { loadCabinetsForModalSelect } from "./cabinet.js";
import { cabinetPositionManager } from "../utils/managers.js";
import type { IpAssignment } from "../types/resources.js";

const tableState = createSortState("name", "asc");
let isLoading = false;

export async function loadCabinetPositionsData(page = 1, sortBy: string | null = null, sortOrder: string | null = null): Promise<void> {
  if (sortBy) tableState.setSort(sortBy, sortOrder as "asc" | "desc" | null);

  if (isLoading) {
    return;
  }
  try {
    isLoading = true;
    const result = await cabinetPositionManager.list({
      page,
      pageSize: DEFAULT_PAGE_SIZE,
      sort_by: tableState.sortBy,
      sort_order: tableState.sortOrder,
    });

    if (!result) return;

    const data = result.data as { items?: Record<string, unknown>[]; total?: number };
    const tbody = document.querySelector("#cabinet-positions-table tbody");

    if (!tbody) {
      console.error("未找到机位表格 tbody 元素");
      return;
    }

    tbody.innerHTML = "";

    const positions = data.items || [];

    if (!Array.isArray(positions) || positions.length === 0) {
      tbody.innerHTML = `<tr class="empty-row"><td colspan="7" class="text-center">暂无机位数据</td></tr>`;
      return;
    }

    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    positions.forEach((position: Record<string, unknown>, index: number) => {
      const row = document.createElement("tr");
      row.innerHTML = `
        <td class="index-column">${startIndex + index + 1}</td>
        <td>${escapeHtml(position.name as string)}</td>
        <td>${escapeHtml(position.cabinet_name as string) || "-"}</td>
        <td>U${position.start_u} - U${position.end_u}</td>
        <td>${(position.ips as unknown[])?.length || 0}</td>
        <td>${escapeHtml(position.description as string) || "-"}</td>
        <td>
          <button class="btn btn-secondary btn-sm btn-edit" data-id="${position.id}">编辑</button>
          <button class="btn btn-danger btn-sm btn-delete" data-id="${position.id}">删除</button>
        </td>
      `;
      tbody.appendChild(row);
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#cabinet-positions-table", data as { total?: number; page?: number; page_size?: number }, loadCabinetPositionsData);
    }

    updateSortIcons("cabinet-positions-table", tableState);
  } catch (error) {
    handleError(error, "加载机位数据失败");
    const tbody = document.querySelector("#cabinet-positions-table tbody");
    if (tbody) {
      tbody.innerHTML = `<tr class="empty-row"><td colspan="7" class="text-center">加载失败，请刷新页面重试</td></tr>`;
    }
  } finally {
    isLoading = false;
  }
}

export function initCabinetPositionSortEvents(): void {
  initSortEvents("cabinet-positions-table", tableState, loadCabinetPositionsData);
}

export async function editCabinetPosition(id: string | number): Promise<void> {
  try {
    const position = await cabinetPositionManager.get(id);
    if (position) {
      openCabinetPositionModal(position as unknown as Record<string, unknown>);
    }
  } catch (error) {
    handleError(error, "获取机位数据失败");
  }
}

export async function deleteCabinetPosition(id: string | number): Promise<void> {
  const result = await cabinetPositionManager.delete(id, { confirmMessage: "确定要删除这个机位吗？" });
  if (result.success) {
    await loadCabinetPositionsData();
  }
}

export async function submitCabinetPositionForm(): Promise<boolean> {
  const form = document.getElementById("cabinet-position-form") as HTMLFormElement | null;
  if (!form) return false;

  const formData = new FormData(form);
  const id = formData.get("cabinet-position-id") as string;
  const name = formData.get("cabinet-position-name") as string;
  const cabinetId = formData.get("cabinet-position-cabinet") as string;
  const startUStr = formData.get("cabinet-position-start-u") as string;
  const endUStr = formData.get("cabinet-position-end-u") as string;
  const description = formData.get("cabinet-position-description") as string;

  const startU = parseInt(startUStr, 10);
  const endU = parseInt(endUStr, 10);

  const ipManager = getManager("cabinet-position");
  const { valid, errors, ips } = ipManager.validateIps();

  if (!valid) {
    showToast(errors?.[0] || "IP配置验证失败", "warning");
    return false;
  }

  const positionData = {
    name: name.trim(),
    cabinet_id: cabinetId,
    start_u: startU,
    end_u: endU,
    description: description.trim() || null,
    ips: ips,
  };

  try {
    let result;
    if (id) {
      result = await cabinetPositionManager.update(id, positionData);
    } else {
      result = await cabinetPositionManager.create(positionData);
    }

    if (result.success) {
      closeModal("cabinet-position-modal");
      await loadCabinetPositionsData();
      return true;
    }
    return false;
  } catch (error) {
    handleError(error, "保存机位数据失败");
    return false;
  }
}

export async function openCabinetPositionModal(position: Record<string, unknown> | null = null): Promise<void> {
  openModal("cabinet-position-modal");

  const title = elementCache.get("cabinet-position-modal-title");
  const form = elementCache.get("cabinet-position-form") as HTMLFormElement | null;

  await loadCabinetsForModalSelect();

  const cabinetSelect = document.getElementById("cabinet-position-cabinet") as HTMLSelectElement | null;
  if (cabinetSelect) {
    cabinetSelect.addEventListener("change", handleCabinetPositionCabinetChange);
  }

  const ipManager = getManager("cabinet-position");
  ipManager.clear();

  if (position) {
    if (title) title.textContent = "编辑机位";
    elementCache.setValue("cabinet-position-id", String(position.id));
    elementCache.setValue("cabinet-position-name", position.name as string);
    elementCache.setValue("cabinet-position-cabinet", String(position.cabinet_id));
    elementCache.setValue("cabinet-position-start-u", String(position.start_u || 1));
    elementCache.setValue("cabinet-position-end-u", String(position.end_u || 1));
    elementCache.setValue("cabinet-position-description", (position.description as string) || "");

    if (position.ips && (position.ips as unknown[]).length > 0) {
      await ipManager.loadIps(position.ips as unknown as IpAssignment[]);
    } else if (position.cabinet_id) {
      await ipManager.addIpRow();
    }
  } else {
    if (title) title.textContent = "添加机位";
    form?.reset();
    elementCache.setValue("cabinet-position-id", "");
  }
}
