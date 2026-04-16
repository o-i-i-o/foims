import { apiGet, } from "../utils/apiClient.js";
import { showToast, appendPaginationToTable, escapeHtml, DEFAULT_PAGE_SIZE, createSortState, updateSortIcons, initSortEvents, } from "../utils/ui.js";
import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";
import { roomManager } from "../utils/managers.js";
import { eventDelegator } from "../utils/eventDelegator.js";
import { templateManager, renderEmptyRow, renderActionButtons } from "../utils/templateManager.js";
import { errorHandler, wrapAsync } from "../utils/errorHandler.js";
const networkCache = {
    networkRegions: null,
    networks: new Map(),
    cacheTime: 0,
    CACHE_TTL: 60 * 1000,
};
function isCacheValid(timestamp) {
    return !!timestamp && (Date.now() - timestamp < networkCache.CACHE_TTL);
}
function extractItems(result) {
    if (!result.success || !result.data)
        return [];
    if (Array.isArray(result.data))
        return result.data;
    if ("items" in result.data && Array.isArray(result.data.items))
        return result.data.items;
    return [];
}
const selectOptionTemplate = '<option value="{{value}}">{{label}}</option>';
function updateSelect(select, data, placeholder = "选择选项") {
    if (!select)
        return null;
    const currentValue = select.value;
    const options = data.map(item => templateManager.render(selectOptionTemplate, { value: item.id, label: item.name })).join("");
    select.innerHTML = `<option value="">${placeholder}</option>${options}`;
    if (currentValue)
        select.value = currentValue;
    return select;
}
async function loadNetworkRegions(select) {
    if (!select)
        return [];
    const now = Date.now();
    if (networkCache.networkRegions && isCacheValid(networkCache.cacheTime)) {
        updateSelect(select, networkCache.networkRegions, "选择网络区域");
        return networkCache.networkRegions;
    }
    return wrapAsync(async () => {
        const result = await apiGet("/api/resources/network-regions?page_size=1000");
        const items = extractItems(result).map(item => ({ id: item.id, name: item.name }));
        if (items.length > 0) {
            networkCache.networkRegions = items;
            networkCache.cacheTime = now;
            updateSelect(select, items, "选择网络区域");
            return items;
        }
        return [];
    }, "加载网络区域失败")() || [];
}
async function loadNetworks(regionId, select, excludeIds = []) {
    if (!select)
        return [];
    const cacheKey = String(regionId || "all");
    const cached = networkCache.networks.get(cacheKey);
    if (cached && isCacheValid(cached.timestamp)) {
        const filtered = cached.data.filter(n => !excludeIds.includes(n.id));
        updateSelect(select, filtered, "选择网段");
        return filtered;
    }
    return wrapAsync(async () => {
        const url = regionId ? `/api/resources/networks?region_id=${regionId}&page_size=1000` : "/api/resources/networks?page_size=1000";
        const result = await apiGet(url);
        const items = extractItems(result).map(item => ({ id: item.id, name: item.name }));
        if (items.length > 0) {
            networkCache.networks.set(cacheKey, { data: items, timestamp: Date.now() });
            const filtered = items.filter(n => !excludeIds.includes(n.id));
            updateSelect(select, filtered, "选择网段");
            return filtered;
        }
        return [];
    }, "加载网段失败")() || [];
}
const networkConfigItemTemplate = `
  <div class="network-config-item">
    <div class="form-row">
      <div class="form-group">
        <label>网络区域<span class="required">*</span></label>
        <select class="{{regionSelectClass}}" required>
          <option value="">选择网络区域</option>
        </select>
      </div>
      <div class="form-group">
        <label>网段选择<span class="required">*</span></label>
        <select class="{{networkSelectClass}}" required>
          <option value="">选择网段</option>
        </select>
      </div>
      <div class="form-group">
        <button type="button" class="btn btn-danger btn-sm {{removeBtnClass}}">删除</button>
        <button type="button" class="btn btn-secondary btn-sm add-btn" style="display: none;">新增</button>
      </div>
    </div>
  </div>
`;
class NetworkConfigManager {
    options;
    container;
    boundHandlers = new Map();
    constructor(options) {
        this.options = options;
        this.container = null;
    }
    async init() {
        this.container = document.getElementById(this.options.containerId);
        if (!this.container) {
            errorHandler.handle(errorHandler.createError("DOM_ERROR", `未找到${this.options.containerId}元素`, "error"));
            return false;
        }
        this.container.innerHTML = "";
        await this.addItem();
        this.updateAddButtons();
        return true;
    }
    createItemHTML() {
        return templateManager.render(networkConfigItemTemplate, {
            regionSelectClass: this.options.regionSelectClass,
            networkSelectClass: this.options.networkSelectClass,
            removeBtnClass: this.options.removeBtnClass,
        });
    }
    async addItem() {
        if (!this.container)
            return;
        const div = document.createElement("div");
        div.innerHTML = this.createItemHTML();
        const item = div.firstElementChild;
        if (!item)
            return;
        this.container.appendChild(item);
        const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
        await loadNetworkRegions(regionSelect);
        this.bindItemEvents(item);
        this.updateAddButtons();
        return item;
    }
    updateAddButtons() {
        if (!this.container)
            return;
        const items = this.container.querySelectorAll(".network-config-item");
        items.forEach((item, index) => {
            const addBtn = item.querySelector(".add-btn");
            if (addBtn) {
                addBtn.style.display = index === items.length - 1 ? "" : "none";
            }
        });
    }
    bindItemEvents(item) {
        const { regionSelectClass, networkSelectClass, removeBtnClass } = this.options;
        const removeBtn = item.querySelector(`.${removeBtnClass}`);
        if (removeBtn) {
            const handler = () => this.removeItem(item);
            removeBtn.addEventListener("click", handler);
            this.storeHandler(removeBtn, "click", handler);
        }
        const addBtn = item.querySelector(".add-btn");
        if (addBtn) {
            const handler = () => this.addItem();
            addBtn.addEventListener("click", handler);
            this.storeHandler(addBtn, "click", handler);
        }
        const regionSelect = item.querySelector(`.${regionSelectClass}`);
        if (regionSelect) {
            const handler = async () => {
                await this.onRegionChange(regionSelect, item);
                this.updateNetworkSelects();
            };
            regionSelect.addEventListener("change", handler);
            this.storeHandler(regionSelect, "change", handler);
        }
        const networkSelect = item.querySelector(`.${networkSelectClass}`);
        if (networkSelect) {
            const handler = () => this.updateNetworkSelects();
            networkSelect.addEventListener("change", handler);
            this.storeHandler(networkSelect, "change", handler);
        }
    }
    storeHandler(element, event, handler) {
        if (!this.boundHandlers.has(element)) {
            this.boundHandlers.set(element, new Map());
        }
        this.boundHandlers.get(element).set(event, handler);
    }
    removeItem(item) {
        const items = this.container.querySelectorAll(".network-config-item");
        if (items.length <= 1) {
            showToast("至少需要保留一个网段配置", "warning");
            return;
        }
        this.cleanupItemHandlers(item);
        item.remove();
        this.updateAddButtons();
        this.updateNetworkSelects();
    }
    cleanupItemHandlers(item) {
        const elements = item.querySelectorAll("*");
        elements.forEach(el => {
            const handlers = this.boundHandlers.get(el);
            if (handlers) {
                handlers.forEach((handler, event) => {
                    el.removeEventListener(event, handler);
                });
                this.boundHandlers.delete(el);
            }
        });
    }
    async onRegionChange(regionSelect, item) {
        const regionId = regionSelect.value;
        const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);
        if (!networkSelect)
            return;
        networkSelect.innerHTML = '<option value="">选择网段</option>';
        if (regionId) {
            await loadNetworks(regionId, networkSelect, this.getSelectedNetworkIds());
        }
    }
    async updateNetworkSelects() {
        const selectedIds = this.getSelectedNetworkIds();
        const items = this.container.querySelectorAll(".network-config-item");
        for (const item of items) {
            const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
            const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);
            if (!regionSelect || !networkSelect)
                continue;
            const currentValue = networkSelect.value;
            const regionId = regionSelect.value;
            if (regionId) {
                const otherIds = selectedIds.filter(id => id !== currentValue);
                await loadNetworks(regionId, networkSelect, otherIds);
                if (currentValue)
                    networkSelect.value = currentValue;
            }
        }
    }
    getSelectedNetworkIds() {
        if (!this.container)
            return [];
        const selects = this.container.querySelectorAll(`.${this.options.networkSelectClass}`);
        return Array.from(selects).map(s => s.value).filter(Boolean);
    }
    async loadExistingNetworks(networks, allNetworks) {
        if (!this.container) {
            this.container = document.getElementById(this.options.containerId);
        }
        if (!this.container)
            return;
        if (!networks?.length) {
            await this.init();
            return;
        }
        this.container.innerHTML = "";
        const networkMap = new Map(allNetworks.map(n => [n.id, n]));
        const selectedIds = networks.map(n => n.id);
        await loadNetworkRegions(document.createElement("select"));
        for (const network of networks) {
            const networkInfo = networkMap.get(network.id);
            if (!networkInfo)
                continue;
            const div = document.createElement("div");
            div.innerHTML = this.createItemHTML();
            const item = div.firstElementChild;
            if (!item)
                continue;
            const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
            const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);
            if (regionSelect && networkSelect) {
                await loadNetworkRegions(regionSelect);
                regionSelect.value = String(networkInfo.network_region_id || "");
                const otherIds = selectedIds.filter(id => id !== network.id).map(String);
                await loadNetworks(networkInfo.network_region_id || "", networkSelect, otherIds);
                networkSelect.value = String(network.id);
            }
            this.container.appendChild(item);
            this.bindItemEvents(item);
        }
        this.updateAddButtons();
    }
    collectData() {
        if (!this.container)
            return { networkIds: [], hasEmpty: false };
        const selects = this.container.querySelectorAll(`.${this.options.networkSelectClass}`);
        const networkIds = [];
        let hasEmpty = false;
        for (const select of selects) {
            if (!select.value) {
                hasEmpty = true;
            }
            else {
                networkIds.push(select.value);
            }
        }
        return { networkIds, hasEmpty };
    }
    destroy() {
        if (this.container) {
            const items = this.container.querySelectorAll(".network-config-item");
            items.forEach(item => this.cleanupItemHandlers(item));
        }
        this.boundHandlers.clear();
        this.container = null;
    }
}
export const roomNetworkConfigManager = new NetworkConfigManager({
    containerId: "network-configs-container",
    regionSelectClass: "network-region-select",
    networkSelectClass: "network-select",
    removeBtnClass: "remove-network-config-btn",
});
const tableState = createSortState("name", "asc");
const roomTemplates = {
    tableRow: `
    <tr data-id="{{id}}">
      <td class="index-column">{{index}}</td>
      <td>{{name}}</td>
      <td>{{roomType}}</td>
      <td>{{{networks}}}</td>
      <td>{{description}}</td>
      <td>{{createdAt}}</td>
      <td>{{{actions}}}</td>
    </tr>
  `,
};
function renderRoomRow(room, index) {
    const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : "";
    const roomType = roomTypeLower === "office" ? t("room.type_office") : roomTypeLower === "data_center" ? t("room.type_datacenter") : room.room_type || "-";
    const networks = room.networks && room.networks.length > 0
        ? room.networks.map(n => `${n.name} (${n.network_region})`).join("<br>")
        : "-";
    return templateManager.render(roomTemplates.tableRow, {
        id: room.id,
        index,
        name: escapeHtml(room.name),
        roomType,
        networks,
        description: escapeHtml(room.description) || "-",
        createdAt: new Date(room.created_at).toLocaleString(),
        actions: renderActionButtons(room.id),
    });
}
export async function loadRoomsData(page = 1, sortBy = null, sortOrder = null) {
    if (sortBy)
        tableState.setSort(sortBy, sortOrder);
    await wrapAsync(async () => {
        const result = await roomManager.list({
            page,
            pageSize: DEFAULT_PAGE_SIZE,
            sort_by: tableState.sortBy,
            sort_order: tableState.sortOrder,
        });
        if (!result)
            return;
        const data = result.data;
        const rooms = data.items || [];
        const tbody = document.querySelector("#rooms-table tbody");
        if (!tbody) {
            errorHandler.handle(errorHandler.createError("DOM_ERROR", "未找到房间表格元素", "error"));
            return;
        }
        if (rooms.length === 0) {
            tbody.innerHTML = renderEmptyRow(7, t("common.no_data"));
            return;
        }
        const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;
        tbody.innerHTML = rooms
            .map((room, index) => renderRoomRow(room, startIndex + index + 1))
            .join("");
        if (data.total !== undefined) {
            appendPaginationToTable("#rooms-table", data, loadRoomsData);
        }
        updateSortIcons("rooms-table", tableState);
    }, "加载房间数据失败")();
}
export function initRoomSortEvents() {
    initSortEvents("rooms-table", tableState, loadRoomsData);
}
export function initRoomTableEvents() {
    eventDelegator.on(document, "click", "#rooms-table .btn-edit", (_event, _target, data) => {
        if (data?.id)
            editRoom(data.id);
    });
    eventDelegator.on(document, "click", "#rooms-table .btn-delete", (_event, _target, data) => {
        if (data?.id)
            deleteRoom(data.id);
    });
}
export async function editRoom(id) {
    await wrapAsync(async () => {
        const room = await roomManager.get(id);
        if (room) {
            openRoomModal(room);
        }
    }, "获取房间数据失败")();
}
export async function deleteRoom(id) {
    const result = await roomManager.delete(id, { confirmMessage: t("room.delete_confirm") });
    if (result.success) {
        await loadRoomsData();
    }
}
export async function submitRoomForm() {
    const form = document.getElementById("room-form");
    if (!form)
        return;
    const formData = new FormData(form);
    const id = formData.get("room-id");
    const name = formData.get("room-name");
    const roomType = formData.get("room-type");
    const description = formData.get("room-description");
    const { networkIds, hasEmpty } = roomNetworkConfigManager.collectData();
    if (hasEmpty) {
        errorHandler.handle(errorHandler.createError("VALIDATION_ERROR", t("room.network_all_required"), "warning"));
        return;
    }
    const roomData = {
        name: name.trim(),
        room_type: roomType.toUpperCase(),
        network_ids: networkIds,
        description: description || null,
    };
    await wrapAsync(async () => {
        let result;
        if (id) {
            result = await roomManager.update(id, roomData);
        }
        else {
            result = await roomManager.create(roomData);
        }
        if (result.success) {
            closeModal("room-modal");
            await loadRoomsData();
            return true;
        }
    }, "保存房间数据失败")();
}
export async function openRoomModal(room = null) {
    openModal("room-modal");
    const title = elementCache.get("room-modal-title");
    const form = elementCache.get("room-form");
    if (room) {
        if (title)
            title.textContent = "编辑房间";
        elementCache.setValue("room-id", String(room.id));
        elementCache.setValue("room-name", room.name);
        elementCache.setValue("room-type", room.room_type ? room.room_type.toLowerCase() : "office");
        elementCache.setValue("room-description", room.description || "");
        await loadRoomNetworks(room);
    }
    else {
        if (title)
            title.textContent = "添加房间";
        form?.reset();
        elementCache.setValue("room-id", "");
        await roomNetworkConfigManager.init();
    }
}
async function loadRoomNetworks(room) {
    await wrapAsync(async () => {
        const networksResult = await apiGet("/api/resources/networks?page_size=1000");
        if (networksResult.success) {
            const allNetworks = networksResult.data.items || networksResult.data || [];
            await roomNetworkConfigManager.loadExistingNetworks(room.networks || [], allNetworks);
        }
        else {
            await roomNetworkConfigManager.init();
        }
    }, "加载网络信息失败")();
}
export function cleanup() {
    eventDelegator.off(document, "click", "#rooms-table .btn-edit");
    eventDelegator.off(document, "click", "#rooms-table .btn-delete");
    roomNetworkConfigManager.destroy();
}
//# sourceMappingURL=room.js.map