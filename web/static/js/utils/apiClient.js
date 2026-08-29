/**
 * API客户端类 - 处理所有API请求
 * 使用 HttpOnly Cookie 进行认证，彻底防止 XSS 窃取 token
 */
import { SessionManager } from "./sessionManager.js";
import { cache } from "./helpers.js";
import { t } from "./i18n.js";
import { showToast } from "./toast.js";

/**
 * 集中翻译服务端返回的 i18n 消息。
 *
 * 后端约定：API 响应形如 { success, message: "<i18n key>",
 * message_params: { name: "value" }（可选）, data }。此函数就地翻译
 * message 并删除 message_params；key 不在翻译目录时 t() 返回 key 本身。
 * 直接使用 fetch 的代码解析 JSON 后也应调用该函数。
 */
export function translateServerMessage(data) {
  if (data && typeof data === "object" && typeof data.message === "string") {
    // 翻译前保留原始 i18n key：调用方需要按键名分支（如验证码联动）
    // 时比较已翻译文案不可靠，t() 缺键还会返回 key 本身
    data.message_key = data.message;
    data.message = t(data.message, data.message_params || {});
    delete data.message_params;
  }
  return data;
}

export class ApiClient {
  static #pendingRequests = new Map();

  static #hashBody(body) {
    if (body instanceof FormData) {
      const entries = [...body.entries()].sort(([a], [b]) => a.localeCompare(b));
      return entries.map(([k, v]) => `${k}=${v}`).join("&");
    }
    if (typeof body === "string") {
      return body.length > 200 ? body.substring(0, 200) : body;
    }
    return String(body);
  }

  static #isRedirecting = false;

  static #refreshPromise = null;

  static #lastRefreshTime = 0;

  static #refreshThresholdMs = 5 * 60 * 1000;

  static setRefreshThreshold(thresholdMs) {
    this.#refreshThresholdMs = thresholdMs;
  }

  static async request(url, options = {}, retryCount = 0) {
    const method = options.method || "GET";
    const bodyHash = options.body ? `_${this.#hashBody(options.body)}` : "";
    const requestKey = `${url}_${method}${bodyHash}`;

    // 并发相同 GET 共享同一 Promise，避免重复请求
    if (method === "GET" && retryCount === 0 && this.#pendingRequests.has(requestKey)) {
      return this.#pendingRequests.get(requestKey);
    }

    // 会话维护：距上次刷新超过阈值时先刷新一次（refreshPromise 单飞去重）。
    // 不再做客户端并发上限与同键节流：浏览器 HTTP/2 多路复用 + 服务端
    // 限流（默认 200 请求/分钟/用户）已足够，人为排队只会放大串行延迟
    // skipAuthCheck：登录类请求不做主动刷新（无会话时 refresh 必然失败）
    const skipAuthCheck = options.skipAuthCheck === true;

    const now = Date.now();
    if (
      !this.isPublicAuthEndpoint(url) &&
      !skipAuthCheck &&
      // 仅在存在已登录会话标志时才做主动刷新：登录页等无会话场景
      // refresh 注定失败，只会徒增请求；401 恢复路径不受影响
      SessionManager.hasSession() &&
      now - this.#lastRefreshTime > this.#refreshThresholdMs
    ) {
      await this.refreshToken();
    }

    const defaultOptions = {
      headers: {
        "Content-Type": "application/json"
      },
      credentials: "include"
    };

    const mergedOptions = {
      ...defaultOptions,
      ...options,
      headers: {
        ...defaultOptions.headers,
        ...options.headers
      }
    };

    if ("skipAuthCheck" in mergedOptions) {
      delete mergedOptions.skipAuthCheck;
    }

    if (options.body instanceof FormData) {
      delete mergedOptions.headers["Content-Type"];
    }

    return this.processRequest(url, mergedOptions, retryCount, requestKey, skipAuthCheck);
  }

  static async processRequest(url, options, retryCount, requestKey, skipAuthCheck = false) {
    const requestPromise = this.makeRequest(url, options, retryCount, requestKey, skipAuthCheck);

    this.#pendingRequests.set(requestKey, requestPromise);

    try {
      const result = await requestPromise;
      // 资源写操作成功后广播：下拉选项短期缓存（resources.js）等据此失效。
      // 用事件而非直接导入，避免 apiClient ↔ resources 循环依赖
      if (result?.success && (options.method || "GET").toUpperCase() !== "GET") {
        document.dispatchEvent(new CustomEvent("ipma:data-mutation", { detail: { url } }));
      }
      return result;
    } finally {
      this.#pendingRequests.delete(requestKey);
    }
  }

  static async makeRequest(url, options, retryCount, requestKey, skipAuthCheck = false) {
    try {
      const response = await fetch(url, options);

      if (!response.ok) {
        if (response.status === 401) {
          return this.#handleUnauthorized(
            url,
            response,
            options,
            retryCount,
            requestKey,
            skipAuthCheck
          );
        }
        if (response.status === 429 && retryCount < 3) {
          // Retry-After 为秒数或 HTTP-date：parseInt 解析失败（含 HTTP-date
          // 得 NaN）时回退默认 2 秒，避免 NaN 延迟导致立即重试
          const retryAfterSeconds = parseInt(response.headers.get("Retry-After") ?? "", 10);
          const delaySeconds = Number.isNaN(retryAfterSeconds) ? 2 : retryAfterSeconds;

          await new Promise((resolve) => setTimeout(resolve, delaySeconds * 1000));
          return this.makeRequest(url, options, retryCount + 1, requestKey, skipAuthCheck);
        }

        try {
          const errorData = await response.json();
          translateServerMessage(errorData);
          return {
            success: false,
            message: errorData.message || `${t("api.request_failed")}: ${response.status}`,
            errorType: errorData.error_type || "api_error",
            errorDetails: errorData.error_details || {},
            suggestedAction: errorData.suggested_action || ""
          };
        } catch (jsonError) {
          console.error("错误响应体非 JSON:", jsonError);
          return {
            success: false,
            message: `${t("api.request_failed")}: ${response.status}`,
            errorType: "network_error"
          };
        }
      }

      const contentType = response.headers.get("Content-Type");

      // 优先处理 JSON 响应
      if (contentType && contentType.includes("application/json")) {
        return translateServerMessage(await response.json());
      }

      // 处理文件下载类型
      if (
        contentType &&
        (contentType.includes("application/octet-stream") ||
          contentType.includes("application/vnd.openxmlformats") ||
          contentType.includes("application/x-pem-file") ||
          contentType.includes("application/zip") ||
          contentType.includes("application/sql"))
      ) {
        let filename = "download";
        const disposition = response.headers.get("Content-Disposition");
        if (disposition && disposition.indexOf("attachment") !== -1) {
          const filenameRegex = /filename[^;=\n]*=((['"]).*?\2|[^;\n]*)/;
          const matches = filenameRegex.exec(disposition);
          if (matches !== null && matches[1]) {
            filename = matches[1].replace(/['"]/g, "");
          }
        }

        return {
          success: true,
          data: await response.blob(),
          isBlob: true,
          filename
        };
      }

      // 默认尝试解析为 JSON
      try {
        return translateServerMessage(await response.json());
      } catch {
        // 如果不是 JSON，返回文本
        const text = await response.text();
        return {
          success: false,
          message: t("api.parse_failed"),
          data: text
        };
      }
    } catch (error) {
      console.error("API请求错误:", error);

      // 抛出值不一定是 Error 实例（如 reject(字符串)），直接访问 .message
      // 会 TypeError 击穿"永不 reject"契约，统一做防御
      const rawMessage = error instanceof Error ? error.message : String(error);
      const errorName = error instanceof Error ? error.name : "";

      let errorType = "network_error";
      let errorMessage = t("api.network_error");

      if (errorName === "AbortError") {
        errorMessage = t("api.cancelled");
        errorType = "request_cancelled";
      } else if (rawMessage.includes("timeout")) {
        errorMessage = t("api.timeout");
        errorType = "timeout";
      } else if (rawMessage.includes("Network")) {
        errorMessage = t("api.network_failed");
      }

      return {
        success: false,
        message: errorMessage,
        errorType,
        errorDetails: { originalError: rawMessage }
      };
    }
  }

  /** 解析认证端点 401 响应体，归一为统一的错误结果对象（响应体非 JSON 时兜底） */
  static async #parseAuthErrorResponse(response) {
    try {
      const errorData = await response.json();
      translateServerMessage(errorData);
      return {
        success: false,
        message: errorData.message || t("api.auth_failed"),
        errorType: errorData.error_type || "unauthorized",
        errorDetails: errorData.error_details || {},
        suggestedAction: errorData.suggested_action || ""
      };
    } catch (jsonError) {
      console.error("认证失败响应体非 JSON:", jsonError);
      return {
        success: false,
        message: t("api.auth_failed"),
        errorType: "unauthorized"
      };
    }
  }

  /**
   * 401 处理：公共认证端点与 skipAuthCheck 请求（登录/发码/忘记密码等）
   * 直接返回真实错误响应，不走注定失败的 refresh，也不弹"令牌已过期"或
   * 强制跳转登录页（否则 LDAP 输错密码会丢输入且真实错误被吞）；
   * 其余请求先尝试刷新令牌并重试，失败才提示并跳转登录页
   */
  static async #handleUnauthorized(url, response, options, retryCount, requestKey, skipAuthCheck) {
    if (this.isPublicAuthEndpoint(url) || skipAuthCheck) {
      return this.#parseAuthErrorResponse(response);
    }

    const refreshSuccess = await this.refreshToken();
    if (refreshSuccess && retryCount < 3) {
      return this.request(url, options, retryCount + 1);
    }

    if (!this.#isRedirecting) {
      this.#isRedirecting = true;
      this.showAuthError();
      setTimeout(() => {
        this.redirectToLogin();
      }, 1500);
    }
    return { success: false, message: t("api.token_expired"), errorType: "token_expired" };
  }

  static isPublicAuthEndpoint(url) {
    if (typeof url !== "string") {
      return false;
    }
    const normalizedUrl = url.split("?")[0];
    return (
      normalizedUrl === "/api/auth/login" ||
      normalizedUrl === "/api/auth/login/email" ||
      normalizedUrl === "/api/auth/login/ldap" ||
      normalizedUrl === "/api/auth/login/send-code" ||
      normalizedUrl === "/api/auth/login/two-factor" ||
      normalizedUrl === "/api/auth/login/send-2fa-code" ||
      normalizedUrl === "/api/auth/forgot-password" ||
      normalizedUrl === "/api/auth/reset-password" ||
      normalizedUrl === "/api/auth/refresh" ||
      // 登录页初始化即调用的公开端点：不列入会在 401 时触发注定失败的
      // refresh 与"令牌已过期"跳转循环
      normalizedUrl === "/api/auth/methods" ||
      normalizedUrl === "/api/auth/init-status" ||
      normalizedUrl === "/api/auth/captcha" ||
      normalizedUrl === "/api/certificate/ca/info" ||
      normalizedUrl.startsWith("/api/init/")
    );
  }

  static async get(url) {
    return this.request(url);
  }

  static async post(url, data, options = {}) {
    const body = data instanceof FormData ? data : JSON.stringify(data);
    return this.request(url, { method: "POST", body, ...options });
  }

  static async put(url, data, options = {}) {
    return this.request(url, { method: "PUT", body: JSON.stringify(data), ...options });
  }

  static async delete(url, data = null, options = {}) {
    const deleteOptions = { method: "DELETE", ...options };
    if (data) {
      deleteOptions.body = JSON.stringify(data);
    }
    return this.request(url, deleteOptions);
  }

  static redirectToLogin() {
    SessionManager.clear();
    // 持久化缓存随登出一并清除：TTL 内的缓存可能含前一账号的业务数据，
    // 共享浏览器换账号后不应残留可读
    try {
      cache.clear();
    } catch (error) {
      console.error("清理持久化缓存失败:", error);
    }
    window.location.href = "/index.html";
  }

  static showAuthError() {
    showToast(t("api.token_expired"), "error");
  }

  static async refreshToken() {
    if (this.#refreshPromise) {
      return this.#refreshPromise;
    }

    this.#refreshPromise = (async () => {
      try {
        const response = await fetch("/api/auth/refresh", {
          method: "POST",
          credentials: "include",
          headers: {
            "Content-Type": "application/json"
          }
        });

        if (response.ok) {
          const result = await response.json();
          if (result.success) {
            this.#lastRefreshTime = Date.now();
          }
          return result.success;
        }
        return false;
      } catch (error) {
        console.error("令牌刷新失败:", error);
        return false;
      } finally {
        this.#refreshPromise = null;
      }
    })();

    return this.#refreshPromise;
  }
}

export const apiRequest = ApiClient.request.bind(ApiClient);
export const apiGet = ApiClient.get.bind(ApiClient);
export const apiPost = ApiClient.post.bind(ApiClient);
export const apiPut = ApiClient.put.bind(ApiClient);
export const apiDelete = ApiClient.delete.bind(ApiClient);
export const redirectToLogin = ApiClient.redirectToLogin.bind(ApiClient);
export const refreshToken = ApiClient.refreshToken.bind(ApiClient);
