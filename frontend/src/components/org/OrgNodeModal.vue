<script setup>
/**
 * 组织节点编辑弹窗（移植自旧版 openOrgModal + organization-modal.html +
 * submitOrgForm）：编辑模式仅改名称/描述（类型由模板决定不可改）；新增
 * 下级时类型由 allowed-child-types 接口决定；新增根节点需先选层级模板，
 * 选中后根类型自动锁定为模板根（type_path 固定 "0"）。
 */
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery } from "@tanstack/vue-query";
import { apiGet, apiPost, apiPut } from "../../api/client.js";
import { showToast } from "../../composables/useToast.js";
import { findRootType, getLevelPaths, getOrgTypeLabel } from "../../config/orgConfig.js";
import ModalShell from "../ModalShell.vue";

/** 模板下拉选项副行：levels 映射展开为可读路径（" / " 连接单行展示） */
function templateLevelText(tpl) {
  return getLevelPaths(tpl.levels).join(" / ");
}

const props = defineProps({
  open: { type: Boolean, default: false },
  /** edit | add-child | add-root */
  mode: { type: String, default: "add-root" },
  /** 编辑模式传入树节点（含 name/description/org_type） */
  node: { type: Object, default: null },
  /** 新增下级时传入父节点 ID */
  parentId: { type: String, default: "" }
});

const emit = defineEmits(["close", "saved"]);

const { t } = useI18n();

const form = reactive({ name: "", description: "", typeId: "", templateId: "" });
const rootLabel = ref("");

// ==================== 查询：可选下级类型 / 模板列表 ====================

