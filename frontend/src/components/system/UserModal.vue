<script setup lang="ts">
/**
 * 用户新增/编辑弹窗（两步结构）：第一步账户信息（原生表单校验），第二步双因素认证
 * （未保存提示 / 扫码启用 / 已启用管理三态）。保存统一由第二页"保存"
 * 提交整个表单；启用/禁用 2FA 即时生效不关框。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import {
  createUser,
  disableTwoFactor,
  enableTwoFactor,
  getUser,
  initTwoFactor as requestTwoFactorInit,
  updateUser,
  type UserFormPayload
} from "../../api/users";
import { showToast } from "../../composables/useToast";
import ConfirmModal from "../ConfirmModal.vue";
import ModalShell from "../ModalShell.vue";
import type { User } from "../../types/api/generated/User";

const props = withDefaults(defineProps<{ user?: User | null }>(), { user: null });

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

const isEdit = computed(() => props.user != null);
const userId = computed(() => props.user?.id ?? null);

const formRef = ref<HTMLFormElement | null>(null);
const username = ref("");
const email = ref("");
const phone = ref("");
const password = ref("");
const passwordConfirm = ref("");
const role = ref("user");
const enabled = ref("true");
const passwordExpiryDays = ref("0");

// 编辑时记录的 2FA 启用状态：进入第二页时决定展示管理页还是设置页
let twoFactorTargetEnabled = false;

const detailQuery = useQuery({
  queryKey: computed(() => ["users-detail", userId.value]),
  queryFn: async () => {
    const id = userId.value;
    if (!id) {
      throw new Error(t("common.load_failed"));
    }
    const result = await getUser(id);
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: isEdit
});

watch(
  () => detailQuery.data.value,
  (user) => {
    if (!user) {
      return;
    }
    twoFactorTargetEnabled = Boolean(user.two_factor_enabled);
    username.value = user.username || "";
    email.value = user.email || "";
    phone.value = user.phone || "";
    role.value = user.role || "user";
    enabled.value = String(Boolean(user.enabled));
    passwordExpiryDays.value = String(user.password_expiry_days ?? 0);
    password.value = "";
    passwordConfirm.value = "";
  }
);

const step = ref<"account" | "2fa">("account");
// 三态：unsaved 新用户待保存 / setup 扫码启用 / manage 已启用可禁用
const twoFactorView = ref<"unsaved" | "setup" | "manage">("unsaved");
const qrCode = ref("");
const secret = ref("");
const enableCode = ref("");
const disableCode = ref("");
const enableError = ref("");
const disableError = ref("");

async function initTwoFactor(targetId: string): Promise<void> {
  enableError.value = "";
  const result = await requestTwoFactorInit(targetId);
  if (result.success) {
    qrCode.value = result.data?.qr_code_base64
      ? `data:image/png;base64,${result.data.qr_code_base64}`
      : "";
    secret.value = result.data?.secret || "";
  } else {
    enableError.value = `${t("two_factor.fetch_config_failed")}：${result.message}`;
  }
}

// 第一步"下一步"：仅切换到第二页（不触发保存）；无效时浏览器标红停留
function goTo2faStep(): void {
  const form = formRef.value;
  if (form && !form.checkValidity()) {
    form.reportValidity();
    return;
  }
  step.value = "2fa";
  enableCode.value = "";
  disableCode.value = "";
  enableError.value = "";
  disableError.value = "";
  if (!userId.value) {
    // 新用户尚未落库：2FA 需针对已存在用户
    twoFactorView.value = "unsaved";
    return;
  }
  twoFactorView.value = twoFactorTargetEnabled ? "manage" : "setup";
  if (!twoFactorTargetEnabled) {
    initTwoFactor(userId.value);
  }
}

// 第二步返回第一步：回改账户信息
function backToAccount(): void {
  step.value = "account";
}

// 启用 2FA：成功后动态切到管理视图（主按钮变"禁用2FA"），不关框
const enabling = ref(false);

async function handleEnable(): Promise<void> {
  if (enabling.value || !userId.value) {
    return;
  }
  if (enableCode.value.length !== 6) {
    enableError.value = t("two_factor.verify_code_placeholder");
    return;
  }
  enabling.value = true;
  try {
    const result = await enableTwoFactor(userId.value, enableCode.value);
    if (result.success) {
      showToast(t("two_factor.enable_success"), "success");
      twoFactorTargetEnabled = true;
      twoFactorView.value = "manage";
      disableCode.value = "";
      disableError.value = "";
      queryClient.invalidateQueries({ queryKey: ["users"] });
    } else {
      enableError.value = `${t("two_factor.enable_failed")}：${result.message}`;
    }
  } finally {
    enabling.value = false;
  }
}

// 禁用 2FA：确认后切回扫码设置页（重新 init 生成新密钥），不关框
const disabling = ref(false);
const showDisableConfirm = ref(false);

async function handleDisable(): Promise<void> {
  if (disabling.value || !userId.value) {
    return;
  }
  if (disableCode.value.length !== 6) {
    disableError.value = t("two_factor.verify_code_placeholder");
    return;
  }
  showDisableConfirm.value = true;
}

async function confirmDisable(): Promise<void> {
  showDisableConfirm.value = false;
  if (disabling.value || !userId.value) {
    return;
  }
  disabling.value = true;
  try {
    const result = await disableTwoFactor(userId.value, disableCode.value);
    if (result.success) {
      showToast(t("two_factor.disable_success"), "success");
      twoFactorTargetEnabled = false;
      twoFactorView.value = "setup";
      enableCode.value = "";
      enableError.value = "";
      queryClient.invalidateQueries({ queryKey: ["users"] });
      await initTwoFactor(userId.value);
    } else {
      disableError.value = `${t("two_factor.disable_failed")}：${result.message}`;
    }
  } finally {
    disabling.value = false;
  }
}

const saving = ref(false);

async function handleSave(): Promise<void> {
  if (saving.value) {
    return;
  }
  const form = formRef.value;
  // 校验失败切回第一页交由浏览器标红（如上一步回改时清空了必填项）
  if (form && !form.checkValidity()) {
    backToAccount();
    form.reportValidity();
    return;
  }
  if (password.value && password.value !== passwordConfirm.value) {
    showToast(t("user.password_mismatch"), "error");
    backToAccount();
    return;
  }

  const userData: UserFormPayload = {
    username: username.value,
    email: email.value,
    // 空串提交表示清空手机号，后端规范化为 NULL
    phone: phone.value.trim(),
    role: role.value,
    enabled: enabled.value === "true",
    password_expiry_days: Number.parseInt(passwordExpiryDays.value, 10) || 0
  };
  if (password.value) {
    userData.password = password.value;
  }

  saving.value = true;
  try {
    const id = userId.value;
    const result = isEdit.value && id
      ? await updateUser(id, userData)
      : await createUser(userData);
    if (result.success) {
      showToast(isEdit.value ? t("user.update_success") : t("user.add_success"), "success");
      emit("saved");
      emit("close");
    } else {
      showToast(
        `${isEdit.value ? t("user.update_failed") : t("user.add_failed")}：${result.message}`,
        "error"
      );
      backToAccount();
    }
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <ModalShell :size="'md'" @close="emit('close')">
    <template #title>
      {{ step === "account" ? t(isEdit ? "user.edit_user" : "user.add_user") : t("two_factor.title") }}
    </template>

    <!-- 第一步：账户信息 -->
    <div v-show="step === 'account'">
      <form ref="formRef">
        <div class="form-row">
          <div class="form-group">
            <label for="user-username">{{ t("auth.username") }}<span class="required">*</span></label>
            <input
              id="user-username"
              v-model="username"
              type="text"
              name="username"
              autocomplete="username"
              required
              :readonly="isEdit"
              :placeholder="t('user.username_placeholder')"
            >
          </div>
          <div class="form-group">
            <label for="user-status">{{ t("user.status") }}<span class="required">*</span></label>
            <select id="user-status" v-model="enabled" name="enabled" required>
              <option value="true">{{ t("user.status_enabled") }}</option>
              <option value="false">{{ t("user.status_disabled") }}</option>
            </select>
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="user-email">{{ t("user.email") }}<span class="required">*</span></label>
            <input
              id="user-email"
              v-model="email"
              type="email"
              name="email"
              autocomplete="email"
              required
              :placeholder="t('user.email_placeholder')"
            >
          </div>
          <div class="form-group">
            <label for="user-phone">{{ t("user.phone") }}</label>
            <input
              id="user-phone"
              v-model="phone"
              type="tel"
              name="phone"
              maxlength="20"
              autocomplete="off"
              :placeholder="t('user.phone_placeholder')"
            >
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="user-password">{{ t("auth.password") }}</label>
            <input
              id="user-password"
              v-model="password"
              type="password"
              name="password"
              autocomplete="new-password"
              :required="!isEdit"
              :placeholder="t(isEdit ? 'user.password_leave_blank' : 'user.password_placeholder')"
            >
          </div>
          <div class="form-group">
            <label for="user-password-confirm">{{ t("auth.confirm_password") }}</label>
            <input
              id="user-password-confirm"
              v-model="passwordConfirm"
              type="password"
              name="password_confirm"
              autocomplete="new-password"
              :required="!isEdit"
              :placeholder="t(isEdit ? 'user.password_leave_blank' : 'user.password_confirm_placeholder')"
            >
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="user-role">{{ t("user.role") }}<span class="required">*</span></label>
            <select id="user-role" v-model="role" name="role" required>
              <option value="admin">{{ t("user.role_admin") }}</option>
              <option value="sysadmin">{{ t("user.role_sysadmin") }}</option>
              <option value="secadmin">{{ t("user.role_secadmin") }}</option>
              <option value="auditor">{{ t("user.role_auditor") }}</option>
              <option value="user">{{ t("user.role_user") }}</option>
            </select>
          </div>
          <div class="form-group">
            <label for="user-password-expiry">{{ t("user.password_expiry_f") }}</label>
            <input id="user-password-expiry" v-model="passwordExpiryDays" type="number" name="password_expiry_days" min="0" max="36500">
            <label for="user-password-expiry">{{ t("user.password_expiry_b") }}</label>
          </div>
        </div>
      </form>
    </div>

    <!-- 第二步：双因素认证（必经此页，可不启用直接"保存"） -->
    <div v-show="step === '2fa'">
      <!-- 未保存的新用户：2FA 需针对已存在用户 -->
      <p v-if="twoFactorView === 'unsaved'" class="hint-text text-center">
        {{ t("two_factor.unsaved_hint") }}
      </p>
      <!-- 未启用：扫码 / 手动输入密钥后提交动态码启用 -->
      <div v-else-if="twoFactorView === 'setup'">
        <p class="hint-text text-center">{{ t("two_factor.scan_hint") }}</p>
        <div class="qr-code-container">
          <img v-if="qrCode" :src="qrCode" alt="2FA QR Code">
        </div>
        <div class="form-group">
          <label for="user-2fa-secret">{{ t("two_factor.secret_key") }}</label>
          <input id="user-2fa-secret" :value="secret" type="text" readonly :aria-label="t('two_factor.secret_key')">
        </div>
        <div class="form-group">
          <label for="user-2fa-verify-code">{{ t("two_factor.verify_code") }}</label>
          <input
            id="user-2fa-verify-code"
            v-model="enableCode"
            type="text"
            maxlength="6"
            inputmode="numeric"
            autocomplete="one-time-code"
            :placeholder="t('two_factor.verify_code_placeholder')"
            :aria-label="t('two_factor.verify_code')"
          >
        </div>
        <div v-if="enableError" class="error-message show" role="alert" aria-live="polite">{{ enableError }}</div>
      </div>
      <!-- 已启用：展示状态并可禁用 -->
      <div v-else>
        <p class="hint-text text-center">{{ t("two_factor.enabled_hint") }}</p>
        <div class="form-group">
          <label for="user-2fa-disable-code">{{ t("two_factor.verify_code") }}</label>
          <input
            id="user-2fa-disable-code"
            v-model="disableCode"
            type="text"
            maxlength="6"
            inputmode="numeric"
            autocomplete="one-time-code"
            :placeholder="t('two_factor.disable_code_placeholder')"
            :aria-label="t('two_factor.disable_code_placeholder')"
          >
        </div>
        <div v-if="disableError" class="error-message show" role="alert" aria-live="polite">{{ disableError }}</div>
      </div>
    </div>

    <template #footer>
      <button v-if="step === 'account'" type="button" class="btn-primary" @click="goTo2faStep">
        {{ t("user.next_step") }}
      </button>
      <template v-else>
        <button type="button" class="btn-secondary" @click="backToAccount">
          {{ t("user.previous_step") }}
        </button>
        <button
          v-if="twoFactorView === 'setup'"
          type="button"
          class="btn-primary"
          :disabled="enabling"
          @click="handleEnable"
        >
          {{ t("two_factor.enable") }}
        </button>
        <button
          v-if="twoFactorView === 'manage'"
          type="button"
          class="btn-danger"
          :disabled="disabling"
          @click="handleDisable"
        >
          {{ t("two_factor.disable") }}
        </button>
        <button type="button" class="btn-primary" :disabled="saving" @click="handleSave">
          {{ t("common.save") }}
        </button>
      </template>
    </template>
  </ModalShell>

  <!-- 禁用 2FA 二次确认（嵌套于业务弹窗之上，取消不关闭父弹窗） -->
  <ConfirmModal
    v-if="showDisableConfirm"
    :message="t('two_factor.disable_confirm')"
    danger
    @confirm="confirmDisable"
    @cancel="showDisableConfirm = false"
  />
</template>
