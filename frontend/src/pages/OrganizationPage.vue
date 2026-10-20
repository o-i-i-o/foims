<script setup>
/**
 * 组织管理页（移植自旧版 main.html organization 分区 + modules/organization.js
 * + pages/organization.css）：组织树（展开/折叠/搜索过滤）+ 节点 CRUD +
 * 人员管理 + 层级模板管理。数据层为 TanStack Query：树/模板/可用类型三查询
 * （可用类型对应旧版模块级缓存，staleTime Infinity），写操作经 useMutation
 * 后失效缓存。搜索为旧版客户端过滤语义：命中节点显示并强制展开其祖先链。
 */
import { computed, provide, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiGet } from "../api/client.js";
import { showToast } from "../composables/useToast.js";
import { resolveOrgIcon } from "../config/orgConfig.js";
import ConfirmModal from "../components/ConfirmModal.vue";
import EmployeeModal from "../components/org/EmployeeModal.vue";
import OrgNodeModal from "../components/org/OrgNodeModal.vue";
import OrgTreeNode from "../components/org/OrgTreeNode.vue";
import TemplateModal from "../components/org/TemplateModal.vue";

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 数据查询 ====================

const treeQuery = useQuery({
  queryKey: ["organizations-tree"],
  queryFn: async () => {
    const result = await apiGet("/api/resources/organizations/tree");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return Array.isArray(result.data) ? result.data : [];
  }
});

const treeData = computed(() => (treeQuery.isError.value ? [] : (treeQuery.data.value ?? [])));

const availableTypesQuery = useQuery({
  queryKey: ["org-available-types"],
  queryFn: async () => {
    const result = await apiGet("/api/resources/org-templates/available-types");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return { types: result.data?.types ?? [], icons: result.data?.icons ?? {} };
  },
  // 对应旧版模块级缓存（loadOrgTypesFromAPI 仅首次请求）
  staleTime: Infinity
});

// 模板列表查询：树节点图标/徽标随树一起加载（与弹窗共用同键缓存，自动去重）
const templatesQuery = useQuery({
  queryKey: ["org-templates"],
  queryFn: async () => {
    const result = await apiGet("/api/resources/org-templates");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return Array.isArray(result.data?.items) ? result.data.items : [];
  }
});

/** 模板图标映射 { templateId: { type_name: icon } }（同名类型互不覆盖） */
const templateIconsById = computed(() => {
  const map = {};
  for (const tpl of templatesQuery.data.value ?? []) {
    if (tpl.icons && typeof tpl.icons === "object") {
      map[tpl.id] = tpl.icons;
    }
  }
  return map;
});

/** 节点图标解析：所属模板 icons → API icons → 关键词匹配 → 默认 */
function resolveIcon(orgType, templateId) {
  return resolveOrgIcon(orgType, templateIconsById.value[templateId] ?? {}, availableTypesQuery.data.value?.icons ?? {});
}

// ==================== 搜索（客户端过滤，防抖 300ms） ====================

const searchInput = ref("");
const searchKeyword = ref("");
let searchTimer = null;

watch(searchInput, (value) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    searchKeyword.value = value.trim().toLowerCase();
    // 清空搜索即整树重渲染：展开/折叠状态还原（对应旧版重新加载树）
    if (!searchKeyword.value) {
      renderEpoch.value += 1;
    }
  }, 300);
});

/**
 * 搜索命中集合：visible 为命中节点及其祖先链；expand 为纯祖先命中
 * （自身不匹配但后代匹配）的强制展开集合。对应旧版按名称过滤并展开
 * 祖先容器的 DOM 操作。
 */
const searchState = computed(() => {
  const keyword = searchKeyword.value;
  if (!keyword) {
    return null;
  }
  const visible = new Set();
  const expand = new Set();
  function walk(nodes) {
    let hit = false;
    for (const node of nodes) {
      const selfHit = node.name.toLowerCase().includes(keyword);
      const childHit = walk(node.children ?? []);
      if (selfHit || childHit) {
        visible.add(node.id);
        if (!selfHit) {
          expand.add(node.id);
        }
        hit = true;
      }
    }
    return hit;
  }
  walk(treeData.value);
  return { visible, expand };
});

