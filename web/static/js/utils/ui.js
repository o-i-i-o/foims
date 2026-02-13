/**
 * UI 工具模块
 * 提供统一的 UI 操作和工具函数
 */

import { apiRequest, apiDelete } from "./apiClient.js";
import { closeModal } from "./modal.js";

// ==========================================
// 常量定义
// ==========================================

const DEFAULT_TOAST_DURATION = 3000;
const DEFAULT_DEBOUNCE_DELAY = 300;
const DEFAULT_THROTTLE_LIMIT = 300;

// ==========================================
// Toast 通知
// ==========================================

/**
 * 显示 Toast 通知
 * @param {string} message - 通知内容
 * @param {string} type - 通知类型 (success, error, warning, info)
 * @param {number} duration - 显示时长(ms)
 */
export function showToast(message, type = "info", duration = DEFAULT_TOAST_DURATION) {
  const normalizedMessage = normalizeMessage(message, type);
  const container = getOrCreateToastContainer();
  const toast = createToastElement(normalizedMessage, type);
  
  container.appendChild(toast);
  
  if (duration > 0) {
    setTimeout(() => removeToast(toast), duration);
  }
}

/**
 * 移除 Toast 通知
 * @param {HTMLElement} toast - Toast 元素
 */
export function removeToast(toast) {
  if (!toast || toast.classList.contains("hiding")) {
    return;
  }
  
  toast.classList.add("hiding");
  toast.addEventListener("animationend", () => {
    toast.parentElement?.removeChild(toast);
    cleanupEmptyToastContainer();
  });
}

/**
 * 显示消息提示
 * @param {string} message - 消息内容
 * @param {string} type - 消息类型
 */
export function showMessage(message, type = "info") {
  showToast(message, type);
}

// ==========================================
// 表格渲染
// ==========================================

/**
 * 通用表格渲染函数
 * @param {string} tableSelector - 表格选择器
 * @param {Array} data - 数据数组
 * @param {Function} rowRenderer - 行渲染函数
 * @param {Object} options - 选项
 */
export function renderTable(tableSelector, data, rowRenderer, options = {}) {
  const { emptyMessage = "暂无数据", colspan = 6 } = options;
  const tbody = document.querySelector(`${tableSelector} tbody`);
  
  if (!tbody) {
    return;
  }
  
  const fragment = document.createDocumentFragment();
  
  if (data && data.length > 0) {
    data.forEach((item) => {
      const row = document.createElement("tr");
      row.innerHTML = rowRenderer(item);
      fragment.appendChild(row);
    });
  } else {
    fragment.appendChild(createEmptyRow(emptyMessage, colspan));
  }
  
  tbody.innerHTML = "";
  tbody.appendChild(fragment);
}

// ==========================================
// 日期时间格式化
// ==========================================

/**
 * 格式化日期时间
 * @param {string|Date|number} date - 日期
 * @param {string} format - 格式
 * @returns {string}
 */
export function formatDateTime(date, format = "YYYY-MM-DD HH:mm:ss") {
  if (!date) {
    return "-";
  }
  
  const d = new Date(date);
  
  if (isNaN(d.getTime())) {
    return "-";
  }
  
  const pad = (n) => String(n).padStart(2, "0");
  
  const replacements = {
    YYYY: d.getFullYear(),
    MM: pad(d.getMonth() + 1),
    DD: pad(d.getDate()),
    HH: pad(d.getHours()),
    mm: pad(d.getMinutes()),
    ss: pad(d.getSeconds()),
  };
  
  let result = format;
  Object.entries(replacements).forEach(([key, value]) => {
    result = result.replace(key, value);
  });
  
  return result;
}

/**
 * 相对时间格式化
 * @param {string|Date|number} date - 日期
 * @returns {string}
 */
export function relativeTime(date) {
  const d = new Date(date);
  const now = new Date();
  const diff = now - d;
  
  const seconds = Math.floor(diff / 1000);
  const minutes = Math.floor(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const days = Math.floor(hours / 24);
  
  if (seconds < 60) return "刚刚";
  if (minutes < 60) return `${minutes}分钟前`;
  if (hours < 24) return `${hours}小时前`;
  if (days < 7) return `${days}天前`;
  
  return formatDateTime(date, "YYYY-MM-DD");
}

// ==========================================
// 加载状态
// ==========================================

/**
 * 设置加载状态
 * @param {HTMLElement} element - 要设置加载状态的元素
 * @param {boolean} isLoading - 是否处于加载状态
 * @param {string} loadingText - 加载文本
 */
export function setLoading(element, isLoading, loadingText = "加载中...") {
  if (!element) {
    return;
  }
  
  if (isLoading) {
    element.disabled = true;
    element.dataset.originalText = element.textContent;
    element.innerHTML = `<span class="loading"></span> ${loadingText}`;
  } else {
    element.disabled = false;
    element.textContent = element.dataset.originalText || "提交";
  }
}

// ==========================================
// 文件下载
// ==========================================

/**
 * 下载文件
 * @param {Blob} blob - 文件 Blob 对象
 * @param {string} filename - 文件名
 */
export function downloadFile(blob, filename) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  document.body.removeChild(link);
  URL.revokeObjectURL(url);
}

// ==========================================
// 表单处理
// ==========================================

/**
 * 获取 DOM 元素值
 * @param {string} elementId - 元素 ID
 * @param {string} type - 类型
 * @returns {*}
 */
export function getElementValue(elementId, type = "string") {
  const element = document.getElementById(elementId);
  
  if (!element) {
    return getDefaultValue(type);
  }
  
  const value = element.value;
  
  switch (type) {
    case "number":
      return value === "" ? null : parseInt(value, 10);
    case "float":
      return value === "" ? null : parseFloat(value);
    case "boolean":
      return value === "true" || value === "1";
    case "trimmed":
      return value.trim();
    default:
      return value;
  }
}

