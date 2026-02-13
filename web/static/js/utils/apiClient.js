/**
 * API客户端类 - 处理所有API请求
 */
export class ApiClient {
  // 使用私有字段存储缓存（ES2022特性）
  static #requestCache = new Map();
  static #pendingRequests = new Map();
  static #requestTimestamps = new Map();
  
  // 节流间隔时间（毫秒）
  static THROTTLE_INTERVAL = 1000;
  
  // 请求队列
  static requestQueue = [];
  
  // 最大并发请求数
  static MAX_CONCURRENT_REQUESTS = 3;
  
  // 当前正在处理的请求数
  static currentRequests = 0;
  
  // 请求状态
  static RequestStatus = {
    PENDING: 'pending',
    PROCESSING: 'processing',
    COMPLETED: 'completed',
    FAILED: 'failed'
  };
  /**
   * 通用 API 请求函数
   * @param {string} url - API 端点
   * @param {object} options - 请求选项
   * @returns {Promise<{success: boolean, data?: any, message?: string}>}
   */
  /**
   * 通用 API 请求函数
   * @param {string} url - API 端点
   * @param {object} options - 请求选项
   * @param {number} retryCount - 重试次数
   * @returns {Promise<{success: boolean, data?: any, message?: string}>}
   */
  static async request(url, options = {}, retryCount = 0) {
    // 检查是否勾选了保持登录
    const localRememberMe = localStorage.getItem("rememberMe");
    const sessionRememberMe = sessionStorage.getItem("rememberMe");
    
    let rememberMe = false;
    if (localRememberMe === "true") {
      rememberMe = true;
    } else if (sessionRememberMe !== null) {
      rememberMe = false;
    }
    
    // 根据rememberMe状态获取access_token
    let accessToken;
    if (rememberMe) {
      accessToken = localStorage.getItem("access_token");
    } else {
      accessToken = sessionStorage.getItem("access_token");
    }
    
    // 如果没有token且不是跳过验证的请求，则跳转到登录页
    if (!accessToken && !options.skipAuthCheck) {
      this.redirectToLogin();
      return { success: false, message: "未登录", errorType: "unauthorized" };
    }

    // 生成请求键
    const requestKey = `${url}_${options.method || 'GET'}`;
    
    // 检查是否有相同的请求正在进行
    if (this.#pendingRequests.has(requestKey)) {
      return this.#pendingRequests.get(requestKey);
    }

    // 检查是否在节流间隔内
    const now = Date.now();
    const lastRequestTime = this.#requestTimestamps.get(requestKey) || 0;
    if (now - lastRequestTime < this.THROTTLE_INTERVAL) {
      // 等待节流间隔结束
      await new Promise(resolve => setTimeout(resolve, this.THROTTLE_INTERVAL - (now - lastRequestTime)));
    }

    // 更新请求时间
    this.#requestTimestamps.set(requestKey, Date.now());

    const defaultOptions = {
      headers: {
        "Content-Type": "application/json",
      },
    };
    
    // 如果有token，添加Authorization头
    if (accessToken) {
      defaultOptions.headers.Authorization = `Bearer ${accessToken}`;
    }

    const mergedOptions = {
      ...defaultOptions,
      ...options,
      headers: {
        ...defaultOptions.headers,
        ...options.headers,
      },
    };
    
    // 删除skipAuthCheck属性，以免传给fetch
    if ('skipAuthCheck' in mergedOptions) {
      delete mergedOptions.skipAuthCheck;
    }

    // 如果是 FormData，删除 Content-Type 让浏览器自动设置
    if (options.body instanceof FormData) {
      delete mergedOptions.headers["Content-Type"];
    }

    // 检查并发请求数
    if (this.currentRequests >= this.MAX_CONCURRENT_REQUESTS) {
      // 将请求加入队列
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
      // 直接处理请求
      return this.processRequest(url, mergedOptions, retryCount, requestKey);
    }
  }
  
