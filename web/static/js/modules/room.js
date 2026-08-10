// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
} from "../utils/apiClient.js";

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
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";
import { loadOrgsForSelect } from "../utils/resources.js";

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

    this.container.innerHTML = '';
    await this.addItem();
    return true;
  }

  createItemHTML() {
    const { regionSelectClass, networkSelectClass, removeBtnClass, addBtnClass } = this.options;

    return `
      <div class="network-config-item">
        <div class="form-row">
          <div class="form-group">
            <select class="${regionSelectClass}" required>
              <option value="">选择网络区域</option>
            </select>
          </div>
          <div class="form-group">
            <select class="${networkSelectClass}" required>
              <option value="">选择网段</option>
            </select>
          </div>
          <div class="form-group network-config-actions">
            <button type="button" class="btn btn-danger btn-sm ${removeBtnClass}">
              删除
            </button>
            <button type="button" class="btn btn-secondary btn-sm ${addBtnClass}" style="display: none;">
              ${t('network.add_network_config')}
            </button>
          </div>
        </div>
      </div>
    `;
  }

  async addItem() {
    if (!this.ensureContainer()) return;

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
    if (!this.ensureContainer()) return;

    const items = this.container.querySelectorAll('.network-config-item');
    items.forEach((item, index) => {
      const addBtn = item.querySelector(`.${this.options.addBtnClass}`);
      if (addBtn) {
        addBtn.style.display = index === items.length - 1 ? '' : 'none';
      }
    });
  }

  bindItemEvents(item) {
    const { regionSelectClass, networkSelectClass, removeBtnClass, addBtnClass } = this.options;

    const removeBtn = item.querySelector(`.${removeBtnClass}`);
    if (removeBtn) {
      this.eventHandler.bind(removeBtn, 'click', () => this.removeItem(item));
    }

    const addBtn = item.querySelector(`.${addBtnClass}`);
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
    if (!this.ensureContainer()) return;
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
    if (!this.ensureContainer()) return;
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
    if (!this.ensureContainer()) return [];
    const selects = this.container.querySelectorAll(`.${this.options.networkSelectClass}`);
    return Array.from(selects).map(s => s.value).filter(Boolean);
  }

  async loadExistingNetworks(networks, allNetworks) {
    // closeModal 会移除模态框 DOM，需重新获取 container
    this.ensureContainer();
    if (!this.container) return;

    if (!networks?.length) {
      await this.init();
      return;
    }

    this.container.innerHTML = '';

    const networkMap = new Map(allNetworks.map(n => [n.id, n]));
    const selectedIds = networks.map(n => n.id);

    await loadNetworkRegions(document.createElement('select'));

    for (const network of networks) {
      const networkInfo = networkMap.get(network.id);
      if (!networkInfo) continue;

      const div = document.createElement('div');
      div.innerHTML = this.createItemHTML();
      const item = div.firstElementChild;

      const regionSelect = item.querySelector(`.${this.options.regionSelectClass}`);
      const networkSelect = item.querySelector(`.${this.options.networkSelectClass}`);

      await loadNetworkRegions(regionSelect);
      regionSelect.value = networkInfo.network_region_id;

      const otherIds = selectedIds.filter(id => id !== network.id);
      await loadNetworks(networkInfo.network_region_id, networkSelect, otherIds);
      networkSelect.value = network.id;

      this.container.appendChild(item);
      this.bindItemEvents(item);
    }

    this.updateAddButtons();
  }

  collectData() {
    if (!this.ensureContainer()) return { networkIds: [], hasEmpty: false };
    
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
  addBtnClass: "add-network-config-btn"
});

// ==========================================
// 房间子项（工位/机柜）动态管理模块
// ==========================================

class RoomChildrenManager {
  constructor() {
    this.container = null;
    this.roomType = 'office';
    this.handlers = new WeakMap();
    this.addHandler = null;
    this.typeChangeHandler = null;
  }

  ensureContainer() {
    if (!this.container || !document.contains(this.container)) {
      this.container = document.getElementById('room-children-container');
    }
    return this.container;
  }

  init() {
    this.ensureContainer();
    if (!this.container) return false;

    this.container.innerHTML = '';
    this.bindTypeChange();
    this.updateLabels();
    this.updateEmptyState();
    return true;
  }

