<script setup lang="ts">
/**
 * 安全页签（移植自旧版 security 分区 + fail2banManager.js/systemManager.js）：
 * 限流设置（共享 ["system-config"] 查询键）/ 等保密码策略 / 应用层 Fail2ban。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet, apiPut } from "../../api/client";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import Fail2banCard from "./Fail2banCard.vue";
import type { PasswordPolicy } from "../../types/api/generated/PasswordPolicy";

/** rate_limit 段配置（GET /api/system/config 的 rate_limit 段，后端未生成 ts-rs 类型） */
interface RateLimitSection {
  enabled?: boolean;
  ip_limit?: number;
  user_limit?: number;
  login_limit?: number;
  window_secs?: number;
  email_limit?: number;
  email_window_secs?: number;
}

/** GET/PUT /api/system/config 局部载荷（本页签仅带 rate_limit 段） */
interface RateLimitConfigPayload {
  rate_limit?: RateLimitSection | null;
}

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();

const readonly = isSectionReadonly("system");

// ==================== 限流设置 ====================

const configQuery = useQuery({
  queryKey: ["system-config"],
  queryFn: async () => {
    const result = await apiGet<RateLimitConfigPayload>("/api/system/config");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const rateEnabled = ref("true");
const rateIpLimit = ref("100");
const rateUserLimit = ref("200");
const rateLoginLimit = ref("5");
const rateWindow = ref("60");
const rateEmailLimit = ref("5");
const rateEmailWindow = ref("3600");

watch(
  () => configQuery.data.value,
  (data) => {
    const rateLimit = data?.rate_limit;
    if (!rateLimit) {
      return;
    }
    // enabled 缺省视为开启（与后端 serde 默认值一致）
    rateEnabled.value = String(rateLimit.enabled !== false);
    rateIpLimit.value = String(rateLimit.ip_limit || 100);
    rateUserLimit.value = String(rateLimit.user_limit || 200);
    rateLoginLimit.value = String(rateLimit.login_limit || 5);
    rateWindow.value = String(rateLimit.window_secs || 60);
    rateEmailLimit.value = String(rateLimit.email_limit || 5);
    rateEmailWindow.value = String(rateLimit.email_window_secs || 3600);
  }
);

const savingRateLimit = ref(false);

async function saveRateLimitConfig(): Promise<void> {
  if (savingRateLimit.value) {
    return;
  }
  savingRateLimit.value = true;
  try {
    const payload: { rate_limit: RateLimitSection } = {
      rate_limit: {
        enabled: rateEnabled.value === "true",
        ip_limit: Number.parseInt(rateIpLimit.value, 10) || 100,
        user_limit: Number.parseInt(rateUserLimit.value, 10) || 200,
        login_limit: Number.parseInt(rateLoginLimit.value, 10) || 5,
        window_secs: Number.parseInt(rateWindow.value, 10) || 60,
        email_limit: Number.parseInt(rateEmailLimit.value, 10) || 5,
        email_window_secs: Number.parseInt(rateEmailWindow.value, 10) || 3600
      }
    };
    const result = await apiPut("/api/system/config", payload);
    if (result.success) {
      showToast(t("config.rate_limit_saved"), "success");
    } else {
      showToast(result.message || t("common.save_failed"), "error");
    }
  } finally {
    savingRateLimit.value = false;
  }
}

// ==================== 等保密码策略 ====================

const policyQuery = useQuery({
  queryKey: ["password-policy"],
  queryFn: async () => {
    const result = await apiGet<PasswordPolicy>("/api/system/password-policy");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const policyMinLength = ref("8");
const policyHistory = ref("5");
const requireUpper = ref(true);
const requireLower = ref(true);
const requireDigit = ref(true);
const requireSpecial = ref(false);

watch(
  () => policyQuery.data.value,
  (policy) => {
    if (!policy) {
      return;
    }
    policyMinLength.value = String(policy.min_length ?? 8);
    policyHistory.value = String(policy.history_count ?? 5);
    requireUpper.value = Boolean(policy.require_upper);
    requireLower.value = Boolean(policy.require_lower);
    requireDigit.value = Boolean(policy.require_digit);
    requireSpecial.value = Boolean(policy.require_special);
  }
);

const savingPolicy = ref(false);

async function savePasswordPolicy(): Promise<void> {
  if (savingPolicy.value) {
    return;
  }
  savingPolicy.value = true;
  try {
    const payload: PasswordPolicy = {
      min_length: Number.parseInt(policyMinLength.value, 10) || 8,
      history_count: Number.parseInt(policyHistory.value, 10) || 0,
      require_upper: requireUpper.value,
      require_lower: requireLower.value,
      require_digit: requireDigit.value,
      require_special: requireSpecial.value
    };
    const result = await apiPut("/api/system/password-policy", payload);
    // 成功失败均使用后端返回的 message 提示
    showToast(result.message, result.success ? "success" : "error");
  } finally {
    savingPolicy.value = false;
  }
}
</script>

<template>
  <div class="tab-content" :class="{ active: props.active }">
    <div class="system-config-section">
      <!-- 限流设置 -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🚦</span>
            <span>{{ t("config.rate_limit_settings") }}</span>
          </div>
          <button v-if="!readonly" type="button" class="btn btn-primary btn-sm" :disabled="savingRateLimit" @click="saveRateLimitConfig">
            {{ t("common.save") }}
          </button>
        </header>
        <div class="nc-fields">
          <div class="nc-field">
            <label for="rate-limit-enabled">{{ t("config.rate_limit_enabled") }}</label>
            <select id="rate-limit-enabled" v-model="rateEnabled" :disabled="readonly">
              <option value="true">{{ t("common.yes") }}</option>
              <option value="false">{{ t("common.no") }}</option>
            </select>
          </div>
          <div class="nc-field">
            <label for="rate-limit-ip">{{ t("config.rate_limit_ip") }}</label>
            <input id="rate-limit-ip" v-model="rateIpLimit" type="number" min="1" max="10000" :disabled="readonly">
          </div>
          <div class="nc-field">
            <label for="rate-limit-user">{{ t("config.rate_limit_user") }}</label>
            <input id="rate-limit-user" v-model="rateUserLimit" type="number" min="1" max="10000" :disabled="readonly">
          </div>
          <div class="nc-field">
            <label for="rate-limit-login">{{ t("config.rate_limit_login") }}</label>
            <input id="rate-limit-login" v-model="rateLoginLimit" type="number" min="1" max="100" :disabled="readonly">
          </div>
          <div class="nc-field">
            <label for="rate-limit-window">{{ t("config.rate_limit_window") }}</label>
            <input id="rate-limit-window" v-model="rateWindow" type="number" min="10" max="3600" :disabled="readonly">
          </div>
          <div class="nc-field">
            <label for="rate-limit-email">{{ t("config.rate_limit_email") }}</label>
            <input id="rate-limit-email" v-model="rateEmailLimit" type="number" min="1" max="100" :disabled="readonly">
          </div>
          <div class="nc-field">
            <label for="rate-limit-email-window">{{ t("config.rate_limit_email_window") }}</label>
            <input id="rate-limit-email-window" v-model="rateEmailWindow" type="number" min="60" max="86400" :disabled="readonly">
          </div>
        </div>
      </section>

      <!-- 等保密码策略 -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🔑</span>
            <span>{{ t("config.password_policy") }}</span>
          </div>
          <button v-if="!readonly" type="button" class="btn btn-primary btn-sm" :disabled="savingPolicy" @click="savePasswordPolicy">
            {{ t("common.save") }}
          </button>
        </header>
        <!-- 输入字段：第一行 -->
        <div class="nc-fields">
          <div class="nc-field">
            <label for="password-policy-min-length">{{ t("config.password_min_length") }}</label>
            <input id="password-policy-min-length" v-model="policyMinLength" type="number" min="8" max="128" :disabled="readonly">
          </div>
          <div class="nc-field">
            <label for="password-policy-history">{{ t("config.password_history") }}</label>
            <input id="password-policy-history" v-model="policyHistory" type="number" min="0" max="24" :disabled="readonly">
          </div>
        </div>
        <!-- 复选框字段：第二行 -->
        <div class="nc-fields">
          <div class="nc-field">
            <label class="checkbox-label">
              <input v-model="requireUpper" type="checkbox" class="form-checkbox" :disabled="readonly">
              <span>{{ t("config.password_require_upper") }}</span>
            </label>
          </div>
          <div class="nc-field">
            <label class="checkbox-label">
              <input v-model="requireLower" type="checkbox" class="form-checkbox" :disabled="readonly">
              <span>{{ t("config.password_require_lower") }}</span>
            </label>
          </div>
          <div class="nc-field">
            <label class="checkbox-label">
              <input v-model="requireDigit" type="checkbox" class="form-checkbox" :disabled="readonly">
              <span>{{ t("config.password_require_digit") }}</span>
            </label>
          </div>
          <div class="nc-field">
            <label class="checkbox-label">
              <input v-model="requireSpecial" type="checkbox" class="form-checkbox" :disabled="readonly">
              <span>{{ t("config.password_require_special") }}</span>
            </label>
          </div>
        </div>
      </section>

      <!-- 应用层 Fail2ban -->
      <Fail2banCard :active="props.active" />
    </div>
  </div>
</template>
