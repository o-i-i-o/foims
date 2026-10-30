<script setup lang="ts">
/**
 * 拓扑画布。
 *
 * 旧实现把「画什么」（TopologyRenderer）与「怎么交互」（TopologyCore）拆成两个
 * 命令式类，交互结果靠 `dataset` 反读、连线靠删节点重画。Vue 化后：
 *
 * - 渲染是纯投影：节点/容器/连线都由 `nodes` + `connections` 经 computed 派生，
 *   拖动期间只在组件内维护预览矩形，连线路径随之自动重算，无需增量重画。
 * - 交互统一在 `<svg>` 上做事件委托：命中 `.port-anchor` / `[data-device-id]` /
 *   `.topology-container-group` 后分派，与旧实现的判定顺序一致。
 * - 缩放不再回写工具栏 DOM：`zoomScale` 经 `zoom-change` 上报，工具栏双向绑定。
 */
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import CanvasGridLayer from "./CanvasGridLayer.vue";
import { useCanvasTooltip } from "../../composables/useCanvasTooltip";
import { useCanvasViewport } from "../../composables/useCanvasViewport";
import { showToast } from "../../composables/useToast";
import {
  CABINET_HEADER,
  CABINET_PADDING,
  ROOM_HEADER,
  ROOM_PADDING,
  clampNonNegative,
  computeGroupBox,
  computeOrthogonalPath,
  computeTempConnectionPath,
  fitRects,
  nodeEdgeAnchor,
  pickAnchor,
  roomGroupKey,
  snapToGrid,
  type GroupBox,
} from "../../utils/visualization/geometry";
import { cabinetGroupKeyOf, groupTopologyNodes } from "../../utils/visualization/viewModel";
import { deviceIconFile, loadDeviceIconMarkup } from "../../utils/visualization/icons";
import type {
  AnchorDirection,
  AnchorPoint,
  CanvasRect,
  TopologyCanvasNode,
  TopologyConnection,
} from "../../types/visualization";

const props = defineProps<{
  nodes: TopologyCanvasNode[];
  connections: TopologyConnection[];
  /** 递增即重新适配视野（加载完成 / 自动布局 / 组织筛选后由父组件触发） */
  fitKey: number;
}>();

const emit = defineEmits<{
  "node-click": [deviceId: string];
  /** 拖动落定：父组件按 rect 写回节点并做分组推挤/保存 */
  "node-drag-end": [deviceId: string, rect: CanvasRect];
  "container-click": [kind: ContainerKind, key: string, box: CanvasRect];
  /** 容器拖动落定：offset 为组内节点的整体位移 */
  "container-drag-end": [fixedKey: string, fixedGroupKey: string | null, offset: { x: number; y: number }];
  "connection-click": [connectionId: string];
  "connection-complete": [
    sourceDeviceId: string,
    sourcePortId: string | null,
    targetDeviceId: string,
    targetPortId: string | null,
  ];
  "canvas-click": [];
  "zoom-change": [scale: number];
}>();

type ContainerKind = "room" | "cabinet";

const { t } = useI18n();

// 模板 ref：容器测尺寸并承载平移光标，SVG 用于坐标换算
const containerRef = ref<HTMLDivElement | null>(null);
const svgRef = ref<SVGSVGElement | null>(null);

const ICON_SIZE = 32;
const TEXT_OFFSET_X = 16;
const FIT_PADDING = 60;
const EMPTY_VIEW: CanvasRect = { x: 0, y: 0, width: 3000, height: 2000 };

const DEVICE_TYPE_I18N_KEYS: Record<string, string> = {
  switch: "device_type.switch",
  network_device: "device_type.network_device",
  server: "device_type.server",
  router: "device_type.router",
  camera: "device_type.camera",
  phone: "device_type.phone",
  desktop: "device_type.desktop",
  laptop: "device_type.laptop",
  printer: "device_type.printer",
  other: "device_type.other",
};

const ANCHOR_DIRECTIONS: readonly AnchorDirection[] = ["top", "right", "bottom", "left"];

function deviceTypeLabel(type: string | null): string {
  const key = type ? DEVICE_TYPE_I18N_KEYS[type] : undefined;
  return key ? t(key) : (type ?? "");
}

