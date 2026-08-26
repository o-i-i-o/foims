// 登录页面专用脚本
//
// 设计来源: example-files/animatedlogin.html 的双栏布局 + 动画角色
// 功能保留: 密码登录 / 邮箱验证码登录 / 2FA / 忘记密码 / 记住我 / i18n / 会话检测
// 动画: 鼠标追踪、眨眼、密码可见偷看、输入互看、登录失败摇头+难过表情

// ES模块导入
import { apiGet, apiPost, refreshToken } from "./utils/apiClient.js";
import { closeModal, openModal, loadModal } from "./utils/modalLoader.js";
import { loginUser } from "./modules/authManager.js";
import { t, initI18n, changeLanguage } from "./utils/i18n.js";
import { SessionManager } from "./utils/sessionManager.js";

// 登录页角色（吉祥物）共用的瞳孔位移：登录失败时看向左下，密码聚焦时看向别处
const PUPIL_ERROR = "translate(-3px, 4px)";
const PUPIL_LOOKING_AWAY = "translate(-5px, -5px)";

// 角色眨眼/偷看动画的随机调度延迟
// eslint-disable-next-line sonarjs/pseudo-random -- 纯动画抖动延迟，非安全场景
const randomDelay = (spread, base) => Math.random() * spread + base;

/**
 * 登录管理器类
 * 负责处理登录页面的所有逻辑和状态
 */
class LoginManager {
  constructor() {
    // 状态枚举
    this.State = {
      INIT: "init",
      SUBMITTING: "submitting",
      TWO_FACTOR: "two_factor"
    };

    // 登录模式枚举（枚举值与 index.html 的 data-tab/id 对应，非密钥）
    this.LoginMode = {
      // eslint-disable-next-line sonarjs/no-hardcoded-passwords -- 登录方式标识，与 HTML 的 data-tab="password-login" 联动
      PASSWORD: "password-login",
      EMAIL: "email-login",
      LDAP: "ldap-login",
      SSO: "sso-login"
    };

    // 当前状态和模式
    this.currentState = this.State.INIT;
    this.currentMode = this.LoginMode.PASSWORD;

    // 缓存 DOM 元素
    this.dom = {
      form: document.getElementById("login-form"),
      twoFactorForm: document.getElementById("two-factor-form"),

      // Tabs
      tabs: document.querySelectorAll(".tab-btn"),
      loginSections: document.querySelectorAll(".login-section"),

      // Views
      views: document.querySelectorAll(".view"),
      loginView: document.getElementById("login-view"),
      twoFactorView: document.getElementById("two-factor-view"),

      // Password Login Inputs
      usernameInput: document.getElementById("username"),
      passwordInput: document.getElementById("password"),

      // Password visibility toggle
      togglePasswordBtn: document.getElementById("toggle-password"),
      eyeIcon: document.getElementById("eye-icon"),
      eyeOffIcon: document.getElementById("eye-off-icon"),

      // Email Login Inputs
      emailInput: document.getElementById("email-login-input"),
      emailCodeInput: document.getElementById("email-login-code"),
      sendLoginCodeBtn: document.getElementById("send-login-code-btn"),

      // LDAP Login Inputs
      ldapUsernameInput: document.getElementById("ldap-username"),
      ldapPasswordInput: document.getElementById("ldap-password"),

      // Common Inputs
      rememberMeCheckbox: document.getElementById("remember-me"),
      submitBtn: document.getElementById("login-submit-btn"),
      errorMsg: document.getElementById("login-error"),

      // 2FA Inputs
      twoFactorCodeInput: document.getElementById("two-factor-code"),
      twoFactorSubmitBtn: document.getElementById("two-factor-submit-btn"),
      backToLoginBtn: document.getElementById("back-to-login-btn"),

      // Forgot Password（模态框模板位于 modals/auth/，init 时加载后再取元素）
      forgotPasswordLink: document.getElementById("forgot-password-link"),
      forgotPasswordForm: null,
      forgotPasswordError: null,
      forgotPasswordSuccess: null
    };

    // 绑定方法上下文
    this.handleTabClick = this.handleTabClick.bind(this);
    this.handleSubmit = this.handleSubmit.bind(this);
    this.handleTwoFactorSubmit = this.handleTwoFactorSubmit.bind(this);
    this.handleSendLoginCode = this.handleSendLoginCode.bind(this);
    this.handleForgotPassword = this.handleForgotPassword.bind(this);
    this.resetToInitState = this.resetToInitState.bind(this);

    // 临时存储 2FA 所需的用户名/密码
    this.tempAuthData = null;
  }

