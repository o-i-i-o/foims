/**
 * 模态框管理模块
 * 提供统一的模态框操作和事件处理
 */

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
};

export function openModal(modalId, title = "") {
  const modal = document.getElementById(modalId);
  
  if (!modal) {
    return;
  }
  
  modal.classList.add("active");
  
  if (title) {
    const titleElement = modal.querySelector(".modal-title");
    if (titleElement) {
      titleElement.textContent = title;
    }
  }
  
  document.body.style.overflow = "hidden";
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
