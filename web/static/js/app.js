/**
 * IPMA - IP/MAC Address Management System
 * Copyright (c) 2024-2025 oi-io <boss@oi-io.cc>
 * SPDX-License-Identifier: MIT
 * 
 * 应用程序主入口
 * 初始化所有模块和事件监听器
 */

import { initI18n } from "./utils/i18n.js";
import { initNavigation } from "./modules/navigation.js";
import { initModals } from "./utils/modal.js";
import { initModalTemplates } from "./utils/modalLoader.js";
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
    
    initModalTemplates();
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
  const createCallback = (module, method) => () => import(`./modules/${module}.js`).then((m) => m[method]());
  
  return {
    openNetworkTypeModal: createCallback('networks', 'openNetworkTypeModal'),
    openNetworkModal: createCallback('networks', 'openNetworkModal'),
    openRoomModal: createCallback('room', 'openRoomModal'),
    openWorkstationModal: createCallback('workstation', 'openWorkstationModal'),
    openCabinetModal: createCallback('cabinet', 'openCabinetModal'),
    openCabinetPositionModal: createCallback('position', 'openCabinetPositionModal'),
    openSwitchModal: createCallback('switch/switchManager', 'openSwitchModal'),
    openUserModal: createCallback('userManager', 'openUserModal'),
    submitNetworkTypeForm: createCallback('networks', 'submitNetworkTypeForm'),
    submitNetworkForm: createCallback('networks', 'submitNetworkForm'),
    submitRoomForm: createCallback('room', 'submitRoomForm'),
    submitWorkstationForm: createCallback('workstation', 'submitWorkstationForm'),
    submitCabinetForm: createCallback('cabinet', 'submitCabinetForm'),
    submitCabinetPositionForm: createCallback('position', 'submitCabinetPositionForm'),
    submitSwitchForm: createCallback('switch/switchManager', 'submitSwitchForm'),
    submitSwitchPortForm: createCallback('switch/switchManager', 'submitSwitchPortForm'),
    submitUserForm: createCallback('userManager', 'submitUserForm'),
    pullIpMacData: createCallback('ipmanager', 'pullIpMacData'),
  };
}

// 启动应用
document.addEventListener("DOMContentLoaded", initApp);
