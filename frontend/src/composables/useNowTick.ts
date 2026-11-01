/**
 * 共享秒级时钟：模块级单例 interval + 引用计数。
 * 首个订阅者启动计时，最后一个卸载即停止，避免每个组件各挂一个 interval；
 * 订阅方拿到同一个每秒推进的 Date.now() 毫秒 ref，用于「基准值 + 本地流逝」
 * 的动态展示（服务端时间/运行时长等快照字段的前端推进）。
 */
import { onUnmounted, ref, type Ref } from "vue";

const now = ref(Date.now());
let timer: number | undefined;
let consumers = 0;

/** 订阅共享秒级时钟：返回每秒推进的毫秒时间戳 ref */
export function useNowTick(): Ref<number> {
  consumers += 1;
  if (timer === undefined) {
    timer = window.setInterval(() => {
      now.value = Date.now();
    }, 1000);
  }
  onUnmounted(() => {
    consumers -= 1;
    if (consumers === 0 && timer !== undefined) {
      window.clearInterval(timer);
      timer = undefined;
    }
  });
  return now;
}
