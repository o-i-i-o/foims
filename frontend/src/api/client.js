/**
 * API 客户端：处理所有 API 请求。
 * 使用 HttpOnly Cookie 进行认证（credentials: include），防止 XSS 窃取 token。
 *
 * 契约与旧版 web/static/js/utils/apiClient.js 一致：
 * - 永不 reject：所有失败归一为 { success: false, message, errorType, ... }；
 * - 服务端响应形如 { success, message: "<i18n key>", message_params?, data }，
 *   message 在此处就地翻译（message_key 保留原始键供调用方分支）；
 * - 401 先刷新令牌重试（公共认证端点除外），429 按 Retry-After 退避重试；
 * - 文件下载响应归一为 { success, data: Blob, isBlob: true, filename }。
 *
 * 与旧版的差异（Vue 重构）：
 * - 移除并发 GET 去重与 data-mutation 事件广播：由 TanStack Query 的
 *   查询去重与缓存失效机制取代；
 * - 移除 refreshThreshold 主动刷新阈值：TanStack Query 的 queryFn 统一
 *   走 401 恢复路径，会话续期完全由服务端驱动的 401 重试承担。
 */
import { i18n } from "../i18n/index.js";
import { clearSession } from "../utils/session.js";

function t(key, params = {}) {
  return i18n.global.t(key, params);
}

/**
 * 集中翻译服务端返回的 i18n 消息：就地翻译 message 并删除 message_params，
 * 保留原始键到 message_key（调用方按键名分支，如验证码联动）。
 */
export function translateServerMessage(data) {
  if (data && typeof data === "object" && typeof data.message === "string") {
    data.message_key = data.message;
    data.message = t(data.message, data.message_params || {});
    delete data.message_params;
  }
  return data;
}

let isRedirecting = false;
let refreshPromise = null;

/** 无会话也允许直接调用的公共端点（登录页/初始化向导） */
export function isPublicAuthEndpoint(url) {
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
    normalizedUrl === "/api/auth/methods" ||
    normalizedUrl === "/api/auth/init-status" ||
    normalizedUrl === "/api/auth/captcha" ||
    normalizedUrl === "/api/certificate/ca/info" ||
    normalizedUrl.startsWith("/api/init/")
  );
}

export async function refreshToken() {
  if (refreshPromise) {
    return refreshPromise;
  }

  refreshPromise = (async () => {
    try {
      const response = await fetch("/api/auth/refresh", {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" }
      });
      if (!response.ok) {
        return false;
      }
      const result = await response.json();
      return result.success === true;
    } catch (error) {
      console.error("令牌刷新失败:", error);
      return false;
    } finally {
      refreshPromise = null;
    }
  })();

  return refreshPromise;
}

export function redirectToLogin() {
  clearSession();
  window.location.href = "/index.html";
}

/** 401 处理：公共认证端点返回真实错误；其余刷新令牌重试，失败跳登录页 */
async function handleUnauthorized(url, response, options, retryCount) {
  if (isPublicAuthEndpoint(url) || options.skipAuthCheck === true) {
    return parseErrorResponse(response, t("api.auth_failed"), "unauthorized");
  }

  const refreshed = await refreshToken();
  if (refreshed && retryCount < 3) {
    return request(url, options, retryCount + 1);
  }

  if (!isRedirecting) {
    isRedirecting = true;
    showToastAuthError();
    setTimeout(() => {
      redirectToLogin();
    }, 1500);
  }
  return { success: false, message: t("api.token_expired"), errorType: "token_expired" };
}

function showToastAuthError() {
  // 延迟导入避免与 toast 组件形成模块循环：错误提示非关键路径
  import("../composables/useToast.js").then(({ showToast }) => {
    showToast(t("api.token_expired"), "error");
  });
}

