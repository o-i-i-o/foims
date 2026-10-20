<script setup>
/**
 * 应用层 Fail2ban 卡片（移植自旧版 fail2banManager.js）：状态徽章 +
 * 配置（findtime/max_retry/bantime）+ 已封禁/追踪 IP 表 + 手动封禁/解禁。
 * 封禁/解禁与保存均为写操作，auditor/user（system 分区只读）隐藏。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiGet, apiPost, apiPut } from "../../api/client.js";
import { isSectionReadonly } from "../../utils/roleCapabilities.js";
import { showToast } from "../../composables/useToast.js";
import ConfirmModal from "../ConfirmModal.vue";
import NavIcon from "../NavIcon.vue";

const props = defineProps({
  active: { type: Boolean, default: false }
});

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

const statusQuery = useQuery({
  queryKey: ["fail2ban-app"],
  queryFn: async () => {
    const result = await apiGet("/api/system/fail2ban/app/status");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const status = computed(() => statusQuery.data.value || {});
const bannedIps = computed(() => (Array.isArray(status.value.banned_ips) ? status.value.banned_ips : []));
const trackedIps = computed(() => (Array.isArray(status.value.tracked_ips) ? status.value.tracked_ips : []));

// ==================== 配置表单 ====================

const enabled = ref("false");
const findtime = ref("");
const maxRetry = ref("");
const bantime = ref("");

watch(
  () => statusQuery.data.value,
  (data) => {
    if (!data) {
      return;
    }
    enabled.value = String(Boolean(data.enabled));
    findtime.value = data.findtime ?? "";
    maxRetry.value = data.max_retry ?? "";
    bantime.value = data.bantime ?? "";
  }
);

const saving = ref(false);

async function saveConfig() {
  if (saving.value) {
    return;
  }
  saving.value = true;
  try {
    const result = await apiPut("/api/system/fail2ban/app/config", {
      enabled: enabled.value === "true",
      findtime: Number.parseInt(findtime.value, 10),
      max_retry: Number.parseInt(maxRetry.value, 10),
      bantime: Number.parseInt(bantime.value, 10)
    });
    if (result.success) {
      showToast(t("security.config_saved"), "success");
      queryClient.invalidateQueries({ queryKey: ["fail2ban-app"] });
    } else {
      // ApiClient 失败响应的错误文案在 message 字段
      showToast(result.message || t("common.save_failed"), "error");
    }
  } finally {
    saving.value = false;
  }
}

// ==================== 封禁 / 解封 ====================

const manualIp = ref("");
// 解封确认目标（表格行内与手动解封共用）
const unbanTarget = ref("");

async function doUnban(ip) {
  const result = await apiPost("/api/system/fail2ban/app/unban", { ip });
  if (result.success) {
    showToast(result.data?.message || t("security.unban_success"), "success");
    queryClient.invalidateQueries({ queryKey: ["fail2ban-app"] });
    return true;
  }
  showToast(result.message || t("security.unban_failed"), "error");
  return false;
}

async function handleUnbanConfirmed() {
  const ip = unbanTarget.value;
  unbanTarget.value = "";
  await doUnban(ip);
  // 仅解封成功才清空手动输入框，失败时保留内容便于修正后重试
  if (manualIp.value.trim() === ip) {
    manualIp.value = "";
  }
}

async function banIp() {
  const ip = manualIp.value.trim();
  if (!ip) {
    showToast(t("security.input_ip"), "warning");
    return;
  }
  const result = await apiPost("/api/system/fail2ban/app/ban", { ip });
  if (result.success) {
    showToast(result.data?.message || t("security.ban_success"), "success");
    manualIp.value = "";
    queryClient.invalidateQueries({ queryKey: ["fail2ban-app"] });
  } else {
    showToast(result.message || t("security.ban_failed"), "error");
  }
}

function unbanIp() {
  const ip = manualIp.value.trim();
  if (!ip) {
    showToast(t("security.input_ip"), "warning");
    return;
  }
  unbanTarget.value = ip;
}
</script>

<template>
  <section class="config-card">
    <header class="config-card-header">
      <div class="config-card-title">
        <span class="icon">🛡️</span>
        <span>{{ t("security.app_fail2ban") }}</span>
      </div>
      <div class="card-header-actions">
        <span class="status-badge" :class="status.enabled ? 'status-active' : 'status-inactive'">
          {{ status.enabled ? t("security.status_active") : t("security.status_inactive") }}
        </span>
        <button type="button" class="btn btn-secondary btn-sm" @click="() => queryClient.invalidateQueries({ queryKey: ['fail2ban-app'] })">
          {{ t("common.refresh") }}
        </button>
        <button v-if="!readonly" type="button" class="btn btn-primary btn-sm" :disabled="saving" @click="saveConfig">
          {{ t("common.save") }}
        </button>
      </div>
    </header>

    <!-- 配置 -->
    <div class="nc-fields">
      <div class="nc-field">
        <label for="app-fail2ban-enabled">{{ t("security.enabled") }}</label>
        <select id="app-fail2ban-enabled" v-model="enabled" :disabled="readonly">
          <option value="true">{{ t("common.yes") }}</option>
          <option value="false">{{ t("common.no") }}</option>
        </select>
      </div>
      <div class="nc-field">
        <label for="app-fail2ban-findtime">{{ t("security.findtime") }}</label>
        <input id="app-fail2ban-findtime" v-model="findtime" type="number" min="60" max="86400" :disabled="readonly">
      </div>
      <div class="nc-field">
        <label for="app-fail2ban-maxretry">{{ t("security.maxretry") }}</label>
        <input id="app-fail2ban-maxretry" v-model="maxRetry" type="number" min="1" max="100" :disabled="readonly">
      </div>
      <div class="nc-field">
        <label for="app-fail2ban-bantime">{{ t("security.bantime") }}</label>
        <input id="app-fail2ban-bantime" v-model="bantime" type="number" min="60" max="604800" :disabled="readonly">
      </div>
    </div>

    <!-- 已封禁 IP -->
    <h4 class="mt-md">{{ t("security.banned_ips") }}</h4>
    <div class="table-container">
      <table class="data-table">
        <thead>
          <tr>
            <th>{{ t("security.ip_address") }}</th>
            <th class="col-center">{{ t("security.remaining_seconds") }}</th>
            <th v-if="!readonly" class="col-center">{{ t("common.actions") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="bannedIps.length === 0" class="empty-row">
            <td class="text-center" :colspan="readonly ? 2 : 3">{{ t("common.no_data") }}</td>
          </tr>
          <tr v-for="item in bannedIps" v-else :key="item.ip">
            <td>{{ item.ip }}</td>
            <td class="col-center">{{ item.remaining_seconds }}</td>
            <td v-if="!readonly" class="col-center">
              <button
                type="button"
                class="icon-btn btn-success"
                :data-tooltip="t('security.unban_ip')"
                :aria-label="t('security.unban_ip')"
                @click="unbanTarget = item.ip"
              >
                <NavIcon name="unlock" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- 追踪中的 IP -->
    <h4 class="mt-md">{{ t("security.tracked_ips") }}</h4>
    <div class="table-container">
      <table class="data-table">
        <thead>
          <tr>
            <th>{{ t("security.ip_address") }}</th>
            <th class="col-center">{{ t("security.failure_count") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="trackedIps.length === 0" class="empty-row">
            <td colspan="2" class="text-center">{{ t("common.no_data") }}</td>
          </tr>
          <tr v-for="item in trackedIps" v-else :key="item.ip">
            <td>{{ item.ip }}</td>
            <td class="col-center">{{ item.failure_count }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- 手动封禁/解封 -->
    <template v-if="!readonly">
      <h4 class="mt-md">{{ t("security.manual_ban") }}</h4>
      <div class="form-group-inline">
        <label for="app-fail2ban-ip-input" class="inline-field-label">{{ t("system.app_fail2ban_ip") }}</label>
        <input id="app-fail2ban-ip-input" v-model="manualIp" type="text" placeholder="192.168.1.100" autocomplete="off">
        <button type="button" class="btn btn-danger btn-sm" @click="banIp">
          {{ t("security.ban_ip") }}
        </button>
        <button type="button" class="btn btn-success btn-sm" @click="unbanIp">
          {{ t("security.unban_ip") }}
        </button>
      </div>
    </template>
  </section>

  <ConfirmModal
    v-if="unbanTarget"
    :message="`${t('security.unban_confirm')}: ${unbanTarget}`"
    @confirm="handleUnbanConfirmed"
    @cancel="unbanTarget = ''"
  />
</template>
