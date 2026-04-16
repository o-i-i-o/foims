// 动态导入模块缓存
const moduleCache = new Map();

// 动态导入辅助函数（带缓存）
export async function loadModule(moduleName, modulePath) {
  if (moduleCache.has(moduleName)) {
    return moduleCache.get(moduleName);
  }
  
  try {
    const module = await import(modulePath);
    moduleCache.set(moduleName, module);
    return module;
  } catch (error) {
    console.error(`加载模块 ${moduleName} 失败:`, error);
    throw error;
  }
}
