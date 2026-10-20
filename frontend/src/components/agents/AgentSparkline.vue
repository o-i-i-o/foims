<script setup>
/**
 * 迷你趋势曲线（移植自旧版 agents.js renderSparklineSeries）：
 * 固定 viewBox（0 0 320 96）自绘 polyline，每序列独立配色（lineClass）。
 * max/min 标注由 formatValue 人性化；null/缺失值视为断点（分段 polyline），
 * 孤立单点渲染为实心圆点；全部无数据时渲染"暂无历史数据"。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";

const props = defineProps({
  /** 序列数组：每项 { values: (number|null)[], lineClass: string } */
  series: { type: Array, required: true },
  /** 数值标注格式化（max/min 标注） */
  formatValue: { type: Function, required: true },
  ariaLabel: { type: String, default: "" }
});

const { t } = useI18n();

// 与旧版一致的画布尺寸与内边距
const WIDTH = 320;
const HEIGHT = 96;
const PAD = 6;

const nums = computed(() =>
  props.series
    .flatMap((item) => item.values)
    .filter((v) => v != null && Number.isFinite(Number(v)))
    .map(Number)
);

const empty = computed(() => nums.value.length === 0);

const scale = computed(() => {
  if (empty.value) {
    return null;
  }
  const max = Math.max(...nums.value);
  const min = Math.min(...nums.value);
  const span = max - min || 1;
  const len = Math.max(...props.series.map((item) => item.values.length));
  const step = (WIDTH - PAD * 2) / Math.max(len - 1, 1);
  const toY = (v) => HEIGHT - PAD - ((v - min) / span) * (HEIGHT - PAD * 2);
  return { max, min, step, toY };
});

// 连续非空点拼接 polyline：null 视为断点；孤立单点记录为圆点，
// 避免序列仅 1 点时曲线空白
const groups = computed(() => {
  if (!scale.value) {
    return [];
  }
  const { step, toY } = scale.value;
  return props.series.map((item) => {
    const segments = [];
    const singles = [];
    let current = [];
    const flush = () => {
      if (current.length > 1) {
        segments.push(current.map((p) => p.join(",")).join(" "));
      } else if (current.length === 1) {
        singles.push({ cx: current[0][0], cy: current[0][1] });
      }
      current = [];
    };
    item.values.forEach((v, i) => {
      if (v == null || !Number.isFinite(Number(v))) {
        flush();
        return;
      }
      current.push([Number((PAD + i * step).toFixed(1)), Number(toY(Number(v)).toFixed(1))]);
    });
    flush();
    return { lineClass: item.lineClass, segments, singles };
  });
});
</script>

<template>
  <p v-if="empty" class="agent-spark-empty">{{ t("agents.no_history") }}</p>
  <template v-else>
    <svg class="agent-spark" :viewBox="`0 0 ${WIDTH} ${HEIGHT}`" preserveAspectRatio="none" role="img" :aria-label="ariaLabel">
      <line class="agent-spark-gridline" :x1="PAD" :y1="HEIGHT / 3" :x2="WIDTH - PAD" :y2="HEIGHT / 3"></line>
      <line class="agent-spark-gridline" :x1="PAD" :y1="(HEIGHT / 3) * 2" :x2="WIDTH - PAD" :y2="(HEIGHT / 3) * 2"></line>
      <g v-for="(group, index) in groups" :key="index" class="agent-spark-line" :class="group.lineClass">
        <polyline v-for="(points, pi) in group.segments" :key="pi" :points="points" />
        <circle v-for="(dot, di) in group.singles" :key="di" :cx="dot.cx" :cy="dot.cy" r="3" />
      </g>
    </svg>
    <div class="agent-spark-range">
      <span>{{ formatValue(scale.max) }}</span>
      <span>{{ formatValue(scale.min) }}</span>
    </div>
  </template>
</template>
