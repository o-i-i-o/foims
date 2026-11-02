/**
 * 运行日志加载（日志页「运行日志」页签专用）。
 *
 * 数据以服务端「本轮」日志文件为权威源：页签激活或界面语言变化时拉取一次
 * 文件尾部（程序启动日志与页面刷新前的历史因此始终可见），SSE 通知流只推
 * 「有新日志」信号，收到后防抖重新拉取并整体替换显示。连接生命周期随
 * enabled 启停：切离页签即断开，不留后台常驻连接；断线由浏览器按
 * EventSource 默认节奏自动重连，期间 connected 为 false。
 *
 * 暂停只停「刷新」：暂停期间通知被忽略（画面冻结），恢复后立即补拉跳到
 * 最新；清空仅清当前显示，下次新日志通知会重新加载文件尾部。拉取失败
 * 保留旧内容静默处理（通知流断连状态已由 connected 指示，避免刷屏提示）。
 */
import { onUnmounted, ref, watch, type Ref } from "vue";
import { useI18n } from "vue-i18n";
import { getRunLogs, runtimeLogsUrl } from "../api/logs";

/** 通知防抖间隔：新日志成批到达（如启动阶段）时只触发一次重新拉取 */
const REFRESH_DEBOUNCE_MS = 300;

export function useLiveLogs(enabled: Ref<boolean>) {
  const { locale } = useI18n();

  const logs = ref<string[]>([]);
  const connected = ref(false);
  const paused = ref(false);

  let source: EventSource | null = null;
  let refreshTimer: number | undefined;

  async function refresh(): Promise<void> {
    const result = await getRunLogs(locale.value);
    if (result.success && Array.isArray(result.data?.lines)) {
      logs.value = result.data.lines;
    }
  }

  function scheduleRefresh(): void {
    if (paused.value) {
      return;
    }
    clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(() => {
      void refresh();
    }, REFRESH_DEBOUNCE_MS);
  }

  function open(): void {
    const next = new EventSource(runtimeLogsUrl());
    next.addEventListener("open", () => {
      connected.value = true;
    });
    next.addEventListener("error", () => {
      connected.value = false;
    });
    next.addEventListener("notify", scheduleRefresh);
    source = next;
    // 建连即拉取一次文件尾部，保证进入页签/刷新页面即见完整本轮日志
    void refresh();
  }

  function close(): void {
    source?.close();
    source = null;
    connected.value = false;
    clearTimeout(refreshTimer);
  }

  // 页签激活/界面语言切换：重建通知流并按新语言重新拉取文件尾部
  watch(
    [enabled, locale] as const,
    ([active]) => {
      close();
      if (active) {
        open();
      }
    },
    { immediate: true }
  );
  // 恢复接收时立即补拉最新日志（暂停期间的通知被忽略，画面停在旧内容）
  watch(paused, (value) => {
    if (!value) {
      void refresh();
    }
  });
  onUnmounted(close);

  function clear(): void {
    logs.value = [];
  }

  return { logs, connected, paused, clear };
}
