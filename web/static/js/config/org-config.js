/**
 * 组织类型配置文件
 * 从后端API动态获取组织类型配置，不再硬编码
 */

import { apiGet } from "../utils/apiClient.js";

// 可用图标列表（用于模板编辑器）
export const AVAILABLE_ICONS = [
  "🏢",
  "🏬",
  "🏠",
  "🏗️",
  "🏫",
  "🏭",
  "🏛️",
  "⛪",
  "📐",
  "🔬",
  "💡",
  "🖥️",
  "💻",
  "📋",
  "🗂️",
  "🗄️",
  "🚪",
  "🛗",
  "🪜",
  "📶",
  "🌐",
  "📡",
  "🔌",
  "🔒",
  "💺",
  "📦",
  "🧯",
  "🚰",
  "⚡"
];

// 默认图标映射（用于未知类型）
export const DEFAULT_ICON_MAPPING = {
  楼: "🏬",
  building: "🏬",
  层: "📐",
  floor: "📐",
  厅: "🚪",
  hall: "🚪",
  办公: "🏠",
  office: "🏠",
  机房: "🖥️",
  数据中心: "🖥️",
  data_center: "🖥️",
  弱电: "📡",
  电信: "📡",
  telecom_closet: "📡",
  工位: "💺",
  workstation: "💺",
  机柜: "🗄️",
  cabinet: "🗄️",
  机位: "📦",
  cabinet_position: "📦"
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
 * 获取组织类型的图标
 * @param {string} orgType - 组织类型
 * @param {Object} templateIconsMap - 模板图标映射（可选）
 * @returns {string} 图标
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

  return "📁"; // 默认图标
}

/**
 * 根据关键词匹配默认图标
 * @param {string} orgType - 组织类型
 * @returns {string} 图标
 */
function matchDefaultIcon(orgType) {
  for (const [keyword, icon] of Object.entries(DEFAULT_ICON_MAPPING)) {
    if (orgType.includes(keyword) || orgType === keyword) {
      return icon;
    }
  }
  return "📁";
}

/**
 * 清除缓存（用于刷新数据）
 */
export function clearOrgTypesCache() {
  orgTypesCache = null;
  orgIconsCache = {};
}