  bindTypeChange() {
    const typeSelect = document.getElementById('room-type');
    if (!typeSelect) return;
    if (this.typeChangeHandler) {
      typeSelect.removeEventListener('change', this.typeChangeHandler);
    }
    this.typeChangeHandler = (e) => this.onTypeChange(e.target.value);
    typeSelect.addEventListener('change', this.typeChangeHandler);
  }

  onTypeChange(newType) {
    this.roomType = newType;
    this.updateLabels();
    // 类型切换时清空列表
    if (this.ensureContainer()) {
      this.container.innerHTML = '';
      this.updateEmptyState();
    }
  }

  updateLabels() {
    const label = document.getElementById('room-children-label');
    if (this.roomType === 'office') {
      if (label) label.textContent = t('room.workstations');
    } else {
      if (label) label.textContent = t('room.cabinets');
    }
  }

  updateEmptyState() {
    if (!this.ensureContainer()) return;
    const existing = this.container.querySelector('.room-child-empty');
    const items = this.container.querySelectorAll('.room-child-item');
    if (items.length === 0 && !existing) {
      const emptyDiv = document.createElement('div');
      emptyDiv.className = 'room-child-empty';
      const addLabel = this.roomType === 'office' ? t('room.add_workstation') : t('room.add_cabinet');
      emptyDiv.innerHTML = `<button type="button" class="btn btn-secondary btn-sm add-child-btn">${addLabel}</button>`;
      const addBtn = emptyDiv.querySelector('.add-child-btn');
      if (addBtn) {
        const handler = () => this.addItem();
        this.handlers.set(addBtn, handler);
        addBtn.addEventListener('click', handler);
      }
      this.container.appendChild(emptyDiv);
    } else if (items.length > 0 && existing) {
      existing.remove();
    }
  }

  createWorkstationRow(data = {}) {
    const id = data.id || '';
    const name = data.name || '';
    const manager = data.manager || '';
    const div = document.createElement('div');
    div.className = 'room-child-item';
    div.dataset.childType = 'workstation';
    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <input type="hidden" class="child-id" value="${escapeHtml(id)}" />
          <input type="text" class="child-name form-control" value="${escapeHtml(name)}" placeholder="${t('workstation.name')}" autocomplete="off" />
        </div>
        <div class="form-group">
          <input type="text" class="child-manager form-control" value="${escapeHtml(manager)}" placeholder="${t('workstation.manager')}" autocomplete="off" />
        </div>
        <div class="form-group room-children-actions">
          <button type="button" class="btn btn-danger btn-sm remove-child-btn">${t('common.delete')}</button>
          <button type="button" class="btn btn-secondary btn-sm add-child-btn" style="display: none;">${t('room.add_workstation')}</button>
        </div>
      </div>
    `;
    this.bindItemEvents(div);
    return div;
  }

  createCabinetRow(data = {}) {
    const id = data.id || '';
    const name = data.name || '';
    const capacity = data.capacity || 42;
    const div = document.createElement('div');
    div.className = 'room-child-item';
    div.dataset.childType = 'cabinet';
    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <input type="hidden" class="child-id" value="${escapeHtml(id)}" />
          <input type="text" class="child-name form-control" value="${escapeHtml(name)}" placeholder="${t('cabinet.name')}" autocomplete="off" />
        </div>
        <div class="form-group">
          <input type="number" class="child-capacity form-control" value="${capacity}" min="1" max="48" placeholder="${t('cabinet.capacity')}" />
        </div>
        <div class="form-group room-children-actions">
          <button type="button" class="btn btn-danger btn-sm remove-child-btn">${t('common.delete')}</button>
          <button type="button" class="btn btn-secondary btn-sm add-child-btn" style="display: none;">${t('room.add_cabinet')}</button>
        </div>
      </div>
    `;
    this.bindItemEvents(div);
    return div;
  }

  addItem(data = {}) {
    if (!this.ensureContainer()) return;
    // 移除空状态提示
    const emptyState = this.container.querySelector('.room-child-empty');
    if (emptyState) emptyState.remove();
    const item = this.roomType === 'office'
      ? this.createWorkstationRow(data)
      : this.createCabinetRow(data);
    this.container.appendChild(item);
    this.updateAddButtons();
  }

  updateAddButtons() {
    if (!this.ensureContainer()) return;
    const items = this.container.querySelectorAll('.room-child-item');
    items.forEach((item, index) => {
      const addBtn = item.querySelector('.add-child-btn');
      if (addBtn) {
        addBtn.style.display = index === items.length - 1 ? '' : 'none';
        addBtn.textContent = this.roomType === 'office' ? t('room.add_workstation') : t('room.add_cabinet');
      }
    });
  }

