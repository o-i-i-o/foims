<script setup>
/**
 * 层级模板管理弹窗（移植自旧版 openTemplateManagement + org-template-modal
 * + openTemplateEditor + org-template-editor-modal）：模板列表（名称/层级
 * 路径/描述）与树形编辑器（层级类型树 + 图标选择 + 快捷填充）。层级以响应
 * 式树数据维护（替代旧版 DOM 收集），提交前校验类型名全局唯一（同名会静默
 * 覆盖子级）。模板变更影响节点类型解析，保存/删除后同步失效组织树缓存。
 */
import { computed, onBeforeUnmount, onMounted, provide, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiGet, apiPost, apiPut } from "../../api/client.js";
import { showToast } from "../../composables/useToast.js";
import { getLevelPaths } from "../../config/orgConfig.js";
import NavIcon from "../NavIcon.vue";
import ConfirmModal from "../ConfirmModal.vue";
import ModalShell from "../ModalShell.vue";
import TemplateTypeNode from "./TemplateTypeNode.vue";

const props = defineProps({
  open: { type: Boolean, default: false }
});

const emit = defineEmits(["close"]);

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 模板列表 ====================

const templatesQuery = useQuery({
  queryKey: ["org-templates"],
  queryFn: async () => {
    const result = await apiGet("/api/resources/org-templates");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return Array.isArray(result.data?.items) ? result.data.items : [];
  },
  enabled: computed(() => props.open)
});

// ==================== 层级树数据 ====================

// 节点唯一标识（图标面板开合按 uid 定位）
let uidSeq = 0;

function createTypeNode(type = "", icon = "") {
  uidSeq += 1;
  return { uid: uidSeq, type, icon, children: [] };
}

/** 将 levels 映射重建为树形数据（icons 可选） */
function buildTreeFromMapping(levels, icons = {}) {
  const rootType = levels && typeof levels === "object" && !Array.isArray(levels)
    ? Object.keys(levels).find((key) => {
        const allChildren = new Set(Object.values(levels).flat());
        return !allChildren.has(key);
      })
    : null;
  if (!rootType) {
    return [createTypeNode()];
  }
  function build(type, isRoot) {
    const node = createTypeNode(type, icons[type] || "");
    (levels[type] || []).forEach((childType) => {
      node.children.push(build(childType, false));
    });
    return node;
  }
  return [build(rootType, true)];
}

/** 收集 levels 映射（空类型名节点跳过，子级取已填写的子类型名） */
function collectLevels(nodes, map = {}) {
  for (const node of nodes) {
    if (!node.type) {
      continue;
    }
    map[node.type] = node.children.filter((child) => child.type).map((child) => child.type);
    collectLevels(node.children, map);
  }
  return map;
}

/** 收集图标映射（含默认图标，与旧版 collectIcons 一致） */
function collectIcons(nodes, icons = {}) {
  for (const node of nodes) {
    if (node.type) {
      icons[node.type] = node.icon;
    }
    collectIcons(node.children, icons);
  }
  return icons;
}

/** 校验类型名全局唯一（collectMapping 以类型名作 key：同名静默覆盖会丢失子级） */
function findDuplicateTypeName(nodes) {
  const seen = new Set();
  let duplicate = null;
  function walk(list) {
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
const editor = reactive({ id: "", name: "", description: "", quickFill: "", nodes: [], activePicker: null });

provide("orgTplEditorCtx", {
  activePicker: computed({
    get: () => editor.activePicker,
    set: (value) => {
      editor.activePicker = value;
    }
  }),
  addChild(parent) {
    parent.children.push(createTypeNode());
  },
  removeNode(target) {
    // 从全树定位包含目标节点的子级数组并移除
    function walk(list) {
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
function onDocumentClick() {
  editor.activePicker = null;
}

onMounted(() => {
  document.addEventListener("click", onDocumentClick);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick);
});

function openEditor(template = null) {
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
    const data = {
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

function handleSubmit() {
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

const deletingTemplate = ref(null);

const deleteMutation = useMutation({
  mutationFn: (template) => apiDelete(`/api/resources/org-templates/${template.id}`),
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

function handleDeleteConfirmed() {
  const template = deletingTemplate.value;
  deletingTemplate.value = null;
  if (template) {
    deleteMutation.mutate(template);
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
