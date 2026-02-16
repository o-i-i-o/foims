/**
 * 事件管理模块
 * 处理全局事件绑定和委托
 */

import { loadModule } from "../utils/moduleLoader.js";
import { showToast } from "../utils/ui.js";
import {
  editNetworkType,
  deleteNetworkType,
  editNetwork,
  deleteNetwork,
  showNetworkUsage,
} from "./networks.js";
import { editRoom, deleteRoom } from "./room.js";
import { editWorkstation, deleteWorkstation } from "./workstation.js";
import { editCabinet, deleteCabinet } from "./cabinet.js";
import { editCabinetPosition, deleteCabinetPosition } from "./position.js";
import { editSwitch, deleteSwitch, deleteSwitchPort } from "./switchManager.js";

// ==========================================
// 编辑/删除函数映射
// ==========================================

const EDIT_FUNCTIONS = {
  "network-types-table": editNetworkType,
  "networks-table": editNetwork,
  "rooms-table": editRoom,
  "workstations-table": editWorkstation,
  "cabinets-table": editCabinet,
  "cabinet-positions-table": editCabinetPosition,
  "switches-table": editSwitch,
  "users-table": async (id) => {
    const { openUserModal } = await loadModule("userManager", "/static/js/modules/userManager.js");
    openUserModal(id);
  },
};

const DELETE_FUNCTIONS = {
  "network-types-table": deleteNetworkType,
  "networks-table": deleteNetwork,
  "rooms-table": deleteRoom,
  "workstations-table": deleteWorkstation,
  "cabinets-table": deleteCabinet,
  "cabinet-positions-table": deleteCabinetPosition,
  "switches-table": deleteSwitch,
  "switch-ports-table": deleteSwitchPort,
  "users-table": async (id) => {
    const { deleteUser } = await loadModule("userManager", "/static/js/modules/userManager.js");
    deleteUser(id);
  },
};

// ==========================================
// 初始化
// ==========================================

/**
 * 初始化所有事件监听器
 */
export function initEventListeners() {
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
    handler: async () => {
      const { loadLogsData } = await loadModule("log", "/static/js/modules/log.js");
      const activeTabBtn = document.querySelector("#logs .tab-btn.active");
      const logType = activeTabBtn?.getAttribute("data-tab") || "operation";
      loadLogsData(logType);
    },
  },
  {
    id: "refresh-notifications-btn",
    event: "click",
    handler: async () => {
      const { loadNotificationsData } = await loadModule("log", "/static/js/modules/log.js");
      const filter = document.getElementById("notifications-filter");
      loadNotificationsData(filter?.value || "all");
    },
  },
  {
    id: "clear-read-notifications-btn",
    event: "click",
    handler: async () => {
      const { clearReadNotifications } = await loadModule("log", "/static/js/modules/log.js");
      clearReadNotifications();
    },
  },
  {
    id: "save-mac-notification-email-btn",
    event: "click",
    handler: async () => {
      const { saveMacNotificationEmail } = await loadModule("log", "/static/js/modules/log.js");
      await saveMacNotificationEmail();
    },
  },
  {
    id: "test-smtp-btn",
    event: "click",
    handler: async () => {
      const { testSmtpConnection } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      testSmtpConnection();
    },
  },
  {
    id: "import-csv-btn",
    event: "click",
    handler: async () => {
      const { importCsvData } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      importCsvData();
    },
  },
  {
    id: "download-template-btn",
    event: "click",
    handler: async () => {
      const { downloadTemplate } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      downloadTemplate("csv");
    },
  },
  {
    id: "export-csv-btn",
    event: "click",
    handler: async () => {
      const { exportCsvData } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      exportCsvData();
    },
  },
  {
    id: "export-database-btn",
    event: "click",
    handler: async () => {
      const { exportDatabase } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      exportDatabase();
    },
  },
  {
    id: "import-database-btn",
    event: "click",
    handler: async () => {
      const { importDatabase } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      importDatabase();
    },
  },
  {
    id: "backup-config-btn",
    event: "click",
    handler: async () => {
      const { backupConfig } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      backupConfig();
    },
  },
  {
    id: "restore-config-btn",
    event: "click",
    handler: async () => {
      const { restoreConfig } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      restoreConfig();
    },
  },
  {
    id: "register-service-btn",
    event: "click",
    handler: async () => {
      const { registerService } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      registerService();
    },
  },
  {
    id: "restart-app-btn",
    event: "click",
    handler: async () => {
      const { restartApplication } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      restartApplication();
    },
  },
  {
    id: "restart-os-btn",
    event: "click",
    handler: async () => {
      const { restartOs } = await loadModule("systemManager", "/static/js/modules/systemManager.js");
      restartOs();
    },
  },
  {
    id: "add-user-btn",
    event: "click",
    handler: async () => {
      const { openUserModal } = await loadModule("userManager", "/static/js/modules/userManager.js");
      openUserModal();
    },
  },
  {
    id: "logout-btn",
    event: "click",
    handler: async () => {
      const { logoutUser } = await loadModule("authManager", "/static/js/modules/authManager.js");
      logoutUser();
    },
  },
  {
    id: "language-selector",
    event: "change",
    handler: async (e) => {
      const { changeLanguage } = await loadModule("i18n", "/static/js/utils/i18n.js");
      changeLanguage(e.target.value);
    },
  },
];

/**
 * 初始化按钮事件绑定
 */
function initButtonEventBindings() {
  BUTTON_EVENT_BINDINGS.forEach(({ id, event, handler }) => {
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
  document.addEventListener("click", async (e) => {
    await handleModalCloseClick(e);
    handleUsageButtonClick(e);
    handleEditDeleteClick(e);
  });
}

/**
 * 处理模态框关闭按钮点击
 * @param {Event} e - 点击事件
 */
async function handleModalCloseClick(e) {
  if (!e.target.hasAttribute("data-modal-id")) {
    return;
  }
  
  const { closeModal } = await import("../utils/modal.js");
  const modalId = e.target.getAttribute("data-modal-id");
  closeModal(modalId);
}

/**
 * 处理使用情况按钮点击
 * @param {Event} e - 点击事件
 */
function handleUsageButtonClick(e) {
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
    showNetworkUsage(id);
  }
}

/**
 * 处理编辑和删除按钮点击
 * @param {Event} e - 点击事件
 */
function handleEditDeleteClick(e) {
  const isEdit = e.target.classList.contains("btn-edit");
  const isDelete = e.target.classList.contains("btn-delete");
  
  if (!isEdit && !isDelete) {
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
    const editFunction = EDIT_FUNCTIONS[tableId];
    editFunction?.(id);
  } else if (isDelete) {
    const deleteFunction = DELETE_FUNCTIONS[tableId];
    deleteFunction?.(id);
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
  
  notificationsFilter?.addEventListener("change", async (e) => {
    const { loadNotificationsData } = await loadModule("log", "/static/js/modules/log.js");
    loadNotificationsData(e.target.value);
  });
}