  bindItemEvents(item) {
    const removeBtn = item.querySelector('.remove-child-btn');
    if (removeBtn) {
      const handler = () => this.removeItem(item);
      this.handlers.set(removeBtn, handler);
      removeBtn.addEventListener('click', handler);
    }

    const addBtn = item.querySelector('.add-child-btn');
    if (addBtn) {
      const handler = () => this.addItem();
      this.handlers.set(addBtn, handler);
      addBtn.addEventListener('click', handler);
    }
  }

  removeItem(item) {
    item.remove();
    this.ensureContainer();
    this.updateEmptyState();
    this.updateAddButtons();
  }

  loadExisting(children) {
    // closeModal 会移除模态框 DOM，需重新获取 container
    this.ensureContainer();
    if (!this.container) return;
    this.container.innerHTML = '';
    this.bindTypeChange();

    if (this.roomType === 'office' && children?.workstations?.length) {
      children.workstations.forEach(ws => this.addItem(ws));
    } else if ((this.roomType === 'data_center' || this.roomType === 'telecom_closet') && children?.cabinets?.length) {
      children.cabinets.forEach(cab => this.addItem(cab));
    }
    this.updateLabels();
    this.updateEmptyState();
    this.updateAddButtons();
  }

  collectData() {
    if (!this.ensureContainer()) return this.roomType === 'office' ? { workstations: [] } : { cabinets: [] };
    const items = this.container.querySelectorAll('.room-child-item');
    if (this.roomType === 'office') {
      const workstations = [];
      for (const item of items) {
        const idInput = item.querySelector('.child-id');
        const nameInput = item.querySelector('.child-name');
        const managerInput = item.querySelector('.child-manager');
        workstations.push({
          id: idInput?.value || null,
          name: (nameInput?.value || '').trim(),
          manager: (managerInput?.value || '').trim() || null,
        });
      }
      return { workstations };
    }
    const cabinets = [];
    for (const item of items) {
      const idInput = item.querySelector('.child-id');
      const nameInput = item.querySelector('.child-name');
      const capacityInput = item.querySelector('.child-capacity');
      cabinets.push({
        id: idInput?.value || null,
        name: (nameInput?.value || '').trim(),
        capacity: parseInt(capacityInput?.value || '42', 10),
      });
    }
    return { cabinets };
  }
}

export const roomChildrenManager = new RoomChildrenManager();

// ==========================================
// 房间信息点管理模块 - 所有房型通用
// ==========================================

class RoomNetOutletsManager {
  constructor() {
    this.container = null;
    this.handlers = new WeakMap();
    this.addHandler = null;
    this.typeChangeHandler = null;
    this.roomId = null;
    this.cabinets = []; // 当前房间的机柜选项 [{id, name}]
  }

  ensureContainer() {
    if (!this.container || !document.contains(this.container)) {
      this.container = document.getElementById('room-net-outlets-container');
    }
    return this.container;
  }

  init(roomId = null) {
    this.roomId = roomId;
    this.ensureContainer();
    if (!this.container) return false;

    this.container.innerHTML = '';
    this.updateEmptyState();
    return true;
  }

  async loadCabinets(roomId) {
    this.roomId = roomId;
    if (!roomId) {
      this.cabinets = [];
      return;
    }
    try {
      const result = await apiGet(`/api/resources/cabinets?room_id=${roomId}&page_size=1000`);
      const data = result.success ? result.data : {};
      this.cabinets = (data.items || data || []).map(c => ({ id: c.id, name: c.name }));
    } catch (e) {
      this.cabinets = [];
    }
  }

  // 刷新所有已存在行的机柜下拉选项
  refreshCabinetOptions() {
    if (!this.ensureContainer()) return;
    const rows = this.container.querySelectorAll('.room-net-outlet-item');
    const opts = this.cabinetOptionsHtml();
    rows.forEach(row => {
      const sel = row.querySelector('.net-outlet-cabinet');
      const current = sel?.dataset.value || '';
      if (sel) {
        sel.innerHTML = `<option value="">${t('net_outlet.select_cabinet') || '选择机柜'}</option>` + opts;
        if (current) sel.value = current;
      }
    });
  }

