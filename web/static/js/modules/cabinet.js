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
import { elementCache } from "../utils/helpers.js";
import { t } from "../utils/i18n.js";
import { iconButton } from "../utils/icons.js";
import { DynamicRowManager } from "../utils/dynamicRowManager.js";

import { loadDataCenterRoomsForSelect, loadRoomNetworksForCabinet } from "../utils/resources.js";

// 两个动态列表管理器（机位/配线架）共用的 init：确保容器、绑定底部
// "添加一行"按钮（无论容器是否找到），容器缺失时返回 false 交由调用方处理
function initDynamicListContainer(manager) {
  manager.ensureContainer();
  manager.bindExternalAddButton();
  if (!manager.container) {
    return false;
  }
  manager.container.innerHTML = "";
  manager.updateEmptyState();
  return true;
}

// ==========================================
// 机柜机位动态管理模块
// ==========================================

class CabinetPositionsManager extends DynamicRowManager {
  constructor() {
    super({
      containerId: "cabinet-positions-container",
      itemSelector: ".cabinet-position-item",
      emptyClassName: "cabinet-position-empty",
      removeBtnSelector: ".remove-position-btn",
      externalAddButtonId: "add-position-row-btn",
      emptyMode: "hint"
    });
  }

  emptyHintText() {
    return t("cabinet.no_positions_hint");
  }

  init() {
    return initDynamicListContainer(this);
  }

  createRow(data = {}) {
    const id = data.id || "";
    const name = data.name || "";
    const startU = data.start_u ?? 1;
    const endU = data.end_u ?? 1;
    const description = data.description || "";
    const div = document.createElement("div");
    div.className = "cabinet-position-item";
    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <input type="hidden" class="position-id" value="${escapeHtml(String(id))}" />
          <input type="text" class="position-name form-control" value="${escapeHtml(name)}" placeholder="${t("cabinet_position.name")}" autocomplete="off" />
        </div>
        <div class="form-group">
          <input type="number" class="position-start-u form-control" value="${startU}" min="1" max="48" placeholder="${t("cabinet_position.start_u")}" />
        </div>
        <div class="form-group">
          <input type="number" class="position-end-u form-control" value="${endU}" min="1" max="48" placeholder="${t("cabinet_position.end_u")}" />
        </div>
        <div class="form-group">
          <input type="text" class="position-description form-control" value="${escapeHtml(description)}" placeholder="${t("cabinet_position.description")}" autocomplete="off" />
        </div>
        <div class="form-group cabinet-item-actions">
          <button type="button" class="btn btn-danger btn-sm remove-position-btn">${t("common.delete")}</button>
        </div>
      </div>
    `;
    this.bindItemEvents(div);
    return div;
  }

  loadExisting(positions) {
    // closeModal 会移除模态框 DOM，需重新获取 container
    this.ensureContainer();
    // 无论 container 是否找到，都要绑定底部"添加机位"按钮
    this.bindExternalAddButton();

    if (!this.container) {
      return;
    }
    this.container.innerHTML = "";

    if (positions?.length) {
      positions.forEach((pos) => this.addItem(pos));
    }
    this.updateEmptyState();
  }

  collectData() {
    if (!this.ensureContainer()) {
      return { positions: [] };
    }
    const items = this.container.querySelectorAll(".cabinet-position-item");
    const positions = [];
    for (const item of items) {
      const idInput = item.querySelector(".position-id");
      const nameInput = item.querySelector(".position-name");
      const startUInput = item.querySelector(".position-start-u");
      const endUInput = item.querySelector(".position-end-u");
      const descInput = item.querySelector(".position-description");
      const idValue = idInput?.value?.trim();
      positions.push({
        id: idValue || null,
        name: (nameInput?.value || "").trim(),
        start_u: parseInt(startUInput?.value || "0", 10),
        end_u: parseInt(endUInput?.value || "0", 10),
        description: (descInput?.value || "").trim() || null
      });
    }
    return { positions };
  }
}

// ==========================================
// 机柜配线架动态管理模块（参照房间信息点逻辑）
// ==========================================

class CabinetPatchPanelsManager extends DynamicRowManager {
  constructor() {
    super({
      containerId: "cabinet-patch-panels-container",
      itemSelector: ".cabinet-patch-panel-item",
      emptyClassName: "cabinet-patch-panel-empty",
      removeBtnSelector: ".remove-patch-panel-btn",
      externalAddButtonId: "add-patch-panel-row-btn",
      emptyMode: "hint"
    });
  }

  emptyHintText() {
    return t("cabinet.no_patch_panels_hint");
  }

  init() {
    return initDynamicListContainer(this);
  }

  createRow(data = {}) {
    const id = data.id || "";
    const name = data.name || "";
    const div = document.createElement("div");
    div.className = "cabinet-patch-panel-item";
    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <input type="hidden" class="patch-panel-id" value="${escapeHtml(String(id))}" />
          <input type="text" class="patch-panel-name form-control" value="${escapeHtml(name)}" placeholder="${t("cabinet.patch_panel_name") || t("net_outlet.name")}" autocomplete="off" />
        </div>
        <div class="form-group cabinet-item-actions">
          <button type="button" class="btn btn-danger btn-sm remove-patch-panel-btn">${t("common.delete")}</button>
        </div>
      </div>
    `;
    this.bindItemEvents(div);
    return div;
  }

