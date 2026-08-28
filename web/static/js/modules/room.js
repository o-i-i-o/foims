// 导入必要的模块
import { apiGet, apiPost, apiPut } from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  getElementValue,
  handleDelete,
  handleError,
  appendPaginationToTable,
  escapeHtml,
  DEFAULT_PAGE_SIZE,
  createSortState,
  updateSortIcons,
  initSortEvents,
  openSimpleListModal
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modalLoader.js";
import { t } from "../utils/i18n.js";
import { iconButton } from "../utils/icons.js";
import { elementCache } from "../utils/helpers.js";
import { loadOrgsForSelect } from "../utils/resources.js";
import { DynamicRowManager } from "../utils/dynamicRowManager.js";

// ==========================================
// 网段配置管理模块 - 仅用于房间管理
// ==========================================

// 网络请求缓存
const networkCache = {
  networkRegions: null,
  networks: new Map(),
  cacheTime: 0,
  CACHE_TTL: 60 * 1000
};

// 检查缓存是否有效
function isCacheValid(timestamp) {
  return timestamp && Date.now() - timestamp < networkCache.CACHE_TTL;
}

function extractItems(result) {
  if (!result.success || !result.data) {
    return [];
  }
  if (Array.isArray(result.data)) {
    return result.data;
  }
  if (result.data.items && Array.isArray(result.data.items)) {
    return result.data.items;
  }
  return [];
}

// 更新选择框选项
function updateSelect(select, data, placeholder = t("room.select_option")) {
  const currentValue = select.value;
  select.innerHTML = `<option value="">${placeholder}</option>${data
    .map((item) => `<option value="${item.id}">${escapeHtml(item.name)}</option>`)
    .join("")}`;

  if (currentValue) {
    select.value = currentValue;
  }
  return select;
}

// 加载网络区域
async function loadNetworkRegions(select) {
  if (!select) {
    return [];
  }

  const now = Date.now();
  if (networkCache.networkRegions && isCacheValid(networkCache.cacheTime)) {
    updateSelect(select, networkCache.networkRegions, t("network.select_region"));
    return networkCache.networkRegions;
  }

  try {
    const result = await apiGet("/api/resources/options/network-regions");
    const items = extractItems(result);
    if (items.length > 0) {
      networkCache.networkRegions = items;
      networkCache.cacheTime = now;
      updateSelect(select, items, t("network.select_region"));
      return items;
    }
  } catch (error) {
    console.error("加载网络区域失败:", error);
  }

  return [];
}

// 加载网段
async function loadNetworks(regionId, select, excludeIds = []) {
  if (!select) {
    return [];
  }

  const cacheKey = regionId || "all";
  const cached = networkCache.networks.get(cacheKey);

  if (cached && isCacheValid(cached.timestamp)) {
    const filtered = cached.data.filter((n) => !excludeIds.includes(n.id));
    updateSelect(select, filtered, t("network.select_segment"));
    return filtered;
  }

  try {
    const url = regionId
      ? `/api/resources/options/networks?region_id=${regionId}`
      : "/api/resources/options/networks";
    const result = await apiGet(url);

    const items = extractItems(result);
    if (items.length > 0) {
      networkCache.networks.set(cacheKey, { data: items, timestamp: Date.now() });
      const filtered = items.filter((n) => !excludeIds.includes(n.id));
      updateSelect(select, filtered, t("network.select_segment"));
      return filtered;
    }
  } catch (error) {
    console.error("加载网段失败:", error);
  }

  return [];
}

// 事件处理器工厂
class EventHandler {
  constructor(manager) {
    this.manager = manager;
    this.handlers = new WeakMap();
  }

  bind(element, event, handler) {
    const oldHandler = this.handlers.get(element);
    if (oldHandler) {
      element.removeEventListener(event, oldHandler);
    }

    element.addEventListener(event, handler);
    this.handlers.set(element, handler);
    return handler;
  }

  clear() {
    for (const [element, handler] of this.handlers.entries()) {
      if (element && handler) {
        element.removeEventListener("click", handler);
        element.removeEventListener("change", handler);
      }
    }
    this.handlers = new WeakMap();
  }
}

// 网段配置管理器类
class NetworkConfigManager {
  constructor(options) {
    this.options = options;
    this.container = null;
    this.addHandler = null;
    this.eventHandler = new EventHandler(this);
  }

  ensureContainer() {
    if (!this.container || !document.contains(this.container)) {
      this.container = document.getElementById(this.options.containerId);
    }
    return this.container;
  }