  cabinetOptionsHtml() {
    return this.cabinets.map(c =>
      `<option value="${escapeHtml(c.id)}">${escapeHtml(c.name)}</option>`
    ).join('');
  }

  updateEmptyState() {
    if (!this.ensureContainer()) return;
    const existing = this.container.querySelector('.room-net-outlet-empty');
    const items = this.container.querySelectorAll('.room-net-outlet-item');
    if (items.length === 0 && !existing) {
      const emptyDiv = document.createElement('div');
      emptyDiv.className = 'room-net-outlet-empty';
      const addLabel = t('room.add_net_outlet') || t('net_outlet.add') || '添加信息点';
      emptyDiv.innerHTML = `<button type="button" class="btn btn-secondary btn-sm add-net-outlet-btn">${addLabel}</button>`;
      const addBtn = emptyDiv.querySelector('.add-net-outlet-btn');
      if (addBtn) {
        const handler = () => this.addItem();
        this.handlers.set(addBtn, handler);
        addBtn.addEventListener('click', handler);
      }
      this.container.appendChild(emptyDiv);
    } else if (items.length > 0 && existing) {
      existing.remove();
    }
  }

  outletTypeOptionsHtml() {
    const types = [
      { value: 'wall_socket', label: t('net_outlet.type_wall_socket') || '墙面插座' },
      { value: 'patch_panel', label: t('net_outlet.type_patch_panel') || '配线架' },
      { value: 'wifi_ap', label: t('net_outlet.type_wifi_ap') || '无线AP' },
      { value: 'other', label: t('net_outlet.type_other') || '其他' },
    ];
    return types.map(o => `<option value="${o.value}">${o.label}</option>`).join('');
  }

