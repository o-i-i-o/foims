import { showToast } from "./toast.js";

export type ErrorSeverity = "error" | "warning" | "info" | "critical";

export interface AppError {
  code: string;
  message: string;
  severity: ErrorSeverity;
  details?: unknown;
  timestamp: Date;
  stack?: string;
}

interface ErrorHandlerOptions {
  showToast?: boolean;
  logToConsole?: boolean;
  reportToServer?: boolean;
  customHandler?: (error: AppError) => void;
}

class GlobalErrorHandler {
  private errorQueue: AppError[] = [];
  private maxQueueSize = 100;
  private handlers: Map<string, (error: AppError) => void> = new Map();
  private defaultOptions: ErrorHandlerOptions = {
    showToast: true,
    logToConsole: true,
    reportToServer: false,
  };

  constructor() {
    this.setupGlobalHandlers();
  }

  private setupGlobalHandlers(): void {
    window.onerror = (message, source, lineno, colno, error) => {
      this.handle({
        code: "RUNTIME_ERROR",
        message: String(message),
        severity: "error",
        details: { source, lineno, colno },
        timestamp: new Date(),
        stack: error?.stack,
      });
      return true;
    };

    window.addEventListener("unhandledrejection", (event) => {
      this.handle({
        code: "UNHANDLED_REJECTION",
        message: event.reason?.message || "Unhandled Promise Rejection",
        severity: "error",
        details: event.reason,
        timestamp: new Date(),
        stack: event.reason?.stack,
      });
    });
  }

  handle(error: AppError, options?: ErrorHandlerOptions): void {
    const opts = { ...this.defaultOptions, ...options };

    this.errorQueue.push(error);
    if (this.errorQueue.length > this.maxQueueSize) {
      this.errorQueue.shift();
    }

    const customHandler = this.handlers.get(error.code);
    if (customHandler) {
      customHandler(error);
      return;
    }

    if (opts.logToConsole) {
      this.logToConsole(error);
    }

    if (opts.showToast) {
      this.showErrorMessage(error);
    }

    if (opts.reportToServer) {
      this.reportToServer(error);
    }

    if (opts.customHandler) {
      opts.customHandler(error);
    }
  }

  createError(code: string, message: string, severity: ErrorSeverity = "error", details?: unknown): AppError {
    return {
      code,
      message,
      severity,
      details,
      timestamp: new Date(),
    };
  }

  throw(code: string, message: string, severity: ErrorSeverity = "error", details?: unknown): never {
    const error = this.createError(code, message, severity, details);
    this.handle(error);
    throw new Error(`[${code}] ${message}`);
  }

  registerHandler(code: string, handler: (error: AppError) => void): void {
    this.handlers.set(code, handler);
  }

  unregisterHandler(code: string): void {
    this.handlers.delete(code);
  }

  getErrorHistory(): AppError[] {
    return [...this.errorQueue];
  }

  clearHistory(): void {
    this.errorQueue = [];
  }

  private logToConsole(error: AppError): void {
    const logMethod = error.severity === "critical" || error.severity === "error"
      ? console.error
      : error.severity === "warning"
        ? console.warn
        : console.info;

    logMethod(
      `[${error.code}] ${error.message}`,
      error.details || "",
      error.stack || ""
    );
  }

  private showErrorMessage(error: AppError): void {
    const toastType = error.severity === "critical" || error.severity === "error"
      ? "error"
      : error.severity === "warning"
        ? "warning"
        : "info";

    showToast(error.message, toastType);
  }

  private async reportToServer(error: AppError): Promise<void> {
    try {
      await fetch("/api/errors/report", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          code: error.code,
          message: error.message,
          severity: error.severity,
          details: error.details,
          timestamp: error.timestamp.toISOString(),
          stack: error.stack,
          url: window.location.href,
          userAgent: navigator.userAgent,
        }),
      });
    } catch (reportError) {
      console.error("Failed to report error to server:", reportError);
    }
  }
}

export const errorHandler = new GlobalErrorHandler();

export function handleApiError(error: unknown, defaultMessage = "操作失败"): void {
  if (error instanceof Error) {
    errorHandler.handle({
      code: "API_ERROR",
      message: error.message || defaultMessage,
      severity: "error",
      details: error,
      timestamp: new Date(),
      stack: error.stack,
    });
  } else if (typeof error === "object" && error !== null) {
    const apiError = error as { message?: string; code?: string };
    errorHandler.handle({
      code: apiError.code || "API_ERROR",
      message: apiError.message || defaultMessage,
      severity: "error",
      details: error,
      timestamp: new Date(),
    });
  } else {
    errorHandler.handle({
      code: "UNKNOWN_ERROR",
      message: defaultMessage,
      severity: "error",
      details: error,
      timestamp: new Date(),
    });
  }
}

export function handleNetworkError(error: unknown): void {
  errorHandler.handle({
    code: "NETWORK_ERROR",
    message: "网络连接失败，请检查网络设置",
    severity: "error",
    details: error,
    timestamp: new Date(),
  });
}

export function handleValidationError(field: string, message: string): void {
  errorHandler.handle({
    code: "VALIDATION_ERROR",
    message: `${field}: ${message}`,
    severity: "warning",
    details: { field, message },
    timestamp: new Date(),
  });
}

export function handleAuthError(message = "认证失败，请重新登录"): void {
  errorHandler.handle({
    code: "AUTH_ERROR",
    message,
    severity: "critical",
    timestamp: new Date(),
  });
}

export function handlePermissionError(action = "此操作"): void {
  errorHandler.handle({
    code: "PERMISSION_ERROR",
    message: `没有权限执行${action}`,
    severity: "warning",
    timestamp: new Date(),
  });
}

export function handleNotFoundError(resource = "资源"): void {
  errorHandler.handle({
    code: "NOT_FOUND_ERROR",
    message: `${resource}不存在`,
    severity: "warning",
    timestamp: new Date(),
  });
}

export function handleTimeoutError(operation = "操作"): void {
  errorHandler.handle({
    code: "TIMEOUT_ERROR",
    message: `${operation}超时，请稍后重试`,
    severity: "warning",
    timestamp: new Date(),
  });
}

export function wrapAsync<T extends (...args: unknown[]) => Promise<unknown>>(
  fn: T,
  errorMessage?: string
): T {
  return (async (...args: Parameters<T>) => {
    try {
      return await fn(...args);
    } catch (error) {
      handleApiError(error, errorMessage);
      throw error;
    }
  }) as T;
}

export function withErrorHandling<T>(
  fn: () => T,
  errorMessage?: string
): T | undefined {
  try {
    return fn();
  } catch (error) {
    handleApiError(error, errorMessage);
    return undefined;
  }
}

export const ErrorCodes = {
  NETWORK_ERROR: "NETWORK_ERROR",
  API_ERROR: "API_ERROR",
  VALIDATION_ERROR: "VALIDATION_ERROR",
  AUTH_ERROR: "AUTH_ERROR",
  PERMISSION_ERROR: "PERMISSION_ERROR",
  NOT_FOUND_ERROR: "NOT_FOUND_ERROR",
  TIMEOUT_ERROR: "TIMEOUT_ERROR",
  RUNTIME_ERROR: "RUNTIME_ERROR",
  UNHANDLED_REJECTION: "UNHANDLED_REJECTION",
} as const;

export type ErrorCode = typeof ErrorCodes[keyof typeof ErrorCodes];
