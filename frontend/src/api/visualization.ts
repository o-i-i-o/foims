/**
 * 可视化模块的 API 封装（纯传输层）。
 *
 * 职责边界：
 * - 只负责「组装 URL / 请求体 → 返回 `ApiResult`」，不做通知、不做 i18n、
 *   不做状态管理，也不吞掉四态语义（成功/失败/空/被取代由调用方判定）；
 * - client 层永不 reject，因此本层也不再包 try/catch；
 * - 载荷类型一律取 `types/api/generated/`（ts-rs 生成物），未纳入生成的
 *   手写响应（`room-cabinets`）取 `types/visualization.ts`。
 */
import { apiDelete, apiGet, apiPost } from "./client";
import type { DeviceInterface } from "../types/api/generated/DeviceInterface";
import type { DeviceLldp } from "../types/api/generated/DeviceLldp";
import type { IpDetailWithNames } from "../types/api/generated/IpDetailWithNames";
import type { LayoutItem } from "../types/api/generated/LayoutItem";
import type { LayoutSaveRequest } from "../types/api/generated/LayoutSaveRequest";
import type { OrganizationTreeNode } from "../types/api/generated/OrganizationTreeNode";
import type { Room } from "../types/api/generated/Room";
import type { SnmpMac } from "../types/api/generated/SnmpMac";
import type { TopologyConnectionRequest } from "../types/api/generated/TopologyConnectionRequest";
import type { TopologyConnectionView } from "../types/api/generated/TopologyConnectionView";
import type { TopologyNodeItem } from "../types/api/generated/TopologyNodeItem";
import type { TopologyNodeWithDevice } from "../types/api/generated/TopologyNodeWithDevice";
import type { ApiResult, Paged } from "../types/api";
import type { RoomCabinet } from "../types/visualization";

/** 裸数组端点归一：服务端 success 但非数组视为异常形状，按空数组处理 */
export function resultArray<T>(result: ApiResult<T[] | null>): T[] {
  return result.success && Array.isArray(result.data) ? result.data : [];
}

/**
 * 裸数组 / `{items}` 双形态端点归一（macs、lldp-neighbors 实测为裸数组，
 * 分页形态仅作兼容）。
 */
export function resultList<T>(result: ApiResult<T[] | { items?: T[] } | null>): T[] {
  if (!result.success || !result.data) {
    return [];
  }
  if (Array.isArray(result.data)) {
    return result.data;
  }
  return Array.isArray(result.data.items) ? result.data.items : [];
}

/** 工位布局：`element_layouts` 中该房间的工位与门记录 */
export function getWorkstationLayout(roomId: string): Promise<ApiResult<LayoutItem[]>> {
  return apiGet<LayoutItem[]>(`/api/resources/layouts/workstation/${encodeURIComponent(roomId)}`);
}

/** 机柜布局：`element_layouts` 中该房间的机柜记录 */
export function getCabinetPositionLayout(roomId: string): Promise<ApiResult<LayoutItem[]>> {
  return apiGet<LayoutItem[]>(`/api/resources/layouts/positions/${encodeURIComponent(roomId)}`);
}

/** 房间机柜图（机柜 + 内嵌机位 + 已保存坐标），裸数组响应 */
export function getRoomCabinets(roomId: string): Promise<ApiResult<RoomCabinet[]>> {
  return apiGet<RoomCabinet[]>(`/api/resources/layouts/room-cabinets/${encodeURIComponent(roomId)}`);
}

/** 整表 upsert 保存布局（`type` 取 workstation / cabinet） */
export function saveLayout(request: LayoutSaveRequest): Promise<ApiResult<unknown>> {
  return apiPost<unknown, LayoutSaveRequest>("/api/resources/layouts", request);
}

/** 删除房间工位布局 */
export function deleteWorkstationLayout(roomId: string): Promise<ApiResult<unknown>> {
  return apiDelete<unknown>(`/api/resources/layouts/workstation/${encodeURIComponent(roomId)}`);
}

/** 删除房间机柜布局 */
export function deleteCabinetLayout(roomId: string): Promise<ApiResult<unknown>> {
  return apiDelete<unknown>(`/api/resources/layouts/positions/${encodeURIComponent(roomId)}`);
}

/** 房间工位列表路径（供 `fetchAllPages` 逐页拼接分页参数） */
export function workstationsByRoomPath(roomId: string): string {
  return `/api/resources/workstations?room_id=${encodeURIComponent(roomId)}`;
}

/** 全量 IP 明细路径 */
export const IP_LIST_PATH = "/api/resources/ip";

/** 按机位 ID 批量取 IP 明细路径：单批 16 柜 × 48U = 768 条，1000 足够覆盖 */
export function ipsByPositionIdsPath(positionIds: string[]): string {
  return `${IP_LIST_PATH}?page_size=1000&position_ids=${encodeURIComponent(positionIds.join(","))}`;
}

