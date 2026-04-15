import {
  apiGet,
  apiPost,
  apiPut,
} from "../utils/apiClient.js";

import {
  showToast,
  getElementValue,
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
  IpConfigManager,
  getManager,
  handleWorkstationRoomChange,
} from "../utils/ipconfig.js";

import {
  loadRoomsForSelect,
} from "../utils/resources.js";

import { elementCache } from "../utils/helpers.js";

const tableState = createSortState("name", "asc");
let isLoading = false;
let currentPage = 1;

export async function editWorkstation(id: string | number): Promise<void> {
  try {
    const result = await apiGet(`/api/resources/workstations/${id}`);
    if (result.success) {
      openWorkstationModal(result.data as Record<string, unknown> | null);
    } else {
      showToast(`获取工位数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取工位数据失败");
  }
}

export async function deleteWorkstation(id: string | number): Promise<void> {
  await handleDelete(id, "/api/resources/workstations", "工位删除成功", loadWorkstationsData);
}

export async function loadWorkstationsData(page = 1, sortBy: string | null = null, sortOrder: string | null = null): Promise<void> {
  if (isLoading) return;

  try {
    isLoading = true;
    currentPage = page;
    if (sortBy) tableState.setSort(sortBy, sortOrder as "asc" | "desc" | null);

    const result = await apiGet(`/api/resources/workstations?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const tbody = document.querySelector("#workstations-table tbody");

    if (!tbody) {
      console.error("未找到工位表格 tbody 元素");
      return;
    }

    tbody.innerHTML = "";

    const data = result.success ? result.data : { items: [], total: 0 };
    const workstations = (data as { items?: Record<string, unknown>[] }).items || data;

    if (Array.isArray(workstations) && workstations.length > 0) {
      const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

      const ipPromises = workstations.map(workstation =>
        apiGet(`/api/resources/ip/workstation/${workstation.id}`)
          .then(ipsData => ({ workstation, ipsData }))
          .catch(ipsError => {
            console.error(`获取工位 ${workstation.id} 的IP地址失败:`, ipsError);
            return { workstation, ipsData: { success: false, data: [] } };
          })
      );

      const results = await Promise.all(ipPromises);
      let rowIndex = 0;

      for (const { workstation, ipsData } of results) {
        const displayName = `${escapeHtml(workstation.room_name as string)}-${escapeHtml(workstation.name as string)}`;

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

        const row = document.createElement("tr");
        row.innerHTML = `
                    <td class="index-column">${startIndex + rowIndex + 1}</td>
                    <td>${displayName}</td>
                    <td>${ipsHtml}</td>
                    <td>${escapeHtml(workstation.manager as string) || "-"}</td>
                    <td>${portsHtml}</td>
                    <td>${escapeHtml(workstation.description as string) || "-"}</td>
                    <td>${new Date(workstation.created_at as string).toLocaleString()}</td>
                    <td>
                        <button class="btn btn-sm btn-edit" data-id="${workstation.id}">编辑</button>
                        <button class="btn btn-sm btn-delete" data-id="${workstation.id}">删除</button>
                    </td>
                `;
        tbody.appendChild(row);
        rowIndex++;
      }

      if ((data as { total?: number }).total !== undefined) {
        appendPaginationToTable("#workstations-table", data as { total?: number; page?: number; page_size?: number }, loadWorkstationsData);
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

export function initWorkstationSortEvents(): void {
  initSortEvents("workstations-table", tableState, loadWorkstationsData);
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
      await ipManager.loadIps(workstation.ips as Record<string, unknown>[]);
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

  const ipManager = getManager("workstation");
  const validation = ipManager.validateIps();

  if (validation.errors && validation.errors.length > 0) {
    showToast(validation.errors[0], "warning");
    return;
  }

  if (validation.ips.length === 0) {
    showToast("请至少添加一个IP地址", "warning");
    return;
  }

  const workstationData = {
    name: name.trim(),
    room_id: roomId,
    manager: manager.trim() || null,
    ports: null,
    ips: validation.ips.map((ip: Record<string, unknown>) => ({ ...ip, device_type: "workstation" })),
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
