/**
 * 组织管理模块
 * 处理组织层级结构的树形展示和CRUD操作
 * 基于模板的层级管理：模板定义层次链，每个根节点关联一个模板
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

const ORG_TYPES = [
  "headquarters",
  "building",
  "floor",
  "hall",
  "office",
  "data_center",
  "workstation",
  "cabinet",
  "cabinet_position",
];

// 快捷填充预设模板（映射格式：类型→允许的子级类型）
const QUICK_FILL_PRESETS = [
  { name: "综合楼", levels: { headquarters: ["building"], building: ["floor"], floor: ["hall", "office", "data_center"], hall: [], office: [], data_center: [] } },
  { name: "办公楼", levels: { headquarters: ["building"], building: ["floor"], floor: ["hall", "office"], hall: [], office: [] } },
  { name: "数据中心", levels: { headquarters: ["data_center"], data_center: ["cabinet"], cabinet: ["cabinet_position"], cabinet_position: [] } },
];

function getOrgTypeLabel(orgType) {
  const key = `organization.types.${orgType}`;
  const translated = t(key);
  // t() 找不到 key 时会返回完整 key 路径，此时应返回原始值
  return translated === key ? orgType : translated;
}

/** 从 levels 映射中找到根类型（不出现在任何子级列表中的类型） */
function findRootType(levels) {
  if (!levels || typeof levels !== "object" || Array.isArray(levels)) return null;
  const allChildren = new Set();
  Object.values(levels).forEach((children) => {
    if (Array.isArray(children)) children.forEach((c) => allChildren.add(c));
  });
  const roots = Object.keys(levels).filter((k) => !allChildren.has(k));
  return roots.length === 1 ? roots[0] : null;
}

/** 将 levels 映射渲染为可读文本，多路径用 separator 分隔 */
function renderLevelsMapping(levels, separator = "<br>") {
  if (Array.isArray(levels)) {
    return levels.map((l) => getOrgTypeLabel(l)).join(" → ");
  }
  const rootType = findRootType(levels);
  if (!rootType) return "";
  const paths = [];
  function findPaths(type, currentPath) {
    const children = levels[type] || [];
    if (children.length === 0) {
      paths.push([...currentPath, type]);
    } else {
      children.forEach((child) => findPaths(child, [...currentPath, type]));
    }
  }
  findPaths(rootType, []);
  return paths.map((p) => p.map((t) => getOrgTypeLabel(t)).join(" → ")).join(separator);
}

let allExpanded = false;

// ==========================================
// 初始化
// ==========================================

export function initOrganization() {
  bindOrgEvents();
  loadOrganizationTree();
}

function bindOrgEvents() {
  const container = document.getElementById("organization-tree-container");
  if (!container) return;

  if (container.dataset.initialized === "true") return;
  container.dataset.initialized = "true";

  container.addEventListener("click", handleTreeClick);

  const addRootBtn = document.getElementById("org-add-root-btn");
  addRootBtn?.addEventListener("click", () => openOrgModal(null, null, null, true));

  const expandAllBtn = document.getElementById("org-expand-all-btn");
  expandAllBtn?.addEventListener("click", expandAllNodes);

  const collapseAllBtn = document.getElementById("org-collapse-all-btn");
  collapseAllBtn?.addEventListener("click", collapseAllNodes);

  const templateMgmtBtn = document.getElementById("org-template-mgmt-btn");
  templateMgmtBtn?.addEventListener("click", openTemplateManagement);

  const searchInput = document.getElementById("org-search-input");
  if (searchInput) {
    searchInput.addEventListener("input", debounce(handleOrgSearch, 300));
  }
}

// ==========================================
// 数据加载
// ==========================================

export async function loadOrganizationTree() {
  const container = document.getElementById("organization-tree-container");
  if (!container) return;

  try {
    const result = await apiGet("/api/resources/organizations/tree");

    if (result.success && result.data) {
      renderOrgTree(container, result.data);
    } else {
      renderEmptyState(container);
    }
  } catch (error) {
    handleError(error, t("common.load_failed"));
    renderEmptyState(container);
  }
}

async function getAllowedChildTypes(parentId) {
  const result = await apiGet(`/api/resources/organizations/${parentId}/allowed-child-types`);
  return result.success ? result.data : null;
}

async function getTemplates() {
  const result = await apiGet("/api/resources/org-templates");
  return result.success && result.data ? result.data.items : [];
}

