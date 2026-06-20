/**
 * 组织管理模块
 * 处理组织层级结构的树形展示和CRUD操作
 */

import { apiGet, apiPost, apiPut, apiDelete } from "../utils/apiClient.js";
import { showToast, handleError, escapeHtml, debounce } from "../utils/ui.js";
import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";
import { elementCache } from "../utils/helpers.js";

// ==========================================
// 常量定义
// ==========================================

const ORG_TYPE_ICONS = {
  headquarters: "🏢",
  building: "🏬",
  floor: "📐",
  hall: "🚪",
  office: "🏠",
  data_center: "🖥️",
  workstation: "💺",
  cabinet: "🗄️",
  cabinet_position: "📦",
};

function getOrgTypeLabel(orgType) {
  return t(`organization.types.${orgType}`) || orgType;
}

// 缓存组织类型结构
let orgSchema = null;
let allExpanded = false;

// ==========================================
// 初始化
// ==========================================

/**
 * 初始化组织管理功能
 */
export function initOrganization() {
  bindOrgEvents();
  loadOrganizationTree();
}

/**
 * 绑定组织管理事件
 */
function bindOrgEvents() {
  const container = document.getElementById("organization-tree-container");
  if (!container) return;

  if (container.dataset.initialized === "true") return;
  container.dataset.initialized = "true";

  container.addEventListener("click", handleTreeClick);

  const addRootBtn = document.getElementById("org-add-root-btn");
  addRootBtn?.addEventListener("click", () => openOrgModal(null, null));

  const expandAllBtn = document.getElementById("org-expand-all-btn");
  expandAllBtn?.addEventListener("click", expandAllNodes);

  const collapseAllBtn = document.getElementById("org-collapse-all-btn");
  collapseAllBtn?.addEventListener("click", collapseAllNodes);

  const searchInput = document.getElementById("org-search-input");
  if (searchInput) {
    searchInput.addEventListener("input", debounce(handleOrgSearch, 300));
  }
}

// ==========================================
// 数据加载
// ==========================================

/**
 * 加载组织树
 */
export async function loadOrganizationTree() {
  const container = document.getElementById("organization-tree-container");
  if (!container) return;

  try {
    const [treeResult, schemaResult] = await Promise.all([
      apiGet("/api/resources/organizations/tree"),
      getOrgSchema(),
    ]);

    if (schemaResult?.success && schemaResult.data?.schema) {
      orgSchema = schemaResult.data.schema;
    }

    if (treeResult.success && treeResult.data) {
      renderOrgTree(container, treeResult.data);
    } else {
      renderEmptyState(container);
    }
  } catch (error) {
    handleError(error, t("common.load_failed"));
    renderEmptyState(container);
  }
}

/**
 * 获取组织类型结构（带缓存）
 */
async function getOrgSchema() {
  if (orgSchema) {
    return { success: true, data: { schema: orgSchema } };
  }
  return apiGet("/api/resources/organizations/schema");
}

/**
 * 获取允许的下级类型
 */
async function getAllowedChildTypes(parentId) {
  const result = await apiGet(`/api/resources/organizations/${parentId}/allowed-child-types`);
  return result.success ? result.data : null;
}

// ==========================================
// 树形渲染
// ==========================================

/**
 * 渲染组织树
 */
function renderOrgTree(container, treeData) {
  if (!treeData || treeData.length === 0) {
    renderEmptyState(container);
    return;
  }

  container.innerHTML = "";
  const fragment = document.createDocumentFragment();

  treeData.forEach((node) => {
    fragment.appendChild(renderTreeNode(node, 0));
  });

  container.appendChild(fragment);
}

/**
 * 渲染单个树节点
 */
