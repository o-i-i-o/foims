/**
 * IPMA - 内联 SVG 图标库（Feather 风格）
 *
 * 所有图标 24x24 viewBox、stroke=currentColor，
 * 颜色随按钮 CSS 状态自动适配，无外部依赖（离线可用）。
 */
import { escapeHtml } from "./helpers.js";

const svg = (content) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${content}</svg>`;

// 纵向加高的组合图标视窗（字母区在上、图标区在下，互不遮挡）
const svgTall = (content, cls) =>
  `<svg class="${cls}" viewBox="0 0 24 32" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${content}</svg>`;

export const ICONS = {
  // 编辑（铅笔）
  edit: svg('<path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z"/>'),
  // 删除（垃圾桶）
  trash: svg(
    '<polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/>'
  ),
  // 使用情况（柱状图）
  chart: svg(
    '<line x1="18" y1="20" x2="18" y2="10"/><line x1="12" y1="20" x2="12" y2="4"/><line x1="6" y1="20" x2="6" y2="14"/>'
  ),
  // 机柜/工位（服务器）
  server: svg(
    '<rect x="2" y="2" width="20" height="8" rx="2" ry="2"/><rect x="2" y="14" width="20" height="8" rx="2" ry="2"/><line x1="6" y1="6" x2="6.01" y2="6"/><line x1="6" y1="18" x2="6.01" y2="18"/>'
  ),
  // 网络出口/信息点（节点连接）
  share: svg(
    '<circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/>'
  ),
  // 机位列表（网格）
  grid: svg(
    '<rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/>'
  ),
  // 设备接口（链接）
  link: svg(
    '<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>'
  ),
  // MAC 表（列表）
  list: svg(
    '<line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>'
  ),
  // 设备接口（列表 + 字母角标单体组合，角标随图标整体居中）
  listP: listBadgeIcon("P"),
  listM: listBadgeIcon("M"),
  listL: listBadgeIcon("L"),
  // LLDP 邻居（信号发现）
  radio: svg(
    '<circle cx="12" cy="12" r="2"/><path d="M16.24 7.76a6 6 0 0 1 0 8.49m-8.48-.01a6 6 0 0 1 0-8.49m11.31-2.82a10 10 0 0 1 0 14.14m-14.14 0a10 10 0 0 1 0-14.14"/>'
  ),
  // 双因素认证（盾牌）
  shield: svg('<path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z"/>'),
  // 立即运行（播放）
  play: svg('<polygon points="5 3 19 12 5 21 5 3"/>'),
  // 启用/停用（电源）
  power: svg('<path d="M18.36 6.64a9 9 0 1 1-12.73 0"/><line x1="12" y1="2" x2="12" y2="12"/>'),
  // 日志（文档）
  fileText: svg(
    '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/>'
  ),
  // 详情（眼睛）
  eye: svg(
    '<path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>'
  ),
  // 标记已读/保存（对勾）
  check: svg('<polyline points="20 6 9 17 4 12"/>'),
  // 双因素认证（锁）
  lock: svg(
    '<rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>'
  ),
  // 解封（开锁）
  unlock: svg(
    '<rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 9.9-1"/>'
  ),
  // 添加子节点（圆圈加号）
  plusCircle: svg(
    '<circle cx="12" cy="12" r="10"/><line x1="12" y1="8" x2="12" y2="16"/><line x1="8" y1="12" x2="16" y2="12"/>'
  ),
  // 人员（多人）
  users: svg(
    '<path d="M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2"/><circle cx="9" cy="7" r="4"/><path d="M23 21v-2a4 4 0 0 0-3-3.87"/><path d="M16 3.13a4 4 0 0 1 0 7.75"/>'
  ),
  // 取消（叉号）
  x: svg('<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>'),
  // 打印（打印机）
  printer: svg(
    '<polyline points="6 9 6 2 18 2 18 9"/><path d="M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2"/><rect x="6" y="14" width="12" height="8"/>'
  )
};

/**
 * 获取指定名称的 SVG 图标字符串
 * @param {string} name 图标名称（ICONS 键）
 * @returns {string} SVG HTML
 */
export function getIcon(name) {
  return ICONS[name] || "";
}

/**
 * 生成"顶部字母 + 列表"的单体组合图标（24x32 纵向视窗）
 * 上部：白底圆内 currentColor 字母；下部：完整 list 线条，两者互不遮挡，
 * 配合 CSS 的 .combo-icon 尺寸可同时清晰显示（接近 svg+css 分层的效果）
 * @param {string} letter 角标字母（ASCII）
 * @returns {string} SVG HTML
 */
function listBadgeIcon(letter) {
  return svgTall(
    '<circle cx="12" cy="6" r="6" fill="var(--color-bg-white)" stroke="none"/>' +
      `<text x="12" y="8.9" text-anchor="middle" font-size="9" font-weight="700" font-family="inherit" fill="currentColor" stroke="none">${letter}</text>` +
      '<line x1="8" y1="18" x2="21" y2="18"/><line x1="8" y1="24" x2="21" y2="24"/><line x1="8" y1="30" x2="21" y2="30"/><line x1="3" y1="18" x2="3.01" y2="18"/><line x1="3" y1="24" x2="3.01" y2="24"/><line x1="3" y1="30" x2="3.01" y2="30"/>',
    "combo-icon"
  );
}

/**
 * 生成图标按钮 HTML
 * 悬浮提示由全局 tooltip 组件读取 data-tooltip 渲染
 * @param {Object} options
 * @param {string} options.icon 图标名称
 * @param {string} options.label 按钮文字（用于提示与 aria-label，调用方保证已本地化）
 * @param {string} [options.cls] 附加的功能类名（如 btn-edit，供事件委托识别）
 * @param {string} [options.attrs] 附加的 HTML 属性字符串（如 data-id="..."）
 * @param {string} [options.badge] 左上角字母角标（如 'P'，用于同图标按钮的快捷区分）
 * @returns {string} 按钮 HTML
 */
export function iconButton({ icon, label, cls = "", attrs = "", badge = "" }) {
  const safeLabel = escapeHtml(label);
  const clsPart = cls ? ` ${cls}` : "";
  const attrsPart = attrs ? ` ${attrs}` : "";
  const badgeHtml = badge
    ? `<span class="icon-badge" aria-hidden="true">${escapeHtml(badge)}</span>`
    : "";
  return `<button type="button" class="icon-btn${clsPart}" data-tooltip="${safeLabel}" aria-label="${safeLabel}"${attrsPart}>${getIcon(icon)}${badgeHtml}</button>`;
}
