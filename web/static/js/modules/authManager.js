import { apiPost, apiGet, redirectToLogin, refreshToken, ApiClient } from "../utils/apiClient.js";

import { showToast } from "../utils/ui.js";

import { t } from "../utils/i18n.js";
import { getUser, setUser, hasSession } from "../utils/sessionManager.js";

const parseDuration = (durationStr) => {
  const match = durationStr.match(/^(\d+)([smhd])$/);
  if (!match) return 15 * 60;

  const value = parseInt(match[1], 10);
  const unit = match[2];

  switch (unit) {
    case "s":
      return value;
    case "m":
      return value * 60;
    case "h":
      return value * 60 * 60;
    case "d":
      return value * 24 * 60 * 60;
    default:
      return 15 * 60;
  }
};

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
      credentials: "include"
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
      currentUserElement.textContent = t("auth.welcome", { username: user.username });
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

export const initAutoRefresh = async () => {
  if (autoRefreshInterval) {
    clearInterval(autoRefreshInterval);
  }

  let refreshIntervalSeconds = 7 * 60;
  let refreshThresholdSeconds = 5 * 60;

  try {
    const result = await apiGet("/api/system/config");
    if (result.success && result.data?.jwt?.access_token_expiry) {
      const accessTokenExpirySeconds = parseDuration(result.data.jwt.access_token_expiry);
      refreshIntervalSeconds = Math.floor(accessTokenExpirySeconds * 0.45);
      refreshThresholdSeconds = Math.floor(accessTokenExpirySeconds * 0.3);
      refreshIntervalSeconds = Math.max(refreshIntervalSeconds, 60);
      refreshThresholdSeconds = Math.max(refreshThresholdSeconds, 30);
    }
  } catch (error) {
    console.warn("获取JWT配置失败，使用默认刷新间隔:", error);
  }

  ApiClient.setRefreshThreshold(refreshThresholdSeconds * 1000);

  autoRefreshInterval = setInterval(async () => {
    try {
      const refreshed = await refreshToken();
      if (!refreshed) {
        console.warn("Token 自动刷新失败");
      }
    } catch (error) {
      console.error("Token 自动刷新错误:", error);
    }
  }, refreshIntervalSeconds * 1000);
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
let boundResetTimeout = null;
let boundEvents = null;

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

  // 移除旧的监听器
  if (boundResetTimeout && boundEvents) {
    boundEvents.forEach((event) => {
      document.removeEventListener(event, boundResetTimeout, true);
    });
  }

  let timeoutMinutes = 30;

  try {
    const result = await apiGet("/api/system/info");
    if (result.success && result.data) {
      timeoutMinutes =
        result.data.config?.server?.page_timeout ||
        result.data.config?.server?.session_timeout ||
        30;
    }
  } catch (error) {
    console.error("获取页面超时配置失败:", error);
  }

  // 创建新的事件处理函数
  boundResetTimeout = () => resetTimeout(timeoutMinutes);
  boundEvents = ["mousedown", "mousemove", "keypress", "scroll", "touchstart", "click"];
  boundEvents.forEach((event) => {
    document.addEventListener(event, boundResetTimeout, true);
  });

  resetTimeout(timeoutMinutes);
};

export const initPageTimeout = async () => {
  await startPageTimeout();
};
