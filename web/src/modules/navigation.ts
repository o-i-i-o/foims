import { loadDashboardData } from "./dashboard.js";
import { initResourceTabs } from "./resourceTabs.js";
import { loadSwitchesForPullMac, loadNetworksForPullMac, loadIpMacData, initIpMacFunctions } from "./ipmanager.js";
import { loadModule } from "../utils/moduleLoader.js";
import { initVisualization } from "./visualization/visualizationManager.js";
import { nextFrame, whenVisible, safeAsync } from "../utils/helpers.js";

const DEFAULT_PAGE = "dashboard";

type PageLoader = () => Promise<void>;

const PAGE_LOADERS: Record<string, PageLoader> = {
  dashboard: loadDashboardPage,
  resources: loadResourcesPage,
  ip: loadIpPage,
  logs: loadLogsPage,
  system: loadSystemPage,
  visualization: loadVisualizationPage,
};

export function initNavigation(): void {
  const navLinks = document.querySelectorAll(".nav-link");
  bindNavClickHandlers(navLinks);
  loadInitialPage();
  bindHashChangeHandler();
}

async function loadDashboardPage(): Promise<void> {
  await loadDashboardData();
}

async function loadResourcesPage(): Promise<void> {
  initResourceTabs();
}

async function loadIpPage(): Promise<void> {
  loadSwitchesForPullMac();
  loadNetworksForPullMac();
  initIpMacFunctions();
  nextFrame(() => loadIpMacData());
}

async function loadLogsPage(): Promise<void> {
  const logModule = await loadModule<Record<string, (...args: unknown[]) => void>>("log", "/static/js/modules/log.js");
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

async function loadSystemPage(): Promise<void> {
  const systemModule = await loadModule<Record<string, (...args: unknown[]) => void>>("systemManager", "/static/js/modules/systemManager.js");
  systemModule.initSystemTabs();

  whenVisible("#system", async () => {
    await safeAsync(() => systemModule.loadSystemInfo(), "加载系统信息");
    await safeAsync(() => systemModule.loadSystemConfig(), "加载系统配置");
    systemModule.initSmtpFunctions();
  });
}

async function loadVisualizationPage(): Promise<void> {
  await initVisualization();
}

function bindNavClickHandlers(navLinks: NodeListOf<Element>): void {
  navLinks.forEach(link => {
    link.addEventListener("click", (e: Event) => {
      e.preventDefault();
      const href = (link as HTMLElement).getAttribute("href");
      if (href) {
        const targetId = href.substring(1);
        window.location.hash = targetId;
      }
    });
  });
}

function bindHashChangeHandler(): void {
  window.addEventListener("hashchange", () => {
    const hash = window.location.hash;
    const targetId = hash ? hash.substring(1) : DEFAULT_PAGE;

    if (!hash && document.querySelector(".content-section.active")) {
      return;
    }

    loadPageContent(targetId);
  });
}

function loadInitialPage(): void {
  const hash = window.location.hash;
  const targetId = hash ? hash.substring(1) : DEFAULT_PAGE;
  loadPageContent(targetId);
}

async function loadPageContent(targetId: string): Promise<void> {
  updateNavActiveState(targetId);
  updateContentVisibility(targetId);
  await executePageLoader(targetId);
}

function updateNavActiveState(targetId: string): void {
  const navLinks = document.querySelectorAll(".nav-link");

  navLinks.forEach(link => {
    link.classList.remove("active");
    if ((link as HTMLElement).getAttribute("href") === `#${targetId}`) {
      link.classList.add("active");
    }
  });
}

function updateContentVisibility(targetId: string): void {
  const contentSections = document.querySelectorAll(".content-section");

  contentSections.forEach(section => {
    section.classList.remove("active");
    if (section.id === targetId) {
      section.classList.add("active");
    }
  });
}

async function executePageLoader(targetId: string): Promise<void> {
  const loader = PAGE_LOADERS[targetId];
  if (loader) {
    await safeAsync(loader, `加载页面 ${targetId}`);
  }
}
