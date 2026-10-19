/**
 * 角色能力助手（Vue 版，移植自旧版 web/static/js/utils/roleCapabilities.js）：
 * 三权分立 + 超管的界面可见性 / 只读模式集中定义。后端始终强制校验（403），
 * 此处仅为界面整洁（隐藏无权限入口与写控件）。角色与权限矩阵：
 * - admin    超级管理员：全部可见、全部可写
 * - sysadmin 系统管理员：系统功能可写；资源类分区只读；日志/主机监控不可见（后端拒绝）
 * - secadmin 安全管理员：用户/安全设置可写；组织/资源/可视化不可见；IP 只读
 * - auditor  审计管理员：全站可见但只读（含系统模块）；主机监控不可见
 * - user     普通用户：资源类分区可写；系统模块只读；日志/主机监控不可见（后端拒绝）
 *
 * 与旧版差异：只读分区的写控件隐藏改由各分区页面声明式实现
 * （v-if / readonly 判断），不再做 DOM 扫描与 MutationObserver 兜底。
 */

import { getUser } from "./session.js";

/** 各角色不可见的分区（后端同样拒绝对应数据，避免页面报错） */
export const ROLE_HIDDEN_SECTIONS = {
  admin: [],
  sysadmin: ["logs", "agents"],
  secadmin: ["organization", "resources", "visualization"],
  auditor: ["agents"],
  user: ["logs", "agents"]
};

/** 各角色以只读模式展示的分区（页面据此隐藏写操作控件） */
export const ROLE_READONLY_SECTIONS = {
  admin: [],
  sysadmin: ["resources", "organization", "ip", "visualization"],
  secadmin: ["ip"],
  auditor: ["resources", "organization", "ip", "visualization", "system"],
  user: ["system"]
};

/** 当前登录角色（未登录回退为最低权限 user） */
export function getRole() {
  return getUser()?.role || "user";
}

/** 当前角色不可见的分区列表 */
export function getHiddenSections(role = getRole()) {
  return ROLE_HIDDEN_SECTIONS[role] ?? [];
}

/** 当前角色以只读模式展示的分区列表 */
export function getReadOnlySections(role = getRole()) {
  return ROLE_READONLY_SECTIONS[role] ?? [];
}

/** 指定分区对当前角色是否只读 */
export function isSectionReadonly(section, role = getRole()) {
  return getReadOnlySections(role).includes(section);
}
