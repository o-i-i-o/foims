export class SessionManager {
    static #userKey = "user";
    static #rememberMeKey = "rememberMe";
    static isRememberMe() {
        return localStorage.getItem(this.#rememberMeKey) === "true";
    }
    static hasSession() {
        return this.getUser() !== null;
    }
    static getUser() {
        const userJson = localStorage.getItem(this.#userKey);
        if (userJson) {
            try {
                return JSON.parse(userJson);
            }
            catch {
                return null;
            }
        }
        return null;
    }
    static setUser(user, rememberMe = false) {
        this.clear();
        localStorage.setItem(this.#rememberMeKey, rememberMe ? "true" : "false");
        localStorage.setItem(this.#userKey, JSON.stringify(user));
    }
    static clear() {
        localStorage.removeItem(this.#userKey);
        localStorage.removeItem(this.#rememberMeKey);
    }
    static updateUser(updates) {
        const user = this.getUser();
        if (user) {
            const updatedUser = { ...user, ...updates };
            localStorage.setItem(this.#userKey, JSON.stringify(updatedUser));
            return updatedUser;
        }
        return null;
    }
}
export const getUser = () => SessionManager.getUser();
export const setUser = (user, rememberMe) => SessionManager.setUser(user, rememberMe);
export const clearSession = () => SessionManager.clear();
export const hasSession = () => SessionManager.hasSession();
export const isRememberMe = () => SessionManager.isRememberMe();
//# sourceMappingURL=sessionManager.js.map