/** 按机位 ID 批量取 IP 明细 */
export function getIpsByPositionIds(positionIds: string[]): Promise<ApiResult<Paged<IpDetailWithNames>>> {
  return apiGet<Paged<IpDetailWithNames>>(ipsByPositionIdsPath(positionIds));
}

/** 拓扑节点（含设备联查信息），裸数组响应 */
export function getTopologyNodes(): Promise<ApiResult<TopologyNodeWithDevice[]>> {
  return apiGet<TopologyNodeWithDevice[]>("/api/resources/topology/nodes");
}

/** 保存拓扑节点坐标（按 `device_id` upsert） */
export function saveTopologyNodes(nodes: TopologyNodeItem[]): Promise<ApiResult<unknown>> {
  return apiPost<unknown, { nodes: TopologyNodeItem[] }>("/api/resources/topology/nodes", { nodes });
}

/** 删除拓扑节点（仅移除画布位置，不删设备） */
export function deleteTopologyNode(deviceId: string): Promise<ApiResult<unknown>> {
  return apiDelete<unknown>(`/api/resources/topology/nodes/${encodeURIComponent(deviceId)}`);
}

/** 拓扑连线（含派生物理连线与逻辑连线），裸数组响应 */
export function getTopologyConnections(): Promise<ApiResult<TopologyConnectionView[]>> {
  return apiGet<TopologyConnectionView[]>("/api/resources/topology/connections");
}

/** 创建连线 */
export function createTopologyConnection(
  request: TopologyConnectionRequest,
): Promise<ApiResult<TopologyConnectionView>> {
  return apiPost<TopologyConnectionView, TopologyConnectionRequest>(
    "/api/resources/topology/connections",
    request,
  );
}

/** 删除连线（派生连线不可删，后端拒绝） */
export function deleteTopologyConnection(id: string): Promise<ApiResult<unknown>> {
  return apiDelete<unknown>(`/api/resources/topology/connections/${encodeURIComponent(id)}`);
}

/** 自动发现结果 */
export interface TopologyAutoDiscoverResult {
  added_nodes: number;
  discovered_connections: number;
}

/** 自动发现：把未入图的设备补进拓扑并按 LLDP 推断连线 */
export function autoDiscoverTopology(): Promise<ApiResult<TopologyAutoDiscoverResult>> {
  return apiPost<TopologyAutoDiscoverResult, Record<string, never>>(
    "/api/resources/topology/auto-discover",
    {},
  );
}

/** 设备端口列表路径（供 `fetchAllPages` 逐页拼接分页参数） */
export function deviceInterfacesPath(deviceId: string): string {
  return `/api/resources/devices/${encodeURIComponent(deviceId)}/interfaces`;
}

/** 设备 MAC/ARP 表，裸数组响应 */
export function getDeviceMacs(deviceId: string): Promise<ApiResult<SnmpMac[] | { items?: SnmpMac[] }>> {
  return apiGet<SnmpMac[] | { items?: SnmpMac[] }>(
    `/api/resources/devices/${encodeURIComponent(deviceId)}/macs`,
  );
}

/** 设备 LLDP 邻居，裸数组响应 */
export function getDeviceLldp(deviceId: string): Promise<ApiResult<DeviceLldp[] | { items?: DeviceLldp[] }>> {
  return apiGet<DeviceLldp[] | { items?: DeviceLldp[] }>(
    `/api/resources/devices/${encodeURIComponent(deviceId)}/lldp-neighbors`,
  );
}

/** 设备端口类型别名（调用方少一层 import） */
export type DevicePort = DeviceInterface;

/** 组织树 */
export function getOrganizationTree(): Promise<ApiResult<OrganizationTreeNode[]>> {
  return apiGet<OrganizationTreeNode[]>("/api/resources/organizations/tree");
}

/** 房间列表查询参数 */
export interface RoomQuery {
  /** 组织过滤（null 为不过滤） */
  orgId?: string | null;
  /** 逗号分隔的房间类型过滤（null 为不过滤） */
  roomTypes?: string | null;
}

/** 构造房间列表路径（`org_id` / `room_type` 过滤由后端执行） */
export function roomsPath({ orgId = null, roomTypes = null }: RoomQuery = {}): string {
  const params = new URLSearchParams({ page_size: "1000" });
  if (orgId) {
    params.set("org_id", orgId);
  }
  if (roomTypes) {
    params.set("room_type", roomTypes);
  }
  return `/api/resources/rooms?${params.toString()}`;
}

/** 房间列表（下拉只需要 id/name，故按 `Room` 消费） */
export function getRooms(query: RoomQuery = {}): Promise<ApiResult<Paged<Room>>> {
  return apiGet<Paged<Room>>(roomsPath(query));
}
