/**
 * IPMA 系统初始化 - 共享状态
 *
 * 将跨模块共享的可变状态集中在此，避免 ui.js / api.js / index.js 之间
 * 产生循环依赖。各模块通过 import 读取/修改同一份状态。
 */
export const state = {
    // 当前所在步骤（1: PostgreSQL 检查 / 2: 数据库初始化 / 3: 管理员账户 / 4: 完成）
    currentStep: 1,
    // 数据库状态检查结果（由 checkDatabaseStatus 写入）
    dbStatus: null,
    // 数据库初始化方式：'create' 新建 | 'import' 导入
    initMode: 'create',
};