/**
 * 设置 DOM 元素值
 * @param {string} elementId - 元素 ID
 * @param {*} value - 值
 */
export function setElementValue(elementId, value) {
  const element = document.getElementById(elementId);
  
  if (!element) {
    return;
  }
  
  if (element.type === "checkbox") {
    element.checked = Boolean(value);
  } else if (element.tagName === "SELECT") {
    element.value = value;
  } else {
    element.value = value ?? "";
  }
}

/**
 * 通用表单提交处理
 * @param {Object} options - 选项
 * @returns {Promise<boolean>}
 */
export async function handleFormSubmit(options) {
  const {
    formData,
    id,
    baseUrl,
    successMessage,
    modalId,
    reloadFunction,
  } = options;
  
  const parsedId = id && id !== "" ? id : null;
  const url = parsedId ? `${baseUrl}/${parsedId}` : baseUrl;
  const method = parsedId ? "PUT" : "POST";
  
  try {
    const result = await apiRequest(url, {
      method,
      body: JSON.stringify(formData),
    });
    
    if (result.success) {
      closeModal(modalId);
      reloadFunction?.();
      showToast(successMessage, "success");
      return true;
    }
    
    const errorMsg = result.message ?? "操作失败，请检查输入信息";
    showToast(`操作失败: ${errorMsg}`, "error");
    return false;
  } catch (error) {
    console.error("表单提交失败:", error);
    showToast("操作失败，请重试", "error");
    return false;
  }
}

/**
 * 通用删除处理函数
 * @param {string} id - 资源 ID
 * @param {string} endpoint - API 端点
 * @param {string} successMessage - 成功消息
 * @param {Function} callback - 成功后的回调函数
 */
export async function handleDelete(id, endpoint, successMessage, callback) {
  if (!confirm("确定要删除吗？")) {
    return;
  }
  
  try {
    const result = await apiDelete(`${endpoint}/${id}`);
    
    if (result.success) {
      showToast(successMessage, "success");
      callback?.();
    } else {
      const errorMessage = normalizeDeleteError(result.message);
      showToast(`删除失败: ${errorMessage}`, "error");
    }
  } catch (error) {
    console.error("删除失败:", error);
    showToast("操作失败，请重试", "error");
  }
}

// ==========================================
// 错误处理
// ==========================================

/**
 * 通用错误处理函数
 * @param {Error|string|Object} error - 错误对象
 * @param {string} context - 错误上下文
 */
export function handleError(error, context = "操作失败") {
  let errorMessage = context;
  
  if (typeof error === "string") {
    errorMessage = error;
  } else if (error.message) {
    errorMessage = `${context}: ${error.message}`;
  } else if (error.response?.data?.message) {
    errorMessage = `${context}: ${error.response.data.message}`;
  }
  
  console.error(`${context}:`, error);
  showToast(errorMessage, "error");
}

// ==========================================
// 防抖和节流
// ==========================================

/**
 * 防抖函数
 * @param {Function} func - 要执行的函数
 * @param {number} delay - 延迟时间(ms)
 * @returns {Function}
 */
export function debounce(func, delay = DEFAULT_DEBOUNCE_DELAY) {
  let timeoutId;
  
  return function (...args) {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => func.apply(this, args), delay);
  };
}

/**
 * 节流函数
 * @param {Function} func - 要执行的函数
 * @param {number} limit - 时间限制(ms)
 * @returns {Function}
 */
export function throttle(func, limit = DEFAULT_THROTTLE_LIMIT) {
  let inThrottle;
  
  return function (...args) {
    if (!inThrottle) {
      func.apply(this, args);
      inThrottle = true;
      setTimeout(() => {
        inThrottle = false;
      }, limit);
    }
  };
}

// ==========================================
// 私有辅助函数
// ==========================================

function normalizeMessage(message, type) {
  if (!message || message.trim() === "") {
    const defaultMessages = {
      success: "操作成功",
      error: "操作失败",
      warning: "警告",
      info: "提示",
    };
    return defaultMessages[type] || "提示";
  }
  return message;
}

function getOrCreateToastContainer() {
  let container = document.querySelector(".toast-container");
  
  if (!container) {
    container = document.createElement("div");
    container.className = "toast-container";
    document.body.appendChild(container);
  }
  
  return container;
}

function createToastElement(message, type) {
  const toast = document.createElement("div");
  toast.className = `toast ${type}`;
  
  const content = document.createElement("div");
  content.className = "toast-message";
  content.textContent = message;
  
  const closeBtn = document.createElement("span");
  closeBtn.className = "toast-close";
  closeBtn.innerHTML = "&times;";
  closeBtn.addEventListener("click", () => removeToast(toast));
  
  toast.appendChild(content);
  toast.appendChild(closeBtn);
  
  return toast;
}

function cleanupEmptyToastContainer() {
  const container = document.querySelector(".toast-container");
  
  if (container && container.children.length === 0) {
    container.remove();
  }
}

function createEmptyRow(message, colspan) {
  const row = document.createElement("tr");
  row.className = "empty-row";
  
  const cell = document.createElement("td");
  cell.colSpan = colspan;
  cell.className = "text-center";
  cell.textContent = message;
  
  row.appendChild(cell);
  
  return row;
}

function getDefaultValue(type) {
  switch (type) {
    case "number":
    case "float":
      return 0;
    case "boolean":
      return false;
    default:
      return "";
  }
}

function normalizeDeleteError(message) {
  if (message?.includes("violates foreign key constraint")) {
    return "无法删除，因为该资源被其他数据引用";
  }
  return message || "未知错误";
}
