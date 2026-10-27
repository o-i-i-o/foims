<script setup lang="ts">
/**
 * 系统配置页签：
 * 会话设置 / LDAP / SSO 三张表单卡片内联实现，服务管理与证书管理拆分为
 * 子组件。会话与 Agent/SNMP/限流等共用 /api/system/config（queryKey
 * ["system-config"]），保存仅替换 server 段并带回其余字段避免丢配置。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import {
  getLdapConfig,
  getSsoConfig,
  getSystemConfig,
  testLdapConnection as requestLdapTest,
  testSsoConnection as requestSsoTest,
  updateLdapConfig,
  updateSsoConfig,
  updateSystemConfig
} from "../../api/system";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import CertificateCard from "./CertificateCard.vue";
import ServicesCard from "./ServicesCard.vue";
import type { UpdateLdapConfigRequest } from "../../types/api/generated/UpdateLdapConfigRequest";
import type { UpdateSsoConfigRequest } from "../../types/api/generated/UpdateSsoConfigRequest";

/** server 段配置（GET /api/system/config 的 server 段，后端未生成 ts-rs 类型；其余字段原样带回） */
interface ServerSection {
  page_timeout?: number;
  public_url?: string;
  [key: string]: unknown;
}

/** GET/PUT /api/system/config 局部载荷（本页签仅带 server 段） */
interface ServerConfigPayload {
  server?: ServerSection | null;
}

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

// ==================== 会话设置（server 段） ====================

// 最近一次加载的 server 段原文：PUT 整体替换，保存时原样带回其余字段
// （初始空对象与 JS 展开 null 等价，均为仅覆盖两个编辑字段的载荷）
let serverBase: ServerSection = {};

const pageTimeout = ref("30");
const publicUrl = ref("");

