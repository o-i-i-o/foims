/**
 * 组织节点图标与类型配置（图标分组与类型语义的单一来源）。
 *
 * 图标按常见组织层级分组，与类型语义一一对应：
 *   业务组织层级：集团 → 事业部/板块 → 子公司/分公司 → 中心/大部门
 *                → 部门 → 科室/小组 → 岗位/人员
 *   物理地点层级：区域(大区) → 城市 → 园区/基地 → 楼宇 → 楼层 → 区域分区
 *   功能空间：大厅 / 前台 / 办公室 / 机房 / 弱电井 / 工位 / 机柜 / 机位
 *
 * 模板 icons JSONB 存储图标 key（如 "group"），历史数据中的 emoji 值由
 * OrgIcon 组件以纯文本渲染。形状以结构化对象描述（避免 v-html），由
 * OrgIcon 组件渲染；每个 key 经 ORG_ICON_COLORS 注入语义配色。
 */
import { i18n } from "../i18n/index";

/** 单个 SVG 形状：tag 选择元素，attrs 为 SVG 属性透传（由 OrgIcon 组件渲染） */
export interface OrgIconShape {
  tag: string;
  attrs: Record<string, string>;
}

const path = (d: string, attrs: Record<string, string> = {}): OrgIconShape => ({
  tag: "path",
  attrs: { d, ...attrs }
});
const rect = (attrs: Record<string, string>): OrgIconShape => ({ tag: "rect", attrs });
const circle = (attrs: Record<string, string>): OrgIconShape => ({ tag: "circle", attrs });

