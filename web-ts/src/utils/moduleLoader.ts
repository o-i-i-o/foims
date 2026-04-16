const moduleCache = new Map<string, unknown>();

export async function loadModule<T = unknown>(moduleName: string, modulePath: string): Promise<T> {
  if (moduleCache.has(moduleName)) {
    return moduleCache.get(moduleName) as T;
  }

  try {
    const module = await import(modulePath) as T;
    moduleCache.set(moduleName, module);
    return module;
  } catch (error) {
    console.error(`加载模块 ${moduleName} 失败:`, error);
    throw error;
  }
}