  /**
   * 处理单个请求
   * @param {string} url - API 端点
   * @param {object} options - 请求选项
   * @param {number} retryCount - 重试次数
   * @param {string} requestKey - 请求键
   * @returns {Promise<{success: boolean, data?: any, message?: string}>}
   */
  static async processRequest(url, options, retryCount, requestKey) {
    this.currentRequests++;
    
    // 创建请求Promise
    const requestPromise = this.makeRequest(url, options, retryCount, requestKey);
    
    // 存储正在进行的请求
    this.#pendingRequests.set(requestKey, requestPromise);
    
    try {
      const result = await requestPromise;
      return result;
    } finally {
      // 请求完成后从 pendingRequests 中移除
      this.#pendingRequests.delete(requestKey);
      this.currentRequests--;
      // 处理队列中的下一个请求
      this.processQueue();
    }
  }
  
  /**
   * 处理请求队列
   */
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

  /**
   * 实际执行请求的函数
   * @param {string} url - API 端点
   * @param {object} options - 请求选项
   * @param {number} retryCount - 重试次数
   * @param {string} requestKey - 请求键
   * @returns {Promise<{success: boolean, data?: any, message?: string}>}
   */
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
          // 令牌刷新失败，显示友好提示
          this.showAuthError();
          this.redirectToLogin();
          return { success: false, message: "登录已过期，请重新登录", errorType: "token_expired" };
        } else if (response.status === 429 && retryCount < 3) {
          // 遇到速率限制，进行重试
          const retryAfter = response.headers.get("Retry-After") || 2;
          const delay = parseInt(retryAfter) * 1000;
          
          console.warn(`请求速率限制，${delay}ms 后重试 (${retryCount + 1}/3)`);
          await new Promise(resolve => setTimeout(resolve, delay));
          return this.makeRequest(url, options, retryCount + 1, requestKey);
        }
        
        // 尝试获取错误响应的JSON数据
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
          // 如果无法解析JSON，返回基本错误信息
          return { 
            success: false, 
            message: `API请求失败: ${response.status}`,
            errorType: "network_error"
          };
        }
      }

      // 检查是否是文件下载
      const contentType = response.headers.get("Content-Type");
      if (
        contentType &&
        (contentType.includes("application/octet-stream") ||
          contentType.includes("application/vnd.openxmlformats") ||
          contentType.includes("application/x-pem-file") ||
          contentType.includes("application/json") === false) // 如果不是JSON，尝试作为文件处理
      ) {
        // 对于非标准类型，如果看起来像JSON，还是尝试解析为JSON
        if (contentType.includes("application/json")) {
           return await response.json();
        }
        
        // 获取文件名
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
      
      // 网络错误分级处理
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

  /**
   * GET 请求简化函数
   * @param {string} url - API 端点
   * @returns {Promise<{success: boolean, data?: any, message?: string}>}
   */
  static async get(url) {
    return this.request(url);
  }

  /**
   * POST 请求简化函数
   * @param {string} url - API 端点
   * @param {any} data - 请求数据
   * @param {object} options - 请求选项
   * @returns {Promise<{success: boolean, data?: any, message?: string}>}
   */
  static async post(url, data, options = {}) {
    const body = data instanceof FormData ? data : JSON.stringify(data);
    return this.request(url, { method: "POST", body, ...options });
  }

  /**
   * PUT 请求简化函数
   * @param {string} url - API 端点
   * @param {any} data - 请求数据
   * @param {object} options - 请求选项
   * @returns {Promise<{success: boolean, data?: any, message?: string}>}
   */
  static async put(url, data, options = {}) {
    return this.request(url, { method: "PUT", body: JSON.stringify(data), ...options });
  }

  /**
   * DELETE 请求简化函数
   * @param {string} url - API 端点
   * @param {object} data - 请求数据
   * @param {object} options - 请求选项
   * @returns {Promise<{success: boolean, data?: any, message?: string}>}
   */
  static async delete(url, data = null, options = {}) {
    const deleteOptions = { method: "DELETE", ...options };
    if (data) {
      deleteOptions.body = JSON.stringify(data);
    }
    return this.request(url, deleteOptions);
  }

  /**
   * 获取访问令牌
   * 根据rememberMe状态从正确的存储位置获取令牌
   * @returns {string|null} 访问令牌
   */
  static getAccessToken() {
    // 检查localStorage中的rememberMe
    const localRememberMe = localStorage.getItem("rememberMe");
    // 检查sessionStorage中的rememberMe
    const sessionRememberMe = sessionStorage.getItem("rememberMe");
    
    // 如果localStorage中有rememberMe且为"true"，从localStorage获取令牌
    if (localRememberMe === "true") {
      return localStorage.getItem("access_token");
    }
    // 如果sessionStorage中有rememberMe（无论值是什么），从sessionStorage获取令牌
    else if (sessionRememberMe !== null) {
      return sessionStorage.getItem("access_token");
    }
    // 如果两者都没有rememberMe，尝试从两个存储中获取令牌
    else {
      return sessionStorage.getItem("access_token") || localStorage.getItem("access_token");
    }
  }

  /**
   * 跳转到登录页
   */
  static redirectToLogin() {
    sessionStorage.removeItem("access_token");
    sessionStorage.removeItem("refresh_token");
    sessionStorage.removeItem("user");
    sessionStorage.removeItem("rememberMe");
    localStorage.removeItem("access_token");
    localStorage.removeItem("refresh_token");
    localStorage.removeItem("user");
    localStorage.removeItem("rememberMe");
    window.location.href = "/static/index.html";
  }

  /**
   * 显示认证错误提示
   */
  static showAuthError() {
    // 创建一个临时的错误提示元素
    const errorElement = document.createElement("div");
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
    
    // 添加动画样式
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
    
    // 3秒后自动移除
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

  /**
   * 刷新令牌
   * @returns {Promise<boolean>} 刷新是否成功
   */
  static async refreshToken() {
    try {
      // 检查是否勾选了保持登录
      const localRememberMe = localStorage.getItem("rememberMe");
      const sessionRememberMe = sessionStorage.getItem("rememberMe");
      
      let rememberMe = false;
      if (localRememberMe === "true") {
        rememberMe = true;
      } else if (sessionRememberMe !== null) {
        rememberMe = false;
      }
      
      // 根据rememberMe状态获取refresh_token
      let refreshToken;
      if (rememberMe) {
        refreshToken = localStorage.getItem("refresh_token");
      } else {
        refreshToken = sessionStorage.getItem("refresh_token");
      }
      
      if (!refreshToken) {
        return false;
      }

      const response = await fetch("/api/auth/refresh", {
        method: "POST",
        headers: {
          Authorization: `Bearer ${refreshToken}`,
        },
      });

      if (response.ok) {
        const result = await response.json();
        if (result.success) {
          // 刷新成功，保存新令牌
          if (rememberMe) {
            // 如果勾选了保持登录，使用localStorage存储
            localStorage.setItem("access_token", result.data.access_token);
            localStorage.setItem("refresh_token", result.data.refresh_token);
          } else {
            // 否则使用sessionStorage存储
            sessionStorage.setItem("access_token", result.data.access_token);
            sessionStorage.setItem("refresh_token", result.data.refresh_token);
          }
          console.log("令牌刷新成功:", { storage: rememberMe ? "localStorage" : "sessionStorage" });
          return true;
        }
      }
      return false;
    } catch (error) {
      console.error("令牌刷新失败:", error);
      return false;
    }
  }
}

// 导出便捷方法
export const apiRequest = ApiClient.request.bind(ApiClient);
export const apiGet = ApiClient.get.bind(ApiClient);
export const apiPost = ApiClient.post.bind(ApiClient);
export const apiPut = ApiClient.put.bind(ApiClient);
export const apiDelete = ApiClient.delete.bind(ApiClient);
export const redirectToLogin = ApiClient.redirectToLogin.bind(ApiClient);
export const refreshToken = ApiClient.refreshToken.bind(ApiClient);
export const getAccessToken = ApiClient.getAccessToken.bind(ApiClient);