  createRow(data = {}) {
    const id = data.id || '';
    const name = data.name || '';
    const outletType = data.outlet_type || 'wall_socket';
    const cabinetId = data.cabinet_id || '';
    const description = data.description || '';
    const div = document.createElement('div');
    div.className = 'room-net-outlet-item';
    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <input type="hidden" class="net-outlet-id" value="${escapeHtml(id)}" />
          <input type="text" class="net-outlet-name form-control" value="${escapeHtml(name)}" placeholder="${t('net_outlet.name') || '信息点名称'}" autocomplete="off" />
        </div>
        <div class="form-group">
          <select class="net-outlet-type form-control">
            ${this.outletTypeOptionsHtml()}
          </select>
        </div>
        <div class="form-group">
          <select class="net-outlet-cabinet form-control" data-value="${escapeHtml(cabinetId)}">
            <option value="">${t('net_outlet.select_cabinet') || '选择机柜'}</option>
            ${this.cabinetOptionsHtml()}
          </select>
        </div>
        <div class="form-group">
          <input type="text" class="net-outlet-description form-control" value="${escapeHtml(description)}" placeholder="${t('common.description') || '描述'}" autocomplete="off" />
        </div>
        <div class="form-group room-net-outlets-actions">
          <button type="button" class="btn btn-danger btn-sm remove-net-outlet-btn">${t('common.delete') || '删除'}</button>
          <button type="button" class="btn btn-secondary btn-sm add-net-outlet-btn" style="display: none;">${t('room.add_net_outlet') || '添加信息点'}</button>
        </div>
      </div>
    `;
    // 设置类型和机柜的选中值
    const typeSel = div.querySelector('.net-outlet-type');
    if (typeSel) typeSel.value = outletType;
    const cabSel = div.querySelector('.net-outlet-cabinet');
    if (cabSel && cabinetId) cabSel.value = cabinetId;
    this.bindItemEvents(div);
    return div;
  }

  addItem(data = {}) {
    if (!this.ensureContainer()) return;
    const emptyState = this.container.querySelector('.room-net-outlet-empty');
    if (emptyState) emptyState.remove();
    const item = this.createRow(data);
    this.container.appendChild(item);
    this.updateAddButtons();
    this.updateEmptyState();
  }

  updateAddButtons() {
    if (!this.ensureContainer()) return;
    const items = this.container.querySelectorAll('.room-net-outlet-item');
    items.forEach((item, index) => {
      const addBtn = item.querySelector('.add-net-outlet-btn');
      if (addBtn) {
        addBtn.style.display = index === items.length - 1 ? '' : 'none';
        addBtn.textContent = t('room.add_net_outlet') || t('net_outlet.add') || '添加信息点';
      }
    });
  }

  bindItemEvents(item) {
    const removeBtn = item.querySelector('.remove-net-outlet-btn');
    if (removeBtn) {
      const handler = () => this.removeItem(item);
      this.handlers.set(removeBtn, handler);
      removeBtn.addEventListener('click', handler);
    }
    const addBtn = item.querySelector('.add-net-outlet-btn');
    if (addBtn) {
      const handler = () => this.addItem();
      this.handlers.set(addBtn, handler);
      addBtn.addEventListener('click', handler);
    }
  }

  removeItem(item) {
    item.remove();
    this.ensureContainer();
    this.updateEmptyState();
    this.updateAddButtons();
  }

  async loadExisting(room) {
    this.ensureContainer();
    if (!this.container) return;
    this.container.innerHTML = '';

    // 加载该房间的机柜选项（信息点行内的机柜下拉需要）
    if (room?.id) {
      await this.loadCabinets(room.id);
    }

    const netOutlets = room?.net_outlets || [];
    netOutlets.forEach(no => this.addItem(no));
    this.updateEmptyState();
    this.updateAddButtons();
  }

  collectData() {
    if (!this.ensureContainer()) return [];
    const items = this.container.querySelectorAll('.room-net-outlet-item');
    const netOutlets = [];
    for (const item of items) {
      const idInput = item.querySelector('.net-outlet-id');
      const nameInput = item.querySelector('.net-outlet-name');
      const typeSel = item.querySelector('.net-outlet-type');
      const cabSel = item.querySelector('.net-outlet-cabinet');
      const descInput = item.querySelector('.net-outlet-description');
      netOutlets.push({
        id: idInput?.value || null,
        name: (nameInput?.value || '').trim(),
        outlet_type: typeSel?.value || 'wall_socket',
        cabinet_id: cabSel?.value || null,
        description: (descInput?.value || '').trim() || null,
      });
    }
    return netOutlets;
  }
}

export const roomNetOutletsManager = new RoomNetOutletsManager();

// ==========================================
// 房间管理功能
// ==========================================

const tableState = createSortState('name', 'asc');
let currentPage = 1;

export async function loadRoomsData(page = 1, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);
  
  try {
    const roomsData = await apiGet(`/api/resources/rooms?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = roomsData.success ? roomsData.data : { items: [], total: 0 };
    const rooms = data.items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#rooms-table", {
      data: rooms,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'room_type', render: (v) => {
          const roomTypeLower = v ? v.toLowerCase() : '';
          if (roomTypeLower === "office") return t('room.type_office');
          if (roomTypeLower === "data_center") return t('room.type_datacenter');
          if (roomTypeLower === "telecom_closet") return t('room.type_telecom_closet');
          return v || '-';
        }},
        { field: 'org_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'networks', render: (v) => v && v.length > 0 ? v.map(n => `${n.name} (${n.network_region})`).join("<br>") : '-' },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'created_at', render: (v) => new Date(v).toLocaleString() },
        { field: 'id', render: (v, row) => {
          const isCabinetRoom = (row.room_type || '').toLowerCase() === 'data_center' || (row.room_type || '').toLowerCase() === 'telecom_closet';
          return `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-secondary btn-room-children-list" data-room-id="${v}" data-room-type="${escapeHtml(row.room_type || '')}">${isCabinetRoom ? (t('room.cabinets') || '机柜列表') : (t('room.workstations') || '工位列表')}</button>
          <button class="btn btn-sm btn-secondary btn-room-net-outlets-list" data-room-id="${v}">${t('room.net_outlets') || '信息点列表'}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t('common.delete')}</button>
        `;
        } }
      ],
      emptyMessage: t('common.no_data')
    });

    bindRoomButtonsEvents();

    if (data.total !== undefined) {
      appendPaginationToTable("#rooms-table", data, loadRoomsData);
    }
    updateSortIcons("rooms-table", tableState);
  } catch (error) {
    handleError(error, "加载房间数据失败", () => {
      renderTable("#rooms-table", { data: [], columns: [], emptyMessage: "加载失败，请刷新页面重试" });
    });
  }
}

let roomTableClickHandler = null;

