<script setup lang="ts">
/**
 * 机柜机位画布。
 *
 * 两点与工位画布不同：
 * 1. 虚拟横向滚动：保存的机柜 x 保留相对排列，柜数多时内容宽度可达数万像素，
 *    整张 SVG 一次性光栅化会失败。这里用「绝对定位 spacer 撑开滚动范围 +
 *    sticky SVG 固定左侧」只平移 viewBox.x，与旧实现同一手法。
 * 2. 机位是机柜的派生几何：机位矩形由机柜矩形 + U 位区间实时推导，
 *    拖动柜体时机位自动跟随，不需要旧实现里按 `data-relX/relY` 手工平移子元素。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import CanvasGridLayer from "./CanvasGridLayer.vue";
import { useCanvasTooltip } from "../../composables/useCanvasTooltip";
import { useCanvasViewport } from "../../composables/useCanvasViewport";
import {
  ALIGNMENT_GUIDE_COLOR,
  buildUMarks,
  clampNonNegative,
  computeAlignmentSegments,
  computeCabinetPositionRect,
  snapToGrid,
  type AlignmentSegment,
} from "../../utils/visualization/geometry";
import type {
  CabinetCanvasNode,
  CabinetPositionCanvasNode,
  CanvasRect,
} from "../../types/visualization";

const props = defineProps<{
  nodes: CabinetCanvasNode[];
  /** 内容总宽（横向滚动占位宽度） */
  totalWidth: number;
  /** 画布高度（= 渲染时容器高度，单 U 高据此定型） */
  canvasHeight: number;
  /** 递增即按当前滚动位置重新定型视野 */
  fitKey: number;
}>();

const emit = defineEmits<{
  /** 容器尺寸变化：父组件据此重算 U 高与柜底贴底位置 */
  resize: [size: { width: number; height: number }];
  "cabinet-click": [id: string];
  "position-click": [id: string];
  "cabinet-commit": [id: string, rect: CanvasRect];
}>();

const { t } = useI18n();

// 模板 ref：容器兼作横向滚动容器，SVG 用于坐标换算
const containerRef = ref<HTMLDivElement | null>(null);
const svgRef = ref<SVGSVGElement | null>(null);

const {
  viewBox,
  containerSize,
  coverRect,
  ruler,
  gridSize,
  setViewBox,
  toCanvasPoint,
  syncContainerSize,
} = useCanvasViewport({
  containerRef,
  svgRef,
  initialViewBox: { x: 0, y: 0, width: 800, height: 600 },
  rulerFontDivisor: 100,
  preserveAspectRatio: "xMidYMax meet",
});

const tooltipRef = ref<HTMLDivElement | null>(null);

const {
  visible: tooltipVisible,
  text: tooltipText,
  style: tooltipStyle,
  show: showTooltip,
  hide: hideTooltip,
} = useCanvasTooltip(tooltipRef);

interface DragState {
  id: string;
  start: { x: number; y: number };
  origin: CanvasRect;
  rect: CanvasRect;
  moved: boolean;
}

const drag = ref<DragState | null>(null);
const selectedId = ref<string | null>(null);

function effectiveCabinetRect(cabinet: CabinetCanvasNode): CanvasRect {
  const state = drag.value;
  return state && state.id === cabinet.id && state.moved ? state.rect : cabinet.rect;
}

interface CabinetRenderItem {
  cabinet: CabinetCanvasNode;
  rect: CanvasRect;
  name: string;
  tooltip: string;
  uMarks: ReturnType<typeof buildUMarks>;
  positions: { position: CabinetPositionCanvasNode; rect: CanvasRect; tooltip: string }[];
}

