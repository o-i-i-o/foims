// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  debounce,
  handleError,
  appendPaginationToTable,
  escapeHtml,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";

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
  return timestamp && (Date.now() - timestamp < networkCache.CACHE_TTL);
}

function extractItems(result) {
  if (!result.success || !result.data) return [];
  if (Array.isArray(result.data)) return result.data;
  if (result.data.items && Array.isArray(result.data.items)) return result.data.items;
  return [];
}

// 更新选择框选项
function updateSelect(select, data, placeholder = '选择选项') {
  const currentValue = select.value;
  select.innerHTML = `<option value="">${placeholder}</option>` +
    data.map(item => `<option value="${item.id}">${item.name}</option>`).join('');
  
  if (currentValue) select.value = currentValue;
  return select;
}

// 加载网络区域
async function loadNetworkRegions(select) {
  if (!select) return [];
  
  const now = Date.now();
  if (networkCache.networkRegions && isCacheValid(networkCache.cacheTime)) {
    updateSelect(select, networkCache.networkRegions, '选择网络区域');
    return networkCache.networkRegions;
  }

  try {
    const result = await apiGet("/api/resources/network-regions?page_size=1000");
    const items = extractItems(result);
    if (items.length > 0) {
      networkCache.networkRegions = items;
      networkCache.cacheTime = now;
      updateSelect(select, items, '选择网络区域');
      return items;
    }
  } catch (error) {
    console.error("加载网络区域失败:", error);
  }
  
  return [];
}

