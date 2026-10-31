/**
 * 运行日志 SSE 订阅（日志页「运行日志」页签专用）。
 *
 * 连接生命周期随 enabled（页签激活状态）启停：切离页签即断开，不留后台
 * 常驻连接；断线由浏览器按 EventSource 默认节奏自动重连，期间 connected
 * 为 false。服务端只推建连之后的新日志（不重放历史）；消费过慢落后超过
 * 服务端缓冲时，收到 skipped 占位事件而非静默丢数据。
 *
 * 暂停只停「展示」：暂停期间新事件直接丢弃（不积压内存），恢复后从最新
 * 日志继续，用于冻结画面查看某条日志的场景。
 */
import { onUnmounted, ref, watch, type Ref } from "vue";
import { useI18n } from "vue-i18n";
import { runtimeLogsUrl } from "../api/logs";
import type { LiveLogEvent } from "../types/api/generated/LiveLogEvent";
import { parseJson } from "./useLiveAgents";

/** 前端缓冲上限：超出丢弃最旧条目，防止长会话内存无限增长 */
const MAX_BUFFER = 1000;

export function useLiveLogs(enabled: Ref<boolean>) {
  const { locale } = useI18n();

  const logs = ref<LiveLogEvent[]>([]);
  const connected = ref(false);
  const paused = ref(false);

  let source: EventSource | null = null;

  function append(event: LiveLogEvent): void {
    if (paused.value) {
      return;
    }
    logs.value.push(event);
    if (logs.value.length > MAX_BUFFER) {
      logs.value.splice(0, logs.value.length - MAX_BUFFER);
    }
  }

  function open(): void {
    if (source) {
      return;
    }
    const next = new EventSource(runtimeLogsUrl(locale.value));
    next.addEventListener("open", () => {
      connected.value = true;
    });
    next.addEventListener("error", () => {
      connected.value = false;
    });
    next.addEventListener("log", (event: MessageEvent) => {
      const parsed = parseJson(event.data);
      if (parsed !== null) {
        append(parsed as LiveLogEvent);
      }
    });
    source = next;
  }

  function close(): void {
    source?.close();
    source = null;
    connected.value = false;
  }

  watch(enabled, (active) => (active ? open() : close()), { immediate: true });
  onUnmounted(close);

  function clear(): void {
    logs.value = [];
  }

  return { logs, connected, paused, clear };
}
