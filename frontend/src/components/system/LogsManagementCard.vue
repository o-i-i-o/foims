<script setup lang="ts">
/**
 * 日志管理卡片：统计三卡 /
 * 日志清理（天数校验 + 确认弹窗）/ 日志外发 syslog 配置与测试报文。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import SystemSectionCard from "./SystemSectionCard.vue";
import {
  clearLogs,
  getLogForwarding,
  getLogStats,
  previewLogsClear,
  saveLogForwarding as saveLogForwardingConfig,
  testLogForwarding as sendLogForwardingTest
} from "../../api/logs";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import { formatDate } from "../../utils/formatter";
import ConfirmModal from "../ConfirmModal.vue";
import type { LogForwarding, LogsStats } from "../../api/logs";
import type { ClearLogsRequest } from "../../types/api/generated/ClearLogsRequest";

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

const statsQuery = useQuery({
  queryKey: ["logs-stats"],
  queryFn: async () => {
    const result = await getLogStats();
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const stats = computed<LogsStats>(() => statsQuery.data.value ?? {});

// 最早记录时间（无记录时不显示）
function oldestText(value: string | null | undefined): string {
  if (!value) {
    return "";
  }
  return `${t("logs.earliest_label")}: ${formatDate(value)}`;
}

const clearType = ref("operation");
// type="number" 的 v-model 会在输入后把值转换为 number，故类型为 string | number，
// 解析时统一经 String() 归一（直接对 number 调 trim 会抛 TypeError）
const clearDays = ref<string | number>("30");
const showClearConfirm = ref(false);
const clearConfirmMessage = ref("");
const clearing = ref(false);
// 确认通过后实际提交的保留天数：requestClearLogs 解析并钳制后暂存，
// doClearLogs 直接使用，避免二次解析时输入被清空导致误按 0（删除全部）处理
const pendingClearDays = ref(0);

// 与后端 clear_logs_core 的 MAX_KEEP_DAYS 同口径的保留天数上限（约 10 年）
const MAX_KEEP_DAYS = 3650;

/** 解析保留天数输入：空/非数字返回 null（未填写），否则钳制到 0..=3650 */
function parseClearDays(): number | null {
  // v-model(type="number") 输入后值为 number、清空后为 ""，统一转字符串处理
  const daysInput = String(clearDays.value ?? "").trim();
  const parsedDays = Number.parseInt(daysInput, 10);
  // 空或非数字视为未填写：直接提示，不按 0（清空全部）处理，
  // 避免误触不可逆的全量清理，也避免确认文案与请求体出现 NaN/null
  if (daysInput === "" || Number.isNaN(parsedDays)) {
    return null;
  }
  // 数值钳制到合法范围：负数归 0（0 表示删除全部）
  return Math.min(Math.max(parsedDays, 0), MAX_KEEP_DAYS);
}

async function requestClearLogs(): Promise<void> {
  const days = parseClearDays();
  if (days === null) {
    showToast(t("logs.days_required"), "warning");
    return;
  }
  pendingClearDays.value = days;
  clearing.value = true;
  try {
    // 预览将删除的条数供确认弹窗展示（与清理同参同口径）；统计失败时
    // 降级为不带数量的既有文案，清理主流程不因预览接口受阻
    const payload: ClearLogsRequest = { log_type: clearType.value, days };
    const result = await previewLogsClear(payload);
    const count = result.success ? result.data?.total : undefined;
    if (days === 0) {
      clearConfirmMessage.value =
        typeof count === "number" ? t("logs.clear_all_confirm_count", { count }) : t("logs.clear_all_confirm");
    } else {
      clearConfirmMessage.value =
        typeof count === "number"
          ? t("logs.clear_days_confirm_count", { days, count })
          : t("logs.clear_days_confirm", { days });
    }
    showClearConfirm.value = true;
  } finally {
    clearing.value = false;
  }
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
      days: pendingClearDays.value
    };
    const result = await clearLogs(payload);
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

