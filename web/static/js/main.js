import { errorHandler } from "./utils/errorHandler.js";
import { initI18n, t, getCurrentLanguage } from "./utils/i18n.js";
import { getUser, hasSession } from "./utils/sessionManager.js";
import { initModals } from "./utils/modal.js";
import { initModalTemplates } from "./utils/modalLoader.js";
import { initEventListeners } from "./modules/eventManager.js";
import { initNavigation } from "./modules/navigation.js";
import { displayCurrentUser, initAutoRefresh, initPageTimeout, initLogout, checkLoginStatus, } from "./modules/authManager.js";
import { initUserEvents } from "./modules/userManager.js";
import "./utils/constants.js";
const defaultConfig = {
    language: "zh-CN",
    debug: false,
    apiBaseUrl: "/api",
};
let appConfig = { ...defaultConfig };
let isInitialized = false;
async function initApp(config) {
    if (isInitialized) {
        console.warn("Application already initialized");
        return;
    }
    appConfig = { ...defaultConfig, ...config };
    try {
        if (appConfig.debug) {
            console.log("[IPMA] Initializing application with config:", appConfig);
        }
        await initI18n();
        await checkLoginStatus();
        initModalTemplates();
        initNavigation();
        initModals(getResourceCallbacks());
        initEventListeners();
        initUserEvents();
        displayCurrentUser();
        initLogout();
        initAutoRefresh();
        await initPageTimeout();
        isInitialized = true;
        if (appConfig.debug) {
            console.log("[IPMA] Application initialized successfully");
        }
    }
    catch (error) {
        errorHandler.handle(errorHandler.createError("INIT_ERROR", "Application initialization failed", "critical", { error }));
        const errorDiv = document.createElement("div");
        errorDiv.style.cssText = "position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%); padding: 20px; background: #fff; border: 1px solid #ccc; border-radius: 8px; text-align: center; z-index: 10000; box-shadow: 0 2px 10px rgba(0,0,0,0.1);";
        errorDiv.innerHTML = `
      <h3 style="margin: 0 0 15px 0; color: #e74c3c;">应用程序加载失败</h3>
      <p style="margin: 0 0 15px 0; color: #666;">请刷新页面重试，或联系管理员。</p>
      <button onclick="location.reload()" style="padding: 8px 16px; background: #3498db; color: #fff; border: none; border-radius: 4px; cursor: pointer;">刷新页面</button>
    `;
        document.body.appendChild(errorDiv);
        throw error;
    }
}
function getResourceCallbacks() {
    const createCallback = (module, method) => {
        return () => import(`./modules/${module}.js`).then((m) => {
            const fn = m[method];
            if (typeof fn === "function") {
                return fn();
            }
            return Promise.resolve();
        });
    };
    return {
        openNetworkTypeModal: createCallback("networks", "openNetworkTypeModal"),
        openNetworkModal: createCallback("networks", "openNetworkModal"),
        openRoomModal: createCallback("room", "openRoomModal"),
        openWorkstationModal: createCallback("workstation", "openWorkstationModal"),
        openCabinetModal: createCallback("cabinet", "openCabinetModal"),
        openCabinetPositionModal: createCallback("position", "openCabinetPositionModal"),
        openSwitchModal: createCallback("switch/switchDevice", "openSwitchModal"),
        openUserModal: createCallback("userManager", "openUserModal"),
        submitNetworkTypeForm: createCallback("networks", "submitNetworkTypeForm"),
        submitNetworkForm: createCallback("networks", "submitNetworkForm"),
        submitRoomForm: createCallback("room", "submitRoomForm"),
        submitWorkstationForm: createCallback("workstation", "submitWorkstationForm"),
        submitCabinetForm: createCallback("cabinet", "submitCabinetForm"),
        submitCabinetPositionForm: createCallback("position", "submitCabinetPositionForm"),
        submitSwitchForm: createCallback("switch/switchDevice", "submitSwitchForm"),
        submitSwitchPortForm: createCallback("switch/switchDevice", "submitSwitchPortForm"),
        submitUserForm: createCallback("userManager", "submitUserForm"),
        pullIpMacData: createCallback("ipmanager", "pullIpMacData"),
    };
}
function getConfig() {
    return { ...appConfig };
}
function isDebug() {
    return appConfig.debug;
}
export { initApp, getConfig, isDebug, isInitialized, getUser, hasSession, getCurrentLanguage, t, };
if (typeof window !== "undefined") {
    window.addEventListener("DOMContentLoaded", () => {
        const config = window.IPMA_CONFIG || {};
        initApp({
            language: config.language || "zh-CN",
            debug: config.debug || false,
            apiBaseUrl: config.apiBaseUrl || "/api",
        }).catch(error => {
            console.error("Failed to initialize application:", error);
        });
    });
}
//# sourceMappingURL=main.js.map