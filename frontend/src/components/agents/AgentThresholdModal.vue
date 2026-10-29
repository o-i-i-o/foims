<script setup lang="ts">
/**
 * 主机告警阈值配置弹窗：
 * 四项阈值留空不监控，整数范围校验（百分比 1-100 / 温度 1-200）；
 * 另四项为全局抖动抑制窗口（持续时长 / 静默窗口 / 恢复冷却 / 迟滞带）。
 * 保存后服务端作废告警运行时缓存并立即补扫一轮。
 */
import { reactive, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import type { AlertThresholds } from "../../types/api/generated/AlertThresholds";
import { apiGet, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

/** 阈值字段键（可留空 = 不监控） */
type ThresholdKey = "cpu_pct" | "mem_pct" | "disk_pct" | "temp_c";
/** 抖动抑制窗口字段键（必填，0 = 关闭该项抑制） */
type WindowKey = "for_secs" | "quiet_window_secs" | "cooldown_secs" | "hysteresis_pct";

/** 阈值字段定义：key 与取值范围（百分比 1-100 / 温度 1-200） */
const THRESHOLD_FIELDS = [
  { key: "cpu_pct", min: 1, max: 100, label: "agents.alert_cpu" },
  { key: "mem_pct", min: 1, max: 100, label: "agents.alert_mem" },
  { key: "disk_pct", min: 1, max: 100, label: "agents.alert_disk" },
  { key: "temp_c", min: 1, max: 200, label: "agents.alert_temp" }
] as const;

/** 窗口字段定义：上限与后端 validate_thresholds 保持一致 */
const WINDOW_FIELDS = [
  { key: "for_secs", max: 3600, label: "agents.alert_for_secs" },
  { key: "quiet_window_secs", max: 86400, label: "agents.alert_quiet_window" },
  { key: "cooldown_secs", max: 86400, label: "agents.alert_cooldown" },
  { key: "hysteresis_pct", max: 50, label: "agents.alert_hysteresis" }
] as const;

// 弹窗经 v-if 挂载，每次打开都重新拉取当前配置
const configQuery = useQuery({
  queryKey: ["agent-thresholds"],
  queryFn: async (): Promise<AlertThresholds | null> => {
    const result = await apiGet<AlertThresholds>("/api/agents/alert-thresholds");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

// 表单态：数值一律按字符串收集（阈值空串 = 不监控，窗口空串按非法处理）
const form = reactive<{ enabled: boolean } & Record<ThresholdKey | WindowKey, string>>({
  enabled: false,
  cpu_pct: "",
  mem_pct: "",
  disk_pct: "",
  temp_c: "",
  for_secs: "15",
  quiet_window_secs: "300",
  cooldown_secs: "600",
  hysteresis_pct: "5"
});

watch(
  () => configQuery.data.value,
  (config) => {
    if (!config) {
      return;
    }
    form.enabled = config.enabled;
    for (const field of THRESHOLD_FIELDS) {
      form[field.key] = String(config[field.key] ?? "");
    }
    for (const field of WINDOW_FIELDS) {
      form[field.key] = String(config[field.key]);
    }
  }
);

const saveMutation = useMutation({
  mutationFn: (payload: AlertThresholds) =>
    apiPut<unknown, AlertThresholds>("/api/agents/alert-thresholds", payload),
  onSuccess: (result) => {
    if (result.success) {
      showToast(t("agents.alert_save_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["agent-thresholds"] });
      emit("close");
      return;
    }
    showToast(`${t("agents.alert_save_failed")}: ${result.message}`, "error");
  }
});

/** 保存告警配置：阈值与窗口分别校验后 PUT */
function handleSubmit() {
  const payload: AlertThresholds = {
    enabled: form.enabled,
    cpu_pct: null,
    mem_pct: null,
    disk_pct: null,
    temp_c: null,
    for_secs: 0,
    quiet_window_secs: 0,
    cooldown_secs: 0,
    hysteresis_pct: 0
  };
  for (const field of THRESHOLD_FIELDS) {
    const raw = form[field.key].trim();
    if (raw === "") {
      continue;
    }
    const value = Number(raw);
    if (!Number.isInteger(value) || value < field.min || value > field.max) {
      showToast(t("agents.alert_invalid_range", { min: field.min, max: field.max }), "error");
      return;
    }
    payload[field.key] = value;
  }
  for (const field of WINDOW_FIELDS) {
    const value = Number(form[field.key].trim());
    if (!Number.isInteger(value) || value < 0 || value > field.max) {
      showToast(
        t("agents.alert_invalid_window", { label: t(field.label), max: field.max }),
        "error"
      );
      return;
    }
    payload[field.key] = value;
  }
  saveMutation.mutate(payload);
}
</script>

<template>
  <ModalShell :title="t('agents.alert_threshold')" @close="emit('close')">
    <form id="agent-alert-threshold-form" @submit.prevent="handleSubmit">
      <div class="form-group">
        <label for="agent-alert-cpu"><span>{{ t("agents.alert_cpu") }}</span></label>
        <input
          id="agent-alert-cpu"
          v-model="form.cpu_pct"
          type="number"
          min="1"
          max="100"
          step="1"
          inputmode="numeric"
          :placeholder="t('agents.alert_placeholder_pct')"
          autocomplete="off"
        >
      </div>
      <div class="form-group">
        <label for="agent-alert-mem"><span>{{ t("agents.alert_mem") }}</span></label>
        <input
          id="agent-alert-mem"
          v-model="form.mem_pct"
          type="number"
          min="1"
          max="100"
          step="1"
          inputmode="numeric"
          :placeholder="t('agents.alert_placeholder_pct')"
          autocomplete="off"
        >
      </div>
      <div class="form-group">
        <label for="agent-alert-disk"><span>{{ t("agents.alert_disk") }}</span></label>
        <input
          id="agent-alert-disk"
          v-model="form.disk_pct"
          type="number"
          min="1"
          max="100"
          step="1"
          inputmode="numeric"
          :placeholder="t('agents.alert_placeholder_pct')"
          autocomplete="off"
        >
      </div>
      <div class="form-group">
        <label for="agent-alert-temp"><span>{{ t("agents.alert_temp") }}</span></label>
        <input
          id="agent-alert-temp"
          v-model="form.temp_c"
          type="number"
          min="1"
          max="200"
          step="1"
          inputmode="numeric"
          :placeholder="t('agents.alert_placeholder_temp')"
          autocomplete="off"
        >
      </div>
      <p class="hint-text">{{ t("agents.alert_window_group") }}</p>
      <div class="form-group">
        <label for="agent-alert-for-secs"><span>{{ t("agents.alert_for_secs") }}</span></label>
        <input
          id="agent-alert-for-secs"
          v-model="form.for_secs"
          type="number"
          min="0"
          max="3600"
          step="1"
          inputmode="numeric"
          autocomplete="off"
        >
        <small class="hint-text">{{ t("agents.alert_for_secs_help") }}</small>
      </div>
      <div class="form-group">
        <label for="agent-alert-quiet"><span>{{ t("agents.alert_quiet_window") }}</span></label>
        <input
          id="agent-alert-quiet"
          v-model="form.quiet_window_secs"
          type="number"
          min="0"
          max="86400"
          step="1"
          inputmode="numeric"
          autocomplete="off"
        >
        <small class="hint-text">{{ t("agents.alert_quiet_window_help") }}</small>
      </div>
      <div class="form-group">
        <label for="agent-alert-cooldown"><span>{{ t("agents.alert_cooldown") }}</span></label>
        <input
          id="agent-alert-cooldown"
          v-model="form.cooldown_secs"
          type="number"
          min="0"
          max="86400"
          step="1"
          inputmode="numeric"
          autocomplete="off"
        >
        <small class="hint-text">{{ t("agents.alert_cooldown_help") }}</small>
      </div>
      <div class="form-group">
        <label for="agent-alert-hysteresis"><span>{{ t("agents.alert_hysteresis") }}</span></label>
        <input
          id="agent-alert-hysteresis"
          v-model="form.hysteresis_pct"
          type="number"
          min="0"
          max="50"
          step="1"
          inputmode="numeric"
          autocomplete="off"
        >
        <small class="hint-text">{{ t("agents.alert_hysteresis_help") }}</small>
      </div>
    </form>
    <template #footer>
      <label class="checkbox-label">
        <input v-model="form.enabled" type="checkbox">
        <span>{{ t("agents.alert_enabled") }}</span>
      </label>
      <button type="submit" class="btn-primary" form="agent-alert-threshold-form" :disabled="saveMutation.isPending.value">
        {{ t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