/** 图标 key → SVG 形状定义（key 为运行时数据，按宽化 Record 消费） */
export const ORG_ICONS: Record<string, OrgIconShape[]> = {
  // 业务组织层级
  // 集团：双子楼
  group: [
    path("M3 21V9l5-3v15"),
    path("M8 21V4l8 4v13"),
    path("M16 21V11l5 3v7"),
    path("M2 21h20")
  ],
  // 事业部/板块：四象限板块
  division: [
    rect({ x: "3", y: "3", width: "7.5", height: "7.5", rx: "1" }),
    rect({ x: "13.5", y: "3", width: "7.5", height: "7.5", rx: "1" }),
    rect({ x: "3", y: "13.5", width: "7.5", height: "7.5", rx: "1" }),
    rect({ x: "13.5", y: "13.5", width: "7.5", height: "7.5", rx: "1" })
  ],
  // 子公司/分公司：带门的独栋
  subsidiary: [
    rect({ x: "4", y: "3", width: "16", height: "18", rx: "1" }),
    path("M10 21v-5h4v5"),
    path("M8 7h.01M12 7h.01M16 7h.01M8 11h.01M12 11h.01M16 11h.01")
  ],
  // 中心/大部门：靶心
  center: [
    circle({ cx: "12", cy: "12", r: "9" }),
    circle({ cx: "12", cy: "12", r: "4.5" }),
    circle({ cx: "12", cy: "12", r: "1" })
  ],
  // 部门：组织架构图（一上二下）
  department: [
    rect({ x: "9", y: "2.5", width: "6", height: "5", rx: "1" }),
    rect({ x: "2.5", y: "16.5", width: "6", height: "5", rx: "1" }),
    rect({ x: "15.5", y: "16.5", width: "6", height: "5", rx: "1" }),
    path("M12 7.5v4.5M5.5 16.5V12h13v4.5")
  ],
  // 科室/小组：多人
  team: [
    path("M15 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2"),
    circle({ cx: "8.5", cy: "7", r: "4" }),
    path("M22 21v-2a4 4 0 0 0-3-3.87"),
    path("M16 3.13a4 4 0 0 1 0 7.75")
  ],
  // 岗位/人员：单人
  person: [
    path("M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"),
    circle({ cx: "12", cy: "7", r: "4" })
  ],

  // 物理地点层级
  // 区域（大区）：地图
  region: [path("M9 4 3 6v14l6-2 6 2 6-2V4l-6 2z"), path("M9 4v14M15 6v14")],
  // 城市：城市天际线
  city: [
    path("M2 21h20"),
    path("M4 21V10h5v11"),
    path("M9 21V3h6v18"),
    path("M15 21v-8h5v8"),
    path("M6.5 13h.01M6.5 17h.01M12 7h.01M12 11h.01M12 15h.01M17.5 16h.01")
  ],
  // 园区/基地：厂房
  campus: [
    path("M2 20a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V8l-7 5V8l-7 5V4a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2Z"),
    path("M17 18h1M12 18h1M7 18h1")
  ],
  // 楼宇：高层建筑
  building: [
    rect({ x: "5", y: "2", width: "14", height: "20", rx: "1" }),
    path("M9 22v-4h6v4"),
    path("M9 6h.01M15 6h.01M12 6h.01M9 10h.01M15 10h.01M12 10h.01M9 14h.01M15 14h.01M12 14h.01")
  ],
  // 楼层：楼层分隔线
  floor: [
    rect({ x: "4", y: "2", width: "16", height: "20", rx: "1" }),
    path("M4 8.5h16M4 15h16M12 2v20")
  ],
  // 区域分区：虚线分区
  zone: [
    rect({ x: "3", y: "3", width: "18", height: "18", rx: "1" }),
    path("M12 3v18", { "stroke-dasharray": "2.5 2.5" }),
    path("M3 12h18", { "stroke-dasharray": "2.5 2.5" })
  ],

  // 功能空间
  // 大厅：廊柱大厅
  hall: [
    path("M3 21h18"),
    path("M5 21V10M9.5 21V10M14.5 21V10M19 21V10"),
    path("M2.5 10 12 3l9.5 7")
  ],
  // 前台：服务铃
  reception: [
    path("M3 19h18"),
    path("M5 19a7 7 0 0 1 14 0"),
    path("M12 12V9.5"),
    circle({ cx: "12", cy: "8.5", r: "1" })
  ],
  // 办公室：公文包
  office: [
    rect({ x: "2", y: "7", width: "20", height: "14", rx: "2" }),
    path("M16 7V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v2"),
    path("M2 13h20")
  ],
  // 机房：服务器
  data_center: [
    rect({ x: "2", y: "3", width: "20", height: "7", rx: "1.5" }),
    rect({ x: "2", y: "14", width: "20", height: "7", rx: "1.5" }),
    path("M6 6.5h.01M6 17.5h.01"),
    path("M10.5 6.5H17M10.5 17.5H17")
  ],
  // 弱电井：无线信号
  telecom_closet: [
    path("M4.9 19.1C1 15.2 1 8.8 4.9 4.9"),
    path("M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5"),
    circle({ cx: "12", cy: "12", r: "2" }),
    path("M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5"),
    path("M19.1 4.9C23 8.8 23 15.2 19.1 19.1")
  ],
  // 工位：显示器
  workstation: [
    rect({ x: "2", y: "3", width: "20", height: "14", rx: "2" }),
    path("M8 21h8M12 17v4")
  ],
  // 机柜：机柜格位
  cabinet: [
    rect({ x: "5", y: "2", width: "14", height: "20", rx: "1" }),
    path("M9 6.5h6M9 10.5h6M9 14.5h6M9 18.5h6")
  ],
  // 机位：包裹（机柜中的设备位）
  cabinet_position: [
    path("M21 8 12 3 3 8v8l9 5 9-5z"),
    path("M3 8l9 5 9-5"),
    path("M12 13v8")
  ],

  // 通用默认
  // 通用组织节点：文件夹
  org: [path("M4 4h5l2 3h9a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z")]
};

/**
 * 图标 key → 语义配色（引用主题 CSS 变量）。
 * 与类型徽章（org-node-type-badge[data-type]）的配色体系对齐：
 * 业务组织层级以主色为主（科室/小组、岗位/人员用成功色），物理地点蓝/青色系，功能空间按既有徽章语义色。
 */
