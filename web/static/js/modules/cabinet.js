import { appendPaginationToTable, escapeHtml, DEFAULT_PAGE_SIZE, createSortState, updateSortIcons, initSortEvents, } from "../utils/ui.js";
import { openModal, closeModal } from "../utils/modal.js";
import { loadDataCenterRoomsForSelect, loadRoomNetworksForCabinet, loadCabinets, } from "../utils/resources.js";
import { cabinetManager } from "../utils/managers.js";
import { eventDelegator, setupTableEvents } from "../utils/eventDelegator.js";
import { templateManager, renderEmptyRow, renderActionButtons } from "../utils/templateManager.js";
import { errorHandler, wrapAsync } from "../utils/errorHandler.js";
const tableState = createSortState("name", "asc");
let roomSelectHandler = null;
const cabinetTemplates = {
    tableRow: `
    <tr data-id="{{id}}">
      <td class="index-column">{{index}}</td>
      <td>{{roomName}}{{name}}</td>
      <td>{{networks}}</td>
      <td>{{description}}</td>
      <td>{{createdAt}}</td>
      <td>{{actions}}</td>
    </tr>
  `,
};
function renderCabinetRow(cabinet, index) {
    const roomName = cabinet.room_name ? `${escapeHtml(cabinet.room_name)} / ` : "";
    const networks = cabinet.networks && cabinet.networks.length > 0
        ? cabinet.networks
            .map(n => `${escapeHtml(n.name)} (${escapeHtml(n.network_region)})`)
            .join("<br>")
        : "-";
    return templateManager.render(cabinetTemplates.tableRow, {
        id: cabinet.id,
        index,
        roomName,
        name: escapeHtml(cabinet.name),
        networks,
        description: escapeHtml(cabinet.description) || "-",
        createdAt: new Date(cabinet.created_at).toLocaleString(),
        actions: renderActionButtons(cabinet.id),
    });
}
export async function loadCabinetsData(page = 1, sortBy = null, sortOrder = null) {
    if (sortBy)
        tableState.setSort(sortBy, sortOrder);
    await wrapAsync(async () => {
        const result = await cabinetManager.list({
            page,
            pageSize: DEFAULT_PAGE_SIZE,
            sort_by: tableState.sortBy,
            sort_order: tableState.sortOrder,
        });
        if (!result)
            return;
        const data = result.data;
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
            .map((cabinet, index) => renderCabinetRow(cabinet, startIndex + index + 1))
            .join("");
        if (data.total !== undefined) {
            appendPaginationToTable("#cabinets-table", data, loadCabinetsData);
        }
        updateSortIcons("cabinets-table", tableState);
    }, "加载机柜数据失败")();
}
export function initCabinetSortEvents() {
    initSortEvents("cabinets-table", tableState, loadCabinetsData);
}
export function initCabinetTableEvents() {
    setupTableEvents(document, "#cabinets-table", {
        onEdit: (id) => editCabinet(id),
        onDelete: (id) => deleteCabinet(id),
    });
}
export async function editCabinet(id) {
    await wrapAsync(async () => {
        const cabinet = await cabinetManager.get(id);
        if (cabinet) {
            openCabinetModal(cabinet);
        }
    }, "获取机柜数据失败")();
}
export async function deleteCabinet(id) {
    const result = await cabinetManager.delete(id, { confirmMessage: "确定要删除这个机柜吗？" });
    if (result.success) {
        await loadCabinetsData();
    }
}
export async function openCabinetModal(cabinet = null) {
    openModal("cabinet-modal");
    const title = document.getElementById("cabinet-modal-title");
    const form = document.getElementById("cabinet-form");
    const capacityInput = document.getElementById("cabinet-capacity");
    await loadDataCenterRoomsForSelect();
    const roomSelect = document.getElementById("cabinet-room");
    if (roomSelectHandler) {
        roomSelect?.removeEventListener("change", roomSelectHandler);
    }
    roomSelectHandler = async (event) => {
        const roomId = event.target.value;
        await loadRoomNetworksForCabinet(roomId);
    };
    roomSelect?.addEventListener("change", roomSelectHandler);
    if (cabinet) {
        if (title)
            title.textContent = "编辑机柜";
        document.getElementById("cabinet-id").value = String(cabinet.id);
        document.getElementById("cabinet-name").value = cabinet.name;
        if (cabinet.room_id) {
            document.getElementById("cabinet-room").value = String(cabinet.room_id);
            await loadRoomNetworksForCabinet(String(cabinet.room_id));
        }
        if (capacityInput)
            capacityInput.value = String(cabinet.capacity || cabinet.total_units || 42);
        document.getElementById("cabinet-description").value = cabinet.description || "";
    }
    else {
        if (title)
            title.textContent = "添加机柜";
        form?.reset();
        document.getElementById("cabinet-id").value = "";
        const inheritedNetworksContainer = document.getElementById("cabinet-inherited-networks");
        if (inheritedNetworksContainer) {
            inheritedNetworksContainer.innerHTML = '<p class="text-muted">请先选择所属房间，将自动继承房间的网段配置</p>';
        }
    }
}
export async function submitCabinetForm() {
    const form = document.getElementById("cabinet-form");
    if (!form)
        return;
    const formData = new FormData(form);
    const id = formData.get("cabinet-id");
    const name = formData.get("cabinet-name");
    const roomId = formData.get("cabinet-room");
    const capacityStr = formData.get("cabinet-capacity");
    const capacity = parseInt(capacityStr, 10);
    const description = formData.get("cabinet-description");
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
        }
        else {
            result = await cabinetManager.create(cabinetData);
        }
        if (result.success) {
            closeModal("cabinet-modal");
            await loadCabinetsData();
            return true;
        }
    }, "保存机柜数据失败")();
}
export async function loadCabinetsForModalSelect() {
    return wrapAsync(async () => {
        const cabinets = await loadCabinets();
        const select = document.getElementById("cabinet-position-cabinet");
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
export function cleanup() {
    if (roomSelectHandler) {
        const roomSelect = document.getElementById("cabinet-room");
        roomSelect?.removeEventListener("change", roomSelectHandler);
        roomSelectHandler = null;
    }
    eventDelegator.off(document, "click", "#cabinets-table .btn-edit");
    eventDelegator.off(document, "click", "#cabinets-table .btn-delete");
}
//# sourceMappingURL=cabinet.js.map