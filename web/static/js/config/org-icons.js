/**
 * 组织节点 SVG 图标库
 *
 * 图标按常见组织层级分组，与类型语义一一对应：
 *   业务组织层级：集团 → 事业部/板块 → 子公司/分公司 → 中心/大部门
 *                → 部门 → 科室/小组 → 岗位/人员
 *   物理地点层级：区域(大区) → 城市 → 园区/基地 → 楼宇 → 楼层 → 区域分区
 *   功能空间：大厅 / 前台 / 办公室 / 机房 / 弱电井 / 工位 / 机柜 / 机位
 *
 * 模板 icons JSONB 中存储图标 key（如 "group"）；历史数据中的 emoji 值
 * 由 renderOrgIcon 兼容渲染为文本。
 */

/** 统一 SVG 外壳（stroke 风格，颜色继承 currentColor） */
function svgWrap(paths) {
  return `<svg viewBox="0 0 24 24" width="1em" height="1em" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;
}

/** 图标 key → SVG 定义 */
export const ORG_ICONS = {
  // ---- 业务组织层级 ----
  // 集团：双子楼
  group: svgWrap(
    '<path d="M3 21V9l5-3v15"/><path d="M8 21V4l8 4v13"/><path d="M16 21V11l5 3v7"/><path d="M2 21h20"/>'
  ),
  // 事业部/板块：四象限板块
  division: svgWrap(
    '<rect x="3" y="3" width="7.5" height="7.5" rx="1"/><rect x="13.5" y="3" width="7.5" height="7.5" rx="1"/><rect x="3" y="13.5" width="7.5" height="7.5" rx="1"/><rect x="13.5" y="13.5" width="7.5" height="7.5" rx="1"/>'
  ),
  // 子公司/分公司：带门的独栋
  subsidiary: svgWrap(
    '<rect x="4" y="3" width="16" height="18" rx="1"/><path d="M10 21v-5h4v5"/><path d="M8 7h.01M12 7h.01M16 7h.01M8 11h.01M12 11h.01M16 11h.01"/>'
  ),
  // 中心/大部门：靶心
  center: svgWrap('<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4.5"/><circle cx="12" cy="12" r="1"/>'),
  // 部门：组织架构图（一上二下）
  department: svgWrap(
    '<rect x="9" y="2.5" width="6" height="5" rx="1"/><rect x="2.5" y="16.5" width="6" height="5" rx="1"/><rect x="15.5" y="16.5" width="6" height="5" rx="1"/><path d="M12 7.5v4.5M5.5 16.5V12h13v4.5"/>'
  ),
  // 科室/小组：多人
  team: svgWrap(
    '<path d="M15 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"/><circle cx="8.5" cy="7" r="4"/><path d="M22 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>'
  ),
  // 岗位/人员：单人
  person: svgWrap('<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>'),

  // ---- 物理地点层级 ----
  // 区域（大区）：地图
  region: svgWrap('<path d="M9 4 3 6v14l6-2 6 2 6-2V4l-6 2z"/><path d="M9 4v14M15 6v14"/>'),
  // 城市：城市天际线
  city: svgWrap(
    '<path d="M2 21h20"/><path d="M4 21V10h5v11"/><path d="M9 21V3h6v18"/><path d="M15 21v-8h5v8"/><path d="M6.5 13h.01M6.5 17h.01M12 7h.01M12 11h.01M12 15h.01M17.5 16h.01"/>'
  ),
  // 园区/基地：厂房
  campus: svgWrap(
    '<path d="M2 20a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8l-7 5V8l-7 5V4a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2Z"/><path d="M17 18h1M12 18h1M7 18h1"/>'
  ),
  // 楼宇：高层建筑
  building: svgWrap(
    '<rect x="5" y="2" width="14" height="20" rx="1"/><path d="M9 22v-4h6v4"/><path d="M9 6h.01M15 6h.01M12 6h.01M9 10h.01M15 10h.01M12 10h.01M9 14h.01M15 14h.01M12 14h.01"/>'
  ),
  // 楼层：楼层分隔线
  floor: svgWrap('<rect x="4" y="2" width="16" height="20" rx="1"/><path d="M4 8.5h16M4 15h16M12 2v20"/>'),
  // 区域分区：虚线分区
  zone: svgWrap('<rect x="3" y="3" width="18" height="18" rx="1"/><path d="M12 3v18" stroke-dasharray="2.5 2.5"/><path d="M3 12h18" stroke-dasharray="2.5 2.5"/>'),

  // ---- 功能空间 ----
  // 大厅：廊柱大厅
  hall: svgWrap('<path d="M3 21h18"/><path d="M5 21V10M9.5 21V10M14.5 21V10M19 21V10"/><path d="M2.5 10 12 3l9.5 7"/>'),
  // 前台：服务铃
  reception: svgWrap('<path d="M3 19h18"/><path d="M5 19a7 7 0 0 1 14 0"/><path d="M12 12V9.5"/><circle cx="12" cy="8.5" r="1"/>'),
  // 办公室：公文包
  office: svgWrap(
    '<rect x="2" y="7" width="20" height="14" rx="2"/><path d="M16 7V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v2"/><path d="M2 13h20"/>'
  ),
  // 机房：服务器
  data_center: svgWrap(
    '<rect x="2" y="3" width="20" height="7" rx="1.5"/><rect x="2" y="14" width="20" height="7" rx="1.5"/><path d="M6 6.5h.01M6 17.5h.01"/><path d="M10.5 6.5H17M10.5 17.5H17"/>'
  ),
  // 弱电井：无线信号
  telecom_closet: svgWrap(
    '<path d="M4.9 19.1C1 15.2 1 8.8 4.9 4.9"/><path d="M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5"/><circle cx="12" cy="12" r="2"/><path d="M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5"/><path d="M19.1 4.9C23 8.8 23 15.2 19.1 19.1"/>'
  ),
  // 工位：显示器
  workstation: svgWrap('<rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/>'),
  // 机柜：机柜格位
  cabinet: svgWrap('<rect x="5" y="2" width="14" height="20" rx="1"/><path d="M9 6.5h6M9 10.5h6M9 14.5h6M9 18.5h6"/>'),
  // 机位：包裹（机柜中的设备位）
  cabinet_position: svgWrap('<path d="M21 8 12 3 3 8v8l9 5 9-5z"/><path d="M3 8l9 5 9-5"/><path d="M12 13v8"/>'),

  // ---- 通用默认 ----
  // 通用组织节点：文件夹
  org: svgWrap('<path d="M4 4h5l2 3h9a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z"/>')
};

/**
 * 图标分组（选择面板展示）。
 * groupKey 为分组标题 i18n 键，icons 为 [{ key, labelKey }]。
 */
export const ORG_ICON_GROUPS = [
  {
    groupKey: "org_template.icon_group_business",
    icons: [
      { key: "group", labelKey: "org_template.icon_corp" },
      { key: "division", labelKey: "org_template.icon_division" },
      { key: "subsidiary", labelKey: "org_template.icon_subsidiary" },
      { key: "center", labelKey: "org_template.icon_center" },
      { key: "department", labelKey: "org_template.icon_department" },
      { key: "team", labelKey: "org_template.icon_team" },
      { key: "person", labelKey: "org_template.icon_person" }
    ]
  },
  {
    groupKey: "org_template.icon_group_location",
    icons: [
      { key: "region", labelKey: "org_template.icon_region" },
      { key: "city", labelKey: "org_template.icon_city" },
      { key: "campus", labelKey: "org_template.icon_campus" },
      { key: "building", labelKey: "org_template.icon_building" },
      { key: "floor", labelKey: "org_template.icon_floor" },
      { key: "zone", labelKey: "org_template.icon_zone" }
    ]
  },
  {
    groupKey: "org_template.icon_group_space",
    icons: [
      { key: "hall", labelKey: "org_template.icon_hall" },
      { key: "reception", labelKey: "org_template.icon_reception" },
      { key: "office", labelKey: "org_template.icon_office" },
      { key: "data_center", labelKey: "org_template.icon_data_center" },
      { key: "telecom_closet", labelKey: "org_template.icon_telecom_closet" },
      { key: "workstation", labelKey: "org_template.icon_workstation" },
      { key: "cabinet", labelKey: "org_template.icon_cabinet" },
      { key: "cabinet_position", labelKey: "org_template.icon_cabinet_position" },
      { key: "org", labelKey: "org_template.icon_org" }
    ]
  }
];

/** 默认图标 key */
export const DEFAULT_ORG_ICON = "org";

/**
 * 渲染图标值为 HTML。
 * 已知 key 渲染 SVG；历史 emoji 值原样文本输出；空值用默认图标。
 * @param {string} value 图标值（key 或历史 emoji）
 * @returns {string} HTML 片段
 */
export function renderOrgIcon(value) {
  if (value && ORG_ICONS[value]) {
    return ORG_ICONS[value];
  }
  if (value) {
    return `<span class="org-icon-legacy">${value}</span>`;
  }
  return ORG_ICONS[DEFAULT_ORG_ICON];
}
