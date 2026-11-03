/**
 * 弹窗焦点陷阱（WCAG 2.4.3）：
 * 激活时记录当前焦点并移入容器——优先首个可编辑控件（表单弹窗直达输入框），
 * 无则聚焦容器本身（tabindex=-1）；Tab/Shift+Tab 循环约束在容器内；
 * 停用时把焦点还给触发元素。可聚焦元素每次 Tab 现场重查，
 * 兼容容器内容动态增删（页签切换、按钮禁用、display:none 面板）。
 *
 * 两种激活方式：
 * - 默认（不传 enabled）：挂载即激活，卸载时停用（ModalShell 等弹窗用例）；
 * - 传入 enabled ref：随其真假启停（常驻节点的局部弹层用例，如移动端抽屉）。
 */
import { onBeforeUnmount, onMounted, watch, type Ref } from "vue";

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

export function useFocusTrap(
  containerRef: Ref<HTMLElement | null>,
  enabled?: Ref<boolean>
): void {
  let previouslyFocused: HTMLElement | null = null;
  let active = false;

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
    const current = document.activeElement;
    if (focusable.length === 0 || !container.contains(current)) {
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
    if (event.shiftKey && current === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && current === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function activate(): void {
    const container = containerRef.value;
    if (!container || active) {
      return;
    }
    active = true;
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
  }

  function deactivate(): void {
    if (!active) {
      return;
    }
    active = false;
    document.removeEventListener("keydown", onKeydown, true);
    if (previouslyFocused && previouslyFocused.isConnected) {
      previouslyFocused.focus();
    }
    previouslyFocused = null;
  }

  if (enabled) {
    // DOM 更新后再启停，确保容器可见性已随开关落定（再激活前先收敛残留状态）
    watch(
      enabled,
      (on) => {
        if (on) {
          activate();
        } else {
          deactivate();
        }
      },
      { flush: "post" }
    );
    onBeforeUnmount(deactivate);
  } else {
    onMounted(activate);
    onBeforeUnmount(deactivate);
  }
}
