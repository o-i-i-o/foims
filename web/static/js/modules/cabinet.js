
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
import { elementCache } from "../utils/helpers.js";
import { t } from "../utils/i18n.js";

import {
  loadDataCenterRoomsForSelect,
  loadRoomNetworksForCabinet,
} from "../utils/resources.js";

// ==========================================
// 机柜机位动态管理模块
// ==========================================

class CabinetPositionsManager {
  constructor() {
    this.container = null;
    this.handlers = new WeakMap();
    this.addHandler = null;
  }

  ensureContainer() {
    if (!this.container || !document.contains(this.container)) {
      this.container = document.getElementById('cabinet-positions-container');
    }
    return this.container;
  }

  init() {
    this.ensureContainer();
    // 无论 container 是否找到，都要绑定底部"添加机位"按钮
    this.bindAddButton();

    if (!this.container) return false;

    this.container.innerHTML = '';
    this.updateEmptyState();
    return true;
  }

  bindAddButton() {
    const addBtn = document.getElementById('add-position-row-btn');
    if (!addBtn) return;
    if (this.addHandler) {
      addBtn.removeEventListener('click', this.addHandler);
    }
    this.addHandler = () => this.addItem();
    addBtn.addEventListener('click', this.addHandler);
  }

  updateEmptyState() {
    if (!this.ensureContainer()) return;
    const existing = this.container.querySelector('.cabinet-position-empty');
    const items = this.container.querySelectorAll('.cabinet-position-item');
    if (items.length === 0 && !existing) {
      const emptyDiv = document.createElement('div');
      emptyDiv.className = 'cabinet-position-empty text-muted';
      emptyDiv.textContent = t('cabinet.no_positions_hint') || '暂无机位，点击下方按钮添加';
      this.container.appendChild(emptyDiv);
    } else if (items.length > 0 && existing) {
      existing.remove();
    }
  }

