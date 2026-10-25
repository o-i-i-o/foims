/**
 * 手写 API 契约：ts-rs 生成物覆盖不到的载荷。
 *
 * 后端用 json! 动态构造、或跨表拼装出的响应不会进入 `types/api/generated/`，
 * 这里集中声明并由前端维护；改动须对照后端 handler 同步确认。
 * （`types/api/index.ts` 只放与后端无关的通用信封类型。）
 */
import type { CabinetBrief } from "./generated/CabinetBrief";
import type { NetOutletBrief } from "./generated/NetOutletBrief";
import type { NetworkInfo } from "./generated/NetworkInfo";
import type { Room } from "./generated/Room";
import type { WorkstationBrief } from "./generated/WorkstationBrief";

/**
 * GET /api/resources/rooms/{id}/brief 响应（后端 room.rs get_room_brief 组装：
 * Room 全部字段 + 各子项 brief 列表，供房间详情弹窗回填使用）。
 */
export type RoomBrief = Room & {
  networks: NetworkInfo[];
  workstations: WorkstationBrief[];
  cabinets: CabinetBrief[];
  net_outlets: NetOutletBrief[];
};
