/**
 * 主应用路由：hash 模式（与旧版 #dashboard 深链保持一致，无需 nginx 改动）。
 * 分区清单即导航数据源（侧边栏按 NAV_ITEMS 顺序渲染），未迁移分区统一
 * 挂 SectionPlaceholder 占位页，迁移完成时在 SECTION_COMPONENTS 登记
 * 实现组件即可替换。
 */
import { createRouter, createWebHashHistory } from "vue-router";
import { hasSession } from "../utils/session.js";
import { getHiddenSections } from "../utils/roleCapabilities.js";
import SectionPlaceholder from "../pages/SectionPlaceholder.vue";
import LogsPage from "../pages/LogsPage.vue";
import DashboardPage from "../pages/DashboardPage.vue";
import OrganizationPage from "../pages/OrganizationPage.vue";

/**
 * 一级分区（顺序与旧版侧边栏一致）：name 同时是路由名、
 * 分区 ID 与 RBAC 能力表（roleCapabilities.js）的键
 */
export const NAV_ITEMS = [
  { name: "dashboard", title: "nav.dashboard", icon: "dashboard" },
  { name: "organization", title: "nav.organization", icon: "organization" },
  { name: "resources", title: "nav.resources", icon: "resources" },
  { name: "ip", title: "nav.ip_detail", icon: "ip" },
  { name: "visualization", title: "nav.visualization", icon: "visualization" },
  { name: "agents", title: "nav.agents", icon: "agents" },
  { name: "logs", title: "nav.logs", icon: "logs" },
  { name: "system", title: "nav.system", icon: "system" }
];

const SECTION_COMPONENTS = {
  dashboard: DashboardPage,
  organization: OrganizationPage,
  resources: SectionPlaceholder,
  ip: SectionPlaceholder,
  visualization: SectionPlaceholder,
  agents: SectionPlaceholder,
  logs: LogsPage,
  system: SectionPlaceholder
};

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