  /**
   * 初始化
   */
  async init() {
    await initI18n();

    // 语言切换：项目约定先按浏览器语言应用，再由页面上的语言按钮切换。
    document.getElementById("language-selector")?.addEventListener("change", (e) => {
      changeLanguage(e.target.value);
    });

    // 检查是否处于初始化模式（config.toml [init].enabled = true）
    // 若是，则跳转到初始化页，不继续登录流程
    if (await this.checkInitMode()) {
      return;
    }

    // 登录页未加载 eventManager（其模态关闭委托只随 main.html 安装），
    // 此处补齐等价委托：忘记密码等模态的关闭按钮/背景点击才能关闭
    document.addEventListener("click", (e) => {
      if (e.target.classList.contains("modal")) {
        closeModal(e.target.id);
        return;
      }
      const closeTrigger = e.target.closest("[data-modal-id]");
      if (closeTrigger) {
        closeModal(closeTrigger.dataset.modalId);
      }
    });

    // SSO 回跳错误提示（须在 cleanUrlParams 清理地址栏前捕获）
    const ssoErrorKey = new URLSearchParams(window.location.search).get("sso_error");

    this.cleanUrlParams();
    this.checkLoginStatus();

    // 按后端配置显示 LDAP / SSO 登录方式
    await this.setupAuthMethods();

    // 站点根证书下载入口（有 CA 时才显示）
    this.setupCaDownload();

    // 图形验证码图片点击刷新（连续登录失败触发后显示）
    document
      .getElementById("captcha-image")
      ?.addEventListener("click", () => this.refreshCaptcha("password"));
    document
      .getElementById("ldap-captcha-image")
      ?.addEventListener("click", () => this.refreshCaptcha("ldap"));

    if (ssoErrorKey) {
      this.showError(t(ssoErrorKey));
    }

    // 绑定 Tabs 事件
    this.dom.tabs.forEach((tab) => {
      tab.addEventListener("click", (e) => this.handleTabClick(e.target));
    });

    // 绑定表单提交事件
    this.dom.form.addEventListener("submit", this.handleSubmit);
    this.dom.twoFactorForm.addEventListener("submit", this.handleTwoFactorSubmit);

    // 绑定验证码发送事件
    this.dom.sendLoginCodeBtn.addEventListener("click", this.handleSendLoginCode);

    // 绑定返回按钮事件
    this.dom.backToLoginBtn.addEventListener("click", this.resetToInitState);

    // 绑定忘记密码事件（先加载独立模板模态框，再缓存其内部元素）
    await loadModal("forgot-password-modal");
    this.dom.forgotPasswordForm = document.getElementById("forgot-password-form");
    this.dom.forgotPasswordError = document.getElementById("forgot-password-error");
    this.dom.forgotPasswordSuccess = document.getElementById("forgot-password-success");

    this.dom.forgotPasswordLink.addEventListener("click", (e) => {
      e.preventDefault();
      openModal("forgot-password-modal");
    });
    this.dom.forgotPasswordForm?.addEventListener("submit", this.handleForgotPassword);

    // 初始化表单验证规则
    this.updateFormValidation();

    // 初始化密码可见性切换 + 动画角色
    this.initPasswordToggle();
    this.initCharactersAnimation();
  }

  /**
   * 按后端 /api/auth/methods 返回的开关显示 LDAP / SSO 标签
   * （请求失败时保持隐藏，_fail-safe）；
   * 邮箱登录依赖 SMTP，未配置时同样静默隐藏
   */
  async setupAuthMethods() {
    try {
      const result = await apiGet("/api/auth/methods");
      if (!result.success || !result.data) {
        return;
      }

      const visibility = {
        "email-login": result.data.email !== false,
        "ldap-login": Boolean(result.data.ldap),
        "sso-login": Boolean(result.data.sso)
      };
      this.dom.tabs.forEach((tab) => {
        const visible = visibility[tab.dataset.tab];
        if (visible !== undefined) {
          tab.hidden = !visible;
        }
      });
    } catch (error) {
      console.error("获取认证方式失败:", error);
    }
  }

  /**
   * 站点根证书下载入口：CA 证书是公开数据，无 CA 时保持隐藏
   */
  async setupCaDownload() {
    try {
      const result = await apiGet("/api/certificate/ca/info");
      if (!result.success || !result.data?.available) {
        return;
      }
      const entry = document.getElementById("ca-download-entry");
      if (entry) {
        entry.hidden = false;
      }
    } catch (error) {
      // 查询失败保持隐藏即可，不影响登录流程
      console.error("获取根证书状态失败:", error);
    }
  }

  /**
   * 更新表单验证规则
   * 根据当前登录模式，动态设置 input 的 required 属性
   * 避免浏览器阻止提交隐藏的 required 字段
   */
  updateFormValidation() {
    const mode = this.currentMode;
    const isPassword = mode === this.LoginMode.PASSWORD;
    const isEmail = mode === this.LoginMode.EMAIL;
    const isLdap = mode === this.LoginMode.LDAP;

    this.dom.usernameInput.required = isPassword;
    this.dom.passwordInput.required = isPassword;

    this.dom.emailInput.required = isEmail;
    this.dom.emailCodeInput.required = isEmail;

    if (this.dom.ldapUsernameInput) {
      this.dom.ldapUsernameInput.required = isLdap;
      this.dom.ldapPasswordInput.required = isLdap;
    }
  }

  /**
   * 处理 Tab 切换
   */
  handleTabClick(targetBtn) {
    if (this.currentState === this.State.SUBMITTING) {
      return;
    }

    // 移除所有 active 类
    this.dom.tabs.forEach((btn) => {
      btn.classList.remove("active");
      btn.setAttribute("aria-selected", "false");
    });
    this.dom.loginSections.forEach((section) => section.classList.remove("active"));

    // 激活当前 Tab（区块 id 与模式名一一对应：{mode}-section）
    targetBtn.classList.add("active");
    targetBtn.setAttribute("aria-selected", "true");
    this.currentMode = targetBtn.dataset.tab;

    const section = document.getElementById(`${this.currentMode}-section`);
    if (section) {
      section.classList.add("active");
    }

    // 更新验证规则
    this.updateFormValidation();

    this.clearError();
  }

  /**
   * 清除 URL 参数
   */
  cleanUrlParams() {
    if (window.location.search) {
      window.history.replaceState({}, document.title, window.location.pathname);
    }
  }

