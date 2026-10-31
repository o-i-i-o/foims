<script setup lang="ts">
/**
 * 通知配置页签：SMTP 服务器卡片（未配置属正常业务态）+ 邮件通知卡片
 * （每收件人独立订阅通知类型）+ Webhook 通知卡片（多目标表格行内编辑，
 * 每条独立渠道/地址/加签密钥/订阅范围）。两渠道彼此独立、互不依赖。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import {
  getNotificationRecipients,
  getSmtpConfig,
  getWebhookConfig,
  testSmtpConnection as requestSmtpTest,
  testWebhookTarget as requestWebhookTest,
  updateNotificationRecipients,
  updateSmtpConfig,
  updateWebhookConfig
} from "../../api/system";
import { listUsersForPicker } from "../../api/users";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import type { UpdateSmtpConfigRequest } from "../../types/api/generated/UpdateSmtpConfigRequest";
import type { UpdateWebhookConfigRequest } from "../../types/api/generated/UpdateWebhookConfigRequest";
import type { WebhookChannel } from "../../types/api/generated/WebhookChannel";
import type { NotificationRecipients } from "../../types/api/generated/NotificationRecipients";
import type { MailRecipient } from "../../types/api/generated/MailRecipient";
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

// ---- Webhook 通知（多目标：每条独立渠道 / 地址 / 加签密钥 / 订阅范围） ----

/** Webhook 目标编辑态草稿（saved 标记是否已落库，未保存目标不可测试） */
interface WebhookTargetDraft {
  id: string;
  name: string;
  channel: WebhookChannel;
  url: string;
  secret: string;
  hasSecret: boolean;
  saved: boolean;
  ipChange: boolean;
  agentAlert: boolean;
  snmpEvent: boolean;
}

const webhookTargets = ref<WebhookTargetDraft[]>([]);

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
    webhookTargets.value = (config.targets ?? []).map((target) => ({
      id: target.id,
      name: target.name,
      channel: target.channel,
      url: target.url,
      secret: "",
      hasSecret: Boolean(target.has_secret),
      saved: true,
      ipChange: target.ip_change !== false,
      agentAlert: target.agent_alert !== false,
      snmpEvent: target.snmp_event !== false
    }));
  }
);

function addWebhookTarget(): void {
  webhookTargets.value.push({
    id: crypto.randomUUID(),
    name: "",
    channel: "generic",
    url: "",
    secret: "",
    hasSecret: false,
    saved: false,
    ipChange: true,
    agentAlert: true,
    snmpEvent: true
  });
}

function removeWebhookTarget(index: number): void {
  webhookTargets.value.splice(index, 1);
}

const savingWebhook = ref(false);

