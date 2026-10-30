/**
 * 可视化画布的前端自有契约与视图模型（手写，非 ts-rs 生成物）。
 *
 * 服务端业务载荷一律复用 `./api/generated/` 生成类型；本文件只补两类东西：
 * 1. 服务端未纳入 ts-rs 的手写 JSON 响应（机柜房间图 `room-cabinets`）；
 * 2. 画布视图模型：在服务端字段之上附加几何矩形（`rect`）与联查详情，
 *    供响应式组件直接渲染，不再从 DOM 反读坐标。
 */
import type { LayoutItem } from "./api/generated/LayoutItem";
import type { IpDetailWithNames } from "./api/generated/IpDetailWithNames";
import type { Position } from "./api/generated/Position";
import type { TopologyConnectionView } from "./api/generated/TopologyConnectionView";
import type { TopologyNodeWithDevice } from "./api/generated/TopologyNodeWithDevice";
import type { WorkstationWithDetails } from "./api/generated/WorkstationWithDetails";

/** 画布矩形（世界坐标，viewBox 1:1 像素） */
export interface CanvasRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** 锚点方向（拓扑连线出入口） */
export type AnchorDirection = "top" | "right" | "bottom" | "left";

export interface AnchorPoint {
  x: number;
  y: number;
  dir: AnchorDirection;
}

/**
 * 工位画布节点：列表载荷（`WorkstationWithDetails`，不含 IP 明细）+
 * 画布矩形 + 归并后的 IP 明细行。
 */
export type WorkstationCanvasNode = Omit<WorkstationWithDetails, "ips"> & {
  rect: CanvasRect;
  ipDetail: IpDetailWithNames | null;
};

/** 门元素：`id` 为 element_layouts 主键，未保存过时为随机 DOM 标识 */
export interface DoorCanvasNode {
  id: string;
  rect: CanvasRect;
}

/**
 * 房间机柜图里的机位（后端手写 JSON：id/name/cabinet_id/start_u/end_u/description）。
 */
export interface RoomCabinetPosition {
  id: string;
  name: string;
  cabinet_id: string | null;
  start_u: number;
  end_u: number;
  description: string | null;
}

/**
 * 房间机柜图条目（`GET /api/resources/layouts/room-cabinets/{room_id}`
 * 的裸数组元素；该端点不在 ts-rs 生成范围内，故手写）。
 */
export interface RoomCabinet {
  id: string;
  name: string;
  room_id: string;
  capacity: number;
  description: string | null;
  positions: RoomCabinetPosition[];
  /** 已保存的机柜坐标（cabinet_layouts），未保存时为 null */
  layout: Position | null;
}

/** 机柜机位画布节点：机位 + 画布矩形 + IP 明细 */
export interface CabinetPositionCanvasNode extends RoomCabinetPosition {
  rect: CanvasRect;
  ipDetail: IpDetailWithNames | null;
}

/** 机柜画布节点：房间机柜图条目 + 画布矩形 + 单 U 高度 + 机位子节点 */
export interface CabinetCanvasNode extends Omit<RoomCabinet, "positions"> {
  rect: CanvasRect;
  /** 单 U 像素高度（按容器高与最大容量整除，全房间统一） */
  uHeight: number;
  positions: CabinetPositionCanvasNode[];
}

/** 拓扑设备节点：服务端节点载荷 + 画布矩形 */
export type TopologyCanvasNode = TopologyNodeWithDevice & { rect: CanvasRect };

/** 拓扑连线（直接复用生成类型） */
export type TopologyConnection = TopologyConnectionView;

/** 拓扑空间分组：房间 → 机柜 两级 */
export interface TopologyCabinetGroup<N> {
  key: string;
  label: string;
  nodes: N[];
}

/** 拓扑房间分组 */
export interface TopologyRoomGroup<N> {
  key: string;
  label: string;
  orgLabel: string;
  cabinets: TopologyCabinetGroup<N>[];
  loose: N[];
  nodes: N[];
}

/** 布局归属实体：保存前由视图模型投影，替代旧实现从 SVG DOM 反读属性 */
export interface LayoutEntity {
  id: string;
  elementType: string;
  rect: CanvasRect;
}

/** 布局保存项（生成类型别名，便于调用处少一层 import） */
export type CanvasLayoutItem = LayoutItem;
