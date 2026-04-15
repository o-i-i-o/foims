/**
 * 模态框管理模块
 * 提供统一的模态框操作和事件处理
 */

import { loadModal } from './modalLoader.js';

const BUTTON_CALLBACK_MAP = {
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

const FORM_CALLBACK_MAP = {
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

const SUBMIT_BUTTON_MAP = {
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

export function openModal(modalId, title = "") {
  let modal = document.getElementById(modalId);
  
  if (!modal) {
    modal = loadModal(modalId);
  }
  
  if (!modal) {
    return null;
  }
  
  modal.classList.add("active");
  
  if (title) {
    const titleElement = modal.querySelector(".modal-title");
    if (titleElement) {
      titleElement.textContent = title;
    }
  }
  
  document.body.style.overflow = "hidden";
  
  return modal;
}

export function closeModal(modalId) {
  const modal = document.getElementById(modalId);
  
  if (!modal) {
    return;
  }
  
  modal.classList.remove("active");
  resetModalForm(modal);
  
  document.body.style.overflow = "";
}

function resetModalForm(modal) {
  const form = modal.querySelector("form");
  
  if (!form) {
    return;
  }
  
  form.reset();
  
  const hiddenIdField = form.querySelector('input[type="hidden"]');
  if (hiddenIdField) {
    hiddenIdField.value = "";
  }

  const ipContainers = modal.querySelectorAll('[id$="-ips-container"]');
  ipContainers.forEach(container => {
    container.innerHTML = '';
  });
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
