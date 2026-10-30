/**
 * 服务端载荷 → 画布视图模型（纯函数，无 DOM 依赖）。
 *
 * 旧实现边读接口边画 DOM，坐标只存在于 SVG 属性上，保存时再反读；Vue 化后
 * 视图模型是坐标的唯一真相：本模块把「接口载荷 + 已保存布局」投影成带
 * `rect` 的节点数组，渲染层只负责按 `rect` 输出声明式模板。
 *
 * 三态语义（成功/无布局/失败）不在此判定：本模块只在调用方确认拿到
 * 布局数据后组装视图，异常坐标按各自的历史回退策略就地修正。
 */
import {
  computeCabinetLayout,
  computeCabinetPositionRect,
  DEFAULT_CABINET_CAPACITY,
  DOOR_RECT,
  generateDomId,
  isValidRect,
  fallbackWorkstationRect,
  CABINET_EDGE_PADDING,
  MIN_VIEW_HEIGHT,
  MIN_VIEW_WIDTH,
  TOPOLOGY_NODE_HEIGHT,
  TOPOLOGY_NODE_WIDTH,
  type CabinetLayoutInput,
} from "./geometry";
import type { IpDetailWithNames } from "../../types/api/generated/IpDetailWithNames";
import type { LayoutItem } from "../../types/api/generated/LayoutItem";
import type { Position } from "../../types/api/generated/Position";
import type { TopologyNodeWithDevice } from "../../types/api/generated/TopologyNodeWithDevice";
import type { WorkstationWithDetails } from "../../types/api/generated/WorkstationWithDetails";
import type {
  CabinetCanvasNode,
  CabinetPositionCanvasNode,
  CanvasRect,
  DoorCanvasNode,
  RoomCabinet,
  TopologyCanvasNode,
  TopologyRoomGroup,
  WorkstationCanvasNode,
} from "../../types/visualization";

/** 画布内容四周留白（与几何模块 `CANVAS_CONTENT_PADDING` 同值，独立常量避免循环意图） */
const CONTENT_PADDING = 50;

/**
 * 布局项按元素 id 建索引（键统一小写）。
 * 工位/机柜数量多时避免逐个 `find`（O(N×M)）。
 */
export function indexLayoutItems(items: LayoutItem[]): Map<string, LayoutItem> {
  const map = new Map<string, LayoutItem>();
  for (const item of items) {
    if (item.id) {
      map.set(item.id.toLowerCase(), item);
    }
  }
  return map;
}

/** 从布局项里取合法坐标：非有限值/负值/零宽高一律视为「无已保存位置」 */
export function savedPosition(item: LayoutItem | undefined): Position | null {
  if (!item) {
    return null;
  }
  const { position } = item;
  return isValidRect(position) ? position : null;
}

/** 门元素：已保存记录沿用其主键（保证可拖拽可保存），未保存时生成临时 id */
export function buildDoorNode(layout: LayoutItem[]): DoorCanvasNode {
  const doorItem = layout.find((item) => item.element_type === "door");
  const rect = savedPosition(doorItem) ?? { ...DOOR_RECT };
  return { id: doorItem?.id ?? generateDomId(), rect };
}

/** 工位画布输入 */
export interface WorkstationCanvasInput {
  workstations: WorkstationWithDetails[];
  /** 已保存布局项（空数组表示无已保存布局，调用方自行决定是否自动排布） */
  layout: LayoutItem[];
  /** IP 明细（按 workstation_id 归并后取一条展示） */
  ipDetails: IpDetailWithNames[];
}

/** 工位画布视图模型 */
export interface WorkstationCanvas {
  nodes: WorkstationCanvasNode[];
  door: DoorCanvasNode;
}

/**
 * 工位画布视图模型。
 *
 * 坐标优先级：已保存且合法 → 使用；否则按 4 列网格回退（与旧实现一致，
 * 保证新增工位在已有布局的房间内仍有可点击落点）。
 */
