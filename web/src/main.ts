import { errorHandler } from "./utils/errorHandler.js";
import { t, getCurrentLanguage } from "./utils/i18n.js";
import { getUser, hasSession } from "./utils/sessionManager.js";
import { initGlobalEvents } from "./modules/eventManager.js";
import { initNavigation } from "./modules/navigation.js";

import "./utils/constants.js";

interface AppConfig {
  language: string;
  debug: boolean;
  apiBaseUrl: string;
}

const defaultConfig: AppConfig = {
  language: "zh-CN",
  debug: false,
  apiBaseUrl: "/api",
};

let appConfig: AppConfig = { ...defaultConfig };
let isInitialized = false;

async function initApp(config?: Partial<AppConfig>): Promise<void> {
  if (isInitialized) {
    console.warn("Application already initialized");
    return;
  }

  appConfig = { ...defaultConfig, ...config };

  try {
    if (appConfig.debug) {
      console.log("[IPMA] Initializing application with config:", appConfig);
    }

    initNavigation();
    initGlobalEvents();

    await loadPageModule();

    isInitialized = true;

    if (appConfig.debug) {
      console.log("[IPMA] Application initialized successfully");
    }
  } catch (error) {
    errorHandler.handle(errorHandler.createError(
      "INIT_ERROR",
      "Application initialization failed",
      "critical",
      { error }
    ));
    throw error;
  }
}

async function loadPageModule(): Promise<void> {
  const path = window.location.pathname;
  const page = path.split("/").pop() || "index";

  const moduleMap: Record<string, () => Promise<void>> = {
    "": async () => {},
    index: async () => {},
    rooms: async () => {
      const m = await import("./modules/room.js");
      m.initRoomSortEvents();
      m.initRoomTableEvents();
    },
    cabinets: async () => {
      const m = await import("./modules/cabinet.js");
      m.initCabinetSortEvents();
      m.initCabinetTableEvents();
    },
    workstations: async () => {
      const m = await import("./modules/workstation.js");
      m.initWorkstationSortEvents();
      m.initWorkstationTableEvents();
    },
    networks: async () => {},
    users: async () => {
      const m = await import("./modules/userManager.js");
      m.initUserEvents();
    },
    switches: async () => {},
    logs: async () => {},
    settings: async () => {},
  };

  const loader = moduleMap[page];
  if (loader) {
    await loader();
  }
}

function getConfig(): Readonly<AppConfig> {
  return { ...appConfig };
}

function isDebug(): boolean {
  return appConfig.debug;
}

export {
  initApp,
  getConfig,
  isDebug,
  isInitialized,
  getUser,
  hasSession,
  getCurrentLanguage,
  t,
};

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