// 加载网段
async function loadNetworks(regionId, select, excludeIds = []) {
  if (!select) return [];
  
  const cacheKey = regionId || 'all';
  const cached = networkCache.networks.get(cacheKey);
  
  if (cached && isCacheValid(cached.timestamp)) {
    const filtered = cached.data.filter(n => !excludeIds.includes(n.id));
    updateSelect(select, filtered, '选择网段');
    return filtered;
  }

  try {
    const url = regionId ? `/api/resources/networks?region_id=${regionId}&page_size=1000` : '/api/resources/networks?page_size=1000';
    const result = await apiGet(url);
    
    const items = extractItems(result);
    if (items.length > 0) {
      networkCache.networks.set(cacheKey, { data: items, timestamp: Date.now() });
      const filtered = items.filter(n => !excludeIds.includes(n.id));
      updateSelect(select, filtered, '选择网段');
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
    if (oldHandler) element.removeEventListener(event, oldHandler);
    
    element.addEventListener(event, handler);
    this.handlers.set(element, handler);
    return handler;
  }

  clear() {
    for (const [element, handler] of this.handlers.entries()) {
      if (element && handler) {
        element.removeEventListener('click', handler);
        element.removeEventListener('change', handler);
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
    this.eventHandler = new EventHandler(this);
  }

  async init() {
    this.container = document.getElementById(this.options.containerId);
    if (!this.container) {
      console.error(`未找到${this.options.containerId}元素`);
      return false;
    }

    this.container.innerHTML = '';
    await this.addItem();
    this.updateAddButtons();
    return true;
  }

  createItemHTML() {
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

  async addItem() {
    if (!this.container) return;

    const div = document.createElement('div');
    div.innerHTML = this.createItemHTML();
    const item = div.firstElementChild;

    this.container.appendChild(item);

    const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
    await loadNetworkRegions(regionSelect);

    this.bindItemEvents(item);
    this.updateAddButtons();
    return item;
  }

  updateAddButtons() {
    if (!this.container) return;
    
    const items = this.container.querySelectorAll('.network-config-item');
    items.forEach((item, index) => {
      const addBtn = item.querySelector('.add-btn');
      if (addBtn) {
        addBtn.style.display = index === items.length - 1 ? '' : 'none';
      }
    });
  }

  bindItemEvents(item) {
    const { regionSelectClass, networkSelectClass, removeBtnClass } = this.options;
    
    const removeBtn = item.querySelector(`.${removeBtnClass}`);
    if (removeBtn) {
      this.eventHandler.bind(removeBtn, 'click', () => this.removeItem(item));
    }

    const addBtn = item.querySelector('.add-btn');
    if (addBtn) {
      this.eventHandler.bind(addBtn, 'click', () => this.addItem());
    }

    const regionSelect = item.querySelector(`.${regionSelectClass}`);
    if (regionSelect) {
      this.eventHandler.bind(regionSelect, 'change', async () => {
        await this.onRegionChange(regionSelect, item);
        this.updateNetworkSelects();
      });
    }

    const networkSelect = item.querySelector(`.${networkSelectClass}`);
    if (networkSelect) {
      this.eventHandler.bind(networkSelect, 'change', () => this.updateNetworkSelects());
    }
  }

  removeItem(item) {
    const items = this.container.querySelectorAll('.network-config-item');
    if (items.length <= 1) {
      showToast('至少需要保留一个网段配置', 'warning');
      return;
    }
    
    item.remove();
    this.updateAddButtons();
    this.updateNetworkSelects();
  }

  async onRegionChange(regionSelect, item) {
    const regionId = regionSelect.value;
    const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);
    
    if (!networkSelect) return;
    
    networkSelect.innerHTML = '<option value="">选择网段</option>';
    if (regionId) {
      await loadNetworks(regionId, networkSelect, this.getSelectedNetworkIds());
    }
  }

  async updateNetworkSelects() {
    const selectedIds = this.getSelectedNetworkIds();
    const items = this.container.querySelectorAll('.network-config-item');
    
    for (const item of items) {
      const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
      const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);
      const currentValue = networkSelect.value;
      const regionId = regionSelect.value;
      
      if (regionId) {
        const otherIds = selectedIds.filter(id => id !== currentValue);
        await loadNetworks(regionId, networkSelect, otherIds);
        if (currentValue) networkSelect.value = currentValue;
      }
    }
  }

  getSelectedNetworkIds() {
    const selects = this.container.querySelectorAll(`.${this.options.networkSelectClass}`);
    return Array.from(selects).map(s => s.value).filter(Boolean);
  }

  async loadExistingNetworks(networks, allNetworks) {
    if (!this.container) {
      this.container = document.getElementById(this.options.containerId);
    }
    if (!this.container) return;
    
    if (!networks?.length) {
      await this.init();
      return;
    }

    this.container.innerHTML = '';
    
    const networkMap = new Map(allNetworks.map(n => [n.id, n]));
    const selectedIds = networks.map(n => n.id);
    
    await loadNetworkRegions(document.createElement('select'));
    
    networks.forEach((network) => {
      const networkInfo = networkMap.get(network.id);
      if (!networkInfo) return;
      
      const div = document.createElement('div');
      div.innerHTML = this.createItemHTML();
      const item = div.firstElementChild;
      
      const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
      const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);
      
      loadNetworkRegions(regionSelect).then(() => {
        regionSelect.value = networkInfo.network_region_id;
        
        const otherIds = selectedIds.filter(id => id !== network.id);
        loadNetworks(networkInfo.network_region_id, networkSelect, otherIds).then(() => {
          networkSelect.value = network.id;
        });
      });
      
      this.container.appendChild(item);
      this.bindItemEvents(item);
    });
    
    this.updateAddButtons();
  }

  collectData() {
    if (!this.container) return { networkIds: [], hasEmpty: false };
    
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
  addBtnId: "add-network-config-btn",
  removeBtnClass: "remove-network-config-btn"
});

// ==========================================
// 房间管理功能
// ==========================================

let currentRoomPage = 1;
const ROOM_PAGE_SIZE = 20;
let currentRoomSort = { by: "name", order: "asc" };

// 加载房间数据
export async function loadRoomsData(page = 1, sortBy = null, sortOrder = null) {
  currentRoomPage = page;
  if (sortBy) currentRoomSort.by = sortBy;
  if (sortOrder) currentRoomSort.order = sortOrder;
  
  try {
    const roomsData = await apiGet(`/api/resources/rooms?page=${page}&page_size=${ROOM_PAGE_SIZE}&sort_by=${currentRoomSort.by}&sort_order=${currentRoomSort.order}`);
    const tbody = document.querySelector("#rooms-table tbody");
    tbody.innerHTML = "";

    const data = roomsData.success ? roomsData.data : { items: [], total: 0 };
    const rooms = data.items || data;

    if (rooms.length > 0) {
      const startIndex = (page - 1) * ROOM_PAGE_SIZE;
      rooms.forEach((room, index) => {
        let networkDisplay = "-";
        if (room.networks && room.networks.length > 0) {
          networkDisplay = room.networks
            .map((network) => {
              return `${network.name} (${network.network_region})`;
            })
            .join("<br>");
        }

        const roomTypeLower = room.room_type ? room.room_type.toLowerCase() : '';
        const roomTypeText = roomTypeLower === "office" ? t('room.type_office') : roomTypeLower === "data_center" ? t('room.type_datacenter') : room.room_type || '-';

        const row = document.createElement("tr");
        row.innerHTML = `
                    <td class="index-column">${startIndex + index + 1}</td>
                    <td>${escapeHtml(room.name)}</td>
                    <td>${escapeHtml(roomTypeText)}</td>
                    <td>${networkDisplay}</td>
                    <td>${escapeHtml(room.description) || "-"}</td>
                    <td>${new Date(room.created_at).toLocaleString()}</td>
                    <td>
                    <button class="btn btn-sm btn-edit" data-id="${room.id}">${t('common.edit')}</button>
                    <button class="btn btn-sm btn-delete" data-id="${room.id}">${t('common.delete')}</button>
                </td>
                `;
        tbody.appendChild(row);
      });

      if (data.total !== undefined) {
        appendPaginationToTable("#rooms-table", data, loadRoomsData);
      }
    } else {
      tbody.innerHTML =
        `<tr class="empty-row"><td colspan="7" class="text-center">${t('common.no_data')}</td></tr>`;
    }
    updateRoomSortIcons();
  } catch (error) {
    console.error("加载房间数据失败:", error);
    const tbody = document.querySelector("#rooms-table tbody");
    tbody.innerHTML =
      '<tr class="empty-row"><td colspan="7" class="text-center">加载失败，请刷新页面重试</td></tr>';
  }
}

// 更新排序图标
function updateRoomSortIcons() {
  const table = document.getElementById("rooms-table");
  if (!table) return;
  
  table.querySelectorAll("th.sortable").forEach(th => {
    const sortKey = th.dataset.sort;
    
    if (sortKey === currentRoomSort.by) {
      th.classList.add("sorted", currentRoomSort.order);
      th.classList.remove(currentRoomSort.order === "asc" ? "desc" : "asc");
    } else {
      th.classList.remove("sorted", "asc", "desc");
    }
  });
}

// 初始化房间排序事件
export function initRoomSortEvents() {
  const table = document.getElementById("rooms-table");
  if (!table) return;
  
  table.querySelectorAll("th.sortable").forEach(th => {
    th.addEventListener("click", () => {
      const sortKey = th.dataset.sort;
      const newOrder = (currentRoomSort.by === sortKey && currentRoomSort.order === "asc") ? "desc" : "asc";
      loadRoomsData(1, sortKey, newOrder);
    });
  });
  
  updateRoomSortIcons();
}

// 编辑房间
export async function editRoom(id) {
  try {
    const result = await apiGet(`/api/resources/rooms/${id}`);
    if (result.success) {
      openRoomModal(result.data);
    } else {
      showToast(`获取房间数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取房间数据失败");
  }
}

// 删除房间
export async function deleteRoom(id) {
  await handleDelete(id, "/api/resources/rooms", "房间删除成功", loadRoomsData);
}

// 提交房间表单
export async function submitRoomForm() {
  const id = getElementValue("room-id");
  const name = getElementValue("room-name");
  const roomType = getElementValue("room-type");
  const description = getElementValue("room-description");

  if (!name) {
    showToast(t('room.name_required'), "warning");
    return;
  }

  if (!roomType) {
    showToast(t('room.type_required'), "warning");
    return;
  }

  const { networkIds, hasEmpty } = roomNetworkConfigManager.collectData();
  
  if (hasEmpty) {
    showToast(t('room.network_all_required'), "warning");
    return;
  }
  
  if (networkIds.length === 0) {
    showToast(t('room.network_required'), "warning");
    return;
  }

  let formattedRoomType;
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
    reloadFunction: loadRoomsData
  });

  return success;
}

// 房间管理模态框
export async function openRoomModal(room = null) {
  openModal("room-modal");
  
  const modal = elementCache.get('room-modal');
  const title = elementCache.get('room-modal-title');
  const form = elementCache.get('room-form');

  if (room) {
    title.textContent = "编辑房间";
    elementCache.setValue('room-id', room.id);
    elementCache.setValue('room-name', room.name);
    elementCache.setValue('room-type', room.room_type ? room.room_type.toLowerCase() : "office");
    elementCache.setValue('room-description', room.description || "");

    await loadRoomNetworks(room);
  } else {
    title.textContent = "添加房间";
    form.reset();
    elementCache.setValue('room-id', '');
    await roomNetworkConfigManager.init();
  }
}

// 加载房间的网络配置
async function loadRoomNetworks(room) {
  try {
    const networksResult = await apiGet("/api/resources/networks?page_size=1000");
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
