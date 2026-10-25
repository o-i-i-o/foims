<script setup lang="ts">
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
import { apiDelete, apiGet } from "../api/client";
import { showToast } from "../composables/useToast";
import { resolveOrgIcon } from "../config/orgConfig";
import type { Paged } from "../types/api";
import type { OrganizationTreeNode } from "../types/api/generated/OrganizationTreeNode";
import type { OrgTemplate } from "../types/api/generated/OrgTemplate";
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
  queryFn: async (): Promise<OrganizationTreeNode[]> => {
    const result = await apiGet<OrganizationTreeNode[]>("/api/resources/organizations/tree");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return Array.isArray(result.data) ? result.data : [];
  }
});

const treeData = computed<OrganizationTreeNode[]>(() =>
  treeQuery.isError.value ? [] : (treeQuery.data.value ?? [])
);

const availableTypesQuery = useQuery({
  queryKey: ["org-available-types"],
  queryFn: async (): Promise<{ types: string[]; icons: Record<string, string> }> => {
    const result = await apiGet<{ types?: string[]; icons?: Record<string, string> }>(
      "/api/resources/org-templates/available-types"
    );
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
  queryFn: async (): Promise<OrgTemplate[]> => {
    const result = await apiGet<Paged<OrgTemplate>>("/api/resources/org-templates");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return Array.isArray(result.data?.items) ? result.data.items : [];
  }
});

/** 模板图标映射 { templateId: { type_name: icon } }（同名类型互不覆盖） */
const templateIconsById = computed<Record<string, Record<string, string>>>(() => {
  const map: Record<string, Record<string, string>> = {};
  for (const tpl of templatesQuery.data.value ?? []) {
    // icons 为 JsonValue，模板编辑器保存的对象形态（type → 图标名）按对象收窄
    if (tpl.icons && typeof tpl.icons === "object" && !Array.isArray(tpl.icons)) {
      map[tpl.id] = tpl.icons as Record<string, string>;
    }
  }
  return map;
});

/** 节点图标解析：所属模板 icons → API icons → 关键词匹配 → 默认 */
function resolveIcon(orgType: unknown, templateId: string | null): string {
  return resolveOrgIcon(orgType, templateIconsById.value[templateId ?? ""] ?? {}, availableTypesQuery.data.value?.icons ?? {});
}

// ==================== 搜索（客户端过滤，防抖 300ms） ====================

const searchInput = ref("");
const searchKeyword = ref("");
let searchTimer: number | undefined;

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
  const visible = new Set<string>();
  const expand = new Set<string>();
  function walk(nodes: OrganizationTreeNode[]): boolean {
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

const orgModal = reactive<{ open: boolean; mode: string; node: OrganizationTreeNode | null; parentId: string }>({
  open: false,
  mode: "add-root",
  node: null,
  parentId: ""
});
const employeeModal = reactive({ open: false, orgId: "", orgName: "" });
const templateModalOpen = ref(false);
const deletingOrg = ref<OrganizationTreeNode | null>(null);

provide("orgTreeCtx", {
  keyword: searchKeyword,
  allExpanded,
  search: searchState,
  resolveIcon,
  onAction
});

function onAction(node: OrganizationTreeNode, action: string): void {
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
  mutationFn: (node: OrganizationTreeNode) => apiDelete(`/api/resources/organizations/${node.id}`),
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

