/**
 * 组织管理模块
 * 处理组织层级结构的树形展示和CRUD操作
 * 基于模板的层级管理：模板定义层次链，每个根节点关联一个模板
 */

import { apiGet, apiPost, apiPut, apiDelete } from "../utils/apiClient.js";
import { showToast, handleError, escapeHtml, debounce } from "../utils/ui.js";
import { openModal, closeModal } from "../utils/modalLoader.js";
import { t } from "../utils/i18n.js";
import { iconButton, getIcon } from "../utils/icons.js";
import { elementCache } from "../utils/helpers.js";
import { ORG_ICON_GROUPS, renderOrgIcon, DEFAULT_ORG_ICON } from "../config/org-icons.js";
import { getOrgIcon } from "../config/org-config.js";

// ==========================================
// 常量定义
// ==========================================

// 高频兜底文案键（文件内出现 10+ 次），抽常量避免字面量散落
const T_KEY_OPERATION_FAILED = "common.operation_failed";

/** 模板图标缓存 { templateId: { type_name: icon } } */
let templateIconsById = {};

/** 构建按模板 ID 索引的图标映射（同名类型互不覆盖，节点按所属模板取图标） */
function buildTemplateIconsMap(templates) {
  const map = {};
  templates.forEach((tpl) => {
    if (tpl.icons && typeof tpl.icons === "object") {
      map[tpl.id] = tpl.icons;
    }
  });
  templateIconsById = map;
}

/** 获取节点图标：所属模板 icons → API icons → 默认 */
async function getNodeIcon(orgType, templateId) {
  const icons = (templateId && templateIconsById[templateId]) || {};
  return await getOrgIcon(orgType, icons);
}

/** 获取组织类型标签（从i18n获取）*/
function getOrgTypeLabel(orgType) {
  const key = `organization.types.${orgType}`;
  const translated = t(key);
  return translated === key ? orgType : translated;
}

/** 将 levels 映射渲染为可读文本，多路径用 separator 分隔 */
function findRootType(levels) {
  if (!levels || typeof levels !== "object" || Array.isArray(levels)) {
    return null;
  }
  const allChildren = new Set();
  Object.values(levels).forEach((children) => {
    if (Array.isArray(children)) {
      children.forEach((c) => allChildren.add(c));
    }
  });
  const roots = Object.keys(levels).filter((k) => !allChildren.has(k));
  return roots.length === 1 ? roots[0] : null;
}