function bindRoomButtonsEvents() {
  const table = elementCache.get("rooms-table");
  if (!table) return;

  if (roomTableClickHandler) {
    table.removeEventListener("click", roomTableClickHandler);
  }

  roomTableClickHandler = async (e) => {
    const target = e.target;
    if (target.classList.contains("btn-room-children-list")) {
      const roomId = target.dataset.roomId;
      if (roomId) {
        await openRoomChildrenListModal(roomId);
      }
    } else if (target.classList.contains("btn-room-net-outlets-list")) {
      const roomId = target.dataset.roomId;
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
      showToast(result.message || t('room.load_failed') || "加载房间数据失败", "error");
      return;
    }
    const room = result.data;
    const roomType = (room.room_type || "").toLowerCase();
    await openModal("room-children-list-modal");

    const titleEl = document.getElementById('room-children-list-modal-title');
    const extraTh = document.getElementById('room-children-list-extra-th');
    const tbody = document.getElementById('room-children-list-tbody');

    if (roomType === 'office') {
      if (titleEl) titleEl.textContent = `${room.name} - ${t('room.workstations') || '工位列表'}`;
      if (extraTh) extraTh.textContent = t('workstation.manager') || '负责人';
      const workstations = room.workstations || [];
      if (tbody) {
        if (workstations.length === 0) {
          tbody.innerHTML = `<tr class="empty-row"><td colspan="3" class="text-center">${t('common.no_data') || '暂无数据'}</td></tr>`;
        } else {
          tbody.innerHTML = workstations.map((ws, idx) => `
            <tr>
              <td>${idx + 1}</td>
              <td>${escapeHtml(ws.name || '')}</td>
              <td>${escapeHtml(ws.manager || '-')}</td>
            </tr>
          `).join('');
        }
      }
    } else if (roomType === 'data_center' || roomType === 'telecom_closet') {
      if (titleEl) titleEl.textContent = `${room.name} - ${t('room.cabinets') || '机柜列表'}`;
      if (extraTh) extraTh.textContent = t('cabinet.capacity') || '容量(U)';
      const cabinets = room.cabinets || [];
      if (tbody) {
        if (cabinets.length === 0) {
          tbody.innerHTML = `<tr class="empty-row"><td colspan="3" class="text-center">${t('common.no_data') || '暂无数据'}</td></tr>`;
        } else {
          tbody.innerHTML = cabinets.map((cab, idx) => `
            <tr>
              <td>${idx + 1}</td>
              <td>${escapeHtml(cab.name || '')}</td>
              <td>${cab.capacity ?? '-'}</td>
            </tr>
          `).join('');
        }
      }
    }
  } catch (error) {
    handleError(error, t('room.load_failed') || "加载房间数据失败");
  }
}

export async function openRoomNetOutletsListModal(roomId) {
  try {
    const result = await apiGet(`/api/resources/rooms/${roomId}`);
    if (!result.success || !result.data) {
      showToast(result.message || t('room.load_failed') || "加载房间数据失败", "error");
      return;
    }
    const room = result.data;
    await openModal("room-net-outlets-list-modal");

    const titleEl = document.getElementById('room-net-outlets-list-modal-title');
    const tbody = document.getElementById('room-net-outlets-list-tbody');

    if (titleEl) titleEl.textContent = `${room.name} - ${t('room.net_outlets') || '信息点列表'}`;

    const netOutlets = room.net_outlets || [];
    if (tbody) {
      if (netOutlets.length === 0) {
        tbody.innerHTML = `<tr class="empty-row"><td colspan="5" class="text-center">${t('common.no_data') || '暂无数据'}</td></tr>`;
      } else {
        tbody.innerHTML = netOutlets.map((no, idx) => `
          <tr>
            <td>${idx + 1}</td>
            <td>${escapeHtml(no.name || '')}</td>
            <td>${escapeHtml(outletTypeLabel(no.outlet_type))}</td>
            <td>${escapeHtml(no.cabinet_name || '-')}</td>
            <td>${escapeHtml(no.description || '-')}</td>
          </tr>
        `).join('');
      }
    }
  } catch (error) {
    handleError(error, t('room.load_failed') || "加载房间数据失败");
  }
}

function outletTypeLabel(type) {
  const map = {
    wall_socket: t('net_outlet.type_wall_socket') || '墙面插座',
    patch_panel: t('net_outlet.type_patch_panel') || '配线架',
    wifi_ap: t('net_outlet.type_wifi_ap') || '无线AP',
    other: t('net_outlet.type_other') || '其他',
  };
  return map[type] || type || '-';
}