  /**
   * 处理登录表单提交
   */
  async handleSubmit(e) {
    e.preventDefault();
    this.clearError();

    if (this.currentState === this.State.SUBMITTING) {
      return;
    }

    const rememberMe = this.dom.rememberMeCheckbox.checked;

    if (this.currentMode === this.LoginMode.PASSWORD) {
      const username = this.dom.usernameInput.value.trim();
      const password = this.dom.passwordInput.value;
      if (!username || !password) {
        this.showError(t("login.username_password_required"));
        return;
      }
      await this.submitPasswordLogin(username, password, rememberMe);
    } else if (this.currentMode === this.LoginMode.LDAP) {
      const username = this.dom.ldapUsernameInput.value.trim();
      const password = this.dom.ldapPasswordInput.value;
      if (!username || !password) {
        this.showError(t("login.username_password_required"));
        return;
      }
      // LDAP 表单要求验证码时先校验输入
      if (this.isCaptchaVisible("ldap") && !this.getCaptchaInput("ldap")) {
        this.showError(t("login.captcha_required"));
        return;
      }
      await this.submitLdapLogin(username, password, rememberMe);
    } else if (this.currentMode === this.LoginMode.SSO) {
      // SSO：跳转后端发起 OIDC 授权码流程
      window.location.href = "/api/auth/sso/login";
    } else {
      const email = this.dom.emailInput.value.trim();
      const code = this.dom.emailCodeInput.value.trim();
      if (!email || !code) {
        this.showError(t("login.email_code_required"));
        return;
      }
      await this.submitEmailLogin(email, code, rememberMe);
    }
  }

  // ==================== 图形验证码（连续失败触发） ====================

  /** 每个表单（password / ldap）的验证码状态 */
  captchaState = { password: { id: null, failures: 0 }, ldap: { id: null, failures: 0 } };

  isCaptchaVisible(form) {
    const group = document.getElementById(form === "ldap" ? "ldap-captcha-group" : "captcha-group");
    return Boolean(group && !group.hidden);
  }

  getCaptchaInput(form) {
    const input = document.getElementById(form === "ldap" ? "ldap-captcha-input" : "captcha-input");
    return (input?.value || "").trim();
  }

  /** 显示验证码并加载新图（点击图片可刷新） */
  async showCaptcha(form) {
    const group = document.getElementById(form === "ldap" ? "ldap-captcha-group" : "captcha-group");
    if (!group) {
      return;
    }
    group.hidden = false;
    await this.refreshCaptcha(form);
  }

  /** 拉取新验证码图片 */
  async refreshCaptcha(form) {
    const image = document.getElementById(form === "ldap" ? "ldap-captcha-image" : "captcha-image");
    if (!image) {
      return;
    }
    try {
      const result = await apiGet("/api/auth/captcha");
      if (result.success && result.data) {
        this.captchaState[form].id = result.data.captcha_id;
        image.innerHTML = result.data.svg;
      }
    } catch (error) {
      console.error("加载验证码失败:", error);
    }
  }

  /** 登录失败后的验证码联动：累计失败次数并按需显示验证码 */
  handleCaptchaOnFailure(form, messageKey) {
    const state = this.captchaState[form];
    state.failures += 1;
    const input = document.getElementById(form === "ldap" ? "ldap-captcha-input" : "captcha-input");
    if (input) {
      input.value = "";
    }

    const required =
      messageKey === "server.auth.captcha_required" || messageKey === "server.auth.captcha_invalid";
    if (required || state.failures >= 3) {
      this.showCaptcha(form);
    }
  }

  /**
   * 提交密码登录
   */
  async submitPasswordLogin(username, password, rememberMe) {
    this.setLoading(true);

    try {
      const body = { username, password, remember_me: rememberMe };
      if (this.isCaptchaVisible("password")) {
        body.captcha_id = this.captchaState.password.id;
        body.captcha_text = this.getCaptchaInput("password");
      }
      const result = await apiPost("/api/auth/login", body, { skipAuthCheck: true });

      if (result.success) {
        if (result.data && result.data.requires_two_factor) {
          // 保存凭证用于 2FA
          this.tempAuthData = { username, password, rememberMe };
          this.switchToTwoFactorView();
        } else {
          loginUser(result.data, rememberMe);
        }
      } else {
        this.showError(this.formatErrorMessage(result.message));
        this.handleCaptchaOnFailure("password", result.message);
      }
    } catch (error) {
      this.handleNetworkError(error);
    } finally {
      this.setLoading(false);
    }
  }

  /**
   * 提交邮箱登录
   */
  async submitEmailLogin(email, code, rememberMe) {
    this.setLoading(true);

    try {
      const result = await apiPost(
        "/api/auth/login/email",
        { email, code, remember_me: rememberMe },
        { skipAuthCheck: true }
      );

      if (result.success) {
        if (result.data && result.data.requires_two_factor) {
          // 邮箱登录通过，但仍需 2FA（如果开启了）
          // 后端 login_with_email_code 返回了 username。
          this.tempAuthData = {
            username: result.data.username,
            password: "", // 邮箱登录没有密码
            rememberMe
          };
          this.switchToTwoFactorView();
        } else {
          loginUser(result.data, rememberMe);
        }
      } else {
        this.showError(this.formatErrorMessage(result.message));
      }
    } catch (error) {
      this.handleNetworkError(error);
    } finally {
      this.setLoading(false);
    }
  }

  /**
   * 提交 LDAP 登录
   */
  async submitLdapLogin(username, password, rememberMe) {
    this.setLoading(true);

    try {
      const body = { username, password, remember_me: rememberMe };
      if (this.isCaptchaVisible("ldap")) {
        body.captcha_id = this.captchaState.ldap.id;
        body.captcha_text = this.getCaptchaInput("ldap");
      }
      const result = await apiPost("/api/auth/login/ldap", body, { skipAuthCheck: true });

      if (result.success) {
        loginUser(result.data, rememberMe);
      } else {
        this.showError(this.formatErrorMessage(result.message));
        this.handleCaptchaOnFailure("ldap", result.message);
      }
    } catch (error) {
      this.handleNetworkError(error);
    } finally {
      this.setLoading(false);
    }
  }

