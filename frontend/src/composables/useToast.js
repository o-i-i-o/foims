/**
 * 全局通知（showToast）：旧版 web/static/js/utils/toast.js 的 Vue 对应物。
 * 通知统一走 showToast，禁止 alert()（项目硬性规范）。
 */
import { reactive } from "vue";

let seed = 0;

// 全局单例状态：AppToast 组件挂载后渲染
export const toastState = reactive({
  toasts: []
});

/**
 * 弹出通知
 * @param {string} message 文案（应先经 t() 翻译）
 * @param {"info"|"success"|"error"|"warning"} type 通知类型
 * @param {number} duration 展示时长（毫秒）
 */
export function showToast(message, type = "info", duration = 3000) {
  const id = ++seed;
  toastState.toasts.push({ id, message, type });
  setTimeout(() => {
    dismissToast(id);
  }, duration);
  return id;
}

export function dismissToast(id) {
  const index = toastState.toasts.findIndex((item) => item.id === id);
  if (index !== -1) {
    toastState.toasts.splice(index, 1);
  }
}
