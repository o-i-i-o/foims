<script setup lang="ts">
/**
 * SMTP 配置页签：
 * SMTP 服务器卡片（未配置属正常业务态）+ Webhook 通知卡片（通用/钉钉/企业微信
 * 三渠道，未配置属正常业务态）+ 通知设置卡片（方式矩阵 + 收件人复选清单）。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import {
  getNotificationSettings,
  getSmtpConfig,
  getWebhookConfig,
  testSmtpConnection as requestSmtpTest,
  testWebhookChannel as requestWebhookTest,
  updateNotificationSettings,
  updateSmtpConfig,
  updateWebhookConfig
} from "../../api/system";
import { listUsersForPicker } from "../../api/users";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import type { UpdateSmtpConfigRequest } from "../../types/api/generated/UpdateSmtpConfigRequest";
import type { UpdateWebhookConfigRequest } from "../../types/api/generated/UpdateWebhookConfigRequest";
import type { NotificationSettings } from "../../types/api/generated/NotificationSettings";
import type { NotificationMethods } from "../../types/api/generated/NotificationMethods";
import type { User } from "../../types/api/generated/User";

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

const smtpHost = ref("");
const smtpPort = ref("587");
const smtpUsername = ref("");
const smtpPassword = ref("");
const smtpFrom = ref("");
// 下拉三态（none/ssl/starttls），提交时折叠为 secure 布尔
const secureType = ref("none");
const smtpHasPassword = ref(false);

const smtpQuery = useQuery({
  queryKey: ["smtp-config"],
  queryFn: async () => {
    const result = await getSmtpConfig();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

watch(
  () => smtpQuery.data.value,
  (config) => {
    if (!config) {
      return;
    }
    if (config.configured) {
      smtpHost.value = config.host || "";
      smtpPort.value = String(config.port || 587);
      smtpUsername.value = config.username || "";
      smtpFrom.value = config.from || "";
      secureType.value = config.secure ? "ssl" : "none";
      smtpHasPassword.value = Boolean(config.has_password);
    } else {
      // 未配置：清空表单（未配置是正常业务状态）
      smtpHost.value = "";
      smtpPort.value = "587";
      smtpUsername.value = "";
      smtpFrom.value = "";
      secureType.value = "none";
      smtpHasPassword.value = false;
    }
    smtpPassword.value = "";
  }
);

const savingSmtp = ref(false);

async function saveSmtpConfig(): Promise<void> {
  if (savingSmtp.value) {
    return;
  }
  savingSmtp.value = true;
  try {
    const payload: UpdateSmtpConfigRequest = {
      host: smtpHost.value,
      port: Number.parseInt(smtpPort.value, 10),
      username: smtpUsername.value,
      password: smtpPassword.value,
      from: smtpFrom.value,
      secure: secureType.value !== "none"
    };
    const result = await updateSmtpConfig(payload);
    if (result.success) {
      showToast(t("smtp.save_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["smtp-config"] });
    } else {
      showToast(`${t("smtp.save_failed")}: ${result.message}`, "error");
    }
  } finally {
    savingSmtp.value = false;
  }
}

const testingSmtp = ref(false);

async function testSmtpConnection(): Promise<void> {
  if (testingSmtp.value) {
    return;
  }
  testingSmtp.value = true;
  try {
    const result = await requestSmtpTest();
    if (result.success) {
      showToast(t("smtp.test_success"), "success");
    } else {
      showToast(`${t("smtp.test_failed")}: ${result.message}`, "error");
    }
  } finally {
    testingSmtp.value = false;
  }
}

// ---- Webhook 通知（通用 / 钉钉 / 企业微信三渠道，未配置属正常业务态） ----

const genericUrl = ref("");
const dingtalkUrl = ref("");
const dingtalkSecret = ref("");
const dingtalkHasSecret = ref(false);
const wecomKey = ref("");

const webhookQuery = useQuery({
  queryKey: ["webhook-config"],
  queryFn: async () => {
    const result = await getWebhookConfig();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

watch(
  () => webhookQuery.data.value,
  (config) => {
    if (!config) {
      return;
    }
    genericUrl.value = config.generic_url || "";
    dingtalkUrl.value = config.dingtalk_url || "";
    wecomKey.value = config.wecom_key || "";
    dingtalkHasSecret.value = Boolean(config.dingtalk_has_secret);
    dingtalkSecret.value = "";
  }
);

const savingWebhook = ref(false);

async function saveWebhookConfig(): Promise<void> {
  if (savingWebhook.value) {
    return;
  }
  savingWebhook.value = true;
  try {
    const payload: UpdateWebhookConfigRequest = {
      generic_url: genericUrl.value,
      dingtalk_url: dingtalkUrl.value,
      // 加签密钥留空表示保留已存密钥（与 SMTP 密码同口径）
      dingtalk_secret: dingtalkSecret.value,
      wecom_key: wecomKey.value
    };
    const result = await updateWebhookConfig(payload);
    if (result.success) {
      showToast(t("webhook.save_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["webhook-config"] });
    } else {
      showToast(`${t("webhook.save_failed")}: ${result.message}`, "error");
    }
  } finally {
    savingWebhook.value = false;
  }
}

const testingChannel = ref("");

async function testWebhookChannel(channel: "generic" | "dingtalk" | "wecom"): Promise<void> {
  if (testingChannel.value) {
    return;
  }
  testingChannel.value = channel;
  try {
    const result = await requestWebhookTest(channel);
    if (result.success) {
      showToast(t("webhook.test_success"), "success");
    } else {
      showToast(`${t("webhook.test_failed")}: ${result.message}`, "error");
    }
  } finally {
    testingChannel.value = "";
  }
}

const ipChange = ref(true);
const agentAlert = ref(true);
const snmpEvent = ref(true);
const webhookIpChange = ref(true);
const webhookAgentAlert = ref(true);
const webhookSnmpEvent = ref(true);
const selectedIds = ref<Set<string>>(new Set());

const usersQuery = useQuery({
  queryKey: ["notification-users"],
  queryFn: async () => {
    const result = await listUsersForPicker();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const notifyUsers = computed<User[]>(() => {
  const data = usersQuery.data.value;
  if (Array.isArray(data)) {
    return data;
  }
  return Array.isArray(data?.items) ? data.items : [];
});

const settingsQuery = useQuery({
  queryKey: ["notification-settings"],
  queryFn: async () => {
    const result = await getNotificationSettings();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

watch(
  () => settingsQuery.data.value,
  (settings) => {
    if (!settings) {
      return;
    }
    // 通知方式开关：后端未配置或字段缺失时默认全开（与后端 serde 默认值一致）
    // （边界断言：存量数据 methods 段可能缺省，按部分字段消费）
    const methods = (settings.methods ?? {}) as Partial<NotificationMethods>;
    ipChange.value = methods.ip_change !== false;
    agentAlert.value = methods.agent_alert !== false;
    snmpEvent.value = methods.snmp_event !== false;
    webhookIpChange.value = methods.webhook_ip_change !== false;
    webhookAgentAlert.value = methods.webhook_agent_alert !== false;
    webhookSnmpEvent.value = methods.webhook_snmp_event !== false;
    selectedIds.value = new Set(settings.email_recipients || []);
  }
);

function toggleRecipient(id: string): void {
  const next = new Set(selectedIds.value);
  if (next.has(id)) {
    next.delete(id);
  } else {
    next.add(id);
  }
  selectedIds.value = next;
}

const savingNotification = ref(false);

async function saveNotificationSettings(): Promise<void> {
  if (savingNotification.value) {
    return;
  }
  savingNotification.value = true;
  try {
    // 通知方式矩阵：行=通知类型，列=邮件/Webhook 两种方式
    const payload: NotificationSettings = {
      email_recipients: [...selectedIds.value],
      methods: {
        ip_change: ipChange.value,
        agent_alert: agentAlert.value,
        snmp_event: snmpEvent.value,
        webhook_ip_change: webhookIpChange.value,
        webhook_agent_alert: webhookAgentAlert.value,
        webhook_snmp_event: webhookSnmpEvent.value
      }
    };
    const result = await updateNotificationSettings(payload);
    if (result.success) {
      showToast(t("notification.save_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["notification-settings"] });
    } else {
      showToast(`${t("notification.save_failed")}: ${result.message}`, "error");
    }
  } finally {
    savingNotification.value = false;
  }
}
</script>

<template>
  <div class="tab-content" :class="{ active: props.active }">
    <div class="system-config-section">
      <!-- SMTP 服务器设置 -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">📧</span>
            <span>{{ t("smtp.server_settings") }}</span>
          </div>
          <div v-if="!readonly" class="card-header-actions">
            <button type="button" class="btn btn-secondary" :disabled="testingSmtp" @click="testSmtpConnection">
              {{ t("smtp.test_connection") }}
            </button>
            <button type="button" class="btn btn-primary" :disabled="savingSmtp" @click="saveSmtpConfig">
              {{ t("common.save") }}
            </button>
          </div>
        </header>
        <div class="form-row">
          <div class="form-group">
            <label for="smtp-host">{{ t("smtp.host") }}</label>
            <input id="smtp-host" v-model="smtpHost" type="text" :placeholder="t('smtp.host_placeholder')" :aria-label="t('smtp.host')">
          </div>
          <div class="form-group">
            <label for="smtp-port">{{ t("smtp.port") }}</label>
            <input id="smtp-port" v-model="smtpPort" type="number" placeholder="587" :aria-label="t('smtp.port')">
          </div>
          <div class="form-group">
            <label for="smtp-username">{{ t("smtp.username") }}</label>
            <input id="smtp-username" v-model="smtpUsername" type="text" :placeholder="t('smtp.username_placeholder')" :aria-label="t('smtp.username')" autocomplete="off">
          </div>
          <div class="form-group">
            <label for="smtp-password">{{ t("smtp.password") }}</label>
            <input
              id="smtp-password"
              v-model="smtpPassword"
              type="password"
              autocomplete="new-password"
              :placeholder="t(smtpHasPassword ? 'smtp.password_configured' : 'smtp.password_input')"
              :aria-label="t('smtp.password')"
            >
          </div>
          <div class="form-group">
            <label for="smtp-from">{{ t("smtp.from") }}</label>
            <input id="smtp-from" v-model="smtpFrom" type="email" :placeholder="t('smtp.from_placeholder')" :aria-label="t('smtp.from')" autocomplete="off">
          </div>
          <div class="form-group">
            <label for="smtp-secure-type">{{ t("smtp.secure_type") }}</label>
            <select id="smtp-secure-type" v-model="secureType" :aria-label="t('smtp.secure_type')">
              <option value="none">{{ t("smtp.ssl_none") }}</option>
              <option value="ssl">{{ t("smtp.ssl_ssl") }}</option>
              <option value="starttls">{{ t("smtp.ssl_starttls") }}</option>
            </select>
          </div>
        </div>
      </section>

      <!-- Webhook 通知设置（通用 / 钉钉 / 企业微信） -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🔗</span>
            <span>{{ t("webhook.settings") }}</span>
          </div>
          <div v-if="!readonly" class="card-header-actions">
            <button type="button" class="btn btn-primary" :disabled="savingWebhook" @click="saveWebhookConfig">
              {{ t("common.save") }}
            </button>
          </div>
        </header>
        <div class="form-row">
          <div class="form-group form-group-block">
            <label for="webhook-generic-url">{{ t("webhook.generic_url") }}</label>
            <input
              id="webhook-generic-url"
              v-model="genericUrl"
              type="url"
              :placeholder="t('webhook.generic_url_placeholder')"
              :aria-label="t('webhook.generic_url')"
              :disabled="readonly"
              autocomplete="off"
            >
            <div class="form-hint">{{ t("webhook.generic_hint") }}</div>
          </div>
          <div class="form-group form-group-block">
            <label for="webhook-dingtalk-url">{{ t("webhook.dingtalk_url") }}</label>
            <input
              id="webhook-dingtalk-url"
              v-model="dingtalkUrl"
              type="url"
              :placeholder="t('webhook.dingtalk_url_placeholder')"
              :aria-label="t('webhook.dingtalk_url')"
              :disabled="readonly"
              autocomplete="off"
            >
          </div>
          <div class="form-group form-group-block">
            <label for="webhook-dingtalk-secret">{{ t("webhook.dingtalk_secret") }}</label>
            <input
              id="webhook-dingtalk-secret"
              v-model="dingtalkSecret"
              type="password"
              autocomplete="new-password"
              :placeholder="t(dingtalkHasSecret ? 'webhook.dingtalk_secret_configured' : 'webhook.dingtalk_secret_input')"
              :aria-label="t('webhook.dingtalk_secret')"
              :disabled="readonly"
            >
          </div>
          <div class="form-group form-group-block">
            <label for="webhook-wecom-key">{{ t("webhook.wecom_key") }}</label>
            <input
              id="webhook-wecom-key"
              v-model="wecomKey"
              type="text"
              :placeholder="t('webhook.wecom_key_placeholder')"
              :aria-label="t('webhook.wecom_key')"
              :disabled="readonly"
              autocomplete="off"
            >
          </div>
          <div v-if="!readonly" class="form-group form-group-block">
            <span class="group-label">{{ t("webhook.test") }}</span>
            <div class="webhook-test-actions">
              <button
                type="button"
                class="btn btn-secondary"
                :disabled="testingChannel !== ''"
                @click="testWebhookChannel('generic')"
              >
                {{ t("webhook.test_generic") }}
              </button>
              <button
                type="button"
                class="btn btn-secondary"
                :disabled="testingChannel !== ''"
                @click="testWebhookChannel('dingtalk')"
              >
                {{ t("webhook.test_dingtalk") }}
              </button>
              <button
                type="button"
                class="btn btn-secondary"
                :disabled="testingChannel !== ''"
                @click="testWebhookChannel('wecom')"
              >
                {{ t("webhook.test_wecom") }}
              </button>
            </div>
          </div>
        </div>
      </section>

      <!-- 通知设置 -->
      <div class="notification-settings-section">
        <section class="config-card">
          <header class="config-card-header">
            <div class="config-card-title">
              <span class="icon">🔔</span>
              <span>{{ t("notification.settings") }}</span>
            </div>
            <button v-if="!readonly" type="button" class="btn btn-primary" :disabled="savingNotification" @click="saveNotificationSettings">
              {{ t("common.save") }}
            </button>
          </header>
          <!-- 通知方式矩阵：行=通知类型，列=邮件/Webhook 两种方式 -->
          <table class="notification-matrix">
            <thead>
              <tr>
                <th>{{ t("notification.col_type") }}</th>
                <th>{{ t("notification.col_email") }}</th>
                <th>{{ t("notification.col_webhook") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>{{ t("notification.type_ip_change") }}</td>
                <td><input v-model="ipChange" type="checkbox" :disabled="readonly"></td>
                <td><input v-model="webhookIpChange" type="checkbox" :disabled="readonly"></td>
              </tr>
              <tr>
                <td>{{ t("notification.type_agent_alert") }}</td>
                <td><input v-model="agentAlert" type="checkbox" :disabled="readonly"></td>
                <td><input v-model="webhookAgentAlert" type="checkbox" :disabled="readonly"></td>
              </tr>
              <tr>
                <td>{{ t("notification.type_snmp_event") }}</td>
                <td><input v-model="snmpEvent" type="checkbox" :disabled="readonly"></td>
                <td><input v-model="webhookSnmpEvent" type="checkbox" :disabled="readonly"></td>
              </tr>
            </tbody>
          </table>
          <div class="form-row">
            <div class="form-group form-group-block">
              <span class="group-label">{{ t("notification.email_recipients") }}</span>
              <div class="users-checkbox-list">
                <div v-if="notifyUsers.length === 0" class="user-checkbox-item">{{ t("common.no_data") }}</div>
                <div v-for="user in notifyUsers" v-else :key="user.id" class="user-checkbox-item">
                  <input
                    :id="`notify-user-${user.id}`"
                    type="checkbox"
                    :checked="selectedIds.has(user.id)"
                    :disabled="readonly"
                    @change="toggleRecipient(user.id)"
                  >
                  <label :for="`notify-user-${user.id}`" class="user-info">
                    <span class="user-name">{{ user.username }}</span>
                    <span class="user-email">{{ user.email }}</span>
                  </label>
                </div>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>
