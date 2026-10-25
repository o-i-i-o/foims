/**
 * 认证域 API：会话探测 / 初始化状态 / 认证方式 / 本地·邮箱·LDAP·两步登录 /
 * 图形验证码 / 邮箱登录验证码 / SSO 发起 / 站点 CA 状态。
 *
 * 职责边界：只组装 URL 与请求体并返回 ApiResult，不做 toast / i18n / 状态管理。
 */
import { apiGet, apiPost } from "./client";
import type { ApiResult } from "../types/api";
import type { CaStatus } from "../types/api/generated/CaStatus";
import type { EmailLoginRequest } from "../types/api/generated/EmailLoginRequest";
import type { InitStatus } from "../types/api/generated/InitStatus";
import type { LdapLoginRequest } from "../types/api/generated/LdapLoginRequest";
import type { TwoFactorLoginRequest } from "../types/api/generated/TwoFactorLoginRequest";
import type { User } from "../types/api/generated/User";
import type { UserLogin } from "../types/api/generated/UserLogin";

// 验证码字段仅在触发后携带（与旧版请求字节形态一致，其余场景不发 null 键）
export type LoginBody = Omit<UserLogin, "captcha_id" | "captcha_text"> & {
  captcha_id?: string | null;
  captcha_text?: string | null;
};

// LDAP 2FA 重试载荷：原登录字段 + 动态码 code
export type LdapTwoFactorBody = Omit<LdapLoginRequest, "captcha_id" | "captcha_text"> & {
  captcha_id?: string | null;
  captcha_text?: string | null;
  code: string;
};

// 登录端点载荷：2FA 分支返回 requires_two_factor（邮箱分支附带 username），否则为用户信息
export interface LoginPayload {
  requires_two_factor?: boolean;
  user: User;
  username?: string;
}

/** 已启用的登录方式（GET /api/auth/methods，后端 json! 动态构造） */
export interface AuthMethods {
  email?: boolean;
  ldap?: boolean;
  sso?: boolean;
}

/** 图形验证码载荷（GET /api/auth/captcha） */
export interface CaptchaResponse {
  captcha_id: string;
  svg: string;
}

/** SSO 登录发起地址（浏览器直链跳转，由后端完成 OIDC 授权码流程） */
export const SSO_LOGIN_URL = "/api/auth/sso/login";

/**
 * 会话探测：返回原始 Response 由调用方按状态码分支。
 *
 * 不走统一客户端：/api/auth/me 的 401 若触发客户端级「跳登录页」
 * 会形成本页自我重载循环，登录页需静默回退到未登录展示。
 */
export function probeSession(): Promise<Response> {
  return fetch("/api/auth/me", { credentials: "include" });
}

/** 初始化模式（config.toml [init].enabled） */
export function getInitStatus(): Promise<ApiResult<InitStatus>> {
  return apiGet<InitStatus>("/api/auth/init-status");
}

export function getAuthMethods(): Promise<ApiResult<AuthMethods>> {
  return apiGet<AuthMethods>("/api/auth/methods");
}

/** 站点 CA 状态（登录页根证书下载入口的可用性判定） */
export function getCaStatus(): Promise<ApiResult<CaStatus>> {
  return apiGet<CaStatus>("/api/certificate/ca/info");
}

/** 本地账号密码登录 */
export function login(payload: LoginBody): Promise<ApiResult<LoginPayload>> {
  return apiPost<LoginPayload, LoginBody>("/api/auth/login", payload);
}

/** 邮箱验证码登录 */
export function loginWithEmail(payload: EmailLoginRequest): Promise<ApiResult<LoginPayload>> {
  return apiPost<LoginPayload, EmailLoginRequest>("/api/auth/login/email", payload);
}

/** LDAP 登录（失败计数达阈值时载荷可携带图形验证码） */
export function loginWithLdap(payload: LoginBody): Promise<ApiResult<LoginPayload>> {
  return apiPost<LoginPayload, LoginBody>("/api/auth/login/ldap", payload);
}

/** LDAP 2FA 重试：重试原端点并携带动态码 */
export function retryLdapWithTwoFactor(payload: LdapTwoFactorBody): Promise<ApiResult<LoginPayload>> {
  return apiPost<LoginPayload, LdapTwoFactorBody>("/api/auth/login/ldap", payload);
}

/** 本地账号动态码校验（登录第二阶段提交） */
export function loginWithTwoFactor(payload: TwoFactorLoginRequest): Promise<ApiResult<LoginPayload>> {
  return apiPost<LoginPayload, TwoFactorLoginRequest>("/api/auth/login/two-factor", payload);
}

/** 图形验证码（点击图片刷新） */
export function fetchCaptcha(): Promise<ApiResult<CaptchaResponse>> {
  return apiGet<CaptchaResponse>("/api/auth/captcha");
}

/** 发送邮箱登录验证码 */
export function sendLoginCode(email: string): Promise<ApiResult<unknown>> {
  return apiPost("/api/auth/login/send-code", { email });
}
