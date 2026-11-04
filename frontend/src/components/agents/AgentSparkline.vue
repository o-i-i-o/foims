<script setup lang="ts">
/**
 * 迷你趋势曲线：
 * 固定 viewBox（0 0 320 96）自绘 polyline，每序列独立配色（lineClass）。
 * max/min 标注由 formatValue 人性化；null/缺失值视为断点（分段 polyline），
 * 孤立单点渲染为实心圆点；全部无数据时渲染"暂无历史数据"。
 * 序列可携带波动带宽（band，min/max 包络多边形，半透明填充在主线下方），
 * 极值缺失（null）的区间不渲染多边形。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";

/** 单条曲线序列：values 为数值点（null 视为断点），lineClass 为配色类；
 * band 为可选波动带宽（min/max 与 values 同长同 x 轴对齐） */
interface SparkSeriesItem {
  values: Array<number | null>;
  lineClass: string;
  band?: { min: Array<number | null>; max: Array<number | null> } | null;
}

const props = withDefaults(
  defineProps<{
    /** 序列数组：每项 { values: (number|null)[], lineClass: string, band? } */
    series: SparkSeriesItem[];
    /** 数值标注格式化（max/min 标注） */
    formatValue: (value: number) => string;
    ariaLabel?: string;
  }>(),
  { ariaLabel: "" }
);

const { t } = useI18n();

// 画布尺寸与内边距（与固定 viewBox 一致）
const WIDTH = 320;
const HEIGHT = 96;
const PAD = 6;

const nums = computed(() =>
  props.series
    .flatMap((item) => [
      ...item.values,
      ...(item.band ? [...item.band.min, ...item.band.max] : [])
    ])
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
  const toY = (v: number) => HEIGHT - PAD - ((v - min) / span) * (HEIGHT - PAD * 2);
  return { max, min, step, toY };
});

// 连续非空点拼接 polyline：null 视为断点；孤立单点记录为圆点，
// 避免序列仅 1 点时曲线空白。band 有效区间（min/max 均有值且连续）拼
// 多边形：正向 max 边 + 反向 min 边闭合
const groups = computed(() => {
  if (!scale.value) {
    return [];
  }
  const { step, toY } = scale.value;
  return props.series.map((item) => {
    const segments: string[] = [];
    const singles: Array<{ cx: number; cy: number }> = [];
    let current: Array<[number, number]> = [];
    const flush = () => {
      if (current.length > 1) {
        segments.push(current.map((p) => p.join(",")).join(" "));
      } else if (current.length === 1) {
        // 长度为 1 时首元素必然存在（收窄 noUncheckedIndexedAccess）
        const point = current[0];
        if (point) {
          singles.push({ cx: point[0], cy: point[1] });
        }
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
    const band = item.band;
    const bandPolygons: string[] = [];
    if (band) {
      let maxs: Array<[number, number]> = [];
      let mins: Array<[number, number]> = [];
      const flushBand = () => {
        if (maxs.length > 1) {
          bandPolygons.push(
            [...maxs, ...mins.toReversed()].map((p) => p.join(",")).join(" ")
          );
        }
        maxs = [];
        mins = [];
      };
      item.values.forEach((_, i) => {
        const lo = band.min[i];
        const hi = band.max[i];
        if (lo == null || hi == null || !Number.isFinite(Number(lo)) || !Number.isFinite(Number(hi))) {
          flushBand();
          return;
        }
        const x = Number((PAD + i * step).toFixed(1));
        maxs.push([x, Number(toY(Number(hi)).toFixed(1))]);
        mins.push([x, Number(toY(Number(lo)).toFixed(1))]);
      });
      flushBand();
    }
    return { lineClass: item.lineClass, segments, singles, bandPolygons };
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
        <polygon v-for="(poly, bi) in group.bandPolygons" :key="`b-${bi}`" :points="poly" />
        <polyline v-for="(points, pi) in group.segments" :key="pi" :points="points" />
        <circle v-for="(dot, di) in group.singles" :key="di" :cx="dot.cx" :cy="dot.cy" r="3" />
      </g>
    </svg>
    <div class="agent-spark-range">
      <!-- v-else 分支仅在非空态渲染，此时 scale 恒有值（?. ?? 0 仅为收窄类型） -->
      <span>{{ formatValue(scale?.max ?? 0) }}</span>
      <span>{{ formatValue(scale?.min ?? 0) }}</span>
    </div>
  </template>
</template>
