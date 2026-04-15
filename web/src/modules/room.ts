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
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";
import type { ApiResponse, PagedData } from "../types/api.js";
import type { NetworkRegion, Network } from "../types/resources.js";

interface SelectItem {
  id: number;
  name: string;
}

const networkCache: {
  networkRegions: SelectItem[] | null;
  networks: Map<string, { data: SelectItem[]; timestamp: number }>;
  cacheTime: number;
  CACHE_TTL: number;
} = {
  networkRegions: null,
  networks: new Map(),
  cacheTime: 0,
  CACHE_TTL: 60 * 1000,
};

function isCacheValid(timestamp: number): boolean {
  return timestamp && (Date.now() - timestamp < networkCache.CACHE_TTL);
}

function extractItems<T>(result: ApiResponse<T[] | PagedData<T>>): T[] {
  if (!result.success || !result.data) return [];
  if (Array.isArray(result.data)) return result.data;
  if ("items" in result.data && Array.isArray(result.data.items)) return result.data.items;
  return [];
}

function updateSelect(select: HTMLSelectElement | null, data: SelectItem[], placeholder = "选择选项"): HTMLSelectElement | null {
  if (!select) return null;
  const currentValue = select.value;
  select.innerHTML = `<option value="">${placeholder}</option>` +
    data.map(item => `<option value="${item.id}">${item.name}</option>`).join("");

  if (currentValue) select.value = currentValue;
  return select;
}

async function loadNetworkRegions(select: HTMLSelectElement | null): Promise<SelectItem[]> {
  if (!select) return [];

  const now = Date.now();
  if (networkCache.networkRegions && isCacheValid(networkCache.cacheTime)) {
    updateSelect(select, networkCache.networkRegions, "选择网络区域");
    return networkCache.networkRegions;
  }

  try {
    const result = await apiGet<NetworkRegion[] | PagedData<NetworkRegion>>("/api/resources/network-regions?page_size=1000");
    const items = extractItems(result).map(item => ({ id: item.id, name: item.name }));
    if (items.length > 0) {
      networkCache.networkRegions = items;
      networkCache.cacheTime = now;
      updateSelect(select, items, "选择网络区域");
      return items;
    }
  } catch (error) {
    console.error("加载网络区域失败:", error);
  }

  return [];
}

async function loadNetworks(regionId: string | number | null, select: HTMLSelectElement | null, excludeIds: (string | number)[] = []): Promise<SelectItem[]> {
  if (!select) return [];

  const cacheKey = String(regionId || "all");
  const cached = networkCache.networks.get(cacheKey);

  if (cached && isCacheValid(cached.timestamp)) {
    const filtered = cached.data.filter(n => !excludeIds.includes(n.id));
    updateSelect(select, filtered, "选择网段");
    return filtered;
  }

  try {
    const url = regionId ? `/api/resources/networks?region_id=${regionId}&page_size=1000` : "/api/resources/networks?page_size=1000";
    const result = await apiGet<Network[] | PagedData<Network>>(url);

    const items = extractItems(result).map(item => ({ id: item.id, name: item.name }));
    if (items.length > 0) {
      networkCache.networks.set(cacheKey, { data: items, timestamp: Date.now() });
      const filtered = items.filter(n => !excludeIds.includes(n.id));
      updateSelect(select, filtered, "选择网段");
      return filtered;
    }
  } catch (error) {
    console.error("加载网段失败:", error);
  }

  return [];
}

class EventHandler {
  private manager: NetworkConfigManager;
  private handlers: WeakMap<Element, EventListener>;

  constructor(manager: NetworkConfigManager) {
    this.manager = manager;
    this.handlers = new WeakMap();
  }

  bind(element: Element, event: string, handler: EventListener): EventListener {
    const oldHandler = this.handlers.get(element);
    if (oldHandler) element.removeEventListener(event, oldHandler);

    element.addEventListener(event, handler);
    this.handlers.set(element, handler);
    return handler;
  }

  clear(): void {
    for (const [element, handler] of this.handlers.entries()) {
      if (element && handler) {
        element.removeEventListener("click", handler);
        element.removeEventListener("change", handler);
      }
    }
    this.handlers = new WeakMap();
  }
}

