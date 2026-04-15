import { apiPost, refreshToken } from "./utils/apiClient.js";
import { closeModal, openModal } from "./utils/modal.js";
import { loginUser } from "./modules/authManager.js";
import { t, initI18n } from "./utils/i18n.js";
import { hasSession, clearSession } from "./utils/sessionManager.js";
class LoginManager {
    State = {
        INIT: "init",
        SUBMITTING: "submitting",
        TWO_FACTOR: "two_factor",
    };
    LoginMode = {
        PASSWORD: "password-login",
        EMAIL: "email-login",
    };
    currentState = this.State.INIT;
    currentMode = this.LoginMode.PASSWORD;
    tempAuthData = null;
    dom;
    constructor() {
        this.dom = {
            form: document.getElementById("login-form"),
            twoFactorForm: document.getElementById("two-factor-form"),
            tabs: document.querySelectorAll(".tab-btn"),
            loginSections: document.querySelectorAll(".login-section"),
            views: document.querySelectorAll(".view"),
            loginView: document.getElementById("login-view"),
            twoFactorView: document.getElementById("two-factor-view"),
            usernameInput: document.getElementById("username"),
            passwordInput: document.getElementById("password"),
            emailInput: document.getElementById("email-login-input"),
            emailCodeInput: document.getElementById("email-login-code"),
            sendLoginCodeBtn: document.getElementById("send-login-code-btn"),
            rememberMeCheckbox: document.getElementById("remember-me"),
            submitBtn: document.getElementById("login-submit-btn"),
            errorMsg: document.getElementById("login-error"),
            twoFactorCodeInput: document.getElementById("two-factor-code"),
            twoFactorSubmitBtn: document.getElementById("two-factor-submit-btn"),
            backToLoginBtn: document.getElementById("back-to-login-btn"),
            forgotPasswordLink: document.getElementById("forgot-password-link"),
            forgotPasswordForm: document.getElementById("forgot-password-form"),
            forgotPasswordError: document.getElementById("forgot-password-error"),
            forgotPasswordSuccess: document.getElementById("forgot-password-success"),
        };
    }
    async init() {
        await initI18n();
        this.cleanUrlParams();
        await this.checkLoginStatus();
        this.dom.tabs.forEach(tab => {
            tab.addEventListener("click", (e) => this.handleTabClick(e.target));
        });
        this.dom.form?.addEventListener("submit", (e) => this.handleSubmit(e));
        this.dom.twoFactorForm?.addEventListener("submit", (e) => this.handleTwoFactorSubmit(e));
        this.dom.sendLoginCodeBtn?.addEventListener("click", () => this.handleSendLoginCode());
        this.dom.backToLoginBtn?.addEventListener("click", () => this.resetToInitState());
        this.dom.forgotPasswordLink?.addEventListener("click", (e) => {
            e.preventDefault();
            openModal("forgot-password-modal");
        });
        this.dom.forgotPasswordForm?.addEventListener("submit", (e) => this.handleForgotPassword(e));
        this.updateFormValidation();
    }
    updateFormValidation() {
        const { usernameInput, passwordInput, emailInput, emailCodeInput } = this.dom;
        if (!usernameInput || !passwordInput || !emailInput || !emailCodeInput)
            return;
        if (this.currentMode === this.LoginMode.PASSWORD) {
            usernameInput.required = true;
            passwordInput.required = true;
            emailInput.required = false;
            emailCodeInput.required = false;
        }
        else {
            usernameInput.required = false;
            passwordInput.required = false;
            emailInput.required = true;
            emailCodeInput.required = true;
        }
    }
    handleTabClick(targetBtn) {
        if (this.currentState === this.State.SUBMITTING)
            return;
        this.dom.tabs.forEach(btn => btn.classList.remove("active"));
        this.dom.loginSections.forEach(section => section.classList.remove("active"));
        targetBtn.classList.add("active");
        const mode = targetBtn.dataset.tab;
        this.currentMode = mode;
        const sectionId = mode === this.LoginMode.PASSWORD ? "password-login-section" : "email-login-section";
        document.getElementById(sectionId)?.classList.add("active");
        this.updateFormValidation();
        this.clearError();
    }
    cleanUrlParams() {
        if (window.location.search) {
            window.history.replaceState({}, document.title, window.location.pathname);
        }
    }
    async handleSubmit(e) {
        e.preventDefault();
        this.clearError();
        if (this.currentState === this.State.SUBMITTING)
            return;
        const rememberMe = this.dom.rememberMeCheckbox?.checked ?? false;
        if (this.currentMode === this.LoginMode.PASSWORD) {
            const username = this.dom.usernameInput?.value.trim() ?? "";
            const password = this.dom.passwordInput?.value ?? "";
            if (!username || !password) {
                this.showError(t("login.username_password_required") || "请输入用户名和密码");
                return;
            }
            await this.submitPasswordLogin(username, password, rememberMe);
        }
        else {
            const email = this.dom.emailInput?.value.trim() ?? "";
            const code = this.dom.emailCodeInput?.value.trim() ?? "";
            if (!email || !code) {
                this.showError(t("login.email_code_required") || "请输入邮箱和验证码");
                return;
            }
            await this.submitEmailLogin(email, code, rememberMe);
        }
    }
    async submitPasswordLogin(username, password, rememberMe) {
        this.setLoading(true);
        try {
            const result = await apiPost("/api/auth/login", { username, password, remember_me: rememberMe }, { skipAuthCheck: true });
            if (result.success) {
                if (result.data?.requires_two_factor) {
                    this.tempAuthData = { username, password, rememberMe };
                    this.switchToTwoFactorView();
                }
                else {
                    loginUser(result.data, rememberMe);
                }
            }
            else {
                this.showError(this.formatErrorMessage(result.message));
            }
        }
        catch (error) {
            this.handleNetworkError(error);
        }
        finally {
            this.setLoading(false);
        }
    }
    async submitEmailLogin(email, code, rememberMe) {
        this.setLoading(true);
        try {
            const result = await apiPost("/api/auth/login/email", { email, code, remember_me: rememberMe }, { skipAuthCheck: true });
            if (result.success) {
                if (result.data?.requires_two_factor) {
                    this.tempAuthData = {
                        username: result.data.username ?? "",
                        password: "",
                        rememberMe,
                    };
                    this.switchToTwoFactorView();
                }
                else {
                    loginUser(result.data, rememberMe);
                }
            }
            else {
                this.showError(this.formatErrorMessage(result.message));
            }
        }
        catch (error) {
            this.handleNetworkError(error);
        }
        finally {
            this.setLoading(false);
        }
    }
    switchToTwoFactorView() {
        this.currentState = this.State.TWO_FACTOR;
        this.clearError();
        this.dom.loginView?.classList.remove("active");
        this.dom.twoFactorView?.classList.add("active");
        if (this.dom.twoFactorCodeInput) {
            this.dom.twoFactorCodeInput.value = "";
            this.dom.twoFactorCodeInput.focus();
        }
    }
    async handleTwoFactorSubmit(e) {
        e.preventDefault();
        if (!this.tempAuthData) {
            this.resetToInitState();
            return;
        }
        const code = this.dom.twoFactorCodeInput?.value.trim() ?? "";
        if (!code) {
            this.showError(t("login.two_factor_code_required") || "请输入验证码");
            return;
        }
        this.setLoading(true);
        try {
            const result = await apiPost("/api/auth/login/two-factor", {
                username: this.tempAuthData.username,
                password: this.tempAuthData.password || "",
                two_factor_code: code,
                remember_me: this.tempAuthData.rememberMe,
            }, { skipAuthCheck: true });
            if (result.success) {
                loginUser(result.data, this.tempAuthData.rememberMe);
            }
            else {
                this.showError(result.message || t("login.two_factor_failed") || "2FA验证失败");
            }
        }
        catch (error) {
            this.handleNetworkError(error);
        }
        finally {
            this.setLoading(false);
        }
    }
    async handleSendLoginCode() {
        const email = this.dom.emailInput?.value.trim() ?? "";
        if (!email) {
            this.showError(t("login.email_required") || "请输入邮箱地址");
            return;
        }
        const btn = this.dom.sendLoginCodeBtn;
        if (!btn)
            return;
        this.setLoadingState(btn, true);
        try {
            const result = await apiPost("/api/auth/login/send-code", { email }, { skipAuthCheck: true });
            if (result.success) {
                this.startCountdown(btn, 60);
            }
            else {
                this.showError(result.message || t("login.send_code_failed") || "发送失败");
                this.setLoadingState(btn, false);
            }
        }
        catch (error) {
            this.handleNetworkError(error);
            this.setLoadingState(btn, false);
        }
    }
    startCountdown(btn, seconds) {
        let countdown = seconds;
        btn.textContent = `${t("login.code_sent") || "已发送"}(${countdown})`;
        btn.disabled = true;
        const timer = setInterval(() => {
            countdown--;
            if (countdown <= 0) {
                clearInterval(timer);
                this.setLoadingState(btn, false);
            }
            else {
                btn.textContent = `${t("login.code_sent") || "已发送"}(${countdown})`;
            }
        }, 1000);
    }
    setLoadingState(btn, isLoading) {
        if (isLoading) {
            btn.dataset.originalText = btn.textContent ?? "";
            btn.textContent = t("login.sending") || "发送中...";
            btn.disabled = true;
            this.clearError();
        }
        else {
            btn.textContent = t("login.send_code") || "发送验证码";
            btn.disabled = false;
        }
    }
    resetToInitState() {
        this.currentState = this.State.INIT;
        this.tempAuthData = null;
        this.clearError();
        this.dom.twoFactorView?.classList.remove("active");
        this.dom.loginView?.classList.add("active");
        if (this.dom.twoFactorCodeInput) {
            this.dom.twoFactorCodeInput.value = "";
        }
        if (this.dom.twoFactorSubmitBtn) {
            this.dom.twoFactorSubmitBtn.textContent = t("login.verify_and_login") || "验证并登录";
            this.dom.twoFactorSubmitBtn.disabled = false;
        }
    }
    setLoading(isLoading) {
        if (isLoading) {
            this.currentState = this.State.SUBMITTING;
            if (this.dom.twoFactorView?.classList.contains("active")) {
                if (this.dom.twoFactorSubmitBtn) {
                    this.dom.twoFactorSubmitBtn.disabled = true;
                    this.dom.twoFactorSubmitBtn.textContent = t("login.processing") || "处理中...";
                }
            }
            else {
                if (this.dom.submitBtn) {
                    this.dom.submitBtn.disabled = true;
                    this.dom.submitBtn.textContent = t("login.processing") || "处理中...";
                }
            }
        }
        else {
            this.currentState = this.dom.twoFactorView?.classList.contains("active")
                ? this.State.TWO_FACTOR
                : this.State.INIT;
            if (this.dom.twoFactorSubmitBtn) {
                this.dom.twoFactorSubmitBtn.disabled = false;
                this.dom.twoFactorSubmitBtn.textContent = t("login.verify_and_login") || "验证并登录";
            }
            if (this.dom.submitBtn) {
                this.dom.submitBtn.disabled = false;
                this.dom.submitBtn.textContent = t("login.login") || "登录";
            }
        }
    }
    formatErrorMessage(message) {
        if (!message)
            return t("api.failed") || "操作失败";
        if (message.includes(".")) {
            const translated = t(message);
            if (translated !== message)
                return translated;
        }
        if (message.includes("账户已禁用"))
            return t("api.account_disabled") || "您的账户已被禁用，请联系管理员";
        if (message.includes("失败次数过多"))
            return t("login.too_many_attempts") || "登录失败次数过多，请5分钟后再试";
        if (message.includes("系统未初始化"))
            return t("login.system_not_initialized") || "系统未初始化，请联系管理员";
        if (message.includes("SMTP未配置"))
            return t("login.smtp_not_configured") || "系统邮件服务未配置，无法发送验证码";
        return message;
    }
    showError(msg) {
        if (this.dom.errorMsg) {
            this.dom.errorMsg.textContent = msg;
            this.dom.errorMsg.classList.add("show");
        }
    }
    clearError() {
        if (this.dom.errorMsg) {
            this.dom.errorMsg.textContent = "";
            this.dom.errorMsg.classList.remove("show");
        }
    }
    handleNetworkError(error) {
        let msg = t("login.network_error") || "请求失败，请检查网络连接";
        if (error.message?.includes("Network")) {
            msg = t("login.network_connection_failed") || "网络连接失败，请检查您的网络设置";
        }
        else if (error.message?.includes("timeout")) {
            msg = t("login.request_timeout") || "请求超时，请稍后再试";
        }
        this.showError(msg);
    }
    async handleForgotPassword(e) {
        e.preventDefault();
        const { forgotPasswordError, forgotPasswordSuccess, forgotPasswordForm } = this.dom;
        if (!forgotPasswordError || !forgotPasswordSuccess || !forgotPasswordForm)
            return;
        forgotPasswordError.textContent = "";
        forgotPasswordError.classList.remove("show");
        forgotPasswordSuccess.textContent = "";
        forgotPasswordSuccess.classList.remove("show");
        const formData = new FormData(e.target);
        const email = formData.get("email");
        const btn = e.target.querySelector('button[type="submit"]');
        const originalText = btn?.textContent ?? "";
        if (btn) {
            btn.disabled = true;
            btn.textContent = t("login.sending") || "发送中...";
        }
        try {
            const result = await apiPost("/api/auth/forgot-password", { email }, { skipAuthCheck: true });
            if (result.success) {
                forgotPasswordSuccess.textContent = t("login.reset_email_sent") || "密码重置邮件已发送，请查收您的邮箱";
                forgotPasswordSuccess.classList.add("show");
                forgotPasswordForm.reset();
                setTimeout(() => {
                    closeModal("forgot-password-modal");
                    forgotPasswordSuccess.classList.remove("show");
                }, 3000);
            }
            else {
                forgotPasswordError.textContent = result.message || t("login.send_failed") || "发送失败，请稍后重试";
                forgotPasswordError.classList.add("show");
            }
        }
        catch (error) {
            const err = error;
            let msg = t("login.send_failed") || "发送失败，请稍后重试";
            if (err.message?.includes("Network"))
                msg = t("login.network_connection_failed") || "网络连接失败";
            forgotPasswordError.textContent = msg;
            forgotPasswordError.classList.add("show");
        }
        finally {
            if (btn) {
                btn.disabled = false;
                btn.textContent = originalText;
            }
        }
    }
    async checkLoginStatus() {
        if (!hasSession())
            return;
        try {
            const response = await fetch("/api/auth/me", {
                credentials: "include",
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
                clearSession();
            }
        }
        catch (_e) {
            // Ignore check login status errors
        }
    }
}
document.addEventListener("DOMContentLoaded", async () => {
    const loginManager = new LoginManager();
    await loginManager.init();
});
//# sourceMappingURL=login.js.map