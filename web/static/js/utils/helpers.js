/**
 * 工具函数模块
 * 提供统一的异步调度、缓存管理和错误处理
 */
import { t } from "./i18n.js";

export function nextFrame(callback) {
  return requestAnimationFrame(() => {
    requestAnimationFrame(callback);
  });
}

export function whenVisible(selector, callback, timeout = 5000) {
  const el = document.querySelector(selector);
  if (!el) {
    callback(null);
    return;
  }

  let observer = null;
  const timer = setTimeout(() => {
    if (observer) {
      observer.disconnect();
      observer = null;
    }
    callback(el);
  }, timeout);

  observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          clearTimeout(timer);
          observer.disconnect();
          observer = null;
          callback(el);
          break;
        }
      }
    },
    { threshold: 0 }
  );

  observer.observe(el);
}

class CacheManager {
  constructor() {
    this.caches = new Map();
    this.defaultTTL = 5 * 60 * 1000;
    this.localStorageKey = "ipma_cache";
    this.cleanupIntervalId = null;
    this.loadFromStorage();
    this.startCleanupInterval();
  }

  loadFromStorage() {
    try {
      const stored = localStorage.getItem(this.localStorageKey);
      if (stored) {
        const now = Date.now();
        this.caches = new Map(
          Object.entries(JSON.parse(stored)).filter(([, item]) => item.expiry > now)
        );
      }
    } catch (e) {
      // Ignore localStorage errors
    }
  }

  saveToStorage() {
    try {
      const now = Date.now();
      const data = Object.fromEntries(
        this.caches
          .entries()
          .filter(([, item]) => item.expiry > now && item.persist !== false)
          .toArray()
      );
      localStorage.setItem(this.localStorageKey, JSON.stringify(data));
    } catch (e) {
      if (e.name === "QuotaExceededError") {
        this.cleanup();
        // 不再递归调用，避免无限循环
        console.warn("localStorage配额超限，已清理缓存");
      }
    }
  }

  get(key) {
    const item = this.caches.get(key);

    if (!item) {
      return null;
    }

    if (Date.now() > item.expiry) {
      this.caches.delete(key);
      return null;
    }

    return item.value;
  }

  set(key, value, ttl = this.defaultTTL, persist = true) {
    this.caches.set(key, {
      value,
      expiry: Date.now() + ttl,
      persist
    });

    if (persist) {
      this.saveToStorage();
    }
  }

  delete(key) {
    this.caches.delete(key);
    this.saveToStorage();
  }

  clear() {
    this.caches.clear();
    localStorage.removeItem(this.localStorageKey);
  }

  cleanup() {
    const now = Date.now();

    for (const [key, item] of this.caches.entries()) {
      if (now > item.expiry) {
        this.caches.delete(key);
      }
    }

    this.saveToStorage();
  }

  startCleanupInterval() {
    if (this.cleanupIntervalId) {
      clearInterval(this.cleanupIntervalId);
    }
    this.cleanupIntervalId = setInterval(() => this.cleanup(), 60 * 1000);
  }

  stopCleanupInterval() {
    if (this.cleanupIntervalId) {
      clearInterval(this.cleanupIntervalId);
      this.cleanupIntervalId = null;
    }
  }

  async getOrSet(key, fetcher, ttl = this.defaultTTL, persist = true) {
    const cached = this.get(key);

    if (cached !== null) {
      return cached;
    }

    const value = await fetcher();
    this.set(key, value, ttl, persist);

    return value;
  }
}

export const cache = new CacheManager();

class ErrorHandler {
  constructor() {
    this.handlers = new Map();
    this.defaultHandler = this.logError;
  }

  register(errorType, handler) {
    this.handlers.set(errorType, handler);
  }

  handle(error, context = t("common.operation_failed"), options = {}) {
    const errorInfo = this.parseError(error);
    const handler = this.handlers.get(errorInfo.type) || this.defaultHandler;

    handler(errorInfo, context, options);
  }