  /**
   * 切换到 2FA 视图
   */
  switchToTwoFactorView() {
    this.currentState = this.State.TWO_FACTOR;
    this.clearError();

    // 切换视图
    this.dom.loginView.classList.remove("active");
    this.dom.twoFactorView.classList.add("active");

    // 聚焦输入
    this.dom.twoFactorCodeInput.value = "";
    this.dom.twoFactorCodeInput.focus();
  }

  /**
   * 处理 2FA 提交
   */
  async handleTwoFactorSubmit(e) {
    e.preventDefault();
    if (!this.tempAuthData) {
      this.resetToInitState();
      return;
    }

    const code = this.dom.twoFactorCodeInput.value.trim();
    if (!code) {
      this.showError(t("login.code_required"));
      return;
    }

    this.setLoading(true);

    const { username, password, rememberMe } = this.tempAuthData;
    this.tempAuthData = null;

    try {
      const result = await apiPost(
        "/api/auth/login/two-factor",
        {
          username,
          password: password || "",
          two_factor_code: code,
          remember_me: rememberMe
        },
        { skipAuthCheck: true }
      );

      if (result.success) {
        loginUser(result.data, rememberMe);
      } else {
        this.showError(result.message || t("login.two_factor_failed"));
      }
    } catch (error) {
      this.handleNetworkError(error);
    } finally {
      this.setLoading(false);
    }
  }

  /**
   * 发送登录验证码
   */
  async handleSendLoginCode() {
    const email = this.dom.emailInput.value.trim();
    if (!email) {
      this.showError(t("login.email_required_error"));
      return;
    }

    this.setLoadingState(this.dom.sendLoginCodeBtn, true);

    try {
      const result = await apiPost("/api/auth/login/send-code", { email }, { skipAuthCheck: true });

      if (result.success) {
        this.startCountdown(this.dom.sendLoginCodeBtn, 60);
      } else {
        this.showError(result.message || t("login.send_failed"));
        this.setLoadingState(this.dom.sendLoginCodeBtn, false);
      }
    } catch (error) {
      this.handleNetworkError(error);
      this.setLoadingState(this.dom.sendLoginCodeBtn, false);
    }
  }

  /**
   * 通用倒计时
   */
  startCountdown(btn, seconds) {
    let countdown = seconds;
    btn.textContent = `${t("login.code_sent")}(${countdown})`;
    btn.disabled = true;

    const timer = setInterval(() => {
      countdown--;
      if (countdown <= 0) {
        clearInterval(timer);
        this.setLoadingState(btn, false);
      } else {
        btn.textContent = `${t("login.code_sent")}(${countdown})`;
      }
    }, 1000);
  }

  /**
   * 设置按钮加载状态
   */
  setLoadingState(btn, isLoading) {
    if (isLoading) {
      btn.dataset.originalText = btn.textContent;
      btn.textContent = t("login.sending");
      btn.disabled = true;
      this.clearError();
    } else {
      btn.textContent = t("login.send_code"); // 恢复默认文本
      btn.disabled = false;
    }
  }

  /**
   * 重置回初始状态
   */
  resetToInitState() {
    this.currentState = this.State.INIT;
    this.tempAuthData = null;
    this.clearError();

    // 切换视图
    this.dom.twoFactorView.classList.remove("active");
    this.dom.loginView.classList.add("active");

    // 重置 2FA 表单
    this.dom.twoFactorCodeInput.value = "";
    this.resetButtonContent(this.dom.twoFactorSubmitBtn, t("login.verify_and_sign_in"));
    this.dom.twoFactorSubmitBtn.disabled = false;
  }

  /**
   * 设置主提交按钮加载状态
   */
  setLoading(isLoading) {
    if (isLoading) {
      this.currentState = this.State.SUBMITTING;
      if (this.dom.twoFactorView.classList.contains("active")) {
        this.dom.twoFactorSubmitBtn.disabled = true;
        this.setButtonLoadingContent(this.dom.twoFactorSubmitBtn, t("common.processing"));
      } else {
        this.dom.submitBtn.disabled = true;
        this.setButtonLoadingContent(this.dom.submitBtn, t("common.processing"));
      }
    } else {
      this.currentState = this.dom.twoFactorView.classList.contains("active")
        ? this.State.TWO_FACTOR
        : this.State.INIT;

      this.dom.twoFactorSubmitBtn.disabled = false;
      this.resetButtonContent(this.dom.twoFactorSubmitBtn, t("login.verify_and_sign_in"));

      this.dom.submitBtn.disabled = false;
      this.resetButtonContent(this.dom.submitBtn, t("login.sign_in"));
    }
  }

  /**
   * 设置带 .btn-text / .btn-hover-content 结构的按钮为加载文案
   * （仅修改可见的 .btn-text，保留 hover 内容）
   */
  setButtonLoadingContent(btn, text) {
    const txt = btn.querySelector(".btn-text");
    if (txt) {
      txt.textContent = text;
    } else {
      btn.textContent = text;
    }
  }

  /**
   * 恢复按钮文案（同步恢复 .btn-text 与 .btn-hover-content 内文本）
   */
  resetButtonContent(btn, text) {
    const txt = btn.querySelector(".btn-text");
    if (txt) {
      txt.textContent = text;
    }
    const hoverTxt = btn.querySelector(".btn-hover-content > span");
    if (hoverTxt) {
      hoverTxt.textContent = text;
    }
  }