export function initRoomSortEvents() {
  initSortEvents("rooms-table", tableState, loadRoomsData);
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

  // 收集并校验子项数据（工位/机柜）
  const childrenData = roomChildrenManager.collectData();
  const childrenError = validateRoomChildren(childrenData, roomType);
  if (childrenError) {
    showToast(childrenError, "warning");
    return;
  }

  let formattedRoomType;
  if (roomType === "office") {
    formattedRoomType = "OFFICE";
  } else if (roomType === "data_center") {
    formattedRoomType = "DATA_CENTER";
  } else if (roomType === "telecom_closet") {
    formattedRoomType = "TELECOM_CLOSET";
  } else {
    formattedRoomType = roomType;
  }

  const orgId = getElementValue("room-org-id");

  const roomData = {
    name: name.trim(),
    room_type: formattedRoomType,
    org_id: orgId || null,
    network_ids: networkIds,
    description: description || null,
  };

  try {
    let roomId = id;
    if (id) {
      const result = await apiPut(`/api/resources/rooms/${id}`, roomData);
      if (!result.success) {
        showToast(result.message || "房间保存失败", "error");
        return;
      }
    } else {
      const result = await apiPost("/api/resources/rooms", roomData);
      if (!result.success) {
        showToast(result.message || "房间保存失败", "error");
        return;
      }
      roomId = result.data?.id;
      if (!roomId) {
        showToast("房间创建成功但未返回ID", "error");
        return;
      }
      // 写回隐藏 ID，便于重试时按更新处理
      elementCache.setValue('room-id', roomId);
    }

    // 同步子项（工位/机柜）
    const syncResult = await apiPut(`/api/resources/rooms/${roomId}/children`, childrenData);
    if (!syncResult.success) {
      showToast(syncResult.message || t('room.children_save_failed'), "error");
      await loadRoomsData();
      return;
    }

    // 同步信息点（所有房型通用）
    const netOutletsData = roomNetOutletsManager.collectData();
    const noSyncResult = await apiPut(`/api/resources/rooms/${roomId}/net-outlets`, { net_outlets: netOutletsData });
    if (!noSyncResult.success) {
      showToast(noSyncResult.message || t('room.net_outlets_save_failed') || "信息点保存失败", "error");
      await loadRoomsData();
      return;
    }

    showToast("房间保存成功", "success");
    closeModal("room-modal");
    await loadRoomsData();
  } catch (error) {
    handleError(error, "房间保存失败");
  }
}

// 校验房间子项数据
function validateRoomChildren(childrenData, roomType) {
  if (roomType === "office") {
    const workstations = childrenData.workstations || [];
    for (const ws of workstations) {
      if (!ws.name) {
        return t('room.child_name_required');
      }
    }
  } else if (roomType === "data_center" || roomType === "telecom_closet") {
    const cabinets = childrenData.cabinets || [];
    for (const cab of cabinets) {
      if (!cab.name) {
        return t('room.child_name_required');
      }
      if (!Number.isInteger(cab.capacity) || cab.capacity < 1 || cab.capacity > 48) {
        return t('room.child_capacity_invalid');
      }
    }
  }
  return null;
}

// 房间管理模态框
export async function openRoomModal(room = null) {
  await openModal("room-modal");

  const title = elementCache.get('room-modal-title');
  const form = elementCache.get('room-form');

  await loadOrgsForSelect("room-org-id");

  if (room) {
    title.textContent = "编辑房间";
    elementCache.setValue('room-id', room.id);
    elementCache.setValue('room-name', room.name);
    elementCache.setValue('room-type', room.room_type ? room.room_type.toLowerCase() : "office");
    elementCache.setValue('room-org-id', room.org_id || "");
    elementCache.setValue('room-description', room.description || "");

    await loadRoomNetworks(room);

    // 初始化子项管理器并加载现有工位/机柜
    roomChildrenManager.roomType = (room.room_type || "OFFICE").toLowerCase();
    roomChildrenManager.loadExisting(room);

    // 初始化信息点管理器并加载现有信息点
    await roomNetOutletsManager.loadExisting(room);
  } else {
    title.textContent = "添加房间";
    if (form) form.reset();
    elementCache.setValue('room-id', '');
    await roomNetworkConfigManager.init();

    // 初始化子项管理器为默认空状态
    roomChildrenManager.roomType = 'office';
    roomChildrenManager.init();

    // 初始化信息点管理器为默认空状态
    roomNetOutletsManager.init();
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