interface NetworkConfigOptions {
  containerId: string;
  regionSelectClass: string;
  networkSelectClass: string;
  addBtnId: string;
  removeBtnClass: string;
}

class NetworkConfigManager {
  options: NetworkConfigOptions;
  container: HTMLElement | null;
  eventHandler: EventHandler;

  constructor(options: NetworkConfigOptions) {
    this.options = options;
    this.container = null;
    this.eventHandler = new EventHandler(this);
  }

  async init(): Promise<boolean> {
    this.container = document.getElementById(this.options.containerId);
    if (!this.container) {
      console.error(`未找到${this.options.containerId}元素`);
      return false;
    }

    this.container.innerHTML = "";
    await this.addItem();
    this.updateAddButtons();
    return true;
  }

  createItemHTML(): string {
    const { regionSelectClass, networkSelectClass, removeBtnClass } = this.options;

    return `
      <div class="network-config-item">
        <div class="form-row">
          <div class="form-group">
            <label>网络区域<span class="required">*</span></label>
            <select class="${regionSelectClass}" required>
              <option value="">选择网络区域</option>
            </select>
          </div>
          <div class="form-group">
            <label>网段选择<span class="required">*</span></label>
            <select class="${networkSelectClass}" required>
              <option value="">选择网段</option>
            </select>
          </div>
          <div class="form-group">
            <button type="button" class="btn btn-danger btn-sm ${removeBtnClass}">
              删除
            </button>
            <button type="button" class="btn btn-secondary btn-sm add-btn" style="display: none;">
              新增
            </button>
          </div>
        </div>
      </div>
    `;
  }

  async addItem(): Promise<Element | undefined> {
    if (!this.container) return;

    const div = document.createElement("div");
    div.innerHTML = this.createItemHTML();
    const item = div.firstElementChild;
    if (!item) return;

    this.container.appendChild(item);

    const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`) as HTMLSelectElement | null;
    await loadNetworkRegions(regionSelect);

    this.bindItemEvents(item);
    this.updateAddButtons();
    return item;
  }

  updateAddButtons(): void {
    if (!this.container) return;

    const items = this.container.querySelectorAll(".network-config-item");
    items.forEach((item, index) => {
      const addBtn = item.querySelector(".add-btn") as HTMLElement | null;
      if (addBtn) {
        addBtn.style.display = index === items.length - 1 ? "" : "none";
      }
    });
  }

  bindItemEvents(item: Element): void {
    const { regionSelectClass, networkSelectClass, removeBtnClass } = this.options;

    const removeBtn = item.querySelector(`.${removeBtnClass}`);
    if (removeBtn) {
      this.eventHandler.bind(removeBtn, "click", () => this.removeItem(item));
    }

    const addBtn = item.querySelector(".add-btn");
    if (addBtn) {
      this.eventHandler.bind(addBtn, "click", () => this.addItem());
    }

    const regionSelect = item.querySelector(`.${regionSelectClass}`);
    if (regionSelect) {
      this.eventHandler.bind(regionSelect, "change", async () => {
        await this.onRegionChange(regionSelect as HTMLSelectElement, item);
        this.updateNetworkSelects();
      });
    }

    const networkSelect = item.querySelector(`.${networkSelectClass}`);
    if (networkSelect) {
      this.eventHandler.bind(networkSelect, "change", () => this.updateNetworkSelects());
    }
  }

  removeItem(item: Element): void {
    const items = this.container!.querySelectorAll(".network-config-item");
    if (items.length <= 1) {
      showToast("至少需要保留一个网段配置", "warning");
      return;
    }

    item.remove();
    this.updateAddButtons();
    this.updateNetworkSelects();
  }

  async onRegionChange(regionSelect: HTMLSelectElement, item: Element): Promise<void> {
    const regionId = regionSelect.value;
    const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`) as HTMLSelectElement | null;

    if (!networkSelect) return;

