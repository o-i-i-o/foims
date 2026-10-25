/**
 * 会话管理：HttpOnly Cookie 承载真实凭证，此处仅缓存用户信息副本。
 * rememberMe 决定存 localStorage 还是 sessionStorage，两存储严格互斥。
 */
import type { User } from "../types/api/generated/User";

/** 会话用户快照：登录响应 data.user 即后端 User 模型，字段一致直接复用生成类型 */
export type UserInfo = User;

const USER_KEY = "user";
const REMEMBER_ME_KEY = "rememberMe";

// 浏览器禁用存储时 localStorage/sessionStorage 访问抛 SecurityError：
// 读写均需防护，失败时降级进程内副本（刷新后丢失，重新登录即可）
let memoryUser: UserInfo | null = null;

function safeGet(storage: Storage, key: string): { ok: boolean; value: string | null } {
  try {
    return { ok: true, value: storage.getItem(key) };
  } catch {
    return { ok: false, value: null };
  }
}

function safeSet(storage: Storage, key: string, value: string): void {
  try {
    storage.setItem(key, value);
  } catch {
    console.warn(`保存 ${key} 失败（存储被禁用）`);
  }
}

function safeRemove(storage: Storage, key: string): void {
  try {
    storage.removeItem(key);
  } catch {
    console.warn(`移除 ${key} 失败（存储被禁用）`);
  }
}

export function isRememberMe(): boolean {
  const { ok, value } = safeGet(localStorage, REMEMBER_ME_KEY);
  return ok && value === "true";
}

function getStorage(): Storage {
  return isRememberMe() ? localStorage : sessionStorage;
}

export function getUser(): UserInfo | null {
  // 严格按 rememberMe 标志读单一存储，不做跨存储回退：
  // 回退会让"非记住登录"的标签读到其他会话写入的副本，读写漂移
  const { ok, value: userJson } = safeGet(getStorage(), USER_KEY);
  if (!ok) {
    return memoryUser;
  }
  if (userJson) {
    try {
      return JSON.parse(userJson) as UserInfo;
    } catch {
      return null;
    }
  }
  return null;
}

export function hasSession(): boolean {
  return getUser() !== null;
}

export function setUser(user: UserInfo, rememberMe = false): void {
  clearSession();
  memoryUser = user;
  safeSet(localStorage, REMEMBER_ME_KEY, rememberMe ? "true" : "false");
  safeSet(getStorage(), USER_KEY, JSON.stringify(user));
}

export function clearSession(): void {
  memoryUser = null;
  safeRemove(localStorage, USER_KEY);
  safeRemove(localStorage, REMEMBER_ME_KEY);
  safeRemove(sessionStorage, USER_KEY);
}

export function updateUser(updates: Partial<UserInfo>): UserInfo | null {
  const user = getUser();
  if (user) {
    const updatedUser: UserInfo = { ...user, ...updates };
    memoryUser = updatedUser;
    safeSet(getStorage(), USER_KEY, JSON.stringify(updatedUser));
    return updatedUser;
  }
  return null;
}
