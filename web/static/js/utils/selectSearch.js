/**
 * 全局下拉框快捷筛选（selectSearch）。
 *
 * 为页面中全部原生 <select> 提供"首项搜索输入框"能力：
 * - 单选：拦截原生气泡，弹出「筛选输入框 + 过滤选项列表」的自定义面板，
 *   选中项仍写回 <select> 并派发 change 事件，业务代码零改动；
 * - 多选 / size>1（原生列表框）：在控件上方固定一枚筛选输入框，
 *   输入即时隐藏（option.hidden）不匹配的选项；
 * - 通过 MutationObserver 自动覆盖后插入的下拉（模态框/动态行）。
 *
 * 退出开关：select 添加 data-nosearch 属性；disabled 下拉自动跳过。
 */

import { t } from "./i18n.js";

/** 已增强的 select，防重复接入 */
const enhanced = new WeakSet();
/** 多选控件当前的筛选词（选项异步重载后重新应用） */
const inlineFilters = new WeakMap();

/** 自定义面板当前状态 */
let panel = null;
let panelSelect = null;
let panelHighlighted = -1;

// ==========================================
// 单选：自定义筛选面板
// ==========================================

function closePanel() {
  if (!panel) {
    return;
  }
  const el = panel;
  panel = null;
  panelSelect = null;
  panelHighlighted = -1;
  window.removeEventListener("scroll", onWindowScroll, true);
  window.removeEventListener("resize", closePanel);
  document.removeEventListener("mousedown", onDocumentMouseDown, true);
  el.remove();
}

function onDocumentMouseDown(e) {
  if (panel && !panel.contains(e.target) && e.target !== panelSelect) {
    closePanel();
  }
}

/** 页面滚动时关闭面板（fixed 定位脱离锚点）；面板自身选项列表的滚动不在此列 */
function onWindowScroll(e) {
  if (!panel || panel.contains(e.target)) {
    return;
  }
  closePanel();
}

/** 构建过滤后的选项列表（支持 optgroup 分组标题） */
function buildOptionList(select) {
  const keyword = (panel?.querySelector(".select-search-input")?.value || "").trim().toLowerCase();
  const fragment = document.createDocumentFragment();
  let visibleCount = 0;
  let currentGroup = null;
  let groupItem = null;

  const match = (option) =>
    !keyword ||
    option.textContent.toLowerCase().includes(keyword) ||
    (option.value || "").toLowerCase().includes(keyword);

  for (const option of select.options) {
    // optgroup：分组标题跟随组内首个可见项重新出现
    const group = option.parentElement.closest("optgroup");
    if (group !== currentGroup) {
      currentGroup = group;
      groupItem = null;
    }
    if (!match(option)) {
      continue;
    }
    if (group && !groupItem) {
      groupItem = document.createElement("li");
      groupItem.className = "select-search-group";
      groupItem.textContent = group.label || "";
      fragment.appendChild(groupItem);
    }
    const item = document.createElement("li");
    item.className = "select-search-option";
    item.setAttribute("role", "option");
    item.textContent = option.textContent;
    if (option.disabled) {
      item.classList.add("disabled");
    }
    if (option.selected) {
      item.classList.add("selected");
    }
    item.dataset.value = option.value;
    item.dataset.index = String(option.index);
    fragment.appendChild(item);
    visibleCount++;
  }

  return { fragment, visibleCount };
}

function renderPanelList() {
  const listEl = panel.querySelector(".select-search-options");
  const emptyEl = panel.querySelector(".select-search-empty");
  const { fragment, visibleCount } = buildOptionList(panelSelect);

  listEl.replaceChildren(fragment);
  panelHighlighted = -1;
  const options = listEl.querySelectorAll(".select-search-option");
  for (const [index, item] of options.entries()) {
    if (item.classList.contains("selected")) {
      panelHighlighted = index;
      break;
    }
  }
  // 显隐走 hidden 属性（UA 默认 display:none），不写内联 style
  emptyEl.hidden = visibleCount !== 0;
}

function moveHighlight(step) {
  const items = [...panel.querySelectorAll(".select-search-option:not(.disabled)")];
  if (items.length === 0) {
    return;
  }
  panelHighlighted = (panelHighlighted + step + items.length) % items.length;
  const target = items[panelHighlighted];
  for (const item of panel.querySelectorAll(".select-search-option")) {
    item.classList.toggle("active", item === target);
  }
  target.scrollIntoView({ block: "nearest" });
}

function pickHighlighted() {
  const items = [...panel.querySelectorAll(".select-search-option:not(.disabled)")];
  const index = panelHighlighted >= 0 ? panelHighlighted : 0;
  const target = items[index];
  if (target) {
    pickOption(target);
  }
}

function pickOption(item) {
  const select = panelSelect;
  const value = item.dataset.value;
  closePanel();
  if (select.value !== value) {
    select.value = value;
    select.dispatchEvent(new Event("input", { bubbles: true }));
    select.dispatchEvent(new Event("change", { bubbles: true }));
  }
}