  async init() {
    this.ensureContainer();
    if (!this.container) {
      console.error(`未找到${this.options.containerId}元素`);
      return false;
    }
    this.bindExternalAddButton();

    this.container.innerHTML = "";
    await this.addItem();
    return true;
  }

  /** 绑定标题右侧的「添加」按钮，并同步提示文案（图标按钮不覆写内容） */
  bindExternalAddButton() {
    const addBtn = document.getElementById(this.options.externalAddButtonId);
    if (!addBtn) {
      return;
    }
    if (this.addHandler) {
      addBtn.removeEventListener("click", this.addHandler);
    }
    this.addHandler = () => this.addItem();
    addBtn.addEventListener("click", this.addHandler);
    addBtn.dataset.tooltip = t("network.add_network_config");
    addBtn.setAttribute("aria-label", t("network.add_network_config"));
  }

  createItemHTML() {
    const { regionSelectClass, networkSelectClass, removeBtnClass } = this.options;

    return `
      <div class="network-config-item">
        <div class="form-row">
          <div class="form-group">
            <select class="${regionSelectClass}" required>
              <option value="">${t("network.select_region")}</option>
            </select>
          </div>
          <div class="form-group">
            <select class="${networkSelectClass}" required>
              <option value="">${t("network.select_segment")}</option>
            </select>
          </div>
          <div class="form-group network-config-actions">
            ${iconButton({ icon: "trash", label: t("common.delete"), cls: `btn-danger ${removeBtnClass}` })}
          </div>
        </div>
      </div>
    `;
  }

  async addItem() {
    if (!this.ensureContainer()) {
      return;
    }

    const div = document.createElement("div");
    div.innerHTML = this.createItemHTML();
    const item = div.firstElementChild;

    this.container.appendChild(item);

    const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
    await loadNetworkRegions(regionSelect);

    this.bindItemEvents(item);
    return item;
  }

  bindItemEvents(item) {
    const { regionSelectClass, networkSelectClass, removeBtnClass } = this.options;

    const removeBtn = item.querySelector(`.${removeBtnClass}`);
    if (removeBtn) {
      this.eventHandler.bind(removeBtn, "click", () => this.removeItem(item));
    }

    const regionSelect = item.querySelector(`.${regionSelectClass}`);
    if (regionSelect) {
      this.eventHandler.bind(regionSelect, "change", async () => {
        await this.onRegionChange(regionSelect, item);
        this.updateNetworkSelects();
      });
    }

    const networkSelect = item.querySelector(`.${networkSelectClass}`);
    if (networkSelect) {
      this.eventHandler.bind(networkSelect, "change", () => this.updateNetworkSelects());
    }
  }

  removeItem(item) {
    if (!this.ensureContainer()) {
      return;
    }
    const items = this.container.querySelectorAll(".network-config-item");
    if (items.length <= 1) {
      showToast(t("room.keep_one_network"), "warning");
      return;
    }

    item.remove();
    this.updateNetworkSelects();
  }

  async onRegionChange(regionSelect, item) {
    const regionId = regionSelect.value;
    const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);

    if (!networkSelect) {
      return;
    }

