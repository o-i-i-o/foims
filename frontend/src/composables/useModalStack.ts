/**
 * 弹窗层级栈（对应旧版 modalLoader 仅关闭最上层活动弹窗的语义）：
 * 嵌套子弹窗场景下，Esc 只关闭最上层弹窗，避免父弹窗被连带关闭。
 * 调用即注册监听（setup 期），返回注销函数供卸载时调用。
 */
const stack: object[] = [];

export function useModalStack(onEscape: () => void): () => void {
  const token: object = {};

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && stack[stack.length - 1] === token) {
      onEscape();
    }
  }

  stack.push(token);
  document.addEventListener("keydown", onKeydown);

  return () => {
    const index = stack.indexOf(token);
    if (index !== -1) {
      stack.splice(index, 1);
    }
    document.removeEventListener("keydown", onKeydown);
  };
}