function openPanel(select) {
  closePanel();

  const el = document.createElement("div");
  el.className = "select-search-panel";
  el.setAttribute("role", "listbox");
  el.innerHTML = `
    <input type="text" class="select-search-input" autocomplete="off"
      placeholder="${t("common.select_search_placeholder")}" />
    <ul class="select-search-options"></ul>
    <div class="select-search-empty" hidden>
      ${t("common.select_search_no_match")}
    </div>`;

  document.body.appendChild(el);

  // 定位：默认贴下拉下方，下方空间不足时贴上方弹出
  const rect = select.getBoundingClientRect();
  const width = Math.max(rect.width, 220);
  const below = window.innerHeight - rect.bottom;
  el.style.width = `${width}px`;
  if (below < 190 && rect.top > below) {
    el.style.left = `${rect.left}px`;
    el.style.bottom = `${window.innerHeight - rect.top + 4}px`;
  } else {
    el.style.left = `${rect.left}px`;
    el.style.top = `${rect.bottom + 4}px`;
  }

  panel = el;
  panelSelect = select;

  renderPanelList();

  const input = el.querySelector(".select-search-input");
  input.addEventListener("input", renderPanelList);
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      moveHighlight(1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      moveHighlight(-1);
    } else if (e.key === "Enter") {
      e.preventDefault();
      pickHighlighted();
    } else if (e.key === "Escape" || e.key === "Tab") {
      closePanel();
    }
  });

  el.querySelector(".select-search-options").addEventListener("mousedown", (e) => {
    const item = e.target.closest(".select-search-option");
    if (item && !item.classList.contains("disabled")) {
      e.preventDefault();
      pickOption(item);
    }
  });

  // 面板打开期间 select 选项被异步重建 → 刷新列表
  panelObserver.disconnect();
  panelObserver.observe(select, { childList: true, subtree: true });

  window.addEventListener("scroll", onWindowScroll, true);
  window.addEventListener("resize", closePanel);
  document.addEventListener("mousedown", onDocumentMouseDown, true);

  input.focus();
}

/** 面板打开时 select 选项集变化（级联加载）→ 关闭面板防脏数据 */
const panelObserver = new MutationObserver(() => {
  if (panelSelect) {
    closePanel();
  }
});

function onSelectMouseDown(e) {
  const select = e.currentTarget;
  if (panel && panelSelect === select) {
    // 二次按下同一控件：收起
    e.preventDefault();
    closePanel();
    return;
  }
  // 阻止原生弹层，改由自定义筛选面板承接
  e.preventDefault();
  if (panel) {
    closePanel();
  }
  openPanel(select);
}

function onSelectKeyDown(e) {
  if (e.altKey || e.ctrlKey || e.metaKey) {
    return;
  }
  const openKeys = ["Enter", " ", "ArrowDown", "ArrowUp"];
  if (openKeys.includes(e.key)) {
    e.preventDefault();
    openPanel(e.currentTarget);
    return;
  }
  // 可打印字符直接进入筛选（模拟原生 type-ahead 的输入式检索）
  if (e.key.length === 1) {
    e.preventDefault();
    openPanel(e.currentTarget);
    const input = panel?.querySelector(".select-search-input");
    if (input) {
      input.value = e.key;
      renderPanelList();
    }
  }
}

// ==========================================
// 多选 / 列表框：上方固定筛选输入框
// ==========================================

function applyInlineFilter(select) {
  const keyword = (inlineFilters.get(select) || "").trim().toLowerCase();
  for (const option of select.options) {
    option.hidden =
      Boolean(keyword) &&
      !option.textContent.toLowerCase().includes(keyword) &&
      !(option.value || "").toLowerCase().includes(keyword);
  }
}

function enhanceInline(select) {
  const input = document.createElement("input");
  input.type = "text";
  input.className = "select-search-inline";
  input.setAttribute("autocomplete", "off");
  input.setAttribute("aria-label", t("common.select_search_placeholder"));
  input.placeholder = t("common.select_search_placeholder");

  input.addEventListener("input", () => {
    inlineFilters.set(select, input.value);
    applyInlineFilter(select);
  });

  // 输入框位于表单内时，Enter 会触发隐式提交（如拓扑连线模态），必须拦截
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
    }
  });

  select.before(input);

  // 选项异步重载后按当前关键字重新过滤；控件移除时同步清理
  const observer = new MutationObserver(() => {
    if (!select.isConnected) {
      observer.disconnect();
      input.remove();
      return;
    }
    applyInlineFilter(select);
  });
  observer.observe(select, { childList: true });
}

// ==========================================
// 接入
// ==========================================

function enhanceSelect(select) {
  if (enhanced.has(select) || select.disabled || select.dataset.nosearch !== undefined) {
    return;
  }
  enhanced.add(select);

  if (select.multiple || select.size > 1) {
    enhanceInline(select);
  } else {
    select.addEventListener("mousedown", onSelectMouseDown);
    select.addEventListener("keydown", onSelectKeyDown);
  }
}

function enhanceAll(root) {
  for (const select of root.querySelectorAll("select")) {
    enhanceSelect(select);
  }
}

const observer = new MutationObserver((mutations) => {
  for (const mutation of mutations) {
    for (const node of mutation.addedNodes) {
      if (node.nodeType !== Node.ELEMENT_NODE) {
        continue;
      }
      if (node.tagName === "SELECT") {
        enhanceSelect(node);
      } else {
        enhanceAll(node);
      }
    }
  }
});

/**
 * 初始化全局下拉筛选：增强现有下拉并监听后续插入
 */
export function initSelectSearch() {
  enhanceAll(document);
  observer.observe(document.body, { childList: true, subtree: true });
}
