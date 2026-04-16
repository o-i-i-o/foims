/**
 * 事件管理模块
 * 处理全局事件绑定和委托
 */

import { loadModule, getCachedModule } from "../utils/moduleLoader.js";
import { showToast } from "../utils/ui.js";
import { closeModal } from "../utils/modal.js";
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
import { editSwitch, deleteSwitch, deleteSwitchPort } from "./switch/switchDevice.js";

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
 * 预加载模块缓存
 */
const MODULE_PATHS = {
  log: "/static/js/modules/log.js",
  systemManager: "/static/js/modules/systemManager.js",
  switchDevice: "/static/js/modules/switch/switchDevice.js",
  userManager: "/static/js/modules/userManager.js",
  authManager: "/static/js/modules/authManager.js",
  i18n: "/static/js/utils/i18n.js",
};

async function preloadModules() {
  await Promise.all(
    Object.entries(MODULE_PATHS).map(([name, path]) => loadModule(name, path))
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
    id: "register-service-btn",
    event: "click",
    handler: () => {
      const { registerService } = getModule("systemManager");
      registerService();
    },
  },
  {
    id: "restart-app-btn",
    event: "click",
    handler: () => {
      const { restartApplication } = getModule("systemManager");
      restartApplication();
    },
  },
  {
    id: "restart-os-btn",
    event: "click",
    handler: () => {
      const { restartOs } = getModule("systemManager");
      restartOs();
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
  {
    id: "test-snmp-btn",
    event: "click",
    handler: () => {
      const switchDevice = getModule("switchDevice");
      if (switchDevice && switchDevice.testSnmpConnection) {
        switchDevice.testSnmpConnection();
      }
    },
  },
  {
    id: "get-snmp-info-btn",
    event: "click",
    handler: () => {
      const switchDevice = getModule("switchDevice");
      if (switchDevice && switchDevice.getSwitchInfoFromSnmp) {
        switchDevice.getSwitchInfoFromSnmp();
      }
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
  
  notificationsFilter?.addEventListener("change", (e) => {
    const { loadNotificationsData } = getModule("log");
    loadNotificationsData(e.target.value);
  });
}
