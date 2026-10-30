<script setup lang="ts">
/**
 * 设备模板管理弹窗：
 * - 模板列表：名称/类型/品牌/型号，行内操作编辑与删除；
 * - 行内编辑：名称必填（name_required），类型下拉复用 9 个 device_type.* key，
 *   品牌/型号/描述可空；保存走 PUT /device-templates/{id}；
 * - 删除：ConfirmModal 确认后 DELETE /device-templates/{id}；
 * - 保存/删除成功后失效列表缓存并 emit("changed")，供父弹窗刷新模板下拉。
 */
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiGet, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import { DEVICE_TYPE_OPTIONS, getDeviceTypeName } from "../../config/deviceOptions";
import ConfirmModal from "../ConfirmModal.vue";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { DeviceTemplateSummary } from "../../types/api/generated/DeviceTemplateSummary";
import type { UpdateDeviceTemplateRequest } from "../../types/api/generated/UpdateDeviceTemplateRequest";

const emit = defineEmits<{ close: []; changed: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

/** 列表行：后端 DeviceTemplateSummary 不返回 description，行内编辑的描述框恒为空（原行为保留） */
type TemplateRow = DeviceTemplateSummary & { description?: string | null };

const templatesQuery = useQuery({
  queryKey: ["device-templates"],
  queryFn: async (): Promise<TemplateRow[]> => {
    const result = await apiGet<{ items?: TemplateRow[] }>("/api/resources/device-templates");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return Array.isArray(result.data?.items) ? result.data.items : [];
  }
});

const templates = computed(() => templatesQuery.data.value ?? []);

const editingId = ref("");
const editForm = reactive({ name: "", deviceType: "other", brand: "", model: "", description: "" });

function openEditor(tmpl: TemplateRow): void {
  editingId.value = tmpl.id;
  editForm.name = tmpl.name || "";
  editForm.deviceType = tmpl.device_type || "other";
  editForm.brand = tmpl.brand || "";
  editForm.model = tmpl.model || "";
  editForm.description = tmpl.description || "";
}

function cancelEdit(): void {
  editingId.value = "";
}

const updateMutation = useMutation({
  mutationFn: async ({ id }: { id: string }) => {
    const name = editForm.name.trim();
    if (!name) {
      showToast(t("device_template.name_required"), "warning");
      return null;
    }
    const data: UpdateDeviceTemplateRequest = {
      name,
      device_type: editForm.deviceType || "other",
      brand: editForm.brand.trim() || null,
      model: editForm.model.trim() || null,
      description: editForm.description.trim() || null
    };
    const result = await apiPut(`/api/resources/device-templates/${id}`, data);
    if (!result.success) {
      throw new Error(result.message || t("common.failed"));
    }
    return result;
  },
  onSuccess: (result) => {
    if (!result) {
      return; // 校验未通过，已提示
    }
    showToast(t("device_template.update_success"), "success");
    cancelEdit();
    queryClient.invalidateQueries({ queryKey: ["device-templates"] });
    emit("changed");
  },
  onError: (error) => {
    showToast(`${t("common.operation_failed")}: ${error.message}`, "error");
  }
});

const deleting = ref<TemplateRow | null>(null);

const deleteMutation = useMutation({
  mutationFn: async (id: string) => {
    const result = await apiDelete(`/api/resources/device-templates/${id}`);
    if (!result.success) {
      throw new Error(result.message || t("common.failed"));
    }
    return result;
  },
  onSuccess: () => {
    showToast(t("device_template.delete_success"), "success");
    deleting.value = null;
    queryClient.invalidateQueries({ queryKey: ["device-templates"] });
    emit("changed");
  },
  onError: (error) => {
    showToast(`${t("common.delete_failed")}: ${error.message}`, "error");
  }
});
</script>

<template>
  <ModalShell :title="t('device_template.title')" size="lg" @close="emit('close')">
    <div class="device-template-list">
      <p v-if="templatesQuery.isLoading.value" class="empty-hint">{{ t("common.loading") }}</p>
      <p v-else-if="templates.length === 0" class="empty-hint">{{ t("device_template.empty") }}</p>
      <template v-else>
        <!-- 编辑态：行内容器原位切换为编辑表单 -->
        <div v-for="tmpl in templates" :key="tmpl.id" class="device-template-item">
          <template v-if="editingId === tmpl.id">
            <form class="device-template-edit-form" @submit.prevent="updateMutation.mutate({ id: tmpl.id })">
              <div class="form-row">
                <div class="form-group">
                  <label>{{ t("common.name") }}<span class="required">*</span></label>
                  <input v-model="editForm.name" type="text" class="nc-input" />
                </div>
                <div class="form-group">
                  <label>{{ t("device.device_type") }}</label>
                  <select v-model="editForm.deviceType" class="nc-input">
                    <option v-for="opt in DEVICE_TYPE_OPTIONS" :key="opt.value" :value="opt.value">
                      {{ t(opt.labelKey) }}
                    </option>
                  </select>
                </div>
              </div>
              <div class="form-row">
                <div class="form-group">
                  <label>{{ t("device.brand") }}</label>
                  <input v-model="editForm.brand" type="text" class="nc-input" />
                </div>
                <div class="form-group">
                  <label>{{ t("device.model") }}</label>
                  <input v-model="editForm.model" type="text" class="nc-input" />
                </div>
              </div>
              <div class="form-group">
                <label>{{ t("common.description") }}</label>
                <textarea v-model="editForm.description" class="nc-input" rows="2"></textarea>
              </div>
              <div class="device-template-edit-actions">
                <button
                  type="button"
                  class="icon-btn btn-secondary"
                  :title="t('common.cancel')"
                  :aria-label="t('common.cancel')"
                  @click="cancelEdit"
                >
                  <NavIcon name="x" />
                </button>
                <button
                  type="button"
                  class="icon-btn btn-primary"
                  :title="t('common.save')"
                  :aria-label="t('common.save')"
                  :disabled="updateMutation.isPending.value"
                  @click="updateMutation.mutate({ id: tmpl.id })"
                >
                  <NavIcon name="check" />
                </button>
              </div>
            </form>
          </template>
          <template v-else>
            <div class="device-template-info">
              <span class="device-template-name">{{ tmpl.name }}</span>
              <span class="device-template-meta">
                <span class="device-template-type">{{ getDeviceTypeName(t, tmpl.device_type) }}</span>
                <span v-if="tmpl.brand" class="device-template-brand">{{ tmpl.brand }}</span>
                <span v-if="tmpl.model" class="device-template-model">{{ tmpl.model }}</span>
              </span>
            </div>
            <div class="device-template-actions">
              <button
                type="button"
                class="icon-btn btn-secondary"
                :title="t('common.edit')"
                :aria-label="t('common.edit')"
                @click="openEditor(tmpl)"
              >
                <NavIcon name="edit" />
              </button>
              <button
                type="button"
                class="icon-btn btn-danger"
                :title="t('common.delete')"
                :aria-label="t('common.delete')"
                @click="deleting = tmpl"
              >
                <NavIcon name="trash" />
              </button>
            </div>
          </template>
        </div>
      </template>
    </div>

    <!-- 删除确认 -->
    <ConfirmModal
      v-if="deleting"
      :message="t('common.confirm_delete', { name: deleting.name || '' })"
      danger
      @confirm="deleteMutation.mutate(deleting.id)"
      @cancel="deleting = null"
    />
  </ModalShell>
</template>