async function saveWebhookConfig(): Promise<void> {
  if (savingWebhook.value) {
    return;
  }
  savingWebhook.value = true;
  try {
    const payload: UpdateWebhookConfigRequest = {
      targets: webhookTargets.value.map((target) => ({
        id: target.id,
        name: target.name,
        channel: target.channel,
        url: target.url,
        // 加签密钥留空表示保留已存密钥（与 SMTP 密码同口径）
        secret: target.secret,
        ip_change: target.ipChange,
        agent_alert: target.agentAlert,
        snmp_event: target.snmpEvent
      }))
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

const testingTargetId = ref("");

async function testWebhookTarget(target: WebhookTargetDraft): Promise<void> {
  if (testingTargetId.value !== "" || !target.saved) {
    return;
  }
  testingTargetId.value = target.id;
  try {
    const result = await requestWebhookTest(target.id);
    if (result.success) {
      showToast(t("webhook.test_success"), "success");
    } else {
      showToast(`${t("webhook.test_failed")}: ${result.message}`, "error");
    }
  } finally {
    testingTargetId.value = "";
  }
}

// ---- 邮件收件人（每个收件人独立订阅通知类型） ----

type ScopeField = "ip_change" | "agent_alert" | "snmp_event";

const recipients = ref<MailRecipient[]>([]);

const recipientsQuery = useQuery({
  queryKey: ["notification-recipients"],
  queryFn: async () => {
    const result = await getNotificationRecipients();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

watch(
  () => recipientsQuery.data.value,
  (data) => {
    recipients.value = data?.recipients ?? [];
  }
);

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

/** 用户的订阅范围：未出现在收件人列表中视为全不订阅（不接收） */
function scopeOf(userId: string): MailRecipient {
  return (
    recipients.value.find((r) => r.user_id === userId) ?? {
      user_id: userId,
      ip_change: false,
      agent_alert: false,
      snmp_event: false
    }
  );
}

function setScope(userId: string, field: ScopeField, value: boolean): void {
  const list = recipients.value.slice();
  const existing = list.find((r) => r.user_id === userId);
  const next: MailRecipient = existing
    ? { ...existing }
    : { user_id: userId, ip_change: false, agent_alert: false, snmp_event: false };
  next[field] = value;
  if (existing) {
    list[list.indexOf(existing)] = next;
  } else {
    list.push(next);
  }
  recipients.value = list;
}

const savingRecipients = ref(false);

async function saveRecipients(): Promise<void> {
  if (savingRecipients.value) {
    return;
  }
  savingRecipients.value = true;
  try {
    // 三类全不勾 = 不接收，不作为收件人保存
    const payload: NotificationRecipients = {
      recipients: recipients.value.filter(
        (r) => r.ip_change || r.agent_alert || r.snmp_event
      )
    };
    const result = await updateNotificationRecipients(payload);
    if (result.success) {
      showToast(t("notification.save_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["notification-recipients"] });
    } else {
      showToast(`${t("notification.save_failed")}: ${result.message}`, "error");
    }
  } finally {
    savingRecipients.value = false;
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

      <!-- 邮件通知（每收件人独立订阅通知类型） -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🔔</span>
            <span>{{ t("notification.settings") }}</span>
          </div>
          <button v-if="!readonly" type="button" class="btn btn-primary" :disabled="savingRecipients" @click="saveRecipients">
            {{ t("common.save") }}
          </button>
        </header>
        <table class="recipients-table">
          <thead>
            <tr>
              <th>{{ t("notification.col_user") }}</th>
              <th>{{ t("notification.type_ip_change") }}</th>
              <th>{{ t("notification.type_agent_alert") }}</th>
              <th>{{ t("notification.type_snmp_event") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="notifyUsers.length === 0">
              <td colspan="4">{{ t("common.no_data") }}</td>
            </tr>
            <tr v-for="user in notifyUsers" v-else :key="user.id">
              <td>
                <span class="user-name">{{ user.username }}</span>
                <span class="user-email">{{ user.email }}</span>
              </td>
              <td>
                <input
                  type="checkbox"
                  :checked="scopeOf(user.id).ip_change"
                  :disabled="readonly"
                  :aria-label="`${user.username} ${t('notification.type_ip_change')}`"
                  @change="setScope(user.id, 'ip_change', ($event.target as HTMLInputElement).checked)"
                >
              </td>
              <td>
                <input
                  type="checkbox"
                  :checked="scopeOf(user.id).agent_alert"
                  :disabled="readonly"
                  :aria-label="`${user.username} ${t('notification.type_agent_alert')}`"
                  @change="setScope(user.id, 'agent_alert', ($event.target as HTMLInputElement).checked)"
                >
              </td>
              <td>
                <input
                  type="checkbox"
                  :checked="scopeOf(user.id).snmp_event"
                  :disabled="readonly"
                  :aria-label="`${user.username} ${t('notification.type_snmp_event')}`"
                  @change="setScope(user.id, 'snmp_event', ($event.target as HTMLInputElement).checked)"
                >
              </td>
            </tr>
          </tbody>
        </table>
        <div class="form-hint">{{ t("notification.recipients_hint") }}</div>
      </section>

      <!-- Webhook 通知设置（多目标表格行内编辑，每条独立渠道/地址/订阅） -->
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🔗</span>
            <span>{{ t("webhook.settings") }}</span>
          </div>
          <div v-if="!readonly" class="card-header-actions">
            <button type="button" class="btn btn-secondary" @click="addWebhookTarget">
              {{ t("webhook.add_target") }}
            </button>
            <button type="button" class="btn btn-primary" :disabled="savingWebhook" @click="saveWebhookConfig">
              {{ t("common.save") }}
            </button>
          </div>
        </header>
        <div class="form-hint">{{ t("webhook.url_hint") }}</div>
        <table class="data-table webhook-table">
          <thead>
            <tr>
              <th>{{ t("webhook.target_name") }}</th>
              <th>{{ t("webhook.channel") }}</th>
              <th>{{ t("webhook.url") }}</th>
              <th>{{ t("webhook.dingtalk_secret") }}</th>
              <th>{{ t("webhook.scope_label") }}</th>
              <th>{{ t("common.actions") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="webhookTargets.length === 0">
              <td colspan="6">{{ t("webhook.empty") }}</td>
            </tr>
            <tr v-for="(target, index) in webhookTargets" v-else :key="target.id">
              <td>
                <input
                  v-model="target.name"
                  type="text"
                  class="cell-input"
                  :placeholder="t('webhook.target_name_placeholder')"
                  :aria-label="t('webhook.target_name')"
                  :disabled="readonly"
                >
              </td>
              <td>
                <select v-model="target.channel" class="cell-input" :aria-label="t('webhook.channel')" :disabled="readonly">
                  <option value="generic">{{ t("webhook.channel_generic") }}</option>
                  <option value="dingtalk">{{ t("webhook.channel_dingtalk") }}</option>
                  <option value="wecom">{{ t("webhook.channel_wecom") }}</option>
                </select>
              </td>
              <td>
                <input
                  v-model="target.url"
                  type="url"
                  class="cell-input"
                  :placeholder="t(target.channel === 'wecom' ? 'webhook.url_placeholder_wecom' : 'webhook.url_placeholder_generic')"
                  :aria-label="t('webhook.url')"
                  :disabled="readonly"
                  autocomplete="off"
                >
              </td>
              <td>
                <input
                  v-if="target.channel === 'dingtalk'"
                  v-model="target.secret"
                  type="password"
                  class="cell-input"
                  autocomplete="new-password"
                  :placeholder="t(target.hasSecret ? 'webhook.dingtalk_secret_configured' : 'webhook.dingtalk_secret_input')"
                  :aria-label="t('webhook.dingtalk_secret')"
                  :disabled="readonly"
                >
                <span v-else class="cell-muted">—</span>
              </td>
              <td>
                <div class="webhook-scope">
                  <label class="webhook-scope-item">
                    <input v-model="target.ipChange" type="checkbox" :disabled="readonly">
                    {{ t("notification.type_ip_change") }}
                  </label>
                  <label class="webhook-scope-item">
                    <input v-model="target.agentAlert" type="checkbox" :disabled="readonly">
                    {{ t("notification.type_agent_alert") }}
                  </label>
                  <label class="webhook-scope-item">
                    <input v-model="target.snmpEvent" type="checkbox" :disabled="readonly">
                    {{ t("notification.type_snmp_event") }}
                  </label>
                </div>
              </td>
              <td class="webhook-actions-cell">
                <button
                  type="button"
                  class="btn btn-secondary btn-sm"
                  :disabled="readonly || !target.saved || testingTargetId !== ''"
                  :title="target.saved ? '' : t('webhook.target_unsaved')"
                  @click="testWebhookTarget(target)"
                >
                  {{ t("webhook.test") }}
                </button>
                <button
                  type="button"
                  class="btn btn-danger btn-sm"
                  :disabled="readonly"
                  @click="removeWebhookTarget(index)"
                >
                  {{ t("common.delete") }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </section>
    </div>
  </div>
</template>
