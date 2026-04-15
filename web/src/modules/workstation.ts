import {
  apiGet,
} from "../utils/apiClient.js";

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
  getManager,
  handleWorkstationRoomChange,
} from "../utils/ipconfig.js";

import type { IpAssignment } from "../types/resources.js";

import {
  loadRoomsForSelect,
} from "../utils/resources.js";

import { elementCache } from "../utils/helpers.js";
import { workstationManager } from "../utils/managers.js";
import { eventDelegator } from "../utils/eventDelegator.js";
import { templateManager, renderEmptyRow, renderActionButtons } from "../utils/templateManager.js";
import { errorHandler, wrapAsync } from "../utils/errorHandler.js";

const tableState = createSortState("name", "asc");
let isLoading = false;

const workstationTemplates = {
  tableRow: `
    <tr data-id="{{id}}">
      <td class="index-column">{{index}}</td>
      <td>{{displayName}}</td>
      <td>{{{ips}}}</td>
      <td>{{manager}}</td>
      <td>{{{ports}}}</td>
      <td>{{description}}</td>
      <td>{{createdAt}}</td>
      <td>{{{actions}}}</td>
    </tr>
  `,
};

export async function editWorkstation(id: string | number): Promise<void> {
  await wrapAsync(async () => {
    const workstation = await workstationManager.get(id);
    if (workstation) {
      openWorkstationModal(workstation as unknown as Record<string, unknown>);
    }
  }, "获取工位数据失败")();
}

export async function deleteWorkstation(id: string | number): Promise<void> {
  const result = await workstationManager.delete(id, { confirmMessage: "确定要删除这个工位吗？" });
  if (result.success) {
    await loadWorkstationsData();
  }
}

function renderWorkstationRow(workstation: Record<string, unknown>, ipsHtml: string, portsHtml: string, index: number): string {
  const displayName = `${escapeHtml(workstation.room_name as string)}-${escapeHtml(workstation.name as string)}`;

  return templateManager.render(workstationTemplates.tableRow, {
    id: workstation.id,
    index,
    displayName,
    ips: ipsHtml,
    manager: escapeHtml(workstation.manager as string) || "-",
    ports: portsHtml,
    description: escapeHtml(workstation.description as string) || "-",
    createdAt: new Date(workstation.created_at as string).toLocaleString(),
    actions: renderActionButtons(workstation.id as string),
  });
}

export async function loadWorkstationsData(page = 1, sortBy: string | null = null, sortOrder: string | null = null): Promise<void> {
  if (isLoading) return;

  isLoading = true;
  if (sortBy) tableState.setSort(sortBy, sortOrder as "asc" | "desc" | null);

  await wrapAsync(async () => {
    const result = await workstationManager.list({
      page,
      pageSize: DEFAULT_PAGE_SIZE,
      sort_by: tableState.sortBy,
      sort_order: tableState.sortOrder,
    });

    if (!result) return;

    const data = result.data as { items?: Record<string, unknown>[]; total?: number };
    const tbody = document.querySelector("#workstations-table tbody");

    if (!tbody) {
      errorHandler.handle(errorHandler.createError("DOM_ERROR", "未找到工位表格元素", "error"));
      return;
    }

    tbody.innerHTML = "";

    const workstations = data.items || [];

    if (Array.isArray(workstations) && workstations.length > 0) {
      const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

      const ipPromises = workstations.map(workstation =>
        apiGet(`/api/resources/ip/workstation/${workstation.id}`)
          .then(ipsData => ({ workstation, ipsData }))
          .catch(() => ({ workstation, ipsData: { success: false, data: [] } }))
      );

      const results = await Promise.all(ipPromises);
      const rows: string[] = [];

      for (const { workstation, ipsData } of results) {
        let ipsHtml = "-";
        let portsHtml = "-";
        const ipsResult = ipsData as { success?: boolean; data?: Record<string, unknown>[] };
        if (ipsResult.success && ipsResult.data && ipsResult.data.length > 0) {
          ipsHtml = ipsResult.data.map(ip => escapeHtml(ip.ip_address as string)).join("<br>");

          const portInfos = ipsResult.data
            .filter(ip => ip.switch_name && ip.switch_port_number)
            .map(ip => `${escapeHtml(ip.switch_name as string)}: ${escapeHtml(ip.switch_port_number as string)}`);
          portsHtml = portInfos.length > 0 ? portInfos.join("<br>") : "-";
        }

        rows.push(renderWorkstationRow(workstation, ipsHtml, portsHtml, startIndex + rows.length + 1));
      }

      tbody.innerHTML = rows.join("");

      if (data.total !== undefined) {
        appendPaginationToTable("#workstations-table", data, loadWorkstationsData);
      }
    } else {
      tbody.innerHTML = renderEmptyRow(8, "暂无工位数据");
    }
    updateSortIcons("workstations-table", tableState);
  }, "加载工位数据失败")();

  isLoading = false;
}

