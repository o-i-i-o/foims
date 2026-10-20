// 仅服务于 Jest：把原生 ESM 前端源码转换为 CommonJS 供测试运行时加载，
// 不参与任何构建产物（项目无构建，浏览器直接加载 ESM）
module.exports = {
  presets: [["@babel/preset-env", { targets: { node: "current" } }]]
};