const forwardingEnabled = ref("false");
const forwardingProtocol = ref("udp");
const forwardingHost = ref("");
const forwardingPort = ref("514");

const forwardingQuery = useQuery({
  queryKey: ["log-forwarding"],
  queryFn: async () => {
    const result = await getLogForwarding();
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
    const payload: LogForwarding = {
      enabled: forwardingEnabled.value === "true",
      protocol: forwardingProtocol.value || "udp",
      host: forwardingHost.value.trim(),
      port: Number.parseInt(forwardingPort.value, 10) || 514
    };
    const result = await saveLogForwardingConfig(payload);
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
    const result = await sendLogForwardingTest();
    showToast(result.message, result.success ? "success" : "error");
  } finally {
    testingForwarding.value = false;
  }
}
</script>

<template>
  <SystemSectionCard icon="logs" :title="t('logs.manager')">
    <div class="section-title-row">
      <h3>{{ t("logs.stats") }}</h3>
    </div>
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
        <div class="log-stat-info">{{ oldestText(stats.notifications?.oldest) }}</div>
      </div>
    </div>

    <div class="section-title-row">
      <h3>{{ t("logs.clear_logs") }}</h3>
      <button v-if="!readonly" type="button" class="btn btn-danger btn-sm" :disabled="clearing" @click="requestClearLogs">
        {{ t("logs.clear") }}
      </button>
    </div>
    <!-- 清理条件直落字段行（原灰底盒已随统一卡片布局退役） -->
    <div class="form-row form-row-2col">
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
        <input id="clear-log-days" v-model="clearDays" type="number" min="0" max="3650" :placeholder="t('logs.clear_zero_hint')" :disabled="readonly">
      </div>
    </div>

    <div class="section-title-row">
      <h3>{{ t("logs.forwarding_title") }}</h3>
      <div v-if="!readonly" class="action-buttons">
        <button type="button" class="btn btn-secondary btn-sm" :disabled="testingForwarding" @click="testLogForwarding">
          {{ t("logs.forwarding_test") }}
        </button>
        <button type="button" class="btn btn-primary btn-sm" :disabled="savingForwarding" @click="saveLogForwarding">
          {{ t("common.save") }}
        </button>
      </div>
    </div>
    <div class="form-row form-row-2col">
      <div class="form-group">
        <label for="log-forwarding-enabled">{{ t("logs.forwarding_enabled") }}</label>
        <select id="log-forwarding-enabled" v-model="forwardingEnabled" :disabled="readonly" :aria-label="t('logs.forwarding_enabled')">
          <option value="true">{{ t("common.yes") }}</option>
          <option value="false">{{ t("common.no") }}</option>
        </select>
      </div>
      <div class="form-group">
        <label for="log-forwarding-protocol">{{ t("logs.forwarding_protocol") }}</label>
        <select id="log-forwarding-protocol" v-model="forwardingProtocol" :disabled="readonly" :aria-label="t('logs.forwarding_protocol')">
          <option value="udp">UDP</option>
          <option value="tcp">TCP</option>
        </select>
      </div>
      <div class="form-group">
        <label for="log-forwarding-host">{{ t("logs.forwarding_host") }}</label>
        <input id="log-forwarding-host" v-model="forwardingHost" type="text" :placeholder="t('logs.forwarding_host_placeholder')" autocomplete="off" :disabled="readonly" :aria-label="t('logs.forwarding_host')">
      </div>
      <div class="form-group">
        <label for="log-forwarding-port">{{ t("logs.forwarding_port") }}</label>
        <input id="log-forwarding-port" v-model="forwardingPort" type="number" min="1" max="65535" placeholder="514" :disabled="readonly" :aria-label="t('logs.forwarding_port')">
      </div>
    </div>
  </SystemSectionCard>
  <ConfirmModal
    v-if="showClearConfirm"
    :message="clearConfirmMessage"
    :title="t('logs.clear_logs')"
    danger
    @confirm="doClearLogs"
    @cancel="showClearConfirm = false"
  />
</template>
