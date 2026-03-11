/**
 * 导航管理模块
 * 处理页面导航、URL 哈希和内容加载
 */

import { loadDashboardData } from "./dashboard.js";
import { initResourceTabs } from "./resourceTabs.js";
import { loadSwitchesForPullMac, loadNetworksForPullMac, loadIpMacData, initIpMacFunctions } from "./ipmanager.js";
import { loadModule } from "../utils/moduleLoader.js";
import { initVisualization } from "./visualization/visualizationManager.js";
import { nextFrame, whenVisible, safeAsync } from "../utils/helpers.js";

// ==========================================
// 常量定义
// ==========================================

const DEFAULT_PAGE = "dashboard";
const PAGE_LOADERS = {
  dashboard: loadDashboardPage,
  resources: loadResourcesPage,
  ip: loadIpPage,
  logs: loadLogsPage,
  system: loadSystemPage,
  visualization: loadVisualizationPage,
};

// ==========================================
// 初始化
// ==========================================

/**
 * 初始化导航功能
 */
export function initNavigation() {
  const navLinks = document.querySelectorAll(".nav-link");
  const contentSections = document.querySelectorAll(".content-section");
  
  bindNavClickHandlers(navLinks);
  loadInitialPage();
  bindHashChangeHandler();
}

// ==========================================
// 页面加载器
// ==========================================

/**
 * 加载仪表盘页面
 */
async function loadDashboardPage() {
  await loadDashboardData();
}

/**
 * 加载资源管理页面
 */
async function loadResourcesPage() {
  initResourceTabs();
}

/**
 * 加载 IP 管理页面
 */
async function loadIpPage() {
  loadSwitchesForPullMac();
  loadNetworksForPullMac();
  initIpMacFunctions();
  nextFrame(() => loadIpMacData());
}

/**
 * 加载日志页面
 */
async function loadLogsPage() {
  const logModule = await loadModule("log", "/static/js/modules/log.js");
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
  const systemModule = await loadModule("systemManager", "/static/js/modules/systemManager.js");
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
  await initVisualization();
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
      window.location.hash = targetId;
    });
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
  updateNavActiveState(targetId);
  updateContentVisibility(targetId);
  await executePageLoader(targetId);
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
