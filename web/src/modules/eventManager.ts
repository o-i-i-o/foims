import { loadModule } from "../utils/moduleLoader.js";
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

type EditDeleteFn = (id: string | number) => void | Promise<void> | Promise<boolean>;

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
};

const MODAL_SUBMIT_FUNCTIONS: Record<string, () => Promise<boolean | void>> = {
  "network-type-form": async () => {
    const mod = await loadModule<Record<string, () => Promise<boolean | void>>>("networks", "/static/js/modules/networks.js");
    return mod.submitNetworkTypeForm();
  },
  "network-form": async () => {
    const mod = await loadModule<Record<string, () => Promise<boolean | void>>>("networks", "/static/js/modules/networks.js");
    return mod.submitNetworkForm();
  },
  "room-form": async () => {
    const mod = await loadModule<Record<string, () => Promise<boolean | void>>>("room", "/static/js/modules/room.js");
    return mod.submitRoomForm();
  },
  "workstation-form": async () => {
    const mod = await loadModule<Record<string, () => Promise<boolean | void>>>("workstation", "/static/js/modules/workstation.js");
    return mod.submitWorkstationForm();
  },
  "cabinet-form": async () => {
    const mod = await loadModule<Record<string, () => Promise<boolean | void>>>("cabinet", "/static/js/modules/cabinet.js");
    return mod.submitCabinetForm();
  },
  "cabinet-position-form": async () => {
    const mod = await loadModule<Record<string, () => Promise<boolean | void>>>("position", "/static/js/modules/position.js");
    return mod.submitCabinetPositionForm();
  },
  "user-form": async () => {
    const mod = await loadModule<Record<string, () => Promise<boolean | void>>>("userManager", "/static/js/modules/userManager.js");
    return mod.submitUserForm();
  },
};

export function initGlobalEvents(): void {
  document.addEventListener("click", handleGlobalClick);
}

export function initEventListeners(): void {
  initGlobalEvents();
}

function handleGlobalClick(e: Event): void {
  const target = e.target as HTMLElement;

  handleTableButtonClick(target);
  handleModalSubmitClick(target);
  handleNetworkUsageClick(target);
}

function handleTableButtonClick(target: HTMLElement): void {
  const btn = target.closest(".btn-edit, .btn-delete, .btn-usage") as HTMLElement | null;
  if (!btn) return;

  const table = btn.closest("table") as HTMLTableElement | null;
  if (!table) return;

  const tableId = table.id;
  const id = btn.getAttribute("data-id");

  if (!id) return;

  if (btn.classList.contains("btn-edit")) {
    const editFn = EDIT_FUNCTIONS[tableId];
    if (editFn) {
      editFn(id);
    }
  } else if (btn.classList.contains("btn-delete")) {
    const deleteFn = DELETE_FUNCTIONS[tableId];
    if (deleteFn) {
      deleteFn(id);
    }
  }
}

function handleModalSubmitClick(target: HTMLElement): void {
  const submitBtn = target.closest("[data-submit-form]") as HTMLElement | null;
  if (!submitBtn) return;

  const formId = submitBtn.getAttribute("data-submit-form");
  if (!formId) return;

  const submitFn = MODAL_SUBMIT_FUNCTIONS[formId];
  if (submitFn) {
    submitFn().then((result) => {
      if (result !== false) {
        const modalId = submitBtn.closest(".modal")?.id;
        if (modalId) {
          closeModal(modalId);
        }
      }
    });
  }
}

function handleNetworkUsageClick(target: HTMLElement): void {
  const btn = target.closest(".btn-usage") as HTMLElement | null;
  if (!btn) return;

  const table = btn.closest("table") as HTMLTableElement | null;
  if (!table || table.id !== "networks-table") return;

  const id = btn.getAttribute("data-id");
  if (id) {
    showNetworkUsage(id);
  }
}
