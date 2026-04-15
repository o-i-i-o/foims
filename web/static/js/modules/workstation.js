import { apiGet, } from "../utils/apiClient.js";
import { showToast, handleError, appendPaginationToTable, escapeHtml, DEFAULT_PAGE_SIZE, createSortState, updateSortIcons, initSortEvents, } from "../utils/ui.js";
import { openModal, closeModal } from "../utils/modal.js";
import { getManager, handleWorkstationRoomChange, } from "../utils/ipconfig.js";
import { loadRoomsForSelect, } from "../utils/resources.js";
import { elementCache } from "../utils/helpers.js";
import { workstationManager } from "../utils/managers.js";
const tableState = createSortState("name", "asc");
let isLoading = false;
export async function editWorkstation(id) {
    try {
        const workstation = await workstationManager.get(id);
        if (workstation) {
            openWorkstationModal(workstation);
        }
    }
    catch (error) {
        handleError(error, "获取工位数据失败");
    }
}
export async function deleteWorkstation(id) {
    const result = await workstationManager.delete(id, { confirmMessage: "确定要删除这个工位吗？" });
    if (result.success) {
        await loadWorkstationsData();
    }
}
export async function loadWorkstationsData(page = 1, sortBy = null, sortOrder = null) {
    if (isLoading)
        return;
    try {
        isLoading = true;
        if (sortBy)
            tableState.setSort(sortBy, sortOrder);
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
            console.error("未找到工位表格 tbody 元素");
            return;
        }
        tbody.innerHTML = "";
        const workstations = data.items || [];
        if (Array.isArray(workstations) && workstations.length > 0) {
            const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;
            const ipPromises = workstations.map(workstation => apiGet(`/api/resources/ip/workstation/${workstation.id}`)
                .then(ipsData => ({ workstation, ipsData }))
                .catch(ipsError => {
                console.error(`获取工位 ${workstation.id} 的IP地址失败:`, ipsError);
                return { workstation, ipsData: { success: false, data: [] } };
            }));
            const results = await Promise.all(ipPromises);
            let rowIndex = 0;
            for (const { workstation, ipsData } of results) {
                const displayName = `${escapeHtml(workstation.room_name)}-${escapeHtml(workstation.name)}`;
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
                const row = document.createElement("tr");
                row.innerHTML = `
                    <td class="index-column">${startIndex + rowIndex + 1}</td>
                    <td>${displayName}</td>
                    <td>${ipsHtml}</td>
                    <td>${escapeHtml(workstation.manager) || "-"}</td>
                    <td>${portsHtml}</td>
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
        }
        else {
            tbody.innerHTML =
                '<tr class="empty-row"><td colspan="8" class="text-center">暂无工位数据</td></tr>';
        }
        updateSortIcons("workstations-table", tableState);
    }
    catch (error) {
        console.error("加载工位数据失败:", error);
        const tbody = document.querySelector("#workstations-table tbody");
        if (tbody) {
            tbody.innerHTML =
                '<tr class="empty-row"><td colspan="8" class="text-center">加载失败，请刷新页面重试</td></tr>';
        }
    }
    finally {
        isLoading = false;
    }
}
export function initWorkstationSortEvents() {
    initSortEvents("workstations-table", tableState, loadWorkstationsData);
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
        ips: validation.ips.map((ip) => ({ ...ip, device_type: "workstation" })),
        description: description.trim() || null,
    };
    try {
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
    }
    catch (error) {
        handleError(error, "保存工位数据失败");
    }
}
//# sourceMappingURL=workstation.js.map