const allowedQuery = useQuery({
  queryKey: computed(() => ["org-allowed-child-types", props.parentId]),
  queryFn: async () => {
    const result = await apiGet(`/api/resources/organizations/${props.parentId}/allowed-child-types`);
    if (!result.success) {
      throw new Error(result.message || t("common.operation_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.open && props.mode === "add-child" && Boolean(props.parentId))
});

const templatesQuery = useQuery({
  queryKey: ["org-templates"],
  queryFn: async () => {
    const result = await apiGet("/api/resources/org-templates");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return Array.isArray(result.data?.items) ? result.data.items : [];
  },
  enabled: computed(() => props.open && props.mode === "add-root")
});

const typeOptions = computed(() => {
  if (props.mode !== "add-child") {
    return [];
  }
  return Array.isArray(allowedQuery.data.value?.allowed_child_types)
    ? allowedQuery.data.value.allowed_child_types
    : [];
});

/** 编辑模式类型展示（单选项，不可修改） */
const editTypeLabel = computed(() =>
  props.mode === "edit" ? getOrgTypeLabel(props.node?.org_type || "") : ""
);

// ==================== 打开时重置表单 ====================

watch(
  () => props.open,
  (open) => {
    if (!open) {
      return;
    }
    form.name = props.node?.name ?? "";
    form.description = props.node?.description ?? "";
    form.typeId = props.mode === "edit" ? (props.node?.org_type || "") : "";
    form.templateId = "";
    rootLabel.value = "";
  }
);

/** 模板选择变化：根类型自动锁定为模板根（type_path 固定 "0"） */
function onTemplateChange() {
  const selected = templatesQuery.data.value?.find((tpl) => tpl.id === form.templateId);
  const rootType = selected ? findRootType(selected.levels) : null;
  if (rootType) {
    form.typeId = "0";
    rootLabel.value = getOrgTypeLabel(rootType);
  } else {
    form.typeId = "";
    rootLabel.value = "";
  }
}

// ==================== 提交 ====================

const saveMutation = useMutation({
  mutationFn: async () => {
    if (props.mode === "edit") {
      // 编辑模式：仅更新名称/描述，类型由模板决定不可修改
      return apiPut(`/api/resources/organizations/${props.node.id}`, {
        name: form.name.trim(),
        description: form.description || null
      });
    }
    const body = {
      name: form.name.trim(),
      type_path: form.typeId,
      parent_id: props.mode === "add-child" ? props.parentId : null,
      description: form.description || null
    };
    if (props.mode === "add-root") {
      body.template_id = form.templateId;
    }
    return apiPost("/api/resources/organizations", body);
  },
  onSuccess: (result) => {
    if (result.success) {
      showToast(props.mode === "edit" ? t("organization.update_success") : t("organization.create_success"), "success");
      emit("saved");
      emit("close");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  }
});

function handleSubmit() {
  if (!form.name.trim()) {
    showToast(t("organization.name_required"), "warning");
    return;
  }
  if (props.mode !== "edit" && !form.typeId) {
    showToast(t("organization.type_required"), "warning");
    return;
  }
  // 新增根节点时必须选择模板
  if (props.mode === "add-root" && !form.templateId) {
    showToast(t("org_template.select_required"), "warning");
    return;
  }
  saveMutation.mutate();
}

const saving = computed(() => saveMutation.isPending.value);

const modalTitle = computed(() => {
  if (props.mode === "edit") {
    return t("organization.edit");
  }
  return props.mode === "add-child" ? t("organization.add") : t("organization.add_node");
});
</script>

<template>
  <ModalShell v-if="open" :title="modalTitle" size="md" @close="emit('close')">
    <form @submit.prevent="handleSubmit">
      <!-- 新增根节点：先选层级模板 -->
      <div v-if="mode === 'add-root'" class="form-group">
        <label for="org-template-select">
          <span>{{ t("org_template.select_template") }}</span><span class="required">*</span>
        </label>
        <select id="org-template-select" v-model="form.templateId" @change="onTemplateChange">
          <option value="">{{ t("org_template.select_template") }}</option>
          <option v-for="tpl in templatesQuery.data.value" :key="tpl.id" :value="tpl.id">
            {{ tpl.name }} ({{ templateLevelText(tpl) }})
          </option>
        </select>
      </div>

      <div class="form-row">
        <div class="form-group">
          <label for="org-name">
            <span>{{ t("common.name") }}</span><span class="required">*</span>
          </label>
          <input id="org-name" v-model="form.name" type="text" autocomplete="off" required>
        </div>
        <div class="form-group">
          <label for="org-type">
            <span>{{ t("organization.type") }}</span><span class="required">*</span>
          </label>
          <!-- 编辑模式：类型不可修改，单选项回显 -->
          <select v-if="mode === 'edit'" id="org-type" disabled>
            <option :value="form.typeId" selected>{{ editTypeLabel }}</option>
          </select>
          <!-- 新增根节点：选中模板后锁定根类型 -->
          <select v-else-if="mode === 'add-root'" id="org-type" :value="form.typeId" disabled>
            <option v-if="form.typeId" :value="form.typeId">{{ rootLabel }}</option>
            <option v-else value="">{{ t("organization.select_type") }}</option>
          </select>
          <!-- 新增下级：类型由父节点模板决定 -->
          <select v-else id="org-type" v-model="form.typeId" :disabled="typeOptions.length === 0">
            <option value="">{{ t("organization.select_type") }}</option>
            <option v-for="opt in typeOptions" :key="opt.type_path || opt.type" :value="opt.type_path || opt.type">
              {{ opt.label || getOrgTypeLabel(opt.type) }}
            </option>
          </select>
        </div>
      </div>

      <div class="form-group form-group-block">
        <textarea
          v-model="form.description"
          :aria-label="t('common.description')"
          :placeholder="t('common.description_placeholder')"
        ></textarea>
      </div>

      <!-- 新增下级：父级信息提示 -->
      <div v-if="mode === 'add-child' && allowedQuery.data.value" class="org-parent-info">
        <p>
          <span>{{ t("organization.parent") }}</span>:
          <strong>{{ allowedQuery.data.value.parent_name }}</strong>
          ({{ getOrgTypeLabel(allowedQuery.data.value.parent_type) }})
        </p>
      </div>
      <p v-if="mode === 'add-child' && typeOptions.length === 0 && !allowedQuery.isLoading.value" class="org-no-child-hint">
        {{ t("organization.no_child_allowed") }}
      </p>
    </form>

    <template #footer>
      <button type="button" class="btn-primary" :disabled="saving" @click="handleSubmit">
        {{ saving ? t("common.saving") : t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
