// 浏览器禁用存储（"阻止所有 Cookie"/存储策略）时 localStorage 访问抛
// SecurityError：apiClient 每个请求都会经过 hasSession()，读写均需防护，
// 失败时按"无会话"降级（登录态本就依赖 Cookie，存储仅缓存用户信息）
// 返回 {ok, value}：ok=false 表示存储访问抛错（被禁用），
// 此时调用方才回退进程内副本；正常路径不受内存副本影响
function safeStorageGet(storage, key) {
  try {
    return { ok: true, value: storage.getItem(key) };
  } catch (error) {
    console.warn(`读取 ${key} 失败（存储被禁用）:`, error);
    return { ok: false, value: null };
  }
}

function safeStorageSet(storage, key, value) {
  try {
    storage.setItem(key, value);
  } catch (error) {
    console.warn(`保存 ${key} 失败（存储被禁用）:`, error);
  }
}

function safeStorageRemove(storage, key) {
  try {
    storage.removeItem(key);
  } catch (error) {
    console.warn(`移除 ${key} 失败（存储被禁用）:`, error);
  }
}

export class SessionManager {
  static #userKey = "user";
  static #rememberMeKey = "rememberMe";
  static #memoryUser = null;

  static #getStorage() {
    return this.isRememberMe() ? localStorage : sessionStorage;
  }

  static isRememberMe() {
    const { ok, value } = safeStorageGet(localStorage, this.#rememberMeKey);
    return ok && value === "true";
  }

  static hasSession() {
    return this.getUser() !== null;
  }

  static getUser() {
    // 严格按 rememberMe 标志读单一存储，不做 localStorage 回退：
    // 回退会让"非记住登录"的标签读到其他会话写入 localStorage 的
    // 用户副本，两个标签以不同 rememberMe 登录时读写漂移
    const { ok, value: userJson } = safeStorageGet(this.#getStorage(), this.#userKey);
    if (!ok) {
      // 存储被禁用时退化为进程内副本（刷新后丢失，重新登录即可）
      return this.#memoryUser;
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

  static setUser(user, rememberMe = false) {
    this.clear();

    this.#memoryUser = user;
    safeStorageSet(localStorage, this.#rememberMeKey, rememberMe ? "true" : "false");
    safeStorageSet(this.#getStorage(), this.#userKey, JSON.stringify(user));
  }

  static clear() {
    this.#memoryUser = null;
    safeStorageRemove(localStorage, this.#userKey);
    safeStorageRemove(localStorage, this.#rememberMeKey);
    safeStorageRemove(sessionStorage, this.#userKey);
  }

  static updateUser(updates) {
    const user = this.getUser();
    if (user) {
      const updatedUser = { ...user, ...updates };
      this.#memoryUser = updatedUser;
      safeStorageSet(this.#getStorage(), this.#userKey, JSON.stringify(updatedUser));
      return updatedUser;
    }
    return null;
  }
}
