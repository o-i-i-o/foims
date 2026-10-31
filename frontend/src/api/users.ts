/**
 * 用户域 API：用户 CRUD / 收件人候选列表 / 双因素认证（2FA）管理。
 *
 * 职责边界：只组装 URL 与请求体并返回 ApiResult，不做 toast / i18n / 状态管理。
 */
import { apiDelete, apiGet, apiPost, apiPut } from "./client";
import type { ApiResult, Paged } from "../types/api";
import type { TwoFactorDisableRequest } from "../types/api/generated/TwoFactorDisableRequest";
import type { TwoFactorEnableRequest } from "../types/api/generated/TwoFactorEnableRequest";
import type { TwoFactorInitRequest } from "../types/api/generated/TwoFactorInitRequest";
import type { User } from "../types/api/generated/User";

/** 用户新增/编辑表单载荷（新增与编辑共用；password 缺省表示不改密码，phone 空串表示清空） */
export interface UserFormPayload {
  username: string;
  email: string;
  phone: string;
  role: string;
  enabled: boolean;
  password_expiry_days: number;
  password?: string;
}

/** 2FA 初始化响应（后端 json! 动态构造，未生成 ts-rs 类型） */
export interface TwoFactorInitData {
  qr_code_base64?: string | null;
  secret?: string | null;
}

/** 分页用户列表地址（与 usePagedList 的 buildUrl 签名一一对应） */
export function usersUrl(page: number, pageSize: number, sortBy: string, sortOrder: string): string {
  return `/api/users?page=${page}&page_size=${pageSize}&sort_by=${sortBy}&sort_order=${sortOrder}`;
}

/**
 * 收件人候选清单：page_size=1000（后端上限）。
 * 默认 20/页会漏掉第 21 个及之后的可选收件人。
 */
export function listUsersForPicker(): Promise<ApiResult<Paged<User>>> {
  return apiGet<Paged<User>>("/api/users?page_size=1000");
}

export function getUser(id: string): Promise<ApiResult<User>> {
  return apiGet<User>(`/api/users/${id}`);
}

export function createUser(payload: UserFormPayload): Promise<ApiResult<unknown>> {
  return apiPost("/api/users", payload);
}

export function updateUser(id: string, payload: UserFormPayload): Promise<ApiResult<unknown>> {
  return apiPut(`/api/users/${id}`, payload);
}

export function deleteUser(id: string): Promise<ApiResult<unknown>> {
  return apiDelete(`/api/users/${id}`);
}

export function initTwoFactor(userId: string): Promise<ApiResult<TwoFactorInitData>> {
  const payload: TwoFactorInitRequest = { user_id: userId };
  return apiPost<TwoFactorInitData, TwoFactorInitRequest>("/api/two-factor/init", payload);
}

export function enableTwoFactor(userId: string, code: string): Promise<ApiResult<unknown>> {
  const payload: TwoFactorEnableRequest = { code, user_id: userId };
  return apiPost<unknown, TwoFactorEnableRequest>("/api/two-factor/enable", payload);
}

export function disableTwoFactor(userId: string, code: string): Promise<ApiResult<unknown>> {
  const payload: TwoFactorDisableRequest = { code, user_id: userId };
  return apiPost<unknown, TwoFactorDisableRequest>("/api/two-factor/disable", payload);
}
