/**
 * 全局通知（showToast）：旧版 web/static/js/utils/toast.js 的 Vue 对应物。
 * 通知统一走 showToast，禁止 alert()（项目硬性规范）。
 */
import { reactive } from "vue";

/** 通知类型 */
export type ToastType = "info" | "success" | "error" | "warning";

/** 单条通知 */
export interface ToastItem {
  id: number;
  message: string;
  type: ToastType;
}

let seed = 0;

// 全局单例状态：AppToast 组件挂载后渲染
export const toastState = reactive({
  toasts: [] as ToastItem[]
});

/**
 * 弹出通知
 * @param message 文案（应先经 t() 翻译）
 * @param type 通知类型
 * @param duration 展示时长（毫秒）
 */
export function showToast(message: string, type: ToastType = "info", duration = 3000): number {
  const id = ++seed;
  toastState.toasts.push({ id, message, type });
  setTimeout(() => {
    dismissToast(id);
  }, duration);
  return id;
}

export function dismissToast(id: number): void {
  const index = toastState.toasts.findIndex((item) => item.id === id);
  if (index !== -1) {
    toastState.toasts.splice(index, 1);
  }
}