  loadExisting(patchPanels) {
    this.ensureContainer();
    this.bindExternalAddButton();
    if (!this.container) {
      return;
    }
    this.container.innerHTML = "";
    if (patchPanels?.length) {
      patchPanels.forEach((pp) => this.addItem(pp));
    }
    this.updateEmptyState();
  }

  collectData() {
    if (!this.ensureContainer()) {
      return [];
    }
    const items = this.container.querySelectorAll(".cabinet-patch-panel-item");
    const patchPanels = [];
    for (const item of items) {
      const idInput = item.querySelector(".patch-panel-id");
      const nameInput = item.querySelector(".patch-panel-name");
      const idValue = idInput?.value?.trim();
      patchPanels.push({
        id: idValue || null,
        name: (nameInput?.value || "").trim()
      });
    }
    return patchPanels;
  }
}

export const cabinetPositionsManager = new CabinetPositionsManager();
export const cabinetPatchPanelsManager = new CabinetPatchPanelsManager();

// ==========================================
// 机柜管理功能
// ==========================================

const tableState = createSortState("name", "asc");
let currentPage = 1;
let currentPageSize = DEFAULT_PAGE_SIZE;

// 房间选择事件监听器引用
let roomSelectHandler = null;

// 加载机柜数据
export async function loadCabinetsData(page = currentPage, sortBy = null, sortOrder = null) {
  currentPage = page;
  if (sortBy) {
    tableState.setSort(sortBy, sortOrder);
  }

  try {
    const result = await apiGet(
      `/api/resources/cabinets?page=${page}&page_size=${currentPageSize}&sort_by=${tableState.sortBy}&sort_order=${tableState.sortOrder}`
    );
    const data = result.success ? result.data : { items: [], total: 0 };
    const cabinets = data.items || data;
    const startIndex = (page - 1) * currentPageSize;

    renderTable("#cabinets-table", {
      data: cabinets,
      columns: [
        {
          field: "id",
          render: (v, row, index) => startIndex + index + 1,
          className: "index-column"
        },
        { field: "room_name", render: (v) => escapeHtml(v) || "-" },
        { field: "name", render: (v) => escapeHtml(v) },
        { field: "capacity", render: (v) => v ?? "-", className: "col-center" },
        { field: "position_count", render: (v) => v ?? 0, className: "col-center" },
        { field: "description", render: (v) => escapeHtml(v) || "-" },
        {
          field: "created_at",
          render: (v) => new Date(v).toLocaleString(),
          className: "col-center"
        },
        {
          field: "id",
          render: (v) => `
          ${iconButton({ icon: "edit", label: t("common.edit"), cls: "btn-edit", attrs: `data-id="${v}"` })}
          ${iconButton({ icon: "list", label: t("cabinet.positions_list"), cls: "btn-primary btn-cabinet-positions-list", attrs: `data-cabinet-id="${v}"` })}
          ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-delete", attrs: `data-id="${v}"` })}
        `
        }
      ],
      emptyMessage: t("common.no_data")
    });

    bindCabinetButtonsEvents();

    if (data.total !== undefined) {
      appendPaginationToTable("#cabinets-table", data, loadCabinetsData, {
        pageSize: currentPageSize,
        onPageSizeChange: (size) => {
          currentPageSize = size;
          loadCabinetsData(1);
        }
      });
    }
    updateSortIcons("cabinets-table", tableState);
  } catch (error) {
    handleError(error, t("cabinet.load_failed"), () => {
      renderTable("#cabinets-table", {
        data: [],
        columns: [],
        emptyMessage: t("common.load_failed")
      });
    });
  }
}

let cabinetTableClickHandler = null;

