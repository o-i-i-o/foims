/**
 * IPMA - 全局轻量 Tooltip 组件
 *
 * 通过事件委托为所有带 data-tooltip 属性的元素显示悬浮提示。
 * 浮动层挂在 body 上（fixed 定位），避免被表格容器 overflow 裁剪；
 * 同时支持键盘焦点（focusin/focusout）触达。
 */
let tooltipEl = null;
let currentTarget = null;

function getTooltipElement() {
  if (!tooltipEl) {
    tooltipEl = document.createElement("div");
    tooltipEl.className = "global-tooltip";
    tooltipEl.setAttribute("role", "tooltip");
    document.body.appendChild(tooltipEl);
  }
  return tooltipEl;
}

function showTooltip(target) {
  const text = target.dataset.tooltip;
  if (!text) {
    return;
  }

  // 侧边栏展开时文字均可见，悬浮提示冗余；仅收起后的图标栏
  // 与始终无文字的图标按钮（如收起/展开按钮）保留提示
  if (
    target.closest(".sidebar") &&
    !target.closest(".sidebar-toggle") &&
    !document.body.classList.contains("sidebar-collapsed")
  ) {
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

function hideTooltip() {
  currentTarget = null;
  if (tooltipEl) {
    tooltipEl.classList.remove("visible");
  }
}

function isVisualizationTarget(target) {
  // 可视化画布（工位/机柜/拓扑 SVG）自带跟随鼠标的纵向浮窗，
  // 全局横向浮窗须让位，避免悬浮时同时出现两个浮窗
  return Boolean(target.closest(".visualization-svg"));
}

/**
 * 初始化全局 tooltip（应用启动时调用一次）
 */
export function initTooltip() {
  document.addEventListener("mouseover", (e) => {
    const target = e.target.closest("[data-tooltip]");
    if (target && !isVisualizationTarget(target)) {
      if (target !== currentTarget) {
        currentTarget = target;
        showTooltip(target);
      }
    } else if (currentTarget) {
      hideTooltip();
    }
  });

  document.addEventListener("focusin", (e) => {
    const target = e.target.closest("[data-tooltip]");
    if (target && !isVisualizationTarget(target)) {
      currentTarget = target;
      showTooltip(target);
    }
  });

  document.addEventListener("focusout", () => {
    if (currentTarget) {
      hideTooltip();
    }
  });

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