export const ORG_ICON_COLORS: Record<string, string> = {
  // 业务组织层级
  group: "var(--color-primary)",
  division: "var(--color-primary)",
  subsidiary: "var(--color-primary)",
  center: "var(--color-primary)",
  department: "var(--color-primary)",
  team: "var(--color-success)",
  person: "var(--color-success)",
  // 物理地点层级
  region: "var(--color-blue)",
  city: "var(--color-blue)",
  campus: "var(--color-success-alt)",
  building: "var(--color-blue)",
  floor: "var(--color-info)",
  zone: "var(--color-info)",
  // 功能空间
  hall: "var(--color-warning-text)",
  reception: "var(--color-warning-text)",
  office: "var(--color-success)",
  data_center: "var(--color-danger)",
  telecom_closet: "var(--color-ipv6)",
  workstation: "var(--color-secondary)",
  cabinet: "var(--color-ipv6)",
  cabinet_position: "var(--color-orange)",
  // 通用默认：中性灰
  org: "var(--color-text-muted)"
};

/** 图标分组选项 */
export interface OrgIconGroup {
  /** 分组标题 i18n 键 */
  groupKey: string;
  icons: { key: string; labelKey: string }[];
}

/**
 * 图标分组（选择面板展示）。
 * groupKey 为分组标题 i18n 键，icons 为 [{ key, labelKey }]。
 */
export const ORG_ICON_GROUPS: OrgIconGroup[] = [
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

/** 默认图标映射（类型关键词 → 图标 key，用于未设置图标的类型） */
export const DEFAULT_ICON_MAPPING: Record<string, string> = {
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

/**
 * 解析组织类型的图标值（优先级：模板 icons → API icons → 关键词匹配 → 默认）。
 * 同步纯函数：模板 icons 与 API icons 由调用方以查询数据传入。
 */
export function resolveOrgIcon(
  orgType: unknown,
  templateIcons: Record<string, string> = {},
  apiIcons: Record<string, string> = {}
): string {
  const type = String(orgType ?? "");
  const templateIcon = templateIcons[type];
  if (templateIcon) {
    return templateIcon;
  }
  const apiIcon = apiIcons[type];
  if (apiIcon) {
    return apiIcon;
  }
  for (const [keyword, icon] of Object.entries(DEFAULT_ICON_MAPPING)) {
    if (type.includes(keyword) || type === keyword) {
      return icon;
    }
  }
  return DEFAULT_ORG_ICON;
}

/** 组织类型标签：语言包有对应键则翻译，否则原样回显（key 为用户可编辑自由文本） */
export function getOrgTypeLabel(orgType: unknown): string {
  if (typeof orgType !== "string" || !orgType) {
    // 非字符串（历史数据）原样回显，空值回退空串
    return orgType == null ? "" : (orgType as string);
  }
  const key = `organization.types.${orgType}`;
  return i18n.global.te(key) ? i18n.global.t(key) : orgType;
}

/** 从 levels 映射推断根类型（未被任何子级引用的 key），唯一根时返回，否则返回 null */
export function findRootType(levels: unknown): string | null {
  if (!levels || typeof levels !== "object" || Array.isArray(levels)) {
    return null;
  }
  // 已校验为非数组对象，此处按映射形状消费
  const levelMap = levels as Record<string, unknown>;
  const allChildren = new Set<string>();
  Object.values(levelMap).forEach((children) => {
    if (Array.isArray(children)) {
      children.forEach((child) => {
        allChildren.add(child);
      });
    }
  });
  const roots = Object.keys(levelMap).filter((key) => !allChildren.has(key));
  return roots.length === 1 ? (roots[0] ?? null) : null;
}

/**
 * 将 levels 映射展开为根到叶的路径数组（每条路径形如 "根 → 子 → 孙"），
 * 供模板列表/下拉选项展示；无唯一根时返回空数组。
 */
export function getLevelPaths(levels: unknown): string[] {
  const rootType = findRootType(levels);
  if (!rootType) {
    return [];
  }
  // findRootType 已校验 levels 为非数组对象，此处按映射形状消费
  const levelMap = (levels ?? {}) as Record<string, string[]>;
  const paths: string[][] = [];
  function findPaths(type: string, currentPath: string[]): void {
    const children = levelMap[type] || [];
    if (children.length === 0) {
      paths.push([...currentPath, type]);
    } else {
      children.forEach((child) => {
        findPaths(child, [...currentPath, type]);
      });
    }
  }
  findPaths(rootType, []);
  return paths.map((pathParts) => pathParts.join(" → "));
}
