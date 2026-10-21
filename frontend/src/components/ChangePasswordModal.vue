<script setup lang="ts">
/**
 * 修改密码模态框（移植自旧版 authManager.initChangePassword +
 * modals/system/change-password-modal.html）。
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
        <h3>{{ t("auth.change_password") }}</h3>
        <button type="button" class="modal-close" :aria-label="t('common.close')" @click="emit('close')">&times;</button>
      </header>

      <form class="modal-body" @submit.prevent="handleSubmit">
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

        <footer class="modal-footer">
          <button type="button" class="btn-secondary" @click="emit('close')">
            {{ t("common.cancel") }}
          </button>
          <button type="submit" class="btn-primary" :disabled="submitting">
            {{ submitting ? t("common.processing") : t("common.save") }}
          </button>
        </footer>
      </form>
    </div>
  </div>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgb(0 0 0 / 50%);
  padding: var(--space-md);
}

.modal-dialog {
  width: 100%;
  max-width: 420px;
  background: var(--color-bg-white);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
  overflow: hidden;
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-md) var(--space-lg);
  border-bottom: 1px solid var(--color-border);
}

.modal-header h3 {
  margin: 0;
  font-size: 1.05rem;
  color: var(--color-text);
}

.modal-close {
  border: none;
  background: transparent;
  color: var(--color-text-muted);
  font-size: 1.5rem;
  line-height: 1;
  cursor: pointer;
}

.modal-close:hover {
  color: var(--color-danger);
}

.modal-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-md);
  padding: var(--space-lg);
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
}

.form-group label {
  font-size: var(--font-size-sm);
  color: var(--color-text-secondary);
}

.required {
  margin-left: 2px;
  color: var(--color-danger);
}

.form-group input {
  padding: 0.5rem 0.75rem;
  border: 1px solid var(--control-border);
  border-radius: var(--radius-md);
  color: var(--color-text);
  background: var(--color-bg-white);
  transition:
    border-color var(--transition-fast),
    box-shadow var(--transition-fast);
}

.form-group input:focus {
  border-color: var(--control-border-focus);
  box-shadow: var(--control-shadow);
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-sm);
  margin-top: var(--space-sm);
}

.btn-primary,
.btn-secondary {
  padding: 0.5rem 1rem;
  border-radius: var(--radius-md);
  border: 1px solid transparent;
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition:
    background-color var(--transition-fast),
    border-color var(--transition-fast),
    color var(--transition-fast);
}

.btn-primary {
  background: var(--color-primary);
  color: var(--color-bg-white);
}

.btn-primary:hover {
  background: var(--color-primary-hover);
}

.btn-primary:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.btn-secondary {
  background: var(--color-bg-white);
  border-color: var(--control-border);
  color: var(--color-text-secondary);
}

.btn-secondary:hover {
  border-color: var(--control-border-hover);
  color: var(--color-text);
}
</style>
