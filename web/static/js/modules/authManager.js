import {
  apiPost,
  redirectToLogin,
  refreshToken,
} from "../utils/apiClient.js";

import {
  showToast,
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";
import {
  getUser,
  setUser,
  clearSession,
  hasSession,
  isRememberMe,
} from "../utils/sessionManager.js";

export const loginUser = (data, rememberMe) => {
  const { user } = data;
  
  setUser(user, rememberMe);

  window.location.href = "/main.html";
};

export const logoutUser = async () => {
  try {
    await apiPost("/api/auth/logout", {});
  } catch (error) {
    console.error("退出登录请求失败:", error);
  }

  redirectToLogin();
};

export const checkLoginStatus = async () => {
  if (!hasSession()) {
    return;
  }
  
  try {
    const response = await fetch("/api/auth/me", {
      credentials: 'include',
    });
    
    if (response.ok) {
      const result = await response.json();
      if (result.success) {
        return;
      }
    }
    
    if (response.status === 401) {
      const refreshed = await refreshToken();
      if (refreshed) {
        return;
      }
    }
    
    redirectToLogin();
  } catch (error) {
    console.error("检查登录状态失败:", error);
    redirectToLogin();
  }
};

export const displayCurrentUser = () => {
  const user = getUser();
  
  if (user) {
    const currentUserElement = document.getElementById("current-user");
    if (currentUserElement) {
      currentUserElement.textContent = t('auth.welcome', { username: user.username });
    }
  }
};

export const initLogout = () => {
  const logoutBtn = document.getElementById("logout-btn");
  if (logoutBtn) {
    logoutBtn.addEventListener("click", logoutUser);
  }
};

export const initAutoRefresh = () => {
};

const handlePageTimeout = () => {
  clearSession();

  showToast("登录已超时，请重新登录", "info");

  setTimeout(() => {
    window.location.href = "/static/index.html";
  }, 2000);
};

const startPageTimeout = async () => {
  if (!hasSession()) return;

  let timeoutMinutes = 30;
  
  try {
    const response = await fetch("/api/system/info", {
      credentials: 'include',
    });

    const result = await response.json();
    timeoutMinutes = result.data?.config?.server?.page_timeout || result.data?.config?.server?.session_timeout || 30;
  } catch (error) {
    console.error("获取页面超时配置失败:", error);
  }
  
  const TIMEOUT_DURATION = timeoutMinutes * 60 * 1000;
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
};

export const initPageTimeout = async () => {
  if (!isRememberMe()) {
    await startPageTimeout();
  }
};