function renderTreeNode(node, depth) {
  const wrapper = document.createElement("div");
  wrapper.className = "org-node-wrapper";
  wrapper.dataset.nodeId = node.id;

  const nodeEl = document.createElement("div");
  nodeEl.className = "org-node";
  nodeEl.style.paddingLeft = `${depth * 24 + 16}px`;

  const hasChildren = node.children && node.children.length > 0;
  const icon = ORG_TYPE_ICONS[node.org_type] || "📁";
  const typeLabel = getOrgTypeLabel(node.org_type);

  const toggleBtn = hasChildren
    ? `<span class="org-toggle" data-action="toggle" role="button" tabindex="0" aria-label="展开/折叠">
         <svg class="org-toggle-icon" width="16" height="16" viewBox="0 0 16 16"><path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="2"/></svg>
       </span>`
    : `<span class="org-toggle-placeholder"></span>`;

  nodeEl.innerHTML = `
    ${toggleBtn}
    <span class="org-node-icon">${icon}</span>
    <span class="org-node-name">${escapeHtml(node.name)}</span>
    <span class="org-node-type-badge" data-type="${node.org_type}">${typeLabel}</span>
    ${node.description ? `<span class="org-node-desc" title="${escapeHtml(node.description)}">${escapeHtml(node.description)}</span>` : ""}
    <span class="org-node-actions">
      ${getAllowedChildButtons(node.org_type, node.id)}
      <button class="btn btn-sm btn-edit" data-action="edit" data-id="${node.id}" data-name="${escapeHtml(node.name)}" data-type="${node.org_type}">${t("common.edit")}</button>
      <button class="btn btn-sm btn-delete" data-action="delete" data-id="${node.id}" data-name="${escapeHtml(node.name)}">${t("common.delete")}</button>
    </span>
  `;

  wrapper.appendChild(nodeEl);

  if (hasChildren) {
    const childrenContainer = document.createElement("div");
    childrenContainer.className = "org-children";
    if (!allExpanded && depth >= 0) {
      childrenContainer.style.display = "none";
    }
    node.children.forEach((child) => {
      childrenContainer.appendChild(renderTreeNode(child, depth + 1));
    });
    wrapper.appendChild(childrenContainer);
  }

  return wrapper;
}

/**
 * 获取允许的下级类型按钮
 */
function getAllowedChildButtons(orgType, nodeId) {
  if (!orgSchema) return "";

  const typeInfo = orgSchema.find((s) => s.type === orgType);
  if (!typeInfo || !typeInfo.allowed_children || typeInfo.allowed_children.length === 0) {
    return "";
  }

  return typeInfo.allowed_children
    .map(
      (child) =>
        `<button class="btn btn-sm btn-add-child" data-action="add-child" data-parent-id="${nodeId}" data-child-type="${child.type}" title="${t("organization.add_child")}: ${child.label}">+${child.label}</button>`,
    )
    .join("");
}

/**
 * 渲染空状态
 */
function renderEmptyState(container) {
  container.innerHTML = `<div class="org-empty-state">${t("organization.empty")}</div>`;
}

// ==========================================
// 树形交互
// ==========================================

/**
 * 处理树点击事件（事件委托）
 */
function handleTreeClick(e) {
  const target = e.target.closest("[data-action]");
  if (!target) return;

  const action = target.dataset.action;

  switch (action) {
    case "toggle":
      e.stopPropagation();
      toggleNode(target);
      break;
    case "add-child":
      e.stopPropagation();
      openOrgModal(null, target.dataset.parentId, target.dataset.childType);
      break;
    case "edit":
      e.stopPropagation();
      editOrganization(target.dataset.id);
      break;
    case "delete":
      e.stopPropagation();
      deleteOrganization(target.dataset.id, target.dataset.name);
      break;
  }
}

/**
 * 展开/折叠节点
 */
function toggleNode(toggleEl) {
  const wrapper = toggleEl.closest(".org-node-wrapper");
  if (!wrapper) return;

  const children = wrapper.querySelector(":scope > .org-children");
  if (!children) return;

  const isHidden = children.style.display === "none";
  children.style.display = isHidden ? "" : "none";
  toggleEl.classList.toggle("expanded", isHidden);
}

/**
 * 展开所有节点
 */
function expandAllNodes() {
  allExpanded = true;
  document.querySelectorAll(".org-children").forEach((el) => {
    el.style.display = "";
  });
  document.querySelectorAll(".org-toggle").forEach((el) => {
    el.classList.add("expanded");
  });
}

/**
 * 折叠所有节点
 */
function collapseAllNodes() {
  allExpanded = false;
  document.querySelectorAll(".org-children").forEach((el) => {
    el.style.display = "none";
  });
  document.querySelectorAll(".org-toggle").forEach((el) => {
    el.classList.remove("expanded");
  });
}

/**
 * 搜索过滤
 */
function handleOrgSearch(e) {
  const keyword = e.target.value.trim().toLowerCase();
  const nodes = document.querySelectorAll(".org-node-wrapper");

  if (!keyword) {
    nodes.forEach((n) => {
      n.style.display = "";
    });
    return;
  }

  nodes.forEach((node) => {
    const name = node.querySelector(".org-node-name")?.textContent.toLowerCase() || "";
    if (name.includes(keyword)) {
      node.style.display = "";
      let parent = node.parentElement?.closest(".org-node-wrapper");
      while (parent) {
        parent.style.display = "";
        const children = parent.querySelector(":scope > .org-children");
        if (children) children.style.display = "";
        const toggle = parent.querySelector(":scope > .org-node .org-toggle");
        if (toggle) toggle.classList.add("expanded");
        parent = parent.parentElement?.closest(".org-node-wrapper");
      }
    } else {
      node.style.display = "none";
    }
  });
}

