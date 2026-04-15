import { apiGet, } from "../utils/apiClient.js";
import { appendPaginationToTable, escapeHtml, DEFAULT_PAGE_SIZE, createSortState, updateSortIcons, initSortEvents, } from "../utils/ui.js";
import { openModal, closeModal } from "../utils/modal.js";
import { getManager, handleWorkstationRoomChange, } from "../utils/ipconfig.js";
import { loadRoomsForSelect, } from "../utils/resources.js";
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
      <td>{{ips}}</td>
      <td>{{manager}}</td>
      <td>{{ports}}</td>
      <td>{{description}}</td>
      <td>{{createdAt}}</td>
      <td>{{actions}}</td>
    </tr>
  `,
};
export async function editWorkstation(id) {
    await wrapAsync(async () => {
        const workstation = await workstationManager.get(id);
        if (workstation) {
            openWorkstationModal(workstation);
        }
    }, "获取工位数据失败")();
}
export async function deleteWorkstation(id) {
    const result = await workstationManager.delete(id, { confirmMessage: "确定要删除这个工位吗？" });
    if (result.success) {
        await loadWorkstationsData();
    }
}
function renderWorkstationRow(workstation, ipsHtml, portsHtml, index) {
    const displayName = `${escapeHtml(workstation.room_name)}-${escapeHtml(workstation.name)}`;
    return templateManager.render(workstationTemplates.tableRow, {
        id: workstation.id,
        index,
        displayName,
        ips: ipsHtml,
        manager: escapeHtml(workstation.manager) || "-",
        ports: portsHtml,
        description: escapeHtml(workstation.description) || "-",
        createdAt: new Date(workstation.created_at).toLocaleString(),
        actions: renderActionButtons(workstation.id),
    });
}
export async function loadWorkstationsData(page = 1, sortBy = null, sortOrder = null) {
    if (isLoading)
        return;
    isLoading = true;
    if (sortBy)
        tableState.setSort(sortBy, sortOrder);
    await wrapAsync(async () => {
        const result = await workstationManager.list({
            page,
            pageSize: DEFAULT_PAGE_SIZE,
            sort_by: tableState.sortBy,
            sort_order: tableState.sortOrder,
        });
        if (!result)
            return;
        const data = result.data;
        const tbody = document.querySelector("#workstations-table tbody");
        if (!tbody) {
            errorHandler.handle(errorHandler.createError("DOM_ERROR", "未找到工位表格元素", "error"));
            return;
        }
        tbody.innerHTML = "";
        const workstations = data.items || [];
        if (Array.isArray(workstations) && workstations.length > 0) {
            const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;
            const ipPromises = workstations.map(workstation => apiGet(`/api/resources/ip/workstation/${workstation.id}`)
                .then(ipsData => ({ workstation, ipsData }))
                .catch(() => ({ workstation, ipsData: { success: false, data: [] } })));
            const results = await Promise.all(ipPromises);
            const rows = [];
            for (const { workstation, ipsData } of results) {
                let ipsHtml = "-";
                let portsHtml = "-";
                const ipsResult = ipsData;
                if (ipsResult.success && ipsResult.data && ipsResult.data.length > 0) {
                    ipsHtml = ipsResult.data.map(ip => escapeHtml(ip.ip_address)).join("<br>");
                    const portInfos = ipsResult.data
                        .filter(ip => ip.switch_name && ip.switch_port_number)
                        .map(ip => `${escapeHtml(ip.switch_name)}: ${escapeHtml(ip.switch_port_number)}`);
                    portsHtml = portInfos.length > 0 ? portInfos.join("<br>") : "-";
                }
                rows.push(renderWorkstationRow(workstation, ipsHtml, portsHtml, startIndex + rows.length + 1));
            }
            tbody.innerHTML = rows.join("");
            if (data.total !== undefined) {
                appendPaginationToTable("#workstations-table", data, loadWorkstationsData);
            }
        }
        else {
            tbody.innerHTML = renderEmptyRow(8, "暂无工位数据");
        }
        updateSortIcons("workstations-table", tableState);
    }, "加载工位数据失败")();
    isLoading = false;
}
export function initWorkstationSortEvents() {
    initSortEvents("workstations-table", tableState, loadWorkstationsData);
}
export function initWorkstationTableEvents() {
    eventDelegator.on(document, "click", "#workstations-table .btn-edit", (_event, _target, data) => {
        if (data?.id)
            editWorkstation(data.id);
    });
    eventDelegator.on(document, "click", "#workstations-table .btn-delete", (_event, _target, data) => {
        if (data?.id)
            deleteWorkstation(data.id);
    });
}
export async function openWorkstationModal(workstation = null) {
    openModal("workstation-modal");
    const title = elementCache.get("workstation-modal-title");
    const form = elementCache.get("workstation-form");
    await loadRoomsForSelect(true);
    const ipManager = getManager("workstation");
    ipManager.clear();
    const roomSelect = elementCache.get("workstation-room");
    if (roomSelect) {
        roomSelect.removeEventListener("change", handleWorkstationRoomChange);
        roomSelect.addEventListener("change", handleWorkstationRoomChange);
    }
    if (workstation) {
        if (title)
            title.textContent = "编辑工位";
        elementCache.setValue("workstation-id", String(workstation.id));
        elementCache.setValue("workstation-name", workstation.name);
        elementCache.setValue("workstation-room", String(workstation.room_id));
        elementCache.setValue("workstation-manager", workstation.manager || "");
        elementCache.setValue("workstation-description", workstation.description || "");
        if (workstation.ips && workstation.ips.length > 0) {
            await ipManager.loadIps(workstation.ips);
        }
        else {
            await ipManager.addIpRow();
        }
    }
    else {
        if (title)
            title.textContent = "添加工位";
        form?.reset();
        elementCache.setValue("workstation-id", "");
    }
}
export async function submitWorkstationForm() {
    const form = document.getElementById("workstation-form");
    if (!form)
        return;
    const formData = new FormData(form);
    const id = formData.get("workstation-id");
    const name = formData.get("workstation-name");
    const roomId = formData.get("workstation-room");
    const manager = formData.get("workstation-manager");
    const description = formData.get("workstation-description");
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
        ips: validation.ips.map((ip) => ({ ...ip, device_type: "workstation" })),
        description: description.trim() || null,
    };
    await wrapAsync(async () => {
        let result;
        if (id) {
            result = await workstationManager.update(id, workstationData);
        }
        else {
            result = await workstationManager.create(workstationData);
        }
        if (result.success) {
            closeModal("workstation-modal");
            await loadWorkstationsData();
        }
    }, "保存工位数据失败")();
}
export function cleanup() {
    eventDelegator.off(document, "click", "#workstations-table .btn-edit");
    eventDelegator.off(document, "click", "#workstations-table .btn-delete");
}
//# sourceMappingURL=workstation.js.map