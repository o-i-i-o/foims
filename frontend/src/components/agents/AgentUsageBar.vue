<script setup lang="ts">
/**
 * 使用率迷你进度条：
 * >85% 红、>60% 橙、其余绿；未上报（null/非数值）显示 "-"。
 * 列表 CPU/内存/磁盘列与详情弹窗内存卡共用。
 */
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    /** 使用率百分数（数值或可转数值的字符串，null 表示未上报） */
    value?: number | string | null;
  }>(),
  { value: null }
);

const pct = computed(() => {
  if (props.value == null) {
    return null;
  }
  const num = Number(props.value);
  if (!Number.isFinite(num)) {
    return null;
  }
  return Math.min(100, Math.max(0, num));
});

// 使用率分档色：>85% 红、>60% 橙、其余绿
const level = computed(() => {
  if (pct.value === null) {
    return "";
  }
  if (pct.value > 85) {
    return "danger";
  }
  if (pct.value > 60) {
    return "warning";
  }
  return "ok";
});
</script>

<template>
  <span v-if="pct === null" class="agent-usage-none">-</span>
  <span v-else class="agent-usage">
    <span class="agent-usage-bar">
      <span class="agent-usage-fill" :class="`agent-usage-${level}`" :style="{ width: pct.toFixed(1) + '%' }"></span>
    </span>
    <span class="agent-usage-num">{{ pct.toFixed(1) }}%</span>
  </span>
</template>
