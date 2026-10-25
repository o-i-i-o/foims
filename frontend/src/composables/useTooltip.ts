/**
 * 全局轻量 Tooltip。
 *
 * 事件委托为所有带 data-tooltip 属性的元素显示悬浮提示，浮动层挂在
 * body 上（fixed 定位）避免被容器 overflow 裁剪，同时支持键盘焦点触达。
 * 侧边栏展开时文字均可见、悬浮提示冗余，仅收起后的图标栏与始终无文字的
 * 图标按钮（收起/展开按钮）保留提示。
 *
 * 可视化画布（工位/机柜/拓扑 SVG）自带跟随鼠标的浮窗，迁移可视化分区时
 * 在 isVisualizationTarget 中恢复让位判断。
 */
let tooltipEl: HTMLDivElement | null = null;
let currentTarget: HTMLElement | null = null;
let started = false;

function getTooltipElement(): HTMLDivElement {
  if (!tooltipEl) {
    tooltipEl = document.createElement("div");
    tooltipEl.className = "global-tooltip";
    tooltipEl.setAttribute("role", "tooltip");
    document.body.appendChild(tooltipEl);
  }
  return tooltipEl;
}

function showTooltip(target: HTMLElement): void {
  const text = target.dataset.tooltip;
  if (!text) {
    return;
  }

  if (target.closest(".sidebar") && !target.closest(".sidebar-toggle") && !document.body.classList.contains("sidebar-collapsed")) {
    return;
  }

  const el = getTooltipElement();
  el.textContent = text;
  el.classList.add("visible");

  const rect = target.getBoundingClientRect();
  const tooltipRect = el.getBoundingClientRect();

  let top = rect.top - tooltipRect.height - 6;
  let left = rect.left + rect.width / 2 - tooltipRect.width / 2;
  left = Math.max(8, Math.min(left, window.innerWidth - tooltipRect.width - 8));
  if (top < 8) {
    top = rect.bottom + 6;
  }

  el.style.top = `${top}px`;
  el.style.left = `${left}px`;
}

function hideTooltip(): void {
  currentTarget = null;
  tooltipEl?.classList.remove("visible");
}

function onMouseOver(e: MouseEvent): void {
  const target = e.target instanceof Element ? e.target.closest<HTMLElement>("[data-tooltip]") : null;
  if (target) {
    if (target !== currentTarget) {
      currentTarget = target;
      showTooltip(target);
    }
  } else if (currentTarget) {
    hideTooltip();
  }
}

function onFocusIn(e: FocusEvent): void {
  const target = e.target instanceof Element ? e.target.closest<HTMLElement>("[data-tooltip]") : null;
  if (target) {
    currentTarget = target;
    showTooltip(target);
  }
}

/** 挂载全局委托（幂等，MainShell 装载时调用一次） */
export function initTooltip(): void {
  if (started) {
    return;
  }
  started = true;

  document.addEventListener("mouseover", onMouseOver);
  document.addEventListener("focusin", onFocusIn);
  document.addEventListener("focusout", hideTooltip);

  // 滚动后位置会失效，直接隐藏
  window.addEventListener(
    "scroll",
    () => {
      if (currentTarget) {
        hideTooltip();
      }
    },
    { passive: true, capture: true }
  );
}
