/**
 * FOIMS - Organization IT Information Management System
 * Copyright (c) 2024-2025 oi-io <boss@oi-io.cc>
 * SPDX-License-Identifier: MIT
 *
 * 应用程序主入口
 * 初始化所有模块和事件监听器
 */

import { initI18n, t } from "./utils/i18n.js";
import { initNavigation } from "./modules/navigation.js";
import { initLanguageMenu } from "./modules/languageMenu.js";
import { initEventListeners, initModals } from "./modules/eventManager.js";
import { initUserEvents } from "./modules/userManager.js";
import { initTooltip } from "./utils/tooltip.js";
import {
  displayCurrentUser,
  initAutoRefresh,
  initPageTimeout,
  initLogout,
  initChangePassword,
  checkLoginStatus
} from "./modules/authManager.js";
import { schedulePreload, lazyLoad, loadModule } from "./utils/resourceLoader.js";
import { prefetchModalsOnIdle } from "./utils/modalLoader.js";

// ==========================================
// 应用初始化
// ==========================================

/**
 * 初始化应用程序
 */
async function initApp() {
  try {
    // i18n 与登录态校验互不依赖（checkLoginStatus 走原生 fetch、不经过
    // translateServerMessage），并行执行省一个串行 RTT
    await Promise.all([initI18n(), checkLoginStatus()]);

    initNavigation();
    initLanguageMenu();
    initModals(getResourceCallbacks());
    initEventListeners();
    initUserEvents();
    initTooltip();

    displayCurrentUser();
    initLogout();
    initChangePassword();

    // 三个互不依赖的异步初始化并行执行，预加载不再被网络请求串行阻塞
    await Promise.allSettled([initAutoRefresh(), initPageTimeout(), initResourcePreloading()]);
  } catch (error) {
    console.error("应用程序初始化失败:", error);
    // 显示用户友好的错误提示
    const errorDiv = document.createElement("div");
    errorDiv.style.cssText =
      "position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); padding: 20px; background: #fff; border: 1px solid #ccc; border-radius: 8px; text-align: center; z-index: 10000; box-shadow: 0 2px 10px rgba(0,0,0,0.1);";
    errorDiv.innerHTML = `
      <h3 style="margin: 0 0 15px 0; color: #e74c3c;">${t("app.load_failed")}</h3>
      <p style="margin: 0 0 15px 0; color: #666;">${t("app.refresh_or_contact")}</p>
    `;
    const reloadBtn = document.createElement("button");
    reloadBtn.textContent = t("app.refresh_page");
    reloadBtn.style.cssText =
      "padding: 8px 16px; background: #3498db; color: #fff; border: none; border-radius: 4px; cursor: pointer;";
    reloadBtn.addEventListener("click", () => location.reload());
    errorDiv.appendChild(reloadBtn);
    document.body.appendChild(errorDiv);
  }
}

/**
 * 获取资源回调函数
 * @returns {Object}
 */
function getResourceCallbacks() {
  // 统一使用 loadModule 加载模块，确保与 eventManager 等其他调用方使用同一个模块实例
  // 避免原生 import() 不带版本号导致浏览器加载两份模块，产生两个独立单例
  const createCallback = (module, method) => async () => {
    const m = await loadModule(module);
    return m[method]();
  };

  return {
    openNetworkRegionModal: createCallback("networks", "openNetworkRegionModal"),
    openNetworkModal: createCallback("networks", "openNetworkModal"),
    openRoomModal: createCallback("room", "openRoomModal"),
    openCabinetModal: createCallback("cabinet", "openCabinetModal"),
    openUserModal: createCallback("userManager", "openUserModal"),
    openOrgModal: createCallback("organization", "openOrgModal"),
    openCableLinkModal: createCallback("cableLink", "openCableLinkModal"),
    openCableLabelPrintModal: createCallback("cableLink", "openCableLabelPrintModal"),
    openDeviceModal: createCallback("device", "openDeviceModal"),
    submitOrgTemplateForm: createCallback("organization", "submitOrgTemplateForm"),
    submitNetworkRegionForm: createCallback("networks", "submitNetworkRegionForm"),
    submitNetworkForm: createCallback("networks", "submitNetworkForm"),
    submitRoomForm: createCallback("room", "submitRoomForm"),
    submitWorkstationForm: createCallback("workstation", "submitWorkstationForm"),
    submitCabinetForm: createCallback("cabinet", "submitCabinetForm"),
    submitCabinetPositionForm: createCallback("position", "submitCabinetPositionForm"),
    submitUserForm: createCallback("userManager", "submitUserForm"),
    submitOrgForm: createCallback("organization", "submitOrgForm"),
    submitCableLinkForm: createCallback("cableLink", "submitCableLinkForm"),
    submitDeviceForm: createCallback("device", "submitDeviceForm")
  };
}

async function initResourcePreloading() {
  // apiClient/toast/confirm/formatter/ui 已在 app.js 静态导入图中随主入口并行加载，
  // 无需重复 modulepreload；此处只预热纯动态加载的业务模块
  schedulePreload(
    [
      "resourceTabs",
      "networks",
      "room",
      "workstation",
      "cabinet",
      "position",
      "cableLink",
      "device",
      "ipDetail",
      "visualizationManager"
    ],
    { delay: 2000, priority: "low" }
  );

  lazyLoad("dashboard", { when: "idle" });

  // idle 分批预热全部模态框 HTML 进内存缓存，首开任意弹框零网络等待
  prefetchModalsOnIdle();
}

// 等待所有样式表加载完成，避免 FOUC
function waitForStylesheets() {
  const links = document.querySelectorAll('link[rel="stylesheet"]');
  const promises = Array.from(links).map((link) => {
    if (link.sheet) {
      try {
        link.sheet.cssRules;
        return Promise.resolve();
      } catch {
        // cross-origin stylesheet, treat as loaded
        return Promise.resolve();
      }
    }
    return new Promise((resolve) => {
      link.addEventListener("load", resolve, { once: true });
      link.addEventListener("error", resolve, { once: true });
    });
  });
  return Promise.all(promises);
}

// 启动应用
document.addEventListener("DOMContentLoaded", async () => {
  await waitForStylesheets();
  initApp();
});
