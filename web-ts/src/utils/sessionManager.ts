import type { User } from "../types/session.js";

export class SessionManager {
  static #userKey = "user";
  static #rememberMeKey = "rememberMe";

  static isRememberMe(): boolean {
    return localStorage.getItem(this.#rememberMeKey) === "true";
  }

  static hasSession(): boolean {
    return this.getUser() !== null;
  }

  static getUser(): User | null {
    const userJson = localStorage.getItem(this.#userKey);

    if (userJson) {
      try {
        return JSON.parse(userJson) as User;
      } catch {
        return null;
      }
    }
    return null;
  }

  static setUser(user: User, rememberMe = false): void {
    this.clear();

    localStorage.setItem(this.#rememberMeKey, rememberMe ? "true" : "false");
    localStorage.setItem(this.#userKey, JSON.stringify(user));
  }

  static clear(): void {
    localStorage.removeItem(this.#userKey);
    localStorage.removeItem(this.#rememberMeKey);
  }

  static updateUser(updates: Partial<User>): User | null {
    const user = this.getUser();
    if (user) {
      const updatedUser = { ...user, ...updates };
      localStorage.setItem(this.#userKey, JSON.stringify(updatedUser));
      return updatedUser;
    }
    return null;
  }
}

export const getUser = (): User | null => SessionManager.getUser();
export const setUser = (user: User, rememberMe?: boolean): void => SessionManager.setUser(user, rememberMe);
export const clearSession = (): void => SessionManager.clear();
export const hasSession = (): boolean => SessionManager.hasSession();
export const isRememberMe = (): boolean => SessionManager.isRememberMe();
