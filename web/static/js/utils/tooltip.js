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
  if (!text) return;

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

/**
 * 初始化全局 tooltip（应用启动时调用一次）
 */
export function initTooltip() {
  document.addEventListener("mouseover", (e) => {
    const target = e.target.closest("[data-tooltip]");
    if (target) {
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
    if (target) {
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
