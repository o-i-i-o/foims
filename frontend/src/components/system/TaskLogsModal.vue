<script setup lang="ts">
/**
 * 任务执行日志弹窗（固定取最近 50 条，五键分页响应取 items）。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../../api/client";
import { formatDateTime } from "../../utils/formatter";
import type { Paged } from "../../types/api";
import type { TaskLog } from "../../types/api/generated/TaskLog";
import ModalShell from "../ModalShell.vue";

const props = defineProps<{
  taskName: string;
}>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

const logsQuery = useQuery({
  queryKey: ["task-logs", props.taskName],
  queryFn: async () => {
    const result = await apiGet<Paged<TaskLog>>(
      `/api/system/scheduled-tasks/logs?task_name=${encodeURIComponent(props.taskName)}&page=1&page_size=50`
    );
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

const logs = computed<TaskLog[]>(() => logsQuery.data.value?.items ?? []);

// 状态徽章：成功为 active，其余原样展示
function statusClass(status: string): string {
  return status === "success" ? "status-active" : "status-inactive";
}

// TaskLog.details 为 JsonValue：仅 plain object 时读取 message/error 文案
function detailText(log: TaskLog): string {
  const details = log.details;
  if (details !== null && typeof details === "object" && !Array.isArray(details)) {
    const value = details.message || details.error;
    return typeof value === "string" ? value : "";
  }
  return "";
}
</script>

<template>
  <ModalShell :title="t('scheduled_tasks.logs_title')" size="lg" @close="emit('close')">
    <table class="data-table">
      <thead>
        <tr>
          <th>{{ t("common.name") }}</th>
          <th>{{ t("common.status") }}</th>
          <th>{{ t("scheduled_tasks.execution_time") }}</th>
          <th>{{ t("scheduled_tasks.duration") }}</th>
          <th>{{ t("scheduled_tasks.details") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-if="logs.length === 0">
          <td colspan="5" class="no-data">{{ t("common.no_data") }}</td>
        </tr>
        <tr v-for="(log, index) in logs" v-else :key="index">
          <td>{{ log.task_name }}</td>
          <td><span class="status-badge" :class="statusClass(log.status)">{{ log.status }}</span></td>
          <td>{{ formatDateTime(log.start_time) }}</td>
          <td>{{ String(log.duration ?? 0) }}</td>
          <td>{{ detailText(log) || "-" }}</td>
        </tr>
      </tbody>
    </table>
  </ModalShell>
</template>
