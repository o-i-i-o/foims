/**
 * FOIMS 系统初始化 - 共享状态
 *
 * 将跨模块共享的可变状态集中在此，避免 ui.js / api.js / index.js 之间
 * 产生循环依赖。各模块通过 import 读取/修改同一份状态。
 */
export const state = {
  // 当前所在步骤
  //（1: PostgreSQL 检查 / 2: 数据库配置 / 3: 数据库初始化 / 4: 管理员账户 / 5: 完成）
  currentStep: 1,
  // 数据库初始化方式：'create' 新建 | 'import' 导入
  initMode: "create"
};