export function buildWorkstationCanvas(input: WorkstationCanvasInput): WorkstationCanvas {
  const { workstations, layout, ipDetails } = input;
  const layoutById = indexLayoutItems(layout.filter((item) => item.element_type !== "door"));
  const door = buildDoorNode(layout);
  const ipMap = mapIpDetailsByWorkstation(ipDetails);

  // 逐条组装画布节点：载荷字段浅拷贝 + 派生 rect/ipDetail，不就地改写载荷对象
  const nodes: WorkstationCanvasNode[] = [];
  for (const [index, workstation] of workstations.entries()) {
    const rect = savedPosition(layoutById.get(workstation.id.toLowerCase())) ?? fallbackWorkstationRect(index);
    nodes.push({
      ...workstation,
      rect,
      ipDetail: ipMap.get(workstation.id) ?? null,
    });
  }

  return { nodes, door };
}

/** IP 明细按工位归并：同一工位多条时 active 优先 */
export function mapIpDetailsByWorkstation(ipDetails: IpDetailWithNames[]): Map<string, IpDetailWithNames> {
  const map = new Map<string, IpDetailWithNames>();
  for (const detail of ipDetails) {
    const key = detail.workstation_id;
    if (!key) {
      continue;
    }
    const existing = map.get(key);
    if (!existing || (existing.status !== "active" && detail.status === "active")) {
      map.set(key, detail);
    }
  }
  return map;
}

/**
 * 内容视野：覆盖全部矩形并至少达到最小视野与容器尺寸。
 * 与自动排布用的 `workstationCanvasRect` 不同——本函数服从实际（含拖动后）坐标。
 */
export function contentViewBox(rects: CanvasRect[], container: { width: number; height: number }): CanvasRect {
  let maxX = 0;
  let maxY = 0;
  for (const rect of rects) {
    if (!isValidRect(rect)) {
      continue;
    }
    maxX = Math.max(maxX, rect.x + rect.width);
    maxY = Math.max(maxY, rect.y + rect.height);
  }
  return {
    x: 0,
    y: 0,
    width: Math.max(MIN_VIEW_WIDTH, maxX + CONTENT_PADDING, container.width || 0),
    height: Math.max(MIN_VIEW_HEIGHT, maxY + CONTENT_PADDING, container.height || 0),
  };
}

/** 机柜画布输入 */
export interface CabinetCanvasInput {
  cabinets: RoomCabinet[];
  layout: LayoutItem[];
  /** 容器真实高度（决定 U 高与柜底贴底位置） */
  containerHeight: number;
}

/** 机柜画布视图模型 */
export interface CabinetCanvas {
  nodes: CabinetCanvasNode[];
  /** 内容总宽（区域宽度、横向滚动占位宽度） */
  totalWidth: number;
}

/**
 * 机柜画布视图模型。
 *
 * 机位坐标由「机柜矩形 + U 位区间」推导，机位 IP 明细留空——机柜按批渲染，
 * IP 由批次拉到后回填（避免逐柜请求）。
 */
export function buildCabinetCanvas(input: CabinetCanvasInput): CabinetCanvas {
  const { cabinets, layout, containerHeight } = input;
  const layoutById = indexLayoutItems(layout);

  const layoutInputs: CabinetLayoutInput[] = cabinets.map((cabinet) => {
    const position = savedPosition(layoutById.get(cabinet.id.toLowerCase()));
    return {
      id: cabinet.id,
      capacity: cabinet.capacity ?? DEFAULT_CABINET_CAPACITY,
      savedX: position?.x ?? null,
      savedWidth: position?.width ?? null,
    };
  });

  const computed = computeCabinetLayout(layoutInputs, containerHeight);
  const entryById = new Map(computed.entries.map((entry) => [entry.id, entry]));

  const nodes: CabinetCanvasNode[] = [];
  for (const cabinet of cabinets) {
    const entry = entryById.get(cabinet.id);
    const rect = entry?.rect ?? {
      x: CABINET_EDGE_PADDING,
      y: 0,
      width: 0,
      height: 0,
    };
    const capacity = entry?.capacity ?? DEFAULT_CABINET_CAPACITY;
    const positions: CabinetPositionCanvasNode[] = [];
    for (const position of cabinet.positions) {
      positions.push({
        ...position,
        rect: computeCabinetPositionRect(rect, capacity, computed.uHeight, position.start_u, position.end_u),
        ipDetail: null,
      });
    }
    nodes.push({
      ...cabinet,
      capacity,
      rect,
      uHeight: computed.uHeight,
      positions,
    });
  }

  return { nodes, totalWidth: computed.totalWidth };
}

