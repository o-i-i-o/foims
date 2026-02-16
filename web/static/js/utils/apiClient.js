/**
 * API客户端类 - 处理所有API请求
 * 使用 HttpOnly Cookie 进行认证，彻底防止 XSS 窃取 token
 */
import { clearSession } from "./sessionManager.js";

export class ApiClient {
  static #pendingRequests = new Map();
  static #requestTimestamps = new Map();
  
  static THROTTLE_INTERVAL = 1000;
  
  static requestQueue = [];
  
  static MAX_CONCURRENT_REQUESTS = 3;
  
  static currentRequests = 0;
  
  static #isRedirecting = false;
  
  static #refreshPromise = null;

  static async request(url, options = {}, retryCount = 0) {
    const requestKey = `${url}_${options.method || 'GET'}`;
    
    if (this.#pendingRequests.has(requestKey)) {
      return this.#pendingRequests.get(requestKey);
    }

    const now = Date.now();
    const lastRequestTime = this.#requestTimestamps.get(requestKey) || 0;
    if (now - lastRequestTime < this.THROTTLE_INTERVAL) {
      await new Promise(resolve => setTimeout(resolve, this.THROTTLE_INTERVAL - (now - lastRequestTime)));
    }

    this.#requestTimestamps.set(requestKey, Date.now());

    const defaultOptions = {
      headers: {
        "Content-Type": "application/json",
      },
      credentials: 'include',
    };

    const mergedOptions = {
      ...defaultOptions,
      ...options,
      headers: {
        ...defaultOptions.headers,
        ...options.headers,
      },
    };
    
    if ('skipAuthCheck' in mergedOptions) {
      delete mergedOptions.skipAuthCheck;
    }

    if (options.body instanceof FormData) {
      delete mergedOptions.headers["Content-Type"];
    }

    if (this.currentRequests >= this.MAX_CONCURRENT_REQUESTS) {
      return new Promise((resolve) => {
        this.requestQueue.push({
          url,
          options: mergedOptions,
          retryCount,
          requestKey,
          resolve
        });
        this.processQueue();
      });
    } else {
      return this.processRequest(url, mergedOptions, retryCount, requestKey);
    }
  }
  
  static async processRequest(url, options, retryCount, requestKey) {
    this.currentRequests++;
    
    const requestPromise = this.makeRequest(url, options, retryCount, requestKey);
    
    this.#pendingRequests.set(requestKey, requestPromise);
    
    try {
      const result = await requestPromise;
      return result;
    } finally {
      this.#pendingRequests.delete(requestKey);
      this.currentRequests--;
      this.processQueue();
    }
  }
  
  static processQueue() {
    while (this.currentRequests < this.MAX_CONCURRENT_REQUESTS && this.requestQueue.length > 0) {
      const queueItem = this.requestQueue.shift();
      if (queueItem) {
        this.processRequest(
          queueItem.url,
          queueItem.options,
          queueItem.retryCount,
          queueItem.requestKey
        ).then(queueItem.resolve);
      }
    }
  }

