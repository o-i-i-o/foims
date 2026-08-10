/**
 * 事件管理模块
 * 处理全局事件绑定和委托
 */

import { loadModule, getCachedModule } from "../utils/resourceLoader.js";
import { showToast } from "../utils/ui.js";
import { closeModal } from "../utils/modal.js";

// ==========================================
// 编辑/删除函数映射（动态加载）
// ==========================================

const EDIT_FUNCTIONS = {
  "network-types-table": { module: "networks", fn: "editNetworkType" },
  "networks-table": { module: "networks", fn: "editNetwork" },
  "rooms-table": { module: "room", fn: "editRoom" },
  "cabinets-table": { module: "cabinet", fn: "editCabinet" },
  "net-outlets-table": { module: "netOutlet", fn: "editNetOutlet" },
  "cable-links-table": { module: "cableLink", fn: "editCableLink" },
  "devices-table": { module: "device", fn: "editDevice" },
  "users-table": { module: "userManager", fn: "openUserModal" },
};

const DELETE_FUNCTIONS = {
  "network-types-table": { module: "networks", fn: "deleteNetworkType" },
  "networks-table": { module: "networks", fn: "deleteNetwork" },
  "rooms-table": { module: "room", fn: "deleteRoom" },
  "cabinets-table": { module: "cabinet", fn: "deleteCabinet" },
  "net-outlets-table": { module: "netOutlet", fn: "deleteNetOutlet" },
  "cable-links-table": { module: "cableLink", fn: "deleteCableLink" },
  "devices-table": { module: "device", fn: "deleteDevice" },
  "device-ports-table": { module: "devicePorts", fn: "deleteDevicePort" },
  "users-table": { module: "userManager", fn: "deleteUser" },
};

// ==========================================
// 初始化
// ==========================================

/**
 * 预加载模块缓存
 */
const PRELOAD_MODULES = ['log', 'systemManager', 'userManager', 'authManager', 'i18n'];

async function preloadModules() {
  await Promise.all(
    PRELOAD_MODULES.map(name => loadModule(name))
  );
}

function getModule(name) {
  return getCachedModule(name);
}

/**
 * 初始化所有事件监听器
 */
export async function initEventListeners() {
  await preloadModules();
  initButtonEventBindings();
  initGlobalClickHandlers();
  initSelectChangeHandlers();
}

// ==========================================
// 按钮事件绑定
// ==========================================

const BUTTON_EVENT_BINDINGS = [
  {
    id: "refresh-logs-btn",
    event: "click",
    handler: () => {
      const { loadLogsData } = getModule("log");
      const activeTabBtn = document.querySelector("#logs .tab-btn.active");
      const logType = activeTabBtn?.getAttribute("data-tab") || "operation";
      loadLogsData(logType);
    },
  },
  {
    id: "refresh-notifications-btn",
    event: "click",
    handler: () => {
      const { loadNotificationsData } = getModule("log");
      const filter = document.getElementById("notifications-filter");
      loadNotificationsData(filter?.value || "all");
    },
  },
  {
    id: "clear-read-notifications-btn",
    event: "click",
    handler: () => {
      const { clearReadNotifications } = getModule("log");
      clearReadNotifications();
    },
  },
  {
    id: "save-mac-notification-email-btn",
    event: "click",
    handler: async () => {
      const { saveMacNotificationEmail } = getModule("log");
      await saveMacNotificationEmail();
    },
  },
  {
    id: "test-smtp-btn",
    event: "click",
    handler: () => {
      const { testSmtpConnection } = getModule("systemManager");
      testSmtpConnection();
    },
  },
  {
    id: "import-csv-btn",
    event: "click",
    handler: () => {
      const { importCsvData } = getModule("systemManager");
      importCsvData();
    },
  },
  {
    id: "download-template-btn",
    event: "click",
    handler: () => {
      const { downloadTemplate } = getModule("systemManager");
      downloadTemplate();
    },
  },
  {
    id: "export-csv-btn",
    event: "click",
    handler: () => {
      const { exportCsvData } = getModule("systemManager");
      exportCsvData();
    },
  },
  {
    id: "export-database-btn",
    event: "click",
    handler: () => {
      const { exportDatabase } = getModule("systemManager");
      exportDatabase();
    },
  },
  {
    id: "backup-config-btn",
    event: "click",
    handler: () => {
      const { backupConfig } = getModule("systemManager");
      backupConfig();
    },
  },
  {
    id: "restore-config-btn",
    event: "click",
    handler: () => {
      const { restoreConfig } = getModule("systemManager");
      restoreConfig();
    },
  },
  {
    id: "add-user-btn",
    event: "click",
    handler: () => {
      const { openUserModal } = getModule("userManager");
      openUserModal();
    },
  },
  {
    id: "logout-btn",
    event: "click",
    handler: () => {
      const { logoutUser } = getModule("authManager");
      logoutUser();
    },
  },
  {
    id: "language-selector",
    event: "change",
    handler: (e) => {
      const { changeLanguage } = getModule("i18n");
      changeLanguage(e.target.value);
    },
  },
];