// ==================== 展开/折叠 ====================

const allExpanded = ref(false);

// 树渲染代际：数据变化或清空搜索时整树重挂载，节点展开态还原
// （对应旧版每次 loadOrganizationTree 全量重渲染的行为）
const renderEpoch = ref(0);
watch(treeQuery.data, () => {
  renderEpoch.value += 1;
});

function expandAll() {
  allExpanded.value = true;
}

function collapseAll() {
  allExpanded.value = false;
}

// ==================== 树节点操作（注入给递归节点） ====================

const orgModal = reactive({ open: false, mode: "add-root", node: null, parentId: "" });
const employeeModal = reactive({ open: false, orgId: "", orgName: "" });
const templateModalOpen = ref(false);
const deletingOrg = ref(null);

provide("orgTreeCtx", {
  keyword: searchKeyword,
  allExpanded,
  search: searchState,
  resolveIcon,
  onAction
});

function onAction(node, action) {
  if (action === "add-child") {
    orgModal.mode = "add-child";
    orgModal.node = null;
    orgModal.parentId = node.id;
    orgModal.open = true;
  } else if (action === "edit") {
    orgModal.mode = "edit";
    orgModal.node = node;
    orgModal.parentId = "";
    orgModal.open = true;
  } else if (action === "employees") {
    employeeModal.orgId = node.id;
    employeeModal.orgName = node.name;
    employeeModal.open = true;
  } else if (action === "delete") {
    deletingOrg.value = node;
  }
}

// ==================== 组织节点删除 ====================

const deleteOrgMutation = useMutation({
  mutationFn: (node) => apiDelete(`/api/resources/organizations/${node.id}`),
  onSuccess: (result) => {
    if (result.success) {
      showToast(t("organization.delete_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["organizations-tree"] });
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  }
});

function handleDeleteOrgConfirmed() {
  const node = deletingOrg.value;
  deletingOrg.value = null;
  if (node) {
    deleteOrgMutation.mutate(node);
  }
}

function invalidateTree() {
  queryClient.invalidateQueries({ queryKey: ["organizations-tree"] });
}

// 组织节点保存后可用类型可能变化（新根类型），同步失效可用类型缓存
function handleOrgSaved() {
  invalidateTree();
  queryClient.invalidateQueries({ queryKey: ["org-available-types"] });
}
</script>

<template>
  <section id="organization" class="content-section active" :aria-label="t('nav.organization')">
    <div class="org-toolbar">
      <div class="org-actions">
        <button type="button" class="btn btn-secondary btn-sm" @click="expandAll">
          {{ t("organization.expand_all") }}
        </button>
        <button type="button" class="btn btn-secondary btn-sm" @click="collapseAll">
          {{ t("organization.collapse_all") }}
        </button>
        <input
          v-model="searchInput"
          type="text"
          class="form-control org-search-input"
          :placeholder="t('organization.search_placeholder')"
          :aria-label="t('organization.search_placeholder')"
        >
      </div>
      <div class="org-actions">
        <button type="button" class="btn btn-primary btn-sm" @click="templateModalOpen = true">
          {{ t("org_template.management") }}
        </button>
        <button type="button" class="btn btn-primary btn-sm" @click="orgModal.mode = 'add-root'; orgModal.node = null; orgModal.parentId = ''; orgModal.open = true">
          {{ t("organization.add_node") }}
        </button>
      </div>
    </div>

    <div class="org-tree-container" role="tree" :aria-label="t('nav.organization')">
      <div v-if="treeQuery.isError.value" class="org-empty-state">{{ treeQuery.error.value?.message }}</div>
      <div v-else-if="treeData.length === 0 && !treeQuery.isLoading.value" class="org-empty-state">
        {{ t("organization.empty") }}
      </div>
      <!-- key 为渲染代际：数据变化/清空搜索时整树重挂载还原展开态 -->
      <div v-else :key="renderEpoch">
        <OrgTreeNode v-for="root in treeData" :key="root.id" :node="root" />
      </div>
    </div>

    <!-- 组织节点新增/编辑 -->
    <OrgNodeModal
      :open="orgModal.open"
      :mode="orgModal.mode"
      :node="orgModal.node"
      :parent-id="orgModal.parentId"
      @close="orgModal.open = false"
      @saved="handleOrgSaved"
    />

    <!-- 人员管理 -->
    <EmployeeModal
      :open="employeeModal.open"
      :org-id="employeeModal.orgId"
      :org-name="employeeModal.orgName"
      @close="employeeModal.open = false"
    />

    <!-- 层级模板管理 -->
    <TemplateModal :open="templateModalOpen" @close="templateModalOpen = false" />

    <!-- 组织节点删除确认 -->
    <ConfirmModal
      v-if="deletingOrg"
      :message="t('organization.delete_confirm', { name: deletingOrg.name })"
      danger
      @confirm="handleDeleteOrgConfirmed"
      @cancel="deletingOrg = null"
    />
  </section>
</template>

<!-- 样式对应旧版 pages/organization.css 的整体移植：组织树节点/徽章配色/
     模板编辑器/图标选择面板被页面、递归节点组件与弹窗共用，scoped 无法
     跨组件生效，故按旧版分层放置于页面层（类名 org-* 无全局冲突） -->
<style>
.org-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-md);
  margin-bottom: 0;
  flex-wrap: wrap;
}

.org-search-input {
  flex: 1;
  max-width: 320px;
  min-width: 200px;
  width: 100%;
}

.org-actions {
  display: flex;
  gap: var(--space-sm);
  flex-wrap: wrap;
}

.org-tree-container {
  background: var(--color-bg-white);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-sm);
  min-height: 400px;
  overflow-x: auto;
}

