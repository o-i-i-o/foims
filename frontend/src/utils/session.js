/**
 * 会话管理：HttpOnly Cookie 承载真实凭证，此处仅缓存用户信息副本。
 * 与旧版 web/static/js/utils/sessionManager.js 语义一致：
 * rememberMe 决定存 localStorage 还是 sessionStorage，两存储严格互斥。
 */

const USER_KEY = "user";
const REMEMBER_ME_KEY = "rememberMe";

// 浏览器禁用存储时 localStorage/sessionStorage 访问抛 SecurityError：
// 读写均需防护，失败时降级进程内副本（刷新后丢失，重新登录即可）
let memoryUser = null;

function safeGet(storage, key) {
  try {
    return { ok: true, value: storage.getItem(key) };
  } catch {
    return { ok: false, value: null };
  }
}

function safeSet(storage, key, value) {
  try {
    storage.setItem(key, value);
  } catch {
    console.warn(`保存 ${key} 失败（存储被禁用）`);
  }
}

function safeRemove(storage, key) {
  try {
    storage.removeItem(key);
  } catch {
    console.warn(`移除 ${key} 失败（存储被禁用）`);
  }
}

export function isRememberMe() {
  const { ok, value } = safeGet(localStorage, REMEMBER_ME_KEY);
  return ok && value === "true";
}

function getStorage() {
  return isRememberMe() ? localStorage : sessionStorage;
}

export function getUser() {
  // 严格按 rememberMe 标志读单一存储，不做跨存储回退：
  // 回退会让"非记住登录"的标签读到其他会话写入的副本，读写漂移
  const { ok, value: userJson } = safeGet(getStorage(), USER_KEY);
  if (!ok) {
    return memoryUser;
  }
  if (userJson) {
    try {
      return JSON.parse(userJson);
    } catch {
      return null;
    }
  }
  return null;
}

export function hasSession() {
  return getUser() !== null;
}

export function setUser(user, rememberMe = false) {
  clearSession();
  memoryUser = user;
  safeSet(localStorage, REMEMBER_ME_KEY, rememberMe ? "true" : "false");
  safeSet(getStorage(), USER_KEY, JSON.stringify(user));
}

export function clearSession() {
  memoryUser = null;
  safeRemove(localStorage, USER_KEY);
  safeRemove(localStorage, REMEMBER_ME_KEY);
  safeRemove(sessionStorage, USER_KEY);
}

export function updateUser(updates) {
  const user = getUser();
  if (user) {
    const updatedUser = { ...user, ...updates };
    memoryUser = updatedUser;
    safeSet(getStorage(), USER_KEY, JSON.stringify(updatedUser));
    return updatedUser;
  }
  return null;
}