  createRow(data = {}) {
    const id = data.id || '';
    const name = data.name || '';
    const startU = data.start_u ?? 1;
    const endU = data.end_u ?? 1;
    const description = data.description || '';
    const div = document.createElement('div');
    div.className = 'cabinet-position-item';
    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <input type="hidden" class="position-id" value="${escapeHtml(String(id))}" />
          <input type="text" class="position-name form-control" value="${escapeHtml(name)}" placeholder="${t('cabinet_position.name')}" autocomplete="off" />
        </div>
        <div class="form-group">
          <input type="number" class="position-start-u form-control" value="${startU}" min="1" max="48" placeholder="${t('cabinet_position.start_u')}" />
        </div>
        <div class="form-group">
          <input type="number" class="position-end-u form-control" value="${endU}" min="1" max="48" placeholder="${t('cabinet_position.end_u')}" />
        </div>
        <div class="form-group">
          <input type="text" class="position-description form-control" value="${escapeHtml(description)}" placeholder="${t('cabinet_position.description')}" autocomplete="off" />
        </div>
        <div class="form-group">
          <button type="button" class="btn btn-danger btn-sm remove-position-btn">${t('common.delete')}</button>
        </div>
      </div>
    `;
    this.bindItemEvents(div);
    return div;
  }

  addItem(data = {}) {
    if (!this.ensureContainer()) return;
    // 移除空状态提示
    const emptyState = this.container.querySelector('.cabinet-position-empty');
    if (emptyState) emptyState.remove();
    const item = this.createRow(data);
    this.container.appendChild(item);
  }

  bindItemEvents(item) {
    const removeBtn = item.querySelector('.remove-position-btn');
    if (removeBtn) {
      const handler = () => this.removeItem(item);
      this.handlers.set(removeBtn, handler);
      removeBtn.addEventListener('click', handler);
    }
  }

  removeItem(item) {
    item.remove();
    this.updateEmptyState();
  }

  loadExisting(positions) {
    // closeModal 会移除模态框 DOM，需重新获取 container
    this.ensureContainer();
    // 无论 container 是否找到，都要绑定底部"添加机位"按钮
    this.bindAddButton();

    if (!this.container) return;
    this.container.innerHTML = '';

    if (positions?.length) {
      positions.forEach(pos => this.addItem(pos));
    }
    this.updateEmptyState();
  }

  collectData() {
    if (!this.ensureContainer()) return { positions: [] };
    const items = this.container.querySelectorAll('.cabinet-position-item');
    const positions = [];
    for (const item of items) {
      const idInput = item.querySelector('.position-id');
      const nameInput = item.querySelector('.position-name');
      const startUInput = item.querySelector('.position-start-u');
      const endUInput = item.querySelector('.position-end-u');
      const descInput = item.querySelector('.position-description');
      const idValue = idInput?.value?.trim();
      positions.push({
        id: idValue || null,
        name: (nameInput?.value || '').trim(),
        start_u: parseInt(startUInput?.value || '0', 10),
        end_u: parseInt(endUInput?.value || '0', 10),
        description: (descInput?.value || '').trim() || null,
      });
    }
    return { positions };
  }
}

export const cabinetPositionsManager = new CabinetPositionsManager();

// ==========================================
// 机柜管理功能
// ==========================================

const tableState = createSortState('name', 'asc');
let currentPage = 1;

// 房间选择事件监听器引用
let roomSelectHandler = null;

// 加载机柜数据
export async function loadCabinetsData(page = 1, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) tableState.setSort(sortBy, sortOrder);

  try {
    const result = await apiGet(`/api/resources/cabinets?page=${page}&page_size=${DEFAULT_PAGE_SIZE}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const cabinets = data.items || data;
    const startIndex = (page - 1) * DEFAULT_PAGE_SIZE;

    renderTable("#cabinets-table", {
      data: cabinets,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'room_name', render: (v) => escapeHtml(v) || '-' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'capacity', render: (v) => v ?? '-' },
        { field: 'position_count', render: (v) => v ?? 0 },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'created_at', render: (v) => new Date(v).toLocaleString() },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">${t('common.edit')}</button>
          <button class="btn btn-sm btn-secondary btn-cabinet-positions-list" data-cabinet-id="${v}">${t('cabinet.positions_list') || '列表'}</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">${t('common.delete')}</button>
        ` }
      ],
      emptyMessage: t('common.no_data')
    });

    bindCabinetButtonsEvents();

    if (data.total !== undefined) {
      appendPaginationToTable("#cabinets-table", data, loadCabinetsData);
    }
    updateSortIcons("cabinets-table", tableState);
  } catch (error) {
    handleError(error, t('cabinet.load_failed'), () => {
      renderTable("#cabinets-table", { data: [], columns: [], emptyMessage: t('common.load_failed') });
    });
  }
}

let cabinetTableClickHandler = null;

function bindCabinetButtonsEvents() {
  const table = elementCache.get("cabinets-table");
  if (!table) return;

  if (cabinetTableClickHandler) {
    table.removeEventListener("click", cabinetTableClickHandler);
  }

  cabinetTableClickHandler = async (e) => {
    const target = e.target;
    if (target.classList.contains("btn-cabinet-positions-list")) {
      const cabinetId = target.dataset.cabinetId;
      if (cabinetId) {
        await openCabinetPositionsListModal(cabinetId);
      }
    }
  };

  table.addEventListener("click", cabinetTableClickHandler);
}

export async function openCabinetPositionsListModal(cabinetId) {
  try {
    const result = await apiGet(`/api/resources/cabinets/${cabinetId}`);
    if (!result.success || !result.data) {
      showToast(result.message || t('cabinet.load_failed') || "加载机柜数据失败", "error");
      return;
    }
    const cabinet = result.data;
    await openModal("cabinet-positions-list-modal");

    const titleEl = document.getElementById('cabinet-positions-list-modal-title');
    const tbody = document.getElementById('cabinet-positions-list-tbody');

    if (titleEl) titleEl.textContent = `${cabinet.name} - ${t('cabinet.positions_list') || '机位列表'}`;

    const positions = cabinet.positions || [];
    if (tbody) {
      if (positions.length === 0) {
        tbody.innerHTML = `<tr class="empty-row"><td colspan="5" class="text-center">${t('common.no_data') || '暂无数据'}</td></tr>`;
      } else {
        tbody.innerHTML = positions.map((pos, idx) => `
          <tr>
            <td>${idx + 1}</td>
            <td>${escapeHtml(pos.name || '')}</td>
            <td>${pos.start_u ?? '-'}</td>
            <td>${pos.end_u ?? '-'}</td>
            <td>${escapeHtml(pos.description || '-')}</td>
          </tr>
        `).join('');
      }
    }
  } catch (error) {
    handleError(error, t('cabinet.load_failed') || "加载机柜数据失败");
  }
}

export function initCabinetSortEvents() {
  initSortEvents("cabinets-table", tableState, loadCabinetsData);
}

// 编辑机柜
export async function editCabinet(id) {
  try {
    const result = await apiGet(`/api/resources/cabinets/${id}`);
    if (result.success) {
      openCabinetModal(result.data);
    } else {
      showToast(`${t('cabinet.load_failed')}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t('cabinet.load_failed'));
  }
}

// 删除机柜
export async function deleteCabinet(id) {
  await handleDelete(id, "/api/resources/cabinets", t('cabinet.delete_success'), loadCabinetsData);
}

