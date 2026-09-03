const moduleCache = new Map();
const loadingPromises = new Map();
const preloadedModules = new Set();

/* 版本号仅用于 CSS / 模态框 HTML 等经 fetch 加载的资源的缓存穿透；
   JS 模块动态 import 一律使用无版本号 URL —— 与静态 import 保持同一 URL 空间，
   避免同一模块因 URL 不同产生双实例、双份独立状态 */
export const MODULE_VERSION = "01372";

export function withVersion(path) {
  if (!path) {
    return path;
  }
  return path.includes("?") ? `${path}&v=${MODULE_VERSION}` : `${path}?v=${MODULE_VERSION}`;
}

/* 模块注册表：仅收录真正经 loadModule 动态加载的模块。
   静态导入的工具（apiClient/ui/toast 等）不在此列 —— 混注册会让读者
   误判哪些是活的懒加载入口（双实例问题见 loadModule 注释）。 */
const MODULE_REGISTRY = {
  networkCardManager: "/static/js/utils/networkCardManager.js",
  dashboard: "/static/js/modules/dashboard.js",
  networks: "/static/js/modules/networks.js",
  organization: "/static/js/modules/organization.js",
  room: "/static/js/modules/room.js",
  workstation: "/static/js/modules/workstation.js",
  cabinet: "/static/js/modules/cabinet.js",
  position: "/static/js/modules/position.js",
  cableLink: "/static/js/modules/cableLink.js",
  userManager: "/static/js/modules/userManager.js",
  systemManager: "/static/js/modules/systemManager.js",
  log: "/static/js/modules/log.js",
  ipDetail: "/static/js/modules/ipDetail.js",
  resourceTabs: "/static/js/modules/resourceTabs.js",
  authManager: "/static/js/modules/authManager.js",
  device: "/static/js/modules/device.js",
  visualizationManager: "/static/js/modules/visualization/visualizationManager.js",
  SVGVisualization: "/static/js/modules/visualization/SVGVisualization.js",
  TopologyVisualization: "/static/js/modules/visualization/TopologyVisualization.js",
  TopologyModal: "/static/js/modules/visualization/TopologyModal.js"
};

export async function loadModule(moduleName, modulePath = null) {
  if (moduleCache.has(moduleName)) {
    return moduleCache.get(moduleName);
  }

  if (loadingPromises.has(moduleName)) {
    return loadingPromises.get(moduleName);
  }

  const path = modulePath || MODULE_REGISTRY[moduleName];
  if (!path) {
    throw new Error(`Module "${moduleName}" not found in registry`);
  }

  const promise = (async () => {
    try {
      const module = await import(path);
      moduleCache.set(moduleName, module);
      loadingPromises.delete(moduleName);
      return module;
    } catch (error) {
      loadingPromises.delete(moduleName);
      throw error;
    }
  })();

  loadingPromises.set(moduleName, promise);
  return promise;
}

export function lazyLoad(moduleName, options = {}) {
  const { delay = 0, when = "idle", condition = () => true } = options;

  return new Promise((resolve, reject) => {
    const executeLoad = async () => {
      if (!condition()) {
        resolve(null);
        return;
      }
      try {
        resolve(await loadModule(moduleName));
      } catch (error) {
        reject(error);
      }
    };

    // 进入视口后加载（与 delay 无关，IntersectionObserver 触发即执行）
    if (when === "visible" && options.selector) {
      const observer = new IntersectionObserver((entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            observer.disconnect();
            executeLoad();
            break;
          }
        }
      });
      const element = document.querySelector(options.selector);
      if (element) {
        observer.observe(element);
      } else {
        executeLoad();
      }
      return;
    }

    if (when === "idle" && "requestIdleCallback" in window) {
      // delay 作为 idle 回调的超时上限：最迟 delay 毫秒后强制执行
      requestIdleCallback(executeLoad, { timeout: Math.max(delay, 5000) });
    } else if (delay > 0) {
      setTimeout(executeLoad, delay);
    } else {
      executeLoad();
    }
  });
}

function preloadModule(moduleName) {
  if (preloadedModules.has(moduleName) || moduleCache.has(moduleName)) {
    return Promise.resolve();
  }

  return loadModule(moduleName)
    .then((module) => {
      preloadedModules.add(moduleName);
      return module;
    })
    .catch((error) => {
      console.warn(`预加载模块 ${moduleName} 失败:`, error);
    });
}

function preloadModules(moduleNames) {
  return Promise.allSettled(moduleNames.map((name) => preloadModule(name)));
}

export function schedulePreload(moduleNames, options = {}) {
  const { delay = 1000, priority = "low" } = options;

  const execute = () => {
    if (priority === "high") {
      preloadModules(moduleNames);
    } else {
      moduleNames.forEach((name) => {
        lazyLoad(name, { delay: 0, when: "idle" });
      });
    }
  };

  if ("requestIdleCallback" in window) {
    requestIdleCallback(execute, { timeout: delay });
  } else {
    setTimeout(execute, delay);
  }
}
