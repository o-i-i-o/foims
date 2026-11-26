/**
 * 采集/同步按钮计时反馈：包装一次异步采集任务，执行期间 syncing=true 且
 * elapsed 每秒递增，供页脚按钮显示「同步中... Ns」与禁用态；结束（含异常）
 * 自动停止。防重入由 run 的 syncing 守卫承担。
 */
import { onBeforeUnmount, ref } from "vue";

export function useSyncTimer() {
  const syncing = ref(false);
  const elapsed = ref(0);

  let timer: ReturnType<typeof setInterval> | null = null;

  function stop(): void {
    syncing.value = false;
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
  }

  /** 按钮文案：空闲返回原始文案，进行中返回「运行文案 + 已耗时秒数」 */
  function label(idleText: string, runningText: string): string {
    return syncing.value ? `${runningText} ${elapsed.value}s` : idleText;
  }

  /** 包装一次采集任务：计时启动/停止，异常同样停止计时后向上抛 */
  async function run(task: () => Promise<void>): Promise<void> {
    if (syncing.value) {
      return;
    }
    syncing.value = true;
    elapsed.value = 0;
    timer = setInterval(() => {
      elapsed.value += 1;
    }, 1000);
    try {
      await task();
    } finally {
      stop();
    }
  }

  onBeforeUnmount(stop);

  return { syncing, elapsed, run, label };
}
