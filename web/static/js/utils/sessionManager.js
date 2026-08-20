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
    const storage = this.#getStorage();
    const userJson = storage.getItem(this.#userKey);

    if (userJson) {
      try {
        return JSON.parse(userJson);
      } catch {
        return null;
      }
    }

    const localUserJson = localStorage.getItem(this.#userKey);
    if (localUserJson) {
      try {
        return JSON.parse(localUserJson);
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
