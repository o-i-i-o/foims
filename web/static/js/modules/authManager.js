import {
  apiGet,
  apiPost,
  redirectToLogin,
  refreshToken,
  getAccessToken,
} from "../utils/apiClient.js";

import {
  showToast,
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";

/**
 * 登录用户并保存会话信息
 * @param {Object} data 登录成功返回的数据 {access_token, refresh_token, user}
 * @param {boolean} rememberMe 是否保持登录
 */
export const loginUser = (data, rememberMe) => {
  const { access_token, refresh_token, user } = data;
  
  console.log("登录成功，开始保存会话信息:", { rememberMe, user: user.username });
  
  // 清除旧数据
  localStorage.removeItem("access_token");
  localStorage.removeItem("refresh_token");
  localStorage.removeItem("user");
  localStorage.removeItem("rememberMe");
  sessionStorage.removeItem("access_token");
  sessionStorage.removeItem("refresh_token");
  sessionStorage.removeItem("user");
  sessionStorage.removeItem("rememberMe");

  if (rememberMe) {
    // 如果勾选了保持登录，使用localStorage持久化存储
    console.log("使用localStorage存储会话信息");
    localStorage.setItem("access_token", access_token);
    localStorage.setItem("refresh_token", refresh_token);
    localStorage.setItem("user", JSON.stringify(user));
    localStorage.setItem("rememberMe", "true");
  } else {
    // 否则使用sessionStorage，仅在当前会话有效
    console.log("使用sessionStorage存储会话信息");
    sessionStorage.setItem("access_token", access_token);
    sessionStorage.setItem("refresh_token", refresh_token);
    sessionStorage.setItem("user", JSON.stringify(user));
    sessionStorage.setItem("rememberMe", "false");
  }

  // 验证存储是否成功
  let storedToken;
  if (rememberMe) {
    storedToken = localStorage.getItem("access_token");
  } else {
    storedToken = getAccessToken();
  }
  console.log("会话信息保存成功:", { hasToken: !!storedToken, storage: rememberMe ? "localStorage" : "sessionStorage" });

  // 跳转到应用首页
  console.log("跳转到应用首页");
  window.location.href = "/main.html";
};

/**
 * 退出登录
 */
export const logoutUser = async () => {
  const token = getAccessToken() || localStorage.getItem("access_token");
  if (token) {
    try {
      // 发送退出登录请求
      await apiPost("/api/auth/logout", {});
    } catch (error) {
      console.error("退出登录请求失败:", error);
    }
  }

  // 清除sessionStorage和localStorage中的登录信息
  redirectToLogin();
};

// 检查登录状态
export const checkLoginStatus = async () => {
  // 检查是否勾选了保持登录
  // 优先检查localStorage中的rememberMe，然后检查sessionStorage中的rememberMe
  const localRememberMe = localStorage.getItem("rememberMe");
  const sessionRememberMe = sessionStorage.getItem("rememberMe");
  
  let rememberMe = false;
  if (localRememberMe === "true") {
    rememberMe = true;
  } else if (sessionRememberMe !== null) {
    rememberMe = false;
  }
  
  // 根据rememberMe状态获取令牌
  let token;
  if (rememberMe) {
    token = localStorage.getItem("access_token");
  } else {
    token = sessionStorage.getItem("access_token");
  }
  
  console.log("检查登录状态:", { 
    localRememberMe, 
    sessionRememberMe, 
    rememberMe, 
    hasToken: !!token, 
    storage: rememberMe ? "localStorage" : "sessionStorage" 
  });
  
  if (!token) {
    // 未登录，跳转到登录页
    window.location.href = "/static/index.html";
    return;
  }

  try {
    // 验证token是否有效
    // apiGet 会自动处理 401 错误（尝试刷新 token，失败则跳转登录）
    const result = await apiGet("/api/auth/me");
    
    // 如果请求成功，result.success 为 true
    // 如果请求失败且是认证错误，apiClient 内部逻辑会处理（跳转）
    // 这里我们只需要处理非认证错误（如网络错误）或者不做处理
  } catch (error) {
    // 这里的 catch 主要是捕获非 API 响应错误（如网络异常）
    console.error("检查登录状态失败:", error);
    // 网络错误时，不跳转登录页，让用户可以重试
  }
};

// 显示当前用户信息
export const displayCurrentUser = () => {
  // 检查是否勾选了保持登录
  // 优先检查localStorage中的rememberMe，然后检查sessionStorage中的rememberMe
  const localRememberMe = localStorage.getItem("rememberMe");
  const sessionRememberMe = sessionStorage.getItem("rememberMe");
  
  let rememberMe = false;
  if (localRememberMe === "true") {
    rememberMe = true;
  } else if (sessionRememberMe !== null) {
    rememberMe = false;
  }
  
  // 根据rememberMe状态获取用户信息
  let userJson;
  if (rememberMe) {
    userJson = localStorage.getItem("user");
  } else {
    userJson = sessionStorage.getItem("user");
  }
  
  if (userJson) {
    const user = JSON.parse(userJson);
    const currentUserElement = document.getElementById("current-user");
    if (currentUserElement) {
      currentUserElement.textContent = t('auth.welcome', { username: user.username });
    }
  }
};

// 初始化退出登录功能
export const initLogout = () => {
  const logoutBtn = document.getElementById("logout-btn");
  if (logoutBtn) {
    logoutBtn.addEventListener("click", logoutUser);
  }
};

// 启动令牌自动刷新
function startTokenRefresh() {
  // 每10分钟刷新一次令牌（令牌有效期为15分钟）
  setInterval(
    async () => {
      // 检查是否勾选了保持登录
      const localRememberMe = localStorage.getItem("rememberMe");
      const sessionRememberMe = sessionStorage.getItem("rememberMe");
      
      let rememberMe = false;
      if (localRememberMe === "true") {
        rememberMe = true;
      } else if (sessionRememberMe !== null) {
        rememberMe = false;
      }
      
      // 检查是否有有效令牌
      let hasToken;
      if (rememberMe) {
        hasToken = !!localStorage.getItem("access_token");
      } else {
        hasToken = !!sessionStorage.getItem("access_token");
      }
      
      // 只有在有效会话中，才尝试刷新
      if (hasToken) {
          console.log("尝试刷新令牌:", { rememberMe, storage: rememberMe ? "localStorage" : "sessionStorage" });
          const refreshSuccess = await refreshToken();
          if (!refreshSuccess) {
            console.error("令牌刷新失败，将跳转到登录页");
            // 刷新失败，跳转到登录页
            redirectToLogin();
          } else {
            console.log("令牌刷新成功");
          }
      }
    },
    10 * 60 * 1000,
  );
}

// 初始化自动刷新
export const initAutoRefresh = () => {
  // 检查是否需要启动令牌自动刷新
  const localRememberMe = localStorage.getItem("rememberMe");
  const sessionRememberMe = sessionStorage.getItem("rememberMe");
  
  let rememberMe = false;
  if (localRememberMe === "true") {
    rememberMe = true;
  } else if (sessionRememberMe !== null) {
    rememberMe = false;
  }
  
  // 检查是否有有效令牌
  let hasToken;
  if (rememberMe) {
    hasToken = !!localStorage.getItem("access_token");
  } else {
    hasToken = !!sessionStorage.getItem("access_token");
  }
  
  if (hasToken) {
    console.log("初始化令牌自动刷新:", { rememberMe, storage: rememberMe ? "localStorage" : "sessionStorage" });
    startTokenRefresh();
  }
};

// 处理页面超时
const handlePageTimeout = () => {
  // 清除会话存储和本地存储
  localStorage.removeItem("access_token");
  localStorage.removeItem("refresh_token");
  localStorage.removeItem("user");
  localStorage.removeItem("rememberMe");
  sessionStorage.removeItem("access_token");
  sessionStorage.removeItem("refresh_token");
  sessionStorage.removeItem("user");
  sessionStorage.removeItem("rememberMe");

  // 显示超时提示
  showToast("登录已超时，请重新登录", "info");

  // 跳转到登录页
  setTimeout(() => {
    window.location.href = "/static/index.html";
  }, 2000);
};

// 启动页面超时管理
const startPageTimeout = async () => {
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
    
    // 根据rememberMe状态获取令牌
    let token;
    if (rememberMe) {
      token = localStorage.getItem("access_token");
    } else {
      token = sessionStorage.getItem("access_token");
    }
    
    if (!token) return;

    const response = await fetch("/api/system/info", {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const result = await response.json();
    // 优先使用 session_timeout (如果后端提供了)
    const config = result.data.config.server;
    const timeoutMinutes = config.session_timeout;
    
    const TIMEOUT_DURATION = timeoutMinutes * 60 * 1000;
    let timeoutId;

    // 重置超时计时器
    const resetTimeout = () => {
      clearTimeout(timeoutId);
      timeoutId = setTimeout(() => {
        // 超时处理
        handlePageTimeout();
      }, TIMEOUT_DURATION);
    };

    // 监听用户操作事件
    const events = ['mousedown', 'mousemove', 'keypress', 'scroll', 'touchstart', 'click'];
    events.forEach(event => {
      document.addEventListener(event, resetTimeout, true);
    });

    // 初始设置超时
    resetTimeout();
  } catch (error) {
    console.error("获取页面超时配置失败:", error);
    // 失败时使用默认值30分钟
    const TIMEOUT_DURATION = 30 * 60 * 1000;
    let timeoutId;

    const resetTimeout = () => {
      clearTimeout(timeoutId);
      timeoutId = setTimeout(() => {
        handlePageTimeout();
      }, TIMEOUT_DURATION);
    };

    const events = ['mousedown', 'mousemove', 'keypress', 'scroll', 'touchstart', 'click'];
    events.forEach(event => {
      document.addEventListener(event, resetTimeout, true);
    });

    resetTimeout();
  }
};

// 初始化页面超时管理
export const initPageTimeout = async () => {
  // 检查是否勾选了保持登录
  const localRememberMe = localStorage.getItem("rememberMe");
  const sessionRememberMe = sessionStorage.getItem("rememberMe");
  
  let rememberMe = false;
  if (localRememberMe === "true") {
    rememberMe = true;
  } else if (sessionRememberMe !== null) {
    rememberMe = false;
  }
  
  if (!rememberMe) {
    // 未勾选保持登录，启动页面超时管理
    console.log("未勾选保持登录，启动页面超时管理");
    await startPageTimeout();
  } else {
    console.log("已勾选保持登录，不启动页面超时管理");
  }
};
