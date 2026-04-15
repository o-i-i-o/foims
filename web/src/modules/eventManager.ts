import { loadModule } from "../utils/moduleLoader.js";
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

type EditDeleteFn = (id: string | number) => void;

const EDIT_FUNCTIONS: Record<string, EditDeleteFn> = {
  "network-types-table": editNetworkType,
  "networks-table": editNetwork,
  "rooms-table": editRoom,
  "workstations-table": editWorkstation,
  "cabinets-table": editCabinet,
  "cabinet-positions-table": editCabinetPosition,
  "switches-table": editSwitch,
  "users-table": async (id) => {
    const mod = await loadModule<Record<string, EditDeleteFn>>("userManager", "/static/js/modules/userManager.js");
    mod.openUserModal(id);
  },
};

const DELETE_FUNCTIONS: Record<string, EditDeleteFn> = {
  "network-types-table": deleteNetworkType,
  "networks-table": deleteNetwork,
  "rooms-table": deleteRoom,
  "workstations-table": deleteWorkstation,
  "cabinets-table": deleteCabinet,
  "cabinet-positions-table": deleteCabinetPosition,
  "switches-table": deleteSwitch,
  "switch-ports-table": deleteSwitchPort,
  "users-table": async (id) => {
    const mod = await loadModule<Record<string, EditDeleteFn>>("userManager", "/static/js/modules/userManager.js");
    mod.deleteUser(id);
  },
};

const moduleCache: Record<string, Record<string, (...args: unknown[]) => unknown>> = {};

const MODULE_PATHS: Record<string, string> = {
  log: "/static/js/modules/log.js",
  systemManager: "/static/js/modules/systemManager.js",
  switchDevice: "/static/js/modules/switch/switchDevice.js",
  userManager: "/static/js/modules/userManager.js",
  authManager: "/static/js/modules/authManager.js",
  i18n: "/static/js/utils/i18n.js",
};

async function preloadModules(): Promise<void> {
  const loadPromises = Object.entries(MODULE_PATHS).map(async ([name, path]) => {
    moduleCache[name] = await loadModule<Record<string, (...args: unknown[]) => unknown>>(name, path);
  });
  await Promise.all(loadPromises);
}

function getModule(name: string): Record<string, (...args: unknown[]) => unknown> {
  return moduleCache[name];
}

export async function initEventListeners(): Promise<void> {
  await preloadModules();
  initButtonEventBindings();
  initGlobalClickHandlers();
  initSelectChangeHandlers();
}

interface ButtonEventBinding {
  id: string;
  event: string;
  handler: (e: Event) => void;
}

const BUTTON_EVENT_BINDINGS: ButtonEventBinding[] = [
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
      const filter = document.getElementById("notifications-filter") as HTMLSelectElement | null;
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
    handler: (e: Event) => {
      const { changeLanguage } = getModule("i18n");
      changeLanguage((e.target as HTMLSelectElement).value);
    },
  },
  {
    id: "test-snmp-btn",
    event: "click",
    handler: () => {
      const switchDevice = getModule("switchDevice");
      if (switchDevice?.testSnmpConnection) {
        switchDevice.testSnmpConnection();
      }
    },
  },
  {
    id: "get-snmp-info-btn",
    event: "click",
    handler: () => {
      const switchDevice = getModule("switchDevice");
      if (switchDevice?.getSwitchInfoFromSnmp) {
        switchDevice.getSwitchInfoFromSnmp();
      }
    },
  },
];

function initButtonEventBindings(): void {
  const clickHandlers: Record<string, (e: Event) => void> = {};
  const otherHandlers: ButtonEventBinding[] = [];

  BUTTON_EVENT_BINDINGS.forEach(({ id, event, handler }) => {
    if (event === "click") {
      clickHandlers[id] = handler;
    } else {
      otherHandlers.push({ id, event, handler });
    }
  });

  document.addEventListener("click", (e: MouseEvent) => {
    const target = e.target as HTMLElement;
    const handler = clickHandlers[target.id];
    if (handler) {
      handler(e);
    }
  });

  otherHandlers.forEach(({ id, event, handler }) => {
    const element = document.getElementById(id);
    element?.addEventListener(event, handler);
  });
}

function initGlobalClickHandlers(): void {
  document.addEventListener("click", (e: MouseEvent) => {
    handleModalCloseClick(e);
    handleUsageButtonClick(e);
    handleEditDeleteClick(e);
  });
}

function handleModalCloseClick(e: MouseEvent): void {
  const target = e.target as HTMLElement;
  if (!target.hasAttribute("data-modal-id")) {
    return;
  }
  const modalId = target.getAttribute("data-modal-id");
  if (modalId) {
    closeModal(modalId);
  }
}

function handleUsageButtonClick(e: MouseEvent): void {
  const target = e.target as HTMLElement;
  if (!target.classList.contains("btn-usage")) {
    return;
  }

  const id = target.dataset.id;
  if (!id || id === "undefined") {
    showToast("操作失败：缺少ID参数", "error");
    return;
  }

  const table = target.closest("table");
  if (table?.id === "networks-table") {
    showNetworkUsage(id);
  }
}

function handleEditDeleteClick(e: MouseEvent): void {
  const target = e.target as HTMLElement;
  const isEdit = target.classList.contains("btn-edit");
  const isDelete = target.classList.contains("btn-delete");

  if (!isEdit && !isDelete) {
    return;
  }

  e.preventDefault();

  const id = target.dataset.id;
  const table = target.closest("table");
  const tableId = table?.id;

  if (!id || id === "undefined") {
    showToast("操作失败：缺少ID参数", "error");
    return;
  }

  if (isEdit) {
    const editFunction = EDIT_FUNCTIONS[tableId || ""];
    editFunction?.(id);
  } else if (isDelete) {
    const deleteFunction = DELETE_FUNCTIONS[tableId || ""];
    deleteFunction?.(id);
  }
}

function initSelectChangeHandlers(): void {
  const notificationsFilter = document.getElementById("notifications-filter");

  notificationsFilter?.addEventListener("change", (e: Event) => {
    const { loadNotificationsData } = getModule("log");
    loadNotificationsData((e.target as HTMLSelectElement).value);
  });
}
