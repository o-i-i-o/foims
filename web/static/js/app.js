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
import { initModalTemplates, preloadModalsOnIdle } from "./utils/modalLoader.js";
import { initEventListeners } from "./modules/eventManager.js";
import { initUserEvents } from "./modules/userManager.js";
import { 
  displayCurrentUser,
  initAutoRefresh,
  initPageTimeout,
  initLogout,
  checkLoginStatus
} from "./modules/authManager.js";
import { 
  prefetchModules,
  schedulePreload,
  lazyLoad
} from "./utils/resourceLoader.js";

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
    await initAutoRefresh();
    await initPageTimeout();
    
    initResourcePreloading();
    
  } catch (error) {
    console.error("应用程序初始化失败:", error);
    // 显示用户友好的错误提示
    const errorDiv = document.createElement('div');
    errorDiv.style.cssText = 'position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); padding: 20px; background: #fff; border: 1px solid #ccc; border-radius: 8px; text-align: center; z-index: 10000; box-shadow: 0 2px 10px rgba(0,0,0,0.1);';
    errorDiv.innerHTML = `
      <h3 style="margin: 0 0 15px 0; color: #e74c3c;">应用程序加载失败</h3>
      <p style="margin: 0 0 15px 0; color: #666;">请刷新页面重试，或联系管理员。</p>
      <button onclick="location.reload()" style="padding: 8px 16px; background: #3498db; color: #fff; border: none; border-radius: 4px; cursor: pointer;">刷新页面</button>
    `;
    document.body.appendChild(errorDiv);
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
    openSwitchModal: createCallback('switch/switchDevice', 'openSwitchModal'),
    openUserModal: createCallback('userManager', 'openUserModal'),
    openOrgModal: createCallback('organization', 'openOrgModal'),
    openNodeModal: createCallback('node', 'openNodeModal'),
    openAccessPointModal: createCallback('accessPoint', 'openAccessPointModal'),
    openDeviceTemplateModal: createCallback('deviceTemplate', 'openDeviceTemplateModal'),
    openDeviceModal: createCallback('device', 'openDeviceModal'),
    submitOrgTemplateForm: createCallback('organization', 'submitOrgTemplateForm'),
    submitNetworkTypeForm: createCallback('networks', 'submitNetworkTypeForm'),
    submitNetworkForm: createCallback('networks', 'submitNetworkForm'),
    submitRoomForm: createCallback('room', 'submitRoomForm'),
    submitWorkstationForm: createCallback('workstation', 'submitWorkstationForm'),
    submitCabinetForm: createCallback('cabinet', 'submitCabinetForm'),
    submitCabinetPositionForm: createCallback('position', 'submitCabinetPositionForm'),
    submitSwitchForm: createCallback('switch/switchDevice', 'submitSwitchForm'),
    submitSwitchPortForm: createCallback('switch/switchDevice', 'submitSwitchPortForm'),
    submitUserForm: createCallback('userManager', 'submitUserForm'),
    submitOrgForm: createCallback('organization', 'submitOrgForm'),
    submitNodeForm: createCallback('node', 'submitNodeForm'),
    submitAccessPointForm: createCallback('accessPoint', 'submitAccessPointForm'),
    submitDeviceTemplateForm: createCallback('deviceTemplate', 'submitDeviceTemplateForm'),
    submitDeviceForm: createCallback('device', 'submitDeviceForm'),
  };
}

function initResourcePreloading() {
  prefetchModules([
    'apiClient',
    'toast',
    'confirm',
    'formatter',
    'ui'
  ]);
  
  schedulePreload([
    'networks',
    'room',
    'workstation',
    'cabinet',
    'position',
    'switchDevice',
    'visualizationManager'
  ], { delay: 2000, priority: 'low' });
  
  lazyLoad('dashboard', { when: 'idle' });
  
  preloadModalsOnIdle();
}

// 启动应用
document.addEventListener("DOMContentLoaded", initApp);
