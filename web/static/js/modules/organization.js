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

function getOrgTypeLabel(orgType) {
  return t(`organization.types.${orgType}`) || orgType;
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
 */
function getChildButton(node) {
  // 只有还有下一级的节点才显示按钮
  // 通过 allowed-child-types API 获取，但渲染时我们用 level_index 判断
  // 如果节点有 template_id 且 level_index < 模板最大层级，则显示
  // 这里简化处理：叶子类型不显示
  const leafTypes = ["workstation", "cabinet_position"];
  if (leafTypes.includes(node.org_type)) {
    return "";
  }
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
        const childType = allowed.allowed_child_types[0];
        populateTypeSelect(typeSelect, childType.type, true);
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
            const levelLabels = tpl.levels
              .map((l) => getOrgTypeLabel(l))
              .join(" → ");
            option.textContent = `${tpl.name} (${levelLabels})`;
            templateSelect.appendChild(option);
          });

          // 模板选择变化时，自动设置根节点类型
          templateSelect.onchange = () => {
            const selectedTpl = templates.find((tpl) => tpl.id === templateSelect.value);
            if (selectedTpl && selectedTpl.levels.length > 0) {
              const rootType = selectedTpl.levels[0];
              populateTypeSelect(typeSelect, rootType, true);
            }
          };
        } catch (error) {
          handleError(error, t("common.operation_failed"));
        }
      }
    }
  }
}

function populateTypeSelect(select, currentValue, disabled) {
  select.innerHTML = "";
  ORG_TYPES.forEach((value) => {
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
    const levelLabels = tpl.levels.map((l) => getOrgTypeLabel(l)).join(" → ");
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
  // 打开模板编辑模态框
  await openModal("org-template-editor-modal", t("org_template.editor_title"));

  const form = document.getElementById("org-template-editor-form");
  if (!form) return;

  form.reset();
  document.getElementById("org-template-editor-id").value = template?.id || "";

  const nameInput = document.getElementById("org-template-editor-name");
  const descInput = document.getElementById("org-template-editor-description");
  const levelsContainer = document.getElementById("org-template-editor-levels-container");

  if (template) {
    nameInput.value = template.name;
    descInput.value = template.description || "";
    levelsContainer.innerHTML = "";
    template.levels.forEach((level) => addLevelRow(level));
  } else {
    levelsContainer.innerHTML = "";
    addLevelRow("headquarters");
  }

  // 绑定编辑器事件（每次打开时重新绑定，因为模态框是动态加载的）
  const addLevelBtn = document.getElementById("org-template-add-level-btn");
  if (addLevelBtn && !addLevelBtn.dataset.bound) {
    addLevelBtn.dataset.bound = "true";
    addLevelBtn.addEventListener("click", () => addLevelRow());
  }

  if (!levelsContainer.dataset.bound) {
    levelsContainer.dataset.bound = "true";
    levelsContainer.addEventListener("click", (e) => {
      if (e.target.classList.contains("org-template-remove-level-btn")) {
        e.target.closest(".org-template-level-row")?.remove();
      }
    });
  }
}

function addLevelRow(selectedType = null) {
  const container = document.getElementById("org-template-editor-levels-container");
  if (!container) return;

  const row = document.createElement("div");
  row.className = "org-template-level-row";

  const select = document.createElement("select");
  select.className = "form-control org-template-level-type";
  ORG_TYPES.forEach((type) => {
    const option = document.createElement("option");
    option.value = type;
    option.textContent = getOrgTypeLabel(type);
    if (selectedType === type) option.selected = true;
    select.appendChild(option);
  });

  const removeBtn = document.createElement("button");
  removeBtn.type = "button";
  removeBtn.className = "btn btn-sm btn-delete org-template-remove-level-btn";
  removeBtn.textContent = "×";

  row.appendChild(select);
  row.appendChild(removeBtn);
  container.appendChild(row);
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
  const levelSelects = document.querySelectorAll(".org-template-level-type");

  if (!name) {
    showToast(t("org_template.name_required"), "warning");
    return;
  }

  const levels = Array.from(levelSelects).map((s) => s.value);
  if (levels.length === 0) {
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
