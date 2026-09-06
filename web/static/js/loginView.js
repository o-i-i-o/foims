/**
 * 登录页视图操作共享助手：login.js（本地登录）与 authManager.js
 * （外部认证 2FA 重试）共用同一错误条与 2FA 视图切换，避免双份实现。
 *
 * 注意：视图元素仅存在于登录页（index.html）；缺失时安全返回。
 */

/** 展示登录页错误条 */
export function showLoginErrorBar(message) {
  const errorEl = document.getElementById("login-error");
  if (!errorEl) {
    return;
  }
  errorEl.textContent = message;
  errorEl.classList.add("show");
}

/** 隐藏登录页错误条 */
export function hideLoginErrorBar() {
  document.getElementById("login-error")?.classList.remove("show");
}

/**
 * 激活 2FA 动态码输入视图（清空并聚焦输入框）
 * @returns {boolean} 视图元素齐全且切换成功
 */
export function activateTwoFactorView() {
  const loginView = document.getElementById("login-view");
  const twoFactorView = document.getElementById("two-factor-view");
  const codeInput = document.getElementById("two-factor-code");
  if (!loginView || !twoFactorView || !codeInput) {
    return false;
  }

  loginView.classList.remove("active");
  twoFactorView.classList.add("active");
  codeInput.value = "";
  codeInput.focus();
  return true;
}
