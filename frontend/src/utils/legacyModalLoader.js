/**
 * 旧版 modalLoader 裁剪迁移版，仅服务 visualization 画布弹窗生态；机制与旧版一致。
 *
 * 函数体从旧版 web/static/js/utils/modalLoader.js 逐字复制，仅两处改造：
 * 1. MODAL_REGISTRY 裁剪为 visualization 画布所需 5 个条目（confirm-modal 与
 *    topology 4 弹窗），其余功能模块条目删除；
 * 2. 模板获取从按 path fetch 改为构建期内联：HTML 原样迁入
 *    frontend/src/templates/modals/，经 vite ?raw 注入 html 字段，
 *    fetchModalHtml 直接读字段（去掉网络请求、版本化 URL 与并发加载去重，
 *    async 签名与返回值语义不变）。
 *
 * 另随裁剪迁入两段旧版配套逻辑：
 * - 模态框关闭按钮的 document 委托（旧版在 eventManager.js，topology 弹窗
 *   的关闭按钮依赖它触发 closeModal）；
 * - icons.js 的帮助按钮与 data-icon 图标填充按需内联（弹窗外壳所需）。
 */
import { t, showToast, updatePageTranslations } from "./legacyBridge.js";
import confirmModalHtml from "../templates/modals/confirm-modal.html?raw";
import topologyConnectionModalHtml from "../templates/modals/topology-connection-modal.html?raw";
import topologyConnectionDetailModalHtml from "../templates/modals/topology-connection-detail-modal.html?raw";
import topologyContainerModalHtml from "../templates/modals/topology-container-modal.html?raw";
import topologyDetailModalHtml from "../templates/modals/topology-detail-modal.html?raw";

const loadedModals = new Set();
const htmlCache = new Map();

// 层叠模态框计数：设备模态框上再开端口详情等场景，仅当全部关闭时恢复页面滚动
let openModalCount = 0;

// 模态框层级规则（唯一层级来源，CSS 侧不再按 DOM 相邻性判定）：
// 按打开顺序显式设置顶层元素内联 z-index —— 基准 1050（与 --z-modal 一致），
// 每多打开一层 +50（第 1 层 1050、第 2 层 1100、第 3 层 1150……），
// 保证多开时后打开的模态框一定覆盖先打开的
const MODAL_Z_INDEX_BASE = 1050;
const MODAL_Z_INDEX_STEP = 50;
// 层级封顶：超过后与顶层同 z（DOM 顺序后者居上），确保任何深度模态
// 内的 data-tooltip（--z-tooltip 3000）都不被模态压住
const MODAL_Z_INDEX_MAX = 2900;

// 模态框打开时的触发元素：关闭时据此归还焦点（键值随模态元素移除而释放）
const modalTriggers = new WeakMap();

// 可聚焦元素选择器：焦点陷阱与首焦点定位共用
const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  'input:not([disabled]):not([type="hidden"])',
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])'
].join(", ");

// ==========================================
// 旧版 icons.js 按需内联（仅弹窗外壳所需：help 图标与 data-icon 填充）
// ==========================================

