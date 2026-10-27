<script setup lang="ts">
/**
 * 工位布局画布。
 *
 * 旧实现在 SVGCore/SVGRenderer 里逐元素 createElementNS 并给每个文本打
 * `data-relX/relY` 补丁，拖动时用这些补丁手工重算子元素坐标。此处改为
 * 「节点 `<g>` 用 transform 定位、子元素全部相对坐标」：拖动只改一个
 * transform，子元素天然跟随，`data-rel*` 机制与机柜子元素联动一并消失。
 *
 * 坐标的唯一真相是父组件持有的视图模型：拖动期间只在组件内维护
 * `drag.rect` 预览，落定后 emit 给父组件写回并投影保存。
 */
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import CanvasGridLayer from "./CanvasGridLayer.vue";
import { useCanvasTooltip } from "../../composables/useCanvasTooltip";
import { useCanvasViewport } from "../../composables/useCanvasViewport";
import {
  ALIGNMENT_GUIDE_COLOR,
  clampNonNegative,
  computeAlignmentSegments,
  snapToGrid,
  type AlignmentSegment,
} from "../../utils/visualization/geometry";
import { contentViewBox } from "../../utils/visualization/viewModel";
import type { CanvasRect, DoorCanvasNode, WorkstationCanvasNode } from "../../types/visualization";

const props = defineProps<{
  nodes: WorkstationCanvasNode[];
  door: DoorCanvasNode;
  /** 递增即重新适配视野（布局加载完成 / 自动排布后由父组件触发） */
  fitKey: number;
}>();

const emit = defineEmits<{
  /** 单击工位：携带画布坐标供编辑弹窗回填 */
  "node-click": [id: string, position: { x: number; y: number }];
  /** 拖动落定：坐标已吸附对齐，父组件写回视图模型并保存布局 */
  "node-commit": [id: string, rect: CanvasRect];
  "door-commit": [id: string, rect: CanvasRect];
}>();

const { t } = useI18n();

// 模板 ref：容器测尺寸、SVG 做坐标换算，由本组件持有并交给视口 composable
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
  initialViewBox: { x: 0, y: 0, width: 1000, height: 800 },
  rulerFontDivisor: 100,
  preserveAspectRatio: "xMinYMin meet",
});

const tooltipRef = ref<HTMLDivElement | null>(null);

const {
  visible: tooltipVisible,
  text: tooltipText,
  style: tooltipStyle,
  show: showTooltip,
  hide: hideTooltip,
} = useCanvasTooltip(tooltipRef);

// ==================== 拖动（预览在组件内，落定交回父组件） ====================

type DragKind = "node" | "door";

interface DragState {
  id: string;
  kind: DragKind;
  /** 按下时的指针画布坐标 */
  start: { x: number; y: number };
  /** 按下时的元素矩形 */
  origin: CanvasRect;
  /** 拖动中的预览矩形 */
  rect: CanvasRect;
  moved: boolean;
}

const drag = ref<DragState | null>(null);
const selectedId = ref<string | null>(null);

/** 拖动中的元素用预览矩形，其余用视图模型坐标 */
function effectiveRect(rect: CanvasRect, id: string): CanvasRect {
  const state = drag.value;
  return state && state.id === id && state.moved ? state.rect : rect;
}

const renderNodes = computed(() =>
  props.nodes.map((node) => {
    const ip = node.ipDetail;
    const ipAddress = ip ? ip.ip_address : t("viz.no_ip");
    const portInfo = ip?.interface_name || t("viz.no_port");
    const network = ip && (ip.network_name || ip.network_region)
      ? [ip.network_region, ip.network_name].filter(Boolean).join(" / ")
      : "";
    // 工位管理人可空：未指定时显示「暂不指定」，避免卡片出现 "null" 或空白
    const managerName = node.manager_name || t("workstation.manager_unset");
    const tooltipLines = [
      `${t("viz.workstation_label")}: ${node.name}`,
      `IP: ${ipAddress}`,
    ];
    if (network) {
      tooltipLines.push(`${t("viz.network_label")}: ${network}`);
    }
    tooltipLines.push(`${t("viz.port_label")}: ${portInfo}`);
    tooltipLines.push(`${t("viz.manager_label")}: ${managerName}`);
    return {
      node,
      rect: effectiveRect(node.rect, node.id),
      statusClass: `status-${ip?.status ?? "unknown"}`,
      tooltip: tooltipLines.join("\n"),
      lines: [
        { y: 30, label: `${t("viz.workstation_label")}: `, value: node.name, valueClass: "workstation-name" },
        { y: 55, label: "IP: ", value: ipAddress, valueClass: "workstation-ip" },
        { y: 80, label: `${t("viz.port_label")}: `, value: portInfo, valueClass: "" },
        { y: 105, label: `${t("viz.manager_label")}: `, value: managerName, valueClass: "workstation-manager" },
      ],
    };
  }),
);