    networkSelect.innerHTML = '<option value="">选择网段</option>';
    if (regionId) {
      await loadNetworks(regionId, networkSelect, this.getSelectedNetworkIds());
    }
  }

  async updateNetworkSelects(): Promise<void> {
    const selectedIds = this.getSelectedNetworkIds();
    const items = this.container!.querySelectorAll(".network-config-item");

    for (const item of items) {
      const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`) as HTMLSelectElement | null;
      const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`) as HTMLSelectElement | null;
      if (!regionSelect || !networkSelect) continue;
      const currentValue = networkSelect.value;
      const regionId = regionSelect.value;

      if (regionId) {
        const otherIds = selectedIds.filter(id => id !== currentValue);
        await loadNetworks(regionId, networkSelect, otherIds);
        if (currentValue) networkSelect.value = currentValue;
      }
    }
  }

  getSelectedNetworkIds(): string[] {
    if (!this.container) return [];
    const selects = this.container.querySelectorAll(`.${this.options.networkSelectClass}`) as NodeListOf<HTMLSelectElement>;
    return Array.from(selects).map(s => s.value).filter(Boolean);
  }

  async loadExistingNetworks(networks: { id: number }[], allNetworks: Network[]): Promise<void> {
    if (!this.container) {
      this.container = document.getElementById(this.options.containerId);
    }
    if (!this.container) return;

    if (!networks?.length) {
      await this.init();
      return;
    }

    this.container.innerHTML = "";

    const networkMap = new Map(allNetworks.map(n => [n.id, n]));
    const selectedIds = networks.map(n => n.id);

    await loadNetworkRegions(document.createElement("select"));

    networks.forEach((network) => {
      const networkInfo = networkMap.get(network.id);
      if (!networkInfo) return;

      const div = document.createElement("div");
      div.innerHTML = this.createItemHTML();
      const item = div.firstElementChild;
      if (!item) return;

      const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`) as HTMLSelectElement | null;
      const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`) as HTMLSelectElement | null;

      if (regionSelect && networkSelect) {
        loadNetworkRegions(regionSelect).then(() => {
          regionSelect.value = String(networkInfo.network_region_id || "");

          const otherIds = selectedIds.filter(id => id !== network.id).map(String);
          loadNetworks(networkInfo.network_region_id || "", networkSelect, otherIds).then(() => {
            networkSelect.value = String(network.id);
          });
        });
      }

      this.container!.appendChild(item);
      this.bindItemEvents(item);
    });

    this.updateAddButtons();
  }

  collectData(): { networkIds: string[]; hasEmpty: boolean } {
    if (!this.container) return { networkIds: [], hasEmpty: false };

    const selects = this.container.querySelectorAll(`.${this.options.networkSelectClass}`) as NodeListOf<HTMLSelectElement>;
    const networkIds: string[] = [];
    let hasEmpty = false;

    for (const select of selects) {
      if (!select.value) {
        hasEmpty = true;
      } else {
        networkIds.push(select.value);
      }
    }

    return { networkIds, hasEmpty };
  }

  destroy(): void {
    this.eventHandler.clear();
    this.container = null;
  }
}

export const roomNetworkConfigManager = new NetworkConfigManager({
  containerId: "network-configs-container",
  regionSelectClass: "network-region-select",
  networkSelectClass: "network-select",
  addBtnId: "add-network-config-btn",
  removeBtnClass: "remove-network-config-btn",
});

const tableState = createSortState("name", "asc");
let currentPage = 1;

export async function loadRoomsData(page = 1, sortBy: string | null = null, sortOrder: string | null = null): Promise<void> {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder as "asc" | "desc" | null);

  try {
    const roomsData = await apiGet(`/api/resources/rooms?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = roomsData.success ? roomsData.data : { items: [], total: 0 };
    const rooms = (data as { items?: unknown[] }).items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#rooms-table", {
      data: rooms as Record<string, unknown>[],
      columns: [
        { field: "id", render: (_v: unknown, _row: unknown, index: number) => startIndex + index + 1, className: "index-column" },
        { field: "name", render: (v: unknown) => escapeHtml(v as string) },
        { field: "room_type", render: (v: unknown) => {
          const roomTypeLower = v ? (v as string).toLowerCase() : "";
          return roomTypeLower === "office" ? t("room.type_office") : roomTypeLower === "data_center" ? t("room.type_datacenter") : v as string || "-";
        }},
        { field: "networks", render: (v: unknown) => v && (v as unknown[]).length > 0 ? (v as { name: string; network_region: string }[]).map(n => `${n.name} (${n.network_region})`).join("<br>") : "-" },
        { field: "description", render: (v: unknown) => escapeHtml(v as string) || "-" },
        { field: "created_at", render: (v: unknown) => new Date(v as string).toLocaleString() },
        { field: "id", render: (v: unknown) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t("common.edit")}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t("common.delete")}</button>
        ` },
      ],
      emptyMessage: t("common.no_data"),
    });

    if ((data as { total?: number }).total !== undefined) {
      appendPaginationToTable("#rooms-table", data as { total?: number; page?: number; page_size?: number }, loadRoomsData);
    }
    updateSortIcons("rooms-table", tableState);
  } catch (error) {
    handleError(error, "加载房间数据失败", () => {
      renderTable("#rooms-table", { data: [], columns: [], emptyMessage: "加载失败，请刷新页面重试" });
    });
  }
}

