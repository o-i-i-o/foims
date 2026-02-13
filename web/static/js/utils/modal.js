/**
 * 模态框管理模块
 * 提供统一的模态框操作和事件处理
 */

// ==========================================
// 常量定义
// ==========================================

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

// ==========================================
// 模态框操作
// ==========================================

/**
 * 打开模态框
 * @param {string} modalId - 模态框 ID
 * @param {string} title - 模态框标题（可选）
 */
export function openModal(modalId, title = "") {
  const modal = document.getElementById(modalId);
  
  if (!modal) {
    console.error(`模态框不存在: ${modalId}`);
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

/**
 * 关闭模态框
 * @param {string} modalId - 模态框 ID
 */
export function closeModal(modalId) {
  const modal = document.getElementById(modalId);
  
  if (!modal) {
    return;
  }
  
  modal.classList.remove("active");
  resetModalForm(modal);
  
  document.body.style.overflow = "";
}

/**
 * 重置模态框表单
 * @param {HTMLElement} modal - 模态框元素
 */
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

// ==========================================
// 事件处理
// ==========================================

/**
 * 初始化模态框事件监听
 * @param {Object} callbacks - 回调函数对象
 */
export function initModals(callbacks = {}) {
  initClickHandlers(callbacks);
  initSubmitHandlers(callbacks);
}

/**
 * 初始化点击事件处理
 * @param {Object} callbacks - 回调函数对象
 */
function initClickHandlers(callbacks) {
  document.addEventListener("click", (e) => {
    // 处理模态框背景点击关闭
    if (e.target.classList.contains("modal")) {
      closeModal(e.target.id);
      return;
    }
    
    // 处理按钮点击
    const buttonId = e.target.id;
    const callbackName = BUTTON_CALLBACK_MAP[buttonId];
    
    if (callbackName && callbacks[callbackName]) {
      e.preventDefault();
      callbacks[callbackName]();
    }
  });
}

/**
 * 初始化表单提交事件处理
 * @param {Object} callbacks - 回调函数对象
 */
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

// ==========================================
// 便捷方法
// ==========================================

/**
 * 确认对话框
 * @param {string} message - 确认消息
 * @param {Function} onConfirm - 确认回调
 * @param {Function} onCancel - 取消回调
 */
export function confirm(message, onConfirm, onCancel) {
  if (window.confirm(message)) {
    onConfirm?.();
  } else {
    onCancel?.();
  }
}

/**
 * 显示加载中的模态框
 * @param {string} modalId - 模态框 ID
 * @param {boolean} isLoading - 是否加载中
 */
export function setModalLoading(modalId, isLoading) {
  const modal = document.getElementById(modalId);
  
  if (!modal) {
    return;
  }
  
  const submitBtn = modal.querySelector('button[type="submit"]');
  
  if (submitBtn) {
    submitBtn.disabled = isLoading;
    
    if (isLoading) {
      submitBtn.dataset.originalText = submitBtn.textContent;
      submitBtn.textContent = "处理中...";
    } else {
      submitBtn.textContent = submitBtn.dataset.originalText || "提交";
    }
  }
}