    networkSelect.innerHTML = `<option value="">${t("network.select_segment")}</option>`;
    if (regionId) {
      await loadNetworks(regionId, networkSelect, this.getSelectedNetworkIds());
    }
  }

  async updateNetworkSelects() {
    if (!this.ensureContainer()) {
      return;
    }
    const selectedIds = this.getSelectedNetworkIds();
    const items = this.container.querySelectorAll(".network-config-item");

    // 各行网段加载互不依赖（loadNetworks 有区域级缓存），并行执行
    await Promise.all(
      Array.from(items).map((item) => {
        const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
        const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);
        const currentValue = networkSelect.value;
        const regionId = regionSelect.value;

        if (!regionId) {
          return Promise.resolve();
        }

        const otherIds = selectedIds.filter((id) => id !== currentValue);
        return loadNetworks(regionId, networkSelect, otherIds).then(() => {
          if (currentValue) {
            networkSelect.value = currentValue;
          }
        });
      })
    );
  }

  getSelectedNetworkIds() {
    if (!this.ensureContainer()) {
      return [];
    }
    const selects = this.container.querySelectorAll(`.${this.options.networkSelectClass}`);
    return Array.from(selects)
      .map((s) => s.value)
      .filter(Boolean);
  }

  async loadExistingNetworks(networks, allNetworks) {
    // closeModal 会移除模态框 DOM，需重新获取 container
    this.ensureContainer();
    if (!this.container) {
      return;
    }

    if (!networks?.length) {
      await this.init();
      return;
    }

    this.container.innerHTML = "";

    const networkMap = new Map(allNetworks.map((n) => [n.id, n]));
    const selectedIds = networks.map((n) => n.id);

    // 预热区域/网段缓存，避免逐条串行请求
    await loadNetworkRegions(document.createElement("select"));

    // 各网段条目的区域/网段选项加载相互独立，并行构建（缓存命中后无额外请求）
    const items = await Promise.all(
      networks.map(async (network) => {
        const networkInfo = networkMap.get(network.id);
        if (!networkInfo) {
          return null;
        }

        const div = document.createElement("div");
        div.innerHTML = this.createItemHTML();
        const item = div.firstElementChild;

        const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
        const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);

        // 区域 id 以房间自带数据为准（brief 接口的 networks 含 network_region_id）；
        // allNetworks 来自精简 options 接口（仅 id/name），不含该字段
        const regionId = network.network_region_id || networkInfo.network_region_id;

        await loadNetworkRegions(regionSelect);
        regionSelect.value = regionId || "";

        const otherIds = selectedIds.filter((id) => id !== network.id);
        await loadNetworks(regionId, networkSelect, otherIds);
        networkSelect.value = network.id;

        return item;
      })
    );

    for (const item of items) {
      if (!item) {
        continue;
      }
      this.container.appendChild(item);
      this.bindItemEvents(item);
    }

    this.bindExternalAddButton();
  }

  collectData() {
    if (!this.ensureContainer()) {
      return { networkIds: [], hasEmpty: false };
    }

    const selects = this.container.querySelectorAll(`.${this.options.networkSelectClass}`);
    const networkIds = [];
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

  destroy() {
    this.eventHandler.clear();
    this.container = null;
  }
}

// 全局房间网段配置管理器实例
export const roomNetworkConfigManager = new NetworkConfigManager({
  containerId: "network-configs-container",
  regionSelectClass: "network-region-select",
  networkSelectClass: "network-select",
  removeBtnClass: "remove-network-config-btn",
  externalAddButtonId: "room-add-network-config-btn"
});

// ==========================================
// 房间子项（工位/机柜）动态管理模块
// ==========================================

/** 当前房间所属组织下的员工（供工位管理人下拉选择；无组织时为空） */
let roomOrgEmployees = [];

/** 按组织加载员工列表（组织为空时清空选项） */
async function loadRoomOrgEmployees(orgId) {
  if (!orgId) {
    roomOrgEmployees = [];
    return;
  }
  try {
    const result = await apiGet(`/api/resources/employees?org_id=${encodeURIComponent(orgId)}`);
    roomOrgEmployees = result.success && Array.isArray(result.data) ? result.data : [];
  } catch (error) {
    console.error("加载组织员工失败:", error);
    roomOrgEmployees = [];
  }
}

/** 渲染工位管理人下拉控件：组织人员是该字段唯一数据来源 */
function renderManagerControl(data = {}) {
  const managerId = data.manager_employee_id || "";
  const options = [
    `<option value="">${t("workstation.manager_unassigned")}</option>`,
    ...roomOrgEmployees.map(
      (emp) =>
        `<option value="${emp.id}" ${managerId === emp.id ? "selected" : ""}>${escapeHtml(emp.name)}</option>`
    )
  ];
  return `<select class="child-manager form-control" autocomplete="off">${options.join("")}</select>`;
}

/** 收集单个管理人控件的值：选中员工时仅提交员工 id */
function collectManagerValue(item) {
  const control = item.querySelector(".child-manager");
  if (!control || control.tagName !== "SELECT") {
    return { manager: null, manager_employee_id: null };
  }
  const value = control.value;
  return value ? { manager: null, manager_employee_id: value } : { manager: null, manager_employee_id: null };
}

/** 组织切换后按新员工列表重建管理人下拉，仍有效的员工选择保持不变 */
function refreshManagerSelects() {
  document.querySelectorAll("#room-children-container .child-manager").forEach((control) => {
    const formGroup = control.parentElement;
    if (!formGroup) {
      return;
    }
    const selectedId = control.tagName === "SELECT" ? control.value : "";
    const wrapper = document.createElement("div");
    wrapper.innerHTML = renderManagerControl({ manager_employee_id: selectedId });
    const next = wrapper.firstElementChild;
    if (next) {
      formGroup.replaceChildren(next);
    }
  });
}

/** 单类子项（工位或机柜）的动态行管理器。 */
class RoomChildListManager extends DynamicRowManager {
  constructor(config, kind) {
    super(config);
    // kind: "workstation" | "cabinet"
    this.kind = kind;
  }

