/**
 * 弹窗焦点陷阱（WCAG 2.4.3）：
 * 挂载时记录当前焦点并移入弹窗——优先首个可编辑控件（表单弹窗直达输入框），
 * 无则聚焦容器本身（tabindex=-1）；Tab/Shift+Tab 循环约束在容器内；
 * 卸载时把焦点还给触发元素。可聚焦元素每次 Tab 现场重查，
 * 兼容弹窗内容动态增删（页签切换、按钮禁用、display:none 面板）。
 */
import { onBeforeUnmount, onMounted, type Ref } from "vue";

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function useFocusTrap(containerRef: Ref<HTMLElement | null>): void {
  let previouslyFocused: HTMLElement | null = null;

  function onKeydown(event: KeyboardEvent): void {
    if (event.key !== "Tab") {
      return;
    }
    const container = containerRef.value;
    if (!container) {
      return;
    }
    // display:none（offsetParent 为 null）与禁用元素不参与循环
    const focusable = Array.from(
      container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)
    ).filter((el) => el.offsetParent !== null);
    const active = document.activeElement;
    if (focusable.length === 0 || !container.contains(active)) {
      // 无可聚焦元素或焦点已逸出容器：拦截并收拢到容器
      event.preventDefault();
      container.focus();
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (!first || !last) {
      return;
    }
    if (event.shiftKey && active === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && active === last) {
      event.preventDefault();
      first.focus();
    }
  }

  onMounted(() => {
    const container = containerRef.value;
    if (!container) {
      return;
    }
    previouslyFocused =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const editable = container.querySelector<HTMLElement>(
      "input:not([type='hidden']), textarea, select"
    );
    if (editable && editable.offsetParent !== null) {
      editable.focus();
    } else {
      container.setAttribute("tabindex", "-1");
      container.focus();
    }
    document.addEventListener("keydown", onKeydown, true);
  });

  onBeforeUnmount(() => {
    document.removeEventListener("keydown", onKeydown, true);
    if (previouslyFocused && previouslyFocused.isConnected) {
      previouslyFocused.focus();
    }
  });
}
