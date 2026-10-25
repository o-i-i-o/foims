/**
 * 视图模型组装的可执行自检（P1 安全网）。
 *
 * 运行：`node scripts/visualization-viewmodel.selfcheck.ts`
 * （Node ≥ 22.6 原生擦除类型；脚本在 tsconfig include 之外，不参与 vue-tsc）。
 *
 * 覆盖：布局索引、门元素归属、工位坐标回退、IP 归并优先级、机柜视图、
 * 拓扑分组与组织筛选。
 */
import {
  buildCabinetCanvas,
  buildDoorNode,
  buildTopologyCanvasNodes,
  buildWorkstationCanvas,
  contentViewBox,
  filterNodesByOrg,
  groupTopologyNodes,
  indexLayoutItems,
  mapIpDetailsByWorkstation,
  savedPosition,
} from "../src/utils/visualization/viewModel.ts";
import { DOOR_RECT, fallbackWorkstationRect } from "../src/utils/visualization/geometry.ts";
import type { IpDetailWithNames } from "../src/types/api/generated/IpDetailWithNames.ts";
import type { LayoutItem } from "../src/types/api/generated/LayoutItem.ts";
import type { TopologyNodeWithDevice } from "../src/types/api/generated/TopologyNodeWithDevice.ts";
import type { WorkstationWithDetails } from "../src/types/api/generated/WorkstationWithDetails.ts";
import type { RoomCabinet } from "../src/types/visualization.ts";

let failures = 0;

function check(name: string, fn: () => void): void {
  try {
    fn();
    console.log(`  ok  ${name}`);
  } catch (error) {
    failures += 1;
    console.error(`FAIL  ${name}`);
    console.error(`      ${error instanceof Error ? error.message : String(error)}`);
  }
}

function equal<T>(actual: T, expected: T, what = "值"): void {
  if (actual !== expected) {
    throw new Error(`${what}不符：期望 ${String(expected)}，实际 ${String(actual)}`);
  }
}

function truthy(value: unknown, what = "条件"): void {
  if (!value) {
    throw new Error(`${what}应为真`);
  }
}

function layoutItem(id: string, elementType: string, x: number, y: number, width = 160, height = 160): LayoutItem {
  return { id, element_type: elementType, position: { x, y, width, height, rotation: 0 } };
}

function workstation(id: string): WorkstationWithDetails {
  return {
    id,
    name: `工位-${id}`,
    room_id: "room-1",
    room_name: "机房",
    manager_name: "张三",
    manager_employee_id: "emp-1",
    ips: [],
    description: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
  };
}

function ipDetail(workstationId: string, status: string, address: string): IpDetailWithNames {
  return {
    id: `ip-${address}`,
    device_interface_id: "if-1",
    device_id: "dev-1",
    device_type: null,
    device_name: null,
    subnet_id: "sub-1",
    workstation_name: null,
    cabinet_position_name: null,
    interface_name: null,
    physical_type: null,
    interface_role: null,
    room_name: null,
    cabinet_name: null,
    org_name: null,
    network_name: "net",
    network_region: "office",
    ip_address: address,
    ip_version: 4,
    mac_address: null,
    hostname: null,
    description: null,
    status,
    last_seen: "2026-01-01T00:00:00Z",
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    position_id: null,
    workstation_id: workstationId,
  };
}

function topologyNode(overrides: Partial<TopologyNodeWithDevice>): TopologyNodeWithDevice {
  return {
    id: "node-1",
    device_id: "dev-1",
    x: 100,
    y: 100,
    width: 200,
    height: 100,
    device_name: "交换机",
    device_type: "switch",
    brand: null,
    model: null,
    seller: null,
    snmp_version: null,
    snmp_port: null,
    location: null,
    workstation_name: null,
    room_id: "room-1",
    room_name: "机房",
    org_id: "org-1",
    org_name: "总部",
    cabinet_id: null,
    cabinet_name: null,
    ip_address: null,
    ...overrides,
  };
}

// ==================== 布局索引 ====================

check("indexLayoutItems 键统一小写并跳过空 id", () => {
  const map = indexLayoutItems([layoutItem("AbC", "workstation", 0, 0), { ...layoutItem("", "workstation", 0, 0) }]);
  equal(map.size, 1);
  truthy(map.has("abc"));
});

check("savedPosition 拒绝负坐标与零宽高", () => {
  equal(savedPosition(undefined), null);
  equal(savedPosition(layoutItem("a", "workstation", -10, 0)), null);
  truthy(savedPosition(layoutItem("a", "workstation", 10, 20)));
});

// ==================== 工位画布 ====================

check("buildDoorNode 未保存时回退默认矩形并生成临时 id", () => {
  const door = buildDoorNode([]);
  equal(door.rect.x, DOOR_RECT.x);
  equal(door.rect.y, DOOR_RECT.y);
  truthy(door.id.length > 0);
});

check("buildDoorNode 沿用已保存主键与坐标", () => {
  const door = buildDoorNode([layoutItem("door-1", "door", 80, 120, 40, 80)]);
  equal(door.id, "door-1");
  equal(door.rect.x, 80);
});

check("buildWorkstationCanvas 无布局项时按 4 列网格回退", () => {
  const canvas = buildWorkstationCanvas({
    workstations: [workstation("w1"), workstation("w2")],
    layout: [layoutItem("door-1", "door", 50, 100, 40, 80)],
    ipDetails: [],
  });
  equal(canvas.nodes[0]?.rect.x, fallbackWorkstationRect(0).x);
  equal(canvas.nodes[1]?.rect.x, fallbackWorkstationRect(1).x);
  // 门元素沿用已保存坐标（无已保存门时由 buildDoorNode 回退，另有用例覆盖）
  equal(canvas.door.id, "door-1");
});

