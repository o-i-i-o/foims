import { loadDashboardData } from "./dashboard.js";
import { initResourceTabs } from "./resourceTabs.js";
import { loadSwitchesForPullMac, loadNetworksForPullMac, loadIpMacData, initIpMacFunctions } from "./ipmanager.js";
import { loadModule } from "../utils/moduleLoader.js";
import { nextFrame, whenVisible, safeAsync } from "../utils/helpers.js";
const DEFAULT_PAGE = "dashboard";
const PAGE_LOADERS = {
    dashboard: loadDashboardPage,
    resources: loadResourcesPage,
    ip: loadIpPage,
    logs: loadLogsPage,
    system: loadSystemPage,
    visualization: loadVisualizationPage,
};
export function initNavigation() {
    const navLinks = document.querySelectorAll(".nav-link");
    bindNavClickHandlers(navLinks);
    loadInitialPage();
    bindHashChangeHandler();
}
async function loadDashboardPage() {
    await loadDashboardData();
}
async function loadResourcesPage() {
    initResourceTabs();
}
async function loadIpPage() {
    loadSwitchesForPullMac();
    loadNetworksForPullMac();
    initIpMacFunctions();
    nextFrame(() => loadIpMacData());
}
async function loadLogsPage() {
    const logModule = await loadModule("log", "/static/js/modules/log.js");
    logModule.initLogTabs();
    whenVisible("#logs .tab-btn.active", () => {
        const activeTabBtn = document.querySelector("#logs .tab-btn.active");
        const logType = activeTabBtn?.getAttribute("data-tab") || "operation";
        if (logType === "notifications") {
            logModule.loadNotificationsData();
        }
        else {
            logModule.loadLogsData(logType);
        }
    });
}
async function loadSystemPage() {
    const systemModule = await loadModule("systemManager", "/static/js/modules/systemManager.js");
    systemModule.initSystemTabs();
    whenVisible("#system", async () => {
        await safeAsync(() => Promise.resolve(systemModule.loadSystemInfo()), "加载系统信息");
        await safeAsync(() => Promise.resolve(systemModule.loadSystemConfig()), "加载系统配置");
        systemModule.initSmtpFunctions();
    });
}
async function loadVisualizationPage() {
    // TODO: implement visualization page
    console.log("Visualization page not implemented yet");
}
function bindNavClickHandlers(navLinks) {
    navLinks.forEach(link => {
        link.addEventListener("click", (e) => {
            e.preventDefault();
            const href = link.getAttribute("href");
            if (href) {
                const targetId = href.substring(1);
                window.location.hash = targetId;
            }
        });
    });
}
function loadInitialPage() {
    const hash = window.location.hash.substring(1) || DEFAULT_PAGE;
    loadPage(hash);
}
function bindHashChangeHandler() {
    window.addEventListener("hashchange", () => {
        const hash = window.location.hash.substring(1) || DEFAULT_PAGE;
        loadPage(hash);
    });
}
export function loadPage(pageId) {
    const sections = document.querySelectorAll(".page-section");
    sections.forEach(section => {
        section.classList.remove("active");
    });
    const targetSection = document.getElementById(pageId);
    if (targetSection) {
        targetSection.classList.add("active");
    }
    const navLinks = document.querySelectorAll(".nav-link");
    navLinks.forEach(link => {
        link.classList.remove("active");
        const href = link.getAttribute("href");
        if (href === `#${pageId}`) {
            link.classList.add("active");
        }
    });
    const loader = PAGE_LOADERS[pageId];
    if (loader) {
        safeAsync(() => Promise.resolve(loader()), `加载${pageId}页面`);
    }
}
//# sourceMappingURL=navigation.js.map