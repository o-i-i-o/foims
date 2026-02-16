/**
 * 应用程序主入口
 * 初始化所有模块和事件监听器
 */

import { initI18n } from "./utils/i18n.js";
import { initNavigation } from "./modules/navigation.js";
import { initModals } from "./utils/modal.js";
import { initEventListeners } from "./modules/eventManager.js";
import { initUserEvents } from "./modules/userManager.js";
import { 
  displayCurrentUser,
  initAutoRefresh,
  initPageTimeout,
  initLogout,
  checkLoginStatus
} from "./modules/authManager.js";

// ==========================================
// 应用初始化
// ==========================================

/**
 * 初始化应用程序
 */
async function initApp() {
  try {
    await initI18n();
    
    await checkLoginStatus();
    
    initNavigation();
    initModals(getResourceCallbacks());
    initEventListeners();
    initUserEvents();
    
    displayCurrentUser();
    initLogout();
    initAutoRefresh();
    await initPageTimeout();
    
  } catch (error) {
    console.error("应用程序初始化失败:", error);
  }
}

/**
 * 获取资源回调函数
 * @returns {Object}
 */
function getResourceCallbacks() {
  return {
    openNetworkTypeModal: () => import("./modules/networks.js").then((m) => m.openNetworkTypeModal()),
    openNetworkModal: () => import("./modules/networks.js").then((m) => m.openNetworkModal()),
    openRoomModal: () => import("./modules/room.js").then((m) => m.openRoomModal()),
    openWorkstationModal: () => import("./modules/workstation.js").then((m) => m.openWorkstationModal()),
    openCabinetModal: () => import("./modules/cabinet.js").then((m) => m.openCabinetModal()),
    openCabinetPositionModal: () => import("./modules/position.js").then((m) => m.openCabinetPositionModal()),
    openSwitchModal: () => import("./modules/switchManager.js").then((m) => m.openSwitchModal()),
    openUserModal: () => import("./modules/userManager.js").then((m) => m.openUserModal()),
    submitNetworkTypeForm: () => import("./modules/networks.js").then((m) => m.submitNetworkTypeForm()),
    submitNetworkForm: () => import("./modules/networks.js").then((m) => m.submitNetworkForm()),
    submitRoomForm: () => import("./modules/room.js").then((m) => m.submitRoomForm()),
    submitWorkstationForm: () => import("./modules/workstation.js").then((m) => m.submitWorkstationForm()),
    submitCabinetForm: () => import("./modules/cabinet.js").then((m) => m.submitCabinetForm()),
    submitCabinetPositionForm: () => import("./modules/position.js").then((m) => m.submitCabinetPositionForm()),
    submitSwitchForm: () => import("./modules/switchManager.js").then((m) => m.submitSwitchForm()),
    submitSwitchPortForm: () => import("./modules/switchManager.js").then((m) => m.submitSwitchPortForm()),
    submitUserForm: () => import("./modules/userManager.js").then((m) => m.submitUserForm()),
    pullIpMacData: () => import("./modules/ipmanager.js").then((m) => m.pullIpMacData()),
  };
}

// 启动应用
document.addEventListener("DOMContentLoaded", initApp);