const {
  viewBox,
  containerSize,
  coverRect,
  ruler,
  zoomScale,
  gridSize,
  setViewBox,
  toCanvasPoint,
  syncContainerSize,
  setZoom,
  zoomBy,
  zoomAt,
} = useCanvasViewport({
  containerRef,
  svgRef,
  initialViewBox: { ...EMPTY_VIEW },
  rulerFontDivisor: 150,
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

/** 用户是否手动平移/缩放过视野：false 时容器尺寸变化自动重新适配内容 */
const userAdjusted = ref(false);
/** 空格键按住（拖拽平移手势） */
const spaceDown = ref(false);
/** 平移中（容器 cursor 切换 grabbing） */
const panning = ref(false);

type Interaction =
  | {
      kind: "pan";
      startClientX: number;
      startClientY: number;
      startViewBox: CanvasRect;
    }
  | {
      kind: "node";
      deviceId: string;
      start: { x: number; y: number };
      origin: CanvasRect;
      rect: CanvasRect;
      moved: boolean;
    }
  | {
      kind: "container";
      containerKind: ContainerKind;
      key: string;
      start: { x: number; y: number };
      offset: { x: number; y: number };
      moved: boolean;
      fixedKey: string;
      fixedGroupKey: string | null;
    };

const interaction = ref<Interaction | null>(null);
const selectedNodeId = ref<string | null>(null);
const selectedConnectionId = ref<string | null>(null);
const connectionMode = ref(false);
const connectionSource = ref<{
  deviceId: string;
  portDir: AnchorDirection;
  x: number;
  y: number;
} | null>(null);
const tempPath = ref("");

/** 拖动预览下的节点矩形（落定前不写回父组件数据） */
function effectiveRect(node: TopologyCanvasNode): CanvasRect {
  const state = interaction.value;
  if (!state) {
    return node.rect;
  }
  if (state.kind === "node" && state.deviceId === node.device_id && state.moved) {
    return state.rect;
  }
  if (state.kind === "container" && state.moved) {
    const inGroup = state.containerKind === "cabinet"
      ? cabinetGroupKeyOf(node) === state.key
      : roomGroupKey(node.room_id) === state.key;
    if (inGroup) {
      return {
        ...node.rect,
        x: node.rect.x + state.offset.x,
        y: node.rect.y + state.offset.y,
      };
    }
  }
  return node.rect;
}

// 图标（本地 lucide，异步一次）

const iconCache = reactive(new Map<string, string>());
const requestedIcons = new Set<string>();

function iconMarkupOf(type: string | null): string {
  return iconCache.get(deviceIconFile(type)) ?? "";
}

function ensureIcons(): void {
  for (const node of props.nodes) {
    const file = deviceIconFile(node.device_type);
    if (requestedIcons.has(file)) {
      continue;
    }
    requestedIcons.add(file);
    void loadDeviceIconMarkup(node.device_type).then((markup) => {
      if (markup) {
        iconCache.set(file, markup);
      }
    });
  }
}

watch(
  () => props.nodes.map((node) => node.device_type ?? "").join("|"),
  () => ensureIcons(),
  { immediate: true },
);

const layoutNodes = computed<TopologyCanvasNode[]>(() =>
  props.nodes.map((node) => ({ ...node, rect: effectiveRect(node) })),
);

const groups = computed(() =>
  groupTopologyNodes(layoutNodes.value, {
    noCabinet: t("viz.group_no_cabinet"),
  }),
);

interface ContainerShape {
  kind: ContainerKind;
  key: string;
  label: string;
  sublabel: string;
  count: number;
  box: GroupBox;
}

const containerShapes = computed<ContainerShape[]>(() => {
  const shapes: ContainerShape[] = [];
  for (const room of groups.value) {
    const roomBox = computeGroupBox(room.nodes, ROOM_PADDING, ROOM_HEADER);
    if (!roomBox) {
      continue;
    }
    shapes.push({
      kind: "room",
      key: room.key,
      label: room.label,
      sublabel: room.orgLabel,
      count: room.nodes.length,
      box: roomBox,
    });
    for (const cabinet of room.cabinets) {
      const cabinetBox = computeGroupBox(cabinet.nodes, CABINET_PADDING, CABINET_HEADER);
      if (!cabinetBox) {
        continue;
      }
      shapes.push({
        kind: "cabinet",
        key: cabinet.key,
        label: cabinet.label,
        sublabel: "",
        count: cabinet.nodes.length,
        box: cabinetBox,
      });
    }
  }
  return shapes;
});

interface NodeRender {
  node: TopologyCanvasNode;
  rect: CanvasRect;
  typeClass: string;
  name: string;
  typeLabel: string;
  detail: string;
  tooltip: string;
  iconMarkup: string;
}

const renderNodes = computed<NodeRender[]>(() =>
  props.nodes.map((node) => {
    const typeLabel = deviceTypeLabel(node.device_type);
    return {
      node,
      rect: effectiveRect(node),
      typeClass: `type-${node.device_type || "other"}`,
      name: node.device_name || "Unknown",
      typeLabel,
      detail: node.ip_address || node.room_name || "",
      tooltip: `${node.device_name || "Unknown"} (${typeLabel || node.device_type || ""})`,
      iconMarkup: iconMarkupOf(node.device_type),
    };
  }),
);

interface ConnectionLabelRender {
  x: number;
  y: number;
  text: string;
}

interface HopRender {
  x: number;
  y: number;
  shape: "circle" | "rect";
  label: string;
  index: number;
  tooltip: string;
}

interface LagBadgeRender {
  x: number;
  y: number;
  width: number;
  text: string;
  summary: string;
}

interface ConnectionRender {
  id: string;
  path: string;
  className: string;
  labels: ConnectionLabelRender[];
  hops: HopRender[];
  cableLabels: ConnectionLabelRender[];
  badge: LagBadgeRender | null;
}

/** 同设备同侧边缘的锚点占用计数：多条连线共用同一锚点会让端口标签重叠 */
function pickSlotAnchor(
  deviceId: string,
  source: CanvasRect,
  target: CanvasRect,
  slots: Map<string, number>,
): AnchorPoint {
  const base = pickAnchor(source, target, 0);
  const key = `${deviceId}:${base.dir}`;
  const slot = slots.get(key) ?? 0;
  slots.set(key, slot + 1);
  return slot === 0 ? base : pickAnchor(source, target, slot);
}

function connectionLabel(
  x: number,
  y: number,
  text: string,
  side: "source" | "target",
): ConnectionLabelRender {
  return { x, y: side === "source" ? y - 10 : y + 16, text };
}

/** 逻辑连线成员端口摘要（port_id 可能缺失，回退占位文案） */
function memberSummary(
  members: { port_number: string | null; port_id: string | null }[] | null | undefined,
): string | null {
  if (!members || members.length === 0) {
    return null;
  }
  const text = members
    .slice(0, 4)
    .map((member) => member.port_number || member.port_id?.slice(0, 8) || t("viz.no_port"))
    .join(", ");
  return members.length > 4 ? `${text}…` : text;
}

function buildPhysicalRender(
  connection: TopologyConnection,
  sourceAnchor: AnchorPoint,
  targetAnchor: AnchorPoint,
  parallelOffset: number,
): ConnectionRender {
  const hops = Array.isArray(connection.hops) ? connection.hops : [];
  const cables = Array.isArray(connection.cables) ? connection.cables : [];
  const labels: ConnectionLabelRender[] = [];
  const hopRenders: HopRender[] = [];
  const cableLabels: ConnectionLabelRender[] = [];
  let path: string;

  if (hops.length === 0) {
    path = computeOrthogonalPath(sourceAnchor, targetAnchor, parallelOffset);
  } else {
    const points: { x: number; y: number }[] = [sourceAnchor];
    const hopPoints = hops.map((_hop, index) => {
      const ratio = (index + 1) / (hops.length + 1);
      return {
        x: sourceAnchor.x + (targetAnchor.x - sourceAnchor.x) * ratio,
        y: sourceAnchor.y + (targetAnchor.y - sourceAnchor.y) * ratio + parallelOffset,
      };
    });
    points.push(...hopPoints, targetAnchor);
    path = points.map((point, index) => `${index === 0 ? "M" : "L"} ${point.x} ${point.y}`).join(" ");

    hops.forEach((hop, index) => {
      const point = hopPoints[index];
      if (!point) {
        return;
      }
      const isPatchPanel = hop.node_type === "patch_panel";
      const hopName = hop.node_label || hop.node_id || "";
      hopRenders.push({
        x: point.x,
        y: point.y,
        shape: isPatchPanel ? "rect" : "circle",
        label: hopName,
        index: index + 1,
        tooltip: `${isPatchPanel ? t("viz.patch_panel_node") : t("viz.net_outlet_node")} ${hopName} - ${t("viz.link_order")}: ${index + 1}/${hops.length}`,
      });
    });

    const pushCableLabel = (
      cable: { cable_label: string | null } | undefined,
      from: { x: number; y: number } | undefined,
      to: { x: number; y: number } | undefined,
    ): void => {
      if (!cable?.cable_label || !from || !to) {
        return;
      }
      cableLabels.push({
        x: (from.x + to.x) / 2,
        y: (from.y + to.y) / 2 - 6,
        text: cable.cable_label,
      });
    };
    hops.forEach((_hop, index) => {
      pushCableLabel(cables[index], points[index], points[index + 1]);
    });
    pushCableLabel(cables[hops.length], points[hops.length], points[hops.length + 1]);
  }

  if (connection.source_port_label) {
    labels.push(connectionLabel(sourceAnchor.x, sourceAnchor.y, connection.source_port_label, "source"));
  }
  if (connection.target_port_label) {
    labels.push(connectionLabel(targetAnchor.x, targetAnchor.y, connection.target_port_label, "target"));
  }

  return {
    id: connection.id,
    path,
    className: connection.derived ? "auto-discovered" : "manual",
    labels,
    hops: hopRenders,
    cableLabels,
    badge: null,
  };
}

function buildLogicalRender(
  connection: TopologyConnection,
  sourceAnchor: AnchorPoint,
  targetAnchor: AnchorPoint,
  parallelOffset: number,
): ConnectionRender {
  const midX = (sourceAnchor.x + targetAnchor.x) / 2;
  const midY = (sourceAnchor.y + targetAnchor.y) / 2 + parallelOffset;
  const badgeText = connection.label || t("viz.lag_badge");
  const labels: ConnectionLabelRender[] = [];
  const sourceMembers = memberSummary(connection.source_members);
  if (sourceMembers) {
    labels.push(connectionLabel(sourceAnchor.x, sourceAnchor.y, sourceMembers, "source"));
  }
  const targetMembers = memberSummary(connection.target_members);
  if (targetMembers) {
    labels.push(connectionLabel(targetAnchor.x, targetAnchor.y, targetMembers, "target"));
  }
  return {
    id: connection.id,
    path: computeOrthogonalPath(sourceAnchor, targetAnchor, parallelOffset),
    className: "logical",
    labels,
    hops: [],
    cableLabels: [],
    badge: {
      x: midX,
      y: midY,
      width: Math.max(badgeText.length * 8 + 20, 64),
      text: badgeText,
      summary: `${connection.source_members?.length ?? 0} + ${connection.target_members?.length ?? 0}`,
    },
  };
}

const connectionRenders = computed<ConnectionRender[]>(() => {
  const rectById = new Map(renderNodes.value.map((item) => [item.node.device_id, item.rect]));
  const pairCounts = new Map<string, number>();
  const anchorSlots = new Map<string, number>();
  const renders: ConnectionRender[] = [];

  for (const connection of props.connections) {
    const sourceRect = rectById.get(connection.source_device_id);
    const targetRect = rectById.get(connection.target_device_id);
    if (!sourceRect || !targetRect) {
      continue;
    }
    const sourceAnchor = pickSlotAnchor(connection.source_device_id, sourceRect, targetRect, anchorSlots);
    const targetAnchor = pickSlotAnchor(connection.target_device_id, targetRect, sourceRect, anchorSlots);

    const pairKey = [connection.source_device_id, connection.target_device_id].toSorted().join("|");
    const pairIndex = pairCounts.get(pairKey) ?? 0;
    pairCounts.set(pairKey, pairIndex + 1);
    const parallelOffset = pairIndex * 15;

    renders.push(
      connection.connection_type === "logical"
        ? buildLogicalRender(connection, sourceAnchor, targetAnchor, parallelOffset)
        : buildPhysicalRender(connection, sourceAnchor, targetAnchor, parallelOffset),
    );
  }
  return renders;
});

function fitView(): void {
  userAdjusted.value = false;
  if (props.nodes.length === 0) {
    setViewBox({ ...EMPTY_VIEW });
    return;
  }
  const shapes = containerShapes.value;
  const rects = shapes.length > 0
    ? shapes.map((shape) => ({
        x: shape.box.minX,
        y: shape.box.minY,
        width: shape.box.maxX - shape.box.minX,
        height: shape.box.maxY - shape.box.minY,
      }))
    : renderNodes.value.map((item) => item.rect);
  if (rects.length === 0) {
    return;
  }
  setViewBox(fitRects(rects, FIT_PADDING, EMPTY_VIEW));
}

watch(
  () => props.fitKey,
  () => fitView(),
);

watch(containerSize, () => {
  if (!userAdjusted.value) {
    fitView();
  }
});

watch(zoomScale, (scale) => emit("zoom-change", scale), { immediate: true });

// 指针交互

function closest<T extends Element>(target: EventTarget | null, selector: string): T | null {
  return target instanceof Element ? target.closest<T>(selector) : null;
}

function shapeOf(kind: ContainerKind, key: string): ContainerShape | undefined {
  return containerShapes.value.find((shape) => shape.kind === kind && shape.key === key);
}

function bindWindowListeners(): void {
  window.addEventListener("mousemove", onWindowMouseMove);
  window.addEventListener("mouseup", onWindowMouseUp);
}

function unbindWindowListeners(): void {
  window.removeEventListener("mousemove", onWindowMouseMove);
  window.removeEventListener("mouseup", onWindowMouseUp);
}

function startPan(event: MouseEvent): void {
  panning.value = true;
  userAdjusted.value = true;
  interaction.value = {
    kind: "pan",
    startClientX: event.clientX,
    startClientY: event.clientY,
    startViewBox: { ...viewBox.value },
  };
  bindWindowListeners();
}

function onMouseDown(event: MouseEvent): void {
  if (connectionMode.value) {
    const anchor = closest<SVGElement>(event.target, ".port-anchor");
    if (anchor) {
      event.preventDefault();
      handleAnchorClick(anchor);
    }
    return;
  }

  // 中键或空格 + 左键：任何位置都进入平移
  if (event.button === 1 || (event.button === 0 && spaceDown.value)) {
    event.preventDefault();
    startPan(event);
    return;
  }
  if (event.button !== 0) {
    return;
  }
  if (closest(event.target, ".port-anchor")) {
    return;
  }

  const containerGroup = closest<SVGGElement>(event.target, ".topology-container-group");
  if (containerGroup) {
    const kind = containerGroup.dataset.containerKind;
    const key = containerGroup.dataset.containerKey;
    if ((kind === "room" || kind === "cabinet") && key) {
      startContainerDrag(event, kind, key);
      return;
    }
  }

  const nodeGroup = closest<SVGGElement>(event.target, "[data-device-id]");
  const deviceId = nodeGroup?.dataset.deviceId;
  if (deviceId) {
    startNodeDrag(event, deviceId);
    return;
  }

  // 空白：清除连线选中，通知父组件，并开始平移
  selectedConnectionId.value = null;
  emit("canvas-click");
  startPan(event);
}

function startNodeDrag(event: MouseEvent, deviceId: string): void {
  const point = toCanvasPoint(event);
  const node = props.nodes.find((item) => item.device_id === deviceId);
  if (!point || !node) {
    return;
  }
  event.preventDefault();
  hideTooltip();
  selectedNodeId.value = deviceId;
  interaction.value = {
    kind: "node",
    deviceId,
    start: point,
    origin: { ...node.rect },
    rect: { ...node.rect },
    moved: false,
  };
  bindWindowListeners();
}

function startContainerDrag(event: MouseEvent, kind: ContainerKind, key: string): void {
  const point = toCanvasPoint(event);
  if (!point) {
    return;
  }
  const shape = shapeOf(kind, key);
  if (!shape) {
    return;
  }
  event.preventDefault();
  hideTooltip();
  // 推挤时被拖容器保持不动：房间取自身键，机柜取成员房间键；机柜级另固定机柜键
  const room = groups.value.find(
    (item) => item.key === key || item.cabinets.some((cabinet) => cabinet.key === key),
  );
  interaction.value = {
    kind: "container",
    containerKind: kind,
    key,
    start: point,
    offset: { x: 0, y: 0 },
    moved: false,
    fixedKey: kind === "room" ? key : (room?.key ?? key),
    fixedGroupKey: kind === "cabinet" ? key : null,
  };
  bindWindowListeners();
}

function onWindowMouseMove(event: MouseEvent): void {
  const state = interaction.value;
  if (!state) {
    return;
  }
  if (state.kind === "pan") {
    hideTooltip();
    const element = containerRef.value;
    const scale = element?.clientWidth ? state.startViewBox.width / element.clientWidth : 1;
    const dx = (event.clientX - state.startClientX) * scale;
    const dy = (event.clientY - state.startClientY) * scale;
    setViewBox({
      ...state.startViewBox,
      x: state.startViewBox.x - dx,
      y: state.startViewBox.y - dy,
    });
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
  hideTooltip();

  if (state.kind === "node") {
    // 坐标钳制为非负：与坐标输入框「拒绝负值」口径一致，防止负坐标入库
    state.rect = {
      ...state.origin,
      x: clampNonNegative(state.origin.x + dx),
      y: clampNonNegative(state.origin.y + dy),
    };
  } else {
    state.offset = { x: dx, y: dy };
  }
}

function onWindowMouseUp(): void {
  unbindWindowListeners();
  panning.value = false;
  const state = interaction.value;
  interaction.value = null;
  if (!state) {
    return;
  }

  if (state.kind === "pan") {
    return;
  }

  if (state.kind === "node") {
    if (!state.moved) {
      emit("node-click", state.deviceId);
      return;
    }
    emit("node-drag-end", state.deviceId, {
      ...state.rect,
      x: clampNonNegative(Math.round(snapToGrid(state.rect.x))),
      y: clampNonNegative(Math.round(snapToGrid(state.rect.y))),
    });
    return;
  }

  if (!state.moved) {
    const shape = shapeOf(state.containerKind, state.key);
    if (shape) {
      emit("container-click", state.containerKind, state.key, {
        x: shape.box.minX,
        y: shape.box.minY,
        width: shape.box.maxX - shape.box.minX,
        height: shape.box.maxY - shape.box.minY,
      });
    }
    return;
  }
  emit("container-drag-end", state.fixedKey, state.fixedGroupKey, { ...state.offset });
}

function onCanvasMouseMove(event: MouseEvent): void {
  if (interaction.value) {
    hideTooltip();
    return;
  }
  if (connectionSource.value) {
    const point = toCanvasPoint(event);
    if (point) {
      tempPath.value = computeTempConnectionPath(connectionSource.value, point);
    }
    hideTooltip();
    return;
  }
  const target = closest<SVGElement>(event.target, "[data-tooltip]");
  const text = target?.getAttribute("data-tooltip") ?? "";
  if (text) {
    showTooltip(event, text);
  } else {
    hideTooltip();
  }
}

function onCanvasClick(event: MouseEvent): void {
  const connectionGroup = closest<SVGGElement>(event.target, ".topology-connection-group");
  const connectionId = connectionGroup?.dataset.connectionId;
  if (connectionId) {
    selectedConnectionId.value = connectionId;
    emit("connection-click", connectionId);
    return;
  }
  if (!closest(event.target, "[data-device-id]") && !closest(event.target, ".topology-container-group")) {
    selectedConnectionId.value = null;
  }
}

function onWheel(event: WheelEvent): void {
  event.preventDefault();
  hideTooltip();
  const point = toCanvasPoint(event);
  if (!point) {
    return;
  }
  userAdjusted.value = true;
  zoomAt(point, event.deltaY > 0 ? 1.1 : 0.9);
}

// 连线模式

function cancelConnection(): void {
  connectionSource.value = null;
  tempPath.value = "";
}

function setConnectionMode(enabled: boolean): void {
  connectionMode.value = enabled;
  if (!enabled) {
    cancelConnection();
  }
}

function anchorPosition(rect: CanvasRect, direction: AnchorDirection): { x: number; y: number } {
  return nodeEdgeAnchor({ ...rect, x: 0, y: 0 }, direction);
}

function handleAnchorClick(anchor: SVGElement): void {
  const nodeGroup = closest<SVGGElement>(anchor, "[data-device-id]");
  const deviceId = nodeGroup?.dataset.deviceId;
  const portDir = anchor.getAttribute("data-port-dir") as AnchorDirection | null;
  if (!deviceId || !portDir) {
    return;
  }
  const node = props.nodes.find((item) => item.device_id === deviceId);
  if (!node) {
    return;
  }
  const rect = effectiveRect(node);
  const point = anchorPosition(rect, portDir);
  const source = connectionSource.value;

  if (!source) {
    connectionSource.value = { deviceId, portDir, x: point.x, y: point.y };
    return;
  }
  if (source.deviceId === deviceId && source.portDir === portDir) {
    cancelConnection();
    return;
  }
  // 同一设备两个端口互连属自环：与模态框创建入口校验一致，拒绝并提示
  if (source.deviceId === deviceId) {
    showToast(t("viz.no_self_connection"), "warning");
    cancelConnection();
    return;
  }
  emit("connection-complete", source.deviceId, null, deviceId, null);
  cancelConnection();
}

// 键盘（空格平移 / Esc 取消连线）

function isCanvasVisible(): boolean {
  const element = containerRef.value;
  return Boolean(element?.isConnected && element.getBoundingClientRect().width > 0);
}

function onKeyDown(event: KeyboardEvent): void {
  if (!isCanvasVisible()) {
    return;
  }
  const target = event.target;
  if (target instanceof Element && target.closest("input, select, textarea, button, [contenteditable]")) {
    return;
  }
  if (event.code === "Space") {
    event.preventDefault();
    spaceDown.value = true;
    return;
  }
  if (event.code === "Escape") {
    cancelConnection();
  }
}

function onKeyUp(event: KeyboardEvent): void {
  if (event.code === "Space") {
    spaceDown.value = false;
  }
}

onMounted(() => {
  syncContainerSize();
  fitView();
  document.addEventListener("keydown", onKeyDown);
  document.addEventListener("keyup", onKeyUp);
});

onBeforeUnmount(() => {
  unbindWindowListeners();
  document.removeEventListener("keydown", onKeyDown);
  document.removeEventListener("keyup", onKeyUp);
});

defineExpose({ setZoom, zoomBy, fitView, setConnectionMode });
</script>

<template>
  <div
    ref="containerRef"
    class="visualization-canvas visualization-canvas-fixed"
    :class="{ panning: panning || spaceDown, 'connection-mode': connectionMode }"
    role="img"
    :aria-label="t('visualization.device')"
    @mouseenter="syncContainerSize"
  >
    <svg
      ref="svgRef"
      class="visualization-svg"
      :viewBox="`${viewBox.x} ${viewBox.y} ${viewBox.width} ${viewBox.height}`"
      preserveAspectRatio="xMinYMin meet"
      @mousedown="onMouseDown"
      @mousemove="onCanvasMouseMove"
      @mouseleave="hideTooltip()"
      @click="onCanvasClick"
      @wheel="onWheel"
    >
      <CanvasGridLayer
        pattern-id="grid-topology"
        :grid-size="gridSize"
        :cover-rect="coverRect"
        :ruler="ruler"
      />

      <!-- 区域容器（房间 → 机柜）：位于连线与节点之下 -->
      <g class="containers-group">
        <g
          v-for="shape in containerShapes"
          :key="`${shape.kind}:${shape.key}`"
          class="topology-container-group"
          :class="`container-${shape.kind}`"
          :data-container-kind="shape.kind"
          :data-container-key="shape.key"
        >
          <rect
            class="topology-container"
            :class="`container-${shape.kind}`"
            :x="shape.box.minX"
            :y="shape.box.minY"
            :width="shape.box.maxX - shape.box.minX"
            :height="shape.box.maxY - shape.box.minY"
            :rx="shape.kind === 'room' ? 10 : 8"
          />
          <text
            class="topology-container-label"
            :x="shape.box.minX + 14"
            :y="shape.box.minY + 20"
            dominant-baseline="middle"
          >{{ shape.label }}</text>
          <text
            class="topology-container-count"
            :x="shape.box.minX + 14"
            :y="shape.box.minY + 38"
            dominant-baseline="middle"
          >{{
            shape.sublabel
              ? `${shape.sublabel} · ${shape.count} ${t("viz.device_unit")}`
              : `${shape.count} ${t("viz.device_unit")}`
          }}</text>
        </g>
      </g>

      <g class="connections-group">
        <g
          v-for="item in connectionRenders"
          :key="item.id"
          class="topology-connection-group"
          :data-connection-id="item.id"
        >
          <path
            class="topology-connection"
            :class="[item.className, { selected: selectedConnectionId === item.id }]"
            :d="item.path"
          />

          <g
            v-for="(hop, index) in item.hops"
            :key="`hop-${index}`"
            class="topology-hop-node"
            :class="hop.shape === 'rect' ? 'hop-patch-panel' : 'hop-net-outlet'"
            :data-tooltip="hop.tooltip"
          >
            <rect
              v-if="hop.shape === 'rect'"
              :x="hop.x - 9"
              :y="hop.y - 9"
              width="18"
              height="18"
              rx="3"
            />
            <circle v-else :cx="hop.x" :cy="hop.y" r="9" />
            <text
              class="topology-outlet-label"
              :x="hop.x"
              :y="hop.y - 15"
              text-anchor="middle"
              dominant-baseline="middle"
            >{{ hop.label }}</text>
            <text
              class="topology-outlet-index"
              :x="hop.x"
              :y="hop.y + 3.5"
              text-anchor="middle"
              dominant-baseline="middle"
            >{{ hop.index }}</text>
          </g>

          <text
            v-for="(label, index) in item.cableLabels"
            :key="`cable-${index}`"
            class="topology-cable-label"
            :x="label.x"
            :y="label.y"
            text-anchor="middle"
            dominant-baseline="middle"
          >{{ label.text }}</text>

          <template v-if="item.badge">
            <g class="topology-lag-badge">
              <rect
                :x="item.badge.x - item.badge.width / 2"
                :y="item.badge.y - 12"
                :width="item.badge.width"
                height="24"
                rx="12"
              />
              <text
                :x="item.badge.x"
                :y="item.badge.y + 4"
                text-anchor="middle"
                dominant-baseline="middle"
              >{{ item.badge.text }}</text>
            </g>
            <text
              class="topology-lag-members"
              :x="item.badge.x"
              :y="item.badge.y + 26"
              text-anchor="middle"
            >{{ t("viz.member_ports") }}: {{ item.badge.summary }}</text>
          </template>

          <text
            v-for="(label, index) in item.labels"
            :key="`label-${index}`"
            class="topology-connection-label"
            :x="label.x"
            :y="label.y"
            text-anchor="middle"
            dominant-baseline="middle"
          >{{ label.text }}</text>
        </g>
      </g>

      <g class="elements-group">
        <g
          v-for="item in renderNodes"
          :key="item.node.device_id"
          class="topology-device-node"
          :class="[item.typeClass, { selected: selectedNodeId === item.node.device_id }]"
          :transform="`translate(${item.rect.x}, ${item.rect.y})`"
          :data-device-id="item.node.device_id"
          :data-tooltip="item.tooltip"
        >
          <rect :width="item.rect.width" :height="item.rect.height" />
          <g class="device-icon" :transform="`translate(14, ${item.rect.height / 2 - ICON_SIZE / 2})`">
            <svg
              class="device-type-icon"
              viewBox="0 0 24 24"
              :width="ICON_SIZE"
              :height="ICON_SIZE"
              aria-hidden="true"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
              v-html="item.iconMarkup"
            ></svg>
          </g>
          <text
            class="device-name"
            :x="item.rect.width / 2 + TEXT_OFFSET_X"
            y="22"
            text-anchor="middle"
            dominant-baseline="middle"
          >{{ item.name }}</text>
          <text
            class="device-type-label"
            :x="item.rect.width / 2 + TEXT_OFFSET_X"
            y="40"
            text-anchor="middle"
            dominant-baseline="middle"
          >{{ item.typeLabel }}</text>
          <text
            class="device-ip"
            :x="item.rect.width / 2 + TEXT_OFFSET_X"
            y="58"
            text-anchor="middle"
            dominant-baseline="middle"
          >{{ item.detail }}</text>

          <circle
            v-for="direction in ANCHOR_DIRECTIONS"
            :key="direction"
            class="port-anchor"
            :class="{
              'connection-source':
                connectionSource?.deviceId === item.node.device_id &&
                connectionSource?.portDir === direction,
            }"
            :data-port-dir="direction"
            r="5"
            :cx="anchorPosition(item.rect, direction).x"
            :cy="anchorPosition(item.rect, direction).y"
          />
        </g>
      </g>

      <!-- 连线模式的临时连线（跟随鼠标的贝塞尔示意） -->
      <g class="temp-connection-group">
        <path v-if="tempPath" class="temp-connection" :d="tempPath" />
      </g>
    </svg>
  </div>

  <Teleport to="body">
    <div ref="tooltipRef" class="tooltip" :class="{ visible: tooltipVisible }" :style="tooltipStyle">
      {{ tooltipText }}
    </div>
  </Teleport>
</template>