const configQuery = useQuery({
  queryKey: ["system-config"],
  queryFn: async () => {
    const result = await getSystemConfig<ServerConfigPayload>();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

watch(
  () => configQuery.data.value,
  (config) => {
    if (!config?.server) {
      return;
    }
    serverBase = config.server;
    pageTimeout.value = String(config.server.page_timeout || 30);
    publicUrl.value = config.server.public_url || "";
  }
);

const savingConfig = ref(false);

async function saveSystemConfig(): Promise<void> {
  if (savingConfig.value) {
    return;
  }
  savingConfig.value = true;
  try {
    const server = {
      ...serverBase,
      page_timeout: Number.parseInt(pageTimeout.value, 10) || 30,
      public_url: publicUrl.value || ""
    };
    const result = await updateSystemConfig({ server });
    if (result.success) {
      showToast(t("system.config_save_success"), "success");
      // 部分配置需重启生效：保存即提示
      showToast(t("system.config_saved_restart_needed"), "warning");
      queryClient.invalidateQueries({ queryKey: ["system-config"] });
    } else {
      showToast(`${t("system.config_save_failed")}: ${result.message}`, "error");
    }
  } finally {
    savingConfig.value = false;
  }
}

// ==================== LDAP 认证配置 ====================

const ldapEnabled = ref(false);
const ldapUrl = ref("");
const ldapBindDn = ref("");
const ldapBaseDn = ref("");
const ldapUserFilter = ref("");
const ldapDefaultRole = ref("user");
const ldapPassword = ref("");
const ldapHasPassword = ref(false);

const ldapQuery = useQuery({
  queryKey: ["ldap-config"],
  queryFn: async () => {
    const result = await getLdapConfig();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

watch(
  () => ldapQuery.data.value,
  (config) => {
    if (!config) {
      return;
    }
    ldapEnabled.value = Boolean(config.enabled);
    ldapUrl.value = config.url || "";
    ldapBindDn.value = config.bind_dn || "";
    ldapBaseDn.value = config.base_dn || "";
    ldapUserFilter.value = config.user_filter || "";
    ldapDefaultRole.value = config.default_role || "user";
    // 密码不回填：留空表示沿用已保存值，占位符提示是否已配置
    ldapPassword.value = "";
    ldapHasPassword.value = Boolean(config.has_password);
  }
);

const savingLdap = ref(false);

async function saveLdapConfig(): Promise<void> {
  if (savingLdap.value) {
    return;
  }
  savingLdap.value = true;
  try {
    const config: UpdateLdapConfigRequest = {
      enabled: ldapEnabled.value,
      url: ldapUrl.value,
      bind_dn: ldapBindDn.value || "",
      bind_password: ldapPassword.value || "",
      base_dn: ldapBaseDn.value,
      user_filter: ldapUserFilter.value,
      default_role: ldapDefaultRole.value || "user"
    };
    const result = await updateLdapConfig(config);
    if (result.success) {
      showToast(t("ldap.save_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["ldap-config"] });
    } else {
      showToast(`${t("ldap.save_failed")}: ${result.message}`, "error");
    }
  } finally {
    savingLdap.value = false;
  }
}

const testingLdap = ref(false);

async function testLdapConnection(): Promise<void> {
  if (testingLdap.value) {
    return;
  }
  testingLdap.value = true;
  try {
    const result = await requestLdapTest();
    if (result.success) {
      showToast(t("ldap.test_success"), "success");
    } else {
      showToast(`${t("ldap.test_failed")}: ${result.message}`, "error");
    }
  } finally {
    testingLdap.value = false;
  }
}

// ==================== SSO（OIDC）配置 ====================

const ssoEnabled = ref(false);
const ssoIssuerUrl = ref("");
const ssoClientId = ref("");
const ssoRedirectUri = ref("");
const ssoDefaultRole = ref("user");
const ssoClientSecret = ref("");
const ssoHasSecret = ref(false);

const ssoQuery = useQuery({
  queryKey: ["sso-config"],
  queryFn: async () => {
    const result = await getSsoConfig();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

watch(
  () => ssoQuery.data.value,
  (config) => {
    if (!config) {
      return;
    }
    ssoEnabled.value = Boolean(config.enabled);
    ssoIssuerUrl.value = config.issuer_url || "";
    ssoClientId.value = config.client_id || "";
    ssoRedirectUri.value = config.redirect_uri || "";
    ssoDefaultRole.value = config.default_role || "user";
    ssoClientSecret.value = "";
    ssoHasSecret.value = Boolean(config.has_secret);
  }
);

const savingSso = ref(false);

async function saveSsoConfig(): Promise<void> {
  if (savingSso.value) {
    return;
  }
  savingSso.value = true;
  try {
    const config: UpdateSsoConfigRequest = {
      enabled: ssoEnabled.value,
      issuer_url: ssoIssuerUrl.value,
      client_id: ssoClientId.value,
      client_secret: ssoClientSecret.value || "",
      redirect_uri: ssoRedirectUri.value || "",
      default_role: ssoDefaultRole.value || "user"
    };
    const result = await updateSsoConfig(config);
    if (result.success) {
      showToast(t("sso.save_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["sso-config"] });
    } else {
      showToast(`${t("sso.save_failed")}: ${result.message}`, "error");
    }
  } finally {
    savingSso.value = false;
  }
}

const testingSso = ref(false);

async function testSsoConnection(): Promise<void> {
  if (testingSso.value) {
    return;
  }
  testingSso.value = true;
  try {
    const result = await requestSsoTest();
    if (result.success) {
      showToast(t("sso.test_success"), "success");
    } else {
      showToast(`${t("sso.test_failed")}: ${result.message}`, "error");
    }
  } finally {
    testingSso.value = false;
  }
}
</script>

<template>
  <div class="tab-content" :class="{ active: props.active }">
    <div class="system-config-section">
      <!-- 会话设置 -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">⏱️</span>
            <span>{{ t("config.session_settings") }}</span>
          </div>
          <button v-if="!readonly" type="button" class="btn btn-primary" :disabled="savingConfig" @click="saveSystemConfig">
            {{ t("common.save") }}
          </button>
        </header>
        <div class="form-row">
          <div class="form-group">
            <label for="page-timeout">{{ t("config.page_timeout") }}</label>
            <input id="page-timeout" v-model="pageTimeout" type="number" min="1" max="1440" placeholder="30" :aria-label="t('config.page_timeout')">
          </div>
          <div class="form-group">
            <label for="public-url">{{ t("config.public_url") }}</label>
            <input id="public-url" v-model="publicUrl" type="url" placeholder="https://example.com" :aria-label="t('config.public_url')">
          </div>
        </div>
      </section>

      <ServicesCard :active="props.active" />
      <CertificateCard :active="props.active" />

      <!-- LDAP 配置 -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🗂️</span>
            <span>{{ t("ldap.title") }}</span>
          </div>
          <div v-if="!readonly" class="config-switch">
            <label for="ldap-enabled">
              <input id="ldap-enabled" v-model="ldapEnabled" type="checkbox" :aria-label="t('ldap.enabled')">
              <span>{{ t("ldap.enabled") }}</span>
            </label>
            <button type="button" class="btn btn-secondary btn-sm" :disabled="testingLdap" @click="testLdapConnection">
              {{ t("ldap.test_connection") }}
            </button>
            <button type="button" class="btn btn-primary btn-sm" :disabled="savingLdap" @click="saveLdapConfig">
              {{ t("common.save") }}
            </button>
          </div>
        </header>
        <div class="form-row">
          <div class="form-group">
            <label for="ldap-url">{{ t("ldap.server_url") }}</label>
            <input id="ldap-url" v-model="ldapUrl" type="text" :placeholder="t('ldap.server_url_placeholder')" :aria-label="t('ldap.server_url')">
          </div>
          <div class="form-group">
            <label for="ldap-base-dn">{{ t("ldap.base_dn") }}</label>
            <input id="ldap-base-dn" v-model="ldapBaseDn" type="text" :placeholder="t('ldap.base_dn_placeholder')" :aria-label="t('ldap.base_dn')">
          </div>
          <div class="form-group">
            <label for="ldap-bind-dn">{{ t("ldap.bind_dn") }}</label>
            <input id="ldap-bind-dn" v-model="ldapBindDn" type="text" :placeholder="t('ldap.bind_dn_placeholder')" :aria-label="t('ldap.bind_dn')">
          </div>
          <div class="form-group">
            <label for="ldap-bind-password">{{ t("ldap.bind_password") }}</label>
            <input
              id="ldap-bind-password"
              v-model="ldapPassword"
              type="password"
              autocomplete="new-password"
              :placeholder="t(ldapHasPassword ? 'ldap.password_configured' : 'ldap.bind_password_placeholder')"
              :aria-label="t('ldap.bind_password')"
            >
          </div>
          <div class="form-group">
            <label for="ldap-user-filter">{{ t("ldap.user_filter") }}</label>
            <input id="ldap-user-filter" v-model="ldapUserFilter" type="text" :placeholder="t('ldap.user_filter_placeholder')" :aria-label="t('ldap.user_filter')">
          </div>
          <div class="form-group">
            <label for="ldap-default-role">{{ t("ldap.default_role") }}</label>
            <select id="ldap-default-role" v-model="ldapDefaultRole" :aria-label="t('ldap.default_role')">
              <option value="user">{{ t("user.role_user") }}</option>
              <option value="admin">{{ t("user.role_admin") }}</option>
            </select>
          </div>
        </div>
      </section>

      <!-- SSO（OIDC）配置 -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🔐</span>
            <span>{{ t("sso.title") }}</span>
          </div>
          <div v-if="!readonly" class="config-switch">
            <label for="sso-enabled">
              <input id="sso-enabled" v-model="ssoEnabled" type="checkbox" :aria-label="t('sso.enabled')">
              <span>{{ t("sso.enabled") }}</span>
            </label>
            <button type="button" class="btn btn-secondary btn-sm" :disabled="testingSso" @click="testSsoConnection">
              {{ t("sso.test_connection") }}
            </button>
            <button type="button" class="btn btn-primary btn-sm" :disabled="savingSso" @click="saveSsoConfig">
              {{ t("common.save") }}
            </button>
          </div>
        </header>
        <div class="form-row">
          <div class="form-group">
            <label for="sso-issuer-url">{{ t("sso.issuer_url") }}</label>
            <input id="sso-issuer-url" v-model="ssoIssuerUrl" type="url" :placeholder="t('sso.issuer_url_placeholder')" :aria-label="t('sso.issuer_url')">
          </div>
          <div class="form-group">
            <label for="sso-client-id">{{ t("sso.client_id") }}</label>
            <input id="sso-client-id" v-model="ssoClientId" type="text" :placeholder="t('sso.client_id_placeholder')" :aria-label="t('sso.client_id')">
          </div>
          <div class="form-group">
            <label for="sso-client-secret">{{ t("sso.client_secret") }}</label>
            <input
              id="sso-client-secret"
              v-model="ssoClientSecret"
              type="password"
              autocomplete="new-password"
              :placeholder="t(ssoHasSecret ? 'sso.secret_configured' : 'sso.client_secret_placeholder')"
              :aria-label="t('sso.client_secret')"
            >
          </div>
          <div class="form-group">
            <label for="sso-redirect-uri">{{ t("sso.redirect_uri") }}</label>
            <input id="sso-redirect-uri" v-model="ssoRedirectUri" type="url" :placeholder="t('sso.redirect_uri_placeholder')" :aria-label="t('sso.redirect_uri')">
          </div>
          <div class="form-group">
            <label for="sso-default-role">{{ t("sso.default_role") }}</label>
            <select id="sso-default-role" v-model="ssoDefaultRole" :aria-label="t('sso.default_role')">
              <option value="user">{{ t("user.role_user") }}</option>
              <option value="admin">{{ t("user.role_admin") }}</option>
            </select>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>