  formatErrorMessage(message) {
    if (!message) {
      return t("api.failed");
    }

    // 尝试使用 i18n 翻译
    if (message.includes(".")) {
      const translated = t(message);
      if (translated !== message) {
        return translated;
      }
    }

    if (message.includes("账户已禁用")) {
      return t("api.account_disabled");
    }
    if (message.includes("失败次数过多")) {
      return t("login.too_many_attempts");
    }
    if (message.includes("系统未初始化")) {
      return t("login.system_not_init");
    }
    if (message.includes("SMTP未配置")) {
      return t("login.email_service_not_configed");
    }

    return message;
  }

  showError(msg) {
    this.dom.errorMsg.textContent = msg;
    this.dom.errorMsg.classList.add("show");
    // 触发角色摇头 + 难过表情动画
    this.triggerLoginError();
  }

  clearError() {
    this.dom.errorMsg.textContent = "";
    this.dom.errorMsg.classList.remove("show");
    // 恢复角色正常姿态
    this.recoverLoginError();
  }

  handleNetworkError(error) {
    let msg = t("login.network_error");
    if (error.message && error.message.includes("Network")) {
      msg = t("login.network_failed");
    } else if (error.message && error.message.includes("timeout")) {
      msg = t("login.timeout");
    }
    this.showError(msg);
  }

  async handleForgotPassword(e) {
    e.preventDefault();
    const { forgotPasswordError, forgotPasswordSuccess } = this.dom;
    if (!forgotPasswordError || !forgotPasswordSuccess) {
      return;
    }

    forgotPasswordError.textContent = "";
    forgotPasswordError.classList.remove("show");
    forgotPasswordSuccess.textContent = "";
    forgotPasswordSuccess.classList.remove("show");

    const formData = new FormData(e.target);
    const email = formData.get("email");
    const btn = e.target.querySelector('button[type="submit"]');

    const originalText = btn.textContent;
    btn.disabled = true;
    btn.textContent = t("login.sending");

    try {
      const result = await apiPost("/api/auth/forgot-password", { email }, { skipAuthCheck: true });

      if (result.success) {
        forgotPasswordSuccess.textContent = t("login.reset_email_sent");
        forgotPasswordSuccess.classList.add("show");
        e.target.reset();
        setTimeout(() => {
          closeModal("forgot-password-modal");
          forgotPasswordSuccess.classList.remove("show");
        }, 3000);
      } else {
        forgotPasswordError.textContent = result.message || t("login.send_failed");
        forgotPasswordError.classList.add("show");
      }
    } catch (error) {
      let msg = t("login.send_failed");
      if (error.message && error.message.includes("Network")) {
        msg = t("login.network_failed");
      }
      forgotPasswordError.textContent = msg;
      forgotPasswordError.classList.add("show");
    } finally {
      btn.disabled = false;
      btn.textContent = originalText;
    }
  }

  /**
   * 检查是否处于初始化模式
   * 后端 config.toml [init].enabled = true 时，前端跳转到初始化页
   * @returns {Promise<boolean>} true 表示已跳转（初始化模式），调用方应中止登录流程
   */
  async checkInitMode() {
    try {
      const result = await apiGet("/api/auth/init-status");
      if (result.success && result.data && result.data.init_enabled) {
        window.location.href = "/init_index.html";
        return true;
      }
    } catch {
      // 接口不可用时按非初始化模式处理（保持登录页可用），解析失败属预期回退
    }
    return false;
  }

  async checkLoginStatus() {
    if (!SessionManager.hasSession()) {
      return;
    }

    try {
      const response = await fetch("/api/auth/me", {
        credentials: "include"
      });

      if (response.ok) {
        window.location.href = "/main.html";
        return;
      }

      if (response.status === 401) {
        const refreshed = await refreshToken();
        if (refreshed) {
          window.location.href = "/main.html";
          return;
        }
        SessionManager.clear();
      }
    } catch {
      // 登录态探测失败不影响登录页可用性，静默回退到未登录展示
    }
  }

  clearStorage() {
    SessionManager.clear();
  }

  // ============================================================
  // 密码可见性切换
  // ============================================================
  initPasswordToggle() {
    this.showPassword = false;
    const toggleBtn = this.dom.togglePasswordBtn;
    if (!toggleBtn) {
      return;
    }

    toggleBtn.addEventListener("click", () => {
      this.showPassword = !this.showPassword;
      this.dom.passwordInput.type = this.showPassword ? "text" : "password";
      this.dom.eyeIcon.style.display = this.showPassword ? "none" : "block";
      this.dom.eyeOffIcon.style.display = this.showPassword ? "block" : "none";
      this.updateCharacters();
      // 密码可见时，可能触发紫色角色偷看
      if (this.showPassword) {
        this.schedulePeek();
      }
    });
  }

  // ============================================================
  // 动画角色（移植自 example-files/animatedlogin.html）
  // ============================================================
  initCharactersAnimation() {
    // 鼠标 / 输入状态
    this.mouseX = 0;
    this.mouseY = 0;
    this.isTyping = false;
    this.isLookingAtEachOther = false;
    this.isPasswordFocused = false;
    this.isLoginError = false;
    this.isPurpleBlinking = false;
    this.isBlackBlinking = false;
    this.isPurplePeeking = false;
    this.typingTimer = null;
    this.errorRecoverTimer = null;

    // 鼠标移动追踪
    document.addEventListener("mousemove", (e) => {
      this.mouseX = e.clientX;
      this.mouseY = e.clientY;
      if (!this.isTyping && !this.isLoginError) {
        this.updateCharacters();
      }
    });

    // 用户名输入框：输入时角色互看
    this.dom.usernameInput.addEventListener("focus", () => this.setTyping(true));
    this.dom.usernameInput.addEventListener("blur", () => this.setTyping(false));
    this.dom.usernameInput.addEventListener("input", () => this.updateCharacters());

    // 密码输入框：聚焦时角色看向别处（保护密码）
    this.dom.passwordInput.addEventListener("focus", () => {
      this.isPasswordFocused = true;
      this.updateCharacters();
    });
    this.dom.passwordInput.addEventListener("blur", () => {
      this.isPasswordFocused = false;
      this.updateCharacters();
    });
    this.dom.passwordInput.addEventListener("input", () => this.updateCharacters());

    // 启动眨眼定时器
    this.scheduleBlinkPurple();
    this.scheduleBlinkBlack();

    // 首次渲染
    this.updateCharacters();
  }

