import {
  apiPost,
  apiGet,
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

let autoRefreshInterval = null;

export const initAutoRefresh = () => {
  if (autoRefreshInterval) {
    clearInterval(autoRefreshInterval);
  }
  
  if (isRememberMe()) {
    autoRefreshInterval = setInterval(async () => {
      try {
        await refreshToken();
      } catch (error) {
        // Ignore refresh errors
      }
    }, 10 * 60 * 1000);
  }
};

const handlePageTimeout = async () => {
  const refreshed = await refreshToken();
  
  if (refreshed) {
    showToast(t("auth.session_refreshed"), "success");
    return;
  }
  
  showToast(t("auth.session_timeout"), "info");
  
  setTimeout(() => {
    redirectToLogin();
  }, 2000);
};

let timeoutId = null;

const resetTimeout = (timeoutMinutes) => {
  if (timeoutId) {
    clearTimeout(timeoutId);
  }
  
  const TIMEOUT_DURATION = timeoutMinutes * 60 * 1000;
  
  timeoutId = setTimeout(() => {
    handlePageTimeout();
  }, TIMEOUT_DURATION);
};

const startPageTimeout = async () => {
  if (!hasSession()) return;

  let timeoutMinutes = 30;
  
  try {
    const result = await apiGet("/api/system/info");
    if (result.success && result.data) {
      timeoutMinutes = result.data.config?.server?.page_timeout 
        || result.data.config?.server?.session_timeout 
        || 30;
    }
  } catch (error) {
    console.error("获取页面超时配置失败:", error);
  }

  const events = ['mousedown', 'mousemove', 'keypress', 'scroll', 'touchstart', 'click'];
  events.forEach(event => {
    document.addEventListener(event, () => resetTimeout(timeoutMinutes), true);
  });

  resetTimeout(timeoutMinutes);
};

export const initPageTimeout = async () => {
  await startPageTimeout();
};
