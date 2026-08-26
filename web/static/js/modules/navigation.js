/**
 * 导航管理模块
 * 处理页面导航、URL 哈希和内容加载
 */

import { loadModule } from "../utils/resourceLoader.js";
import { loadPageStyles, preloadPageStyles } from "../utils/styleLoader.js";
import { nextFrame, whenVisible, safeAsync } from "../utils/helpers.js";
import { t } from "../utils/i18n.js";
import { SessionManager } from "../utils/sessionManager.js";

const DEFAULT_PAGE = "dashboard";
const PAGE_LOADERS = {
  dashboard: loadDashboardPage,
  resources: loadResourcesPage,
  organization: loadOrganizationPage,
  ip: loadIpPage,
  logs: loadLogsPage,
  system: loadSystemPage,
  visualization: loadVisualizationPage
};

const SIDEBAR_COLLAPSE_STORAGE_KEY = "sidebar-collapsed";

// ==========================================
// 初始化
// ==========================================

/**
 * 初始化导航功能
 */
export function initNavigation() {
  const navLinks = document.querySelectorAll(".nav-link");

  applyRoleVisibility();
  bindNavClickHandlers(navLinks);
  loadInitialPage();
  bindHashChangeHandler();
  initSidebarCollapse();
}

/**
 * 等保三权分立：按角色隐藏无权访问的导航入口。
 * 后端始终强制校验（403），此处仅为界面整洁：
 * - auditor（审计管理员）：仅仪表盘与日志
 * - secadmin（安全管理员）：用户/系统安全/日志，不涉资源运维
 */
export function applyRoleVisibility() {
  const user = SessionManager.getUser();
  const role = user?.role || "user";
  if (!["auditor", "secadmin"].includes(role)) {
    return;
  }

  const hiddenSections =
    role === "auditor"
      ? ["organization", "resources", "ip", "visualization", "system"]
      : ["organization", "resources", "visualization"];

  for (const section of hiddenSections) {
    const link = document.querySelector(`.nav-link[href="#${section}"]`);
    link?.closest("li")?.classList.add("hidden");
  }

  // 当前落在被隐藏分区时回到仪表盘
  if (hiddenSections.includes(window.location.hash.slice(1))) {
    window.location.hash = "#dashboard";
  }
}

// ==========================================
// 侧边栏收起/展开
// ==========================================

/**
 * 初始化侧边栏收起/展开功能
 * 折叠按钮位于标题块左侧，状态持久化到 localStorage，
 * 收起后仅保留图标导航栏，展开恢复正常显示
 */
export function initSidebarCollapse() {
  const toggleBtn = document.getElementById("sidebar-toggle-btn");
  if (!toggleBtn) {
    return;
  }

  const applyState = (collapsed) => {
    document.body.classList.toggle("sidebar-collapsed", collapsed);
    toggleBtn.setAttribute("aria-expanded", collapsed ? "false" : "true");
    const label = t(collapsed ? "nav.expand_sidebar" : "nav.collapse_sidebar");
    toggleBtn.dataset.tooltip = label;
    toggleBtn.setAttribute("aria-label", label);
    localStorage.setItem(SIDEBAR_COLLAPSE_STORAGE_KEY, collapsed ? "1" : "0");
  };

  applyState(localStorage.getItem(SIDEBAR_COLLAPSE_STORAGE_KEY) === "1");

  toggleBtn.addEventListener("click", () => {
    applyState(!document.body.classList.contains("sidebar-collapsed"));
  });

  // 语言切换后同步按钮提示与无障碍标签
  window.addEventListener("languagechange", () => {
    applyState(document.body.classList.contains("sidebar-collapsed"));
  });
}

// ==========================================
// 页面加载器
// ==========================================

/**
 * 加载仪表盘页面
 */
async function loadDashboardPage() {
  const dashboard = await loadModule("dashboard");
  await dashboard.loadDashboardData();
}

/**
 * 加载资源管理页面
 */
async function loadResourcesPage() {
  const resourceTabs = await loadModule("resourceTabs");
  await resourceTabs.initResourceTabs();
}

/**
 * 加载组织管理页面
 */
