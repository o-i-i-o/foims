import { apiPost, apiGet, redirectToLogin, refreshToken, ApiClient } from "../utils/apiClient.js";

import { showToast } from "../utils/ui.js";

import { t } from "../utils/i18n.js";
import { SessionManager } from "../utils/sessionManager.js";
import { loadModal, openModal, closeModal } from "../utils/modalLoader.js";
import { elementCache } from "../utils/helpers.js";

const parseDuration = (durationStr) => {
  const match = durationStr.match(/^(\d+)([smhd])$/);
  if (!match) {
    return 15 * 60;
  }

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
  // 外部登录（LDAP 等）在用户开启 2FA 时返回 requires_two_factor 标记：
  // 切换到动态码输入步骤（复用本地登录 2FA 视图与文案）并暂存凭证，不建立会话。
  // login.js 的本地登录路径已自行处理该标记，此处兜底覆盖未检查的 LDAP
  // （及未来其他外部登录）路径
  if (data && data.requires_two_factor) {
    const usernameInput = document.getElementById("ldap-username");
    const passwordInput = document.getElementById("ldap-password");
    // 凭证快照连同验证码字段一并采集：后端在失败计数达阈值后强制校验
    // 验证码，2FA 重试请求缺验证码字段会必然返回 captcha_required 且
    // 动态码视图无验证码输入框可满足，外部账户将被锁死在 2FA 步骤
    const captchaGroup = document.getElementById("ldap-captcha-group");
    const captchaInput = document.getElementById("ldap-captcha-input");
    const captchaImage = document.getElementById("ldap-captcha-image");
    const captchaRequired = Boolean(captchaGroup && !captchaGroup.hidden);
    const captcha = captchaRequired
      ? {
          captchaId: captchaImage?.dataset.captchaId || "",
          captchaText: (captchaInput?.value || "").trim()
        }
      : null;
    const switched = switchToExternalTwoFactorStep({
      endpoint: "/api/auth/login/ldap",
      username: usernameInput ? usernameInput.value.trim() : "",
      password: passwordInput ? passwordInput.value : "",
      rememberMe,
      captcha
    });
    if (!switched) {
      // 登录页 2FA 视图缺失（非预期场景）：不建立会话，避免绕过二次验证
      console.error("收到 2FA 要求但登录页动态码视图不可用，已中止登录");
    }
    return;
  }

  const { user } = data;

  SessionManager.setUser(user, rememberMe);

  window.location.href = "/main.html";
};

// ==========================================
// 外部登录（LDAP/SSO）2FA 契约
//
// 后端约定：外部账户开启 TOTP 时，/api/auth/login/ldap 等外部登录端点返回
// 与本地登录一致的 { requires_two_factor: true, ... } 响应，并接受携带
// code 字段（TOTP）的重试请求（重试同一端点）。login.js 的 LDAP 路径验证
// 通过后直接调用 loginUser，故在 loginUser 内统一接管；动态码表单提交经
// 捕获阶段监听拦截后携带 code 重试原端点。
// SSO 为后端整页跳转（login.js 仅重定向到 /api/auth/sso/login），前端无
// 完成端点可接管，无需处理。
// ==========================================

/** 待重试的外部登录 2FA 上下文（null 表示无待办） */
let pendingExternalTwoFactor = null;

/** 展示登录页错误条（复用本地登录的错误元素与样式） */
const showLoginError = (message) => {
  const errorEl = document.getElementById("login-error");
  if (!errorEl) {
    return;
  }
  errorEl.textContent = message;
  errorEl.classList.add("show");
};

/** 隐藏登录页错误条 */
const hideLoginError = () => {
  document.getElementById("login-error")?.classList.remove("show");
};

/**
 * 切换到动态码输入步骤（复用本地登录 2FA 视图与文案），暂存重试上下文
 * @param {Object} context 重试上下文（端点 + 凭证快照 + rememberMe）
 * @returns {boolean} 视图存在且切换成功
 */
const switchToExternalTwoFactorStep = (context) => {
  const loginView = document.getElementById("login-view");
  const twoFactorView = document.getElementById("two-factor-view");
  const codeInput = document.getElementById("two-factor-code");
  if (!loginView || !twoFactorView || !codeInput) {
    return false;
  }

  pendingExternalTwoFactor = context;
  hideLoginError();

  loginView.classList.remove("active");
  twoFactorView.classList.add("active");
  codeInput.value = "";
  codeInput.focus();
  return true;
};

/**
 * 处理外部登录 2FA 表单提交：携带 code 重试原登录端点
 */
