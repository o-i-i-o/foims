<script setup lang="ts">
/**
 * 主机告警阈值配置弹窗（移植自旧版 modals/agents/agent-alert-threshold-modal.html
 * + agents.js openAlertThresholdModal/saveAlertThresholds）：
 * 四项阈值留空不监控，整数范围校验（百分比 1-100 / 温度 1-200）；
 * 保存后服务端立即评估一轮告警。
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

/** 四项阈值字段的键 */
type ThresholdKey = "cpu_pct" | "mem_pct" | "disk_pct" | "temp_c";

/** 保存载荷结构（与后端 AlertThresholds 对齐，未监控项为 null） */
interface ThresholdPayload {
  enabled: boolean;
  cpu_pct: number | null;
  mem_pct: number | null;
  disk_pct: number | null;
  temp_c: number | null;
}

/** 阈值字段定义：key 与取值范围（百分比 1-100 / 温度 1-200） */
const THRESHOLD_FIELDS = [
  { key: "cpu_pct", min: 1, max: 100 },
  { key: "mem_pct", min: 1, max: 100 },
  { key: "disk_pct", min: 1, max: 100 },
  { key: "temp_c", min: 1, max: 200 }
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

// 表单态：阈值为字符串（空串 = 不监控），回显与校验都按原始输入处理
const form = reactive<{ enabled: boolean } & Record<ThresholdKey, string | number>>({
  enabled: false,
  cpu_pct: "",
  mem_pct: "",
  disk_pct: "",
  temp_c: ""
});

watch(
  () => configQuery.data.value,
  (config) => {
    if (!config) {
      return;
    }
    form.enabled = config.enabled;
    for (const field of THRESHOLD_FIELDS) {
      form[field.key] = config[field.key] ?? "";
    }
  }
);

const saveMutation = useMutation({
  mutationFn: (payload: ThresholdPayload) =>
    apiPut<unknown, ThresholdPayload>("/api/agents/alert-thresholds", payload),
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

/** 保存告警阈值：前端范围校验后 PUT */
function handleSubmit() {
  const payload: ThresholdPayload = {
    enabled: form.enabled,
    cpu_pct: null,
    mem_pct: null,
    disk_pct: null,
    temp_c: null
  };
  for (const field of THRESHOLD_FIELDS) {
    const raw = String(form[field.key]).trim();
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
    </form>
    <template #footer>
      <label class="checkbox-label">
        <input v-model="form.enabled" type="checkbox">
        <span>{{ t("agents.alert_enabled") }}</span>
      </label>
      <button type="submit" class="btn btn-primary" form="agent-alert-threshold-form" :disabled="saveMutation.isPending.value">
        {{ t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