.org-empty-state {
  text-align: center;
  padding: var(--space-xl) var(--space-md);
  color: var(--color-text-muted);
  font-size: var(--font-size-base);
}

/* 入场动画（keyframes 对应旧版 utilities/animations.css 同名定义） */
@keyframes org-fade-in {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

/* ==================== 树节点 ==================== */

.org-node-wrapper {
  position: relative;
  animation: org-fade-in var(--transition-slow) ease;
}

.org-node {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
  padding: var(--space-sm) var(--space-md);
  border-radius: var(--radius-md);
  transition: background-color var(--transition-fast);
  cursor: default;
  border: 1px solid transparent;
  white-space: nowrap;
}

.org-node:hover {
  background-color: var(--color-bg-hover);
  border-color: var(--color-border-light);
}

/* 展开/折叠按钮 */
.org-toggle {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  cursor: pointer;
  color: var(--color-text-muted);
  transition: transform var(--transition-fast);
  flex-shrink: 0;
}

.org-toggle:hover {
  color: var(--color-primary);
}

.org-toggle-icon {
  transition: transform var(--transition-fast);
}

.org-toggle.expanded .org-toggle-icon {
  transform: rotate(90deg);
}

.org-toggle-placeholder {
  display: inline-block;
  width: 20px;
  flex-shrink: 0;
}

/* 节点图标：底衬以图标自身颜色生成浅色圆角背景，SVG 缩放至底衬内 */
.org-node-icon {
  font-size: 1.2rem;
  color: var(--color-text);
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}

.org-node-icon .org-icon,
.org-node-icon .org-icon-legacy {
  width: 28px;
  height: 28px;
  border-radius: var(--radius-sm);
  background: color-mix(in srgb, currentColor 12%, transparent);
}

.org-node-icon .org-icon svg {
  width: 18px;
  height: 18px;
}

.org-node-name {
  font-weight: 500;
  color: var(--color-text);
  font-size: var(--font-size-base);
}

.org-node-type-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: var(--radius-sm);
  font-size: var(--font-size-xs);
  font-weight: 600;
  background: var(--color-primary-light);
  color: var(--color-primary);
  flex-shrink: 0;
}

/* 节点类型徽章配色：底色统一取文字 token 的 10% 透明度（同文件 color-mix 模式） */
.org-node-type-badge[data-type="headquarters"] {
  background: color-mix(in srgb, var(--color-secondary-hover) 8%, transparent);
  /* 比主色深一档的 lime 文字：主色向文字色深化 */
  color: color-mix(in srgb, var(--color-primary) 85%, var(--color-text));
}

