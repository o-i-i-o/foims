<script setup>
/**
 * 忘记密码模态框（移植自 web/static/modals/auth/forgot-password-modal.html
 * + login.js handleForgotPassword）。模板片段改为 SFC 内联，不再走
 * modalLoader 动态加载。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiPost } from "../api/client.js";

const emit = defineEmits(["close"]);

const { t } = useI18n();

const email = ref("");
const errorMessage = ref("");
const successMessage = ref("");
const sending = ref(false);

async function handleSubmit() {
  errorMessage.value = "";
  successMessage.value = "";

  sending.value = true;
  try {
    const result = await apiPost(
      "/api/auth/forgot-password",
      { email: email.value },
      { skipAuthCheck: true }
    );

    if (result.success) {
      successMessage.value = t("login.reset_email_sent");
      email.value = "";
      setTimeout(() => {
        emit("close");
        successMessage.value = "";
      }, 3000);
    } else {
      errorMessage.value = result.message || t("login.send_failed");
    }
  } catch (error) {
    let message = t("login.send_failed");
    if (error?.message?.includes("Network")) {
      message = t("login.network_failed");
    }
    errorMessage.value = message;
  } finally {
    sending.value = false;
  }
}
</script>

<template>
  <div
    class="login-modal active"
    role="dialog"
    aria-modal="true"
    :aria-label="t('login.forgot_title')"
    @click.self="emit('close')"
  >
    <div class="login-modal-content">
      <header class="login-modal-header">
        <h3 class="login-modal-title">{{ t("login.forgot_title") }}</h3>
        <button type="button" class="close" :aria-label="'Close'" @click="emit('close')">&times;</button>
      </header>
      <div class="login-modal-body">
        <form id="forgot-password-form" @submit.prevent="handleSubmit">
          <div class="login-form-group">
            <label for="forgot-email">{{ t("login.forgot_email_label") }}</label>
            <input
              id="forgot-email"
              v-model="email"
              type="email"
              autocomplete="email"
              name="email"
              required
              :placeholder="t('login.email_placeholder')"
              :aria-label="t('auth.email')"
            >
          </div>
          <small class="login-hint-text">{{ t("login.forgot_hint") }}</small>
          <div class="login-error-message" :class="{ show: errorMessage !== '' }" role="alert" aria-live="polite">
            {{ errorMessage }}
          </div>
          <div class="login-success-message" :class="{ show: successMessage !== '' }" role="status" aria-live="polite">
            {{ successMessage }}
          </div>
        </form>
      </div>
      <footer class="login-modal-footer">
        <button type="submit" class="login-btn-primary" form="forgot-password-form" :disabled="sending">
          {{ sending ? t("login.sending") : t("login.send_reset_email") }}
        </button>
      </footer>
    </div>
  </div>
</template>
