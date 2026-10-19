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

// 纯静态类改为对象字面量（noStaticOnlyClass / no-extraneous-class）：
// 私有静态字段与方法上移为模块级状态与函数，对外导出与调用方式不变
const USER_KEY = "user";
const REMEMBER_ME_KEY = "rememberMe";
let memoryUser = null;

function getStorage() {
  return SessionManager.isRememberMe() ? localStorage : sessionStorage;
}

export const SessionManager = {
  isRememberMe() {
    const { ok, value } = safeStorageGet(localStorage, REMEMBER_ME_KEY);
    return ok && value === "true";
  },

  hasSession() {
    return SessionManager.getUser() !== null;
  },

  getUser() {
    // 严格按 rememberMe 标志读单一存储，不做 localStorage 回退：
    // 回退会让"非记住登录"的标签读到其他会话写入 localStorage 的
    // 用户副本，两个标签以不同 rememberMe 登录时读写漂移
    const { ok, value: userJson } = safeStorageGet(getStorage(), USER_KEY);
    if (!ok) {
      // 存储被禁用时退化为进程内副本（刷新后丢失，重新登录即可）
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
  },

  setUser(user, rememberMe = false) {
    SessionManager.clear();

    memoryUser = user;
    safeStorageSet(localStorage, REMEMBER_ME_KEY, rememberMe ? "true" : "false");
    safeStorageSet(getStorage(), USER_KEY, JSON.stringify(user));
  },

  clear() {
    memoryUser = null;
    safeStorageRemove(localStorage, USER_KEY);
    safeStorageRemove(localStorage, REMEMBER_ME_KEY);
    safeStorageRemove(sessionStorage, USER_KEY);
  },

  updateUser(updates) {
    const user = SessionManager.getUser();
    if (user) {
      const updatedUser = { ...user, ...updates };
      memoryUser = updatedUser;
      safeStorageSet(getStorage(), USER_KEY, JSON.stringify(updatedUser));
      return updatedUser;
    }
    return null;
  }
};