async function loadOrganizationPage() {
  const orgModule = await loadModule("organization");
  orgModule.initOrganization();
}

/**
 * 加载 IP 查询页面
 */
async function loadIpPage() {
  const ipmanager = await loadModule("ipmanager");
  ipmanager.initIpMacFunctions();
  nextFrame(() => ipmanager.loadIpMacData());
}

/**
 * 加载日志页面
 */
async function loadLogsPage() {
  const logModule = await loadModule("log");
  logModule.initLogTabs();

  whenVisible("#logs .tab-btn.active", () => {
    const activeTabBtn = document.querySelector("#logs .tab-btn.active");
    const logType = activeTabBtn?.getAttribute("data-tab") || "operation";
    if (logType === "notifications") {
      logModule.loadNotificationsData();
    } else {
      logModule.loadLogsData(logType);
    }
  });
}

/**
 * 加载系统管理页面
 */
async function loadSystemPage() {
  const systemModule = await loadModule("systemManager");
  systemModule.initSystemTabs();

  whenVisible("#system", async () => {
    await safeAsync(() => systemModule.loadSystemInfo(), "加载系统信息");
    await safeAsync(() => systemModule.loadSystemConfig(), "加载系统配置");
    systemModule.initSmtpFunctions();
  });
}

/**
 * 加载可视化页面
 */
async function loadVisualizationPage() {
  const viz = await loadModule("visualizationManager");
  await viz.initVisualization();
}

// ==========================================
// 导航处理
// ==========================================

/**
 * 绑定导航点击事件
 * @param {NodeList} navLinks - 导航链接元素列表
 */
function bindNavClickHandlers(navLinks) {
  navLinks.forEach((link) => {
    link.addEventListener("click", (e) => {
      e.preventDefault();

      const targetId = link.getAttribute("href").substring(1);
      preloadPageStyles(targetId);
      window.location.hash = targetId;
    });

    link.addEventListener(
      "mouseenter",
      () => {
        const targetId = link.getAttribute("href").substring(1);
        preloadPageStyles(targetId);
      },
      { once: false }
    );
  });
}

/**
 * 绑定哈希变化事件
 */
function bindHashChangeHandler() {
  window.addEventListener("hashchange", () => {
    const hash = window.location.hash;
    const targetId = hash ? hash.substring(1) : DEFAULT_PAGE;

    // 如果哈希为空，不要重新加载默认页面，除非当前没有激活的页面
    if (!hash && document.querySelector(".content-section.active")) {
      return;
    }

    loadPageContent(targetId);
  });
}

/**
 * 加载初始页面
 */
function loadInitialPage() {
  const hash = window.location.hash;
  const targetId = hash ? hash.substring(1) : DEFAULT_PAGE;

  loadPageContent(targetId);
}

/**
 * 加载页面内容
 * @param {string} targetId - 目标页面 ID
 */
async function loadPageContent(targetId) {
  // 页面专属 CSS 与模块 JS 互不依赖，并行加载省一段串行等待
  const stylesReady = loadPageStyles(targetId);
  updateNavActiveState(targetId);
  updateContentVisibility(targetId);
  const loaderReady = executePageLoader(targetId);
  await Promise.all([stylesReady, loaderReady]);
}

/**
 * 更新导航激活状态
 * @param {string} targetId - 目标页面 ID
 */
function updateNavActiveState(targetId) {
  const navLinks = document.querySelectorAll(".nav-link");

  navLinks.forEach((link) => {
    link.classList.remove("active");

    if (link.getAttribute("href") === `#${targetId}`) {
      link.classList.add("active");
    }
  });
}

/**
 * 更新内容区域可见性
 * @param {string} targetId - 目标页面 ID
 */
function updateContentVisibility(targetId) {
  const contentSections = document.querySelectorAll(".content-section");

  contentSections.forEach((section) => {
    section.classList.remove("active");

    if (section.id === targetId) {
      section.classList.add("active");
    }
  });
}

/**
 * 执行页面加载器
 * @param {string} targetId - 目标页面 ID
 */
async function executePageLoader(targetId) {
  const loader = PAGE_LOADERS[targetId];

  if (loader) {
    await safeAsync(loader, `加载页面 ${targetId}`);
  }
}
