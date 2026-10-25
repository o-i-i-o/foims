/**
 * 可视化页的组织/房间下拉数据加工（纯函数，无 DOM 依赖）。
 *
 * 旧实现 `utils/legacyResources.js` 直接创建 `<option>` 并注入目标 select，
 * 需要等待元素出现、维护级联加载序号。Vue 化后下拉由模板 `v-for` 渲染，
 * 本模块只负责把接口载荷加工成「选项数组」，加载时序交给响应式状态。
 */
import type { OrganizationTreeNode } from "../../types/api/generated/OrganizationTreeNode";
import type { RoomQuery } from "../../api/visualization";

/**
 * 房间类型分类。
 *
 * 办公类（工位管理范围）：办公室、大厅、前台；机房类（机柜管理范围）：
 * 机房、弱电井。"其他"无固定性质，两类筛选均包含。
 */
export const OFFICE_ROOM_TYPE_LIST = "office,lobby,reception,other";
export const DATA_CENTER_ROOM_TYPE_LIST = "data_center,telecom_closet,other";

/** 房间类别：两个分类别名 + 任意具体类型（按字面过滤） */
export type RoomCategory = "office" | "datacenter" | string;

/** 类别别名展开为后端的逗号分隔类型列表 */
export function resolveRoomTypes(category: RoomCategory): string {
  if (category === "office") {
    return OFFICE_ROOM_TYPE_LIST;
  }
  if (category === "datacenter") {
    return DATA_CENTER_ROOM_TYPE_LIST;
  }
  return category;
}

/** 构造房间下拉的请求参数（分类别名在此展开，请求层只认逗号列表） */
export function roomsQuery(category: RoomCategory, orgId: string | null): RoomQuery {
  return { orgId, roomTypes: resolveRoomTypes(category) };
}

// ==================== 组织树 ====================

/** 展平后的组织项（带缩进层级） */
export interface FlatOrganization {
  id: string;
  name: string;
  org_type: string;
  /** 根节点为 0 */
  depth: number;
}

/** 组织树按前序展平为带缩进深度的列表（下拉展示用） */
export function flattenOrgTree(nodes: OrganizationTreeNode[], depth = 0): FlatOrganization[] {
  const result: FlatOrganization[] = [];
  for (const node of nodes) {
    result.push({ id: node.id, name: node.name, org_type: node.org_type, depth });
    if (node.children.length > 0) {
      result.push(...flattenOrgTree(node.children, depth + 1));
    }
  }
  return result;
}

/**
 * 组织选项的展示文本：按层级用不换行空格缩进，后缀组织类型。
 * 用 `\u00A0` 而非普通空格，避免 `<option>` 内连续空格被折叠。
 */
export function organizationOptionLabel(org: FlatOrganization): string {
  return `${"\u00A0\u00A0\u00A0\u00A0".repeat(org.depth)}${org.name} (${org.org_type})`;
}

/**
 * 组织及其全部后代的 id 集合（拓扑按组织筛选）。
 * 树中未命中时退化为自身，保证筛选条件至少包含所选组织。
 */
export function collectOrgSubtreeIds(tree: OrganizationTreeNode[], orgId: string): Set<string> {
  const ids = new Set<string>();
  const walk = (nodes: OrganizationTreeNode[], inside: boolean): void => {
    for (const node of nodes) {
      const hit = inside || node.id === orgId;
      if (hit) {
        ids.add(node.id);
      }
      if (node.children.length > 0) {
        walk(node.children, hit);
      }
    }
  };
  walk(tree, false);
  if (ids.size === 0) {
    ids.add(orgId);
  }
  return ids;
}