// ==========================================
// CRUD 操作
// ==========================================

/**
 * 打开组织模态框
 */
export async function openOrgModal(org = null, parentId = null, presetType = null) {
  await openModal("organization-modal");

  const modal = elementCache.get("organization-modal");
  const title = elementCache.get("organization-modal-title");
  const form = elementCache.get("organization-form");
  const typeSelect = elementCache.get("org-type");
  const parentInfo = document.getElementById("org-parent-info");

  form.reset();
  elementCache.setValue("org-id", "");

  if (org) {
    title.textContent = t("organization.edit");
    elementCache.setValue("org-id", org.id);
    elementCache.setValue("org-name", org.name);
    elementCache.setValue("org-type", org.org_type);
    elementCache.setValue("org-description", org.description || "");
    elementCache.setValue("org-parent-id", org.parent_id || "");
    populateTypeSelect(typeSelect, org.org_type, true);
    if (parentInfo) parentInfo.style.display = "none";
  } else if (parentId) {
    title.textContent = t("organization.add");
    elementCache.setValue("org-parent-id", parentId);

    try {
      const allowed = await getAllowedChildTypes(parentId);
      if (allowed && allowed.allowed_child_types) {
        populateTypeSelectWithOptions(typeSelect, allowed.allowed_child_types, presetType);
      }
      if (parentInfo) {
        parentInfo.style.display = "";
        document.getElementById("org-parent-name").textContent = allowed?.parent_name || "";
        document.getElementById("org-parent-type").textContent =
          getOrgTypeLabel(allowed?.parent_type) || allowed?.parent_type || "";
      }
    } catch (error) {
      handleError(error, t("common.operation_failed"));
    }
  } else {
    title.textContent = t("organization.add_headquarters");
    elementCache.setValue("org-parent-id", "");
    populateTypeSelect(typeSelect, "headquarters", true);
    if (parentInfo) parentInfo.style.display = "none";
  }
}

/**
 * 填充类型选择框（单选模式）
 */
function populateTypeSelect(select, currentValue, disabled) {
  select.innerHTML = "";
  const types = ["headquarters", "building", "floor", "hall", "office", "data_center", "workstation", "cabinet", "cabinet_position"];
  types.forEach((value) => {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = getOrgTypeLabel(value);
    if (value === currentValue) option.selected = true;
    select.appendChild(option);
  });
  select.disabled = disabled;
}

/**
 * 填充类型选择框（多选模式）
 */
function populateTypeSelectWithOptions(select, options, presetType) {
  select.innerHTML = `<option value="">${t("organization.select_type")}</option>`;
  options.forEach((opt) => {
    const option = document.createElement("option");
    option.value = opt.type;
    option.textContent = opt.label;
    if (presetType && opt.type === presetType) option.selected = true;
    select.appendChild(option);
  });
  select.disabled = false;
}

/**
 * 编辑组织节点
 */
export async function editOrganization(id) {
  try {
    const result = await apiGet(`/api/resources/organizations/${id}`);
    if (result.success) {
      openOrgModal(result.data);
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("common.operation_failed"));
  }
}

/**
 * 删除组织节点
 */
export async function deleteOrganization(id, name) {
  const confirmed = await import("../utils/confirm.js").then((m) =>
    m.showConfirm(t("organization.delete_confirm", { name })),
  );
  if (!confirmed) return;

  try {
    const result = await apiDelete(`/api/resources/organizations/${id}`);
    if (result.success) {
      showToast(t("organization.delete_success"), "success");
      loadOrganizationTree();
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("common.operation_failed"));
  }
}

/**
 * 提交组织表单
 */
export async function submitOrgForm() {
  const id = elementCache.getValue("org-id");
  const name = elementCache.getValue("org-name");
  const orgType = elementCache.getValue("org-type");
  const description = elementCache.getValue("org-description");
  const parentId = elementCache.getValue("org-parent-id");

  if (!name) {
    showToast(t("organization.name_required"), "warning");
    return;
  }

  if (!orgType) {
    showToast(t("organization.type_required"), "warning");
    return;
  }

  const orgData = {
    name: name.trim(),
    org_type: orgType,
    parent_id: parentId || null,
    description: description || null,
  };

  try {
    let result;
    if (id) {
      result = await apiPut(`/api/resources/organizations/${id}`, orgData);
    } else {
      result = await apiPost("/api/resources/organizations", orgData);
    }

    if (result.success) {
      showToast(id ? t("organization.update_success") : t("organization.create_success"), "success");
      closeModal("organization-modal");
      loadOrganizationTree();
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("common.operation_failed"));
  }
}