  setTyping(typing) {
    this.isTyping = typing;
    if (typing) {
      this.isLookingAtEachOther = true;
      clearTimeout(this.typingTimer);
      this.typingTimer = setTimeout(() => {
        this.isLookingAtEachOther = false;
        this.updateCharacters();
      }, 800);
    } else {
      this.isLookingAtEachOther = false;
    }
    this.updateCharacters();
  }

  // 紫色角色眨眼
  scheduleBlinkPurple() {
    setTimeout(
      () => {
        this.isPurpleBlinking = true;
        this.updateCharacters();
        setTimeout(() => {
          this.isPurpleBlinking = false;
          this.updateCharacters();
          this.scheduleBlinkPurple();
        }, 150);
      },
      randomDelay(4000, 3000)
    );
  }

  // 黑色角色眨眼
  scheduleBlinkBlack() {
    setTimeout(
      () => {
        this.isBlackBlinking = true;
        this.updateCharacters();
        setTimeout(() => {
          this.isBlackBlinking = false;
          this.updateCharacters();
          this.scheduleBlinkBlack();
        }, 150);
      },
      randomDelay(4000, 3000)
    );
  }
  // 密码可见时，紫色角色偶尔偷看
  schedulePeek() {
    if (this.dom.passwordInput.value.length > 0 && this.showPassword) {
      setTimeout(
        () => {
          if (this.dom.passwordInput.value.length > 0 && this.showPassword) {
            this.isPurplePeeking = true;
            this.updateCharacters();
            setTimeout(() => {
              this.isPurplePeeking = false;
              this.updateCharacters();
              this.schedulePeek();
            }, 800);
          }
        },
        randomDelay(3000, 2000)
      );
    }
  }

  // 根据鼠标计算角色朝向
  calcPosition(el) {
    const rect = el.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 3;
    const dx = this.mouseX - cx;
    const dy = this.mouseY - cy;
    const faceX = Math.max(-15, Math.min(15, dx / 20));
    const faceY = Math.max(-10, Math.min(10, dy / 30));
    const bodySkew = Math.max(-6, Math.min(6, -dx / 120));
    return { faceX, faceY, bodySkew };
  }

  calcPupilOffset(el, maxDist) {
    const rect = el.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const dx = this.mouseX - cx;
    const dy = this.mouseY - cy;
    const dist = Math.min(Math.sqrt(dx * dx + dy * dy), maxDist);
    const angle = Math.atan2(dy, dx);
    return { x: Math.cos(angle) * dist, y: Math.sin(angle) * dist };
  }

  // 更新所有角色姿态：身体/眼睛/瞳孔按角色拆分到独立方法
  updateCharacters() {
    const purple = document.getElementById("char-purple");
    const black = document.getElementById("char-black");
    const orange = document.getElementById("char-orange");
    const yellow = document.getElementById("char-yellow");
    if (!purple || !black || !orange || !yellow) {
      return;
    }

    const pwdLen = this.dom.passwordInput.value.length;
    const isShowingPwd = pwdLen > 0 && this.showPassword;
    // 密码框聚焦且密码不可见时，角色看向别处
    const isLookingAway = this.isPasswordFocused && !this.showPassword;
    const ctx = { isShowingPwd, isLookingAway };

    this._updatePurpleCharacter(ctx, this.calcPosition(purple));
    this._updateBlackCharacter(ctx, this.calcPosition(black));
    this._updateOrangeCharacter(ctx, this.calcPosition(orange));
    this._updateYellowCharacter(ctx, this.calcPosition(yellow));
  }

  // 紫色角色：身体倾斜 + 眼睛/瞳孔（含眨眼与密码可见时的偷看）
  _updatePurpleCharacter({ isShowingPwd, isLookingAway }, purplePos) {
    const purple = document.getElementById("char-purple");
    if (isShowingPwd) {
      purple.style.transform = "skewX(0deg)";
      purple.style.height = "370px";
    } else if (isLookingAway) {
      purple.style.transform = "skewX(-14deg) translateX(-20px)";
      purple.style.height = "410px";
    } else if (this.isTyping) {
      purple.style.transform = `skewX(${(purplePos.bodySkew || 0) - 12}deg) translateX(40px)`;
      purple.style.height = "410px";
    } else {
      purple.style.transform = `skewX(${purplePos.bodySkew}deg)`;
      purple.style.height = "370px";
    }

    const purpleEyes = document.getElementById("purple-eyes");
    const purpleEyeL = document.getElementById("purple-eye-l");
    const purpleEyeR = document.getElementById("purple-eye-r");
    const purplePupilL = document.getElementById("purple-pupil-l");
    const purplePupilR = document.getElementById("purple-pupil-r");
    if (purpleEyes && purpleEyeL && purpleEyeR && purplePupilL && purplePupilR) {
      purpleEyeL.style.height = this.isPurpleBlinking ? "2px" : "18px";
      purpleEyeR.style.height = this.isPurpleBlinking ? "2px" : "18px";

      if (this.isLoginError) {
        purpleEyes.style.left = "30px";
        purpleEyes.style.top = "55px";
        purplePupilL.style.transform = PUPIL_ERROR;
        purplePupilR.style.transform = PUPIL_ERROR;
      } else if (isLookingAway) {
        purpleEyes.style.left = "20px";
        purpleEyes.style.top = "25px";
        purplePupilL.style.transform = PUPIL_LOOKING_AWAY;
        purplePupilR.style.transform = PUPIL_LOOKING_AWAY;
      } else if (isShowingPwd) {
        purpleEyes.style.left = "20px";
        purpleEyes.style.top = "35px";
        const px = this.isPurplePeeking ? 4 : -4;
        const py = this.isPurplePeeking ? 5 : -4;
        purplePupilL.style.transform = `translate(${px}px, ${py}px)`;
        purplePupilR.style.transform = `translate(${px}px, ${py}px)`;
      } else if (this.isLookingAtEachOther) {
        purpleEyes.style.left = "55px";
        purpleEyes.style.top = "65px";
        purplePupilL.style.transform = "translate(3px, 4px)";
        purplePupilR.style.transform = "translate(3px, 4px)";
      } else {
        purpleEyes.style.left = `${45 + purplePos.faceX}px`;
        purpleEyes.style.top = `${40 + purplePos.faceY}px`;
        const po = this.calcPupilOffset(purpleEyeL, 5);
        purplePupilL.style.transform = `translate(${po.x}px, ${po.y}px)`;
        purplePupilR.style.transform = `translate(${po.x}px, ${po.y}px)`;
      }
    }
  }

