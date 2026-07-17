const moduleCache = new Map();
const loadingPromises = new Map();
const preloadedModules = new Set();

export const MODULE_VERSION = '01203';

function withVersion(path) {
    if (!path) return path;
    return path.includes('?') ? `${path}&v=${MODULE_VERSION}` : `${path}?v=${MODULE_VERSION}`;
}

const MODULE_REGISTRY = {
    'apiClient': '/static/js/utils/apiClient.js',
    'confirm': '/static/js/utils/confirm.js',
    'formatter': '/static/js/utils/formatter.js',
    'helpers': '/static/js/utils/helpers.js',
    'i18n': '/static/js/utils/i18n.js',
    'ipconfig': '/static/js/utils/ipconfig.js',
    'networkCardManager': '/static/js/utils/networkCardManager.js',
    'modal': '/static/js/utils/modal.js',
    'modalLoader': '/static/js/utils/modalLoader.js',
    'pagination': '/static/js/utils/pagination.js',
    'resources': '/static/js/utils/resources.js',
    'sessionManager': '/static/js/utils/sessionManager.js',
    'styleLoader': '/static/js/utils/styleLoader.js',
    'toast': '/static/js/utils/toast.js',
    'ui': '/static/js/utils/ui.js',
    'dashboard': '/static/js/modules/dashboard.js',
    'navigation': '/static/js/modules/navigation.js',
    'networks': '/static/js/modules/networks.js',
    'organization': '/static/js/modules/organization.js',
    'room': '/static/js/modules/room.js',
    'workstation': '/static/js/modules/workstation.js',
    'cabinet': '/static/js/modules/cabinet.js',
    'position': '/static/js/modules/position.js',
    'netOutlet': '/static/js/modules/netOutlet.js',
    'userManager': '/static/js/modules/userManager.js',
    'systemManager': '/static/js/modules/systemManager.js',
    'log': '/static/js/modules/log.js',
    'ipmanager': '/static/js/modules/ipmanager.js',
    'resourceTabs': '/static/js/modules/resourceTabs.js',
    'eventManager': '/static/js/modules/eventManager.js',
    'authManager': '/static/js/modules/authManager.js',
    'device': '/static/js/modules/device.js',
    'devicePorts': '/static/js/modules/devicePorts.js',
    'deviceMacLldp': '/static/js/modules/deviceMacLldp.js',
    'deviceSnmp': '/static/js/modules/deviceSnmp.js',
    'visualizationManager': '/static/js/modules/visualization/visualizationManager.js',
    'SVGVisualization': '/static/js/modules/visualization/SVGVisualization.js',
    'TopologyVisualization': '/static/js/modules/visualization/TopologyVisualization.js',
    'TopologyModal': '/static/js/modules/visualization/TopologyModal.js',
};

export async function loadModule(moduleName, modulePath = null) {
    if (moduleCache.has(moduleName)) {
        return moduleCache.get(moduleName);
    }
    
    if (loadingPromises.has(moduleName)) {
        return loadingPromises.get(moduleName);
    }
    
    const basePath = modulePath || MODULE_REGISTRY[moduleName];
    if (!basePath) {
        throw new Error(`Module "${moduleName}" not found in registry`);
    }

    const path = withVersion(basePath);
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
    const { 
        delay = 0, 
        when = 'idle',
        condition = () => true 
    } = options;
    
    return new Promise((resolve, reject) => {
        const executeLoad = async () => {
            if (!condition()) {
                resolve(null);
                return;
            }
            try {
                const module = await loadModule(moduleName);
                resolve(module);
            } catch (error) {
                reject(error);
            }
        };
        
        if (delay > 0) {
            setTimeout(() => {
                if (when === 'idle' && 'requestIdleCallback' in window) {
                    requestIdleCallback(executeLoad, { timeout: delay });
                } else {
                    executeLoad();
                }
            }, delay);
        } else if (when === 'idle' && 'requestIdleCallback' in window) {
            requestIdleCallback(executeLoad, { timeout: 5000 });
        } else if (when === 'visible' && options.selector) {
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
        } else {
            executeLoad();
        }
    });
}

function preloadModule(moduleName) {
    if (preloadedModules.has(moduleName) || moduleCache.has(moduleName)) {
        return Promise.resolve();
    }
    
    return loadModule(moduleName).then(module => {
        preloadedModules.add(moduleName);
        return module;
    }).catch(error => {
        console.warn(`预加载模块 ${moduleName} 失败:`, error);
    });
}

function preloadModules(moduleNames) {
    return Promise.allSettled(moduleNames.map(name => preloadModule(name)));
}

function prefetchModule(moduleName) {
    const basePath = MODULE_REGISTRY[moduleName];
    if (!basePath || preloadedModules.has(moduleName)) {
        return;
    }

    const path = withVersion(basePath);
    const link = document.createElement('link');
    link.rel = 'modulepreload';
    link.href = path;
    document.head.appendChild(link);
    preloadedModules.add(moduleName);
}

export function prefetchModules(moduleNames) {
    moduleNames.forEach(name => prefetchModule(name));
}

export function schedulePreload(moduleNames, options = {}) {
    const { 
        delay = 1000,
        priority = 'low'
    } = options;
    
    const execute = () => {
        if (priority === 'high') {
            preloadModules(moduleNames);
        } else {
            moduleNames.forEach(name => {
                lazyLoad(name, { delay: 0, when: 'idle' });
            });
        }
    };
    
    if ('requestIdleCallback' in window) {
        requestIdleCallback(execute, { timeout: delay });
    } else {
        setTimeout(execute, delay);
    }
}

export function getCachedModule(moduleName) {
    return moduleCache.get(moduleName) || null;
}
