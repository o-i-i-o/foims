/**
 * 资源管理标签页初始化模块
 * 处理资源管理页面的标签页切换和数据加载
 */

import { loadNetworkTypesData, loadNetworksData, initNetworksFilters } from "./networks.js";
import { loadRoomsData, initRoomSortEvents } from "./room.js";
import { loadWorkstationsData, initWorkstationSortEvents } from "./workstation.js";
import { loadCabinetsData, initCabinetSortEvents } from "./cabinet.js";
import { loadCabinetPositionsData, initPositionSortEvents } from "./position.js";
import { loadAccessPointsData, initAccessPointSortEvents } from "./accessPoint.js";
import { loadDevicesData, initDeviceSortEvents } from "./device.js";
import { nextFrame, safeAsync } from "../utils/helpers.js";

// ==========================================
// 常量定义
// ==========================================

const TAB_DATA_LOADERS = {
  rooms: loadRoomsData,
  workstations: loadWorkstationsData,
  "network-regions": loadNetworkTypesData,
  networks: loadNetworksData,
  cabinets: loadCabinetsData,
  "cabinet-positions": loadCabinetPositionsData,
  "access-points": loadAccessPointsData,
  devices: loadDevicesData,
};

// ==========================================
// 初始化
// ==========================================

/**
 * 初始化资源管理标签页
 */
export function initResourceTabs() {
  const resourcesContainer = document.getElementById("resources");
  
  if (!resourcesContainer) {
    return;
  }
  
  if (isAlreadyInitialized(resourcesContainer)) {
    return;
  }
  
  initNetworksFilters();
  initRoomSortEvents();
  initWorkstationSortEvents();
  initCabinetSortEvents();
  initPositionSortEvents();
  initAccessPointSortEvents();
  initDeviceSortEvents();
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
  nextFrame(() => {
    const loader = TAB_DATA_LOADERS[tabId];
    
    if (loader) {
      safeAsync(loader, `加载标签 ${tabId} 数据`);
    }
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
 * @param {HTMLElement} container - 容器元素
 */
function loadDefaultTabData(container) {
  const activeTabBtn = container.querySelector(".tab-btn.active");
  const defaultTabBtn = activeTabBtn || container.querySelector(".tab-btn");
  
  if (defaultTabBtn) {
    const tabId = defaultTabBtn.getAttribute("data-tab");
    loadTabData(tabId);
  }
}
