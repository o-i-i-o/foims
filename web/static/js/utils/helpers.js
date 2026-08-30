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
    this.localStorageKey = "foims_cache";
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
      // localStorage 不可用/数据损坏时放弃缓存，保留内存态继续工作
      console.error(`读取 localStorage 缓存失败（${this.localStorageKey}）:`, e);
    }
  }

  saveToStorage() {
    try {
      const now = Date.now();
      // 常规循环替代迭代器 helper 链（filter/toArray 是极新的 ES 特性，
      // 不支持的引擎抛 TypeError 且被 catch 静默吞掉，缓存持久化无声失效）
      const data = {};
      for (const [key, item] of this.caches.entries()) {
        if (item.expiry > now && item.persist !== false) {
          data[key] = item;
        }
      }
      localStorage.setItem(this.localStorageKey, JSON.stringify(data));
    } catch (e) {
      if (e.name === "QuotaExceededError") {
        this.cleanup();
        // 不再递归调用，避免无限循环
        console.warn("localStorage配额超限，已清理缓存");
      }
    }
  }

  /** 落盘防抖：dashboard 等一次加载会 set 多个键，合并为一次序列化 */
  scheduleSave() {
    clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => {
      this.saveTimer = null;
      this.saveToStorage();
    }, 200);
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
      this.scheduleSave();
    }
  }

  delete(key) {
    this.caches.delete(key);
    this.scheduleSave();
  }

  clear() {
    this.caches.clear();
    // 取消挂起的防抖落盘，避免 removeItem 后又被空快照写回
    clearTimeout(this.saveTimer);
    this.saveTimer = null;
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

    this.bindVisibilityPause();
  }

  /** 页面不可见时暂停清理与落盘，避免后台标签页周期性唤醒主线程（只注册一次） */
  bindVisibilityPause() {
    if (this.visibilityBound) {
      return;
    }
    this.visibilityBound = true;
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) {
        this.stopCleanupInterval();
      } else if (!this.cleanupIntervalId) {
        this.startCleanupInterval();
      }
    });
    // 跳转登录页/刷新前冲刷挂起的防抖写，避免丢失最后一次缓存
    window.addEventListener("pagehide", () => {
      if (this.saveTimer) {
        clearTimeout(this.saveTimer);
        this.saveTimer = null;
        this.saveToStorage();
      }
    });
  }

  stopCleanupInterval() {
    if (this.cleanupIntervalId) {
      clearInterval(this.cleanupIntervalId);
      this.cleanupIntervalId = null;
    }
  }
}

export const cache = new CacheManager();

export async function safeAsync(fn, context = t("common.operation_failed"), options = {}) {
  try {
    return await fn();
  } catch (error) {
    console.error(context, error);
    if (options.showToast !== false) {
      // 动态导入避免 helpers ↔ ui 静态循环依赖
      const { showToast } = await import("./ui.js");
      showToast(`${context}: ${error?.message ?? error}`, "error");
    }
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

  getValue(id) {
    const el = this.get(id);
    return el ? el.value : "";
  }

  setValue(id, value) {
    const el = this.get(id);
    if (el) {
      el.value = value;
    }
  }

  getChecked(id) {
    const el = this.get(id);
    return el ? el.checked : false;
  }

  setChecked(id, checked) {
    const el = this.get(id);
    if (el) {
      el.checked = checked;
    }
  }

  clear(id) {
    if (id) {
      this.cache.delete(id);
    } else {
      this.cache.clear();
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
  if (!text) {
    return "";
  }
  return String(text).replace(/[&<>"']/g, (ch) => HTML_ESCAPE_MAP[ch]);
}

export const elementCache = new ElementCache();

// ==========================================
// 子标签页持久化（刷新后停留在当前子标签）
// ==========================================

const SUBTAB_STORAGE_PREFIX = "foims_subtab_";

/**
 * 记住某个页面当前激活的子标签
 * @param {string} pageId - 顶层页面 ID（如 "resources"、"logs"）
 * @param {string} tabId - 子标签 ID（如 "devices"）
 */
export function setActiveSubtab(pageId, tabId) {
  if (!pageId || !tabId) {
    return;
  }
  try {
    localStorage.setItem(SUBTAB_STORAGE_PREFIX + pageId, tabId);
  } catch (e) {
    // localStorage 不可用时记录并跳过，不影响页面功能
    console.error(`保存子标签偏好失败（${pageId}）:`, e);
  }
}

/**
 * 读取某个页面上次记住的子标签
 * @param {string} pageId - 顶层页面 ID
 * @returns {string|null} 子标签 ID，未记录或不可用时返回 null
 */
export function getActiveSubtab(pageId) {
  if (!pageId) {
    return null;
  }
  try {
    return localStorage.getItem(SUBTAB_STORAGE_PREFIX + pageId);
  } catch (e) {
    console.error(`读取子标签偏好失败（${pageId}）:`, e);
    return null;
  }
}
