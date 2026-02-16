/**
 * UI 工具模块
 * 提供统一的 UI 操作和工具函数
 */

import { apiRequest, apiDelete } from "./apiClient.js";
import { closeModal } from "./modal.js";

const DEFAULT_TOAST_DURATION = 3000;
const DEFAULT_DEBOUNCE_DELAY = 300;

export function showToast(message, type = "info", duration = DEFAULT_TOAST_DURATION) {
  const normalizedMessage = normalizeMessage(message, type);
  const container = getOrCreateToastContainer();
  const toast = createToastElement(normalizedMessage, type);
  
  container.appendChild(toast);
  
  if (duration > 0) {
    setTimeout(() => removeToast(toast), duration);
  }
}

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

export function showMessage(message, type = "info") {
  showToast(message, type);
}

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
    showToast("操作失败，请重试", "error");
    return false;
  }
}

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
    showToast("操作失败，请重试", "error");
  }
}

export function handleError(error, context = "操作失败") {
  let errorMessage = context;
  
  if (typeof error === "string") {
    errorMessage = error;
  } else if (error.message) {
    errorMessage = `${context}: ${error.message}`;
  } else if (error.response?.data?.message) {
    errorMessage = `${context}: ${error.response.data.message}`;
  }
  
  showToast(errorMessage, "error");
}

export function debounce(func, delay = DEFAULT_DEBOUNCE_DELAY) {
  let timeoutId;
  
  return function (...args) {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => func.apply(this, args), delay);
  };
}

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
