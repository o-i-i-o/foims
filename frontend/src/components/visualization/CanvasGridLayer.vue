<script setup lang="ts">
/**
 * 画布网格底图：细网格 pattern + 覆盖矩形 + 主网格线与坐标标尺。
 *
 * 三张画布（工位/机柜/拓扑）共用同一套网格结构，只有 pattern id 与标尺字号
 * 分母不同。覆盖矩形由视口 composable 反算 letterbox 后传入，网格因此能铺满
 * 任意宽高比的容器，不再需要旧实现在两份 Core 里各写一遍 DOM 同步。
 */
import type { GridRulerData } from "../../utils/visualization/geometry";
import type { CanvasRect } from "../../types/visualization";

defineProps<{
  /** pattern 唯一 id（同页多张画布共存，不能重复） */
  patternId: string;
  /** 细网格间距（世界坐标像素） */
  gridSize: number;
  /** 与可视区等大的世界坐标矩形 */
  coverRect: CanvasRect;
  /** 主网格线与坐标标注（异常视野下为 null，只保留细网格底图） */
  ruler: GridRulerData | null;
}>();
</script>

<template>
  <defs>
    <pattern :id="patternId" :width="gridSize" :height="gridSize" patternUnits="userSpaceOnUse">
      <path
        class="grid-minor-line"
        :d="`M ${gridSize} 0 L 0 0 0 ${gridSize}`"
        fill="none"
        stroke-width="0.5"
      />
    </pattern>
  </defs>
  <rect
    :x="coverRect.x"
    :y="coverRect.y"
    :width="coverRect.width"
    :height="coverRect.height"
    :fill="`url(#${patternId})`"
  />
  <g class="grid-ruler">
    <template v-if="ruler">
      <line
        v-for="(line, index) in ruler.lines"
        :key="`line-${index}`"
        class="grid-major-line"
        :x1="line.x1"
        :y1="line.y1"
        :x2="line.x2"
        :y2="line.y2"
      />
      <text
        v-for="(label, index) in ruler.labels"
        :key="`label-${index}`"
        class="grid-label"
        :x="label.x"
        :y="label.y"
        :font-size="label.fontSize"
      >{{ label.text }}</text>
    </template>
  </g>
</template>
