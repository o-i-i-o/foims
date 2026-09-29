/**
 * 角色能力助手：三权分立 + 超管的界面可见性 / 只读模式集中定义。
 *
 * 后端始终强制校验（403），此处仅为界面整洁（隐藏无权限入口与写控件）。
 * 角色与权限矩阵：
 * - admin    超级管理员：全部可见、全部可写
 * - sysadmin 系统管理员：系统功能可写；资源类分区只读；日志不可见（后端拒绝）
 * - secadmin 安全管理员：用户/安全设置可写；组织/资源/可视化不可见；IP 只读
 * - auditor  审计管理员：全站可见但只读（含系统模块）
 * - user     普通用户：资源类分区可写；系统模块只读；日志不可见（后端拒绝）
 */

import { SessionManager } from "./sessionManager.js";
import { t } from "./i18n.js";

/** 各角色不可见的分区（后端同样拒绝对应数据，避免页面报错） */
const ROLE_HIDDEN_SECTIONS = {
  admin: [],
  sysadmin: ["logs"],
  secadmin: ["organization", "resources", "visualization"],
  auditor: [],
  user: ["logs"]
};

/** 各角色以只读模式展示的分区（隐藏写操作控件） */
const ROLE_READONLY_SECTIONS = {
  admin: [],
  sysadmin: ["resources", "organization", "ip", "visualization"],
  secadmin: ["ip"],
  auditor: ["resources", "organization", "ip", "visualization", "system"],
  user: ["system"]
};

/**
 * 只读分区内需要隐藏的"写控件"选择器。
 * 仅覆盖静态工具栏与常见行内操作（增删改/导入导出/生成/同步等），
 * 未覆盖到的写操作由后端 403 兜底。
 */
const WRITE_CONTROL_SELECTOR = [
  // 工具栏按钮（静态 HTML，按 id 语义匹配）
  'button[id^="add-"]',
  'button[id^="create-"]',
  'button[id^="save-"]',
  'button[id^="delete-"]',
  'button[id^="import-"]',
  'button[id^="export-"]',
  'button[id^="generate-"]',
  'button[id^="sync-"]',
  'button[id^="open-pull-"]',
  'button[id$="-save-btn"]',
  'button[id="ca-generate-btn"]',
  'button[id="ca-import-btn"]',
  'button[id="cert-generate-btn"]',
  'button[id="cert-import-btn"]',
  'button[id="download-template-btn"]',
  'button[id="clear-logs-btn"]',
  'button[id="test-ldap-btn"]',
  'button[id="test-smtp-btn"]',
  'button[id="test-sso-btn"]',
  'button[id="log-forwarding-test-btn"]',
  'button[id="org-template-mgmt-btn"]',
  'button[id="toggle-connection-mode"]',
  'button[id="app-fail2ban-ban-btn"]',
  'button[id="app-fail2ban-unban-btn"]',
  // 行内操作（模块 JS 渲染）
  ".btn-edit",
  ".btn-delete",
  ".btn-danger",
  ".user-2fa",
  ".app-unban-btn",
  '[data-action^="edit"]',
  '[data-action^="delete"]',
  '[data-action="run-task"]',
  '[data-action="toggle-task"]'
].join(", ");

/** 只读控件的隐藏样式类（见 helpers.css） */
const RO_HIDDEN_CLASS = "ro-hidden";

let sweepScheduled = false;

/**
 * 获取当前登录角色（未登录回退为最低权限 user）
 * @returns {string} 角色值
 */
export function getRole() {
  return SessionManager.getUser()?.role || "user";
}

/**
 * 当前角色不可见的分区列表
 * @returns {string[]}
 */
export function getHiddenSections() {
  return ROLE_HIDDEN_SECTIONS[getRole()] || [];
}

/**
 * 当前角色以只读模式展示的分区列表
 * @returns {string[]}
 */
export function getReadOnlySections() {
  return ROLE_READONLY_SECTIONS[getRole()] || [];
}

/**
 * 隐藏只读分区内已渲染的写控件
 */
function sweepWriteControls() {
  for (const sectionId of getReadOnlySections()) {
    const section = document.getElementById(sectionId);
    if (!section) {
      continue;
    }
    for (const el of section.querySelectorAll(WRITE_CONTROL_SELECTOR)) {
      el.classList.add(RO_HIDDEN_CLASS);
    }
  }
}

/**
 * rAF 合并触发的兜底清扫：分区数据多为异步渲染，
 * 以 body 子树的 DOM 变更为信号重复执行隐藏
 */
function scheduleSweep() {
  if (sweepScheduled) {
    return;
  }
  sweepScheduled = true;
  requestAnimationFrame(() => {
    sweepScheduled = false;
    sweepWriteControls();
  });
}

/**
 * 应用角色界面模式：
 * 1) 分区元素补充 readonly-section 标记类（供样式与查询）；
 * 2) 启动 DOM 观察器兜底隐藏异步渲染的写控件。
 * 分区菜单的隐藏（hidden class + hash 重定向）由 navigation.applyRoleVisibility 负责。
 */
export function applyRoleUIMode() {
  const badge = t("common.readonly_badge");
  for (const sectionId of getReadOnlySections()) {
    const section = document.getElementById(sectionId);
    if (!section) {
      continue;
    }
    section.classList.add("readonly-section");
    // 分区标题追加只读徽标（文案见 helpers.css 的 ::after 规则）；
    // 每次重赋值以随语言切换刷新文案
    for (const heading of section.querySelectorAll("h2")) {
      heading.dataset.roBadge = badge;
    }
  }

  sweepWriteControls();
  startSweepObserver();
}

// DOM 观察器只启动一次（languagechange 会重跑 applyRoleUIMode）
let sweepObserverStarted = false;

function startSweepObserver() {
  if (sweepObserverStarted) {
    return;
  }
  sweepObserverStarted = true;
  const observer = new MutationObserver(scheduleSweep);
  observer.observe(document.body, { childList: true, subtree: true });
}

// 只读徽标文案随语言切换刷新（badge 值由 t() 按当前语言重新计算）
window.addEventListener("languagechange", applyRoleUIMode);
