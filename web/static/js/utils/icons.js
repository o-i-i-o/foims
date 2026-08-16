/**
 * IPMA - 内联 SVG 图标库（Feather 风格）
 *
 * 所有图标 24x24 viewBox、stroke=currentColor，
 * 颜色随按钮 CSS 状态自动适配，无外部依赖（离线可用）。
 */
import { escapeHtml } from "./helpers.js";

const svg = (content) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${content}</svg>`;

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
  // 取消（叉号）
  x: svg('<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>')
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
 * 生成图标按钮 HTML
 * 悬浮提示由全局 tooltip 组件读取 data-tooltip 渲染
 * @param {Object} options
 * @param {string} options.icon 图标名称
 * @param {string} options.label 按钮文字（用于提示与 aria-label，调用方保证已本地化）
 * @param {string} [options.cls] 附加的功能类名（如 btn-edit，供事件委托识别）
 * @param {string} [options.attrs] 附加的 HTML 属性字符串（如 data-id="..."）
 * @returns {string} 按钮 HTML
 */
export function iconButton({ icon, label, cls = "", attrs = "" }) {
  const safeLabel = escapeHtml(label);
  return `<button type="button" class="icon-btn${cls ? ` ${cls}` : ""}" data-tooltip="${safeLabel}" aria-label="${safeLabel}"${attrs ? ` ${attrs}` : ""}>${getIcon(icon)}</button>`;
}
