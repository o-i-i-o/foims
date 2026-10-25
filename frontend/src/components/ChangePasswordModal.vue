<script setup lang="ts">
/**
 * 修改密码模态框。
 * 等保要求：改密成功后服务端吊销全部令牌，前端延时跳转登录页重新认证。
 * 样式为 SFC 自包含（shell 未迁移 modals.css，避免引入整包组件样式）。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiPost, redirectToLogin } from "../api/client";
import { showToast } from "../composables/useToast";
import type { ChangePasswordRequest } from "../types/api/generated/ChangePasswordRequest";

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

const oldPassword = ref("");
const newPassword = ref("");
const confirmPassword = ref("");
const submitting = ref(false);

async function handleSubmit() {
  if (submitting.value) {
    return;
  }
  if (!oldPassword.value || !newPassword.value) {
    showToast(t("auth.password_required"), "warning");
    return;
  }
  if (newPassword.value !== confirmPassword.value) {
    showToast(t("auth.password_mismatch"), "warning");
    return;
  }

  submitting.value = true;
  try {
    const result = await apiPost<unknown, ChangePasswordRequest>("/api/auth/change-password", {
      old_password: oldPassword.value,
      new_password: newPassword.value
    });
    if (result.success) {
      showToast(result.message, "success");
      emit("close");
      // 改密后令牌全部失效，跳转登录页重新认证
      setTimeout(() => {
        redirectToLogin();
      }, 1200);
    } else {
      showToast(result.message, "error");
    }
  } catch (error) {
    // apiPost 永不 reject（失败归一为 ApiResult），此分支为防御性兜底
    console.error("修改密码请求失败:", error);
    showToast(t("auth.change_password_failed"), "error");
  } finally {
    submitting.value = false;
  }
}
</script>

<template>
  <div class="modal-overlay" @click.self="emit('close')">
    <div class="modal-dialog" role="dialog" aria-modal="true" :aria-label="t('auth.change_password')">
      <header class="modal-header">
        <h3 class="modal-title">{{ t("auth.change_password") }}</h3>
        <button type="button" class="modal-close" :aria-label="t('common.close')" @click="emit('close')">&times;</button>
      </header>

      <form id="change-password-form" class="modal-body" @submit.prevent="handleSubmit">
        <div class="form-group">
          <label for="change-password-old">
            {{ t("auth.old_password") }}<span class="required">*</span>
          </label>
          <input
            id="change-password-old"
            v-model="oldPassword"
            type="password"
            name="old_password"
            autocomplete="current-password"
            required
          >
        </div>
        <div class="form-group">
          <label for="change-password-new">
            {{ t("auth.new_password") }}<span class="required">*</span>
          </label>
          <input
            id="change-password-new"
            v-model="newPassword"
            type="password"
            name="new_password"
            autocomplete="new-password"
            minlength="8"
            required
          >
        </div>
        <div class="form-group">
          <label for="change-password-confirm">
            {{ t("auth.confirm_password") }}<span class="required">*</span>
          </label>
          <input
            id="change-password-confirm"
            v-model="confirmPassword"
            type="password"
            name="confirm_password"
            autocomplete="new-password"
            minlength="8"
            required
          >
        </div>
      </form>

      <footer class="modal-footer">
        <button type="button" class="btn-secondary" @click="emit('close')">
          {{ t("common.cancel") }}
        </button>
        <button type="submit" form="change-password-form" class="btn-primary" :disabled="submitting">
          {{ submitting ? t("common.processing") : t("common.save") }}
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
/* 外壳/页头/页脚/表单样式见全局 modals.css 与 forms.css；此处仅保留
   弹窗专属宽度与提交中禁用态（footer 外置出滚动区保证页脚常驻） */
.modal-dialog {
  max-width: 420px;
}

.btn-primary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>
