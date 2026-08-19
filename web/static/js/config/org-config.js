/**
 * 组织类型配置文件
 * 从后端API动态获取组织类型配置，不再硬编码
 *
 * 图标体系见 org-icons.js：模板 icons JSONB 存储图标 key（如 "group"），
 * 历史 emoji 值由 renderOrgIcon 兼容渲染。
 */

import { apiGet } from "../utils/apiClient.js";
import { DEFAULT_ORG_ICON } from "./org-icons.js";

// 默认图标映射（类型关键词 → 图标 key，用于未设置图标的类型）
export const DEFAULT_ICON_MAPPING = {
  // 业务组织层级
  集团: "group",
  group: "group",
  事业部: "division",
  板块: "division",
  division: "division",
  子公司: "subsidiary",
  分公司: "subsidiary",
  subsidiary: "subsidiary",
  中心: "center",
  center: "center",
  部门: "department",
  department: "department",
  小组: "team",
  科室: "team",
  team: "team",
  岗位: "person",
  人员: "person",
  员工: "person",
  person: "person",
  // 物理地点层级
  大区: "region",
  区域: "region",
  region: "region",
  城市: "city",
  city: "city",
  园区: "campus",
  基地: "campus",
  campus: "campus",
  楼: "building",
  楼宇: "building",
  building: "building",
  层: "floor",
  楼层: "floor",
  floor: "floor",
  分区: "zone",
  zone: "zone",
  // 功能空间
  大厅: "hall",
  厅: "hall",
  hall: "hall",
  前台: "reception",
  reception: "reception",
  办公: "office",
  office: "office",
  机房: "data_center",
  数据中心: "data_center",
  data_center: "data_center",
  弱电: "telecom_closet",
  电信: "telecom_closet",
  telecom_closet: "telecom_closet",
  工位: "workstation",
  workstation: "workstation",
  机柜: "cabinet",
  cabinet: "cabinet",
  机位: "cabinet_position",
  cabinet_position: "cabinet_position"
};

// 缓存组织类型配置
let orgTypesCache = null;
let orgIconsCache = {};

/**
 * 从API加载组织类型配置
 * @returns {Promise<{types: string[], icons: Object}>}
 */
export async function loadOrgTypesFromAPI() {
  if (orgTypesCache !== null) {
    return { types: orgTypesCache, icons: orgIconsCache };
  }

  try {
    const result = await apiGet("/api/resources/org-templates/available-types");
    if (result.success) {
      orgTypesCache = result.data.types || [];
      orgIconsCache = result.data.icons || {};
      return { types: orgTypesCache, icons: orgIconsCache };
    }
  } catch (error) {
    console.warn("Failed to load org types from API:", error);
  }

  // 返回默认值
  return { types: [], icons: {} };
}

/**
 * 获取所有组织类型（从API获取）
 * @returns {Promise<Array<string>>} 组织类型列表
 */
export async function getAllOrgTypes() {
  const { types } = await loadOrgTypesFromAPI();
  return types;
}

/**
 * 获取组织类型的图标值（图标 key 或历史 emoji，渲染用 renderOrgIcon）
 * @param {string} orgType - 组织类型
 * @param {Object} templateIconsMap - 模板图标映射（可选）
 * @returns {string} 图标值
 */
export async function getOrgIcon(orgType, templateIconsMap = {}) {
  // 优先级：模板 icons → API icons → 默认匹配
  if (templateIconsMap[orgType]) {
    return templateIconsMap[orgType];
  }

  // 确保已加载API数据
  const { icons } = await loadOrgTypesFromAPI();
  if (icons[orgType]) {
    return icons[orgType];
  }

  // 使用关键词匹配
  for (const [keyword, icon] of Object.entries(DEFAULT_ICON_MAPPING)) {
    if (orgType.includes(keyword) || orgType === keyword) {
      return icon;
    }
  }

  return DEFAULT_ORG_ICON; // 默认图标
}

/**
 * 清除缓存（用于刷新数据）
 */
export function clearOrgTypesCache() {
  orgTypesCache = null;
  orgIconsCache = {};
}
