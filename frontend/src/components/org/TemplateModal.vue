<script setup lang="ts">
/**
 * 层级模板管理弹窗：模板列表（名称/层级
 * 路径/描述）与树形编辑器（层级类型树 + 图标选择 + 快捷填充）。层级以响应
 * 式树数据维护，提交前校验类型名全局唯一（同名会静默
 * 覆盖子级）。模板变更影响节点类型解析，保存/删除后同步失效组织树缓存。
 */
import { computed, onBeforeUnmount, onMounted, provide, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiGet, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import { getLevelPaths } from "../../config/orgConfig";
import NavIcon from "../NavIcon.vue";
import ConfirmModal from "../ConfirmModal.vue";
import ModalShell from "../ModalShell.vue";
import TemplateTypeNode from "./TemplateTypeNode.vue";
import type { ApiResult } from "../../types/api";
import type { OrgTemplateCreate } from "../../types/api/generated/OrgTemplateCreate";
import type { OrgTemplateSummary } from "../../types/api/generated/OrgTemplateSummary";

const props = withDefaults(defineProps<{ open?: boolean }>(), { open: false });

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 模板列表 ====================

const templatesQuery = useQuery({
  queryKey: ["org-templates"],
  queryFn: async (): Promise<OrgTemplateSummary[]> => {
    const result = await apiGet<{ items?: OrgTemplateSummary[] }>("/api/resources/org-templates");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    // 原实现以 Array.isArray 防御非数组响应；类型保证下与 ?? [] 等价
    return result.data?.items ?? [];
  },
  enabled: computed(() => props.open)
});

// ==================== 层级树数据 ====================

// 节点唯一标识（图标面板开合按 uid 定位）
let uidSeq = 0;

/** 编辑器层级树节点（与 TemplateTypeNode 的节点形状同构） */
interface TplTypeNode {
  uid: number;
  type: string;
  icon: string;
  children: TplTypeNode[];
}

function createTypeNode(type = "", icon = ""): TplTypeNode {
  uidSeq += 1;
  return { uid: uidSeq, type, icon, children: [] };
}

/** 将 levels 映射重建为树形数据（icons 为类型名 → 图标 key 映射，可缺省） */
function buildTreeFromMapping(levels: unknown, icons: unknown = {}): TplTypeNode[] {
  // 已校验为非数组对象，按映射形状消费（与 orgConfig.findRootType 同口径）
  const levelMap =
    levels && typeof levels === "object" && !Array.isArray(levels)
      ? (levels as Record<string, unknown>)
      : null;
  // 全体子级引用集合：未被引用的 key 即根类型（原实现在 find 回调内重算，幂等故上提）
  const allChildren = new Set<unknown>(levelMap ? Object.values(levelMap).flat() : []);
  const rootType = levelMap ? Object.keys(levelMap).find((key) => !allChildren.has(key)) : null;
  if (!rootType) {
    return [createTypeNode()];
  }
  // icons 同样按映射形状消费（调用侧已将空值兜为空对象）
  const iconMap =
    icons && typeof icons === "object" && !Array.isArray(icons)
      ? (icons as Record<string, string>)
      : {};
  function build(type: string): TplTypeNode {
    const node = createTypeNode(type, iconMap[type] || "");
    const children = levelMap?.[type];
    (Array.isArray(children) ? children : []).forEach((childType) => {
      node.children.push(build(childType));
    });
    return node;
  }
  return [build(rootType)];
}

/** 收集 levels 映射（空类型名节点跳过，子级取已填写的子类型名） */
function collectLevels(
  nodes: TplTypeNode[],
  map: Record<string, string[]> = {}
): Record<string, string[]> {
  for (const node of nodes) {
    if (!node.type) {
      continue;
    }
    map[node.type] = node.children.filter((child) => child.type).map((child) => child.type);
    collectLevels(node.children, map);
  }
  return map;
}

/** 收集图标映射（含默认图标） */
function collectIcons(
  nodes: TplTypeNode[],
  icons: Record<string, string> = {}
): Record<string, string> {
  for (const node of nodes) {
    if (node.type) {
      icons[node.type] = node.icon;
    }
    collectIcons(node.children, icons);
  }
  return icons;
}

/** 校验类型名全局唯一（collectMapping 以类型名作 key：同名静默覆盖会丢失子级） */
function findDuplicateTypeName(nodes: TplTypeNode[]): string | null {
  const seen = new Set<string>();
  let duplicate: string | null = null;
  function walk(list: TplTypeNode[]): void {
    for (const node of list) {
      if (duplicate) {
        return;
      }
      if (node.type) {
        if (seen.has(node.type)) {
          duplicate = node.type;
          return;
        }
        seen.add(node.type);
      }
      walk(node.children);
    }
  }
  walk(nodes);
  return duplicate;
}

// ==================== 编辑器状态 ====================

const editorOpen = ref(false);

/** 编辑器状态：nodes 为层级树，activePicker 为图标面板打开的节点 uid */
interface EditorState {
  id: string;
  name: string;
  description: string;
  quickFill: string;
  nodes: TplTypeNode[];
  activePicker: number | null;
}

const editor = reactive<EditorState>({
  id: "",
  name: "",
  description: "",
  quickFill: "",
  nodes: [],
  activePicker: null
});

provide("orgTplEditorCtx", {
  activePicker: computed({
    get: () => editor.activePicker,
    set: (value) => {
      editor.activePicker = value;
    }
  }),
  addChild(parent: TplTypeNode): void {
    parent.children.push(createTypeNode());
  },
  removeNode(target: TplTypeNode): void {
    // 从全树定位包含目标节点的子级数组并移除
    function walk(list: TplTypeNode[]): boolean {
      const index = list.indexOf(target);
      if (index !== -1) {
        list.splice(index, 1);
        return true;
      }
      return list.some((node) => walk(node.children));
    }
    walk(editor.nodes);
  }
});

// 点击编辑器任意位置外区域关闭图标面板
function onDocumentClick(): void {
  editor.activePicker = null;
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick);
});

