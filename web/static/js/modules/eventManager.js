/**
 * 事件管理模块
 * 处理全局事件绑定和委托
 */

import { loadModule } from "../utils/resourceLoader.js";
import { t } from "../utils/i18n.js";
import { showToast } from "../utils/ui.js";
import { closeModal } from "../utils/modalLoader.js";

// ==========================================
// 编辑/删除函数映射（动态加载）
// ==========================================

const EDIT_FUNCTIONS = {
  "network-regions-table": { module: "networks", fn: "editNetworkRegion" },
  "networks-table": { module: "networks", fn: "editNetwork" },
  "rooms-table": { module: "room", fn: "editRoom" },
  "cabinets-table": { module: "cabinet", fn: "editCabinet" },
  "cable-links-table": { module: "cableLink", fn: "editCableLink" },
  "devices-table": { module: "device", fn: "editDevice" },
  "users-table": { module: "userManager", fn: "openUserModal" }
};

const DELETE_FUNCTIONS = {
  "network-regions-table": { module: "networks", fn: "deleteNetworkRegion" },
  "networks-table": { module: "networks", fn: "deleteNetwork" },
  "rooms-table": { module: "room", fn: "deleteRoom" },
  "cabinets-table": { module: "cabinet", fn: "deleteCabinet" },
  "cable-links-table": { module: "cableLink", fn: "deleteCableLink" },
  "devices-table": { module: "device", fn: "deleteDevice" },
  "users-table": { module: "userManager", fn: "deleteUser" }
};

// ==========================================
// 初始化
// ==========================================

/**
 * 预加载模块缓存
 * 仅预热纯动态加载的模块：userManager/authManager/i18n 已在 app.js 静态导入图中，
 * 再经 loadModule 加载会形成第二实例（独立状态），故不列入
 */
const PRELOAD_MODULES = ["log", "systemManager"];

/** idle 时预热日志/系统模块；事件绑定不再被模块加载阻塞 */
function preloadModulesOnIdle() {
  const run = () => {
    Promise.allSettled(PRELOAD_MODULES.map((name) => loadModule(name)));
  };
  if (typeof requestIdleCallback === "function") {
    requestIdleCallback(run, { timeout: 5000 });
  } else {
    setTimeout(run, 2000);
  }
}

/** 懒加载模块：loadModule 内部有缓存与并发去重，重复调用无额外开销 */
async function getModule(name) {
  return loadModule(name);
}

/**
 * 初始化所有事件监听器
 */
export function initEventListeners() {
  preloadModulesOnIdle();
  initButtonEventBindings();
  initGlobalClickHandlers();
}

// ==========================================
// 资源新建按钮 / 表单提交委托（回调由 app.js 提供懒加载实现）
// ==========================================

const RESOURCE_BUTTON_CALLBACK_MAP = {
  "add-network-region-btn": "openNetworkRegionModal",
  "add-network-btn": "openNetworkModal",
  "add-room-btn": "openRoomModal",
  "add-cabinet-btn": "openCabinetModal",
  "add-user-btn": "openUserModal",
  "add-cable-link-btn": "openCableLinkModal",
  "cable-label-print-btn": "openCableLabelPrintModal",
  "add-device-btn": "openDeviceModal"
};

const RESOURCE_FORM_CALLBACK_MAP = {
  "network-region-form": "submitNetworkRegionForm",
  "network-form": "submitNetworkForm",
  "room-form": "submitRoomForm",
  "workstation-form": "submitWorkstationForm",
  "cabinet-form": "submitCabinetForm",
  "cabinet-position-form": "submitCabinetPositionForm",
  "user-form": "submitUserForm",
  "organization-form": "submitOrgForm",
  "org-template-editor-form": "submitOrgTemplateForm",
  "cable-link-form": "submitCableLinkForm",
  "device-form": "submitDeviceForm"
};

/**
 * 初始化资源按钮与表单的委托监听
 * @param {Object} callbacks 懒加载回调集合（见 app.js getResourceCallbacks）
 */
