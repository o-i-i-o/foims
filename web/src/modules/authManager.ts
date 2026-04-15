import { apiPost, apiGet, redirectToLogin, refreshToken } from "../utils/apiClient.js";
import { showToast } from "../utils/ui.js";
import { t } from "../utils/i18n.js";
import { getUser, setUser, hasSession, isRememberMe } from "../utils/sessionManager.js";
import type { User, LoginData } from "../types/session.js";

export const loginUser = (data: LoginData, rememberMe: boolean): void => {
  const user = data.user as User;
  setUser(user, rememberMe);
  window.location.href = "/main.html";
};

export const logoutUser = async (): Promise<void> => {
  try {
    await apiPost("/api/auth/logout", {});
  } catch (error) {
    console.error("退出登录请求失败:", error);
  }
  redirectToLogin();
};

export const checkLoginStatus = async (): Promise<void> => {
  if (!hasSession()) {
    return;
  }

  try {
    const response = await fetch("/api/auth/me", {
      credentials: "include",
    });

    if (response.ok) {
      const result = (await response.json()) as { success: boolean };
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

export const displayCurrentUser = (): void => {
  const user = getUser();

  if (user) {
    const currentUserElement = document.getElementById("current-user");
    if (currentUserElement) {
      currentUserElement.textContent = t("auth.welcome", { username: user.username });
    }
  }
};

export const initLogout = (): void => {
  const logoutBtn = document.getElementById("logout-btn");
  if (logoutBtn) {
    logoutBtn.addEventListener("click", logoutUser);
  }
};

let autoRefreshInterval: ReturnType<typeof setInterval> | null = null;

export const initAutoRefresh = (): void => {
  if (autoRefreshInterval) {
    clearInterval(autoRefreshInterval);
  }

  if (isRememberMe()) {
    autoRefreshInterval = setInterval(async () => {
      try {
        const refreshed = await refreshToken();
        if (!refreshed) {
          console.warn("Token 自动刷新失败");
        }
      } catch (error) {
        console.error("Token 自动刷新错误:", error);
      }
    }, 10 * 60 * 1000);
  }
};

const handlePageTimeout = async (): Promise<void> => {
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

let timeoutId: ReturnType<typeof setTimeout> | null = null;
let boundResetTimeout: (() => void) | null = null;
let boundEvents: string[] | null = null;

const resetTimeout = (timeoutMinutes: number): void => {
  if (timeoutId) {
    clearTimeout(timeoutId);
  }

  const TIMEOUT_DURATION = timeoutMinutes * 60 * 1000;

  timeoutId = setTimeout(() => {
    handlePageTimeout();
  }, TIMEOUT_DURATION);
};

const startPageTimeout = async (): Promise<void> => {
  if (!hasSession()) return;

  if (boundResetTimeout && boundEvents) {
    boundEvents.forEach(event => {
      document.removeEventListener(event, boundResetTimeout!, true);
    });
  }

  let timeoutMinutes = 30;

  try {
    const result = await apiGet<Record<string, unknown>>("/api/system/info");
    if (result.success && result.data) {
      const config = result.data.config as Record<string, unknown> | undefined;
      const server = config?.server as Record<string, unknown> | undefined;
      timeoutMinutes = (server?.page_timeout as number) || (server?.session_timeout as number) || 30;
    }
  } catch (error) {
    console.error("获取页面超时配置失败:", error);
  }

  boundResetTimeout = () => resetTimeout(timeoutMinutes);
  boundEvents = ["mousedown", "mousemove", "keypress", "scroll", "touchstart", "click"];
  boundEvents.forEach(event => {
    document.addEventListener(event, boundResetTimeout!, true);
  });

  resetTimeout(timeoutMinutes);
};

export const initPageTimeout = async (): Promise<void> => {
  await startPageTimeout();
};
