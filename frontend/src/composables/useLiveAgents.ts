/**
 * SNMP 实时快照订阅（SSE）。
 *
 * 同源 EventSource 自动携带 HttpOnly Cookie（后端 /api/agents 组经
 * agent_admin_guard 鉴权），断线由浏览器按 retry 自动重连；组件卸载时
 * 关闭连接。服务端只在注册表有写入时推送，`summary` 为全量摘要，
 * 带 device_id 订阅时另推该设备的 `device` 事件（端口明细）。
 */
import { onUnmounted, ref } from "vue";
import type { LiveDeviceDetail } from "../types/api/generated/LiveDeviceDetail";
import type { LiveSummaryRow } from "../types/api/generated/LiveSummaryRow";

const LIVE_STREAM_URL = "/api/agents/live/stream";

function parseJson(raw: unknown): unknown {
  if (typeof raw !== "string") {
    return null;
  }
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return null;
  }
}

/**
 * 订阅实时摘要。`enabled=false`（角色无主机监控权限）时不建连接，
 * 避免 EventSource 对 403 无限重连。
 */
export function useLiveAgents(enabled = true) {
  const devices = ref<LiveSummaryRow[]>([]);
  const connected = ref(false);

  const source = enabled ? new EventSource(LIVE_STREAM_URL) : null;
  source?.addEventListener("open", () => {
    connected.value = true;
  });
  source?.addEventListener("error", () => {
    connected.value = false;
  });
  source?.addEventListener("summary", (event: MessageEvent) => {
    const parsed = parseJson(event.data);
    if (Array.isArray(parsed)) {
      devices.value = parsed as LiveSummaryRow[];
    }
  });
  onUnmounted(() => source?.close());

  return { devices, connected };
}

/** 单设备实时明细订阅（端口状态/速率），用于设备端口弹窗。 */
export function useLiveDevicePorts(deviceId: string, enabled = true) {
  const detail = ref<LiveDeviceDetail | null>(null);

  const source = enabled
    ? new EventSource(`${LIVE_STREAM_URL}?device_id=${encodeURIComponent(deviceId)}`)
    : null;
  source?.addEventListener("device", (event: MessageEvent) => {
    const parsed = parseJson(event.data);
    if (parsed && typeof parsed === "object") {
      detail.value = parsed as LiveDeviceDetail;
    }
  });
  onUnmounted(() => source?.close());

  return { detail };
}