/** 解析非 2xx 响应体：JSON 时提取服务端消息，非 JSON 时保留原文片段 */
async function parseErrorResponse(response, fallbackMessage, fallbackType) {
  const errorText = await response.text();
  try {
    const errorData = JSON.parse(errorText);
    translateServerMessage(errorData);
    return {
      success: false,
      message: errorData.message || fallbackMessage,
      errorType: errorData.error_type || fallbackType,
      errorDetails: errorData.error_details || {},
      suggestedAction: errorData.suggested_action || ""
    };
  } catch {
    const detail = errorText ? ` (${errorText.slice(0, 120)})` : "";
    return {
      success: false,
      message: `${fallbackMessage}: ${response.status}${detail}`,
      errorType: fallbackType === "unauthorized" ? fallbackType : "api_error"
    };
  }
}

/** 从 Content-Disposition 提取下载文件名 */
function extractFilename(disposition) {
  if (!disposition || disposition.indexOf("attachment") === -1) {
    return "download";
  }
  const matches = /filename[^;=\n]*=((['"]).*?\2|[^;\n]*)/.exec(disposition);
  return matches?.[1] ? matches[1].replace(/['"]/g, "") : "download";
}

const DOWNLOAD_CONTENT_TYPES = [
  "application/octet-stream",
  "application/vnd.openxmlformats",
  "application/x-pem-file",
  "application/zip",
  "application/x-rpm",
  "application/vnd.debian.binary-package",
  "application/sql"
];

async function makeRequest(url, options, retryCount) {
  try {
    const response = await fetch(url, options);

    if (!response.ok) {
      if (response.status === 401) {
        return handleUnauthorized(url, response, options, retryCount);
      }
      if (response.status === 429 && retryCount < 3) {
        // Retry-After 为秒数或 HTTP-date：解析失败回退默认 2 秒
        const retryAfterSeconds = Number.parseInt(response.headers.get("Retry-After") ?? "", 10);
        const delaySeconds = Number.isNaN(retryAfterSeconds) ? 2 : retryAfterSeconds;
        await new Promise((resolve) => setTimeout(resolve, delaySeconds * 1000));
        return makeRequest(url, options, retryCount + 1);
      }
      return parseErrorResponse(response, t("api.request_failed"), "api_error");
    }

    const contentType = response.headers.get("Content-Type") || "";

    if (contentType.includes("application/json")) {
      return translateServerMessage(await response.json());
    }

    if (DOWNLOAD_CONTENT_TYPES.some((type) => contentType.includes(type))) {
      return {
        success: true,
        data: await response.blob(),
        isBlob: true,
        filename: extractFilename(response.headers.get("Content-Disposition"))
      };
    }

    // 非 JSON 非 download：尝试按 JSON 解析，失败按错误处理并保留原文
    try {
      return translateServerMessage(await response.json());
    } catch {
      const text = await response.text();
      return { success: false, message: t("api.parse_failed"), data: text };
    }
  } catch (error) {
    console.error("API请求错误:", error);

    // 抛出值不一定是 Error 实例，统一做防御
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

/**
 * 请求入口：组装默认头后交 makeRequest。会话续期交给 401 恢复路径：
 * 主动刷新阈值依赖全局计时状态，TanStack Query 场景下由服务端 401
 * 驱动重试更可靠。
 */
export async function request(url, options = {}, retryCount = 0) {
  const mergedOptions = {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...options.headers
    },
    credentials: "include"
  };
  delete mergedOptions.skipAuthCheck;

  if (options.body instanceof FormData) {
    delete mergedOptions.headers["Content-Type"];
  }

  return makeRequest(url, mergedOptions, retryCount);
}

export const apiGet = (url) => request(url);

export function apiPost(url, data, options = {}) {
  const body = data instanceof FormData ? data : JSON.stringify(data);
  return request(url, { method: "POST", body, ...options });
}

export function apiPut(url, data, options = {}) {
  return request(url, { method: "PUT", body: JSON.stringify(data), ...options });
}

export function apiDelete(url, data = null, options = {}) {
  const deleteOptions = { method: "DELETE", ...options };
  if (data) {
    deleteOptions.body = JSON.stringify(data);
  }
  return request(url, deleteOptions);
}