const renderDoor = computed(() => ({
  rect: effectiveRect(props.door.rect, props.door.id),
  tooltip: t("viz.room_entrance_ref"),
}));

/** 对齐参考线：拖动中的节点与其他节点的中心/缘线在阈值内对齐时给出虚线 */
const alignment = computed<AlignmentSegment[]>(() => {
  const state = drag.value;
  if (!state || !state.moved || state.kind !== "node") {
    return [];
  }
  const others = props.nodes
    .filter((node) => node.id !== state.id)
    .map((node) => node.rect);
  return computeAlignmentSegments(state.rect, others);
});

function startDrag(
  event: MouseEvent,
  id: string,
  kind: DragKind,
  rect: CanvasRect,
): void {
  if (event.button !== 0) {
    return;
  }
  const point = toCanvasPoint(event);
  if (!point) {
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  hideTooltip();
  selectedId.value = id;
  drag.value = {
    id,
    kind,
    start: point,
    origin: { ...rect },
    rect: { ...rect },
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
    if (state.kind === "node") {
      emit("node-click", state.id, { x: state.origin.x, y: state.origin.y });
    }
    return;
  }
  // 落定：吸附网格 + 非负钳制（与坐标输入框 min=0 的口径一致）
  const rect: CanvasRect = {
    ...state.rect,
    x: clampNonNegative(Math.round(snapToGrid(state.rect.x))),
    y: clampNonNegative(Math.round(snapToGrid(state.rect.y))),
  };
  if (state.kind === "door") {
    emit("door-commit", state.id, rect);
  } else {
    emit("node-commit", state.id, rect);
  }
}

// ==================== 视野适配 ====================

function applyFit(): void {
  const rects = [props.door.rect, ...props.nodes.map((node) => node.rect)];
  setViewBox(contentViewBox(rects, containerSize.value));
}

watch(
  () => props.fitKey,
  () => applyFit(),
);

// 隐藏页签下容器高宽为 0：切回可见时按真实尺寸重新适配（工位画布无可手动缩放）
watch(containerSize, () => applyFit());

onMounted(() => {
  syncContainerSize();
  applyFit();
});

// ==================== 提示与选中 ====================

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
    :aria-label="t('visualization.workstation')"
    @mouseenter="syncContainerSize"
  >
    <svg
      ref="svgRef"
      class="visualization-svg"
      :viewBox="`${viewBox.x} ${viewBox.y} ${viewBox.width} ${viewBox.height}`"
      preserveAspectRatio="xMinYMin meet"
      @mousemove="onCanvasMouseMove"
      @mouseleave="hideTooltip()"
      @click="onCanvasClick"
    >
      <CanvasGridLayer
        pattern-id="grid-workstation"
        :grid-size="gridSize"
        :cover-rect="coverRect"
        :ruler="ruler"
      />

      <g class="elements-group">
        <g
          v-for="item in renderNodes"
          :key="item.node.id"
          class="workstation-element"
          :class="[item.statusClass, { selected: selectedId === item.node.id }]"
          :transform="`translate(${item.rect.x}, ${item.rect.y})`"
          :data-id="item.node.id"
          :data-tooltip="item.tooltip"
          @mousedown="startDrag($event, item.node.id, 'node', item.node.rect)"
        >
          <rect :width="item.rect.width" :height="item.rect.height" />
          <text
            v-for="line in item.lines"
            :key="line.y"
            :x="item.rect.width / 2"
            :y="line.y"
          >
            <tspan>{{ line.label }}</tspan>
            <tspan v-if="line.valueClass" :class="line.valueClass">{{ line.value }}</tspan>
            <tspan v-else>{{ line.value }}</tspan>
          </text>
        </g>

        <g
          class="door-element"
          :class="{ selected: selectedId === door.id }"
          :transform="`translate(${renderDoor.rect.x}, ${renderDoor.rect.y})`"
          :data-id="door.id"
          :data-tooltip="renderDoor.tooltip"
          @mousedown="startDrag($event, door.id, 'door', door.rect)"
        >
          <rect
            :width="renderDoor.rect.width"
            :height="renderDoor.rect.height"
            :style="{ fill: 'var(--color-bg-secondary)', stroke: 'var(--color-text-secondary)' }"
            stroke-width="2"
          />
          <circle
            :cx="renderDoor.rect.width - 10"
            :cy="renderDoor.rect.height / 2"
            r="3"
            :style="{ fill: 'var(--color-text-secondary)' }"
          />
          <text :x="renderDoor.rect.width / 2" y="-10">{{ t("viz.door") }}</text>
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

  <Teleport to="body">
    <div ref="tooltipRef" class="tooltip" :class="{ visible: tooltipVisible }" :style="tooltipStyle">
      {{ tooltipText }}
    </div>
  </Teleport>
</template>