export function initModals(callbacks = {}) {
  document.addEventListener("click", (e) => {
    if (e.target.classList.contains("modal")) {
      closeModal(e.target.id);
      return;
    }

    const callbackName = RESOURCE_BUTTON_CALLBACK_MAP[e.target.id];
    if (callbackName && callbacks[callbackName]) {
      e.preventDefault();
      callbacks[callbackName]();
      return;
    }

    // 页脚按钮通过 form 属性关联表单时，拦截 submit 类型按钮的点击
    if (e.target.type === "submit" && e.target.hasAttribute("form")) {
      const formCallbackName = RESOURCE_FORM_CALLBACK_MAP[e.target.getAttribute("form")];
      if (formCallbackName && callbacks[formCallbackName]) {
        e.preventDefault();
        callbacks[formCallbackName]();
      }
    }
  });

  document.addEventListener("submit", (e) => {
    const callbackName = RESOURCE_FORM_CALLBACK_MAP[e.target.id];
    if (callbackName && callbacks[callbackName]) {
      e.preventDefault();
      callbacks[callbackName]();
    }
  });
}

// ==========================================
// 按钮事件绑定
// ==========================================

const BUTTON_EVENT_BINDINGS = [
  {
    id: "mark-all-read-btn",
    event: "click",
    handler: async () => {
      const { markAllNotificationsRead } = await getModule("log");
      markAllNotificationsRead();
    }
  },
  {
    id: "test-smtp-btn",
    event: "click",
    handler: async () => {
      const { testSmtpConnection } = await getModule("systemManager");
      testSmtpConnection();
    }
  },
  {
    id: "test-ldap-btn",
    event: "click",
    handler: async () => {
      const { testLdapConnection } = await getModule("systemManager");
      testLdapConnection();
    }
  },
  {
    id: "test-sso-btn",
    event: "click",
    handler: async () => {
      const { testSsoConnection } = await getModule("systemManager");
      testSsoConnection();
    }
  },
  {
    id: "import-csv-btn",
    event: "click",
    handler: async () => {
      const { importCsvData } = await getModule("systemManager");
      importCsvData();
    }
  },
  {
    id: "download-template-btn",
    event: "click",
    handler: async () => {
      const { downloadTemplate } = await getModule("systemManager");
      downloadTemplate();
    }
  },
  {
    id: "export-csv-btn",
    event: "click",
    handler: async () => {
      const { exportCsvData } = await getModule("systemManager");
      exportCsvData();
    }
  },
  {
    id: "export-database-btn",
    event: "click",
    handler: async () => {
      const { exportDatabase } = await getModule("systemManager");
      exportDatabase();
    }
  },
  {
    id: "backup-config-btn",
    event: "click",
    handler: async () => {
      const { backupConfig } = await getModule("systemManager");
      backupConfig();
    }
  },
  {
    id: "restore-config-btn",
    event: "click",
    handler: async () => {
      const { restoreConfig } = await getModule("systemManager");
      restoreConfig();
    }
  }
  // logout-btn 不在此绑定：authManager.initLogout()（app.js 调用）已绑定同一按钮，
  // 此处再绑会导致每次退出触发两次 logoutUser（二次请求 401 竞态）
  // 语言切换由 languageMenu 模块负责（下拉选择，见 modules/languageMenu.js）
];

/**
 * 初始化按钮事件绑定
 */
function initButtonEventBindings() {
  const clickHandlers = {};
  const otherHandlers = [];

  BUTTON_EVENT_BINDINGS.forEach(({ id, event, handler }) => {
    if (event === "click") {
      clickHandlers[id] = handler;
    } else {
      otherHandlers.push({ id, event, handler });
    }
  });

  document.addEventListener("click", (e) => {
    // 图标按钮内含 SVG 子元素，点击目标是 svg/path，需向上查找带 id 的宿主元素
    const target = e.target.closest("[id]");
    const handler = target ? clickHandlers[target.id] : null;
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
  const button = e.target.closest(".btn-usage");

  if (!button) {
    return;
  }

  const id = button.dataset.id;

  if (!id || id === "undefined") {
    showToast(t("common.missing_id_param"), "error");
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
  // 图标按钮内嵌 SVG，e.target 可能是 svg/path，必须用 closest 定位按钮
  const button = e.target.closest(".btn-edit, .btn-delete");

  if (!button) {
    return;
  }

  const isEdit = button.classList.contains("btn-edit");
  const isDelete = button.classList.contains("btn-delete");

  // 有 data-action 属性的按钮由模块自身的事件处理器处理，跳过全局处理
  if (button.dataset.action) {
    return;
  }

  e.preventDefault();

  const id = button.dataset.id;
  const table = button.closest("table");
  const tableId = table?.id;

  if (!id || id === "undefined") {
    showToast(t("common.missing_id_param"), "error");
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
