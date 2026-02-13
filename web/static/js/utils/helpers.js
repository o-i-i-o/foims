/**
 * 工具函数模块
 * 提供统一的异步调度、缓存管理和错误处理
 */

// ==========================================
// 异步调度工具
// ==========================================

/**
 * 使用 requestAnimationFrame 调度回调
 * 确保在下一帧执行，比 setTimeout 更可靠
 * @param {Function} callback - 回调函数
 * @returns {number} - 帧ID
 */
export function nextFrame(callback) {
  return requestAnimationFrame(() => {
    requestAnimationFrame(callback);
  });
}

/**
 * 在 DOM 更新后执行回调
 * @param {Function} callback - 回调函数
 * @param {number} frames - 等待帧数
 */
export function afterFrame(callback, frames = 1) {
  let count = 0;
  
  function schedule() {
    if (count >= frames) {
      callback();
      return;
    }
    count++;
    requestAnimationFrame(schedule);
  }
  
  requestAnimationFrame(schedule);
}

/**
 * 等待元素可见后执行回调
 * @param {string} selector - 元素选择器
 * @param {Function} callback - 回调函数
 * @param {number} timeout - 超时时间(ms)
 */
export function whenVisible(selector, callback, timeout = 5000) {
  const element = document.querySelector(selector);
  
  if (element && element.offsetParent !== null) {
    callback(element);
    return;
  }
  
  const startTime = Date.now();
  
  function check() {
    const el = document.querySelector(selector);
    
    if (el && el.offsetParent !== null) {
      callback(el);
    } else if (Date.now() - startTime < timeout) {
      requestAnimationFrame(check);
    } else {
      console.warn(`元素 ${selector} 等待超时`);
    }
  }
  
  requestAnimationFrame(check);
}

// ==========================================
// 缓存管理
// ==========================================

class CacheManager {
  constructor() {
    this.caches = new Map();
    this.defaultTTL = 5 * 60 * 1000; // 5分钟
  }
  
  /**
   * 获取缓存
   * @param {string} key - 缓存键
   * @returns {*} 缓存值或 null
   */
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
  
  /**
   * 设置缓存
   * @param {string} key - 缓存键
   * @param {*} value - 缓存值
   * @param {number} ttl - 过期时间(ms)
   */
  set(key, value, ttl = this.defaultTTL) {
    this.caches.set(key, {
      value,
      expiry: Date.now() + ttl,
    });
  }
  
  /**
   * 删除缓存
   * @param {string} key - 缓存键
   */
  delete(key) {
    this.caches.delete(key);
  }
  
  /**
   * 清空所有缓存
   */
  clear() {
    this.caches.clear();
  }
  
  /**
   * 清理过期缓存
   */
  cleanup() {
    const now = Date.now();
    
    for (const [key, item] of this.caches.entries()) {
      if (now > item.expiry) {
        this.caches.delete(key);
      }
    }
  }
  
  /**
   * 获取或设置缓存（如果不存在则调用 fetcher）
   * @param {string} key - 缓存键
   * @param {Function} fetcher - 获取数据的函数
   * @param {number} ttl - 过期时间(ms)
   * @returns {Promise<*>}
   */
  async getOrSet(key, fetcher, ttl = this.defaultTTL) {
    const cached = this.get(key);
    
    if (cached !== null) {
      return cached;
    }
    
    const value = await fetcher();
    this.set(key, value, ttl);
    
    return value;
  }
}

export const cache = new CacheManager();

// ==========================================
// 错误处理
// ==========================================

class ErrorHandler {
  constructor() {
    this.handlers = new Map();
    this.defaultHandler = this.logError;
  }
  
  /**
   * 注册错误处理器
   * @param {string} errorType - 错误类型
   * @param {Function} handler - 处理函数
   */
  register(errorType, handler) {
    this.handlers.set(errorType, handler);
  }
  
  /**
   * 处理错误
   * @param {Error|string|Object} error - 错误对象
   * @param {string} context - 错误上下文
   * @param {Object} options - 选项
   */
  handle(error, context = "操作失败", options = {}) {
    const errorInfo = this.parseError(error);
    const handler = this.handlers.get(errorInfo.type) || this.defaultHandler;
    
    handler(errorInfo, context, options);
  }
  
  /**
   * 解析错误信息
   * @param {Error|string|Object} error - 错误对象
   * @returns {Object}
   */
  parseError(error) {
    if (typeof error === "string") {
      return { type: "unknown", message: error, original: error };
    }
    
    if (error instanceof Error) {
      return {
        type: this.getErrorType(error),
        message: error.message,
        stack: error.stack,
        original: error,
      };
    }
    
    if (typeof error === "object" && error !== null) {
      return {
        type: error.errorType || error.type || "api",
        message: error.message || "未知错误",
        details: error.errorDetails || error.details,
        original: error,
      };
    }
    
    return { type: "unknown", message: String(error), original: error };
  }
  
  /**
   * 获取错误类型
   * @param {Error} error - 错误对象
   * @returns {string}
   */
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
  
  /**
   * 默认错误处理器
   * @param {Object} errorInfo - 错误信息
   * @param {string} context - 错误上下文
   * @param {Object} options - 选项
   */
  logError(errorInfo, context, options) {
    console.error(`[${context}]`, errorInfo.message, errorInfo.original);
    
    if (options.showToast !== false) {
      import("./ui.js").then(({ showToast }) => {
        showToast(`${context}: ${errorInfo.message}`, "error");
      });
    }
  }
  
  /**
   * 创建安全的异步函数包装器
   * @param {Function} fn - 异步函数
   * @param {string} context - 错误上下文
   * @returns {Function}
   */
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

/**
 * 安全执行异步函数
 * @param {Function} fn - 异步函数
 * @param {string} context - 错误上下文
 * @param {Object} options - 选项
 * @returns {Promise<*>}
 */
export async function safeAsync(fn, context = "操作失败", options = {}) {
  try {
    return await fn();
  } catch (error) {
    errorHandler.handle(error, context, options);
    return options.defaultValue ?? null;
  }
}

// ==========================================
// 批量操作工具
// ==========================================

/**
 * 批量执行异步任务
 * @param {Array} items - 数据项数组
 * @param {Function} processor - 处理函数
 * @param {number} concurrency - 并发数
 * @returns {Promise<Array>}
 */
export async function batchProcess(items, processor, concurrency = 3) {
  const results = [];
  const queue = [...items];
  
  async function processNext() {
    while (queue.length > 0) {
      const item = queue.shift();
      try {
        const result = await processor(item);
        results.push({ success: true, data: result, item });
      } catch (error) {
        results.push({ success: false, error, item });
      }
    }
  }
  
  const workers = Array(Math.min(concurrency, items.length))
    .fill(null)
    .map(() => processNext());
  
  await Promise.all(workers);
  
  return results;
}

// ==========================================
// 重试工具
// ==========================================

/**
 * 带重试的异步执行
 * @param {Function} fn - 异步函数
 * @param {Object} options - 选项
 * @returns {Promise<*>}
 */
export async function retry(fn, options = {}) {
  const { maxRetries = 3, delay = 1000, backoff = 2 } = options;
  let lastError;
  
  for (let i = 0; i < maxRetries; i++) {
    try {
      return await fn();
    } catch (error) {
      lastError = error;
      
      if (i < maxRetries - 1) {
        await new Promise(resolve => setTimeout(resolve, delay * Math.pow(backoff, i)));
      }
    }
  }
  
  throw lastError;
}
