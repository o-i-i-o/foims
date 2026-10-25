/**
 * 主应用路由：hash 模式（#dashboard 式深链无需 nginx 改动）。
 * 分区清单即导航数据源（侧边栏按 NAV_ITEMS 顺序渲染），八个分区均已登记
 * Vue 实现组件；分区组件按路由懒加载，首屏只装载壳层与所在分区。
 */
import { createRouter, createWebHashHistory } from "vue-router";
import type { RouteRecordRaw } from "vue-router";
import { hasSession } from "../utils/session";
import { getHiddenSections } from "../utils/roleCapabilities";

/** 路由 meta 声明合并：覆盖本路由表实际用到的字段 */
declare module "vue-router" {
  interface RouteMeta {
    /** 分区标题 i18n 键 */
    title: string;
    /** 分区图标名（对应 config/icons 的 ICONS 键） */
    icon: string;
  }
}

/**
 * 一级分区（数组顺序即侧边栏渲染顺序）：name 同时是路由名、
 * 分区 ID 与 RBAC 能力表（utils/roleCapabilities）的键
 */
export interface NavItem {
  /** 分区 ID（路由名 / RBAC 能力表键） */
  name: SectionName;
  /** 标题 i18n 键 */
  title: string;
  /** 图标名（ICONS 键） */
  icon: string;
}

const SECTION_COMPONENTS = {
  dashboard: () => import("../pages/DashboardPage.vue"),
  organization: () => import("../pages/OrganizationPage.vue"),
  resources: () => import("../pages/ResourcesPage.vue"),
  ip: () => import("../pages/IpPage.vue"),
  visualization: () => import("../pages/VisualizationPage.vue"),
  agents: () => import("../pages/AgentsPage.vue"),
  logs: () => import("../pages/LogsPage.vue"),
  system: () => import("../pages/SystemPage.vue")
} satisfies Record<string, RouteRecordRaw["component"]>;

/** 分区 ID 联合类型：由分区组件映射表的键派生，保证清单与组件一一对应 */
type SectionName = keyof typeof SECTION_COMPONENTS;

export const NAV_ITEMS: NavItem[] = [
  { name: "dashboard", title: "nav.dashboard", icon: "dashboard" },
  { name: "organization", title: "nav.organization", icon: "organization" },
  { name: "resources", title: "nav.resources", icon: "resources" },
  { name: "ip", title: "nav.ip_detail", icon: "ip" },
  { name: "visualization", title: "nav.visualization", icon: "visualization" },
  { name: "agents", title: "nav.agents", icon: "agents" },
  { name: "logs", title: "nav.logs", icon: "logs" },
  { name: "system", title: "nav.system", icon: "system" }
];

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: { name: "dashboard" } },
    ...NAV_ITEMS.map((item) => ({
      path: `/${item.name}`,
      name: item.name,
      component: SECTION_COMPONENTS[item.name],
      meta: { title: item.title, icon: item.icon }
    })),
    // 未知路径（书签残留/手输错值）回落仪表盘
    { path: "/:pathMatch(.*)*", redirect: { name: "dashboard" } }
  ]
});

router.beforeEach((to) => {
  // 无本地会话快照：整页跳登录页（登录页是独立入口，不走本路由表）
  if (!hasSession()) {
    window.location.href = "/index.html";
    return false;
  }

  // 角色无权访问的分区（hash 直达可绕过菜单隐藏）重定向回仪表盘
  if (getHiddenSections().includes(String(to.name))) {
    return { name: "dashboard" };
  }

  return true;
});

export default router;
