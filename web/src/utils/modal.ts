import { loadModal } from "./modalLoader.js";

const BUTTON_CALLBACK_MAP: Record<string, string> = {
  "add-network-type-btn": "openNetworkTypeModal",
  "add-network-btn": "openNetworkModal",
  "add-room-btn": "openRoomModal",
  "add-workstation-btn": "openWorkstationModal",
  "add-cabinet-btn": "openCabinetModal",
  "add-cabinet-position-btn": "openCabinetPositionModal",
  "pull-ip-btn": "pullIpMacData",
  "add-user-btn": "openUserModal",
  "add-switch-btn": "openSwitchModal",
};

const FORM_CALLBACK_MAP: Record<string, string> = {
  "network-type-form": "submitNetworkTypeForm",
  "network-form": "submitNetworkForm",
  "room-form": "submitRoomForm",
  "workstation-form": "submitWorkstationForm",
  "cabinet-form": "submitCabinetForm",
  "cabinet-position-form": "submitCabinetPositionForm",
  "user-form": "submitUserForm",
  "switch-form": "submitSwitchForm",
  "switch-port-form": "submitSwitchPortForm",
  "switch-port-form-expanded": "submitSwitchPortForm",
};

const SUBMIT_BUTTON_MAP: Record<string, string> = {
  "network-type-modal": "network-type-form",
  "network-modal": "network-form",
  "room-modal": "room-form",
  "workstation-modal": "workstation-form",
  "cabinet-modal": "cabinet-form",
  "cabinet-position-modal": "cabinet-position-form",
  "user-modal": "user-form",
  "switch-modal": "switch-form",
  "switch-port-modal": "switch-port-form",
  "switch-port-detail-modal": "switch-port-form-expanded",
};

type ModalCallback = () => void;

export function openModal(modalId: string, title = ""): HTMLElement | null {
  let modal = document.getElementById(modalId);

  if (!modal) {
    modal = loadModal(modalId);
  }

  if (!modal) {
    return null;
  }

  modal.classList.add("active");

  if (title) {
    const titleElement = modal.querySelector(".modal-title") as HTMLElement | null;
    if (titleElement) {
      titleElement.textContent = title;
    }
  }

  document.body.style.overflow = "hidden";

  return modal;
}

export function closeModal(modalId: string): void {
  const modal = document.getElementById(modalId);

  if (!modal) {
    return;
  }

  modal.classList.remove("active");
  resetModalForm(modal);

  document.body.style.overflow = "";
}

function resetModalForm(modal: HTMLElement): void {
  const form = modal.querySelector("form") as HTMLFormElement | null;

  if (!form) {
    return;
  }

  form.reset();

  const hiddenIdField = form.querySelector('input[type="hidden"]') as HTMLInputElement | null;
  if (hiddenIdField) {
    hiddenIdField.value = "";
  }

  const ipContainers = modal.querySelectorAll('[id$="-ips-container"]');
  ipContainers.forEach(container => {
    container.innerHTML = "";
  });
}

export function initModals(callbacks: Record<string, ModalCallback> = {}): void {
  initClickHandlers(callbacks);
  initSubmitHandlers(callbacks);
}

function initClickHandlers(callbacks: Record<string, ModalCallback>): void {
  document.addEventListener("click", (e: MouseEvent) => {
    const target = e.target as HTMLElement;
    if (target.classList.contains("modal")) {
      closeModal(target.id);
      return;
    }

    const buttonId = target.id;
    const callbackName = BUTTON_CALLBACK_MAP[buttonId];

    if (callbackName && callbacks[callbackName]) {
      e.preventDefault();
      callbacks[callbackName]();
      return;
    }

    if ((target as HTMLButtonElement).type === "submit" && target.hasAttribute("form")) {
      const formId = target.getAttribute("form")!;
      const formCallbackName = FORM_CALLBACK_MAP[formId];

      if (formCallbackName && callbacks[formCallbackName]) {
        e.preventDefault();
        callbacks[formCallbackName]();
      }
    }
  });
}

function initSubmitHandlers(callbacks: Record<string, ModalCallback>): void {
  document.addEventListener("submit", (e: SubmitEvent) => {
    const formId = (e.target as HTMLFormElement).id;
    const callbackName = FORM_CALLBACK_MAP[formId];

    if (callbackName && callbacks[callbackName]) {
      e.preventDefault();
      callbacks[callbackName]();
    }
  });
}

export { BUTTON_CALLBACK_MAP, FORM_CALLBACK_MAP, SUBMIT_BUTTON_MAP };
