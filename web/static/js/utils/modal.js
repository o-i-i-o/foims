import { loadModal, openModal as _openModal, closeModal as _closeModal, initModalTemplates } from './modalLoader.js';

const BUTTON_CALLBACK_MAP = {
  "add-network-type-btn": "openNetworkTypeModal",
  "add-network-btn": "openNetworkModal",
  "add-room-btn": "openRoomModal",
  "add-cabinet-btn": "openCabinetModal",
  "add-user-btn": "openUserModal",
  "add-net-outlet-btn": "openNetOutletModal",
  "add-device-btn": "openDeviceModal",
};

const FORM_CALLBACK_MAP = {
  "network-type-form": "submitNetworkTypeForm",
  "network-form": "submitNetworkForm",
  "room-form": "submitRoomForm",
  "workstation-form": "submitWorkstationForm",
  "cabinet-form": "submitCabinetForm",
  "cabinet-position-form": "submitCabinetPositionForm",
  "user-form": "submitUserForm",
  "device-port-form-expanded": "submitDevicePortForm",
  "organization-form": "submitOrgForm",
  "org-template-editor-form": "submitOrgTemplateForm",
  "net-outlet-form": "submitNetOutletForm",
  "device-form": "submitDeviceForm",
};

export function openModal(modalId, title = "") {
  return _openModal(modalId, title);
}

export function closeModal(modalId) {
  _closeModal(modalId);
}

export function initModals(callbacks = {}) {
  initClickHandlers(callbacks);
  initSubmitHandlers(callbacks);
}

function initClickHandlers(callbacks) {
  document.addEventListener("click", (e) => {
    if (e.target.classList.contains("modal")) {
      closeModal(e.target.id);
      return;
    }
    
    const buttonId = e.target.id;
    const callbackName = BUTTON_CALLBACK_MAP[buttonId];
    
    if (callbackName && callbacks[callbackName]) {
      e.preventDefault();
      callbacks[callbackName]();
      return;
    }
    
    if (e.target.type === "submit" && e.target.hasAttribute("form")) {
      const formId = e.target.getAttribute("form");
      const callbackName = FORM_CALLBACK_MAP[formId];
      
      if (callbackName && callbacks[callbackName]) {
        e.preventDefault();
        callbacks[callbackName]();
      }
    }
  });
}

function initSubmitHandlers(callbacks) {
  document.addEventListener("submit", (e) => {
    const formId = e.target.id;
    const callbackName = FORM_CALLBACK_MAP[formId];
    
    if (callbackName && callbacks[callbackName]) {
      e.preventDefault();
      callbacks[callbackName]();
    }
  });
}

export { initModalTemplates, loadModal };