.org-node-type-badge[data-type="building"] {
  background: color-mix(in srgb, var(--color-blue) 10%, transparent);
  color: var(--color-blue);
}

.org-node-type-badge[data-type="floor"] {
  background: color-mix(in srgb, var(--color-info) 10%, transparent);
  color: var(--color-info);
}

.org-node-type-badge[data-type="data_center"] {
  background: color-mix(in srgb, var(--color-danger) 10%, transparent);
  color: var(--color-danger);
}

.org-node-type-badge[data-type="telecom_closet"],
.org-node-type-badge[data-type="cabinet"] {
  background: color-mix(in srgb, var(--color-ipv6) 10%, transparent);
  color: var(--color-ipv6);
}

.org-node-type-badge[data-type="office"] {
  background: color-mix(in srgb, var(--color-success) 10%, transparent);
  color: var(--color-success);
}

.org-node-type-badge[data-type="hall"] {
  background: color-mix(in srgb, var(--color-warning) 10%, transparent);
  color: var(--color-warning-text);
}

.org-node-type-badge[data-type="workstation"] {
  background: color-mix(in srgb, var(--color-secondary) 10%, transparent);
  color: var(--color-secondary);
}

.org-node-type-badge[data-type="cabinet_position"] {
  background: color-mix(in srgb, var(--color-orange) 10%, transparent);
  color: var(--color-orange);
}

.org-node-desc {
  color: var(--color-text-muted);
  font-size: var(--font-size-sm);
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 节点操作按钮：默认隐藏，悬停节点时显形 */
.org-node-actions {
  display: flex;
  gap: 4px;
  margin-left: auto;
  opacity: 0;
  transition: opacity var(--transition-fast);
  flex-shrink: 0;
}

.org-node:hover .org-node-actions {
  opacity: 1;
}

/* 树节点图标按钮形态：无边框、透明底，着色/悬浮由全局 buttons.css 提供 */
.icon-btn.btn-add-child {
  background: transparent;
  border: none;
}

.icon-btn.btn-employees {
  background: transparent;
  border: none;
  color: var(--color-primary);
}

.icon-btn.btn-employees:hover {
  background: color-mix(in srgb, var(--color-primary) 12%, transparent);
}

/* 子节点容器：嵌套 padding-left 逐层缩进 + 竖向引导线 */
.org-children {
  position: relative;
  padding-left: 24px;
}

.org-children::before {
  content: "";
  position: absolute;
  left: 28px;
  top: 0;
  bottom: 0;
  width: 1px;
  background: var(--color-border-light);
}

/* ==================== 人员管理弹窗 ==================== */

.employee-modal-org {
  color: var(--color-text-secondary);
  font-weight: 400;
  font-size: 0.9em;
  margin-left: 6px;
}

.employee-table-container {
  max-height: 380px;
  overflow: auto;
}

.employee-actions-cell {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--space-xs);
}

.modal-toolbar {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 8px;
}

/* 父级信息 */
.org-parent-info {
  background: var(--control-bg-muted);
  padding: var(--space-sm) var(--space-md);
  border-radius: var(--radius-md);
  margin-bottom: var(--space-md);
}

.org-parent-info p {
  margin: 0;
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}

.org-no-child-hint {
  margin: 0;
  font-size: var(--font-size-sm);
  color: var(--color-text-muted);
}

/* ==================== 模板管理 ==================== */

.org-template-toolbar {
  display: flex;
  justify-content: flex-end;
  margin-bottom: var(--space-md);
}

.org-template-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-sm);
  max-height: 60vh;
  overflow-y: auto;
}

.org-template-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-md);
  padding: var(--space-sm) var(--space-md);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-bg-white);
  transition: border-color var(--transition-fast);
}

.org-template-item:hover {
  border-color: var(--color-primary);
}

.org-template-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
  min-width: 0;
}

.org-template-info strong {
  font-size: var(--font-size-base);
  color: var(--color-text);
}

.org-template-levels {
  font-size: var(--font-size-sm);
  color: var(--color-primary);
  font-weight: 500;
}

.org-template-level-path {
  display: block;
}

.org-template-desc {
  font-size: var(--font-size-xs);
  color: var(--color-text-muted);
}