const renderCabinets = computed<CabinetRenderItem[]>(() =>
  props.nodes.map((cabinet) => {
    const rect = effectiveCabinetRect(cabinet);
    const uMarks = buildUMarks(cabinet.capacity, cabinet.uHeight, rect.width);
    const positions = cabinet.positions.map((position) => {
      const positionRect = computeCabinetPositionRect(
        rect,
        cabinet.capacity,
        cabinet.uHeight,
        position.start_u,
        position.end_u,
      );
      const ip = position.ipDetail;
      const startU = position.start_u > 0 ? position.start_u : 1;
      const endU = position.end_u >= startU ? position.end_u : startU;
      const ipAddress = ip ? ip.ip_address : t("viz.no_ip");
      const network = ip && (ip.network_name || ip.network_region)
        ? [ip.network_region, ip.network_name].filter(Boolean).join(" / ")
        : "";
      const tooltipLines = [
        `${t("viz.position_label")}: ${position.name}`,
        `${t("viz.u_position_label")}: ${startU}-${endU}`,
        `IP: ${ipAddress}`,
      ];
      if (network) {
        tooltipLines.push(`${t("viz.network_label")}: ${network}`);
      }
      tooltipLines.push(ip?.interface_name || t("viz.no_port"));
      return { position, rect: positionRect, tooltip: tooltipLines.join("\n") };
    });
    return {
      cabinet,
      rect,
      name: cabinet.name,
      tooltip: `${t("viz.cabinet_label")}: ${cabinet.name}\n${t("viz.capacity_label")}: ${cabinet.capacity}${t("viz.u_unit")}`,
      uMarks,
      positions,
    };
  }),
);

const alignment = computed<AlignmentSegment[]>(() => {
  const state = drag.value;
  if (!state || !state.moved) {
    return [];
  }
  const others = props.nodes
    .filter((cabinet) => cabinet.id !== state.id)
    .map((cabinet) => cabinet.rect);
  return computeAlignmentSegments(state.rect, others);
});

function startCabinetDrag(event: MouseEvent, cabinet: CabinetCanvasNode): void {
  if (event.button !== 0) {
    return;
  }
  const point = toCanvasPoint(event);
  if (!point) {
    return;
  }
  event.preventDefault();
  hideTooltip();
  selectedId.value = cabinet.id;
  drag.value = {
    id: cabinet.id,
    start: point,
    origin: { ...cabinet.rect },
    rect: { ...cabinet.rect },
    moved: false,
  };
  window.addEventListener("mousemove", onDragMove);
  window.addEventListener("mouseup", onDragEnd);
}

function onDragMove(event: MouseEvent): void {
  const state = drag.value;
  if (!state) {
    return;
  }
  const point = toCanvasPoint(event);
  if (!point) {
    return;
  }
  const dx = point.x - state.start.x;
  const dy = point.y - state.start.y;
  if (!state.moved && Math.abs(dx) <= 1 && Math.abs(dy) <= 1) {
    return;
  }
  state.moved = true;
  state.rect = { ...state.origin, x: state.origin.x + dx, y: state.origin.y + dy };
}

function onDragEnd(): void {
  window.removeEventListener("mousemove", onDragMove);
  window.removeEventListener("mouseup", onDragEnd);
  const state = drag.value;
  drag.value = null;
  if (!state) {
    return;
  }
  if (!state.moved) {
    emit("cabinet-click", state.id);
    return;
  }
  emit("cabinet-commit", state.id, {
    ...state.rect,
    x: clampNonNegative(Math.round(snapToGrid(state.rect.x))),
    y: clampNonNegative(Math.round(snapToGrid(state.rect.y))),
  });
}

let scrollRaf = 0;

/** 视野横向跟随滚动位置，纵向固定为渲染时容器高（滚动条出现不改变 U 高） */
function applyScrollViewBox(): void {
  const element = containerRef.value;
  if (!element) {
    return;
  }
  setViewBox({
    x: element.scrollLeft,
    y: 0,
    width: element.clientWidth || props.totalWidth || 800,
    height: props.canvasHeight || element.clientHeight || 600,
  });
}

function onScroll(): void {
  if (scrollRaf) {
    return;
  }
  scrollRaf = requestAnimationFrame(() => {
    scrollRaf = 0;
    applyScrollViewBox();
  });
}

watch(
  () => [props.totalWidth, props.canvasHeight, props.fitKey] as const,
  () => applyScrollViewBox(),
);

watch(containerSize, (size) => {
  emit("resize", { width: size.width, height: size.height });
  applyScrollViewBox();
});

onMounted(() => {
  syncContainerSize();
  applyScrollViewBox();
  emit("resize", { ...containerSize.value });
});

onBeforeUnmount(() => {
  if (scrollRaf) {
    cancelAnimationFrame(scrollRaf);
    scrollRaf = 0;
  }
  window.removeEventListener("mousemove", onDragMove);
  window.removeEventListener("mouseup", onDragEnd);
});