function bindCabinetButtonsEvents() {
  const table = elementCache.get("cabinets-table");
  if (!table) {
    return;
  }

  if (cabinetTableClickHandler) {
    table.removeEventListener("click", cabinetTableClickHandler);
  }

  cabinetTableClickHandler = async (e) => {
    const positionsBtn = e.target.closest(".btn-cabinet-positions-list");
    if (positionsBtn) {
      const cabinetId = positionsBtn.dataset.cabinetId;
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
      showToast(result.message || t("cabinet.load_failed"), "error");
      return;
    }
    const cabinet = result.data;
    const positions = cabinet.positions || [];
    await openSimpleListModal({
      title: `${cabinet.name} - ${t("cabinet.positions_list")}`,
      columns: [
        { label: t("common.name") },
        { label: t("cabinet_position.start_u") },
        { label: t("cabinet_position.end_u") },
        { label: t("common.description") }
      ],
      rows: positions.map((pos) => [
        escapeHtml(pos.name || "-"),
        pos.start_u ?? "-",
        pos.end_u ?? "-",
        escapeHtml(pos.description || "-")
      ])
    });
  } catch (error) {
    handleError(error, t("cabinet.load_failed"));
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
      showToast(`${t("cabinet.load_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("cabinet.load_failed"));
  }
}

// 删除机柜
export async function deleteCabinet(id) {
  await handleDelete(id, "/api/resources/cabinets", t("cabinet.delete_success"), loadCabinetsData);
}

// ====== 机柜管理模态框 ======
export async function openCabinetModal(cabinet = null) {
  await openModal("cabinet-modal");

  const title = elementCache.get("cabinet-modal-title");
  const form = elementCache.get("cabinet-form");
  const capacityInput = elementCache.get("cabinet-capacity");

  // 加载机房选项
  await loadDataCenterRoomsForSelect();

  // 获取房间选择框
  const roomSelect = elementCache.get("cabinet-room");

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
    title.textContent = t("cabinet.edit_cabinet");
    elementCache.setValue("cabinet-id", cabinet.id);
    elementCache.setValue("cabinet-name", cabinet.name);
    if (cabinet.room_id) {
      elementCache.setValue("cabinet-room", cabinet.room_id);
      await loadRoomNetworksForCabinet(cabinet.room_id);
    }
    if (capacityInput) {
      capacityInput.value = cabinet.capacity || 42;
    }
    elementCache.setValue("cabinet-description", cabinet.description || "");

    // 加载现有机位
    cabinetPositionsManager.loadExisting(cabinet.positions || []);
    // 加载现有配线架
    cabinetPatchPanelsManager.loadExisting(cabinet.patch_panels || []);
  } else {
    // 添加模式
    title.textContent = t("cabinet.add_cabinet");
    if (form) {
      form.reset();
    }
    elementCache.setValue("cabinet-id", "");
    const inheritedNetworksContainer = elementCache.get("cabinet-inherited-networks");
    if (inheritedNetworksContainer) {
      inheritedNetworksContainer.innerHTML = `<p class="text-muted">${t("cabinet.inherited_networks_hint")}</p>`;
    }
    // 初始化机位管理器为默认空状态
    cabinetPositionsManager.init();
    // 初始化配线架管理器为默认空状态
    cabinetPatchPanelsManager.init();
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
    showToast(t("cabinet.name_required"), "warning");
    return;
  }

  if (!roomId) {
    showToast(t("cabinet.select_room_first"), "warning");
    return;
  }

  if (isNaN(capacity) || capacity <= 0) {
    showToast(t("cabinet.capacity_invalid"), "warning");
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
    description: description.trim() || null
  };

  try {
    let cabinetId = id;
    if (id) {
      const result = await apiPut(`/api/resources/cabinets/${id}`, cabinetData);
      if (!result.success) {
        showToast(result.message || t("cabinet.save_failed"), "error");
        return;
      }
    } else {
      const result = await apiPost("/api/resources/cabinets", cabinetData);
      if (!result.success) {
        showToast(result.message || t("cabinet.save_failed"), "error");
        return;
      }
      cabinetId = result.data?.id;
      if (!cabinetId) {
        showToast(t("cabinet.save_failed"), "error");
        return;
      }
      elementCache.setValue("cabinet-id", cabinetId);
    }

    // 同步机位
    const syncResult = await apiPut(
      `/api/resources/cabinets/${cabinetId}/positions`,
      positionsData
    );
    if (!syncResult.success) {
      showToast(syncResult.message || t("cabinet.positions_save_failed"), "error");
      await loadCabinetsData();
      return;
    }

    // 同步配线架
    const patchPanelsData = cabinetPatchPanelsManager.collectData();
    const ppResult = await apiPut(`/api/resources/cabinets/${cabinetId}/patch-panels`, {
      patch_panels: patchPanelsData
    });
    if (!ppResult.success) {
      showToast(
        ppResult.message || t("cabinet.patch_panels_save_failed") || t("cabinet.save_failed"),
        "error"
      );
      await loadCabinetsData();
      return;
    }

    showToast(t("cabinet.save_success"), "success");
    closeModal("cabinet-modal");
    await loadCabinetsData();
  } catch (error) {
    handleError(error, t("cabinet.save_failed"));
  }
}

// 校验机位数据
function validatePositions(positions) {
  if (!positions || positions.length === 0) {
    return null;
  }
  for (const pos of positions) {
    if (!pos.name) {
      return t("cabinet.position_name_required");
    }
    if (
      !Number.isInteger(pos.start_u) ||
      pos.start_u < 1 ||
      pos.start_u > 48 ||
      !Number.isInteger(pos.end_u) ||
      pos.end_u < 1 ||
      pos.end_u > 48 ||
      pos.end_u < pos.start_u
    ) {
      return t("cabinet.position_u_invalid");
    }
  }
  return null;
}