  addLabel() {
    return this.kind === "workstation" ? t("room.add_workstation") : t("room.add_cabinet");
  }

  emptyHintText() {
    return t("common.no_data");
  }

  createWorkstationRow(data = {}) {
    const id = data.id || "";
    const name = data.name || "";
    const div = document.createElement("div");
    div.className = "room-child-item";
    div.dataset.childType = "workstation";
    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <input type="hidden" class="child-id" value="${escapeHtml(id)}" />
          <input type="text" class="child-name form-control" value="${escapeHtml(name)}" placeholder="${t("workstation.name")}" autocomplete="off" />
        </div>
        <div class="form-group">
          ${renderManagerControl(data)}
        </div>
        <div class="form-group room-children-actions">
          ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-danger remove-child-btn" })}
        </div>
      </div>
    `;
    this.bindItemEvents(div);
    return div;
  }

  createCabinetRow(data = {}) {
    const id = data.id || "";
    const name = data.name || "";
    const capacity = data.capacity || 42;
    const div = document.createElement("div");
    div.className = "room-child-item";
    div.dataset.childType = "cabinet";
    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <input type="hidden" class="child-id" value="${escapeHtml(id)}" />
          <input type="text" class="child-name form-control" value="${escapeHtml(name)}" placeholder="${t("cabinet.name")}" autocomplete="off" />
        </div>
        <div class="form-group">
          <input type="number" class="child-capacity form-control" value="${capacity}" min="1" max="48" placeholder="${t("cabinet.capacity")}" />
        </div>
        <div class="form-group room-children-actions">
          ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-danger remove-child-btn" })}
        </div>
      </div>
    `;
    this.bindItemEvents(div);
    return div;
  }

  // 按子项类型派发到对应的行构造器
  createRow(data = {}) {
    return this.kind === "workstation"
      ? this.createWorkstationRow(data)
      : this.createCabinetRow(data);
  }

  collectItems() {
    if (!this.ensureContainer()) {
      return [];
    }
    const items = this.container.querySelectorAll(".room-child-item");
    if (this.kind === "workstation") {
      return Array.from(items).map((item) => {
        const idInput = item.querySelector(".child-id");
        const nameInput = item.querySelector(".child-name");
        return {
          id: idInput?.value || null,
          name: (nameInput?.value || "").trim(),
          ...collectManagerValue(item)
        };
      });
    }
    return Array.from(items).map((item) => {
      const idInput = item.querySelector(".child-id");
      const nameInput = item.querySelector(".child-name");
      const capacityInput = item.querySelector(".child-capacity");
      return {
        id: idInput?.value || null,
        name: (nameInput?.value || "").trim(),
        capacity: parseInt(capacityInput?.value || "42", 10)
      };
    });
  }
}

/**
 * 房间子项协调器：按房间类型决定显示工位列表、机柜列表或两者。
 * 办公类（办公室/大厅/前台）仅工位；机房类（机房/弱电井）仅机柜；
 * "其他"无固定性质，工位与机柜列表同时展示。
 */
class RoomChildrenManager {
  constructor() {
    this.workstationList = new RoomChildListManager(
      {
        containerId: "room-children-container",
        itemSelector: ".room-child-item",
        emptyClassName: "room-child-empty",
        removeBtnSelector: ".remove-child-btn",
        externalAddButtonId: "add-room-workstation-btn",
        emptyMode: "hint"
      },
      "workstation"
    );
    this.cabinetList = new RoomChildListManager(
      {
        containerId: "room-cabinets-container",
        itemSelector: ".room-child-item",
        emptyClassName: "room-child-empty",
        removeBtnSelector: ".remove-child-btn",
        externalAddButtonId: "add-room-cabinet-btn",
        emptyMode: "hint"
      },
      "cabinet"
    );
    this.roomType = "office";
    this.typeChangeHandler = null;
  }

  get isOfficeRoom() {
    return ["office", "lobby", "reception"].includes(this.roomType);
  }

  get isCabinetRoom() {
    return ["data_center", "telecom_closet"].includes(this.roomType);
  }

  // "其他"房间工位与机柜均可管理
  get isMixedRoom() {
    return this.roomType === "other";
  }

  clearLists() {
    for (const list of [this.workstationList, this.cabinetList]) {
      if (list.ensureContainer()) {
        list.container.innerHTML = "";
        list.updateEmptyState();
      }
    }
  }