const svg = (content) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${content}</svg>`;

const ICONS = {
  // 帮助中心（圆圈问号）
  help: svg(
    '<circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><line x1="12" y1="17" x2="12.01" y2="17"/>'
  )
};

/**
 * 获取指定名称的 SVG 图标字符串
 * @param {string} name 图标名称（ICONS 键）
 * @returns {string} SVG HTML
 */
function getIcon(name) {
  return ICONS[name] || "";
}

/**
 * 生成帮助中心「?」按钮 HTML（卡片标题/弹窗标题旁，见旧版 helpManager.js）
 * aria-label 与 tooltip 的翻译由 data-i18n-* 属性在注入后统一填充
 * @param {string} helpKey 帮助中心目标键（i18n help 命名空间）
 * @param {string} titleKey 帮助弹窗标题的 i18n 键
 * @returns {string} 按钮 HTML
 */
function helpButton(helpKey, titleKey) {
  return (
    `<button type="button" class="help-btn" data-help="${helpKey}"` +
    ` data-help-title="${titleKey}" data-icon="help"` +
    ' data-i18n-aria-label="common.help" aria-label="帮助"' +
    ' data-i18n-tooltip="common.help" data-tooltip="帮助"></button>'
  );
}

/**
 * 收集模态框内当前可见的可聚焦元素（display:none 的隐藏步骤/面板会被 offsetParent 过滤）
 * @param {HTMLElement} modal 模态框根元素
 * @returns {HTMLElement[]}
 */
function getModalFocusable(modal) {
  return Array.from(modal.querySelectorAll(FOCUSABLE_SELECTOR)).filter(
    (el) => el.offsetParent !== null
  );
}

/**
 * Tab 焦点陷阱：键盘焦点限制在当前模态框内首尾循环；
 * 焦点落在模态框外（如点击遮罩后）时 Tab 拉回框内
 * @param {KeyboardEvent} e
 */
function trapModalFocus(e) {
  if (e.key !== "Tab") {
    return;
  }
  const modal = e.currentTarget;
  const focusable = getModalFocusable(modal);
  if (focusable.length === 0) {
    e.preventDefault();
    return;
  }
  const first = focusable[0];
  const last = focusable[focusable.length - 1];
  const active = document.activeElement;
  if (!modal.contains(active)) {
    e.preventDefault();
    (e.shiftKey ? last : first).focus();
  } else if (e.shiftKey && active === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && active === last) {
    e.preventDefault();
    first.focus();
  }
}

// 模态框清单：HTML 模板经 ?raw 构建期内联（见文件头说明）。
// 片段分两类：
// - 标准片段（带 title/titleHtml 字段）：文件只含 .modal-body（及可选 .modal-footer），
//   外壳与 header（标题/关闭按钮）由 buildModalShell 统一生成；
//   title 为 i18n key，titleHtml 为复合标题（i18n span + 动态 span），
//   "" 表示空标题（由 openModal(id, title) 填充）；
// - 自包含片段（仅 html）：自带完整外壳（confirm 自定义 close 语义、
//   topology-detail 独立 header 结构），原样注入。
const MODAL_REGISTRY = {
  // 公共确认框（自包含片段，close 语义由 confirm.js 接管）
  "confirm-modal": { html: confirmModalHtml },
  // 可视化
  "topology-connection-modal": {
    html: topologyConnectionModalHtml,
    title: "viz.create_connection_title",
    help: "modal.topology_connection"
  },
  "topology-connection-detail-modal": {
    html: topologyConnectionDetailModalHtml,
    title: "viz.connection_detail"
  },
  "topology-container-modal": {
    html: topologyContainerModalHtml,
    title: "viz.container_coords",
    help: "modal.topology_container"
  },
  "topology-detail-modal": { html: topologyDetailModalHtml }
};

/**
 * 标准片段外壳：header（标题 + 关闭按钮）全站唯一实现，
 * 片段只提供 .modal-body 与可选 .modal-footer。
 * 标题元素保留 {id}-title 供 aria-labelledby 与 JS 按需写入；
 * data-i18n 标题由 loadModal 注入后的 updatePageTranslations 即时填充。
 * entry.help 为帮助中心目标键（help 命名空间），存在时标题后追加 ? 按钮；
 * helpTitle 为按钮弹窗标题的 i18n 键，缺省复用 entry.title。
 */
function buildModalShell(id, entry, innerHtml) {
  const titleI18nAttr = entry.titleHtml || !entry.title ? "" : ` data-i18n="${entry.title}"`;
  const titleContent = entry.titleHtml || "";
  const contentClass = entry.contentClass ? ` ${entry.contentClass}` : "";
  const helpBtn = entry.help ? helpButton(entry.help, entry.helpTitle || entry.title || "") : "";

  return `<div id="${id}" class="modal" role="dialog" aria-modal="true" aria-labelledby="${id}-title">
  <div class="modal-content${contentClass}">
    <header class="modal-header">
      <h3 class="modal-title" id="${id}-title"${titleI18nAttr}>${titleContent}</h3>
      ${helpBtn}<button type="button" class="close" data-modal-id="${id}" aria-label="Close">&times;</button>
    </header>
${innerHtml}
  </div>