/** 将 levels 映射渲染为可读文本，多路径用 separator 分隔 */
function renderLevelsMapping(levels, separator = "<br>") {
  const rootType = findRootType(levels);
  if (!rootType) {
    return "";
  }
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
  // 层级 key 为用户可编辑的自由文本，拼接结果统一转义防注入
  return paths.map((p) => escapeHtml(p.join(" → "))).join(separator);
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
  if (!container) {
    return;
  }

  if (container.dataset.initialized === "true") {
    return;
  }
  container.dataset.initialized = "true";

  container.addEventListener("click", handleTreeClick);

  const addRootBtn = document.getElementById("org-add-root-btn");
  addRootBtn?.addEventListener("click", () => openOrgModal(null, null));

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

/**
 * 性能优化建议：
 * - 当组织节点数量 > 1000 时，建议实现虚拟滚动或分页加载
 * - 可采用懒加载策略：仅加载可见区域的节点
 * - 或采用"按需展开"策略：初始只加载根节点，点击展开时动态加载子节点
 * - 当前实现适用于中小规模组织树（< 1000节点）
 */

export async function loadOrganizationTree() {
  const container = document.getElementById("organization-tree-container");
  if (!container) {
    return;
  }

  try {
    const [treeResult, templatesResult] = await Promise.all([
      apiGet("/api/resources/organizations/tree"),
      apiGet("/api/resources/org-templates")
    ]);

    // 构建模板图标映射
    if (templatesResult.success && templatesResult.data?.items) {
      buildTemplateIconsMap(templatesResult.data.items);
    }

    if (treeResult.success && treeResult.data) {
      await renderOrgTree(container, treeResult.data);
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

async function renderOrgTree(container, treeData) {
  if (!treeData || treeData.length === 0) {
    renderEmptyState(container);
    return;
  }

  container.innerHTML = "";
  const fragment = document.createDocumentFragment();

  for (const node of treeData) {
    fragment.appendChild(await renderTreeNode(node, 0));
  }

  container.appendChild(fragment);
}

async function renderTreeNode(node, depth) {
  const wrapper = document.createElement("div");
  wrapper.className = "org-node-wrapper";
  wrapper.dataset.nodeId = node.id;
  wrapper.dataset.templateId = node.template_id || "";

  const nodeEl = document.createElement("div");
  nodeEl.className = "org-node";
  nodeEl.style.paddingLeft = `${depth * 24 + 16}px`;

  const hasChildren = node.children && node.children.length > 0;
  const icon = await getNodeIcon(node.org_type, node.template_id);
  const typeLabel = getOrgTypeLabel(node.org_type);

  const toggleStateClass = allExpanded ? "expanded" : "";
  const toggleBtn = hasChildren
    ? `<span class="org-toggle ${toggleStateClass}" data-action="toggle" role="button" tabindex="0">
         <svg class="org-toggle-icon" width="16" height="16" viewBox="0 0 16 16"><path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="2"/></svg>
       </span>`
    : '<span class="org-toggle-placeholder"></span>';

  const addChildBtns = getAllowedChildButtons(node);

  nodeEl.innerHTML = `
    ${toggleBtn}
    <span class="org-node-icon">${renderOrgIcon(icon)}</span>
    <span class="org-node-name">${escapeHtml(node.name)}</span>
    <span class="org-node-type-badge" data-type="${escapeHtml(node.org_type)}">${escapeHtml(typeLabel)}</span>
    ${node.description ? `<span class="org-node-desc" title="${escapeHtml(node.description)}">${escapeHtml(node.description)}</span>` : ""}
    <span class="org-node-actions">
      ${addChildBtns}
      ${iconButton({ icon: "edit", label: t("common.edit"), cls: "btn-edit", attrs: `data-action="edit" data-id="${node.id}" data-name="${escapeHtml(node.name)}"` })}
      ${iconButton({ icon: "users", label: t("organization.manage_employees"), cls: "btn-employees", attrs: `data-action="employees" data-id="${node.id}" data-name="${escapeHtml(node.name)}"` })}
      ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-delete", attrs: `data-action="delete" data-id="${node.id}" data-name="${escapeHtml(node.name)}"` })}
    </span>
  `;

  wrapper.appendChild(nodeEl);

  if (hasChildren) {
    const childrenContainer = document.createElement("div");
    childrenContainer.className = "org-children";
    if (!allExpanded) {
      childrenContainer.style.display = "none";
    }
    for (const child of node.children) {
      childrenContainer.appendChild(await renderTreeNode(child, depth + 1));
    }
    wrapper.appendChild(childrenContainer);
  }

  return wrapper;
}

/**
 * 获取允许的下级类型按钮
 * 基于模板层级，每个允许的下级类型一个按钮
 */
function getAllowedChildButtons(node) {
  return iconButton({
    icon: "plusCircle",
    label: t("organization.add_child"),
    cls: "btn-add-child",
    attrs: `data-action="add-child" data-parent-id="${node.id}"`
  });
}

function renderEmptyState(container) {
  container.innerHTML = `<div class="org-empty-state">${t("organization.empty")}</div>`;
}

// ==========================================
// 树形交互
// ==========================================

function handleTreeClick(e) {
  const target = e.target.closest("[data-action]");
  if (!target) {
    return;
  }

  const action = target.dataset.action;

  switch (action) {
    case "toggle":
      e.stopPropagation();
      toggleNode(target);
      break;
    case "add-child":
      e.stopPropagation();
      openOrgModal(null, target.dataset.parentId, target.dataset.childType || null);
      break;
    case "edit":
      e.stopPropagation();
      editOrganization(target.dataset.id);
      break;
    case "employees":
      e.stopPropagation();
      openEmployeeModal(target.dataset.id, target.dataset.name);
      break;
    case "delete":
      e.stopPropagation();
      deleteOrganization(target.dataset.id, target.dataset.name);
      break;
  }
}

function toggleNode(toggleEl) {
  const wrapper = toggleEl.closest(".org-node-wrapper");
  if (!wrapper) {
    return;
  }

  const children = wrapper.querySelector(":scope > .org-children");
  if (!children) {
    return;
  }

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
    // 清空搜索：重新渲染树以还原展开/折叠状态
    loadOrganizationTree();
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
        if (children) {
          children.style.display = "";
        }
        const toggle = parent.querySelector(":scope > .org-node .org-toggle");
        if (toggle) {
          toggle.classList.add("expanded");
        }
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
 * @param {string|null} presetType - 预设的下级类型（从子节点按钮传入）
 */
export async function openOrgModal(org = null, parentId = null, presetType = null) {
  await openModal("organization-modal");

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
    elementCache.setValue("org-type", org.type_path || org.org_type || "");
    elementCache.setValue("org-description", org.description || "");
    elementCache.setValue("org-parent-id", org.parent_id || "");
    // 编辑时类型不可修改，显示从模板派生的名称
    typeSelect.innerHTML = "";
    const option = document.createElement("option");
    option.value = org.type_path || org.org_type || "";
    option.textContent = getOrgTypeLabel(org.org_type || org.type_path || "");
    option.dataset.typeName = org.org_type || "";
    option.selected = true;
    typeSelect.appendChild(option);
    typeSelect.disabled = true;
    parentInfo?.classList.add("hidden");
    templateSelectContainer?.classList.add("hidden");
  } else if (parentId) {
    // 新增子节点 - 类型由模板决定
    title.textContent = t("organization.add");
    elementCache.setValue("org-parent-id", parentId);

    try {
      const allowed = await getAllowedChildTypes(parentId);
      if (allowed && allowed.allowed_child_types && allowed.allowed_child_types.length > 0) {
        populateTypeSelectWithOptions(typeSelect, allowed.allowed_child_types, presetType);
      } else {
        typeSelect.innerHTML = `<option value="">${t("organization.no_child_allowed")}</option>`;
        typeSelect.disabled = true;
      }
      if (parentInfo) {
        parentInfo.classList.remove("hidden");
        document.getElementById("org-parent-name").textContent = allowed?.parent_name || "";
        document.getElementById("org-parent-type").textContent =
          getOrgTypeLabel(allowed?.parent_type) || allowed?.parent_type || "";
      }
      templateSelectContainer?.classList.add("hidden");
    } catch (error) {
      handleError(error, t(T_KEY_OPERATION_FAILED));
    }
  } else {
    // 新增根节点 - 需要选择模板
    title.textContent = t("organization.add_node");
    elementCache.setValue("org-parent-id", "");
    parentInfo?.classList.add("hidden");

    // 显示模板选择
    if (templateSelectContainer) {
      templateSelectContainer.classList.remove("hidden");
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

          // 模板选择变化时，自动设置根节点类型（type_path="0"）
          templateSelect.onchange = () => {
            const selectedTpl = templates.find((tpl) => tpl.id === templateSelect.value);
            if (selectedTpl && selectedTpl.levels) {
              const rootType = findRootType(selectedTpl.levels);
              if (rootType) {
                // 根节点 type_path 固定为 "0"
                typeSelect.innerHTML = "";
                const option = document.createElement("option");
                option.value = "0";
                option.textContent = getOrgTypeLabel(rootType);
                option.dataset.typeName = rootType;
                option.selected = true;
                typeSelect.appendChild(option);
                typeSelect.disabled = true;
              }
            }
          };
        } catch (error) {
          handleError(error, t(T_KEY_OPERATION_FAILED));
        }
      }
    }
  }
}

function populateTypeSelectWithOptions(select, options, presetType) {
  select.innerHTML = `<option value="">${t("organization.select_type")}</option>`;
  options.forEach((opt) => {
    const option = document.createElement("option");
    option.value = opt.type_path || opt.type;
    option.textContent = opt.label || getOrgTypeLabel(opt.type);
    option.dataset.typeName = opt.type;
    if (presetType && opt.type === presetType) {
      option.selected = true;
    }
    select.appendChild(option);
  });
  select.disabled = false;
}

export async function editOrganization(id) {
  try {
    const result = await apiGet(`/api/resources/organizations/${id}`);
    if (result.success) {
      openOrgModal(result.data);
    } else {
      showToast(`${t(T_KEY_OPERATION_FAILED)}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t(T_KEY_OPERATION_FAILED));
  }
}

export async function deleteOrganization(id, name) {
  const confirmed = await import("../utils/confirm.js").then((m) =>
    m.showConfirm(t("organization.delete_confirm", { name }))
  );
  if (!confirmed) {
    return;
  }

  try {
    const result = await apiDelete(`/api/resources/organizations/${id}`);
    if (result.success) {
      showToast(t("organization.delete_success"), "success");
      loadOrganizationTree();
    } else {
      showToast(`${t(T_KEY_OPERATION_FAILED)}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t(T_KEY_OPERATION_FAILED));
  }
}

// ==========================================
// 人员管理（员工挂在组织节点下）
// ==========================================

/** 性别取值显示文案 */
function genderText(gender) {
  if (gender === "male") {
    return t("employee.gender_male");
  }
  if (gender === "female") {
    return t("employee.gender_female");
  }
  return t("employee.gender_unknown");
}

/** 打开人员管理模态框（orgName 用于标题展示） */
export async function openEmployeeModal(orgId, orgName) {
  const modal = await openModal("employee-modal");
  if (!modal) {
    return;
  }

  const orgIdInput = document.getElementById("employee-modal-org-id");
  if (orgIdInput) {
    orgIdInput.value = orgId || "";
  }
  const orgNameEl = document.getElementById("employee-modal-org-name");
  if (orgNameEl) {
    orgNameEl.textContent = orgName ? ` - ${orgName}` : "";
  }

  const addBtn = document.getElementById("employee-add-btn");
  if (addBtn) {
    addBtn.onclick = () => openEmployeeEditModal(orgId, null);
  }

  await loadEmployeeList(orgId);
}

/** 加载组织下的员工列表 */
async function loadEmployeeList(orgId) {
  const tbody = document.querySelector("#employee-table tbody");
  if (!tbody) {
    return;
  }

  try {
    const result = await apiGet(`/api/resources/employees?org_id=${encodeURIComponent(orgId)}`);
    const employees = result.success && Array.isArray(result.data) ? result.data : [];
    if (!employees.length) {
      tbody.innerHTML = `<tr class="empty-row"><td colspan="6" class="text-center">${t("common.no_data")}</td></tr>`;
      return;
    }

    tbody.innerHTML = "";
    employees.forEach((emp) => {
      const tr = document.createElement("tr");
      tr.innerHTML = `
        <td>${escapeHtml(emp.name || "-")}</td>
        <td>${escapeHtml(genderText(emp.gender))}</td>
        <td>${escapeHtml(emp.phone || "-")}</td>
        <td>${escapeHtml(emp.email || "-")}</td>
        <td>${escapeHtml(emp.hire_date || "-")}</td>
        <td class="employee-actions-cell"></td>`;

      const actions = tr.querySelector(".employee-actions-cell");

      actions.innerHTML = `
        ${iconButton({ icon: "edit", label: t("common.edit"), cls: "btn-edit employee-edit-btn" })}
        ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-delete employee-delete-btn" })}
      `;
      actions.querySelector(".employee-edit-btn")?.addEventListener("click", () => openEmployeeEditModal(orgId, emp));
      actions.querySelector(".employee-delete-btn")?.addEventListener("click", () => deleteEmployee(emp));

      tbody.appendChild(tr);
    });
  } catch (error) {
    handleError(error, t("employee.load_failed"));
  }
}

/** 打开员工编辑模态框（employee 为 null 时是新增） */
async function openEmployeeEditModal(orgId, employee) {
  const modal = await openModal("employee-edit-modal");
  if (!modal) {
    return;
  }

  const title = document.getElementById("employee-edit-modal-title");
  if (title) {
    title.textContent = employee ? t("employee.edit_title") : t("employee.add_title");
  }

  elementCache.setValue("employee-edit-id", employee?.id || "");
  elementCache.setValue("employee-edit-org-id", orgId || "");
  elementCache.setValue("employee-edit-name", employee?.name || "");
  elementCache.setValue("employee-edit-gender", employee?.gender || "unknown");
  elementCache.setValue("employee-edit-phone", employee?.phone || "");
  elementCache.setValue("employee-edit-email", employee?.email || "");
  elementCache.setValue("employee-edit-hire-date", employee?.hire_date || "");

  const form = elementCache.get("employee-edit-form");
  form.onsubmit = async (e) => {
    e.preventDefault();
    const id = elementCache.getValue("employee-edit-id");
    const name = elementCache.getValue("employee-edit-name").trim();
    if (!name) {
      showToast(t("employee.name_required"), "warning");
      return;
    }

    const body = {
      name,
      gender: elementCache.getValue("employee-edit-gender") || "unknown",
      phone: elementCache.getValue("employee-edit-phone").trim(),
      email: elementCache.getValue("employee-edit-email").trim(),
      hire_date: elementCache.getValue("employee-edit-hire-date") || null
    };

    try {
      const result = id
        ? await apiPut(`/api/resources/employees/${id}`, body)
        : await apiPost("/api/resources/employees", { ...body, org_id: orgId });
      if (result.success) {
        showToast(result.message, "success");
        closeModal("employee-edit-modal");
        form.reset();
        loadEmployeeList(orgId);
      } else {
        showToast(`${t(T_KEY_OPERATION_FAILED)}: ${result.message}`, "error");
      }
    } catch (error) {
      handleError(error, t(T_KEY_OPERATION_FAILED));
    }
  };
}

/** 删除员工（工位管理人的员工引用会自动解绑） */
async function deleteEmployee(employee) {
  const confirmed = await import("../utils/confirm.js").then((m) =>
    m.showConfirm(t("employee.delete_confirm", { name: employee.name }))
  );
  if (!confirmed) {
    return;
  }

  try {
    const result = await apiDelete(`/api/resources/employees/${employee.id}`);
    if (result.success) {
      showToast(result.message, "success");
      const orgId = document.getElementById("employee-modal-org-id")?.value;
      loadEmployeeList(orgId);
    } else {
      showToast(`${t(T_KEY_OPERATION_FAILED)}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t(T_KEY_OPERATION_FAILED));
  }
}

export async function submitOrgForm() {
  const id = elementCache.getValue("org-id");
  const name = elementCache.getValue("org-name");
  const typeSelect = elementCache.get("org-type");
  const typePath = typeSelect?.value || "";
  const description = elementCache.getValue("org-description");
  const parentId = elementCache.getValue("org-parent-id");
  const templateSelect = document.getElementById("org-template-select");
  const templateId =
    templateSelect && templateSelect.style.display !== "none" ? templateSelect.value : null;

  if (!name) {
    showToast(t("organization.name_required"), "warning");
    return;
  }

  if (!typePath) {
    showToast(t("organization.type_required"), "warning");
    return;
  }

  // 新增根节点时必须选择模板
  if (!id && !parentId && !templateId) {
    showToast(t("org_template.select_required"), "warning");
    return;
  }

  try {
    let result;
    if (id) {
      // 编辑模式：仅更新 name / description，type_path 由模板决定不可修改
      result = await apiPut(`/api/resources/organizations/${id}`, {
        name: name.trim(),
        description: description || null
      });
    } else {
      // 新增模式：需要 parent_id（子节点）或 template_id（根节点）
      const orgData = {
        name: name.trim(),
        type_path: typePath,
        parent_id: parentId || null,
        description: description || null
      };
      if (!parentId && templateId) {
        orgData.template_id = templateId;
      }
      result = await apiPost("/api/resources/organizations", orgData);
    }

    if (result.success) {
      showToast(
        id ? t("organization.update_success") : t("organization.create_success"),
        "success"
      );
      closeModal("organization-modal");
      loadOrganizationTree();
    } else {
      showToast(`${t(T_KEY_OPERATION_FAILED)}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t(T_KEY_OPERATION_FAILED));
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
  if (!listContainer) {
    return;
  }

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
        ${iconButton({ icon: "edit", label: t("common.edit"), cls: "btn-edit", attrs: `data-action="edit-template" data-template-id="${tpl.id}"` })}
        ${iconButton({ icon: "trash", label: t("common.delete"), cls: "btn-delete", attrs: `data-action="delete-template" data-template-id="${tpl.id}" data-template-name="${escapeHtml(tpl.name)}"` })}
      </div>
    `;
    container.appendChild(item);
  });
}

function bindTemplateMgmtEvents() {
  const listContainer = document.getElementById("org-template-list");
  if (!listContainer || listContainer.dataset.initialized === "true") {
    return;
  }
  listContainer.dataset.initialized = "true";

  listContainer.addEventListener("click", (e) => {
    const target = e.target.closest("[data-action]");
    if (!target) {
      return;
    }

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
    handleError(error, t(T_KEY_OPERATION_FAILED));
  }
}

async function openTemplateEditor(template = null) {
  await openModal("org-template-editor-modal", t("org_template.editor_title"));

  const form = document.getElementById("org-template-editor-form");
  if (!form) {
    return;
  }

  form.reset();
  document.getElementById("org-template-editor-id").value = template?.id || "";

  const nameInput = document.getElementById("org-template-editor-name");
  const descInput = document.getElementById("org-template-editor-description");
  const treeContainer = document.getElementById("org-template-editor-levels-container");

  if (template) {
    nameInput.value = template.name;
    descInput.value = template.description || "";
    loadMappingIntoTree(treeContainer, template.levels, template.icons || {});
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
      if (!btn) {
        return;
      }

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

  // 快捷填充（从数据库模板加载）
  const quickFill = document.getElementById("org-template-quick-fill");
  if (quickFill) {
    populateQuickFillFromDB(quickFill);
    if (!quickFill.dataset.bound) {
      quickFill.dataset.bound = "true";
      quickFill.addEventListener("change", async () => {
        if (!quickFill.value) {
          return;
        }
        try {
          const templates = await getTemplates();
          const template = templates.find((t) => t.name === quickFill.value);
          if (template) {
            loadMappingIntoTree(treeContainer, template.levels, template.icons || {});
          }
        } catch (error) {
          handleError(error, t("common.load_failed"));
        }
        quickFill.value = "";
      });
    }
  }
}

/** 创建一个类型节点 DOM 元素 */
function createTypeNode(type = "", isRoot = false, icon = "") {
  const wrapper = document.createElement("div");
  wrapper.className = "org-template-type-node";
  if (isRoot) {
    wrapper.classList.add("org-template-type-root");
  }

  const row = document.createElement("div");
  row.className = "org-template-type-row";

  if (isRoot) {
    const rootBadge = document.createElement("span");
    rootBadge.className = "org-template-root-badge";
    rootBadge.textContent = t("org_template.root");
    row.appendChild(rootBadge);
  }

  // 图标选择按钮（存储图标 key，未设置时用默认图标）
  const iconBtn = document.createElement("button");
  iconBtn.type = "button";
  iconBtn.className = "org-template-icon-btn";
  iconBtn.dataset.icon = icon || DEFAULT_ORG_ICON;
  iconBtn.innerHTML = renderOrgIcon(iconBtn.dataset.icon);
  iconBtn.title = t("org_template.select_icon");
  iconBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    showIconPicker(iconBtn);
  });

  // 类型输入框
  const input = document.createElement("input");
  input.type = "text";
  input.className = "form-control org-template-type-input";
  input.placeholder = t("org_template.type_placeholder");
  input.maxLength = 50;
  if (type) {
    input.value = type;
  }

  // 类型徽标预览
  const badgePreview = document.createElement("span");
  badgePreview.className = "org-node-type-badge";
  if (type) {
    badgePreview.dataset.type = type;
    badgePreview.textContent = getOrgTypeLabel(type);
  }

  // 输入变化时实时更新徽标（图标需要手动选择）
  input.addEventListener("input", () => {
    const val = input.value.trim();
    if (val) {
      badgePreview.dataset.type = val;
      badgePreview.textContent = getOrgTypeLabel(val);
      badgePreview.style.display = "";
    } else {
      badgePreview.removeAttribute("data-type");
      badgePreview.textContent = "";
      badgePreview.style.display = "none";
    }
  });

  const addChildBtn = document.createElement("button");
  addChildBtn.type = "button";
  addChildBtn.className = "icon-btn btn-add-child";
  addChildBtn.dataset.action = "add-type-child";
  addChildBtn.title = t("org_template.add_child_type");
  addChildBtn.innerHTML = getIcon("plus");

  row.appendChild(iconBtn);
  row.appendChild(input);
  row.appendChild(badgePreview);
  row.appendChild(addChildBtn);

  if (!isRoot) {
    const removeBtn = document.createElement("button");
    removeBtn.type = "button";
    removeBtn.className = "icon-btn btn-delete";
    removeBtn.dataset.action = "remove-type";
    removeBtn.title = t("common.delete");
    removeBtn.innerHTML = getIcon("x");
    row.appendChild(removeBtn);
  }

  const childrenContainer = document.createElement("div");
  childrenContainer.className = "org-template-type-children";

  wrapper.appendChild(row);
  wrapper.appendChild(childrenContainer);
  return wrapper;
}

/** 当前打开的图标选择面板的清理函数（含 document 级监听移除） */
let dismissActiveIconPicker = null;

/** 显示图标选择面板（按 业务组织/物理地点/功能空间 分组的 SVG 图标） */
function showIconPicker(iconBtn) {
  // 关闭已有面板，同步移除其 document 级关闭监听（避免遗留至无关点击）
  if (dismissActiveIconPicker) {
    dismissActiveIconPicker();
  }
  document.querySelectorAll(".org-icon-picker").forEach((p) => p.remove());

  const picker = document.createElement("div");
  picker.className = "org-icon-picker";

  for (const group of ORG_ICON_GROUPS) {
    const groupEl = document.createElement("div");
    groupEl.className = "org-icon-picker-group";
    const groupTitle = document.createElement("div");
    groupTitle.className = "org-icon-picker-group-title";
    groupTitle.textContent = t(group.groupKey);
    groupEl.appendChild(groupTitle);

    const grid = document.createElement("div");
    grid.className = "org-icon-picker-grid";
    for (const item of group.icons) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "org-icon-picker-item";
      btn.dataset.icon = item.key;
      btn.title = t(item.labelKey);
      btn.innerHTML = renderOrgIcon(item.key);
      if (item.key === iconBtn.dataset.icon) {
        btn.classList.add("active");
      }
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        iconBtn.dataset.icon = item.key;
        iconBtn.innerHTML = renderOrgIcon(item.key);
        dismissPicker();
      });
      grid.appendChild(btn);
    }
    groupEl.appendChild(grid);
    picker.appendChild(groupEl);
  }

  // 定位面板
  iconBtn.style.position = "relative";
  iconBtn.appendChild(picker);

  // 点击外部关闭；选中图标时同样经由 dismissPicker 移除监听
  const dismissPicker = () => {
    picker.remove();
    document.removeEventListener("click", closePicker);
    if (dismissActiveIconPicker === dismissPicker) {
      dismissActiveIconPicker = null;
    }
  };
  const closePicker = (e) => {
    if (!picker.contains(e.target) && e.target !== iconBtn) {
      dismissPicker();
    }
  };
  dismissActiveIconPicker = dismissPicker;
  setTimeout(() => document.addEventListener("click", closePicker), 0);
}

/** 获取节点的子级容器 */
function getChildrenContainer(node) {
  return node.querySelector(":scope > .org-template-type-children");
}

/** 获取节点的类型名称 */
function getTypeOfNode(node) {
  const input = node.querySelector(":scope > .org-template-type-row .org-template-type-input");
  return input?.value?.trim() || null;
}

/** 将映射格式加载为树形 DOM */
function loadMappingIntoTree(container, mapping, icons = {}) {
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
    const icon = icons[type] || "";
    const node = createTypeNode(type, isRoot, icon);
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
    if (!type) {
      return;
    }
    const cc = getChildrenContainer(node);
    const childTypes = [];
    if (cc) {
      for (const childNode of cc.children) {
        if (!childNode.classList.contains("org-template-type-node")) {
          continue;
        }
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

/** 从树形 DOM 收集图标映射数据 */
function collectIcons(container) {
  const icons = {};

  function processNode(node) {
    const type = getTypeOfNode(node);
    if (!type) {
      return;
    }
    const iconBtn = node.querySelector(":scope > .org-template-type-row .org-template-icon-btn");
    if (iconBtn && iconBtn.dataset.icon) {
      icons[type] = iconBtn.dataset.icon;
    }
    const cc = getChildrenContainer(node);
    if (cc) {
      for (const childNode of cc.children) {
        if (!childNode.classList.contains("org-template-type-node")) {
          continue;
        }
        processNode(childNode);
      }
    }
  }

  for (const rootNode of container.children) {
    if (rootNode.classList.contains("org-template-type-node")) {
      processNode(rootNode);
    }
  }

  return icons;
}

/**
 * 填充快捷填充下拉框（从数据库加载模板）
 */
async function populateQuickFillFromDB(select) {
  try {
    select.innerHTML = `<option value="">${t("org_template.select_quick_fill")}</option>`;
    const templates = await getTemplates();
    templates.forEach((template) => {
      const option = document.createElement("option");
      option.value = template.name;
      option.textContent = template.name;
      select.appendChild(option);
    });
  } catch (error) {
    console.error("加载快捷填充列表失败:", error);
  }
}

async function deleteTemplate(id, name) {
  const confirmed = await import("../utils/confirm.js").then((m) =>
    m.showConfirm(t("org_template.delete_confirm", { name }))
  );
  if (!confirmed) {
    return;
  }

  try {
    const result = await apiDelete(`/api/resources/org-templates/${id}`);
    if (result.success) {
      showToast(t("org_template.delete_success"), "success");
      openTemplateManagement();
      // 模板变更可能影响节点类型解析（重命名/层级变化），重载组织树保证徽标一致
      await loadOrganizationTree();
    } else {
      showToast(`${t(T_KEY_OPERATION_FAILED)}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t(T_KEY_OPERATION_FAILED));
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

  const icons = collectIcons(treeContainer);

  const data = {
    name: name.trim(),
    levels,
    icons: Object.keys(icons).length > 0 ? icons : null,
    description: description || null
  };

  try {
    let result;
    if (id) {
      result = await apiPut(`/api/resources/org-templates/${id}`, data);
    } else {
      result = await apiPost("/api/resources/org-templates", data);
    }

    if (result.success) {
      showToast(
        id ? t("org_template.update_success") : t("org_template.create_success"),
        "success"
      );
      closeModal("org-template-editor-modal");
      openTemplateManagement();
      // 模板变更可能影响节点类型解析（重命名/层级变化），重载组织树保证徽标一致
      await loadOrganizationTree();
    } else {
      showToast(`${t(T_KEY_OPERATION_FAILED)}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t(T_KEY_OPERATION_FAILED));
  }
}