  // 黑色角色：身体倾斜 + 眼睛/瞳孔（含眨眼）
  _updateBlackCharacter({ isShowingPwd, isLookingAway }, blackPos) {
    const black = document.getElementById("char-black");
    if (isShowingPwd) {
      black.style.transform = "skewX(0deg)";
    } else if (isLookingAway) {
      black.style.transform = "skewX(12deg) translateX(-10px)";
    } else if (this.isLookingAtEachOther) {
      black.style.transform = `skewX(${(blackPos.bodySkew || 0) * 1.5 + 10}deg) translateX(20px)`;
    } else if (this.isTyping) {
      black.style.transform = `skewX(${(blackPos.bodySkew || 0) * 1.5}deg)`;
    } else {
      black.style.transform = `skewX(${blackPos.bodySkew}deg)`;
    }

    const blackEyes = document.getElementById("black-eyes");
    const blackEyeL = document.getElementById("black-eye-l");
    const blackEyeR = document.getElementById("black-eye-r");
    const blackPupilL = document.getElementById("black-pupil-l");
    const blackPupilR = document.getElementById("black-pupil-r");
    if (blackEyes && blackEyeL && blackEyeR && blackPupilL && blackPupilR) {
      blackEyeL.style.height = this.isBlackBlinking ? "2px" : "16px";
      blackEyeR.style.height = this.isBlackBlinking ? "2px" : "16px";

      if (this.isLoginError) {
        blackEyes.style.left = "15px";
        blackEyes.style.top = "40px";
        blackPupilL.style.transform = PUPIL_ERROR;
        blackPupilR.style.transform = PUPIL_ERROR;
      } else if (isLookingAway) {
        blackEyes.style.left = "10px";
        blackEyes.style.top = "20px";
        blackPupilL.style.transform = "translate(-4px, -5px)";
        blackPupilR.style.transform = "translate(-4px, -5px)";
      } else if (isShowingPwd) {
        blackEyes.style.left = "10px";
        blackEyes.style.top = "28px";
        blackPupilL.style.transform = "translate(-4px, -4px)";
        blackPupilR.style.transform = "translate(-4px, -4px)";
      } else if (this.isLookingAtEachOther) {
        blackEyes.style.left = "32px";
        blackEyes.style.top = "12px";
        blackPupilL.style.transform = "translate(0px, -4px)";
        blackPupilR.style.transform = "translate(0px, -4px)";
      } else {
        blackEyes.style.left = `${26 + blackPos.faceX}px`;
        blackEyes.style.top = `${32 + blackPos.faceY}px`;
        const bo = this.calcPupilOffset(blackEyeL, 4);
        blackPupilL.style.transform = `translate(${bo.x}px, ${bo.y}px)`;
        blackPupilR.style.transform = `translate(${bo.x}px, ${bo.y}px)`;
      }
    }
  }

  // 橙色角色：身体倾斜 + 眼睛/瞳孔 + 失败时的难过嘴
  _updateOrangeCharacter({ isShowingPwd, isLookingAway }, orangePos) {
    const orange = document.getElementById("char-orange");
    const orangeMouth = document.getElementById("orange-mouth");
    if (this.isLoginError && orangeMouth) {
      orangeMouth.style.left = `${80 + orangePos.faceX}px`;
      orangeMouth.style.top = "130px";
    }
    if (isShowingPwd) {
      orange.style.transform = "skewX(0deg)";
    } else {
      orange.style.transform = `skewX(${orangePos.bodySkew}deg)`;
    }

    const orangeEyes = document.getElementById("orange-eyes");
    const orangePupilL = document.getElementById("orange-pupil-l");
    const orangePupilR = document.getElementById("orange-pupil-r");
    if (orangeEyes && orangePupilL && orangePupilR) {
      if (this.isLoginError) {
        orangeEyes.style.left = "60px";
        orangeEyes.style.top = "95px";
        orangePupilL.style.transform = PUPIL_ERROR;
        orangePupilR.style.transform = PUPIL_ERROR;
      } else if (isLookingAway) {
        orangeEyes.style.left = "50px";
        orangeEyes.style.top = "75px";
        orangePupilL.style.transform = PUPIL_LOOKING_AWAY;
        orangePupilR.style.transform = PUPIL_LOOKING_AWAY;
      } else if (isShowingPwd) {
        orangeEyes.style.left = "50px";
        orangeEyes.style.top = "85px";
        orangePupilL.style.transform = "translate(-5px, -4px)";
        orangePupilR.style.transform = "translate(-5px, -4px)";
      } else {
        orangeEyes.style.left = `${82 + orangePos.faceX}px`;
        orangeEyes.style.top = `${90 + orangePos.faceY}px`;
        const oo = this.calcPupilOffset(orangePupilL, 5);
        orangePupilL.style.transform = `translate(${oo.x}px, ${oo.y}px)`;
        orangePupilR.style.transform = `translate(${oo.x}px, ${oo.y}px)`;
      }
    }
  }