</div>`;
}

export async function fetchModalHtml(modalId) {
  // 模板构建期内联：无网络请求与版本化 URL，缓存键直接用 modalId
  if (htmlCache.has(modalId)) {
    return htmlCache.get(modalId);
  }

  const entry = MODAL_REGISTRY[modalId];
  const html = entry?.html;
  if (!html) {
    return null;
  }

  const match = html.match(/<template[^>]*>([\s\S]*?)<\/template>/);
  const innerHtml = match ? match[1].trim() : html.trim();

  // 标准片段（注册表带 title/titleHtml）由外壳函数补全 header；
  // 自包含片段原样注入
  const result =
    entry && ("title" in entry || "titleHtml" in entry)
      ? buildModalShell(modalId, entry, innerHtml)
      : innerHtml;

  htmlCache.set(modalId, result);
  return result;
}

export async function loadModal(id) {
  if (loadedModals.has(id)) {
    const existing = document.getElementById(id);
    if (existing) {
      return existing;
    }
    loadedModals.delete(id);
  }
  const innerHtml = await fetchModalHtml(id);
  if (!innerHtml) {
    showToast(t("common.load_failed"), "error");
    return null;
  }

  // await 期间可能已被并发调用装载完成，避免重复解析追加
  const concurrent = document.getElementById(id);
  if (concurrent) {
    return concurrent;
  }

  const container = document.createElement("div");
  container.innerHTML = innerHtml;
  const modal = container.firstElementChild;

  if (!modal) {
    return null;
  }

  document.body.appendChild(modal);
  loadedModals.add(id);

  // 静态模板中的图标按钮以 data-icon 声明图标名，注入时统一填充 SVG
  modal.querySelectorAll("button[data-icon]").forEach((btn) => {
    if (!btn.querySelector("svg")) {
      btn.insertAdjacentHTML("afterbegin", getIcon(btn.dataset.icon || ""));
    }
  });

  // 仅扫描模态框子树，避免每次开框对全文档跑多轮翻译查询
  updatePageTranslations(modal);

  return modal;
}

export async function openModal(id, title = "") {
  // await 之前捕获触发元素：模板加载期间焦点仍在调用按钮上，关闭时据此归还
  const trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;

  let modal = document.getElementById(id);

  if (!modal) {
    modal = await loadModal(id);
  }

  if (!modal) {
    return;
  }

  // 激活状态判定放在（可能的）await 之后：并发首开时两个调用方共享
  // loadModal 的同一 Promise，先恢复者完成激活，后恢复者据此跳过计数，
  // 否则 openModalCount 虚高、closeModal 归不了零，body.overflow 永久锁死
  const alreadyActive = modal.classList.contains("active");

  modal.classList.add("active");

  if (!alreadyActive) {
    openModalCount++;
    // 按当前打开层数显式指定层级：后打开的一定在上
    modal.style.zIndex = String(
      Math.min(MODAL_Z_INDEX_BASE + (openModalCount - 1) * MODAL_Z_INDEX_STEP, MODAL_Z_INDEX_MAX)
    );

    // ARIA 语义与焦点管理：自包含片段可能缺 role，打开时统一补齐；
    // 焦点移入框内并启用 Tab 陷阱，关闭时归还触发元素（见 closeModal）
    modal.setAttribute("role", "dialog");
    modal.setAttribute("aria-modal", "true");
    modalTriggers.set(modal, trigger);
    modal.addEventListener("keydown", trapModalFocus);

    const focusable = getModalFocusable(modal);
    focusable[0]?.focus();
  }

  if (title) {
    const titleElement = modal.querySelector(".modal-title");
    if (titleElement) {
      titleElement.textContent = title;
    }
  }

  document.body.style.overflow = "hidden";

  return modal;
}

export function closeModal(id) {
  const modal = document.getElementById(id);

  if (!modal) {
    return;
  }

  const wasActive = modal.classList.contains("active");

  modal.classList.remove("active");

  // 重置全部表单（多表单模态框只重置第一个会残留旧输入，
  // 含 hidden id，再次打开可能误走更新分支）
  modal.querySelectorAll("form").forEach((form) => {
    form.reset();
    const hiddenIdField = form.querySelector('input[type="hidden"]');
    if (hiddenIdField) {
      hiddenIdField.value = "";
    }
  });

  const ipContainers = modal.querySelectorAll('[id$="-ips-container"]');
  ipContainers.forEach((container) => {
    container.innerHTML = "";
  });

  if (wasActive) {
    openModalCount = Math.max(0, openModalCount - 1);
    // 恢复（清除）内联层级：下次打开按新的打开顺序重新计算
    modal.style.zIndex = "";
  }
  if (openModalCount === 0) {
    document.body.style.overflow = "";
  }

  // 焦点归还触发元素：仅当焦点当前仍在本模态框内（关闭底层模态时不抢夺上层焦点）
  if (modal.contains(document.activeElement)) {
    const trigger = modalTriggers.get(modal);
    if (trigger instanceof HTMLElement && document.contains(trigger)) {
      trigger.focus();
    }
  }
  modalTriggers.delete(modal);

  modal.remove();
  loadedModals.delete(id);
}

// 模态框关闭按钮的全局委托：旧版由 eventManager.js 统一注册，
// topology 弹窗的关闭按钮（data-modal-id 指向所属模态框）依赖它触发 closeModal。
// 注意 confirm-modal 的 #confirm-close 不带 data-modal-id，不经过此委托
// （其 Promise 结束语义由 legacyBridge 的 showConfirm 接管）。
document.addEventListener("click", (e) => {
  if (!e.target.hasAttribute("data-modal-id")) {
    return;
  }

  closeModal(e.target.getAttribute("data-modal-id"));
});