  parseError(error) {
    if (typeof error === "string") {
      return { type: "unknown", message: error, original: error };
    }

    if (error instanceof Error) {
      return {
        type: this.getErrorType(error),
        message: error.message,
        stack: error.stack,
        original: error
      };
    }

    if (typeof error === "object" && error !== null) {
      return {
        type: error.errorType || error.type || "api",
        message: error.message || t("common.unknown_error"),
        details: error.errorDetails || error.details,
        original: error
      };
    }

    return { type: "unknown", message: String(error), original: error };
  }

  getErrorType(error) {
    if (error.name === "NetworkError" || error.message.includes("network")) {
      return "network";
    }
    if (error.name === "TimeoutError" || error.message.includes("timeout")) {
      return "timeout";
    }
    if (error.name === "AbortError") {
      return "cancelled";
    }
    if (error.message.includes("unauthorized") || error.message.includes("401")) {
      return "auth";
    }
    return "unknown";
  }

  logError(errorInfo, context, options) {
    if (options.showToast !== false) {
      import("./ui.js").then(({ showToast }) => {
        showToast(`${context}: ${errorInfo.message}`, "error");
      });
    }
  }

  wrapAsync(fn, context) {
    return async (...args) => {
      try {
        return await fn(...args);
      } catch (error) {
        this.handle(error, context);
        return null;
      }
    };
  }
}

export const errorHandler = new ErrorHandler();

export async function safeAsync(fn, context = t("common.operation_failed"), options = {}) {
  try {
    return await fn();
  } catch (error) {
    errorHandler.handle(error, context, options);
    return options.defaultValue ?? null;
  }
}

class ElementCache {
  constructor() {
    this.cache = new Map();
  }

  get(id) {
    const cached = this.cache.get(id);
    if (cached && document.contains(cached)) {
      return cached;
    }
    this.cache.delete(id);
    const el = document.getElementById(id);
    if (el) {
      this.cache.set(id, el);
    }
    return el;
  }

  getMultiple(...ids) {
    const result = {};
    for (const id of ids) {
      result[id.replace(/-/g, "_")] = this.get(id);
    }
    return result;
  }

  getValue(id) {
    const el = this.get(id);
    return el ? el.value : "";
  }

  setValue(id, value) {
    const el = this.get(id);
    if (el) el.value = value;
  }

  getChecked(id) {
    const el = this.get(id);
    return el ? el.checked : false;
  }

  setChecked(id, checked) {
    const el = this.get(id);
    if (el) el.checked = checked;
  }

  clear(id) {
    if (id) {
      this.cache.delete(id);
    } else {
      this.cache.clear();
    }
  }

  refresh() {
    for (const [id] of this.cache) {
      this.cache.set(id, document.getElementById(id));
    }
  }
}

// 纯字符串转义：不创建临时 DOM 元素，且较 textContent 方案多覆盖引号（属性场景同样安全）
const HTML_ESCAPE_MAP = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;"
};

export function escapeHtml(text) {
  if (!text) return "";
  return String(text).replace(/[&<>"']/g, (ch) => HTML_ESCAPE_MAP[ch]);
}

export const elementCache = new ElementCache();

// ==========================================
// 子标签页持久化（刷新后停留在当前子标签）
// ==========================================

const SUBTAB_STORAGE_PREFIX = "ipma_subtab_";

/**
 * 记住某个页面当前激活的子标签
 * @param {string} pageId - 顶层页面 ID（如 "resources"、"logs"）
 * @param {string} tabId - 子标签 ID（如 "devices"）
 */
export function setActiveSubtab(pageId, tabId) {
  if (!pageId || !tabId) return;
  try {
    localStorage.setItem(SUBTAB_STORAGE_PREFIX + pageId, tabId);
  } catch (e) {
    // 忽略 localStorage 不可用的情况
  }
}

/**
 * 读取某个页面上次记住的子标签
 * @param {string} pageId - 顶层页面 ID
 * @returns {string|null} 子标签 ID，未记录或不可用时返回 null
 */
export function getActiveSubtab(pageId) {
  if (!pageId) return null;
  try {
    return localStorage.getItem(SUBTAB_STORAGE_PREFIX + pageId);
  } catch (e) {
    return null;
  }
}
