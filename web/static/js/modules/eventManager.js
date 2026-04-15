import { closeModal } from "../utils/modal.js";
import { showToast } from "../utils/ui.js";
import { t } from "../utils/i18n.js";
import { editNetworkType, deleteNetworkType, editNetwork, deleteNetwork, showNetworkUsage, } from "./networks.js";
import { editRoom, deleteRoom } from "./room.js";
import { editWorkstation, deleteWorkstation } from "./workstation.js";
import { editCabinet, deleteCabinet } from "./cabinet.js";
import { editCabinetPosition, deleteCabinetPosition } from "./position.js";
import { editSwitch, deleteSwitch, deleteSwitchPort } from "./switch/switchDevice.js";
const EDIT_FUNCTIONS = {
    "network-types-table": editNetworkType,
    "networks-table": editNetwork,
    "rooms-table": editRoom,
    "workstations-table": editWorkstation,
    "cabinets-table": editCabinet,
    "cabinet-positions-table": editCabinetPosition,
    "switches-table": editSwitch,
    "users-table": async (id) => {
        const { openUserModal } = await import("./userManager.js");
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
        const { deleteUser } = await import("./userManager.js");
        deleteUser(id);
    },
};
const MODAL_SUBMIT_FUNCTIONS = {
    "network-type-form": async () => {
        const mod = await import("./networks.js");
        return mod.submitNetworkTypeForm();
    },
    "network-form": async () => {
        const mod = await import("./networks.js");
        return mod.submitNetworkForm();
    },
    "room-form": async () => {
        const mod = await import("./room.js");
        return mod.submitRoomForm();
    },
    "workstation-form": async () => {
        const mod = await import("./workstation.js");
        return mod.submitWorkstationForm();
    },
    "cabinet-form": async () => {
        const mod = await import("./cabinet.js");
        return mod.submitCabinetForm();
    },
    "cabinet-position-form": async () => {
        const mod = await import("./position.js");
        return mod.submitCabinetPositionForm();
    },
    "user-form": async () => {
        const mod = await import("./userManager.js");
        return mod.submitUserForm();
    },
};
const BUTTON_EVENT_BINDINGS = [
    {
        id: "refresh-logs-btn",
        event: "click",
        handler: () => {
            import("./log.js").then(({ loadLogsData }) => {
                const activeTabBtn = document.querySelector("#logs .tab-btn.active");
                const logType = activeTabBtn?.getAttribute("data-tab") || "operation";
                loadLogsData(logType);
            });
        },
    },
    {
        id: "refresh-notifications-btn",
        event: "click",
        handler: () => {
            import("./log.js").then(({ loadNotificationsData }) => {
                const filter = document.getElementById("notifications-filter");
                loadNotificationsData(filter?.value || "all");
            });
        },
    },
    {
        id: "clear-read-notifications-btn",
        event: "click",
        handler: () => {
            import("./log.js").then(({ clearReadNotifications }) => {
                clearReadNotifications();
            });
        },
    },
    {
        id: "save-mac-notification-email-btn",
        event: "click",
        handler: () => {
            import("./log.js").then(({ saveMacNotificationEmail }) => {
                saveMacNotificationEmail();
            });
        },
    },
    {
        id: "test-smtp-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ testSmtpConnection }) => {
                testSmtpConnection();
            });
        },
    },
    {
        id: "import-csv-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ importCsvData }) => {
                importCsvData();
            });
        },
    },
    {
        id: "download-template-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ downloadTemplate }) => {
                downloadTemplate();
            });
        },
    },
    {
        id: "export-csv-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ exportCsvData }) => {
                exportCsvData();
            });
        },
    },
    {
        id: "export-database-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ exportDatabase }) => {
                exportDatabase();
            });
        },
    },
    {
        id: "backup-config-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ backupConfig }) => {
                backupConfig();
            });
        },
    },
    {
        id: "restore-config-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ restoreConfig }) => {
                restoreConfig();
            });
        },
    },
    {
        id: "register-service-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ registerService }) => {
                registerService();
            });
        },
    },
    {
        id: "restart-app-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ restartApplication }) => {
                restartApplication();
            });
        },
    },
    {
        id: "restart-os-btn",
        event: "click",
        handler: () => {
            import("./systemManager.js").then(({ restartOs }) => {
                restartOs();
            });
        },
    },
    {
        id: "add-user-btn",
        event: "click",
        handler: () => {
            import("./userManager.js").then(({ openUserModal }) => {
                openUserModal();
            });
        },
    },
    {
        id: "language-selector",
        event: "change",
        handler: (e) => {
            import("../utils/i18n.js").then(({ changeLanguage }) => {
                changeLanguage(e.target.value);
            });
        },
    },
    {
        id: "test-snmp-btn",
        event: "click",
        handler: () => {
            import("./switch/switchSnmp.js").then(({ testSnmpConnection }) => {
                testSnmpConnection();
            });
        },
    },
    {
        id: "get-snmp-info-btn",
        event: "click",
        handler: () => {
            import("./switch/switchSnmp.js").then(({ getSwitchInfoFromSnmp }) => {
                getSwitchInfoFromSnmp();
            });
        },
    },
];
function initButtonEventBindings() {
    const clickHandlers = {};
    const otherHandlers = [];
    BUTTON_EVENT_BINDINGS.forEach(({ id, event, handler }) => {
        if (event === "click") {
            clickHandlers[id] = handler;
        }
        else {
            otherHandlers.push({ id, event, handler });
        }
    });
    document.addEventListener("click", (e) => {
        const target = e.target;
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
function initGlobalClickHandlers() {
    document.addEventListener("click", (e) => {
        handleModalCloseClick(e);
        handleUsageButtonClick(e);
        handleEditDeleteClick(e);
    });
}
function handleModalCloseClick(e) {
    const target = e.target;
    if (!target.hasAttribute("data-modal-id"))
        return;
    const modalId = target.getAttribute("data-modal-id");
    if (modalId)
        closeModal(modalId);
}
function handleUsageButtonClick(e) {
    const target = e.target;
    if (!target.classList.contains("btn-usage"))
        return;
    const id = target.dataset.id;
    if (!id || id === "undefined") {
        showToast(t("common.missing_id") || "操作失败：缺少ID参数", "error");
        return;
    }
    const table = target.closest("table");
    if (table?.id === "networks-table") {
        showNetworkUsage(id);
    }
}
function handleEditDeleteClick(e) {
    const target = e.target;
    const isEdit = target.classList.contains("btn-edit");
    const isDelete = target.classList.contains("btn-delete");
    if (!isEdit && !isDelete)
        return;
    e.preventDefault();
    const id = target.dataset.id;
    const table = target.closest("table");
    const tableId = table?.id;
    if (!id || id === "undefined") {
        showToast(t("common.missing_id") || "操作失败：缺少ID参数", "error");
        return;
    }
    if (isEdit) {
        const editFunction = EDIT_FUNCTIONS[tableId || ""];
        editFunction?.(id);
    }
    else if (isDelete) {
        const deleteFunction = DELETE_FUNCTIONS[tableId || ""];
        deleteFunction?.(id);
    }
}
function initSelectChangeHandlers() {
    const notificationsFilter = document.getElementById("notifications-filter");
    notificationsFilter?.addEventListener("change", (e) => {
        import("./log.js").then(({ loadNotificationsData }) => {
            loadNotificationsData(e.target.value);
        });
    });
}
export function initGlobalEvents() {
    document.addEventListener("click", handleGlobalClick);
}
function handleGlobalClick(e) {
    const target = e.target;
    handleTableButtonClick(target);
    handleModalSubmitClick(target);
    handleNetworkUsageClick(target);
}
function handleTableButtonClick(target) {
    const btn = target.closest(".btn-edit, .btn-delete");
    if (!btn)
        return;
    const table = btn.closest("table");
    if (!table)
        return;
    const tableId = table.id;
    const id = btn.getAttribute("data-id");
    if (!id)
        return;
    if (btn.classList.contains("btn-edit")) {
        const editFn = EDIT_FUNCTIONS[tableId];
        if (editFn)
            editFn(id);
    }
    else if (btn.classList.contains("btn-delete")) {
        const deleteFn = DELETE_FUNCTIONS[tableId];
        if (deleteFn)
            deleteFn(id);
    }
}
function handleModalSubmitClick(target) {
    const submitBtn = target.closest("[data-submit-form]");
    if (!submitBtn)
        return;
    const formId = submitBtn.getAttribute("data-submit-form");
    if (!formId)
        return;
    const submitFn = MODAL_SUBMIT_FUNCTIONS[formId];
    if (submitFn) {
        submitFn().then((result) => {
            if (result !== false) {
                const modalId = submitBtn.closest(".modal")?.id;
                if (modalId)
                    closeModal(modalId);
            }
        });
    }
}
function handleNetworkUsageClick(target) {
    const btn = target.closest(".btn-usage");
    if (!btn)
        return;
    const table = btn.closest("table");
    if (!table || table.id !== "networks-table")
        return;
    const id = btn.getAttribute("data-id");
    if (id)
        showNetworkUsage(id);
}
export function initEventListeners() {
    initButtonEventBindings();
    initGlobalClickHandlers();
    initSelectChangeHandlers();
    initGlobalEvents();
}
//# sourceMappingURL=eventManager.js.map