import type { LoginData } from "../types/session.js";
export declare const loginUser: (data: LoginData, rememberMe: boolean) => void;
export declare const logoutUser: () => Promise<void>;
export declare const checkLoginStatus: () => Promise<void>;
export declare const displayCurrentUser: () => void;
export declare const initLogout: () => void;
export declare const initAutoRefresh: () => void;
export declare const initPageTimeout: () => Promise<void>;
//# sourceMappingURL=authManager.d.ts.map