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
 * 等保三权分立：按角色计算不可访问的分区列表。
 * 后端始终强制校验（403），此处仅为界面整洁：
 * - auditor（审计管理员）：仅仪表盘与日志
 * - secadmin（安全管理员）：用户/系统安全/日志，不涉资源运维
 * @returns {string[]} 当前角色无权访问的分区 ID 列表（未受限角色为空数组）
 */
function getHiddenSections() {
  const user = SessionManager.getUser();
  const role = user?.role || "user";
  if (role === "auditor") {
    return ["organization", "resources", "ip", "visualization", "system"];
  }
  if (role === "secadmin") {
    return ["organization", "resources", "visualization"];
  }
  return [];
}

export function applyRoleVisibility() {
  const hiddenSections = getHiddenSections();
  if (hiddenSections.length === 0) {
    return;
  }

  for (const section of hiddenSections) {
    const link = document.querySelector(`.nav-link[href="#${section}"]`);
    link?.closest("li")?.classList.add("hidden");
  }

  // 当前落在被隐藏分区时回到仪表盘；已是默认页时不再赋值，
  // 避免等值赋值之外的多余 hashchange（配合下方初始化去重，杜绝仪表盘双载）
  if (
    hiddenSections.includes(window.location.hash.slice(1)) &&
    window.location.hash !== `#${DEFAULT_PAGE}`
  ) {
    window.location.hash = `#${DEFAULT_PAGE}`;
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
    await safeAsync(() => systemModule.loadSystemInfo(), t("nav.load_system_info"));
    await safeAsync(() => systemModule.loadSystemConfig(), t("nav.load_system_config"));
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
 * 初始化期间已加载的分区：程序化修改 hash 会再触发一次 hashchange，
 * 目标与初始加载相同时跳过，保证初始 loadPageContent 只执行一次
 */
let initialLoadedPageId = null;

/**
 * 绑定哈希变化事件
 */
function bindHashChangeHandler() {
  window.addEventListener("hashchange", () => {
    const hash = window.location.hash;
    const targetId = hash ? hash.substring(1) : DEFAULT_PAGE;

    // 角色无权访问的分区（hash 直达绕过菜单隐藏）重定向回仪表盘：
    // 设置 hash 会再次触发本监听，按仪表盘正常加载
    if (getHiddenSections().includes(targetId)) {
      window.location.hash = `#${DEFAULT_PAGE}`;
      return;
    }

    // 初始化时程序化设置 hash 引发的首次 hashchange：初始加载已按最终
    // hash 执行过，同一目标直接跳过（否则初始页会被加载两次）
    if (initialLoadedPageId === targetId) {
      initialLoadedPageId = null;
      return;
    }
    initialLoadedPageId = null;

    // 未知 hash 兜底：回落默认页重定向加载，避免主内容区空白
    if (!PAGE_LOADERS[targetId]) {
      window.location.hash = `#${DEFAULT_PAGE}`;
      return;
    }

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

  // 未知分区（书签/手输错值）回落默认页：纠正地址栏 hash 并同步加载默认页；
  // 由此触发的 hashchange 由 initialLoadedPageId 去重，不会二次加载。
  // 空 hash 本就解析为默认页，不额外赋值
  const known = Boolean(PAGE_LOADERS[targetId]);
  const effectiveId = known ? targetId : DEFAULT_PAGE;
  if (!known && hash !== `#${DEFAULT_PAGE}`) {
    window.location.hash = `#${DEFAULT_PAGE}`;
  }
  initialLoadedPageId = effectiveId;
  loadPageContent(effectiveId);
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
    await safeAsync(loader, t("nav.load_page", { page: targetId }));
  }
}
