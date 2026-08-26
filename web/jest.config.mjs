// Jest 配置：源码为原生 ESM，经 babel 按当前 Node 目标转 CommonJS 运行（无需实验 flag）
/** @type {import('jest').Config} */
export default {
  testEnvironment: "jsdom",
  testMatch: ["<rootDir>/tests/**/*.test.js"],
  transform: {
    "^.+\\.js$": ["babel-jest", { configFile: "./babel.config.cjs" }]
  },
  // 前端模块以浏览器全局（document/localStorage/navigator）为准，jsdom 已提供
  clearMocks: true
};
