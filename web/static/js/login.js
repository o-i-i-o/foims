// 登录页面专用脚本

// ES模块导入
import { apiPost, apiGet, refreshToken } from "./utils/apiClient.js";
import { closeModal, openModal } from "./utils/modal.js";
import { loginUser } from "./modules/authManager.js";
import { t, initI18n } from "./utils/i18n.js";

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
      TWO_FACTOR: "two_factor",
    };

    // 登录模式枚举
    this.LoginMode = {
      PASSWORD: "password-login",
      EMAIL: "email-login",
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
      
      // Email Login Inputs
      emailInput: document.getElementById("email-login-input"),
      emailCodeInput: document.getElementById("email-login-code"),
      sendLoginCodeBtn: document.getElementById("send-login-code-btn"),
      
      // Common Inputs
      rememberMeCheckbox: document.getElementById("remember-me"),
      submitBtn: document.getElementById("login-submit-btn"),
      errorMsg: document.getElementById("login-error"),
      
      // 2FA Inputs
      twoFactorCodeInput: document.getElementById("two-factor-code"),
      twoFactorSubmitBtn: document.getElementById("two-factor-submit-btn"),
      backToLoginBtn: document.getElementById("back-to-login-btn"),
      
      // Forgot Password
      forgotPasswordLink: document.getElementById("forgot-password-link"),
      forgotPasswordForm: document.getElementById("forgot-password-form"),
      forgotPasswordError: document.getElementById("forgot-password-error"),
      forgotPasswordSuccess: document.getElementById("forgot-password-success"),
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
    this.cleanUrlParams();
    this.checkLoginStatus();
    
    // 绑定 Tabs 事件
    this.dom.tabs.forEach(tab => {
      tab.addEventListener("click", (e) => this.handleTabClick(e.target));
    });

    // 绑定表单提交事件
    this.dom.form.addEventListener("submit", this.handleSubmit);
    this.dom.twoFactorForm.addEventListener("submit", this.handleTwoFactorSubmit);
    
    // 绑定验证码发送事件
    this.dom.sendLoginCodeBtn.addEventListener("click", this.handleSendLoginCode);
    
    // 绑定返回按钮事件
    this.dom.backToLoginBtn.addEventListener("click", this.resetToInitState);

    // 绑定忘记密码事件
    this.dom.forgotPasswordLink.addEventListener("click", (e) => {
      e.preventDefault();
      openModal("forgot-password-modal");
    });
    this.dom.forgotPasswordForm.addEventListener("submit", this.handleForgotPassword);

    // 初始化表单验证规则
    this.updateFormValidation();
  }

  /**
   * 更新表单验证规则
   * 根据当前登录模式，动态设置 input 的 required 属性
   * 避免浏览器阻止提交隐藏的 required 字段
   */
  updateFormValidation() {
    if (this.currentMode === this.LoginMode.PASSWORD) {
      // 启用密码登录验证
      this.dom.usernameInput.required = true;
      this.dom.passwordInput.required = true;
      
      // 禁用邮箱登录验证
      this.dom.emailInput.required = false;
      this.dom.emailCodeInput.required = false;
    } else {
      // 禁用密码登录验证
      this.dom.usernameInput.required = false;
      this.dom.passwordInput.required = false;
      
      // 启用邮箱登录验证
      this.dom.emailInput.required = true;
      this.dom.emailCodeInput.required = true;
    }
  }

  /**
   * 处理 Tab 切换
   */
  handleTabClick(targetBtn) {
    if (this.currentState === this.State.SUBMITTING) return;
    
    // 移除所有 active 类
    this.dom.tabs.forEach(btn => btn.classList.remove("active"));
    this.dom.loginSections.forEach(section => section.classList.remove("active"));
    
    // 激活当前 Tab
    targetBtn.classList.add("active");
    const mode = targetBtn.dataset.tab;
    this.currentMode = mode;
    
    // 显示对应的内容区域
    const sectionId = mode === this.LoginMode.PASSWORD ? "password-login-section" : "email-login-section";
    document.getElementById(sectionId).classList.add("active");
    
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

    if (this.currentState === this.State.SUBMITTING) return;

    const rememberMe = this.dom.rememberMeCheckbox.checked;

    if (this.currentMode === this.LoginMode.PASSWORD) {
      const username = this.dom.usernameInput.value.trim();
      const password = this.dom.passwordInput.value;
      if (!username || !password) {
        this.showError("请输入用户名和密码");
        return;
      }
      await this.submitPasswordLogin(username, password, rememberMe);
    } else {
      const email = this.dom.emailInput.value.trim();
      const code = this.dom.emailCodeInput.value.trim();
      if (!email || !code) {
        this.showError("请输入邮箱和验证码");
        return;
      }
      await this.submitEmailLogin(email, code, rememberMe);
    }
  }

  /**
   * 提交密码登录
   */
  async submitPasswordLogin(username, password, rememberMe) {
    this.setLoading(true);

    try {
      const result = await apiPost(
        "/api/auth/login", 
        { username, password, remember_me: rememberMe }, 
        { skipAuthCheck: true }
      );

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
           // 注意：邮箱免密登录通常意味着已经验证了邮箱，但如果开启了 2FA，逻辑上仍需第二步验证。
           // 这里我们需要知道 username 来进行后续 2FA（如果用 TOTP）。
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
      this.showError("请输入验证码");
      return;
    }
    
    this.setLoading(true);
    
    try {
      const result = await apiPost(
        "/api/auth/login/two-factor",
        { 
          username: this.tempAuthData.username, 
          password: this.tempAuthData.password || "", // 邮箱登录时为空，后端允许为空时不验证密码
          two_factor_code: code, 
          remember_me: this.tempAuthData.rememberMe 
        },
        { skipAuthCheck: true }
      );

      if (result.success) {
        loginUser(result.data, this.tempAuthData.rememberMe);
      } else {
        // 如果是因为密码为空导致的失败（虽然前端没法区分具体原因，除了 message），
        // 如果是邮箱登录过来的，可能确实无法通过。
        // 但如果后端 login_with_two_factor 必须密码，那我们这里传空字符串肯定会挂。
        this.showError(result.message || "2FA验证失败");
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
      this.showError("请输入邮箱地址");
      return;
    }
    
    this.setLoadingState(this.dom.sendLoginCodeBtn, true);
    
    try {
      const result = await apiPost(
        "/api/auth/login/send-code",
        { email },
        { skipAuthCheck: true }
      );
      
      if (result.success) {
        this.startCountdown(this.dom.sendLoginCodeBtn, 60);
      } else {
        this.showError(result.message || "发送失败");
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
    btn.textContent = `已发送(${countdown})`;
    btn.disabled = true;
    
    const timer = setInterval(() => {
      countdown--;
      if (countdown <= 0) {
        clearInterval(timer);
        this.setLoadingState(btn, false);
      } else {
        btn.textContent = `已发送(${countdown})`;
      }
    }, 1000);
  }
  
  /**
   * 设置按钮加载状态
   */
  setLoadingState(btn, isLoading) {
    if (isLoading) {
      btn.dataset.originalText = btn.textContent;
      btn.textContent = "发送中...";
      btn.disabled = true;
      this.clearError();
    } else {
      btn.textContent = "发送验证码"; // 恢复默认文本
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
    this.dom.twoFactorSubmitBtn.textContent = "验证并登录";
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
        this.dom.twoFactorSubmitBtn.textContent = "处理中...";
      } else {
        this.dom.submitBtn.disabled = true;
        this.dom.submitBtn.textContent = "处理中...";
      }
    } else {
      this.currentState = this.dom.twoFactorView.classList.contains("active") ? this.State.TWO_FACTOR : this.State.INIT;
      
      this.dom.twoFactorSubmitBtn.disabled = false;
      this.dom.twoFactorSubmitBtn.textContent = "验证并登录";
      
      this.dom.submitBtn.disabled = false;
      this.dom.submitBtn.textContent = "登录";
    }
  }

  formatErrorMessage(message) {
    if (!message) return t("api.failed") || "操作失败";

    // 尝试使用 i18n 翻译
    if (message.includes(".")) {
      const translated = t(message);
      if (translated !== message) return translated;
    }
    
    if (message.includes("账户已禁用")) return t("api.account_disabled") || "您的账户已被禁用，请联系管理员";
    if (message.includes("失败次数过多")) return "登录失败次数过多，请5分钟后再试";
    if (message.includes("系统未初始化")) return "系统未初始化，请联系管理员";
    if (message.includes("SMTP未配置")) return "系统邮件服务未配置，无法发送验证码";
    
    return message;
  }

  showError(msg) {
    this.dom.errorMsg.textContent = msg;
    this.dom.errorMsg.classList.add("show");
  }

  clearError() {
    this.dom.errorMsg.textContent = "";
    this.dom.errorMsg.classList.remove("show");
  }

  handleNetworkError(error) {
    let msg = "请求失败，请检查网络连接";
    if (error.message && error.message.includes("Network")) {
      msg = "网络连接失败，请检查您的网络设置";
    } else if (error.message && error.message.includes("timeout")) {
      msg = "请求超时，请稍后再试";
    }
    this.showError(msg);
  }

  async handleForgotPassword(e) {
    e.preventDefault();
    const { forgotPasswordError, forgotPasswordSuccess } = this.dom;
    
    forgotPasswordError.textContent = "";
    forgotPasswordError.classList.remove("show");
    forgotPasswordSuccess.textContent = "";
    forgotPasswordSuccess.classList.remove("show");

    const formData = new FormData(e.target);
    const email = formData.get("email");
    const btn = e.target.querySelector('button[type="submit"]');
    
    const originalText = btn.textContent;
    btn.disabled = true;
    btn.textContent = "发送中...";

    try {
      const result = await apiPost("/api/auth/forgot-password", { email }, { skipAuthCheck: true });

      if (result.success) {
        forgotPasswordSuccess.textContent = "密码重置邮件已发送，请查收您的邮箱";
        forgotPasswordSuccess.classList.add("show");
        e.target.reset();
        setTimeout(() => {
          closeModal("forgot-password-modal");
          forgotPasswordSuccess.classList.remove("show");
        }, 3000);
      } else {
        forgotPasswordError.textContent = result.message || "发送失败，请稍后重试";
        forgotPasswordError.classList.add("show");
      }
    } catch (error) {
      let msg = "发送失败，请稍后重试";
      if (error.message && error.message.includes("Network")) msg = "网络连接失败";
      forgotPasswordError.textContent = msg;
      forgotPasswordError.classList.add("show");
    } finally {
      btn.disabled = false;
      btn.textContent = originalText;
    }
  }

  async checkLoginStatus() {
    const accessToken = sessionStorage.getItem("access_token") || localStorage.getItem("access_token");
    if (!accessToken) return;

    try {
      const response = await fetch("/api/auth/me", {
        headers: { Authorization: `Bearer ${accessToken}` }
      });

      if (response.ok) {
        window.location.href = "/main.html";
        return;
      }

      if (response.status === 401) {
        const rememberMe = localStorage.getItem("rememberMe") === "true";
        if (rememberMe) {
          const refreshed = await refreshToken();
          if (refreshed) {
            window.location.href = "/main.html";
            return;
          }
        }
        this.clearStorage();
      }
    } catch (e) {
      console.warn("Check login status failed:", e);
    }
  }

  clearStorage() {
    localStorage.removeItem("access_token");
    localStorage.removeItem("refresh_token");
    localStorage.removeItem("user");
    localStorage.removeItem("rememberMe");
    sessionStorage.removeItem("access_token");
    sessionStorage.removeItem("refresh_token");
    sessionStorage.removeItem("user");
    sessionStorage.removeItem("rememberMe");
  }
}

document.addEventListener("DOMContentLoaded", async () => {
  const loginManager = new LoginManager();
  await loginManager.init();
});