  bindTypeChange() {
    const typeSelect = document.getElementById("room-type");
    if (!typeSelect) {
      return;
    }
    if (this.typeChangeHandler) {
      typeSelect.removeEventListener("change", this.typeChangeHandler);
    }
    this.typeChangeHandler = (e) => this.onTypeChange(e.target.value);
    typeSelect.addEventListener("change", this.typeChangeHandler);
  }

  onTypeChange(newType) {
    this.roomType = newType;
    // 类型切换时清空列表并按新类型展示对应分区
    this.updateSections();
    this.clearLists();
  }

  updateSections() {
    const wsGroup = document.getElementById("room-workstations-group");
    const cabGroup = document.getElementById("room-cabinets-group");
    const showWorkstations = this.isOfficeRoom || this.isMixedRoom;
    const showCabinets = this.isCabinetRoom || this.isMixedRoom;
    wsGroup?.classList.toggle("hidden", !showWorkstations);
    cabGroup?.classList.toggle("hidden", !showCabinets);
  }

  init() {
    this.bindTypeChange();
    this.updateSections();
    this.clearLists();
    for (const list of [this.workstationList, this.cabinetList]) {
      list.bindExternalAddButton();
    }
  }

  loadExisting(children) {
    // closeModal 会移除模态框 DOM，需重新获取 container
    this.bindTypeChange();
    this.updateSections();
    this.clearLists();
    for (const list of [this.workstationList, this.cabinetList]) {
      list.bindExternalAddButton();
    }

    if ((this.isOfficeRoom || this.isMixedRoom) && children?.workstations?.length) {
      children.workstations.forEach((ws) => this.workstationList.addItem(ws));
    }
    if ((this.isCabinetRoom || this.isMixedRoom) && children?.cabinets?.length) {
      children.cabinets.forEach((cab) => this.cabinetList.addItem(cab));
    }

    for (const list of [this.workstationList, this.cabinetList]) {
      list.updateEmptyState();
    }
  }

  collectData() {
    if (this.isCabinetRoom) {
      return { cabinets: this.cabinetList.collectItems() };
    }
    if (this.isMixedRoom) {
      return {
        workstations: this.workstationList.collectItems(),
        cabinets: this.cabinetList.collectItems()
      };
    }
    return { workstations: this.workstationList.collectItems() };
  }
}

export const roomChildrenManager = new RoomChildrenManager();

// ==========================================
// 房间信息点管理模块 - 所有房型通用
// ==========================================

class RoomNetOutletsManager extends DynamicRowManager {
  constructor() {
    super({
      containerId: "room-net-outlets-container",
      itemSelector: ".room-net-outlet-item",
      emptyClassName: "room-net-outlet-empty",
      removeBtnSelector: ".remove-net-outlet-btn",
      externalAddButtonId: "add-room-net-outlet-btn",
      emptyMode: "hint"
    });
    this.roomId = null;
  }

  addLabel() {
    return t("room.add_net_outlet") || t("net_outlet.add");
  }

  emptyHintText() {
    return t("common.no_data");
  }

  init(roomId = null) {
    this.roomId = roomId;
    this.ensureContainer();
    this.bindExternalAddButton();
    if (!this.container) {
      return false;
    }

    this.container.innerHTML = "";
    this.updateEmptyState();
    return true;
  }

