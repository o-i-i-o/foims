export class SessionManager {
  static #userKey = "user";
  static #rememberMeKey = "rememberMe";

  static #getStorage() {
    return this.isRememberMe() ? localStorage : sessionStorage;
  }

  static isRememberMe() {
    return localStorage.getItem(this.#rememberMeKey) === "true";
  }

  static hasSession() {
    return this.getUser() !== null;
  }

  static getUser() {
    // 严格按 rememberMe 标志读单一存储，不做 localStorage 回退：
    // 回退会让"非记住登录"的标签读到其他会话写入 localStorage 的
    // 用户副本，两个标签以不同 rememberMe 登录时读写漂移
    const userJson = this.#getStorage().getItem(this.#userKey);

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

    localStorage.setItem(this.#rememberMeKey, rememberMe ? "true" : "false");
    const storage = this.#getStorage();
    storage.setItem(this.#userKey, JSON.stringify(user));
  }

  static clear() {
    localStorage.removeItem(this.#userKey);
    localStorage.removeItem(this.#rememberMeKey);
    sessionStorage.removeItem(this.#userKey);
  }

  static updateUser(updates) {
    const user = this.getUser();
    if (user) {
      const updatedUser = { ...user, ...updates };
      const storage = this.#getStorage();
      storage.setItem(this.#userKey, JSON.stringify(updatedUser));
      return updatedUser;
    }
    return null;
  }
}