export function initWorkstationSortEvents(): void {
  initSortEvents("workstations-table", tableState, loadWorkstationsData);
}

export function initWorkstationTableEvents(): void {
  eventDelegator.on(document, "click", "#workstations-table .btn-edit", (_event, _target, data) => {
    if (data?.id) editWorkstation(data.id);
  });
  eventDelegator.on(document, "click", "#workstations-table .btn-delete", (_event, _target, data) => {
    if (data?.id) deleteWorkstation(data.id);
  });
}

export async function openWorkstationModal(workstation: Record<string, unknown> | null = null): Promise<void> {
  openModal("workstation-modal");

  const title = elementCache.get("workstation-modal-title");
  const form = elementCache.get("workstation-form") as HTMLFormElement | null;

  await loadRoomsForSelect(true);

  const ipManager = getManager("workstation");
  ipManager.clear();

  const roomSelect = elementCache.get("workstation-room") as HTMLSelectElement | null;

  if (roomSelect) {
    roomSelect.removeEventListener("change", handleWorkstationRoomChange);
    roomSelect.addEventListener("change", handleWorkstationRoomChange);
  }

  if (workstation) {
    if (title) title.textContent = "编辑工位";
    elementCache.setValue("workstation-id", String(workstation.id));
    elementCache.setValue("workstation-name", workstation.name as string);
    elementCache.setValue("workstation-room", String(workstation.room_id));
    elementCache.setValue("workstation-manager", (workstation.manager as string) || "");
    elementCache.setValue("workstation-description", (workstation.description as string) || "");

    if (workstation.ips && (workstation.ips as unknown[]).length > 0) {
      await ipManager.loadIps(workstation.ips as unknown as IpAssignment[]);
    } else {
      await ipManager.addIpRow();
    }
  } else {
    if (title) title.textContent = "添加工位";
    form?.reset();
    elementCache.setValue("workstation-id", "");
  }
}

export async function submitWorkstationForm(): Promise<void> {
  const form = document.getElementById("workstation-form") as HTMLFormElement | null;
  if (!form) return;

  const formData = new FormData(form);
  const id = formData.get("workstation-id") as string;
  const name = formData.get("workstation-name") as string;
  const roomId = formData.get("workstation-room") as string;
  const manager = formData.get("workstation-manager") as string;
  const description = formData.get("workstation-description") as string;

  const ipManager = getManager("workstation");
  const validation = ipManager.validateIps();

  if (validation.errors && validation.errors.length > 0) {
    errorHandler.handle(errorHandler.createError("VALIDATION_ERROR", validation.errors[0], "warning"));
    return;
  }

  if (validation.ips.length === 0) {
    errorHandler.handle(errorHandler.createError("VALIDATION_ERROR", "请至少添加一个IP地址", "warning"));
    return;
  }

  const workstationData = {
    name: name.trim(),
    room_id: roomId,
    manager: manager.trim() || null,
    ports: null,
    ips: validation.ips.map((ip: IpAssignment) => ({ ...ip, device_type: "workstation" })),
    description: description.trim() || null,
  };

  await wrapAsync(async () => {
    let result;
    if (id) {
      result = await workstationManager.update(id, workstationData);
    } else {
      result = await workstationManager.create(workstationData);
    }

    if (result.success) {
      closeModal("workstation-modal");
      await loadWorkstationsData();
    }
  }, "保存工位数据失败")();
}

export function cleanup(): void {
  eventDelegator.off(document, "click", "#workstations-table .btn-edit");
  eventDelegator.off(document, "click", "#workstations-table .btn-delete");
}