// ==========================================
// 树形渲染
// ==========================================

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

  const addChildBtn = getChildButton(node);

  nodeEl.innerHTML = `
    ${toggleBtn}
    <span class="org-node-icon">${icon}</span>
    <span class="org-node-name">${escapeHtml(node.name)}</span>
    <span class="org-node-type-badge" data-type="${node.org_type}">${typeLabel}</span>
    ${node.description ? `<span class="org-node-desc" title="${escapeHtml(node.description)}">${escapeHtml(node.description)}</span>` : ""}
    <span class="org-node-actions">
      ${addChildBtn}
      <button class="btn btn-sm btn-edit" data-action="edit" data-id="${node.id}" data-name="${escapeHtml(node.name)}">${t("common.edit")}</button>
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
 * 获取"新增下级"按钮 - 基于模板层级
 * 始终显示按钮，由后端验证是否允许添加下级
 */
function getChildButton(node) {
  return `<button class="btn btn-sm btn-add-child" data-action="add-child" data-parent-id="${node.id}" title="${t("organization.add_child")}">${t("organization.add_child")}</button>`;
}

function renderEmptyState(container) {
  container.innerHTML = `<div class="org-empty-state">${t("organization.empty")}</div>`;
}

// ==========================================
// 树形交互
// ==========================================

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
      openOrgModal(null, target.dataset.parentId, null, false);
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

function toggleNode(toggleEl) {
  const wrapper = toggleEl.closest(".org-node-wrapper");
  if (!wrapper) return;

  const children = wrapper.querySelector(":scope > .org-children");
  if (!children) return;

  const isHidden = children.style.display === "none";
  children.style.display = isHidden ? "" : "none";
  toggleEl.classList.toggle("expanded", isHidden);
}

function expandAllNodes() {
  allExpanded = true;
  document.querySelectorAll(".org-children").forEach((el) => {
    el.style.display = "";
  });
  document.querySelectorAll(".org-toggle").forEach((el) => {
    el.classList.add("expanded");
  });
}

function collapseAllNodes() {
  allExpanded = false;
  document.querySelectorAll(".org-children").forEach((el) => {
    el.style.display = "none";
  });
  document.querySelectorAll(".org-toggle").forEach((el) => {
    el.classList.remove("expanded");
  });
}

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
// 组织节点 CRUD
// ==========================================

/**
 * 打开组织节点模态框
 * @param {Object|null} org - 编辑时传入现有节点
 * @param {string|null} parentId - 新增子节点时传入父节点ID
 * @param {string|null} presetType - 预设类型（不再使用，类型由模板决定）
 * @param {boolean} isRoot - 是否为新增根节点
 */
export async function openOrgModal(org = null, parentId = null, presetType = null, isRoot = false) {
  await openModal("organization-modal");

  const modal = elementCache.get("organization-modal");
  const title = elementCache.get("organization-modal-title");
  const form = elementCache.get("organization-form");
  const typeSelect = elementCache.get("org-type");
  const parentInfo = document.getElementById("org-parent-info");
  const templateSelectContainer = document.getElementById("org-template-select-container");

  form.reset();
  elementCache.setValue("org-id", "");

  if (org) {
    // 编辑模式
    title.textContent = t("organization.edit");
    elementCache.setValue("org-id", org.id);
    elementCache.setValue("org-name", org.name);
    elementCache.setValue("org-type", org.org_type);
    elementCache.setValue("org-description", org.description || "");
    elementCache.setValue("org-parent-id", org.parent_id || "");
    populateTypeSelect(typeSelect, org.org_type, true);
    if (parentInfo) parentInfo.style.display = "none";
    if (templateSelectContainer) templateSelectContainer.style.display = "none";
  } else if (parentId) {
    // 新增子节点 - 类型由模板决定
    title.textContent = t("organization.add");
    elementCache.setValue("org-parent-id", parentId);

    try {
      const allowed = await getAllowedChildTypes(parentId);
      if (allowed && allowed.allowed_child_types && allowed.allowed_child_types.length > 0) {
        const allowedTypes = allowed.allowed_child_types.map((ct) => ct.type);
        populateTypeSelect(typeSelect, allowedTypes[0], allowedTypes.length === 1, allowedTypes);
      } else {
        typeSelect.innerHTML = `<option value="">${t("organization.no_child_allowed")}</option>`;
        typeSelect.disabled = true;
      }
      if (parentInfo) {
        parentInfo.style.display = "";
        document.getElementById("org-parent-name").textContent = allowed?.parent_name || "";
        document.getElementById("org-parent-type").textContent =
          getOrgTypeLabel(allowed?.parent_type) || allowed?.parent_type || "";
      }
      if (templateSelectContainer) templateSelectContainer.style.display = "none";
    } catch (error) {
      handleError(error, t("common.operation_failed"));
    }
  } else if (isRoot) {
    // 新增根节点 - 需要选择模板
    title.textContent = t("organization.add_node");
    elementCache.setValue("org-parent-id", "");
    if (parentInfo) parentInfo.style.display = "none";

    // 显示模板选择
    if (templateSelectContainer) {
      templateSelectContainer.style.display = "";
      const templateSelect = document.getElementById("org-template-select");
      if (templateSelect) {
        templateSelect.innerHTML = `<option value="">${t("org_template.select_template")}</option>`;
        try {
          const templates = await getTemplates();
          templates.forEach((tpl) => {
            const option = document.createElement("option");
            option.value = tpl.id;
            const levelLabels = renderLevelsMapping(tpl.levels, " / ");
            option.textContent = `${tpl.name} (${levelLabels})`;
            templateSelect.appendChild(option);
          });

          // 模板选择变化时，自动设置根节点类型
          templateSelect.onchange = () => {
            const selectedTpl = templates.find((tpl) => tpl.id === templateSelect.value);
            if (selectedTpl && selectedTpl.levels) {
              const rootType = findRootType(selectedTpl.levels);
              if (rootType) {
                populateTypeSelect(typeSelect, rootType, true);
              }
            }
          };
        } catch (error) {
          handleError(error, t("common.operation_failed"));
        }
      }
    }
  }
}

function populateTypeSelect(select, currentValue, disabled, allowedValues = null) {
  select.innerHTML = "";
  const types = allowedValues ? [...allowedValues] : [...ORG_TYPES];
  // 如果当前值不在列表中，添加为选项
  if (currentValue && !types.includes(currentValue)) {
    types.push(currentValue);
  }
  types.forEach((value) => {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = getOrgTypeLabel(value);
    if (value === currentValue) option.selected = true;
    select.appendChild(option);
  });
  select.disabled = disabled;
}

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

export async function submitOrgForm() {
  const id = elementCache.getValue("org-id");
  const name = elementCache.getValue("org-name");
  const orgType = elementCache.getValue("org-type");
  const description = elementCache.getValue("org-description");
  const parentId = elementCache.getValue("org-parent-id");
  const templateSelect = document.getElementById("org-template-select");
  const templateId = templateSelect && templateSelect.style.display !== "none" ? templateSelect.value : null;

  if (!name) {
    showToast(t("organization.name_required"), "warning");
    return;
  }

  if (!orgType) {
    showToast(t("organization.type_required"), "warning");
    return;
  }

  // 新增根节点时必须选择模板
  if (!id && !parentId && !templateId) {
    showToast(t("org_template.select_required"), "warning");
    return;
  }

  const orgData = {
    name: name.trim(),
    org_type: orgType,
    parent_id: parentId || null,
    description: description || null,
  };

  // 新增根节点时带上 template_id
  if (!id && !parentId && templateId) {
    orgData.template_id = templateId;
  }

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

// ==========================================
// 模板管理
// ==========================================

/**
 * 打开模板管理界面
 */
export async function openTemplateManagement() {
  await openModal("org-template-modal", t("org_template.management"));

  const listContainer = document.getElementById("org-template-list");
  if (!listContainer) return;

  try {
    const templates = await getTemplates();
    renderTemplateList(listContainer, templates);
  } catch (error) {
    handleError(error, t("common.load_failed"));
  }

  // 绑定模板管理事件
  bindTemplateMgmtEvents();
}

function renderTemplateList(container, templates) {
  if (!templates || templates.length === 0) {
    container.innerHTML = `<p class="org-empty-state">${t("org_template.empty")}</p>`;
    return;
  }

  container.innerHTML = "";
  templates.forEach((tpl) => {
    const item = document.createElement("div");
    item.className = "org-template-item";
    const levelLabels = renderLevelsMapping(tpl.levels);
    item.innerHTML = `
      <div class="org-template-info">
        <strong>${escapeHtml(tpl.name)}</strong>
        <span class="org-template-levels">${levelLabels}</span>
        ${tpl.description ? `<span class="org-template-desc">${escapeHtml(tpl.description)}</span>` : ""}
      </div>
      <div class="org-template-actions">
        <button class="btn btn-sm btn-edit" data-action="edit-template" data-template-id="${tpl.id}">${t("common.edit")}</button>
        <button class="btn btn-sm btn-delete" data-action="delete-template" data-template-id="${tpl.id}" data-template-name="${escapeHtml(tpl.name)}">${t("common.delete")}</button>
      </div>
    `;
    container.appendChild(item);
  });
}

function bindTemplateMgmtEvents() {
  const listContainer = document.getElementById("org-template-list");
  if (!listContainer || listContainer.dataset.initialized === "true") return;
  listContainer.dataset.initialized = "true";

  listContainer.addEventListener("click", (e) => {
    const target = e.target.closest("[data-action]");
    if (!target) return;

    const action = target.dataset.action;
    const templateId = target.dataset.templateId;

    if (action === "edit-template") {
      editTemplate(templateId);
    } else if (action === "delete-template") {
      deleteTemplate(templateId, target.dataset.templateName);
    }
  });

  const addBtn = document.getElementById("org-template-add-btn");
  addBtn?.addEventListener("click", () => openTemplateEditor(null));
}

async function editTemplate(id) {
  try {
    const result = await apiGet(`/api/resources/org-templates/${id}`);
    if (result.success && result.data) {
      openTemplateEditor(result.data);
    }
  } catch (error) {
    handleError(error, t("common.operation_failed"));
  }
}

async function openTemplateEditor(template = null) {
  await openModal("org-template-editor-modal", t("org_template.editor_title"));

  const form = document.getElementById("org-template-editor-form");
  if (!form) return;

  form.reset();
  document.getElementById("org-template-editor-id").value = template?.id || "";

  const nameInput = document.getElementById("org-template-editor-name");
  const descInput = document.getElementById("org-template-editor-description");
  const treeContainer = document.getElementById("org-template-editor-levels-container");

  if (template) {
    nameInput.value = template.name;
    descInput.value = template.description || "";
    loadMappingIntoTree(treeContainer, template.levels);
  } else {
    nameInput.value = "";
    descInput.value = "";
    treeContainer.innerHTML = "";
    treeContainer.appendChild(createTypeNode("", true));
  }

  // 绑定树形编辑器事件（事件委托）
  if (!treeContainer.dataset.bound) {
    treeContainer.dataset.bound = "true";
    treeContainer.addEventListener("click", (e) => {
      const btn = e.target.closest("[data-action]");
      if (!btn) return;

      if (btn.dataset.action === "add-type-child") {
        const node = btn.closest(".org-template-type-node");
        const cc = getChildrenContainer(node);
        if (cc) {
          const newNode = createTypeNode("");
          cc.appendChild(newNode);
          newNode.querySelector(".org-template-type-input")?.focus();
        }
      } else if (btn.dataset.action === "remove-type") {
        btn.closest(".org-template-type-node")?.remove();
      }
    });
  }

  // 快捷填充
  const quickFill = document.getElementById("org-template-quick-fill");
  if (quickFill) {
    populateQuickFill(quickFill);
    if (!quickFill.dataset.bound) {
      quickFill.dataset.bound = "true";
      quickFill.addEventListener("change", () => {
        if (!quickFill.value) return;
        const preset = QUICK_FILL_PRESETS.find((p) => p.name === quickFill.value);
        if (preset) {
          loadMappingIntoTree(treeContainer, preset.levels);
        }
        quickFill.value = "";
      });
    }
  }
}

/** 创建一个类型节点 DOM 元素 */
function createTypeNode(type = "", isRoot = false) {
  const wrapper = document.createElement("div");
  wrapper.className = "org-template-type-node";
  if (isRoot) wrapper.classList.add("org-template-type-root");

  const row = document.createElement("div");
  row.className = "org-template-type-row";

  if (isRoot) {
    const rootBadge = document.createElement("span");
    rootBadge.className = "org-template-root-badge";
    rootBadge.textContent = t("org_template.root");
    row.appendChild(rootBadge);
  }

  const input = document.createElement("input");
  input.type = "text";
  input.className = "form-control org-template-type-input";
  input.placeholder = t("org_template.type_placeholder");
  input.maxLength = 50;
  if (type) input.value = type;

  const addChildBtn = document.createElement("button");
  addChildBtn.type = "button";
  addChildBtn.className = "btn btn-sm btn-add-child";
  addChildBtn.dataset.action = "add-type-child";
  addChildBtn.textContent = "+";
  addChildBtn.title = t("organization.add_child");

  row.appendChild(input);
  row.appendChild(addChildBtn);

  if (!isRoot) {
    const removeBtn = document.createElement("button");
    removeBtn.type = "button";
    removeBtn.className = "btn btn-sm btn-delete";
    removeBtn.dataset.action = "remove-type";
    removeBtn.textContent = "×";
    row.appendChild(removeBtn);
  }

  const childrenContainer = document.createElement("div");
  childrenContainer.className = "org-template-type-children";

  wrapper.appendChild(row);
  wrapper.appendChild(childrenContainer);
  return wrapper;
}

/** 获取节点的子级容器 */
function getChildrenContainer(node) {
  for (const child of node.children) {
    if (child.classList.contains("org-template-type-children")) return child;
  }
  return null;
}

/** 获取节点的类型名称 */
function getTypeOfNode(node) {
  for (const child of node.children) {
    if (child.classList.contains("org-template-type-row")) {
      const input = child.querySelector(".org-template-type-input");
      return input?.value?.trim() || null;
    }
  }
  return null;
}

/** 将映射格式加载为树形 DOM */
function loadMappingIntoTree(container, mapping) {
  container.innerHTML = "";
  if (!mapping || typeof mapping !== "object" || Array.isArray(mapping)) {
    container.appendChild(createTypeNode("", true));
    return;
  }
  const rootType = findRootType(mapping);
  if (!rootType) {
    container.appendChild(createTypeNode("", true));
    return;
  }

  function addTypeWithChildren(parentContainer, type, isRoot = false) {
    const node = createTypeNode(type, isRoot);
    parentContainer.appendChild(node);
    const cc = getChildrenContainer(node);
    const children = mapping[type] || [];
    children.forEach((childType) => addTypeWithChildren(cc, childType, false));
  }

  addTypeWithChildren(container, rootType, true);
}

/** 从树形 DOM 收集映射数据 */
function collectMapping(container) {
  const mapping = {};

  function processNode(node) {
    const type = getTypeOfNode(node);
    if (!type) return;
    const cc = getChildrenContainer(node);
    const childTypes = [];
    if (cc) {
      for (const childNode of cc.children) {
        if (!childNode.classList.contains("org-template-type-node")) continue;
        const childType = getTypeOfNode(childNode);
        if (childType) {
          childTypes.push(childType);
          processNode(childNode);
        }
      }
    }
    mapping[type] = childTypes;
  }

  for (const rootNode of container.children) {
    if (rootNode.classList.contains("org-template-type-node")) {
      processNode(rootNode);
    }
  }

  return mapping;
}

/**
 * 填充快捷填充下拉框
 */
function populateQuickFill(select) {
  select.innerHTML = `<option value="">${t("org_template.select_quick_fill")}</option>`;
  QUICK_FILL_PRESETS.forEach((preset) => {
    const option = document.createElement("option");
    option.value = preset.name;
    option.textContent = `${preset.name}: ${renderLevelsMapping(preset.levels, " / ")}`;
    select.appendChild(option);
  });
}

async function deleteTemplate(id, name) {
  const confirmed = await import("../utils/confirm.js").then((m) =>
    m.showConfirm(t("org_template.delete_confirm", { name })),
  );
  if (!confirmed) return;

  try {
    const result = await apiDelete(`/api/resources/org-templates/${id}`);
    if (result.success) {
      showToast(t("org_template.delete_success"), "success");
      openTemplateManagement();
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("common.operation_failed"));
  }
}

export async function submitOrgTemplateForm() {
  const id = document.getElementById("org-template-editor-id")?.value;
  const name = document.getElementById("org-template-editor-name")?.value;
  const description = document.getElementById("org-template-editor-description")?.value;
  const treeContainer = document.getElementById("org-template-editor-levels-container");

  if (!name) {
    showToast(t("org_template.name_required"), "warning");
    return;
  }

  const levels = collectMapping(treeContainer);
  if (Object.keys(levels).length === 0) {
    showToast(t("org_template.levels_required"), "warning");
    return;
  }

  const data = {
    name: name.trim(),
    levels: levels,
    description: description || null,
  };

  try {
    let result;
    if (id) {
      result = await apiPut(`/api/resources/org-templates/${id}`, data);
    } else {
      result = await apiPost("/api/resources/org-templates", data);
    }

    if (result.success) {
      showToast(id ? t("org_template.update_success") : t("org_template.create_success"), "success");
      closeModal("org-template-editor-modal");
      openTemplateManagement();
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("common.operation_failed"));
  }
}
