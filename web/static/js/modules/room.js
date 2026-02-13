// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
  getAccessToken,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  debounce,
  handleError
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";

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
    const result = await apiGet("/api/resources/network-regions");
    if (result.success && result.data) {
      networkCache.networkRegions = result.data;
      networkCache.cacheTime = now;
      updateSelect(select, result.data, '选择网络区域');
      return result.data;
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
    const url = regionId ? `/api/resources/networks?region_id=${regionId}` : '/api/resources/networks';
    const result = await apiGet(url);
    
    if (result.success && result.data) {
      networkCache.networks.set(cacheKey, { data: result.data, timestamp: Date.now() });
      const filtered = result.data.filter(n => !excludeIds.includes(n.id));
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
      element.removeEventListener(handler.event, handler);
    }
    this.handlers.clear();
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

// 加载房间数据
export async function loadRoomsData() {
  const token = getAccessToken();
  if (!token) {
    return;
  }

  try {
    const roomsResponse = await fetch("/api/resources/rooms", {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const roomsData = await roomsResponse.json();
    const tbody = document.querySelector("#rooms-table tbody");

    if (roomsData.success && roomsData.data.length > 0) {
      tbody.innerHTML = "";
      roomsData.data.forEach((room) => {
        let networkDisplay = "-";
        if (room.networks && room.networks.length > 0) {
          networkDisplay = room.networks
            .map((network) => {
              return `${network.name} (${network.network_region})`;
            })
            .join("<br>");
        }

        let roomTypeLower = room.room_type.toLowerCase();
        let roomTypeText = roomTypeLower === "office" ? "办公室" : roomTypeLower === "data_center" ? "机房" : room.room_type;

        const row = document.createElement("tr");
        row.innerHTML = `
                    <td>${room.name}</td>
                    <td>${roomTypeText}</td>
                    <td>${networkDisplay}</td>
                    <td>${room.description || "-"}</td>
                    <td>${new Date(room.created_at).toLocaleString()}</td>
                    <td>
                    <button class="btn btn-sm btn-edit" data-id="${room.id}">编辑</button>
                    <button class="btn btn-sm btn-delete" data-id="${room.id}">删除</button>
                </td>
                `;
        tbody.appendChild(row);
      });
    } else {
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="6" class="text-center">暂无房间数据</td></tr>';
    }
  } catch (error) {
    console.error("加载房间数据失败:", error);
    const tbody = document.querySelector("#rooms-table tbody");
    tbody.innerHTML =
      '<tr class="empty-row"><td colspan="6" class="text-center">加载失败，请刷新页面重试</td></tr>';
  }
}

// 编辑房间
export async function editRoom(id) {
  const token = getAccessToken();
  if (!token) return;

  try {
    const response = await fetch(`/api/resources/rooms/${id}`, {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const result = await response.json();
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
  const token = getAccessToken();
  if (!token) return;

  const id = getElementValue("room-id");
  const name = getElementValue("room-name", "trimmed");
  const roomType = getElementValue("room-type");
  const description = getElementValue("room-description", "trimmed");

  if (!name) {
    showToast("房间名称不能为空", "warning");
    return;
  }

  if (!roomType) {
    showToast("房间类型不能为空", "warning");
    return;
  }

  const { networkIds, hasEmpty } = roomNetworkConfigManager.collectData();
  
  if (hasEmpty) {
    showToast("请选择所有网段", "warning");
    return;
  }
  
  if (networkIds.length === 0) {
    showToast("至少需要选择一个网段", "warning");
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
    token,
    successMessage: "房间保存成功",
    modalId: "room-modal",
    reloadFunction: loadRoomsData
  });

  return success;
}

// 房间管理模态框
export async function openRoomModal(room = null) {
  const modal = document.getElementById("room-modal");
  const title = document.getElementById("room-modal-title");
  const form = document.getElementById("room-form");

  if (room) {
    title.textContent = "编辑房间";
    document.getElementById("room-id").value = room.id;
    document.getElementById("room-name").value = room.name;
    document.getElementById("room-type").value = room.room_type.toLowerCase();
    document.getElementById("room-description").value = room.description || "";

    await loadRoomNetworks(room);
  } else {
    title.textContent = "添加房间";
    form.reset();
    document.getElementById("room-id").value = "";
    await roomNetworkConfigManager.init();
  }

  openModal("room-modal");
}

// 加载房间的网络配置
async function loadRoomNetworks(room) {
  const localRememberMe = localStorage.getItem("rememberMe");
  let token;
  if (localRememberMe === "true") {
    token = localStorage.getItem("access_token");
  } else {
    token = sessionStorage.getItem("access_token");
  }
  
  if (!token) {
    await roomNetworkConfigManager.init();
    return;
  }

  try {
    const networksResponse = await fetch("/api/resources/networks", {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });
    const networksResult = await networksResponse.json();
    if (networksResult.success) {
      await roomNetworkConfigManager.loadExistingNetworks(room.networks, networksResult.data);
    } else {
      await roomNetworkConfigManager.init();
    }
  } catch (error) {
    console.error("加载网络信息失败:", error);
    await roomNetworkConfigManager.init();
  }
}