function onCanvasMouseMove(event: MouseEvent): void {
  if (drag.value) {
    hideTooltip();
    return;
  }
  const target = event.target instanceof Element
    ? event.target.closest<SVGElement>("[data-tooltip]")
    : null;
  const text = target?.getAttribute("data-tooltip") ?? "";
  if (text) {
    showTooltip(event, text);
  } else {
    hideTooltip();
  }
}

function onCanvasClick(event: MouseEvent): void {
  const hit = event.target instanceof Element ? event.target.closest("[data-id]") : null;
  if (!hit) {
    selectedId.value = null;
  }
}
</script>

<template>
  <div
    ref="containerRef"
    class="visualization-canvas visualization-canvas-scroll"
    role="img"
    :aria-label="t('visualization.cabinet')"
    :style="{ position: 'relative' }"
    @scroll="onScroll"
    @mouseenter="syncContainerSize"
  >
    <!-- 滚动占位：撑开内容宽度，自身不参与命中与绘制 -->
    <div
      class="cabinet-spacer"
      :style="{
        position: 'absolute',
        top: '0',
        left: '0',
        height: '1px',
        pointerEvents: 'none',
        width: `${totalWidth}px`,
      }"
    ></div>

    <div
      class="cabinet-sticky"
      :style="{ position: 'sticky', left: '0', width: '100%', height: '100%' }"
    >
      <svg
        ref="svgRef"
        class="visualization-svg"
        :viewBox="`${viewBox.x} ${viewBox.y} ${viewBox.width} ${viewBox.height}`"
        preserveAspectRatio="xMidYMax meet"
        @mousemove="onCanvasMouseMove"
        @mouseleave="hideTooltip()"
        @click="onCanvasClick"
      >
        <CanvasGridLayer
          pattern-id="grid-cabinet"
          :grid-size="gridSize"
          :cover-rect="coverRect"
          :ruler="ruler"
        />

        <g class="elements-group">
          <g
            v-for="item in renderCabinets"
            :key="item.cabinet.id"
            class="cabinet-element"
            :class="{ selected: selectedId === item.cabinet.id }"
            :transform="`translate(${item.rect.x}, ${item.rect.y})`"
            :data-id="item.cabinet.id"
            :data-tooltip="item.tooltip"
            @mousedown="startCabinetDrag($event, item.cabinet)"
          >
            <rect :width="item.rect.width" :height="item.rect.height" />
            <line
              v-for="(line, index) in item.uMarks.lines"
              :key="`u-line-${index}`"
              :x1="line.x1"
              :y1="line.y1"
              :x2="line.x2"
              :y2="line.y2"
              :style="{ stroke: 'var(--color-border-light)' }"
              stroke-width="0.5"
            />
            <text
              v-for="(label, index) in item.uMarks.labels"
              :key="`u-label-${index}`"
              class="u-mark"
              :x="label.x"
              :y="label.y"
            >{{ label.text }}</text>
            <text :x="item.rect.width / 2" :y="20">{{ item.name }}</text>

            <!-- 机位：相对柜体定位，拖动柜体时由视图模型重算矩形自动跟随 -->
            <g
              v-for="slot in item.positions"
              :key="slot.position.id"
              class="cabinet-position-element"
              :transform="`translate(${slot.rect.x - item.rect.x}, ${slot.rect.y - item.rect.y})`"
              :data-id="slot.position.id"
              :data-tooltip="slot.tooltip"
              @mousedown.stop
              @click.stop="emit('position-click', slot.position.id)"
            >
              <rect :width="slot.rect.width" :height="slot.rect.height" />
              <text :x="slot.rect.width / 2" :y="slot.rect.height / 2 + 4">
                {{ slot.position.name }}
              </text>
            </g>
          </g>
        </g>

        <g class="alignment-lines">
          <line
            v-for="(segment, index) in alignment"
            :key="index"
            :x1="segment.x1"
            :y1="segment.y1"
            :x2="segment.x2"
            :y2="segment.y2"
            :stroke="ALIGNMENT_GUIDE_COLOR"
            stroke-width="1"
            stroke-dasharray="4,4"
            opacity="0.7"
          />
        </g>
      </svg>
    </div>
  </div>

  <Teleport to="body">
    <div ref="tooltipRef" class="tooltip" :class="{ visible: tooltipVisible }" :style="tooltipStyle">
      {{ tooltipText }}
    </div>
  </Teleport>
</template>