  static async makeRequest(url, options, retryCount, requestKey) {
    try {
      const response = await fetch(url, options);

      if (!response.ok) {
        if (response.status === 401) {
          if (this.isPublicAuthEndpoint(url)) {
            try {
              const errorData = await response.json();
              return {
                success: false,
                message: errorData.message || "认证失败",
                errorType: errorData.error_type || "unauthorized",
                errorDetails: errorData.error_details || {},
                suggestedAction: errorData.suggested_action || "",
              };
            } catch (jsonError) {
              return {
                success: false,
                message: "认证失败",
                errorType: "unauthorized",
              };
            }
          }

          const refreshSuccess = await this.refreshToken();
          if (refreshSuccess) {
            return this.request(url, options);
          }
          
          if (!this.#isRedirecting) {
            this.#isRedirecting = true;
            this.showAuthError();
            setTimeout(() => {
              this.redirectToLogin();
            }, 1500);
          }
          return { success: false, message: "登录已过期，请重新登录", errorType: "token_expired" };
        } else if (response.status === 429 && retryCount < 3) {
          const retryAfter = response.headers.get("Retry-After") || 2;
          const delay = parseInt(retryAfter) * 1000;
          
          console.warn(`请求速率限制，${delay}ms 后重试 (${retryCount + 1}/3)`);
          await new Promise(resolve => setTimeout(resolve, delay));
          return this.makeRequest(url, options, retryCount + 1, requestKey);
        }
        
        try {
          const errorData = await response.json();
          return { 
            success: false, 
            message: errorData.message || `API请求失败: ${response.status}`,
            errorType: errorData.error_type || "api_error",
            errorDetails: errorData.error_details || {},
            suggestedAction: errorData.suggested_action || ""
          };
        } catch (jsonError) {
          return { 
            success: false, 
            message: `API请求失败: ${response.status}`,
            errorType: "network_error"
          };
        }
      }

      const contentType = response.headers.get("Content-Type");
      if (
        contentType &&
        (contentType.includes("application/octet-stream") ||
          contentType.includes("application/vnd.openxmlformats") ||
          contentType.includes("application/x-pem-file") ||
          contentType.includes("application/json") === false)
      ) {
        if (contentType.includes("application/json")) {
           return await response.json();
        }
        
        let filename = "download";
        const disposition = response.headers.get("Content-Disposition");
        if (disposition && disposition.indexOf("attachment") !== -1) {
          const filenameRegex = /filename[^;=\n]*=((['"]).*?\2|[^;\n]*)/;
          const matches = filenameRegex.exec(disposition);
          if (matches != null && matches[1]) {
            filename = matches[1].replace(/['"]/g, "");
          }
        }
        
        return { 
          success: true, 
          data: await response.blob(), 
          isBlob: true,
          filename: filename
        };
      }

      return await response.json();
    } catch (error) {
      console.error("API请求错误:", error);
      
      let errorType = "network_error";
      let errorMessage = "API请求失败，请检查网络连接";
      
      if (error.name === "AbortError") {
        errorMessage = "请求已取消";
        errorType = "request_cancelled";
      } else if (error.message.includes("timeout")) {
        errorMessage = "请求超时，请稍后再试";
        errorType = "timeout";
      } else if (error.message.includes("Network")) {
        errorMessage = "网络连接失败，请检查您的网络设置";
        errorType = "network_error";
      }
      
      return { 
        success: false, 
        message: errorMessage,
        errorType,
        errorDetails: { originalError: error.message }
      };
    }
  }

  static isPublicAuthEndpoint(url) {
    if (typeof url !== "string") return false;
    const normalizedUrl = url.split("?")[0];
    return (
      normalizedUrl === "/api/auth/login" ||
      normalizedUrl === "/api/auth/login/email" ||
      normalizedUrl === "/api/auth/login/send-code" ||
      normalizedUrl === "/api/auth/login/two-factor" ||
      normalizedUrl === "/api/auth/login/send-2fa-code" ||
      normalizedUrl === "/api/auth/forgot-password" ||
      normalizedUrl === "/api/auth/reset-password" ||
      normalizedUrl === "/api/auth/refresh" ||
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
    clearSession();
    window.location.href = "/static/index.html";
  }

  static showAuthError() {
    const existingError = document.getElementById("auth-error-toast");
    if (existingError) return;
    
    const errorElement = document.createElement("div");
    errorElement.id = "auth-error-toast";
    errorElement.style.cssText = `
      position: fixed;
      top: 20px;
      right: 20px;
      background-color: #f8d7da;
      color: #721c24;
      padding: 15px 20px;
      border-radius: 4px;
      border: 1px solid #f5c6cb;
      z-index: 10000;
      box-shadow: 0 2px 10px rgba(0,0,0,0.1);
      font-family: Arial, sans-serif;
      font-size: 14px;
      animation: slideIn 0.3s ease-out;
    `;
    
    const style = document.createElement("style");
    style.textContent = `
      @keyframes slideIn {
        from {
          transform: translateX(100%);
          opacity: 0;
        }
        to {
          transform: translateX(0);
          opacity: 1;
        }
      }
    `;
    document.head.appendChild(style);
    
    errorElement.textContent = "登录已过期，请重新登录";
    document.body.appendChild(errorElement);
    
    setTimeout(() => {
      errorElement.style.animation = "slideIn 0.3s ease-out reverse";
      setTimeout(() => {
        if (document.body.contains(errorElement)) {
          document.body.removeChild(errorElement);
        }
        if (document.head.contains(style)) {
          document.head.removeChild(style);
        }
      }, 300);
    }, 3000);
  }

  static async refreshToken() {
    if (this.#refreshPromise) {
      return this.#refreshPromise;
    }
    
    this.#refreshPromise = (async () => {
      try {
        const response = await fetch("/api/auth/refresh", {
          method: "POST",
          credentials: 'include',
          headers: {
            "Content-Type": "application/json",
          },
        });

        if (response.ok) {
          const result = await response.json();
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