  createRow(data = {}) {
    const id = data.id || "";
    const name = data.name || "";
    const div = document.createElement("div");
    div.className = "room-net-outlet-item";
    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <input type="hidden" class="net-outlet-id" value="${escapeHtml(id)}" />
          <input type="text" class="net-outlet-name form-control" value="${escapeHtml(name)}" placeholder="${t("net_outlet.name")}" autocomplete="off" />
        </div>
        <div class="form-group room-net-outlets-actions">
          ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-danger remove-net-outlet-btn" })}
        </div>
      </div>
    `;
    this.bindItemEvents(div);
    return div;
  }

  async loadExisting(room) {
    this.ensureContainer();
    this.bindExternalAddButton();
    if (!this.container) {
      return;
    }
    this.container.innerHTML = "";

    const netOutlets = room?.net_outlets || [];
    netOutlets.forEach((no) => this.addItem(no));
    this.updateEmptyState();
  }

  collectData() {
    if (!this.ensureContainer()) {
      return [];
    }
    const items = this.container.querySelectorAll(".room-net-outlet-item");
    const netOutlets = [];
    for (const item of items) {
      const idInput = item.querySelector(".net-outlet-id");
      const nameInput = item.querySelector(".net-outlet-name");
      netOutlets.push({
        id: idInput?.value || null,
        name: (nameInput?.value || "").trim()
      });
    }
    return netOutlets;
  }
}

export const roomNetOutletsManager = new RoomNetOutletsManager();

// ==========================================
// 房间管理功能
// ==========================================

const tableState = createSortState("name", "asc");
let currentPage = 1;
let currentPageSize = DEFAULT_PAGE_SIZE;

export async function loadRoomsData(page = currentPage, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) {
    tableState.setSort(sortBy, sortOrder);
  }

  try {
    const roomsData = await apiGet(
      `/api/resources/rooms?page=${page}&page_size=${currentPageSize}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`
    );
    const data = roomsData.success ? roomsData.data : { items: [], total: 0 };
    const rooms = data.items || data;
    const startIndex = (page - 1) * currentPageSize;

    renderTable("#rooms-table", {
      data: rooms,
      columns: [
        {
          field: "id",
          render: (v, row, index) => startIndex + index + 1,
          className: "index-column"
        },
        { field: "name", render: (v) => escapeHtml(v) },
        {
          field: "room_type",
          className: "col-center",
          render: (v) => {
            const typeMap = {
              office: t("room.type_office"),
              lobby: t("room.type_lobby"),
              reception: t("room.type_reception"),
              data_center: t("room.type_datacenter"),
              telecom_closet: t("room.type_telecom_closet"),
              other: t("room.type_other")
            };
            return typeMap[(v || "").toLowerCase()] || v || "-";
          }
        },
        { field: "org_name", render: (v) => escapeHtml(v) || "-" },
        {
          field: "workstation_count",
          className: "col-center",
          render: (v) => (v != null ? String(v) : "0")
        },
        {
          field: "networks",
          render: (v) =>
            v && v.length > 0
              ? v.map((n) => `${escapeHtml(n.name)} (${escapeHtml(n.network_region)})`).join("<br>")
              : "-"
        },
        { field: "description", render: (v) => escapeHtml(v) || "-" },
        {
          field: "created_at",
          render: (v) => new Date(v).toLocaleString(),
          className: "col-center"
        },
        {
          field: "id",
          render: (v, row) => {
            const roomTypeLower = (row.room_type || "").toLowerCase();
            const isCabinetRoom =
              roomTypeLower === "data_center" || roomTypeLower === "telecom_closet";
            return `
          ${iconButton({ icon: "edit", label: t("common.edit"), cls: "btn-edit", attrs: `data-id="${v}"` })}
          ${iconButton({ icon: "list", label: isCabinetRoom ? t("room.cabinets") : t("room.workstations"), cls: "btn-primary btn-room-children-list", attrs: `data-room-id="${v}" data-room-type="${escapeHtml(row.room_type || "")}"` })}
          ${iconButton({ icon: "list", label: t("room.net_outlets"), cls: "btn-success btn-room-net-outlets-list", attrs: `data-room-id="${v}"` })}
          ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-delete", attrs: `data-id="${v}"` })}
        `;
          }
        }
      ],
      emptyMessage: t("common.no_data")
    });

    bindRoomButtonsEvents();

    if (data.total !== undefined) {
      appendPaginationToTable("#rooms-table", data, loadRoomsData, {
        pageSize: currentPageSize,
        onPageSizeChange: (size) => {
          currentPageSize = size;
          loadRoomsData(1);
        }
      });
    }
    updateSortIcons("rooms-table", tableState);
  } catch (error) {
    handleError(error, t("room.load_failed"), () => {
      renderTable("#rooms-table", {
        data: [],
        columns: [],
        emptyMessage: t("common.load_failed_retry")
      });
    });
  }
}

let roomTableClickHandler = null;

function bindRoomButtonsEvents() {
  const table = elementCache.get("rooms-table");
  if (!table) {
    return;
  }

  if (roomTableClickHandler) {
    table.removeEventListener("click", roomTableClickHandler);
  }

  roomTableClickHandler = async (e) => {
    const childrenBtn = e.target.closest(".btn-room-children-list");
    const outletsBtn = e.target.closest(".btn-room-net-outlets-list");
    if (childrenBtn) {
      const roomId = childrenBtn.dataset.roomId;
      if (roomId) {
        await openRoomChildrenListModal(roomId);
      }
    } else if (outletsBtn) {
      const roomId = outletsBtn.dataset.roomId;
      if (roomId) {
        await openRoomNetOutletsListModal(roomId);
      }
    }
  };

  table.addEventListener("click", roomTableClickHandler);
}

export async function openRoomChildrenListModal(roomId) {
  try {
    const result = await apiGet(`/api/resources/rooms/${roomId}`);
    if (!result.success || !result.data) {
      showToast(result.message || t("room.load_failed"), "error");
      return;
    }
    const room = result.data;
    const roomType = (room.room_type || "").toLowerCase();

    // "其他"房间工位与机柜均可能存在，优先展示有数据的一类
    const isCabinetList =
      roomType === "data_center" ||
      roomType === "telecom_closet" ||
      (roomType === "other" && !(room.workstations || []).length);

    if (!isCabinetList) {
      const workstations = room.workstations || [];
      await openSimpleListModal({
        title: `${room.name} - ${t("room.workstations")}`,
        columns: [{ label: t("common.name") }, { label: t("workstation.manager") }],
        rows: workstations.map((ws) => [escapeHtml(ws.name || "-"), escapeHtml(ws.manager || "-")])
      });
    } else {
      const cabinets = room.cabinets || [];
      await openSimpleListModal({
        title: `${room.name} - ${t("room.cabinets")}`,
        columns: [{ label: t("common.name") }, { label: t("cabinet.capacity") }],
        rows: cabinets.map((cab) => [escapeHtml(cab.name || "-"), cab.capacity ?? "-"])
      });
    }
  } catch (error) {
    handleError(error, t("room.load_failed"));
  }
}

export async function openRoomNetOutletsListModal(roomId) {
  try {
    const result = await apiGet(`/api/resources/rooms/${roomId}`);
    if (!result.success || !result.data) {
      showToast(result.message || t("room.load_failed"), "error");
      return;
    }
    const room = result.data;
    const netOutlets = room.net_outlets || [];
    await openSimpleListModal({
      title: `${room.name} - ${t("room.net_outlets")}`,
      columns: [{ label: t("common.name") }],
      rows: netOutlets.map((no) => [escapeHtml(no.name || "-")])
    });
  } catch (error) {
    handleError(error, t("room.load_failed"));
  }
}

export function initRoomSortEvents() {
  initSortEvents("rooms-table", tableState, loadRoomsData);
}

// 编辑房间（轻量端点：仅基础字段 + 网络绑定，避免完整详情的 7 次查询）
export async function editRoom(id) {
  try {
    const result = await apiGet(`/api/resources/rooms/${id}/brief`);
    if (result.success) {
      openRoomModal(result.data);
    } else {
      showToast(`${t("room.fetch_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("room.fetch_failed"));
  }
}

// 删除房间
export async function deleteRoom(id) {
  await handleDelete(id, "/api/resources/rooms", t("room.delete_success"), loadRoomsData);
}

// 提交房间表单
export async function submitRoomForm() {
  const id = getElementValue("room-id");
  const name = getElementValue("room-name");
  const roomType = getElementValue("room-type");
  const description = getElementValue("room-description");

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

  // 收集并校验子项数据（工位/机柜）
  const childrenData = roomChildrenManager.collectData();
  const childrenError = validateRoomChildren(childrenData, roomType);
  if (childrenError) {
    showToast(childrenError, "warning");
    return;
  }

  // 信息点名称在所有房间范围内唯一：先做本地非空/查重校验，跨房间冲突由后端兜底
  const netOutletsForCheck = roomNetOutletsManager.collectData();
  const netOutletsError = validateNetOutletsLocal(netOutletsForCheck);
  if (netOutletsError) {
    showToast(netOutletsError, "warning");
    return;
  }

  // 后端统一按大写存储房型
  const formattedRoomType = roomType.toUpperCase();

  const orgId = getElementValue("room-org-id");

  const roomData = {
    name: name.trim(),
    room_type: formattedRoomType,
    org_id: orgId || null,
    network_ids: networkIds,
    description: description || null
  };

  try {
    let roomId = id;
    if (id) {
      const result = await apiPut(`/api/resources/rooms/${id}`, roomData);
      if (!result.success) {
        showToast(result.message || t("room.save_failed"), "error");
        return;
      }
    } else {
      const result = await apiPost("/api/resources/rooms", roomData);
      if (!result.success) {
        showToast(result.message || t("room.save_failed"), "error");
        return;
      }
      roomId = result.data?.id;
      if (!roomId) {
        showToast(t("room.no_id_returned"), "error");
        return;
      }
      // 写回隐藏 ID，便于重试时按更新处理
      elementCache.setValue("room-id", roomId);
    }

    // 同步子项（工位/机柜）
    const syncResult = await apiPut(`/api/resources/rooms/${roomId}/children`, childrenData);
    if (!syncResult.success) {
      showToast(syncResult.message || t("room.children_save_failed"), "error");
      await loadRoomsData();
      return;
    }

    // 同步信息点（所有房型通用）
    const netOutletsData = netOutletsForCheck;
    const noSyncResult = await apiPut(`/api/resources/rooms/${roomId}/net-outlets`, {
      net_outlets: netOutletsData
    });
    if (!noSyncResult.success) {
      showToast(noSyncResult.message || t("room.net_outlets_save_failed"), "error");
      await loadRoomsData();
      return;
    }

    showToast(t("room.save_success"), "success");
    closeModal("room-modal");
    await loadRoomsData();
  } catch (error) {
    handleError(error, t("room.save_failed"));
  }
}

// 校验房间子项数据（办公类含"其他"校验工位；机房类含"其他"校验机柜）
function validateRoomChildren(childrenData, roomType) {
  const type = (roomType || "").toLowerCase();
  const checkWorkstations = ["office", "lobby", "reception", "other"].includes(type);
  const checkCabinets = ["data_center", "telecom_closet", "other"].includes(type);

  if (checkWorkstations) {
    for (const ws of childrenData.workstations || []) {
      if (!ws.name) {
        return t("room.child_name_required");
      }
    }
  }
  if (checkCabinets) {
    for (const cab of childrenData.cabinets || []) {
      if (!cab.name) {
        return t("room.child_name_required");
      }
      if (!Number.isInteger(cab.capacity) || cab.capacity < 1 || cab.capacity > 48) {
        return t("room.child_capacity_invalid");
      }
    }
  }
  return null;
}

// 信息点本地校验：名称非空且不重复（全局唯一，跨房间重名由后端校验）
function validateNetOutletsLocal(netOutlets) {
  const seen = new Set();
  for (const outlet of netOutlets) {
    if (!outlet.name) {
      return t("net_outlet.name_required");
    }
    if (seen.has(outlet.name)) {
      return t("net_outlet.name_duplicate", { name: outlet.name });
    }
    seen.add(outlet.name);
  }
  return null;
}

// 房间管理模态框
export async function openRoomModal(room = null) {
  await openModal("room-modal");

  const title = elementCache.get("room-modal-title");
  const form = elementCache.get("room-form");

  // 组织树 / 组织员工 / 网段选项相互独立，并行加载缩短弹窗就绪时间
  const networkReady = room ? loadRoomNetworks(room) : roomNetworkConfigManager.init();
  await Promise.all([
    loadOrgsForSelect("room-org-id"),
    room ? loadRoomOrgEmployees(room.org_id || "") : loadRoomOrgEmployees("")
  ]);
  bindRoomOrgEmployeeSync();

  if (room) {
    title.textContent = t("room.edit");
    elementCache.setValue("room-id", room.id);
    elementCache.setValue("room-name", room.name);
    elementCache.setValue("room-type", room.room_type ? room.room_type.toLowerCase() : "office");
    elementCache.setValue("room-org-id", room.org_id || "");
    elementCache.setValue("room-description", room.description || "");

    // 初始化子项管理器并加载现有工位/机柜（依赖员工列表就绪，供管理人下拉渲染）
    roomChildrenManager.roomType = (room.room_type || "OFFICE").toLowerCase();
    roomChildrenManager.loadExisting(room);

    // 初始化信息点管理器并加载现有信息点
    roomNetOutletsManager.loadExisting(room);
  } else {
    title.textContent = t("room.add");
    if (form) {
      form.reset();
    }
    elementCache.setValue("room-id", "");

    // 初始化子项管理器为默认空状态
    roomChildrenManager.roomType = "office";
    roomChildrenManager.init();

    // 初始化信息点管理器为默认空状态
    roomNetOutletsManager.init();
  }

  // 网段配置渲染不依赖表单其余字段，最后等待完成即可
  await networkReady;
}

// 组织切换 → 重新加载该组织员工并刷新管理人下拉
// （模态框每次打开会重建 DOM，因此每次都需重新绑定）
function bindRoomOrgEmployeeSync() {
  const orgSelect = document.getElementById("room-org-id");
  if (!orgSelect) {
    return;
  }
  orgSelect.addEventListener("change", async (e) => {
    await loadRoomOrgEmployees(e.target.value);
    refreshManagerSelects();
  });
}

// 加载房间的网络配置
async function loadRoomNetworks(room) {
  try {
    const networksResult = await apiGet("/api/resources/options/networks");
    if (networksResult.success) {
      const allNetworks = networksResult.data.items || networksResult.data || [];
      await roomNetworkConfigManager.loadExistingNetworks(room.networks, allNetworks);
    } else {
      await roomNetworkConfigManager.init();
    }
  } catch (error) {
    console.error("加载网络信息失败:", error);
    await roomNetworkConfigManager.init();
  }
}
