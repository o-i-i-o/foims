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
    `;
    const reloadBtn = document.createElement('button');
    reloadBtn.textContent = '刷新页面';
    reloadBtn.style.cssText = 'padding: 8px 16px; background: #3498db; color: #fff; border: none; border-radius: 4px; cursor: pointer;';
    reloadBtn.addEventListener('click', () => location.reload());
    errorDiv.appendChild(reloadBtn);
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
    openCabinetModal: createCallback('cabinet', 'openCabinetModal'),
    openUserModal: createCallback('userManager', 'openUserModal'),
    openOrgModal: createCallback('organization', 'openOrgModal'),
    openNetOutletModal: createCallback('netOutlet', 'openNetOutletModal'),
    openDeviceModal: createCallback('device', 'openDeviceModal'),
    submitOrgTemplateForm: createCallback('organization', 'submitOrgTemplateForm'),
    submitNetworkTypeForm: createCallback('networks', 'submitNetworkTypeForm'),
    submitNetworkForm: createCallback('networks', 'submitNetworkForm'),
    submitRoomForm: createCallback('room', 'submitRoomForm'),
    submitWorkstationForm: createCallback('workstation', 'submitWorkstationForm'),
    submitCabinetForm: createCallback('cabinet', 'submitCabinetForm'),
    submitCabinetPositionForm: createCallback('position', 'submitCabinetPositionForm'),
    submitDevicePortForm: createCallback('devicePorts', 'submitDevicePortForm'),
    submitUserForm: createCallback('userManager', 'submitUserForm'),
    submitOrgForm: createCallback('organization', 'submitOrgForm'),
    submitNetOutletForm: createCallback('netOutlet', 'submitNetOutletForm'),
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
    'resourceTabs',
    'networks',
    'room',
    'workstation',
    'cabinet',
    'position',
    'netOutlet',
    'device',
    'devicePorts',
    'ipmanager',
    'visualizationManager'
  ], { delay: 2000, priority: 'low' });
  
  lazyLoad('dashboard', { when: 'idle' });
  
  preloadModalsOnIdle();
}

// 等待所有样式表加载完成，避免 FOUC
function waitForStylesheets() {
  const links = document.querySelectorAll('link[rel="stylesheet"]');
  const promises = Array.from(links).map(link => {
    if (link.sheet) {
      try {
        link.sheet.cssRules;
        return Promise.resolve();
      } catch {
        // cross-origin stylesheet, treat as loaded
        return Promise.resolve();
      }
    }
    return new Promise(resolve => {
      link.addEventListener('load', resolve, { once: true });
      link.addEventListener('error', resolve, { once: true });
    });
  });
  return Promise.all(promises);
}

// 启动应用
document.addEventListener("DOMContentLoaded", async () => {
  await waitForStylesheets();
  initApp();
});
