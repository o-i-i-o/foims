/**
 * 画布跟随鼠标的多行浮窗（工位 / 机柜 / 拓扑三画布共享）。
 *
 * 旧实现在每张画布里各自创建 `#visualization-tooltip-{type}` div 并直接改
 * style；Vue 化后浮窗是模板里的 `.tooltip` 元素，位置按「鼠标右下方展开、
 * 越界则向相反方向翻转」在 nextTick 后测量一次即可（与旧四象限翻转等价）。
 */
import { computed, nextTick, ref, type ComputedRef, type Ref } from "vue";

/** 浮窗与光标的最小间距 */
const CURSOR_GAP = 12;
/** 浮窗与视口边缘的最小间距 */
const VIEWPORT_MARGIN = 8;

export interface CanvasTooltip {
  visible: Ref<boolean>;
  text: Ref<string>;
  /** 定位样式（left/top 绝对像素，配合 `.tooltip` 的 fixed 定位） */
  style: ComputedRef<Record<string, string>>;
  /** 在光标处显示/更新浮窗；text 为空时隐藏 */
  show(event: { clientX: number; clientY: number }, text: string): void;
  hide(): void;
}

export function useCanvasTooltip(tooltipRef: Ref<HTMLDivElement | null>): CanvasTooltip {
  const visible = ref(false);
  const text = ref("");
  const position = ref({ left: 0, top: 0 });

  const style = computed<Record<string, string>>(() => ({
    left: `${position.value.left}px`,
    top: `${position.value.top}px`,
  }));

  async function place(anchor: { x: number; y: number }): Promise<void> {
    await nextTick();
    const element = tooltipRef.value;
    if (!element) {
      return;
    }
    const width = element.offsetWidth;
    const height = element.offsetHeight;
    let left = anchor.x + CURSOR_GAP;
    let top = anchor.y + CURSOR_GAP;
    if (left + width > window.innerWidth - VIEWPORT_MARGIN) {
      left = anchor.x - width - CURSOR_GAP;
    }
    if (top + height > window.innerHeight - VIEWPORT_MARGIN) {
      top = anchor.y - height - CURSOR_GAP;
    }
    position.value = {
      left: Math.max(VIEWPORT_MARGIN, left),
      top: Math.max(VIEWPORT_MARGIN, top),
    };
  }

  function show(event: { clientX: number; clientY: number }, nextText: string): void {
    const anchor = { x: event.clientX, y: event.clientY };
    if (!nextText) {
      hide();
      return;
    }
    const textChanged = text.value !== nextText;
    text.value = nextText;
    visible.value = true;
    // 首次显示与文本变化需要重新测量尺寸，其余情况只移动位置
    if (textChanged) {
      position.value = { left: anchor.x + CURSOR_GAP, top: anchor.y + CURSOR_GAP };
      void place(anchor);
    } else {
      position.value = {
        left: Math.max(VIEWPORT_MARGIN, anchor.x + CURSOR_GAP),
        top: Math.max(VIEWPORT_MARGIN, anchor.y + CURSOR_GAP),
      };
    }
  }

  function hide(): void {
    visible.value = false;
    text.value = "";
  }

  return { visible, text, style, show, hide };
}
