/**
 * 资源管理标签页初始化模块
 * 处理资源管理页面的标签页切换和数据加载
 */

import { loadModule } from "../utils/resourceLoader.js";
import { nextFrame, safeAsync, setActiveSubtab, getActiveSubtab } from "../utils/helpers.js";

// ==========================================
// 常量定义
// ==========================================

const TAB_CONFIG = {
  rooms: { module: "room", initFn: "initRoomSortEvents", loadFn: "loadRoomsData" },
  "network-regions": {
    module: "networks",
    initFn: "initNetworksFilters",
    loadFn: "loadNetworkRegionsData"
  },
  networks: { module: "networks", initFn: "initNetworksFilters", loadFn: "loadNetworksData" },
  cabinets: { module: "cabinet", initFn: "initCabinetSortEvents", loadFn: "loadCabinetsData" },
  "cable-links": {
    module: "cableLink",
    initFn: "initCableLinkSortEvents",
    loadFn: "loadCableLinksData"
  },
  devices: { module: "device", initFn: "initDeviceSortEvents", loadFn: "loadDevicesData" }
};

const initializedModules = new Set();

// ==========================================
// 初始化
// ==========================================

/**
 * 初始化资源管理标签页
 */
export async function initResourceTabs() {
  const resourcesContainer = document.getElementById("resources");

  if (!resourcesContainer) {
    return;
  }

  if (isAlreadyInitialized(resourcesContainer)) {
    return;
  }

  bindTabClickHandlers(resourcesContainer);
  markAsInitialized(resourcesContainer);
  loadDefaultTabData(resourcesContainer);
}

// ==========================================
// 事件绑定
// ==========================================

/**
 * 绑定标签页点击事件
 * @param {HTMLElement} container - 资源管理容器
 */
function bindTabClickHandlers(container) {
  const tabBtns = container.querySelectorAll(".tab-btn");
  const tabContents = container.querySelectorAll(".tab-content");

  tabBtns.forEach((btn) => {
    btn.addEventListener("click", function () {
      const tabId = this.getAttribute("data-tab");

      updateActiveTab(tabBtns, tabContents, this, tabId);
      setActiveSubtab(container.id, tabId);
      loadTabData(tabId);
    });
  });
}

/**
 * 更新激活的标签页
 * @param {NodeList} tabBtns - 标签按钮列表
 * @param {NodeList} tabContents - 标签内容列表
 * @param {HTMLElement} activeBtn - 激活的按钮
 * @param {string} tabId - 标签 ID
 */
function updateActiveTab(tabBtns, tabContents, activeBtn, tabId) {
  tabBtns.forEach((btn) => btn.classList.remove("active"));
  activeBtn.classList.add("active");

  tabContents.forEach((content) => content.classList.remove("active"));

  const targetTab = document.getElementById(`${tabId}-tab`);
  targetTab?.classList.add("active");
}

/**
 * 加载标签页数据
 * @param {string} tabId - 标签 ID
 */
function loadTabData(tabId) {
  nextFrame(async () => {
    const config = TAB_CONFIG[tabId];
    if (!config) return;

    const module = await loadModule(config.module);

    if (!initializedModules.has(config.module)) {
      initializedModules.add(config.module);
      module[config.initFn]?.();
    }

    safeAsync(() => module[config.loadFn](), `加载标签 ${tabId} 数据`);
  });
}

// ==========================================
// 辅助函数
// ==========================================

/**
 * 检查是否已初始化
 * @param {HTMLElement} container - 容器元素
 * @returns {boolean}
 */
function isAlreadyInitialized(container) {
  return container.dataset.tabsInitialized === "true";
}

/**
 * 标记为已初始化
 * @param {HTMLElement} container - 容器元素
 */
function markAsInitialized(container) {
  container.dataset.tabsInitialized = "true";
}

/**
 * 加载默认标签页数据
 * 优先恢复上次记住的子标签（刷新后仍停留在原标签），否则使用 HTML 默认激活项
 * @param {HTMLElement} container - 容器元素
 */
function loadDefaultTabData(container) {
  const tabBtns = container.querySelectorAll(".tab-btn");
  const tabContents = container.querySelectorAll(".tab-content");

  // 优先恢复上次记住的子标签
  const savedTabId = getActiveSubtab(container.id);
  let targetBtn = savedTabId
    ? container.querySelector(`.tab-btn[data-tab="${CSS.escape(savedTabId)}"]`)
    : null;

  // 回退到 HTML 默认激活项或第一个标签
  if (!targetBtn) {
    targetBtn = container.querySelector(".tab-btn.active") || container.querySelector(".tab-btn");
  }

  if (targetBtn) {
    const tabId = targetBtn.getAttribute("data-tab");
    updateActiveTab(tabBtns, tabContents, targetBtn, tabId);
    loadTabData(tabId);
  }
}
