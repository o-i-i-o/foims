<script setup lang="ts">
/**
 * cron 表达式示例弹窗（静态示例表，在任务编辑弹窗之上叠加打开）。
 */
import { useI18n } from "vue-i18n";
import ModalShell from "../ModalShell.vue";

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

// 与后端解析器（croner）支持的语法一致：逗号列表 / 区间 / 步进 / L（当月最后一天）
const EXAMPLES: Array<{ expression: string; key: string }> = [
  { expression: "* * * * *", key: "every_1_minute" },
  { expression: "*/10 * * * *", key: "every_10_minutes" },
  { expression: "0 */1 * * *", key: "every_1_hour" },
  { expression: "30 21 * * *", key: "nightly_21_30" },
  { expression: "3,15 * * * *", key: "minute_3_and_15" },
  { expression: "3,15 8-11 * * *", key: "minute_3_15_8_to_11" },
  { expression: "0 0 L * *", key: "last_day_of_month" },
  { expression: "0 0 * * 0", key: "sunday_midnight" }
];
</script>

<template>
  <ModalShell :title="t('scheduled_tasks.cron_examples.title')" size="md" @close="emit('close')">
    <p class="hint-text">{{ t("scheduled_tasks.cron_examples.format_note") }}</p>
    <table class="data-table">
      <thead>
        <tr>
          <th>{{ t("scheduled_tasks.cron_examples.expression") }}</th>
          <th>{{ t("scheduled_tasks.cron_examples.description") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="example in EXAMPLES" :key="example.key">
          <td><code>{{ example.expression }}</code></td>
          <td>{{ t(`scheduled_tasks.cron_examples.${example.key}`) }}</td>
        </tr>
      </tbody>
    </table>
  </ModalShell>
</template>
