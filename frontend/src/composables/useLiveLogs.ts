/**
 * 运行日志加载（日志页「运行日志」页签专用）。
 *
 * 数据以服务端「本轮」日志文件为权威源：页签激活或界面语言变化时拉取一次
 * 文件尾部（程序启动日志与页面刷新前的历史因此始终可见），SSE 通知流只推
 * 「有新日志」信号，收到后防抖重新拉取并整体替换显示。连接生命周期随
 * enabled 启停：切离页签即断开，不留后台常驻连接；断线由浏览器按
 * EventSource 默认节奏自动重连，期间 connected 为 false。建连过渡期
 * （已发起尚未收到 open/error）由 connecting 标记，供界面显示「连接中」
 * 而非误导性的「未连接」——慢网络下建连可能耗时数秒。
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
  // 建连过渡态：EventSource 已创建但尚未收到 open/error，期间不显示误导性的「未连接」
  const connecting = ref(false);
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
      connecting.value = false;
      connected.value = true;
    });
    next.addEventListener("error", () => {
      connecting.value = false;
      connected.value = false;
    });
    next.addEventListener("notify", scheduleRefresh);
    source = next;
    connecting.value = true;
    connected.value = false;
    // 建连即拉取一次文件尾部，保证进入页签/刷新页面即见完整本轮日志
    void refresh();
  }

  function close(): void {
    source?.close();
    source = null;
    connected.value = false;
    connecting.value = false;
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
  // 会话恢复（401 → refresh 成功）后立即重建通知流：EventSource 不走 fetch
  // 拦截器，401 后只能等浏览器默认约 3s 的自动重连节奏，这里毫秒级重连，
  // 刷新页面（会话已过期）时连接状态不必迟到数秒
  function handleSessionRestored(): void {
    if (source && !connected.value) {
      close();
      open();
    }
  }
  window.addEventListener("foims:session-restored", handleSessionRestored);
  onUnmounted(() => {
    window.removeEventListener("foims:session-restored", handleSessionRestored);
    close();
  });

  function clear(): void {
    logs.value = [];
  }

  return { logs, connected, connecting, paused, clear };
}