.org-template-actions {
  display: flex;
  gap: 4px;
  flex-shrink: 0;
}

/* ==================== 模板编辑器 ==================== */

.org-template-tree-container {
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  padding: var(--space-md);
  background: var(--color-bg-white);
  margin-bottom: var(--space-sm);
  min-height: 80px;
}

.org-template-type-node {
  display: flex;
  flex-direction: column;
  position: relative;
}

.org-template-type-row {
  display: flex;
  gap: var(--space-sm);
  align-items: center;
  padding: 3px 0;
}

.org-template-type-root > .org-template-type-row {
  background: var(--color-primary-light);
  padding: 6px 10px;
  border-radius: var(--radius-md);
  border: 1px solid color-mix(in srgb, var(--color-primary) 20%, transparent);
}

.org-template-root-badge {
  display: inline-block;
  padding: 2px 10px;
  border-radius: var(--radius-sm);
  font-size: var(--font-size-xs);
  font-weight: 600;
  background: var(--color-primary);
  color: var(--color-bg-white);
  flex-shrink: 0;
  letter-spacing: 1px;
}

/* 图标选择按钮（内嵌 SVG 图标） */
.org-template-icon-btn {
  font-size: 1.05rem;
  color: var(--color-text);
  flex-shrink: 0;
  width: 2em;
  height: 2em;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  background: var(--color-bg-white);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition:
    border-color var(--transition-fast),
    background-color var(--transition-fast);
  position: relative;
}

.org-template-icon-btn .org-icon,
.org-template-icon-btn .org-icon-legacy {
  width: 1.35em;
  height: 1.35em;
}

.org-template-icon-btn:hover {
  border-color: var(--color-primary);
  background: var(--color-primary-light);
}

/* 图标选择面板：按 业务组织/物理地点/功能空间 分组 */
.org-icon-picker {
  position: absolute;
  top: 100%;
  left: 0;
  z-index: var(--z-popover);
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px;
  width: 264px;
  max-height: 320px;
  overflow: auto;
  background: var(--color-bg-white);
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  margin-top: 4px;
}

.org-icon-picker-group-title {
  font-size: 0.72rem;
  color: var(--color-text-secondary);
  margin-bottom: 4px;
}

.org-icon-picker-grid {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 4px;
}

.org-icon-picker-item {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  aspect-ratio: 1;
  min-width: 0;
  padding: 0;
  font-size: 1.2rem;
  color: var(--color-text);
  background: transparent;
  border: none;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background-color var(--transition-fast);
}

.org-icon-picker-item .org-icon,
.org-icon-picker-item .org-icon-legacy {
  width: 1.35rem;
  height: 1.35rem;
}

.org-icon-picker-item:hover {
  background: var(--color-bg-hover);
}

.org-icon-picker-item.active {
  background: var(--color-primary-light);
  outline: 2px solid var(--color-primary);
}

.org-template-type-row .org-template-type-input {
  flex: 1;
  min-width: 0;
}

/* 行内「添加子层级/删除」图标按钮：统一 icon-btn 尺寸，仅保留防挤压 */
.org-template-type-row .icon-btn.btn-add-child,
.org-template-type-row .icon-btn.btn-delete {
  flex-shrink: 0;
  width: 28px;
  min-width: 28px;
  height: 28px;
}

.org-template-type-children {
  margin-left: 20px;
  padding-left: 16px;
  border-left: 2px solid
    color-mix(in srgb, var(--color-primary) 25%, transparent);
  margin-top: 6px;
  margin-bottom: 4px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

/* 子节点悬停时高亮整条引导线 */
.org-template-type-children:hover {
  border-left-color: var(--color-primary);
}

/* ==================== 响应式 ==================== */

@media (width <= 768px) {
  .org-toolbar {
    flex-direction: column;
    align-items: stretch;
  }

  .org-actions {
    justify-content: flex-start;
  }

  .org-node {
    flex-wrap: wrap;
    padding: var(--space-xs) var(--space-sm);
  }

  .org-node-actions {
    opacity: 1;
    flex-wrap: wrap;
  }

  .org-node-desc {
    display: none;
  }
}
</style>
