export function nextFrame(callback: FrameRequestCallback): void {
  requestAnimationFrame(() => {
    requestAnimationFrame(callback);
  });
}

export function whenVisible(selector: string, callback: (element: HTMLElement) => void, timeout = 5000): void {
  const element = document.querySelector<HTMLElement>(selector);

  if (element && element.offsetParent !== null) {
    callback(element);
    return;
  }

  const startTime = Date.now();

  function check(): void {
    const el = document.querySelector<HTMLElement>(selector);

    if (el && el.offsetParent !== null) {
      callback(el);
    } else if (Date.now() - startTime < timeout) {
      requestAnimationFrame(check);
    }
  }

  requestAnimationFrame(check);
}

interface CacheItem {
  value: unknown;
  expiry: number;
  persist: boolean;
}

class CacheManager {
  private caches: Map<string, CacheItem> = new Map();
  private defaultTTL = 5 * 60 * 1000;
  private localStorageKey = "ipma_cache";
  private cleanupIntervalId: ReturnType<typeof setInterval> | null = null;

  constructor() {
    this.loadFromStorage();
    this.startCleanupInterval();
  }

  private loadFromStorage(): void {
    try {
      const stored = localStorage.getItem(this.localStorageKey);
      if (stored) {
        const data = JSON.parse(stored) as Record<string, CacheItem>;
        const now = Date.now();

        for (const [key, item] of Object.entries(data)) {
          if (item.expiry > now) {
            this.caches.set(key, item);
          }
        }
      }
    } catch (_e) {
      // Ignore localStorage errors
    }
  }

  private saveToStorage(): void {
    try {
      const data: Record<string, CacheItem> = {};
      const now = Date.now();

      for (const [key, item] of this.caches.entries()) {
        if (item.expiry > now && item.persist !== false) {
          data[key] = item;
        }
      }

      localStorage.setItem(this.localStorageKey, JSON.stringify(data));
    } catch (e) {
      if (e instanceof DOMException && e.name === "QuotaExceededError") {
        this.cleanup();
        console.warn("localStorage配额超限，已清理缓存");
      }
    }
  }

  get<T = unknown>(key: string): T | null {
    const item = this.caches.get(key);

    if (!item) {
      return null;
    }

    if (Date.now() > item.expiry) {
      this.caches.delete(key);
      return null;
    }

    return item.value as T;
  }

  set<T = unknown>(key: string, value: T, ttl = this.defaultTTL, persist = true): void {
    this.caches.set(key, {
      value,
      expiry: Date.now() + ttl,
      persist,
    });

    if (persist) {
      this.saveToStorage();
    }
  }

  delete(key: string): void {
    this.caches.delete(key);
    this.saveToStorage();
  }

  clear(): void {
    this.caches.clear();
    localStorage.removeItem(this.localStorageKey);
  }

  cleanup(): void {
    const now = Date.now();

    for (const [key, item] of this.caches.entries()) {
      if (now > item.expiry) {
        this.caches.delete(key);
      }
    }

    this.saveToStorage();
  }

  private startCleanupInterval(): void {
    if (this.cleanupIntervalId) {
      clearInterval(this.cleanupIntervalId);
    }
    this.cleanupIntervalId = setInterval(() => this.cleanup(), 60 * 1000);
  }

  stopCleanupInterval(): void {
    if (this.cleanupIntervalId) {
      clearInterval(this.cleanupIntervalId);
      this.cleanupIntervalId = null;
    }
  }

  async getOrSet<T = unknown>(key: string, fetcher: () => Promise<T>, ttl = this.defaultTTL, persist = true): Promise<T> {
    const cached = this.get<T>(key);

    if (cached !== null) {
      return cached;
    }

    const value = await fetcher();
    this.set(key, value, ttl, persist);

    return value;
  }
}

export const cache = new CacheManager();

interface ErrorInfo {
  type: string;
  message: string;
  stack?: string;
  details?: Record<string, unknown>;
  original: unknown;
}

interface ErrorHandlerOptions {
  showToast?: boolean;
  defaultValue?: unknown;
}

type ErrorHandlerFn = (errorInfo: ErrorInfo, context: string, options: ErrorHandlerOptions) => void;

class ErrorHandler {
  private handlers: Map<string, ErrorHandlerFn> = new Map();
  private defaultHandler: ErrorHandlerFn = this.logError;

  register(errorType: string, handler: ErrorHandlerFn): void {
    this.handlers.set(errorType, handler);
  }

  handle(error: unknown, context = "操作失败", options: ErrorHandlerOptions = {}): void {
    const errorInfo = this.parseError(error);
    const handler = this.handlers.get(errorInfo.type) || this.defaultHandler;

    handler(errorInfo, context, options);
  }

  parseError(error: unknown): ErrorInfo {
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
      const err = error as Record<string, unknown>;
      return {
        type: (err.errorType as string) || (err.type as string) || "api",
        message: (err.message as string) || "未知错误",
        details: (err.errorDetails as Record<string, unknown>) || (err.details as Record<string, unknown>),
        original: error,
      };
    }

    return { type: "unknown", message: String(error), original: error };
  }

  private getErrorType(error: Error): string {
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

  private logError(errorInfo: ErrorInfo, context: string, options: ErrorHandlerOptions): void {
    if (options.showToast !== false) {
      import("./toast.js").then(({ showToast }) => {
        showToast(`${context}: ${errorInfo.message}`, "error");
      });
    }
  }

  wrapAsync<T extends (...args: unknown[]) => Promise<unknown>>(fn: T, context: string): T {
    return (async (...args: Parameters<T>) => {
      try {
        return await fn(...args);
      } catch (error) {
        this.handle(error, context);
        return null;
      }
    }) as T;
  }
}

export const errorHandler = new ErrorHandler();

export async function safeAsync<T = unknown>(
  fn: () => Promise<T>,
  context = "操作失败",
  options: ErrorHandlerOptions = {}
): Promise<T | null> {
  try {
    return await fn();
  } catch (error) {
    errorHandler.handle(error, context, options);
    return (options.defaultValue as T) ?? null;
  }
}

class ElementCache {
  private cache: Map<string, HTMLElement | null> = new Map();

  get(id: string): HTMLElement | null {
    const cached = this.cache.get(id);
    if (cached !== undefined) {
      return cached;
    }
    const el = document.getElementById(id);
    this.cache.set(id, el);
    return el;
  }

  getMultiple(...ids: string[]): Record<string, HTMLElement | null> {
    const result: Record<string, HTMLElement | null> = {};
    for (const id of ids) {
      result[id.replace(/-/g, "_")] = this.get(id);
    }
    return result;
  }

  getValue(id: string): string {
    const el = this.get(id);
    return el ? (el as HTMLInputElement).value : "";
  }

  setValue(id: string, value: string): void {
    const el = this.get(id);
    if (el) (el as HTMLInputElement).value = value;
  }

  getChecked(id: string): boolean {
    const el = this.get(id);
    return el ? (el as HTMLInputElement).checked : false;
  }

  setChecked(id: string, checked: boolean): void {
    const el = this.get(id);
    if (el) (el as HTMLInputElement).checked = checked;
  }

  clear(id?: string): void {
    if (id) {
      this.cache.delete(id);
    } else {
      this.cache.clear();
    }
  }

  refresh(): void {
    for (const [id] of this.cache) {
      this.cache.set(id, document.getElementById(id));
    }
  }
}

export function escapeHtml(text: string | null | undefined): string {
  if (!text) return "";
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}

export const elementCache = new ElementCache();
