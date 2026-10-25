<script setup lang="ts">
/**
 * 登录页（移植自 web/static/js/login.js + loginView.js + authManager.js 登录相关逻辑）
 *
 * 功能：密码 / 邮箱验证码 / LDAP / SSO 四种登录方式、2FA 动态码、图形验证码
 * 联动、忘记密码模态框、会话探测跳转、i18n、动画角色（鼠标追踪/眨眼/偷看/
 * 登录失败摇头）。角色动画为逐帧 DOM 操作，保留命令式实现（元素缓存 +
 * 定时器），其余状态全部响应式化。
 */
import { ref, reactive, computed, onMounted, onUnmounted, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { refreshToken } from "../api/client";
import {
  SSO_LOGIN_URL,
  fetchCaptcha,
  getAuthMethods,
  getCaStatus,
  getInitStatus,
  login,
  loginWithEmail,
  loginWithLdap,
  loginWithTwoFactor,
  probeSession,
  retryLdapWithTwoFactor,
  sendLoginCode,
  type LoginBody,
  type LoginPayload,
  type LdapTwoFactorBody
} from "../api/auth";
import { CA_DOWNLOAD_URL } from "../api/system";
import { hasSession, setUser, clearSession } from "../utils/session";
import ForgotPasswordModal from "../components/ForgotPasswordModal.vue";
import AppToast from "../components/AppToast.vue";
import { i18n, changeLanguage } from "../i18n/index";
import type { ApiResult } from "../types/api";

const { t } = useI18n();

// ==================== 登录模式与视图状态 ====================

const LOGIN_MODES = {
  PASSWORD: "password-login",
  EMAIL: "email-login",
  LDAP: "ldap-login",
  SSO: "sso-login"
};

const mode = ref(LOGIN_MODES.PASSWORD);
const activeView = ref("login"); // login | twoFactor
const submitting = ref(false);

// 按后端 /api/auth/methods 开关显示邮箱 / LDAP / SSO 标签（请求失败保持默认，fail-safe）
const emailTabVisible = ref(true);
const ldapTabVisible = ref(false);
const ssoTabVisible = ref(false);

// 站点根证书下载入口（有 CA 时才显示）
const caAvailable = ref(false);

// 表单字段
const username = ref("");
const password = ref("");
const email = ref("");
const emailCode = ref("");
const ldapUsername = ref("");
const ldapPassword = ref("");
const twoFactorCode = ref("");
const rememberMe = ref(false);
const showPassword = ref(false);

// 错误条
const errorMessage = ref("");
const errorVisible = ref(false);

// 忘记密码模态框
const showForgotModal = ref(false);

// 语言选择器
const currentLanguage = ref(i18n.global.locale.value);

// ==================== 图形验证码（连续失败触发） ====================

type CaptchaForm = "password" | "ldap";

const captchaState = reactive<Record<CaptchaForm, { id: string | null; failures: number; visible: boolean; svg: string }>>({
  password: { id: null, failures: 0, visible: false, svg: "" },
  ldap: { id: null, failures: 0, visible: false, svg: "" }
});
const captchaInput = reactive<Record<CaptchaForm, string>>({ password: "", ldap: "" });

// ==================== 2FA 上下文 ====================
// kind: local（密码/邮箱，POST /api/auth/login/two-factor，字段 two_factor_code）
//       ldap（外部登录，重试原端点，字段 code + 可选验证码快照）
type TempAuthData =
  | { kind: "local"; username: string; password: string; remember: boolean }
  | {
      kind: "ldap";
      username: string;
      password: string;
      remember: boolean;
      captcha: { captchaId: string | null; captchaText: string } | null;
    };

let tempAuthData: TempAuthData | null = null;

// 发送验证码倒计时
const sendingCode = ref(false);
const sendCodeCountdown = ref(0);
let countdownTimer: number | undefined;

// 按钮文案
const loginSubmitLabel = computed(() =>
  submitting.value && activeView.value === "login" ? t("common.processing") : t("login.sign_in")
);
const twoFactorSubmitLabel = computed(() =>
  submitting.value && activeView.value === "twoFactor"
    ? t("common.processing")
    : t("login.verify_and_sign_in")
);
const sendCodeLabel = computed(() => {
  if (sendingCode.value) {
    return t("login.sending");
  }
  if (sendCodeCountdown.value > 0) {
    return `${t("login.code_sent")}(${sendCodeCountdown.value})`;
  }
  return t("login.send_code");
});

// ==================== 错误条与 2FA 视图切换 ====================

function showError(message: string): void {
  errorMessage.value = message;
  errorVisible.value = true;
  triggerLoginError();
}

function clearError(): void {
  errorVisible.value = false;
  // 姿态不立即复位，交由 triggerLoginError 的恢复定时器自然还原
}

const twoFactorCodeInput = ref<HTMLInputElement | null>(null);

function switchToTwoFactorView() {
  activeView.value = "twoFactor";
  twoFactorCode.value = "";
  clearError();
  nextTick(() => {
    twoFactorCodeInput.value?.focus();
  });
}

// ==================== 会话探测与初始化 ====================

/** 已登录则跳主应用；401 时尝试刷新，成功同样跳转 */
async function checkLoginStatus() {
  if (!hasSession()) {
    return;
  }
  try {
    const response = await probeSession();
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
  } catch {
    // 登录态探测失败不影响登录页可用性，静默回退到未登录展示
  }
}

/** 初始化模式（config.toml [init].enabled）时跳转初始化向导 */
async function checkInitMode() {
  try {
    const result = await getInitStatus();
    if (result.success && result.data?.init_enabled) {
      window.location.href = "/init_index.html";
      return true;
    }
  } catch {
    // 接口不可用时按非初始化模式处理（保持登录页可用）
  }
  return false;
}

async function setupAuthMethods() {
  try {
    const result = await getAuthMethods();
    if (!result.success || !result.data) {
      return;
    }
    emailTabVisible.value = result.data.email !== false;
    ldapTabVisible.value = Boolean(result.data.ldap);
    ssoTabVisible.value = Boolean(result.data.sso);
  } catch (error) {
    console.error("获取认证方式失败:", error);
  }
}

async function setupCaDownload() {
  try {
    const result = await getCaStatus();
    if (result.success && result.data?.available) {
      caAvailable.value = true;
    }
  } catch (error) {
    // 查询失败保持隐藏即可，不影响登录流程
    console.error("获取根证书状态失败:", error);
  }
}

function cleanUrlParams() {
  if (window.location.search) {
    window.history.replaceState({}, document.title, window.location.pathname);
  }
}

// ==================== 登录提交 ====================

function handleTabClick(targetMode: string): void {
  if (submitting.value) {
    return;
  }
  mode.value = targetMode;
  clearError();
}

function requireCaptchaIfNeeded(form: CaptchaForm): boolean {
  if (captchaState[form].visible && !captchaInput[form].trim()) {
    showError(t("login.captcha_required"));
    return false;
  }
  return true;
}

async function handleSubmit() {
  clearError();
  if (submitting.value) {
    return;
  }

  const remember = rememberMe.value;

  if (mode.value === LOGIN_MODES.PASSWORD) {
    const user = username.value.trim();
    const pwd = password.value;
    if (!user || !pwd) {
      showError(t("login.username_password_required"));
      return;
    }
    await submitPasswordLogin(user, pwd, remember);
  } else if (mode.value === LOGIN_MODES.LDAP) {
    const user = ldapUsername.value.trim();
    const pwd = ldapPassword.value;
    if (!user || !pwd) {
      showError(t("login.username_password_required"));
      return;
    }
    if (!requireCaptchaIfNeeded("ldap")) {
      return;
    }
    await submitLdapLogin(user, pwd, remember);
  } else if (mode.value === LOGIN_MODES.SSO) {
    // SSO：跳转后端发起 OIDC 授权码流程
    window.location.href = SSO_LOGIN_URL;
  } else {
    const mail = email.value.trim();
    const code = emailCode.value.trim();
    if (!mail || !code) {
      showError(t("login.email_code_required"));
      return;
    }
    await submitEmailLogin(mail, code, remember);
  }
}

async function submitPasswordLogin(usernameValue: string, passwordValue: string, remember: boolean) {
  submitting.value = true;
  try {
    const body: LoginBody = { username: usernameValue, password: passwordValue, remember_me: remember };
    if (captchaState.password.visible) {
      body.captcha_id = captchaState.password.id;
      body.captcha_text = captchaInput.password.trim();
    }
    const result = await login(body);

    if (result.success) {
      if (result.data?.requires_two_factor) {
        tempAuthData = { kind: "local", username: usernameValue, password: passwordValue, remember };
        switchToTwoFactorView();
      } else {
        // 后端契约：success 必含 user 载荷，data 不会为 null
        finishLogin(result.data as LoginPayload, remember);
      }
    } else {
      showError(formatErrorMessage(result.message));
      handleCaptchaOnFailure("password", result.message_key);
    }
  } catch (error) {
    handleNetworkError(error);
  } finally {
    submitting.value = false;
  }
}

async function submitEmailLogin(mail: string, code: string, remember: boolean) {
  submitting.value = true;
  try {
    const result = await loginWithEmail(
      { email: mail, code, remember_me: remember }
    );

    if (result.success) {
      if (result.data?.requires_two_factor) {
        // 后端 login_with_email_code 返回了 username（success 必含 user 载荷，
        // 且 2FA 分支的响应载荷必带 username 字符串）
        tempAuthData = {
          kind: "local",
          username: (result.data as LoginPayload & { username: string }).username,
          password: "",
          remember
        };
        switchToTwoFactorView();
      } else {
        // 后端契约：success 必含 user 载荷，data 不会为 null
        finishLogin(result.data as LoginPayload, remember);
      }
    } else {
      showError(formatErrorMessage(result.message));
    }
  } catch (error) {
    handleNetworkError(error);
  } finally {
    submitting.value = false;
  }
}

async function submitLdapLogin(usernameValue: string, passwordValue: string, remember: boolean) {
  submitting.value = true;
  try {
    const body: LoginBody = { username: usernameValue, password: passwordValue, remember_me: remember };
    if (captchaState.ldap.visible) {
      body.captcha_id = captchaState.ldap.id;
      body.captcha_text = captchaInput.ldap.trim();
    }
    const result = await loginWithLdap(body);

    if (result.success) {
      if (result.data?.requires_two_factor) {
        // 外部账户 2FA 契约：重试原端点并携带 code 字段；验证码快照
        // 一并采集（后端失败计数达阈值后强制校验，2FA 视图无验证码
        // 输入框，缺快照会被锁死在动态码步骤）
        tempAuthData = {
          kind: "ldap",
          username: usernameValue,
          password: passwordValue,
          remember,
          captcha: captchaState.ldap.visible
            ? { captchaId: captchaState.ldap.id, captchaText: captchaInput.ldap.trim() }
            : null
        };
        switchToTwoFactorView();
      } else {
        // 后端契约：success 必含 user 载荷，data 不会为 null
        finishLogin(result.data as LoginPayload, remember);
      }
    } else {
      showError(formatErrorMessage(result.message));
      handleCaptchaOnFailure("ldap", result.message_key);
    }
  } catch (error) {
    handleNetworkError(error);
  } finally {
    submitting.value = false;
  }
}

async function handleTwoFactorSubmit() {
  if (!tempAuthData) {
    resetToInitState();
    return;
  }

  const code = twoFactorCode.value.trim();
  if (!code) {
    showError(t("login.code_required"));
    return;
  }

  submitting.value = true;

  // 失败时保留 tempAuthData 供直接重试；仅登录成功或用户主动返回登录
  // 视图时才清空，避免输错一次动态码就弹回重输全部凭证
  try {
    let result: ApiResult<LoginPayload>;
    if (tempAuthData.kind === "ldap") {
      const body: LdapTwoFactorBody = {
        username: tempAuthData.username,
        password: tempAuthData.password,
        remember_me: tempAuthData.remember,
        code
      };
      if (tempAuthData.captcha) {
        body.captcha_id = tempAuthData.captcha.captchaId;
        body.captcha_text = tempAuthData.captcha.captchaText;
      }
      result = await retryLdapWithTwoFactor(body);
    } else {
      result = await loginWithTwoFactor({
        username: tempAuthData.username,
        password: tempAuthData.password || "",
        two_factor_code: code,
        remember_me: tempAuthData.remember
      });
    }

    if (result.success) {
      const remember = tempAuthData.remember;
      tempAuthData = null;
      // 后端契约：success 必含 user 载荷，data 不会为 null
      finishLogin(result.data as LoginPayload, remember);
    } else {
      showError(result.message || t("login.two_factor_failed"));
      if (tempAuthData.kind === "ldap") {
        // 动态码错误保留待办状态，用户可直接重试
        twoFactorCode.value = "";
        twoFactorCodeInput.value?.focus();
      }
    }
  } catch (error) {
    handleNetworkError(error);
  } finally {
    submitting.value = false;
  }
}

/** 建立会话并跳转主应用 */
function finishLogin(data: LoginPayload, remember: boolean): void {
  setUser(data.user, remember);
  window.location.href = "/main.html";
}

function resetToInitState(): void {
  tempAuthData = null;
  activeView.value = "login";
  twoFactorCode.value = "";
  clearError();
}

function handleNetworkError(error: unknown): void {
  let message = t("login.network_error");
  // 抛出值不一定是 Error 实例（与 client 层同款防御），非 Error 时按默认文案
  const raw = error instanceof Error ? error.message : "";
  if (raw.includes("Network")) {
    message = t("login.network_failed");
  } else if (raw.includes("timeout")) {
    message = t("login.timeout");
  }
  showError(message);
}

function formatErrorMessage(message: string): string {
  if (!message) {
    return t("api.failed");
  }

  // 服务端 message 为 i18n key：尝试翻译（t() 缺键返回 key 本身）
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

// ==================== 图形验证码 ====================

/** 拉取新验证码图片（点击图片可刷新） */
async function refreshCaptcha(form: CaptchaForm) {
  try {
    const result = await fetchCaptcha();
    if (result.success && result.data) {
      // 先经 DOMParser 校验确为 svg 再注入，非 SVG 内容不进入 DOM
      const doc = new DOMParser().parseFromString(result.data.svg || "", "image/svg+xml");
      if (doc.documentElement && doc.documentElement.localName === "svg") {
        captchaState[form].id = result.data.captcha_id;
        captchaState[form].svg = result.data.svg;
      } else {
        showError(t("common.load_failed"));
      }
    }
  } catch (error) {
    console.error("加载验证码失败:", error);
  }
}

/** 显示验证码并加载新图 */
function showCaptcha(form: CaptchaForm): void {
  captchaState[form].visible = true;
  refreshCaptcha(form);
}

/** 登录失败后的验证码联动：累计失败次数并按需显示验证码 */
function handleCaptchaOnFailure(form: CaptchaForm, messageKey: string | undefined): void {
  const state = captchaState[form];
  state.failures += 1;
  captchaInput[form] = "";

  const required =
    messageKey === "server.auth.captcha_required" || messageKey === "server.auth.captcha_invalid";
  if (required || state.failures >= 3) {
    showCaptcha(form);
  }
}

// ==================== 发送邮箱登录验证码 ====================

async function handleSendLoginCode() {
  const mail = email.value.trim();
  if (!mail) {
    showError(t("login.email_required_error"));
    return;
  }

  sendingCode.value = true;
  clearError();

  try {
    const result = await sendLoginCode(mail);

    if (result.success) {
      startCountdown(60);
    } else {
      showError(result.message || t("login.send_failed"));
    }
  } catch (error) {
    handleNetworkError(error);
  } finally {
    sendingCode.value = false;
  }
}

function startCountdown(seconds: number): void {
  // 新倒计时开始前先清掉旧 interval，避免连续触发时多个计时器叠加
  if (countdownTimer) {
    clearInterval(countdownTimer);
  }
  sendCodeCountdown.value = seconds;
  countdownTimer = setInterval(() => {
    sendCodeCountdown.value -= 1;
    if (sendCodeCountdown.value <= 0) {
      clearInterval(countdownTimer);
      countdownTimer = undefined;
      sendCodeCountdown.value = 0;
    }
  }, 1000);
}

// ============================================================
// 动画角色（移植自 example-files/animatedlogin.html，命令式实现）
// ============================================================

// 角色状态类全集（挂在 .character 根上，静态几何由 login.css 状态类供给）
const CHAR_STATES = ["error", "look-away", "show-pwd", "face-each-other", "typing"];

const sceneRoot = ref<HTMLElement | null>(null);
// 元素缓存：onMounted 时一次性建立，避免逐帧 getElementById
const els: Record<string, HTMLElement | null> = {};

// 动画状态（模板不直接读取，无需响应式）
let mouseX = 0;
let mouseY = 0;
let isTyping = false;
let isLookingAtEachOther = false;
let isPasswordFocused = false;
let isLoginError = false;
let isPurpleBlinking = false;
let isBlackBlinking = false;
let isPurplePeeking = false;
let typingTimer: number | undefined;
let errorRecoverTimer: number | undefined;
let mousemoveBound = false;

// 角色眨眼/偷看动画的随机调度延迟（纯动画抖动，非安全场景）
const randomDelay = (spread: number, base: number): number => Math.random() * spread + base;

const CHARACTER_IDS = [
  "char-purple",
  "char-black",
  "char-orange",
  "char-yellow",
  "purple-eyes",
  "purple-eye-l",
  "purple-pupil-l",
  "purple-pupil-r",
  "black-eyes",
  "black-eye-l",
  "black-pupil-l",
  "black-pupil-r",
  "orange-eyes",
  "orange-pupil-l",
  "orange-pupil-r",
  "orange-mouth",
  "yellow-eyes",
  "yellow-pupil-l",
  "yellow-pupil-r",
  "yellow-mouth"
];

function cacheElements() {
  const root = sceneRoot.value;
  if (!root) {
    return;
  }
  for (const id of CHARACTER_IDS) {
    els[id] = root.querySelector<HTMLElement>(`#${id}`);
  }
}

/**
 * 角色状态判定（优先级与原实现一致）：
 * error > look-away > show-pwd > face-each-other > typing > 自由态("")
 */
function resolveCharState(isShowingPwd: boolean, isLookingAway: boolean): string {
  if (isLoginError) {
    return "error";
  }
  if (isLookingAway) {
    return "look-away";
  }
  if (isShowingPwd) {
    return "show-pwd";
  }
  if (isLookingAtEachOther) {
    return "face-each-other";
  }
  if (isTyping) {
    return "typing";
  }
  return "";
}

// 清除元素上的行内样式属性：进入 CSS 状态类管辖的静态分支前调用，
// 防止此前鼠标跟随帧写下的内联值压过类规则
function clearInlineStyles(el: HTMLElement | null | undefined, ...props: string[]): void {
  if (!el) {
    return;
  }
  for (const prop of props) {
    // CSSStyleDeclaration 无字符串索引签名，按行内样式键名断言写入
    (el.style as unknown as Record<string, string>)[prop] = "";
  }
}

function applyStateClasses(el: HTMLElement | null | undefined, state: string, extra: Record<string, boolean> = {}): void {
  if (!el) {
    return;
  }
  for (const cls of CHAR_STATES) {
    el.classList.toggle(cls, cls === state);
  }
  for (const [cls, on] of Object.entries(extra)) {
    el.classList.toggle(cls, on);
  }
}

/** 姿态计算结果：视线偏移与身体倾斜 */
interface CharPos {
  faceX: number;
  faceY: number;
  bodySkew: number;
}

function calcPosition(el: HTMLElement): CharPos {
  const rect = el.getBoundingClientRect();
  const cx = rect.left + rect.width / 2;
  const cy = rect.top + rect.height / 3;
  const dx = mouseX - cx;
  const dy = mouseY - cy;
  const faceX = Math.max(-15, Math.min(15, dx / 20));
  const faceY = Math.max(-10, Math.min(10, dy / 30));
  const bodySkew = Math.max(-6, Math.min(6, -dx / 120));
  return { faceX, faceY, bodySkew };
}

function calcPupilOffset(el: HTMLElement, maxDist: number): { x: number; y: number } {
  const rect = el.getBoundingClientRect();
  const cx = rect.left + rect.width / 2;
  const cy = rect.top + rect.height / 2;
  const dx = mouseX - cx;
  const dy = mouseY - cy;
  const dist = Math.min(Math.sqrt(dx * dx + dy * dy), maxDist);
  const angle = Math.atan2(dy, dx);
  return { x: Math.cos(angle) * dist, y: Math.sin(angle) * dist };
}

function setFollowEyes(
  prefix: string,
  pos: CharPos,
  eyeOffsetX: number,
  eyeOffsetY: number,
  pupilMaxDist: number,
  pupilRefEl?: HTMLElement | null
): void {
  const eyes = els[`${prefix}-eyes`];
  const pupilL = els[`${prefix}-pupil-l`];
  const pupilR = els[`${prefix}-pupil-r`];
  if (!eyes || !pupilL || !pupilR) {
    return;
  }
  eyes.style.left = `${eyeOffsetX + pos.faceX}px`;
  eyes.style.top = `${eyeOffsetY + pos.faceY}px`;
  const po = calcPupilOffset(pupilRefEl || pupilL, pupilMaxDist);
  pupilL.style.transform = `translate(${po.x}px, ${po.y}px)`;
  pupilR.style.transform = `translate(${po.x}px, ${po.y}px)`;
}

function clearFollowEyes(prefix: string): void {
  clearInlineStyles(els[`${prefix}-eyes`], "left", "top");
  clearInlineStyles(els[`${prefix}-pupil-l`], "transform");
  clearInlineStyles(els[`${prefix}-pupil-r`], "transform");
}

// 紫色角色：身体倾斜 + 眼睛/瞳孔（含眨眼与密码可见时的偷看）
function updatePurpleCharacter(isShowingPwd: boolean, isLookingAway: boolean, pos: CharPos): void {
  const purple = els["char-purple"];
  if (!purple) {
    return;
  }
  const state = resolveCharState(isShowingPwd, isLookingAway);
  applyStateClasses(purple, state, {
    peek: isShowingPwd && isPurplePeeking,
    blink: isPurpleBlinking
  });

  if (isShowingPwd || isLookingAway) {
    clearInlineStyles(purple, "transform");
  } else if (isTyping) {
    purple.style.transform = `skewX(${(pos.bodySkew || 0) - 12}deg) translateX(40px)`;
  } else {
    purple.style.transform = `skewX(${pos.bodySkew}deg)`;
  }
  // 高度全态由 CSS 供给，清掉可能残留的内联值
  clearInlineStyles(purple, "height");

  if (state === "" || state === "typing") {
    setFollowEyes("purple", pos, 45, 40, 5, els["purple-eye-l"]);
  } else {
    clearFollowEyes("purple");
  }
}

// 黑色角色：身体倾斜 + 眼睛/瞳孔（含眨眼）
function updateBlackCharacter(isShowingPwd: boolean, isLookingAway: boolean, pos: CharPos): void {
  const black = els["char-black"];
  if (!black) {
    return;
  }
  const state = resolveCharState(isShowingPwd, isLookingAway);
  applyStateClasses(black, state, { blink: isBlackBlinking });

  if (state === "look-away" || state === "show-pwd") {
    clearInlineStyles(black, "transform");
  } else if (state === "face-each-other") {
    black.style.transform = `skewX(${(pos.bodySkew || 0) * 1.5 + 10}deg) translateX(20px)`;
  } else if (state === "typing") {
    black.style.transform = `skewX(${(pos.bodySkew || 0) * 1.5}deg)`;
  } else {
    black.style.transform = `skewX(${pos.bodySkew}deg)`;
  }

  if (state === "" || state === "typing") {
    setFollowEyes("black", pos, 26, 32, 4, els["black-eye-l"]);
  } else {
    clearFollowEyes("black");
  }
}

// 橙色角色：身体倾斜 + 眼睛/瞳孔 + 失败时的难过嘴
function updateOrangeCharacter(isShowingPwd: boolean, isLookingAway: boolean, pos: CharPos): void {
  const orange = els["char-orange"];
  if (!orange) {
    return;
  }
  const state = resolveCharState(isShowingPwd, isLookingAway);
  applyStateClasses(orange, state);

  if (state === "show-pwd") {
    clearInlineStyles(orange, "transform");
  } else {
    orange.style.transform = `skewX(${pos.bodySkew}deg)`;
  }

  if (state === "" || state === "typing" || state === "face-each-other") {
    setFollowEyes("orange", pos, 82, 90, 5);
  } else {
    clearFollowEyes("orange");
  }

  const orangeMouth = els["orange-mouth"];
  if (orangeMouth) {
    if (state === "error") {
      orangeMouth.style.left = `${80 + pos.faceX}px`;
    } else {
      clearInlineStyles(orangeMouth, "left");
    }
  }
}

// 黄色角色：身体倾斜 + 眼睛/瞳孔/嘴
function updateYellowCharacter(isShowingPwd: boolean, isLookingAway: boolean, pos: CharPos): void {
  const yellow = els["char-yellow"];
  if (!yellow) {
    return;
  }
  const state = resolveCharState(isShowingPwd, isLookingAway);
  applyStateClasses(yellow, state);

  if (state === "show-pwd") {
    clearInlineStyles(yellow, "transform");
  } else {
    yellow.style.transform = `skewX(${pos.bodySkew}deg)`;
  }

  if (state === "" || state === "typing" || state === "face-each-other") {
    setFollowEyes("yellow", pos, 52, 40, 5);
    const yellowMouth = els["yellow-mouth"];
    if (yellowMouth) {
      yellowMouth.style.left = `${40 + pos.faceX}px`;
      yellowMouth.style.top = `${88 + pos.faceY}px`;
    }
  } else {
    clearFollowEyes("yellow");
    clearInlineStyles(els["yellow-mouth"], "left", "top");
  }
}

// 更新所有角色姿态
function updateCharacters(): void {
  const purple = els["char-purple"];
  const black = els["char-black"];
  const orange = els["char-orange"];
  const yellow = els["char-yellow"];
  if (!purple || !black || !orange || !yellow) {
    return;
  }

  const pwdLen = password.value.length;
  const isShowingPwd = pwdLen > 0 && showPassword.value;
  // 密码框聚焦且密码不可见时，角色看向别处
  const isLookingAway = isPasswordFocused && !showPassword.value;

  const purplePos = calcPosition(purple);
  const blackPos = calcPosition(black);
  const orangePos = calcPosition(orange);
  const yellowPos = calcPosition(yellow);

  updatePurpleCharacter(isShowingPwd, isLookingAway, purplePos);
  updateBlackCharacter(isShowingPwd, isLookingAway, blackPos);
  updateOrangeCharacter(isShowingPwd, isLookingAway, orangePos);
  updateYellowCharacter(isShowingPwd, isLookingAway, yellowPos);
}

function setTyping(typing: boolean): void {
  isTyping = typing;
  if (typing) {
    isLookingAtEachOther = true;
    clearTimeout(typingTimer);
    typingTimer = setTimeout(() => {
      isLookingAtEachOther = false;
      updateCharacters();
    }, 800);
  } else {
    isLookingAtEachOther = false;
  }
  updateCharacters();
}

function scheduleBlink(which: "purple" | "black"): void {
  setTimeout(() => {
    if (which === "purple") {
      isPurpleBlinking = true;
    } else {
      isBlackBlinking = true;
    }
    updateCharacters();
    setTimeout(() => {
      if (which === "purple") {
        isPurpleBlinking = false;
      } else {
        isBlackBlinking = false;
      }
      updateCharacters();
      scheduleBlink(which);
    }, 150);
  }, randomDelay(4000, 3000));
}

// 密码可见时，紫色角色偶尔偷看
function schedulePeek(): void {
  if (password.value.length > 0 && showPassword.value) {
    setTimeout(() => {
      if (password.value.length > 0 && showPassword.value) {
        isPurplePeeking = true;
        updateCharacters();
        setTimeout(() => {
          isPurplePeeking = false;
          updateCharacters();
          schedulePeek();
        }, 800);
      }
    }, randomDelay(3000, 2000));
  }
}

function togglePasswordVisible(): void {
  showPassword.value = !showPassword.value;
  updateCharacters();
  // 密码可见时，可能触发紫色角色偷看
  if (showPassword.value) {
    schedulePeek();
  }
}

// 触发登录失败动画：角色摇头 + 难过嘴
function triggerLoginError(): void {
  if (errorRecoverTimer) {
    clearTimeout(errorRecoverTimer);
    errorRecoverTimer = undefined;
  }

  const shakeIds = [
    "purple-eyes",
    "black-eyes",
    "orange-eyes",
    "yellow-eyes",
    "yellow-mouth",
    "orange-mouth"
  ];
  const shakeEls = shakeIds.map((id) => els[id]).filter((el): el is HTMLElement => el !== null);

  // 重置 shake 动画（移除 class → 强制 reflow → 重新添加）
  shakeEls.forEach((el) => {
    el.classList.remove("shake-head");
  });
  // 读取 offsetHeight 强制 reflow，以重放 shake 动画
  void document.body.offsetHeight;

  isLoginError = true;
  isPasswordFocused = false;
  updateCharacters();

  // 显示橙色难过嘴
  els["orange-mouth"]?.classList.add("visible");

  // 身体过渡(0.7s)进行到一半（350ms）时再开始摇头
  setTimeout(() => {
    shakeEls.forEach((el) => {
      el.classList.add("shake-head");
    });
  }, 350);

  // 2.5 秒后自动恢复（仅恢复姿态；错误提示由 clearError 控制）
  errorRecoverTimer = setTimeout(() => {
    isLoginError = false;
    errorRecoverTimer = undefined;
    els["orange-mouth"]?.classList.remove("visible");
    shakeEls.forEach((el) => {
      el.classList.remove("shake-head");
    });
    updateCharacters();
  }, 2500);
}

// ==================== 生命周期 ====================

function onMouseMove(e: MouseEvent): void {
  mouseX = e.clientX;
  mouseY = e.clientY;
  if (!isTyping && !isLoginError) {
    updateCharacters();
  }
}

onMounted(async () => {
  cacheElements();

  if (await checkInitMode()) {
    return;
  }

  // SSO 回跳错误提示（须在 cleanUrlParams 清理地址栏前捕获）
  const ssoErrorKey = new URLSearchParams(window.location.search).get("sso_error");
  cleanUrlParams();

  await checkLoginStatus();
  await setupAuthMethods();
  setupCaDownload();

  if (ssoErrorKey) {
    showError(t(ssoErrorKey));
  }

  document.addEventListener("mousemove", onMouseMove);
  mousemoveBound = true;

  scheduleBlink("purple");
  scheduleBlink("black");
  updateCharacters();
});

onUnmounted(() => {
  if (mousemoveBound) {
    document.removeEventListener("mousemove", onMouseMove);
  }
  clearTimeout(typingTimer);
  clearTimeout(errorRecoverTimer);
  if (countdownTimer) {
    clearInterval(countdownTimer);
  }
});
</script>

<template>
  <div>
    <AppToast />

    <div id="login-page">
      <!-- ============ 左面板（动画角色） ============ -->
      <div class="left-panel">
        <div class="characters-wrapper">
          <div class="characters-scene" ref="sceneRoot">
            <!-- 紫色角色 -->
            <div class="character char-purple" id="char-purple">
              <div class="eyes" id="purple-eyes">
                <div class="eyeball" id="purple-eye-l">
                  <div class="pupil" id="purple-pupil-l"></div>
                </div>
                <div class="eyeball" id="purple-eye-r">
                  <div class="pupil" id="purple-pupil-r"></div>
                </div>
              </div>
            </div>
            <!-- 黑色角色 -->
            <div class="character char-black" id="char-black">
              <div class="eyes" id="black-eyes">
                <div class="eyeball" id="black-eye-l">
                  <div class="pupil" id="black-pupil-l"></div>
                </div>
                <div class="eyeball" id="black-eye-r">
                  <div class="pupil" id="black-pupil-r"></div>
                </div>
              </div>
            </div>
            <!-- 橙色角色 -->
            <div class="character char-orange" id="char-orange">
              <div class="eyes" id="orange-eyes">
                <div class="bare-pupil" id="orange-pupil-l"></div>
                <div class="bare-pupil" id="orange-pupil-r"></div>
              </div>
              <div class="orange-mouth" id="orange-mouth"></div>
            </div>
            <!-- 黄色角色 -->
            <div class="character char-yellow" id="char-yellow">
              <div class="eyes" id="yellow-eyes">
                <div class="bare-pupil" id="yellow-pupil-l"></div>
                <div class="bare-pupil" id="yellow-pupil-r"></div>
              </div>
              <div class="yellow-mouth" id="yellow-mouth"></div>
            </div>
          </div>
        </div>
      </div>

      <!-- ============ 右面板（表单） ============ -->
      <div class="right-panel">
        <div class="form-container">
          <div class="login-lang-switch">
            <select
              id="language-selector"
              v-model="currentLanguage"
              :aria-label="t('language.select')"
              @change="changeLanguage(currentLanguage)"
            >
              <option value="zh">中文</option>
              <option value="en">English</option>
            </select>
          </div>
          <div class="brand-name">FOIMS</div>
          <div class="form-header">
            <h1>{{ t("login.welcome") }}</h1>
            <p>{{ t("login.subtitle") }}</p>
          </div>

          <!-- 登录模式 Tabs：密码 / 邮箱 / LDAP / SSO（LDAP 与 SSO 默认隐藏，
               由 /api/auth/methods 按配置开关显示） -->
          <div class="login-tabs" role="tablist">
            <button
              type="button"
              class="login-tab-btn"
              :class="{ active: mode === 'password-login' }"
              role="tab"
              :aria-selected="mode === 'password-login'"
              @click="handleTabClick('password-login')"
            >
              {{ t("login.password_auth") }}
            </button>
            <button
              v-if="emailTabVisible"
              type="button"
              class="login-tab-btn"
              :class="{ active: mode === 'email-login' }"
              role="tab"
              :aria-selected="mode === 'email-login'"
              @click="handleTabClick('email-login')"
            >
              {{ t("login.email_auth") }}
            </button>
            <button
              v-if="ldapTabVisible"
              type="button"
              class="login-tab-btn"
              :class="{ active: mode === 'ldap-login' }"
              role="tab"
              :aria-selected="mode === 'ldap-login'"
              @click="handleTabClick('ldap-login')"
            >
              {{ t("login.ldap_auth") }}
            </button>
            <button
              v-if="ssoTabVisible"
              type="button"
              class="login-tab-btn"
              :class="{ active: mode === 'sso-login' }"
              role="tab"
              :aria-selected="mode === 'sso-login'"
              @click="handleTabClick('sso-login')"
            >
              {{ t("login.sso_auth") }}
            </button>
          </div>

          <!-- 视图容器 -->
          <div class="login-views">
            <!-- 登录视图（密码 / 邮箱 / LDAP / SSO） -->
            <div class="view" :class="{ active: activeView === 'login' }">
              <form novalidate @submit.prevent="handleSubmit">
                <h2 class="sr-only">{{ t("login.form_title") }}</h2>

                <!-- 密码登录区域 -->
                <div class="login-section" :class="{ active: mode === 'password-login' }">
                  <div class="login-form-group">
                    <label for="username">{{ t("login.username_or_email") }}</label>
                    <div class="input-wrapper">
                      <input
                        id="username"
                        v-model="username"
                        type="text"
                        name="username"
                        required
                        aria-required="true"
                        :placeholder="t('login.username_placeholder')"
                        autocomplete="username"
                        :aria-label="t('login.username_or_email')"
                        @focus="setTyping(true)"
                        @blur="setTyping(false)"
                        @input="updateCharacters()"
                      >
                    </div>
                  </div>

                  <div class="login-form-group">
                    <label for="password">{{ t("auth.password") }}</label>
                    <div class="input-wrapper">
                      <input
                        id="password"
                        v-model="password"
                        :type="showPassword ? 'text' : 'password'"
                        name="password"
                        required
                        aria-required="true"
                        :placeholder="t('login.password_placeholder')"
                        autocomplete="current-password"
                        :aria-label="t('auth.password')"
                        @focus="isPasswordFocused = true; updateCharacters()"
                        @blur="isPasswordFocused = false; updateCharacters()"
                        @input="updateCharacters()"
                      >
                      <button
                        type="button"
                        class="toggle-password"
                        :aria-label="t('login.toggle_password')"
                        @click="togglePasswordVisible"
                      >
                        <svg :hidden="showPassword" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"></path>
                          <circle cx="12" cy="12" r="3"></circle>
                        </svg>
                        <!-- 与 eye 图标互斥显示 -->
                        <svg :hidden="!showPassword" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                          <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"></path>
                          <line x1="1" y1="1" x2="23" y2="23"></line>
                        </svg>
                      </button>
                    </div>
                  </div>

                  <!-- 图形验证码（连续登录失败达到阈值后显示） -->
                  <div v-show="captchaState.password.visible" class="login-form-group">
                    <label for="captcha-input">{{ t("login.captcha") }}</label>
                    <div class="captcha-row">
                      <input
                        id="captcha-input"
                        v-model="captchaInput.password"
                        type="text"
                        name="captcha_input"
                        maxlength="5"
                        autocomplete="off"
                        :aria-label="t('login.captcha')"
                      >
                      <button
                        type="button"
                        class="captcha-image"
                        :title="t('login.captcha_refresh')"
                        :aria-label="t('login.captcha_refresh')"
                        @click="refreshCaptcha('password')"
                        v-html="captchaState.password.svg"
                      ></button>
                    </div>
                  </div>
                </div>

                <!-- 邮箱验证码登录区域 -->
                <div class="login-section" :class="{ active: mode === 'email-login' }">
                  <div class="login-form-group">
                    <label for="email-login-input">{{ t("auth.email") }}</label>
                    <div class="input-wrapper">
                      <input
                        id="email-login-input"
                        v-model="email"
                        type="email"
                        name="email"
                        :placeholder="t('login.email_placeholder')"
                        autocomplete="email"
                        :aria-label="t('auth.email')"
                      >
                    </div>
                  </div>

                  <div class="login-form-group">
                    <label for="email-login-code">{{ t("login.verify_code") }}</label>
                    <div class="form-inline-group">
                      <input
                        id="email-login-code"
                        v-model="emailCode"
                        type="text"
                        name="email_code"
                        :placeholder="t('login.code_placeholder')"
                        autocomplete="one-time-code"
                        :aria-label="t('login.verify_code')"
                        maxlength="6"
                        pattern="[0-9]{6}"
                      >
                      <button
                        type="button"
                        class="btn-send-code"
                        :disabled="sendingCode || sendCodeCountdown > 0"
                        @click="handleSendLoginCode"
                      >
                        {{ sendCodeLabel }}
                      </button>
                    </div>
                  </div>
                </div>

                <!-- LDAP 登录区域（使用 LDAP 目录账户） -->
                <div class="login-section" :class="{ active: mode === 'ldap-login' }">
                  <div class="login-form-group">
                    <label for="ldap-username">{{ t("login.ldap_username") }}</label>
                    <div class="input-wrapper">
                      <input
                        id="ldap-username"
                        v-model="ldapUsername"
                        type="text"
                        name="ldap_username"
                        :placeholder="t('login.ldap_username_placeholder')"
                        autocomplete="username"
                        :aria-label="t('login.ldap_username')"
                      >
                    </div>
                  </div>

                  <div class="login-form-group">
                    <label for="ldap-password">{{ t("auth.password") }}</label>
                    <div class="input-wrapper">
                      <input
                        id="ldap-password"
                        v-model="ldapPassword"
                        type="password"
                        name="ldap_password"
                        :placeholder="t('login.ldap_password_placeholder')"
                        autocomplete="current-password"
                        :aria-label="t('auth.password')"
                      >
                    </div>
                  </div>

                  <!-- 图形验证码（连续登录失败达到阈值后显示） -->
                  <div v-show="captchaState.ldap.visible" class="login-form-group">
                    <label for="ldap-captcha-input">{{ t("login.captcha") }}</label>
                    <div class="captcha-row">
                      <input
                        id="ldap-captcha-input"
                        v-model="captchaInput.ldap"
                        type="text"
                        name="ldap_captcha_input"
                        maxlength="5"
                        autocomplete="off"
                        :aria-label="t('login.captcha')"
                      >
                      <button
                        type="button"
                        class="captcha-image"
                        :title="t('login.captcha_refresh')"
                        :aria-label="t('login.captcha_refresh')"
                        @click="refreshCaptcha('ldap')"
                        v-html="captchaState.ldap.svg"
                      ></button>
                    </div>
                  </div>
                </div>

                <!-- SSO 登录区域（跳转 OIDC 认证平台） -->
                <div class="login-section" :class="{ active: mode === 'sso-login' }">
                  <div class="login-form-group">
                    <p class="login-hint-text">{{ t("login.sso_hint") }}</p>
                  </div>
                </div>

                <!-- 主登录按钮（带 hover 滑动箭头） -->
                <button
                  type="submit"
                  class="btn-login"
                  :disabled="submitting && activeView === 'login'"
                  :aria-label="t('login.sign_in')"
                >
                  <span class="btn-text">{{ loginSubmitLabel }}</span>
                  <span class="btn-hover-content">
                    <span>{{ t("login.sign_in") }}</span>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                      <line x1="5" y1="12" x2="19" y2="12" />
                      <polyline points="12 5 19 12 12 19" />
                    </svg>
                  </span>
                </button>

                <!-- 保持登录 + 忘记密码（同一行） -->
                <div class="form-options">
                  <label class="remember-me">
                    <input v-model="rememberMe" type="checkbox" name="remember" :aria-label="t('login.remember_me')">
                    <span>{{ t("login.remember_me") }}</span>
                  </label>
                  <button
                    type="button"
                    class="forgot-link link-button"
                    @click="showForgotModal = true"
                  >
                    {{ t("login.forgot_password") }}
                  </button>
                </div>
              </form>
            </div>

            <!-- 2FA 验证视图 -->
            <div class="view" :class="{ active: activeView === 'twoFactor' }">
              <div class="view-header">
                <h3>{{ t("login.two_factor_title") }}</h3>
                <p class="view-desc">{{ t("login.two_factor_desc") }}</p>
              </div>

              <form @submit.prevent="handleTwoFactorSubmit">
                <div class="login-form-group">
                  <label for="two-factor-code">{{ t("login.verify_code") }}</label>
                  <input
                    id="two-factor-code"
                    ref="twoFactorCodeInput"
                    v-model="twoFactorCode"
                    type="text"
                    name="two_factor_code"
                    :placeholder="t('login.code_placeholder')"
                    autocomplete="one-time-code"
                    :aria-label="t('login.verify_code')"
                    maxlength="6"
                    pattern="[0-9]{6}"
                    required
                  >
                </div>

                <div class="login-form-group password-hint">
                  <small class="login-hint-text">{{ t("login.two_factor_hint") }}</small>
                </div>

                <button
                  type="submit"
                  class="btn-login"
                  :disabled="submitting && activeView === 'twoFactor'"
                >
                  <span class="btn-text">{{ twoFactorSubmitLabel }}</span>
                  <span class="btn-hover-content">
                    <span>{{ t("login.verify_and_sign_in") }}</span>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                      <line x1="5" y1="12" x2="19" y2="12" />
                      <polyline points="12 5 19 12 12 19" />
                    </svg>
                  </span>
                </button>

                <button type="button" class="btn-text-full" @click="resetToInitState">
                  {{ t("login.back_to_login") }}
                </button>
              </form>
            </div>
          </div>

          <div class="login-error-message" :class="{ show: errorVisible }" role="alert" aria-live="polite">
            {{ errorMessage }}
          </div>

          <!-- 站点根证书下载入口：CA 证书是公开数据，登录前即可下载；
               无 CA 时整体隐藏 -->
          <div v-if="caAvailable" class="ca-download-entry">
            <a :href="CA_DOWNLOAD_URL" class="ca-download-link" download>
              {{ t("login.ca_download") }}
            </a>
          </div>
        </div>
      </div>

      <ForgotPasswordModal v-if="showForgotModal" @close="showForgotModal = false" />
    </div>
  </div>
</template>
