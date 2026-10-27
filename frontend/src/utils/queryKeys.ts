/**
 * 全局缓存 key 唯一来源（TanStack Query 按前缀匹配失效）。
 *
 * 只登记「跨组件共享」的 key：查询方与失效方必须引用同一常量，否则 key 分裂
 * 会让一侧的 invalidateQueries 打不中另一侧的缓存（历史上组织树同时存在
 * ["organizations","tree"] 与 ["organizations-tree"] 两套 key，任一侧失效都失效）。
 *
 * 分页列表把分页/筛选参数追加在 key 尾部（usePagedList 生成的形态为
 * [...key, { page, pageSize, sortBy, sortOrder, ...筛选 }]），因此失效只需传入
 * 不带参数的 key 前缀，即可命中该域下全部参数组合的查询。
 */
import type { QueryClient } from "@tanstack/vue-query";

export const queryKeys = {
  /** 资源台账：网段区域 / 网段 / 房间 / 机柜 / 设备 / 连线 / IP 明细 */
  networkRegions: ["network-regions"],
  networks: ["networks"],
  rooms: ["rooms"],
  cabinets: ["cabinets"],
  devices: ["devices"],
  cableLinks: ["cable-links"],
  ip: ["ip"],
  /** 组织树：组织页与房间/工位/机柜表单的组织选择器共用 */
  organizationsTree: ["organizations-tree"],
  /** 用户管理、主机监控、站内通知列表 */
  users: ["users"],
  agents: ["agents"],
  notifications: ["notifications"],
  /** 日志页的审计/登录列表（与通知列表同页不同 tab，各自独立） */
  logsOperation: ["logs-operation"],
  logsLogin: ["logs-login"],
  /** SNMP 事件弹窗：事件分页列表 + 其设备筛选下拉 */
  snmpEvents: ["snmp-events"],
  snmpEventDevices: ["snmp-events-devices"],
  /** 设备网口/IP 明细（参数为设备 id） */
  deviceNics: (deviceId: string) => ["device-nics", deviceId],
} as const;

/**
 * 设备写操作的连带失效面：一次设备保存/删除会改写网口与 interface_ips、
 * 删除被移除网口的 cable_links、并改变房间与机柜的「已分配功率」，
 * 而这些页签以 v-show 常驻挂载（不会因切回而重新取数），不主动失效
 * 就会长期显示陈旧数据。
 */
const DEVICE_WRITE_KEYS = [
  queryKeys.devices,
  queryKeys.ip,
  queryKeys.cableLinks,
  queryKeys.rooms,
  queryKeys.cabinets,
];

/**
 * 失效设备写操作影响的全部查询。
 *
 * `deviceId` 用于端口编辑/同步场景：除台账外还需刷新该设备的网口明细。
 */
export function invalidateDeviceWrite(queryClient: QueryClient, deviceId?: string): void {
  for (const key of DEVICE_WRITE_KEYS) {
    queryClient.invalidateQueries({ queryKey: key });
  }
  if (deviceId) {
    queryClient.invalidateQueries({ queryKey: queryKeys.deviceNics(deviceId) });
  }
}