// ====== 机柜管理模态框 ======
export async function openCabinetModal(cabinet = null) {
  await openModal("cabinet-modal");

  const title = elementCache.get('cabinet-modal-title');
  const form = elementCache.get('cabinet-form');
  const capacityInput = elementCache.get('cabinet-capacity');

  // 加载机房选项
  await loadDataCenterRoomsForSelect();

  // 获取房间选择框
  const roomSelect = elementCache.get('cabinet-room');

  // 移除旧的事件监听器
  if (roomSelectHandler) {
    roomSelect.removeEventListener("change", roomSelectHandler);
  }

  // 创建新的事件监听器
  roomSelectHandler = async (event) => {
    const roomId = event.target.value;
    await loadRoomNetworksForCabinet(roomId);
  };

  if (roomSelect) {
    roomSelect.addEventListener("change", roomSelectHandler);
  }

  if (cabinet) {
    // 编辑模式
    title.textContent = t('cabinet.edit_cabinet');
    elementCache.setValue('cabinet-id', cabinet.id);
    elementCache.setValue('cabinet-name', cabinet.name);
    if (cabinet.room_id) {
      elementCache.setValue('cabinet-room', cabinet.room_id);
      await loadRoomNetworksForCabinet(cabinet.room_id);
    }
    if (capacityInput) capacityInput.value = cabinet.capacity || 42;
    elementCache.setValue('cabinet-description', cabinet.description || "");

    // 加载现有机位
    cabinetPositionsManager.loadExisting(cabinet.positions || []);
  } else {
    // 添加模式
    title.textContent = t('cabinet.add_cabinet');
    if (form) form.reset();
    elementCache.setValue('cabinet-id', '');
    const inheritedNetworksContainer = elementCache.get('cabinet-inherited-networks');
    if (inheritedNetworksContainer) {
      inheritedNetworksContainer.innerHTML = `<p class="text-muted">${t('cabinet.inherited_networks_hint')}</p>`;
    }
    // 初始化机位管理器为默认空状态
    cabinetPositionsManager.init();
  }
}

// 提交机柜表单
export async function submitCabinetForm() {
  const id = getElementValue("cabinet-id");
  const name = getElementValue("cabinet-name");
  const roomId = getElementValue("cabinet-room");
  const capacityStr = getElementValue("cabinet-capacity");
  const capacity = parseInt(capacityStr, 10);
  const description = getElementValue("cabinet-description");

  if (!name.trim()) {
    showToast(t('cabinet.name_required'), "warning");
    return;
  }

  if (!roomId) {
    showToast(t('cabinet.select_room_first'), "warning");
    return;
  }

  if (isNaN(capacity) || capacity <= 0) {
    showToast(t('cabinet.capacity_invalid'), "warning");
    return;
  }

  // 收集并校验机位数据
  const positionsData = cabinetPositionsManager.collectData();
  const positionsError = validatePositions(positionsData.positions);
  if (positionsError) {
    showToast(positionsError, "warning");
    return;
  }

  const cabinetData = {
    name: name.trim(),
    room_id: roomId,
    capacity,
    description: description.trim() || null,
  };

  try {
    let cabinetId = id;
    if (id) {
      const result = await apiPut(`/api/resources/cabinets/${id}`, cabinetData);
      if (!result.success) {
        showToast(result.message || t('cabinet.save_failed'), "error");
        return;
      }
    } else {
      const result = await apiPost("/api/resources/cabinets", cabinetData);
      if (!result.success) {
        showToast(result.message || t('cabinet.save_failed'), "error");
        return;
      }
      cabinetId = result.data?.id;
      if (!cabinetId) {
        showToast(t('cabinet.save_failed'), "error");
        return;
      }
      elementCache.setValue('cabinet-id', cabinetId);
    }

    // 同步机位
    const syncResult = await apiPut(`/api/resources/cabinets/${cabinetId}/positions`, positionsData);
    if (!syncResult.success) {
      showToast(syncResult.message || t('cabinet.positions_save_failed'), "error");
      await loadCabinetsData();
      return;
    }

    showToast(t('cabinet.save_success'), "success");
    closeModal("cabinet-modal");
    await loadCabinetsData();
  } catch (error) {
    handleError(error, t('cabinet.save_failed'));
  }
}

// 校验机位数据
function validatePositions(positions) {
  if (!positions || positions.length === 0) {
    return null;
  }
  for (const pos of positions) {
    if (!pos.name) {
      return t('cabinet.position_name_required');
    }
    if (!Number.isInteger(pos.start_u) || pos.start_u < 1 || pos.start_u > 48 ||
        !Number.isInteger(pos.end_u) || pos.end_u < 1 || pos.end_u > 48 ||
        pos.end_u < pos.start_u) {
      return t('cabinet.position_u_invalid');
    }
  }
  return null;
}