/** 拓扑节点坐标兜底（异常数据按网格原点排布，避免 NaN 进 viewBox） */
function topologyRect(node: TopologyNodeWithDevice): CanvasRect {
  const rect = { x: node.x, y: node.y, width: node.width, height: node.height };
  if (isValidRect(rect)) {
    return rect;
  }
  return { x: 100, y: 100, width: TOPOLOGY_NODE_WIDTH, height: TOPOLOGY_NODE_HEIGHT };
}

/** 拓扑节点 → 画布视图模型（附加矩形） */
export function buildTopologyCanvasNodes(nodes: TopologyNodeWithDevice[]): TopologyCanvasNode[] {
  return nodes.map((node) => ({ ...node, rect: topologyRect(node) }));
}

/** 拓扑分组标题的本地化文案（保持本模块无 i18n 依赖，由调用方注入） */
export interface TopologyGroupLabels {
  /** 无机柜归属的容器标题 */
  noCabinet: string;
}

/**
 * 空间分组：房间 → 机柜 两级。
 * 按房间 id 分组（设备必属房间、房间必属组织，故不存在无房间分组）；
 * 房间内有指定机柜的设备时成组，其余归为散件。保持首次出现顺序，
 * 布局推挤以此顺序为初始序列。
 */
export function groupTopologyNodes(
  nodes: TopologyCanvasNode[],
  labels: TopologyGroupLabels,
): TopologyRoomGroup<TopologyCanvasNode>[] {
  const rooms = new Map<string, TopologyRoomGroup<TopologyCanvasNode>>();

  for (const node of nodes) {
    const roomKey = `room:${node.room_id}`;
    let room = rooms.get(roomKey);
    if (!room) {
      room = {
        key: roomKey,
        label: node.room_name,
        orgLabel: node.org_name,
        cabinets: [],
        loose: [],
        nodes: [],
      };
      rooms.set(roomKey, room);
    }
    room.nodes.push(node);

    const cabinetKey = cabinetGroupKeyOf(node);
    if (!cabinetKey) {
      room.loose.push(node);
      continue;
    }
    let cabinet = room.cabinets.find((group) => group.key === cabinetKey);
    if (!cabinet) {
      cabinet = { key: cabinetKey, label: node.cabinet_name || labels.noCabinet, nodes: [] };
      room.cabinets.push(cabinet);
    }
    cabinet.nodes.push(node);
  }

  return [...rooms.values()];
}

/** 机柜分组键（与几何模块 `cabinetGroupKey` 同口径，此处按画布节点取字段） */
export function cabinetGroupKeyOf(node: { cabinet_id: string | null; cabinet_name: string | null }): string | null {
  if (node.cabinet_id) {
    return `cab:${node.cabinet_id}`;
  }
  if (node.cabinet_name) {
    return `cab:name:${node.cabinet_name}`;
  }
  return null;
}

/** 可见节点过滤：组织筛选集合为 null 时显示全部 */
export function filterNodesByOrg(
  nodes: TopologyCanvasNode[],
  orgFilter: Set<string> | null,
): TopologyCanvasNode[] {
  if (!orgFilter) {
    return nodes;
  }
  return nodes.filter((node) => orgFilter.has(node.org_id));
}
