/**
 * 可视化画布的纯几何/布局算法（无 DOM 依赖，可直接单测）。
 *
 * 旧实现把这些计算散落在 SVGDataManager / SVGCore / TopologyVisualization
 * 里，并与 DOM 读写交织（坐标写在 SVG 属性上、保存时再反读）。本模块只做
 * 「输入矩形 → 输出矩形/路径」的纯函数，坐标的唯一真相交给响应式视图模型。
 *
 * 自检脚本：在 frontend/ 下执行 `npm run selfcheck:viz`
 * （Node ≥ 22.6 原生擦除类型，无需额外依赖）。
 */
import type {
  AnchorDirection,
  AnchorPoint,
  CanvasLayoutItem,
  CanvasRect,
  LayoutEntity,
  TopologyCabinetGroup,
  TopologyRoomGroup,
} from "../../types/visualization";

// ==================== 通用尺寸常量 ====================

/** 网格步长（像素）：拖拽吸附与网格 pattern 的基本单位 */
export const GRID_SIZE = 20;
/** 主网格线间距倍数：每 5 个网格画一条线并标注坐标 */
export const MAJOR_GRID_FACTOR = 5;
/** 主网格线条数上限：异常视野（1e8 级）下按 step 逐条建线会冻结页面 */
export const MAX_GRID_LINES = 500;
/** 画布四周内容留白（工位/机柜视图） */
export const CANVAS_CONTENT_PADDING = 50;
/** 画布最小视野（内容较少时不留空白带） */
export const MIN_VIEW_WIDTH = 1000;
export const MIN_VIEW_HEIGHT = 800;

// ==================== 工位布局常量 ====================

/** 工位卡片默认尺寸 */
export const WORKSTATION_WIDTH = 160;
export const WORKSTATION_HEIGHT = 160;
/** 工位自动排布间距与起点 */
export const WORKSTATION_GAP = 20;
export const WORKSTATION_START_X = 150;
export const WORKSTATION_START_Y = 100;
/** 无已保存布局时按 4 列回退 */
export const WORKSTATION_FALLBACK_COLS = 4;

/** 门元素默认矩形（房间入口示意） */
export const DOOR_RECT: CanvasRect = { x: 50, y: 100, width: 40, height: 80 };

// ==================== 机柜布局常量 ====================

/** 画布四周预留边距：首柜距左缘 / 柜底距底缘（像素，viewBox 1:1） */
export const CABINET_EDGE_PADDING = 16;
/** 相邻机柜水平间距 */
export const CABINET_GAP = 50;
/** 机柜宽度 */
export const CABINET_WIDTH = 150;
/** 每渲染完一批机柜统一拉取该批机位 IP 信息 */
export const CABINET_BATCH_SIZE = 16;
/** 机柜头部高度（名称区）与机位左右内缩 */
export const CABINET_HEADER_HEIGHT = 40;
export const CABINET_POSITION_INSET_X = 20;
export const CABINET_POSITION_WIDTH = 110;

/**
 * 机柜容量缺省值：与后端建表默认（foims-init cabinets DDL `capacity DEFAULT 42`）一致。
 */
export const DEFAULT_CABINET_CAPACITY = 42;

// ==================== 拓扑分组常量 ====================

/** 房间容器留白：四周内边距，顶部另加标题区高度 */
export const ROOM_PADDING = 40;
export const ROOM_HEADER = 52;
/** 机柜容器留白 */
export const CABINET_PADDING = 24;
export const CABINET_HEADER = 36;
/** 房间/机柜容器之间的最小间距 */
export const ROOM_GAP = 80;
export const CABINET_GROUP_GAP = 40;

/** 拓扑自动布局的网格参数 */
export const TOPOLOGY_NODE_WIDTH = 200;
export const TOPOLOGY_NODE_HEIGHT = 100;
export const TOPOLOGY_GRID_X = 40;
export const TOPOLOGY_GRID_Y = 40;
export const TOPOLOGY_CABINET_COLS = 3;
export const TOPOLOGY_LOOSE_COLS = 4;
export const TOPOLOGY_CABINET_GAP = 60;
export const TOPOLOGY_ROOM_COLS = 3;
export const TOPOLOGY_ROOM_GAP_Y = 100;
/** 自动布局起点 */
export const TOPOLOGY_LAYOUT_ORIGIN = 100;

/** 拓扑推挤的最大迭代轮数（两两推开后的收敛上限） */
export const GROUP_SEPARATION_ITERATIONS = 40;
/** 同侧边缘上相邻锚点的错开间距 */
export const ANCHOR_SPREAD = 18;

// ==================== 基础校验 ====================

/**
 * 布局坐标合法性校验：x/y 有限且非负、宽高有限且为正。
 * 异常坐标（NaN/负值/零宽高）按"无有效布局"处理，避免进入 viewBox。
 */