  // 黄色角色：身体倾斜 + 眼睛/瞳孔/嘴
  _updateYellowCharacter({ isShowingPwd, isLookingAway }, yellowPos) {
    const yellow = document.getElementById("char-yellow");
    if (isShowingPwd) {
      yellow.style.transform = "skewX(0deg)";
    } else {
      yellow.style.transform = `skewX(${yellowPos.bodySkew}deg)`;
    }

    const yellowEyes = document.getElementById("yellow-eyes");
    const yellowPupilL = document.getElementById("yellow-pupil-l");
    const yellowPupilR = document.getElementById("yellow-pupil-r");
    const yellowMouth = document.getElementById("yellow-mouth");
    if (yellowEyes && yellowPupilL && yellowPupilR && yellowMouth) {
      if (this.isLoginError) {
        yellowEyes.style.left = "35px";
        yellowEyes.style.top = "45px";
        yellowPupilL.style.transform = PUPIL_ERROR;
        yellowPupilR.style.transform = PUPIL_ERROR;
        yellowMouth.style.left = "30px";
        yellowMouth.style.top = "92px";
        yellowMouth.style.transform = "rotate(-8deg)";
      } else if (isLookingAway) {
        yellowEyes.style.left = "20px";
        yellowEyes.style.top = "30px";
        yellowPupilL.style.transform = PUPIL_LOOKING_AWAY;
        yellowPupilR.style.transform = PUPIL_LOOKING_AWAY;
        yellowMouth.style.left = "15px";
        yellowMouth.style.top = "78px";
        yellowMouth.style.transform = "rotate(0deg)";
      } else if (isShowingPwd) {
        yellowEyes.style.left = "20px";
        yellowEyes.style.top = "35px";
        yellowPupilL.style.transform = "translate(-5px, -4px)";
        yellowPupilR.style.transform = "translate(-5px, -4px)";
        yellowMouth.style.left = "10px";
        yellowMouth.style.top = "88px";
        yellowMouth.style.transform = "rotate(0deg)";
      } else {
        yellowEyes.style.left = `${52 + yellowPos.faceX}px`;
        yellowEyes.style.top = `${40 + yellowPos.faceY}px`;
        const yo = this.calcPupilOffset(yellowPupilL, 5);
        yellowPupilL.style.transform = `translate(${yo.x}px, ${yo.y}px)`;
        yellowPupilR.style.transform = `translate(${yo.x}px, ${yo.y}px)`;
        yellowMouth.style.left = `${40 + yellowPos.faceX}px`;
        yellowMouth.style.top = `${88 + yellowPos.faceY}px`;
        yellowMouth.style.transform = "rotate(0deg)";
      }
    }
  }

  // 触发登录失败动画：角色摇头 + 难过嘴
  triggerLoginError() {
    // 清除上一次的恢复定时器，便于重复点击
    if (this.errorRecoverTimer) {
      clearTimeout(this.errorRecoverTimer);
      this.errorRecoverTimer = null;
    }

    const shakeIds = [
      "purple-eyes",
      "black-eyes",
      "orange-eyes",
      "yellow-eyes",
      "yellow-mouth",
      "orange-mouth"
    ];
    const shakeEls = shakeIds.map((id) => document.getElementById(id)).filter(Boolean);

    // 重置 shake 动画（移除 class → 强制 reflow → 重新添加）
    shakeEls.forEach((el) => el.classList.remove("shake-head"));
    // eslint-disable-next-line sonarjs/void-use -- 读取 offsetHeight 强制 reflow，以重放 shake 动画
    void document.body.offsetHeight;

    this.isLoginError = true;
    this.isPasswordFocused = false;
    this.updateCharacters();

    // 显示橙色难过嘴
    const orangeMouth = document.getElementById("orange-mouth");
    if (orangeMouth) {
      orangeMouth.classList.add("visible");
    }

    // 身体过渡(0.7s)结束后再开始摇头
    setTimeout(() => {
      shakeEls.forEach((el) => el.classList.add("shake-head"));
    }, 350);

    // 2.5 秒后自动恢复（仅恢复姿态；错误提示由 clearError 控制）
    this.errorRecoverTimer = setTimeout(() => {
      this.isLoginError = false;
      this.errorRecoverTimer = null;
      if (orangeMouth) {
        orangeMouth.classList.remove("visible");
      }
      shakeEls.forEach((el) => el.classList.remove("shake-head"));
      this.updateCharacters();
    }, 2500);
  }

  // 清除错误时立即恢复角色姿态（保留 isLoginError 由 triggerLoginError 自身的恢复定时器管理）
  recoverLoginError() {
    // 不强行重置 isLoginError，避免与 triggerLoginError 的定时器冲突；
    // 错误提示隐藏即可，角色姿态由其自身的 2.5s 定时器自然恢复。
  }
}

document.addEventListener("DOMContentLoaded", async () => {
  const loginManager = new LoginManager();
  await loginManager.init();
});
