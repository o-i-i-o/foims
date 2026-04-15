const moduleCache = new Map();
export async function loadModule(moduleName, modulePath) {
    if (moduleCache.has(moduleName)) {
        return moduleCache.get(moduleName);
    }
    try {
        const module = await import(modulePath);
        moduleCache.set(moduleName, module);
        return module;
    }
    catch (error) {
        console.error(`加载模块 ${moduleName} 失败:`, error);
        throw error;
    }
}
//# sourceMappingURL=moduleLoader.js.map