const handleExternalTwoFactorSubmit = async () => {
  const pending = pendingExternalTwoFactor;
  if (!pending) {
    return;
  }

  const codeInput = document.getElementById("two-factor-code");
  const code = codeInput ? codeInput.value.trim() : "";
  if (!code) {
    showLoginError(t("login.code_required"));
    return;
  }

  // 复用本地 2FA 视图的提交按钮状态管理（.btn-text 结构与 login.js 一致）
  const submitButton = document.getElementById("two-factor-submit-btn");
  const btnText = submitButton?.querySelector(".btn-text");
  const originalText = btnText ? btnText.textContent : "";
  if (submitButton) {
    submitButton.disabled = true;
  }
  if (btnText) {
    btnText.textContent = t("common.processing");
  }
  hideLoginError();

  try {
    // 按契约携带 code 字段（TOTP）重试同一外部登录端点；
    // 采集到验证码快照时一并携带（后端达失败阈值后强制校验）
    const body = {
      username: pending.username,
      password: pending.password,
      remember_me: pending.rememberMe,
      code
    };
    if (pending.captcha) {
      body.captcha_id = pending.captcha.captchaId;
      body.captcha_text = pending.captcha.captchaText;
    }
    const result = await apiPost(pending.endpoint, body, { skipAuthCheck: true });

    if (result.success) {
      pendingExternalTwoFactor = null;
      loginUser(result.data, pending.rememberMe);
    } else {
      // 动态码错误保留待办状态，用户可直接重试（不清凭证快照）
      showLoginError(result.message || t("login.two_factor_failed"));
      if (codeInput) {
        codeInput.value = "";
        codeInput.focus();
      }
    }
  } catch (error) {
    console.error("外部登录 2FA 验证请求失败:", error);
    showLoginError(t("login.two_factor_failed"));
  } finally {
    if (submitButton) {
      submitButton.disabled = false;
    }
    if (btnText) {
      btnText.textContent = originalText;
    }
  }
};

// 动态码表单提交在捕获阶段拦截：存在外部 2FA 待办时由本模块处理，
// stopImmediatePropagation 阻止 login.js 本地 2FA 处理器在 tempAuthData
// 为空时把视图重置回登录页；无待办时完全不干预本地登录流程
document.addEventListener(
  "submit",
  (e) => {
    if (pendingExternalTwoFactor && e.target?.id === "two-factor-form") {
      e.preventDefault();
      e.stopImmediatePropagation();
      handleExternalTwoFactorSubmit();
    }
  },
  true
);

// 返回账号登录时清空外部 2FA 待办，避免残留状态影响后续登录
document.addEventListener(
  "click",
  (e) => {
    if (pendingExternalTwoFactor && e.target?.closest?.("#back-to-login-btn")) {
      pendingExternalTwoFactor = null;
    }
  },
  true
);

export const logoutUser = async () => {
  try {
    await apiPost("/api/auth/logout", {});
  } catch (error) {
    console.error("退出登录请求失败:", error);
  }

  redirectToLogin();
};

export const checkLoginStatus = async () => {
  if (!SessionManager.hasSession()) {
    return;
  }

  try {
    // 统一经 ApiClient（401 单飞刷新 + 自动重试 + 错误归一），
    // 不再绕过统一客户端裸 fetch（与其余请求同口径）
    const result = await apiGet("/api/auth/me");

    if (result.success) {
      return;
    }

    redirectToLogin();
  } catch (error) {
    console.error("检查登录状态失败:", error);
    redirectToLogin();
  }
};

// 语言切换后需重渲染当前用户信息（含悬浮提示文案）
let currentUserRenderHandler = null;

export const displayCurrentUser = () => {
  const user = SessionManager.getUser();
  const currentUserElement = document.getElementById("current-user");
  const userInfoElement = document.getElementById("user-info");

  if (!user || !currentUserElement) {
    return;
  }

  const render = () => {
    const welcome = t("auth.welcome", { username: user.username });
    currentUserElement.textContent = welcome;
    // 侧边栏收起后用户名不可见，由悬浮提示兜底展示
    userInfoElement?.setAttribute("data-tooltip", welcome);
  };

  render();

  if (currentUserRenderHandler) {
    window.removeEventListener("languagechange", currentUserRenderHandler);
  }
  currentUserRenderHandler = render;
  window.addEventListener("languagechange", currentUserRenderHandler);
};

export const initLogout = () => {
  const logoutBtn = document.getElementById("logout-btn");
  if (logoutBtn) {
    logoutBtn.addEventListener("click", logoutUser);
  }
};

// 修改密码（等保：改密后吊销全部令牌，强制重新登录）
export const initChangePassword = () => {
  const changeBtn = document.getElementById("change-password-btn");
  changeBtn?.addEventListener("click", async () => {
    const modal = await loadModal("change-password-modal");
    if (!modal) {
      return;
    }

    const form = elementCache.get("change-password-form");
    if (!form) {
      return;
    }

    // 防重入：请求进行中忽略重复提交（快速双击会发起两次改密请求）
    let submitting = false;
    form.onsubmit = async (e) => {
      e.preventDefault();
      if (submitting) {
        return;
      }
      const oldPassword = elementCache.getValue("change-password-old");
      const newPassword = elementCache.getValue("change-password-new");
      const confirmPassword = elementCache.getValue("change-password-confirm");

      if (!oldPassword || !newPassword) {
        showToast(t("auth.password_required"), "warning");
        return;
      }
      if (newPassword !== confirmPassword) {
        showToast(t("auth.password_mismatch"), "warning");
        return;
      }

      const submitButton = form.querySelector('[type="submit"]');
      submitting = true;
      if (submitButton) {
        submitButton.disabled = true;
      }

      try {
        const result = await apiPost("/api/auth/change-password", {
          old_password: oldPassword,
          new_password: newPassword
        });
        if (result.success) {
          showToast(result.message, "success");
          closeModal("change-password-modal");
          form.reset();
          // 改密后令牌全部失效，跳转登录页重新认证
          setTimeout(() => redirectToLogin(), 1200);
        } else {
          showToast(result.message, "error");
        }
      } catch (error) {
        console.error("修改密码请求失败:", error);
        showToast(t("auth.change_password_failed"), "error");
      } finally {
        submitting = false;
        if (submitButton) {
          submitButton.disabled = false;
        }
      }
    };

    openModal("change-password-modal");
  });
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
  if (!SessionManager.hasSession()) {
    return;
  }

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
