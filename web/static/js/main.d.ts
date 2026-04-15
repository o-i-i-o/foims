import { t, getCurrentLanguage } from "./utils/i18n.js";
import { getUser, hasSession } from "./utils/sessionManager.js";
import "./utils/constants.js";
interface AppConfig {
    language: string;
    debug: boolean;
    apiBaseUrl: string;
}
declare let isInitialized: boolean;
declare function initApp(config?: Partial<AppConfig>): Promise<void>;
declare function getConfig(): Readonly<AppConfig>;
declare function isDebug(): boolean;
export { initApp, getConfig, isDebug, isInitialized, getUser, hasSession, getCurrentLanguage, t, };
//# sourceMappingURL=main.d.ts.map