<script setup lang="ts">
/**
 * Webhook 目标新增/编辑弹窗：名称（可选）+ 渠道类型 + 推送地址（必填，
 * 原生表单校验）+ 钉钉加签密钥（仅钉钉渠道展示，留空保留已存密钥）+
 * 接收内容订阅范围。确认仅回写页面草稿，统一落库由通知页"保存"承担。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import ModalShell from "../ModalShell.vue";
import type { WebhookChannel } from "../../types/api/generated/WebhookChannel";

export interface WebhookTargetDraft {
  id: string;
  name: string;
  channel: WebhookChannel;
  url: string;
  /** 新输入的加签密钥（空串 = 保留已存密钥） */
  secret: string;
  /** 是否已有落库密钥（决定密钥输入框 placeholder 口径） */
  hasSecret: boolean;
  /** 是否已随配置落库（未保存目标不可测试） */
  saved: boolean;
  ipChange: boolean;
  agentAlert: boolean;
  snmpEvent: boolean;
}

/** 弹窗确认回传：表单可编辑字段（id/saved 由列表侧维护） */
export interface WebhookTargetFormResult {
  name: string;
  channel: WebhookChannel;
  url: string;
  secret: string;
  ipChange: boolean;
  agentAlert: boolean;
  snmpEvent: boolean;
}

const props = withDefaults(defineProps<{ target?: WebhookTargetDraft | null }>(), {
  target: null
});

const emit = defineEmits<{ close: []; confirm: [result: WebhookTargetFormResult] }>();

const { t } = useI18n();

const formRef = ref<HTMLFormElement | null>(null);
const name = ref(props.target?.name ?? "");
const channel = ref<WebhookChannel>(props.target?.channel ?? "generic");
const url = ref(props.target?.url ?? "");
const secret = ref("");
const ipChange = ref(props.target?.ipChange ?? true);
const agentAlert = ref(props.target?.agentAlert ?? true);
const snmpEvent = ref(props.target?.snmpEvent ?? true);

/** 确认：原生校验通过后回传表单结果，由列表侧写回草稿 */
function handleConfirm(): void {
  const form = formRef.value;
  if (form && !form.checkValidity()) {
    form.reportValidity();
    return;
  }
  emit("confirm", {
    name: name.value,
    channel: channel.value,
    url: url.value,
    secret: secret.value,
    ipChange: ipChange.value,
    agentAlert: agentAlert.value,
    snmpEvent: snmpEvent.value
  });
}
</script>

<template>
  <ModalShell size="md" @close="emit('close')">
    <template #title>
      {{ t(target ? "webhook.edit_target" : "webhook.add_target") }}
    </template>

    <form ref="formRef">
      <div class="form-row">
        <div class="form-group">
          <label for="webhook-target-name">{{ t("webhook.target_name") }}</label>
          <input
            id="webhook-target-name"
            v-model="name"
            type="text"
            maxlength="100"
            :placeholder="t('webhook.target_name_placeholder')"
            :aria-label="t('webhook.target_name')"
          >
        </div>
        <div class="form-group">
          <label for="webhook-target-channel">{{ t("webhook.channel") }}<span class="required">*</span></label>
          <select id="webhook-target-channel" v-model="channel" :aria-label="t('webhook.channel')">
            <option value="generic">{{ t("webhook.channel_generic") }}</option>
            <option value="dingtalk">{{ t("webhook.channel_dingtalk") }}</option>
            <option value="wecom">{{ t("webhook.channel_wecom") }}</option>
          </select>
        </div>
      </div>
      <div class="form-group">
        <label for="webhook-target-url">{{ t("webhook.url") }}<span class="required">*</span></label>
        <input
          id="webhook-target-url"
          v-model="url"
          type="url"
          maxlength="2048"
          required
          autocomplete="off"
          :placeholder="t(channel === 'wecom' ? 'webhook.url_placeholder_wecom' : 'webhook.url_placeholder_generic')"
          :aria-label="t('webhook.url')"
        >
      </div>
      <div v-if="channel === 'dingtalk'" class="form-group">
        <label for="webhook-target-secret">{{ t("webhook.dingtalk_secret") }}</label>
        <input
          id="webhook-target-secret"
          v-model="secret"
          type="password"
          maxlength="256"
          autocomplete="new-password"
          :placeholder="t(props.target?.hasSecret ? 'webhook.dingtalk_secret_configured' : 'webhook.dingtalk_secret_input')"
          :aria-label="t('webhook.dingtalk_secret')"
        >
      </div>
      <div class="form-group">
        <label>{{ t("webhook.scope_label") }}</label>
        <div class="webhook-scope">
          <label class="webhook-scope-item">
            <input v-model="ipChange" type="checkbox">
            {{ t("notification.type_ip_change") }}
          </label>
          <label class="webhook-scope-item">
            <input v-model="agentAlert" type="checkbox">
            {{ t("notification.type_agent_alert") }}
          </label>
          <label class="webhook-scope-item">
            <input v-model="snmpEvent" type="checkbox">
            {{ t("notification.type_snmp_event") }}
          </label>
        </div>
      </div>
      <div class="form-hint">{{ t("webhook.url_hint") }}</div>
    </form>

    <template #footer>
      <button type="button" class="btn-secondary" @click="emit('close')">
        {{ t("common.cancel") }}
      </button>
      <button type="button" class="btn-primary" @click="handleConfirm">
        {{ t("common.confirm") }}
      </button>
    </template>
  </ModalShell>
</template>