/**
 * 初始化按钮事件绑定
 */
function initButtonEventBindings() {
  const clickHandlers = {};
  const otherHandlers = [];

  BUTTON_EVENT_BINDINGS.forEach(({ id, event, handler }) => {
    if (event === 'click') {
      clickHandlers[id] = handler;
    } else {
      otherHandlers.push({ id, event, handler });
    }
  });

  document.addEventListener('click', (e) => {
    const handler = clickHandlers[e.target.id];
    if (handler) {
      handler(e);
    }
  });

  otherHandlers.forEach(({ id, event, handler }) => {
    const element = document.getElementById(id);
    element?.addEventListener(event, handler);
  });
}

// ==========================================
// 全局点击处理
// ==========================================

/**
 * 初始化全局点击处理器
 */
function initGlobalClickHandlers() {
  document.addEventListener("click", (e) => {
    handleModalCloseClick(e);
    handleUsageButtonClick(e);
    handleEditDeleteClick(e);
  });
}

/**
 * 处理模态框关闭按钮点击
 * @param {Event} e - 点击事件
 */
function handleModalCloseClick(e) {
  if (!e.target.hasAttribute("data-modal-id")) {
    return;
  }
  
  const modalId = e.target.getAttribute("data-modal-id");
  closeModal(modalId);
}

/**
 * 处理使用情况按钮点击
 * @param {Event} e - 点击事件
 */
async function handleUsageButtonClick(e) {
  if (!e.target.classList.contains("btn-usage")) {
    return;
  }

  const button = e.target;
  const id = button.dataset.id;

  if (!id || id === "undefined") {
    showToast("操作失败：缺少ID参数", "error");
    return;
  }

  const table = button.closest("table");

  if (table?.id === "networks-table") {
    const { showNetworkUsage } = await loadModule("networks");
    showNetworkUsage(id);
  }
}

/**
 * 处理编辑和删除按钮点击
 * @param {Event} e - 点击事件
 */
async function handleEditDeleteClick(e) {
  const isEdit = e.target.classList.contains("btn-edit");
  const isDelete = e.target.classList.contains("btn-delete");

  if (!isEdit && !isDelete) {
    return;
  }

  // 有 data-action 属性的按钮由模块自身的事件处理器处理，跳过全局处理
  if (e.target.dataset.action) {
    return;
  }

  e.preventDefault();

  const button = e.target;
  const id = button.dataset.id;
  const table = button.closest("table");
  const tableId = table?.id;

  if (!id || id === "undefined") {
    showToast("操作失败：缺少ID参数", "error");
    return;
  }

  if (isEdit) {
    const config = EDIT_FUNCTIONS[tableId];
    if (config) {
      const module = await loadModule(config.module);
      module[config.fn]?.(id);
    }
  } else if (isDelete) {
    const config = DELETE_FUNCTIONS[tableId];
    if (config) {
      const module = await loadModule(config.module);
      module[config.fn]?.(id);
    }
  }
}

// ==========================================
// 选择器变化处理
// ==========================================

/**
 * 初始化选择器变化处理器
 */
function initSelectChangeHandlers() {
  const notificationsFilter = document.getElementById("notifications-filter");
  
  notificationsFilter?.addEventListener("change", (e) => {
    const { loadNotificationsData } = getModule("log");
    loadNotificationsData(e.target.value);
  });
}