export function isValidRect(rect: CanvasRect | null | undefined): rect is CanvasRect {
  if (!rect) {
    return false;
  }
  return (
    Number.isFinite(rect.x) &&
    Number.isFinite(rect.y) &&
    Number.isFinite(rect.width) &&
    Number.isFinite(rect.height) &&
    rect.x >= 0 &&
    rect.y >= 0 &&
    rect.width > 0 &&
    rect.height > 0
  );
}

/** 吸附到网格：四舍五入到最近网格点 */
export function snapToGrid(value: number, gridSize: number = GRID_SIZE): number {
  return Math.round(value / gridSize) * gridSize;
}

/** 坐标钳制为非负（拖出画布左上时不产生负坐标） */
export function clampNonNegative(value: number): number {
  return value < 0 ? 0 : value;
}

/** 生成 DOM 标识：HTTP 内网部署下 crypto.randomUUID 不存在，回退 RFC 4122 v4 */
export function generateDomId(): string {
  if (typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  const sixth = bytes[6] ?? 0;
  const eighth = bytes[8] ?? 0;
  bytes[6] = (sixth & 0x0f) | 0x40; // version 4
  bytes[8] = (eighth & 0x3f) | 0x80; // variant 10
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
  return [
    hex.slice(0, 8),
    hex.slice(8, 12),
    hex.slice(12, 16),
    hex.slice(16, 20),
    hex.slice(20),
  ].join("-");
}

// ==================== 视图覆盖矩形与网格标尺 ====================

/**
 * 反算与可视区等大的世界坐标矩形（viewBox ∪ letterbox 区）。
 *
 * meet 缩放下 viewBox 宽高比与容器不一致时，viewBox 只映射到画布的一个
 * 子矩形，letterbox 区会露出无网格白底；按 preserveAspectRatio 锚定方向
 * 把缺失的宽/高补回到世界坐标，网格与标尺即可铺满任意屏幕。
 */
export function computeCoverRect(
  viewBox: CanvasRect,
  container: { width: number; height: number },
  preserveAspectRatio: string,
): CanvasRect {
  const cover: CanvasRect = { ...viewBox };
  if (!viewBox.width || !viewBox.height || !container.width || !container.height) {
    return cover;
  }
  const scale = Math.min(container.width / viewBox.width, container.height / viewBox.height);
  if (!Number.isFinite(scale) || scale <= 0) {
    return cover;
  }
  const extraWidth = container.width / scale - viewBox.width;
  const extraHeight = container.height / scale - viewBox.height;
  const match = /^x(Min|Mid|Max)Y(Min|Mid|Max)/.exec(preserveAspectRatio);
  const alignX = match?.[1] ?? "Mid";
  const alignY = match?.[2] ?? "Mid";

  const anchorOffset = (align: string, extra: number): number => {
    if (align === "Min") {
      return 0;
    }
    return align === "Max" ? extra : extra / 2;
  };

  cover.x -= anchorOffset(alignX, extraWidth);
  cover.y -= anchorOffset(alignY, extraHeight);
  cover.width += extraWidth;
  cover.height += extraHeight;
  return cover;
}

/** 一条网格线（世界坐标） */
export interface GridLine {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/** 一个坐标标注 */
export interface GridLabel {
  x: number;
  y: number;
  text: string;
  fontSize: number;
}

/** 主网格与坐标标注的绘制数据 */
export interface GridRulerData {
  lines: GridLine[];
  labels: GridLabel[];
}

/**
 * 按覆盖矩形生成主网格线与坐标标注。
 *
 * 异常视野防护：节点坐标异常（超大/非有限值）经 fit 后 viewBox 可达 1e8 级，
 * 按 step 逐条建线会创建百万级 SVG 节点直接冻结页面。非有限值返回 null，
 * 条数封顶保证渲染有界（合法最大视野约 300 条，封顶不影响正常使用）。
 * 按索引循环而非累加：`gx += step` 在浮点精度停滞时（大于 2^53 后加不变）会死循环。
 */
export function buildGridRuler(
  cover: CanvasRect,
  options: { gridSize?: number; fontDivisor?: number; maxLines?: number } = {},
): GridRulerData | null {
  const gridSize = options.gridSize ?? GRID_SIZE;
  const fontDivisor = options.fontDivisor ?? 100;
  const maxLines = options.maxLines ?? MAX_GRID_LINES;
  if (!cover.width || !cover.height) {
    return null;
  }
  if (
    !Number.isFinite(cover.x) ||
    !Number.isFinite(cover.y) ||
    !Number.isFinite(cover.width) ||
    !Number.isFinite(cover.height)
  ) {
    return null;
  }

  const step = gridSize * MAJOR_GRID_FACTOR;
  // 标注字号随可视区世界宽度缩放，缩放后保持可读
  const fontSize = Math.max(9, Math.min(14, cover.width / fontDivisor));
  const endX = cover.x + cover.width;
  const endY = cover.y + cover.height;
  const lines: GridLine[] = [];
  const labels: GridLabel[] = [];

  const startX = Math.floor(cover.x / step) * step;
  const countX = Math.min(Math.ceil((endX - startX) / step), maxLines);
  for (let i = 0; i < countX; i += 1) {
    const gx = startX + i * step;
    lines.push({ x1: gx, y1: cover.y, x2: gx, y2: endY });
    labels.push({ x: gx + 2, y: cover.y + fontSize, text: String(gx), fontSize });
  }

  const startY = Math.floor(cover.y / step) * step;
  const countY = Math.min(Math.ceil((endY - startY) / step), maxLines);
  for (let j = 0; j < countY; j += 1) {
    const gy = startY + j * step;
    lines.push({ x1: cover.x, y1: gy, x2: endX, y2: gy });
    labels.push({ x: cover.x + 2, y: gy - 2, text: String(gy), fontSize });
  }

  return { lines, labels };
}

// ==================== 对齐线 ====================

/** 一条对齐参考线 */
export interface AlignmentSegment {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/**
 * 计算拖动中元素与其他元素的中心/边线对齐参考线。
 * 阈值内（默认 10px）命中即生成一条贯穿两个矩形的虚线。
 */
export function computeAlignmentSegments(
  moving: CanvasRect,
  others: CanvasRect[],
  threshold = 10,
): AlignmentSegment[] {
  const segments: AlignmentSegment[] = [];
  const centerX = moving.x + moving.width / 2;
  const centerY = moving.y + moving.height / 2;
  const right = moving.x + moving.width;
  const bottom = moving.y + moving.height;

  for (const other of others) {
    const otherCenterX = other.x + other.width / 2;
    const otherCenterY = other.y + other.height / 2;
    const otherRight = other.x + other.width;
    const otherBottom = other.y + other.height;

    if (Math.abs(centerX - otherCenterX) < threshold) {
      segments.push({
        x1: otherCenterX,
        y1: Math.min(moving.y, other.y),
        x2: otherCenterX,
        y2: Math.max(bottom, otherBottom),
      });
    }
    if (Math.abs(centerY - otherCenterY) < threshold) {
      segments.push({
        x1: Math.min(moving.x, other.x),
        y1: otherCenterY,
        x2: Math.max(right, otherRight),
        y2: otherCenterY,
      });
    }
    if (Math.abs(moving.x - other.x) < threshold) {
      segments.push({
        x1: other.x,
        y1: Math.min(moving.y, other.y),
        x2: other.x,
        y2: Math.max(bottom, otherBottom),
      });
    }
    if (Math.abs(moving.y - other.y) < threshold) {
      segments.push({
        x1: Math.min(moving.x, other.x),
        y1: other.y,
        x2: Math.max(right, otherRight),
        y2: other.y,
      });
    }
  }
  return segments;
}

// ==================== 工位布局 ====================

/** 自动排布的列数：按工位数量选择，保持画布接近方形 */
export function workstationColumns(count: number): number {
  if (count <= 5) {
    return 3;
  }
  if (count <= 12) {
    return 4;
  }
  if (count <= 24) {
    return 6;
  }
  return Math.min(8, Math.max(3, Math.ceil(Math.sqrt(count))));
}

/** 工位自动排布：返回每个索引对应的矩形（不接受已保存坐标） */
export function arrangeWorkstationRects(count: number): CanvasRect[] {
  const cols = workstationColumns(count);
  return Array.from({ length: count }, (_unused, index) => {
    const col = index % cols;
    const row = Math.floor(index / cols);
    return {
      x: WORKSTATION_START_X + col * (WORKSTATION_WIDTH + WORKSTATION_GAP),
      y: WORKSTATION_START_Y + row * (WORKSTATION_HEIGHT + WORKSTATION_GAP),
      width: WORKSTATION_WIDTH,
      height: WORKSTATION_HEIGHT,
    };
  });
}

/** 无已保存坐标时的回退排布（4 列，工位加载用） */
export function fallbackWorkstationRect(index: number): CanvasRect {
  const col = index % WORKSTATION_FALLBACK_COLS;
  const row = Math.floor(index / WORKSTATION_FALLBACK_COLS);
  return {
    x: WORKSTATION_START_X + col * (WORKSTATION_WIDTH + WORKSTATION_GAP),
    y: WORKSTATION_START_Y + row * (WORKSTATION_HEIGHT + WORKSTATION_GAP),
    width: WORKSTATION_WIDTH,
    height: WORKSTATION_HEIGHT,
  };
}

/** 自动排布后的画布视野（覆盖内容与容器，最小 1000×800） */
export function workstationCanvasRect(
  count: number,
  door: CanvasRect,
  container: { width: number; height: number },
): CanvasRect {
  const cols = workstationColumns(count);
  const rows = Math.ceil(count / cols);
  // 与 arrangeWorkstationRects 同一公式：末列/末行的右/下缘 + 留白
  const gridWidth =
    WORKSTATION_START_X + cols * (WORKSTATION_WIDTH + WORKSTATION_GAP) + CANVAS_CONTENT_PADDING;
  const gridHeight =
    WORKSTATION_START_Y + rows * (WORKSTATION_HEIGHT + WORKSTATION_GAP) + CANVAS_CONTENT_PADDING;

  return {
    x: 0,
    y: 0,
    width: Math.max(MIN_VIEW_WIDTH, gridWidth, door.x + door.width, container.width),
    height: Math.max(MIN_VIEW_HEIGHT, gridHeight, door.y + door.height, container.height),
  };
}

// ==================== 机柜布局 ====================

/** 机柜布局输入：容量与已保存坐标（缺失时按默认排布） */
export interface CabinetLayoutInput {
  id: string;
  capacity: number | null;
  savedX: number | null;
  savedWidth: number | null;
}

/** 单个机柜的布局结果 */
export interface CabinetLayoutEntry {
  id: string;
  rect: CanvasRect;
  capacity: number;
}

/** 机柜布局结果：全房间共享的 U 高与内容总宽 */
export interface CabinetLayoutResult {
  entries: CabinetLayoutEntry[];
  uHeight: number;
  totalWidth: number;
  containerHeight: number;
}

/**
 * 计算全部机柜位置并定型画布参数。
 *
 * - 单 U 高按容器高整除最大容量（柜底贴底，无小数像素）；容器高不足时
 *   钳制为 1，避免出现 0 或负的 U 高导致元素不可见；
 * - 保存的 x 坐标仅保留相对排列，整体平移使最左机柜距左缘固定边距；
 * - 保存宽度需为正的有限值，否则回退默认宽度。
 */
export function computeCabinetLayout(
  inputs: CabinetLayoutInput[],
  containerHeight: number,
): CabinetLayoutResult {
  const padding = CABINET_EDGE_PADDING;
  const usableHeight = containerHeight - padding * 2 - CABINET_HEADER_HEIGHT;
  const maxCapacity = inputs.reduce(
    (acc, input) => Math.max(acc, input.capacity ?? DEFAULT_CABINET_CAPACITY),
    DEFAULT_CABINET_CAPACITY,
  );
  const uHeight = Math.max(1, Math.floor(usableHeight / maxCapacity));

  let minX = Number.POSITIVE_INFINITY;
  let maxX = 0;
  const drafts = inputs.map((input, index) => {
    const capacity = input.capacity ?? DEFAULT_CABINET_CAPACITY;
    const height = capacity * uHeight + CABINET_HEADER_HEIGHT;
    const x =
      input.savedX !== null && Number.isFinite(input.savedX)
        ? input.savedX
        : padding + index * (CABINET_WIDTH + CABINET_GAP);
    const width =
      input.savedWidth !== null && Number.isFinite(input.savedWidth) && input.savedWidth > 0
        ? input.savedWidth
        : CABINET_WIDTH;
    minX = Math.min(minX, x);
    maxX = Math.max(maxX, x + width);
    return {
      id: input.id,
      capacity,
      rect: { x, y: containerHeight - padding - height, width, height },
    };
  });

  // 整体平移：最左机柜距左缘固定 CABINET_EDGE_PADDING
  const offsetX = (Number.isFinite(minX) ? minX : 0) - padding;
  const entries: CabinetLayoutEntry[] = [];
  for (const draft of drafts) {
    entries.push({ ...draft, rect: { ...draft.rect, x: draft.rect.x - offsetX } });
  }
  const totalWidth = maxX - offsetX + padding;

  return { entries, uHeight, totalWidth, containerHeight };
}

/** 机柜内单个机位（U 位）的画布矩形 */
export function computeCabinetPositionRect(
  cabinet: CanvasRect,
  capacity: number,
  uHeight: number,
  startU: number,
  endU: number,
): CanvasRect {
  const fromU = startU > 0 ? startU : 1;
  const toU = endU >= fromU ? endU : fromU;
  const height = Math.max((toU - fromU + 1) * uHeight, uHeight);
  return {
    x: cabinet.x + CABINET_POSITION_INSET_X,
    y: cabinet.y + CABINET_HEADER_HEIGHT + (capacity - toU) * uHeight,
    width: CABINET_POSITION_WIDTH,
    height,
  };
}

/** 机柜 U 位刻度线（相对机柜左上角的坐标） */
export interface UMarkLine {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
}

/** 机柜 U 位编号标注（相对机柜左上角的坐标） */
export interface UMarkLabel {
  x: number;
  y: number;
  text: string;
}

/** U 位刻度数据：线全量、编号按 5 的倍数与首末 U 递减 */
export interface UMarkData {
  lines: UMarkLine[];
  labels: UMarkLabel[];
}

/** 生成机柜 U 位刻度（相对坐标，机柜整体拖拽只需改 g 的 transform） */
export function buildUMarks(capacity: number, uHeight: number, width: number): UMarkData {
  const lines: UMarkLine[] = [];
  const labels: UMarkLabel[] = [];
  const startY = CABINET_HEADER_HEIGHT;

  for (let u = 1; u <= capacity; u += 1) {
    const y = startY + (capacity - u) * uHeight;
    lines.push({ x1: 10, y1: y, x2: width - 10, y2: y });
    if (u % 5 === 0 || u === 1 || u === capacity) {
      labels.push({ x: 5, y: y + uHeight / 2, text: String(u) });
    }
  }
  return { lines, labels };
}

// ==================== 布局持久化投影 ====================

/**
 * 由视图模型投影出布局保存项（替代旧实现从 SVG DOM 反读属性）。
 *
 * 校验与手工录入一致（min=0）：非有限值不入库，负坐标钳制为 0，
 * 宽高必须为正数（0/负值元素无法点击与拖拽，入库即坏数据）。
 */
export function buildLayoutItems(entities: LayoutEntity[]): CanvasLayoutItem[] {
  const items: CanvasLayoutItem[] = [];
  for (const entity of entities) {
    const { rect } = entity;
    if (!Number.isFinite(rect.x) || !Number.isFinite(rect.y)) {
      continue;
    }
    if (!Number.isFinite(rect.width) || !Number.isFinite(rect.height)) {
      continue;
    }
    if (rect.width <= 0 || rect.height <= 0) {
      continue;
    }
    items.push({
      id: entity.id,
      element_type: entity.elementType,
      position: {
        x: Math.max(0, rect.x),
        y: Math.max(0, rect.y),
        width: rect.width,
        height: rect.height,
        rotation: 0,
      },
    });
  }
  return items;
}

// ==================== 拓扑：分组包围盒 ====================

/** 分组包围盒（含容器留白） */
export interface GroupBox {
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
}

/** 分组节点最小形状（只需矩形） */
export interface RectNode {
  rect: CanvasRect;
}

/** 分组包围盒：空分组返回 null */
export function computeGroupBox(
  nodes: RectNode[],
  padding: number,
  header: number,
): GroupBox | null {
  if (nodes.length === 0) {
    return null;
  }
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const node of nodes) {
    minX = Math.min(minX, node.rect.x);
    minY = Math.min(minY, node.rect.y);
    maxX = Math.max(maxX, node.rect.x + node.rect.width);
    maxY = Math.max(maxY, node.rect.y + node.rect.height);
  }
  return {
    minX: minX - padding,
    minY: minY - padding - header,
    maxX: maxX + padding,
    maxY: maxY + padding,
  };
}

/** 推挤位移分配：自身固定不动；对方固定时承担全部推量；双方可动时对半分摊 */
function separateShift(selfFixed: boolean, otherFixed: boolean, push: number): number {
  if (selfFixed) {
    return 0;
  }
  return otherFixed ? push : push / 2;
}

/** 参与推挤的分组 */
export interface SeparationGroup<N extends RectNode> {
  key: string;
  nodes: N[];
  padding: number;
  header: number;
}

interface SeparationEntry<N extends RectNode> extends SeparationGroup<N> {
  box: GroupBox;
}

/**
 * 分组两两推挤：包围盒相交时沿穿透量较小的轴整体平移分组内全部节点。
 * `fixedKey` 指定的分组保持不动。返回是否有分组发生移动。
 *
 * 迭代上限 40 轮：三个及以上分组连锁推开时单轮可能又产生新的相交，
 * 上限保证最坏情况有界（每轮都把重叠量向 0 收敛）。
 */
export function separateGroupBoxes<N extends RectNode>(
  groups: SeparationGroup<N>[],
  gap: number,
  fixedKey: string | null = null,
): boolean {
  const entries: SeparationEntry<N>[] = [];
  for (const group of groups) {
    const box = computeGroupBox(group.nodes, group.padding, group.header);
    if (box) {
      entries.push({ ...group, box });
    }
  }
  if (entries.length < 2) {
    return false;
  }

  let moved = false;

  const shiftGroup = (entry: SeparationEntry<N>, dx: number, dy: number): void => {
    if (dx === 0 && dy === 0) {
      return;
    }
    for (const node of entry.nodes) {
      node.rect.x = Math.round(node.rect.x + dx);
      node.rect.y = Math.round(node.rect.y + dy);
    }
    entry.box.minX += dx;
    entry.box.maxX += dx;
    entry.box.minY += dy;
    entry.box.maxY += dy;
    moved = true;
  };

  for (let iter = 0; iter < GROUP_SEPARATION_ITERATIONS; iter += 1) {
    let adjusted = false;
    for (let i = 0; i < entries.length; i += 1) {
      const a = entries[i];
      if (!a) {
        continue;
      }
      for (let j = i + 1; j < entries.length; j += 1) {
        const b = entries[j];
        if (!b) {
          continue;
        }
        const overlapX =
          Math.min(a.box.maxX, b.box.maxX) - Math.max(a.box.minX, b.box.minX);
        const overlapY =
          Math.min(a.box.maxY, b.box.maxY) - Math.max(a.box.minY, b.box.minY);
        if (overlapX <= 0 || overlapY <= 0) {
          continue;
        }
        adjusted = true;
        separatePair(a, b, overlapX, overlapY, gap, fixedKey, shiftGroup);
      }
    }
    if (!adjusted) {
      break;
    }
  }

  return moved;
}

/** 单对相交分组的推开：沿穿透量较小的轴整体平移，固定组不动 */
function separatePair<N extends RectNode>(
  a: SeparationEntry<N>,
  b: SeparationEntry<N>,
  overlapX: number,
  overlapY: number,
  gap: number,
  fixedKey: string | null,
  shiftGroup: (entry: SeparationEntry<N>, dx: number, dy: number) => void,
): void {
  const aFixed = a.key === fixedKey;
  const bFixed = b.key === fixedKey;

  if (overlapX <= overlapY) {
    // 水平推开：左边的向左、右边的向右
    const aShift = separateShift(aFixed, bFixed, overlapX + gap);
    const bShift = separateShift(bFixed, aFixed, overlapX + gap);
    const aLeft = (a.box.minX + a.box.maxX) / 2 <= (b.box.minX + b.box.maxX) / 2;
    shiftGroup(a, aLeft ? -aShift : aShift, 0);
    shiftGroup(b, aLeft ? bShift : -bShift, 0);
    return;
  }
  // 垂直推开：上边的向上、下边的向下
  const aShift = separateShift(aFixed, bFixed, overlapY + gap);
  const bShift = separateShift(bFixed, aFixed, overlapY + gap);
  const aTop = (a.box.minY + a.box.maxY) / 2 <= (b.box.minY + b.box.maxY) / 2;
  shiftGroup(a, 0, aTop ? -aShift : aShift);
  shiftGroup(b, 0, aTop ? bShift : -bShift);
}

/** 机柜分组键：优先按 id，无 id 时按名称；不属于任何机柜返回 null */
export function cabinetGroupKey(node: {
  cabinet_id: string | null;
  cabinet_name: string | null;
}): string | null {
  if (node.cabinet_id) {
    return `cab:${node.cabinet_id}`;
  }
  if (node.cabinet_name) {
    return `cab:name:${node.cabinet_name}`;
  }
  return null;
}

/** 房间分组键（缺失房间归入"未分房间"） */
export function roomGroupKey(roomId: string | null): string {
  return roomId ? `room:${roomId}` : "room:none";
}

/**
 * 自动布局：按 房间 → 机柜 分组排布。
 *
 * 房间内机柜横向排列（柜内设备三列网格），散件设备在机柜行下方四列排布；
 * 房间块按三列网格铺开，保证房间/机柜容器互不覆盖。直接改写节点矩形。
 */
export function applyGroupedLayout<N extends RectNode>(
  rooms: TopologyRoomGroup<N>[],
): void {
  const nodeWidth = TOPOLOGY_NODE_WIDTH;
  const nodeHeight = TOPOLOGY_NODE_HEIGHT;

  // 1) 房间内布局（局部坐标，内容区从 (0, 0) 开始）
  const blocks: { room: TopologyRoomGroup<N>; box: GroupBox | null }[] = [];
  for (const room of rooms) {
    let cursorX = 0;
    let cabinetsHeight = 0;

    for (const cabinet of room.cabinets) {
      const cols = Math.min(cabinet.nodes.length, TOPOLOGY_CABINET_COLS);
      const rows = Math.ceil(cabinet.nodes.length / TOPOLOGY_CABINET_COLS);
      const cabinetWidth =
        cols * (nodeWidth + TOPOLOGY_GRID_X) -
        TOPOLOGY_GRID_X +
        CABINET_PADDING * 2;
      const cabinetHeight =
        rows * (nodeHeight + TOPOLOGY_GRID_Y) -
        TOPOLOGY_GRID_Y +
        CABINET_PADDING * 2 +
        CABINET_HEADER;

      cabinet.nodes.forEach((node, index) => {
        const col = index % TOPOLOGY_CABINET_COLS;
        const row = Math.floor(index / TOPOLOGY_CABINET_COLS);
        node.rect = {
          x: cursorX + CABINET_PADDING + col * (nodeWidth + TOPOLOGY_GRID_X),
          y: CABINET_PADDING + CABINET_HEADER + row * (nodeHeight + TOPOLOGY_GRID_Y),
          width: nodeWidth,
          height: nodeHeight,
        };
      });

      cabinetsHeight = Math.max(cabinetsHeight, cabinetHeight);
      cursorX += cabinetWidth + TOPOLOGY_CABINET_GAP;
    }

    // 散件区：机柜行下方（无机柜时从 (0, 0) 开始）
    const looseY = room.cabinets.length > 0 ? cabinetsHeight + ROOM_PADDING : 0;
    room.loose.forEach((node, index) => {
      const col = index % TOPOLOGY_LOOSE_COLS;
      const row = Math.floor(index / TOPOLOGY_LOOSE_COLS);
      node.rect = {
        x: col * (nodeWidth + TOPOLOGY_GRID_X),
        y: looseY + row * (nodeHeight + TOPOLOGY_GRID_Y),
        width: nodeWidth,
        height: nodeHeight,
      };
    });

    blocks.push({ room, box: computeGroupBox(room.nodes, ROOM_PADDING, ROOM_HEADER) });
  }

  // 2) 房间块网格铺开（全局坐标）
  let cursorX = TOPOLOGY_LAYOUT_ORIGIN;
  let cursorY = TOPOLOGY_LAYOUT_ORIGIN;
  let rowHeight = 0;
  let column = 0;

  for (const block of blocks) {
    if (!block.box) {
      continue;
    }
    if (column >= TOPOLOGY_ROOM_COLS) {
      column = 0;
      cursorX = TOPOLOGY_LAYOUT_ORIGIN;
      cursorY += rowHeight + TOPOLOGY_ROOM_GAP_Y;
      rowHeight = 0;
    }

    const dx = Math.round(cursorX - block.box.minX);
    const dy = Math.round(cursorY - block.box.minY);
    for (const node of block.room.nodes) {
      node.rect.x = Math.round(node.rect.x) + dx;
      node.rect.y = Math.round(node.rect.y) + dy;
    }

    const blockWidth = block.box.maxX - block.box.minX;
    const blockHeight = block.box.maxY - block.box.minY;
    cursorX += blockWidth + ROOM_GAP;
    rowHeight = Math.max(rowHeight, blockHeight);
    column += 1;
  }
}

/** 整组平移（移动到指定容器原点，坐标模态框用） */
export function moveGroupTo<N extends RectNode>(
  nodes: N[],
  padding: number,
  header: number,
  targetX: number,
  targetY: number,
): boolean {
  const box = computeGroupBox(nodes, padding, header);
  if (!box) {
    return false;
  }
  const dx = targetX - box.minX;
  const dy = targetY - box.minY;
  if (dx === 0 && dy === 0) {
    return false;
  }
  for (const node of nodes) {
    node.rect.x = Math.round(node.rect.x + dx);
    node.rect.y = Math.round(node.rect.y + dy);
  }
  return true;
}

// ==================== 拓扑：连线路径与锚点 ====================

/**
 * 选择连线出入口锚点：按两节点中心的主轴方向取向，
 * 同侧边缘按连接次序错开（中心、±S、±2S…），避免端口标签重叠。
 */
export function pickAnchor(
  source: CanvasRect,
  target: CanvasRect,
  slotIndex: number,
): AnchorPoint {
  const centerX = source.x + source.width / 2;
  const centerY = source.y + source.height / 2;
  const targetCenterX = target.x + target.width / 2;
  const targetCenterY = target.y + target.height / 2;
  const dx = targetCenterX - centerX;
  const dy = targetCenterY - centerY;

  let anchor: AnchorPoint;
  if (Math.abs(dx) > Math.abs(dy)) {
    anchor =
      dx > 0
        ? { x: source.x + source.width, y: centerY, dir: "right" }
        : { x: source.x, y: centerY, dir: "left" };
  } else if (dy > 0) {
    anchor = { x: centerX, y: source.y + source.height, dir: "bottom" };
  } else {
    anchor = { x: centerX, y: source.y, dir: "top" };
  }

  const magnitude = Math.floor((slotIndex + 1) / 2);
  const offset = magnitude * (slotIndex % 2 === 0 ? -ANCHOR_SPREAD : ANCHOR_SPREAD);

  if (anchor.dir === "top" || anchor.dir === "bottom") {
    anchor.x = Math.max(
      source.x + 16,
      Math.min(source.x + source.width - 16, anchor.x + offset),
    );
  } else {
    anchor.y = Math.max(
      source.y + 16,
      Math.min(source.y + source.height - 16, anchor.y + offset),
    );
  }
  return anchor;
}

/**
 * 正交折线路径（物理/逻辑连线共用的直角走线）。
 * 端点方向决定先水平还是先垂直，拐点按中点落在两节点之间并留 20px 缓冲。
 */
export function computeOrthogonalPath(
  source: AnchorPoint,
  target: AnchorPoint,
  parallelOffset = 0,
): string {
  const dx = target.x - source.x;
  const dy = target.y - source.y;

  if (source.dir === "bottom" || source.dir === "top") {
    const midY = source.y + dy * 0.5 + parallelOffset;
    const elbowY =
      source.dir === "bottom" ? midY : source.y - Math.abs(dy) * 0.5 - parallelOffset;
    const clamped =
      source.dir === "bottom"
        ? Math.max(source.y + 20, Math.min(target.y - 20, elbowY))
        : Math.min(source.y - 20, Math.max(target.y + 20, elbowY));
    return `M ${source.x} ${source.y} L ${source.x} ${clamped} L ${target.x} ${clamped} L ${target.x} ${target.y}`;
  }

  if (source.dir === "right" || source.dir === "left") {
    const midX = source.x + dx * 0.5 + parallelOffset;
    const elbowX =
      source.dir === "right" ? midX : source.x - Math.abs(dx) * 0.5 - parallelOffset;
    const clamped =
      source.dir === "right"
        ? Math.max(source.x + 20, Math.min(target.x - 20, elbowX))
        : Math.min(source.x - 20, Math.max(target.x + 20, elbowX));
    return `M ${source.x} ${source.y} L ${clamped} ${source.y} L ${clamped} ${target.y} L ${target.x} ${target.y}`;
  }

  return `M ${source.x} ${source.y} L ${target.x} ${target.y}`;
}

/** 设备节点边缘锚点（固定方向，端口锚点圆点位置） */
export function nodeEdgeAnchor(
  rect: CanvasRect,
  direction: AnchorDirection,
): { x: number; y: number } {
  switch (direction) {
    case "top":
      return { x: rect.x + rect.width / 2, y: rect.y };
    case "right":
      return { x: rect.x + rect.width, y: rect.y + rect.height / 2 };
    case "bottom":
      return { x: rect.x + rect.width / 2, y: rect.y + rect.height };
    default:
      return { x: rect.x, y: rect.y + rect.height / 2 };
  }
}

/** 临时连线（连线模式下跟随鼠标的贝塞尔示意） */
export function computeTempConnectionPath(
  from: { x: number; y: number },
  to: { x: number; y: number },
): string {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const distance = Math.sqrt(dx * dx + dy * dy);
  const offset = Math.min(distance * 0.3, 80);
  return `M ${from.x} ${from.y} C ${from.x + offset} ${from.y}, ${to.x - offset} ${to.y}, ${to.x} ${to.y}`;
}

/** 视野适配矩形：包含全部矩形并留白 */
export function fitRects(
  rects: CanvasRect[],
  padding: number,
  fallback: CanvasRect,
): CanvasRect {
  if (rects.length === 0) {
    return fallback;
  }
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const rect of rects) {
    minX = Math.min(minX, rect.x);
    minY = Math.min(minY, rect.y);
    maxX = Math.max(maxX, rect.x + rect.width);
    maxY = Math.max(maxY, rect.y + rect.height);
  }
  if (!Number.isFinite(minX) || !Number.isFinite(minY)) {
    return fallback;
  }
  return {
    x: minX - padding,
    y: minY - padding,
    width: Math.max(1, maxX - minX + padding * 2),
    height: Math.max(1, maxY - minY + padding * 2),
  };
}

// ==================== IP 归并 ====================

/** 工位 ID → IP 明细（同一工位多条 IP 时 active 优先） */
export function mapIpsByWorkstation<T extends { workstation_id: string | null; status: string }>(
  ips: T[],
): Map<string, T> {
  const map = new Map<string, T>();
  for (const ip of ips) {
    if (!ip.workstation_id) {
      continue;
    }
    const existing = map.get(ip.workstation_id);
    if (!existing || (existing.status !== "active" && ip.status === "active")) {
      map.set(ip.workstation_id, ip);
    }
  }
  return map;
}

/** 机位 ID → IP 明细（同机位多条时 active 优先） */
export function mapIpsByPosition<T extends { position_id: string | null; status: string }>(
  ips: T[],
): Map<string, T> {
  const map = new Map<string, T>();
  for (const ip of ips) {
    if (!ip.position_id) {
      continue;
    }
    const existing = map.get(ip.position_id);
    if (!existing || (existing.status !== "active" && ip.status === "active")) {
      map.set(ip.position_id, ip);
    }
  }
  return map;
}

/** 剔除重复项后分块（机柜分批渲染与 IP 批量查询共用） */
export function chunk<T>(items: T[], size: number): T[][] {
  if (size <= 0) {
    return [];
  }
  const result: T[][] = [];
  for (let index = 0; index < items.length; index += size) {
    result.push(items.slice(index, index + size));
  }
  return result;
}

/** 拓扑机柜分组类型别名再导出（供调用方少一层类型 import） */
export type { TopologyCabinetGroup, TopologyRoomGroup };