export function initRoomSortEvents(): void {
  initSortEvents("rooms-table", tableState, loadRoomsData);
}

export async function editRoom(id: string | number): Promise<void> {
  try {
    const result = await apiGet(`/api/resources/rooms/${id}`);
    if (result.success) {
      openRoomModal(result.data as Record<string, unknown> | null);
    } else {
      showToast(`获取房间数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取房间数据失败");
  }
}

export async function deleteRoom(id: string | number): Promise<void> {
  await handleDelete(id, "/api/resources/rooms", "房间删除成功", loadRoomsData);
}

export async function submitRoomForm(): Promise<boolean | void> {
  const id = getElementValue("room-id") as string;
  const name = getElementValue("room-name") as string;
  const roomType = getElementValue("room-type") as string;
  const description = getElementValue("room-description") as string;

  if (!name) {
    showToast(t("room.name_required"), "warning");
    return;
  }

  if (!roomType) {
    showToast(t("room.type_required"), "warning");
    return;
  }

  const { networkIds, hasEmpty } = roomNetworkConfigManager.collectData();

  if (hasEmpty) {
    showToast(t("room.network_all_required"), "warning");
    return;
  }

  if (networkIds.length === 0) {
    showToast(t("room.network_required"), "warning");
    return;
  }

  let formattedRoomType: string;
  if (roomType === "office") {
    formattedRoomType = "OFFICE";
  } else if (roomType === "data_center") {
    formattedRoomType = "DATA_CENTER";
  } else {
    formattedRoomType = roomType;
  }

  const roomData = {
    name: name.trim(),
    room_type: formattedRoomType,
    network_ids: networkIds,
    description: description || null,
  };

  const success = await handleFormSubmit({
    formData: roomData,
    id,
    baseUrl: "/api/resources/rooms",
    successMessage: "房间保存成功",
    modalId: "room-modal",
    reloadFunction: loadRoomsData,
  });

  return success;
}

export async function openRoomModal(room: Record<string, unknown> | null = null): Promise<void> {
  openModal("room-modal");

  const title = elementCache.get("room-modal-title");
  const form = elementCache.get("room-form") as HTMLFormElement | null;

  if (room) {
    if (title) title.textContent = "编辑房间";
    elementCache.setValue("room-id", String(room.id));
    elementCache.setValue("room-name", room.name as string);
    elementCache.setValue("room-type", room.room_type ? (room.room_type as string).toLowerCase() : "office");
    elementCache.setValue("room-description", (room.description as string) || "");

    await loadRoomNetworks(room);
  } else {
    if (title) title.textContent = "添加房间";
    form?.reset();
    elementCache.setValue("room-id", "");
    await roomNetworkConfigManager.init();
  }
}

async function loadRoomNetworks(room: Record<string, unknown>): Promise<void> {
  try {
    const networksResult = await apiGet("/api/resources/networks?page_size=1000");
    if (networksResult.success) {
      const allNetworks = (networksResult.data as { items?: Network[] }).items || networksResult.data as Network[] || [];
      await roomNetworkConfigManager.loadExistingNetworks(
        (room.networks as { id: number }[]) || [],
        allNetworks,
      );
    } else {
      await roomNetworkConfigManager.init();
    }
  } catch (error) {
    console.error("加载网络信息失败:", error);
    await roomNetworkConfigManager.init();
  }
}