function openEditor(template: OrgTemplateSummary | null = null): void {
  editor.id = template?.id ?? "";
  editor.name = template?.name ?? "";
  editor.description = template?.description ?? "";
  editor.quickFill = "";
  editor.activePicker = null;
  editor.nodes = template ? buildTreeFromMapping(template.levels, template.icons || {}) : [createTypeNode()];
  editorOpen.value = true;
}

/** 快捷填充：从数据库已有模板载入层级与图标 */
watch(
  () => editor.quickFill,
  (name) => {
    if (!name) {
      return;
    }
    const selected = templatesQuery.data.value?.find((tpl) => tpl.name === name);
    if (selected) {
      editor.nodes = buildTreeFromMapping(selected.levels, selected.icons || {});
    }
    editor.quickFill = "";
  }
);

// ==================== 提交与删除 ====================

const saveMutation = useMutation({
  mutationFn: () => {
    const levels = collectLevels(editor.nodes);
    const icons = collectIcons(editor.nodes);
    const data: OrgTemplateCreate = {
      name: editor.name.trim(),
      levels,
      icons: Object.keys(icons).length > 0 ? icons : null,
      description: editor.description || null
    };
    if (editor.id) {
      return apiPut(`/api/resources/org-templates/${editor.id}`, data);
    }
    return apiPost("/api/resources/org-templates", data);
  },
  onSuccess: (result) => {
    if (result.success) {
      showToast(editor.id ? t("org_template.update_success") : t("org_template.create_success"), "success");
      editorOpen.value = false;
      // 模板变更可能影响节点类型解析（重命名/层级变化），同步失效组织树保证徽标一致
      queryClient.invalidateQueries({ queryKey: ["org-templates"] });
      queryClient.invalidateQueries({ queryKey: ["organizations-tree"] });
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  }
});

function handleSubmit(): void {
  if (!editor.name.trim()) {
    showToast(t("org_template.name_required"), "warning");
    return;
  }
  const duplicateType = findDuplicateTypeName(editor.nodes);
  if (duplicateType) {
    showToast(t("org_template.type_duplicate", { name: duplicateType }), "warning");
    return;
  }
  const levels = collectLevels(editor.nodes);
  if (Object.keys(levels).length === 0) {
    showToast(t("org_template.levels_required"), "warning");
    return;
  }
  saveMutation.mutate();
}

const deletingTemplate = ref<OrgTemplateSummary | null>(null);

const deleteMutation = useMutation({
  // TVariables 仅取 { id }：OrgTemplateSummary 携带递归 JsonValue（levels/icons），
  // 参与 vue-query 泛型自动展开会触发 TS2589 实例化过深
  mutationFn: (template: { id: string }): Promise<ApiResult<unknown>> =>
    apiDelete(`/api/resources/org-templates/${template.id}`),
  onSuccess: (result) => {
    if (result.success) {
      showToast(t("org_template.delete_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["org-templates"] });
      queryClient.invalidateQueries({ queryKey: ["organizations-tree"] });
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  }
});

function handleDeleteConfirmed(): void {
  const template = deletingTemplate.value;
  deletingTemplate.value = null;
  if (template) {
    deleteMutation.mutate({ id: template.id });
  }
}

const saving = computed(() => saveMutation.isPending.value);
</script>

<template>
  <ModalShell v-if="open" :title="t('org_template.management')" @close="emit('close')">
    <div class="org-template-toolbar">
      <button type="button" class="icon-btn btn-success" :title="t('org_template.add')" :aria-label="t('org_template.add')" @click="openEditor(null)">
        <NavIcon name="plus" />
      </button>
    </div>

    <div class="org-template-list">
      <p v-if="templatesQuery.isError.value" class="org-empty-state">{{ templatesQuery.error.value?.message }}</p>
      <p v-else-if="templatesQuery.data.value?.length === 0" class="org-empty-state">{{ t("org_template.empty") }}</p>
      <template v-else>
        <div v-for="tpl in templatesQuery.data.value" :key="tpl.id" class="org-template-item">
          <div class="org-template-info">
            <strong>{{ tpl.name }}</strong>
            <span class="org-template-levels">
              <span v-for="(pathText, index) in getLevelPaths(tpl.levels)" :key="index" class="org-template-level-path">{{ pathText }}</span>
            </span>
            <span v-if="tpl.description" class="org-template-desc">{{ tpl.description }}</span>
          </div>
          <div class="org-template-actions">
            <button type="button" class="icon-btn btn-edit" :title="t('common.edit')" :aria-label="t('common.edit')" @click="openEditor(tpl)">
              <NavIcon name="edit" />
            </button>
            <button type="button" class="icon-btn btn-delete" :title="t('common.delete')" :aria-label="t('common.delete')" @click="deletingTemplate = tpl">
              <NavIcon name="trash" />
            </button>
          </div>
        </div>
      </template>
    </div>

    <!-- 模板编辑器子弹窗 -->
    <ModalShell v-if="editorOpen" :title="t('org_template.editor_title')" size="md" @close="editorOpen = false">
      <form @submit.prevent="handleSubmit">
        <div class="form-group">
          <label for="org-template-editor-name">
            <span>{{ t("common.name") }}</span><span class="required">*</span>
          </label>
          <input id="org-template-editor-name" v-model="editor.name" type="text" autocomplete="off" required>
        </div>
        <div class="form-group">
          <label for="org-template-quick-fill">{{ t("org_template.quick_fill") }}</label>
          <select id="org-template-quick-fill" v-model="editor.quickFill">
            <option value="">{{ t("org_template.select_quick_fill") }}</option>
            <option v-for="tpl in templatesQuery.data.value" :key="tpl.id" :value="tpl.name">{{ tpl.name }}</option>
          </select>
        </div>
        <div class="form-group form-group-block">
          <!-- 层级树为动态容器、无单一控件可关联：分组标题替代 label -->
          <span class="group-label">{{ t("org_template.levels") }}</span>
          <div class="org-template-tree-container">
            <TemplateTypeNode v-for="node in editor.nodes" :key="node.uid" :node="node" :is-root="true" />
          </div>
        </div>
        <div class="form-group form-group-block">
          <textarea
            v-model="editor.description"
            :aria-label="t('common.description')"
            :placeholder="t('common.description_placeholder')"
          ></textarea>
        </div>
      </form>
      <template #footer>
        <button type="button" class="btn-primary" :disabled="saving" @click="handleSubmit">
          {{ saving ? t("common.saving") : t("common.save") }}
        </button>
      </template>
    </ModalShell>

    <!-- 删除确认 -->
    <ConfirmModal
      v-if="deletingTemplate"
      :message="t('org_template.delete_confirm', { name: deletingTemplate.name })"
      danger
      @confirm="handleDeleteConfirmed"
      @cancel="deletingTemplate = null"
    />
  </ModalShell>
</template>
