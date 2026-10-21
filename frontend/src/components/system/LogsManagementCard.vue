<script setup lang="ts">
/**
 * 日志管理卡片（移植自旧版 systemManager.js 的日志段）：统计三卡 /
 * 日志清理（天数校验 + 确认弹窗）/ 日志外发 syslog 配置与测试报文。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiGet, apiPost, apiPut } from "../../api/client";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import { formatDate } from "../../utils/formatter";
import ConfirmModal from "../ConfirmModal.vue";
import type { ClearLogsRequest } from "../../types/api/generated/ClearLogsRequest";

/** 单类日志统计（后端 json! 动态构造，未生成 ts-rs 类型） */
interface LogCategoryStats {
  count?: number;
  oldest?: string | null;
}

/** 日志统计载荷（GET /api/system/logs/stats） */
interface LogsStatsPayload {
  operation_logs?: LogCategoryStats | null;
  login_logs?: LogCategoryStats | null;
  notifications?: LogCategoryStats | null;
}

/** 日志外发配置载荷（GET 返回 / PUT 提交同形） */
interface LogForwardingPayload {
  enabled?: boolean;
  protocol?: string | null;
  host?: string | null;
  port?: number | null;
}

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

// ==================== 日志统计 ====================

const statsQuery = useQuery({
  queryKey: ["logs-stats"],
  queryFn: async () => {
    const result = await apiGet<LogsStatsPayload>("/api/system/logs/stats");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const stats = computed<LogsStatsPayload>(() => statsQuery.data.value ?? {});

// 最早记录时间（无记录时不显示）
function oldestText(value: string | null | undefined): string {
  if (!value) {
    return "";
  }
  return `${t("logs.earliest_label")}: ${formatDate(value)}`;
}

// ==================== 日志清理 ====================

const clearType = ref("operation");
const clearDays = ref("30");
const showClearConfirm = ref(false);
const clearConfirmMessage = ref("");
const clearing = ref(false);

function requestClearLogs(): void {
  const daysInput = (clearDays.value || "").trim();
  const parsedDays = Number.parseInt(daysInput, 10);
  // 空或非数字视为未填写：直接提示，不按 0（清空全部）处理，
  // 避免误触不可逆的全量清理，也避免确认文案与请求体出现 NaN/null
  if (daysInput === "" || Number.isNaN(parsedDays)) {
    showToast(t("logs.days_required"), "warning");
    return;
  }
  // 数值钳制到合法范围：负数归 0（0 表示删除全部），上限 36500 天（约百年）
  const days = Math.min(Math.max(parsedDays, 0), 36500);
  clearConfirmMessage.value = days === 0 ? t("logs.clear_all_confirm") : t("logs.clear_days_confirm", { days });
  showClearConfirm.value = true;
}

async function doClearLogs(): Promise<void> {
  showClearConfirm.value = false;
  if (clearing.value) {
    return;
  }
  clearing.value = true;
  try {
    const payload: ClearLogsRequest = {
      log_type: clearType.value,
      days: Math.min(Math.max(Number.parseInt(clearDays.value, 10) || 0, 0), 36500)
    };
    const result = await apiPost("/api/system/logs/clear", payload);
    if (result.success) {
      showToast(result.message, "success");
      queryClient.invalidateQueries({ queryKey: ["logs-stats"] });
    } else {
      showToast(`${t("logs.clear_failed")}: ${result.message}`, "error");
    }
  } finally {
    clearing.value = false;
  }
}

// ==================== 日志外发（syslog） ====================

const forwardingEnabled = ref("false");
const forwardingProtocol = ref("udp");
const forwardingHost = ref("");
const forwardingPort = ref("514");

const forwardingQuery = useQuery({
  queryKey: ["log-forwarding"],
  queryFn: async () => {
    const result = await apiGet<LogForwardingPayload>("/api/system/logs/forwarding");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

watch(
  () => forwardingQuery.data.value,
  (config) => {
    if (!config) {
      return;
    }
    // 未配置时后端返回默认值：关闭
    forwardingEnabled.value = config.enabled ? "true" : "false";
    forwardingProtocol.value = (config.protocol || "udp").toLowerCase();
    forwardingHost.value = config.host || "";
    forwardingPort.value = String(config.port || 514);
  }
);

const savingForwarding = ref(false);

async function saveLogForwarding(): Promise<void> {
  if (savingForwarding.value) {
    return;
  }
  savingForwarding.value = true;
  try {
    const payload: LogForwardingPayload = {
      enabled: forwardingEnabled.value === "true",
      protocol: forwardingProtocol.value || "udp",
      host: forwardingHost.value.trim(),
      port: Number.parseInt(forwardingPort.value, 10) || 514
    };
    const result = await apiPut("/api/system/logs/forwarding", payload);
    // 成功失败均使用后端返回的 message 提示
    showToast(result.message, result.success ? "success" : "error");
  } finally {
    savingForwarding.value = false;
  }
}

const testingForwarding = ref(false);

async function testLogForwarding(): Promise<void> {
  if (testingForwarding.value) {
    return;
  }
  testingForwarding.value = true;
  try {
    const result = await apiPost("/api/system/logs/forwarding/test", {});
    showToast(result.message, result.success ? "success" : "error");
  } finally {
    testingForwarding.value = false;
  }
}
</script>

<template>
  <section class="config-card">
    <header class="config-card-header">
      <div class="config-card-title">
        <span class="icon">📋</span>
        <span>{{ t("logs.manager") }}</span>
      </div>
    </header>
    <div class="data-management-container">
      <section class="section">
        <h3>{{ t("logs.stats") }}</h3>
        <div class="logs-stats-grid">
          <div class="log-stat-card">
            <div class="log-stat-title">{{ t("logs.operation_logs") }}</div>
            <div class="log-stat-value">{{ stats.operation_logs?.count ?? 0 }}</div>
            <div class="log-stat-info">{{ oldestText(stats.operation_logs?.oldest) }}</div>
          </div>
          <div class="log-stat-card">
            <div class="log-stat-title">{{ t("logs.login_logs") }}</div>
            <div class="log-stat-value">{{ stats.login_logs?.count ?? 0 }}</div>
            <div class="log-stat-info">{{ oldestText(stats.login_logs?.oldest) }}</div>
          </div>
          <div class="log-stat-card">
            <div class="log-stat-title">{{ t("logs.notifications") }}</div>
            <div class="log-stat-value">{{ stats.notifications?.count ?? 0 }}</div>
          </div>
        </div>
        <h3>{{ t("logs.clear_logs") }}</h3>
        <div class="clear-logs-form">
          <div class="form-row">
            <div class="form-group">
              <label for="clear-log-type">{{ t("logs.log_type") }}</label>
              <select id="clear-log-type" v-model="clearType" :disabled="readonly">
                <option value="operation">{{ t("logs.operation_logs") }}</option>
                <option value="login">{{ t("logs.login_logs") }}</option>
                <option value="notification">{{ t("logs.notifications") }}</option>
                <option value="all">{{ t("logs.all_logs") }}</option>
              </select>
            </div>
            <div class="form-group">
              <label for="clear-log-days">{{ t("logs.keep_days") }}</label>
              <input id="clear-log-days" v-model="clearDays" type="number" min="0" max="365" :placeholder="t('logs.clear_zero_hint')" :disabled="readonly">
            </div>
          </div>
          <button v-if="!readonly" type="button" class="btn btn-danger" :disabled="clearing" @click="requestClearLogs">
            {{ t("logs.clear") }}
          </button>
        </div>
      </section>
      <section class="section">
        <h3>{{ t("logs.forwarding_title") }}</h3>
        <div class="nc-fields">
          <div class="nc-field">
            <label for="log-forwarding-enabled">{{ t("logs.forwarding_enabled") }}</label>
            <select id="log-forwarding-enabled" v-model="forwardingEnabled" :disabled="readonly">
              <option value="true">{{ t("common.yes") }}</option>
              <option value="false">{{ t("common.no") }}</option>
            </select>
          </div>
          <div class="nc-field">
            <label for="log-forwarding-protocol">{{ t("logs.forwarding_protocol") }}</label>
            <select id="log-forwarding-protocol" v-model="forwardingProtocol" :disabled="readonly">
              <option value="udp">UDP</option>
              <option value="tcp">TCP</option>
            </select>
          </div>
          <div class="nc-field">
            <label for="log-forwarding-host">{{ t("logs.forwarding_host") }}</label>
            <input id="log-forwarding-host" v-model="forwardingHost" type="text" :placeholder="t('logs.forwarding_host_placeholder')" autocomplete="off" :disabled="readonly">
          </div>
          <div class="nc-field">
            <label for="log-forwarding-port">{{ t("logs.forwarding_port") }}</label>
            <input id="log-forwarding-port" v-model="forwardingPort" type="number" min="1" max="65535" placeholder="514" :disabled="readonly">
          </div>
        </div>
        <div v-if="!readonly" class="action-buttons">
          <button type="button" class="btn btn-secondary btn-sm" :disabled="testingForwarding" @click="testLogForwarding">
            {{ t("logs.forwarding_test") }}
          </button>
          <button type="button" class="btn btn-primary btn-sm" :disabled="savingForwarding" @click="saveLogForwarding">
            {{ t("common.save") }}
          </button>
        </div>
      </section>
    </div>
  </section>
  <ConfirmModal
    v-if="showClearConfirm"
    :message="clearConfirmMessage"
    :title="t('logs.clear_logs')"
    danger
    @confirm="doClearLogs"
    @cancel="showClearConfirm = false"
  />
</template>