check("buildWorkstationCanvas 已保存坐标优先且非法坐标回退", () => {
  const canvas = buildWorkstationCanvas({
    workstations: [workstation("w1"), workstation("w2"), workstation("w3")],
    layout: [
      layoutItem("door-1", "door", 50, 100, 40, 80),
      layoutItem("W1", "workstation", 400, 300),
      layoutItem("w2", "workstation", -50, 300),
    ],
    ipDetails: [],
  });
  equal(canvas.nodes[0]?.rect.x, 400);
  equal(canvas.nodes[1]?.rect.x, fallbackWorkstationRect(1).x);
  equal(canvas.nodes[2]?.rect.x, fallbackWorkstationRect(2).x);
});

check("mapIpDetailsByWorkstation 同类多行时 active 优先且忽略无归属行", () => {
  const map = mapIpDetailsByWorkstation([
    ipDetail("w1", "inactive", "10.0.0.1"),
    ipDetail("w1", "active", "10.0.0.2"),
  ]);
  equal(map.get("w1")?.ip_address, "10.0.0.2");
  equal(map.size, 1);
});

check("contentViewBox 覆盖内容并抬高到最小视野", () => {
  const box = contentViewBox([{ x: 0, y: 0, width: 1500, height: 900 }], { width: 0, height: 0 });
  equal(box.width, 1550);
  equal(box.height, 950);
});

// ==================== 机柜画布 ====================

function roomCabinet(id: string, capacity: number, positions: RoomCabinet["positions"]): RoomCabinet {
  return {
    id,
    name: `柜-${id}`,
    room_id: "room-1",
    capacity,
    description: null,
    positions,
    layout: null,
  };
}

check("buildCabinetCanvas 按 U 位推导机位矩形并保留已保存 x", () => {
  const canvas = buildCabinetCanvas({
    cabinets: [
      roomCabinet("c1", 42, [
        { id: "p1", name: "机位1", cabinet_id: "c1", start_u: 1, end_u: 2, description: null },
      ]),
      roomCabinet("c2", 42, []),
    ],
    layout: [layoutItem("C1", "network_device", 300, 0, 150, 42 * 19 + 40)],
    containerHeight: 900,
  });
  equal(canvas.nodes[0]?.uHeight, 19);
  // 已保存 x=300，c2 未保存按默认排布 216；整体左对齐后 c2 贴左缘、c1 相对偏移 84
  equal(canvas.nodes[1]?.rect.x, 16);
  equal(canvas.nodes[0]?.rect.x, 100);
  equal(canvas.nodes[0]?.positions[0]?.rect.y, canvas.nodes[0]?.rect.y + 40 + (42 - 2) * 19);
  // 画布总宽覆盖所有机柜右缘（含右侧边距）
  for (const node of canvas.nodes) {
    truthy(node.rect.x + node.rect.width <= canvas.totalWidth);
  }
});

// ==================== 拓扑画布 ====================

check("buildTopologyCanvasNodes 异常坐标回退网格原点", () => {
  const nodes = buildTopologyCanvasNodes([
    topologyNode({ id: "n1" }),
    topologyNode({ id: "n2", x: Number.NaN }),
  ]);
  equal(nodes[0]?.rect.x, 100);
  equal(nodes[1]?.rect.x, 100);
  equal(nodes[1]?.rect.width, 200);
});

check("groupTopologyNodes 房间与机柜两级分组、保持首次出现顺序", () => {
  const nodes = buildTopologyCanvasNodes([
    topologyNode({ id: "n1", device_id: "d1", cabinet_id: "cab-1", cabinet_name: "A柜" }),
    topologyNode({ id: "n2", device_id: "d2" }),
    topologyNode({ id: "n3", device_id: "d3", room_id: null, room_name: null }),
    topologyNode({ id: "n4", device_id: "d4", cabinet_id: null, cabinet_name: "B柜" }),
  ]);
  const groups = groupTopologyNodes(nodes, { noRoom: "未分房间", noCabinet: "未分机柜" });
  equal(groups.length, 2);
  equal(groups[0]?.key, "room:room-1");
  equal(groups[0]?.cabinets.length, 2);
  equal(groups[0]?.cabinets[0]?.key, "cab:cab-1");
  equal(groups[0]?.cabinets[1]?.key, "cab:name:B柜");
  equal(groups[0]?.loose.length, 1);
  equal(groups[1]?.label, "未分房间");
  equal(groups[1]?.orgLabel, "总部");
});

check("filterNodesByOrg 为 null 时全量，否则按组织集合过滤", () => {
  const nodes = buildTopologyCanvasNodes([
    topologyNode({ id: "n1", org_id: "org-1" }),
    topologyNode({ id: "n2", org_id: "org-2" }),
    topologyNode({ id: "n3", org_id: null }),
  ]);
  equal(filterNodesByOrg(nodes, null).length, 3);
  equal(filterNodesByOrg(nodes, new Set(["org-1"])).length, 1);
  equal(filterNodesByOrg(nodes, new Set()).length, 0);
});

if (failures > 0) {
  throw new Error(`视图模型自检失败 ${failures} 项`);
}
console.log("视图模型自检全部通过");
