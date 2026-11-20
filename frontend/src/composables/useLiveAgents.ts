/**
 * SNMP 实时快照订阅（SSE）。
 *
 * 同源 EventSource 自动携带 HttpOnly Cookie（后端 /api/agents 组经
 * agent_admin_guard 鉴权），断线由浏览器按 retry 自动重连；引用计数归零时
 * 关闭连接。服务端只在注册表有写入时推送；带 device_id 订阅时另推该设备的
 * `device` 事件（端口明细）。
 *
 * 观察范围 `scope` 会驱动后端采样档位与摘要裁剪：
 * - `dashboard`：全设备 5s 汇总（卡片需要全量数据，但无需秒级）；
 * - `agents`：只有 deviceIds 中的设备升级为 1s（只采资源组），摘要也只含
 *   这些设备；其余设备保持基线且不再随该连接下发。
 */
import { isRef, onUnmounted, ref, watch, type Ref } from "vue";
import type { LiveDeviceDetail } from "../types/api/generated/LiveDeviceDetail";
import type { LiveSummaryRow } from "../types/api/generated/LiveSummaryRow";

const LIVE_STREAM_URL = "/api/agents/live/stream";

export type LiveScope = "dashboard" | "agents";

/** 摘要订阅选项：同一 scope 的页面共享一条连接。 */
export interface LiveAgentsOptions {
  scope?: LiveScope;
  /** 当前页需要实时刷新的设备 id（仅 agents scope 生效；按集合去重） */
  deviceIds?: Ref<readonly string[]> | readonly string[];
}

export function parseJson(raw: unknown): unknown {
  if (typeof raw !== "string") {
    return null;
  }
  try {
    return JSON.parse(raw) as unknown;
  } catch {
    return null;
  }
}

/** 摘要流的 scope 级共享通道：连接、数据、引用计数同生命周期 */
interface SummaryChannel {
  devices: Ref<LiveSummaryRow[]>;
  connected: Ref<boolean>;
  source: EventSource | null;
  refs: number;
  /** 已登记的设备集合签名；变化时重连以更新后端观察范围 */
  signature: string;
}

const summaryChannels = new Map<LiveScope, SummaryChannel>();

function normalizeDeviceIds(ids: readonly string[] | undefined): string {
  if (!ids || ids.length === 0) {
    return "";
  }
  return Array.from(new Set(ids.filter(Boolean))).sort().join(",");
}

function buildSummaryUrl(scope: LiveScope, signature: string): string {
  const params = new URLSearchParams({ scope });
  if (scope === "agents" && signature) {
    params.set("device_ids", signature);
  }
  return `${LIVE_STREAM_URL}?${params.toString()}`;
}

/** 建立 EventSource，并只让当前实例写回通道数据（旧连接事件不污染新状态） */
function openSummarySource(
  channel: SummaryChannel,
  scope: LiveScope,
  signature: string
): EventSource {
  const source = new EventSource(buildSummaryUrl(scope, signature));
  source.addEventListener("open", () => {
    if (channel.source === source) {
      channel.connected.value = true;
    }
  });
  source.addEventListener("error", () => {
    if (channel.source === source) {
      channel.connected.value = false;
    }
  });
  source.addEventListener("summary", (event: MessageEvent) => {
    if (channel.source !== source) {
      return;
    }
    const parsed = parseJson(event.data);
    if (Array.isArray(parsed)) {
      channel.devices.value = parsed as LiveSummaryRow[];
    }
  });
  return source;
}

/**
 * 取用共享摘要流（同 scope 已有则复用并计数）：服务端每个节拍为「每条连接」
 * 构建并序列化一次全量快照，同页多消费方（看板/主机列表/详情弹窗）各建一条
 * 连接等于成倍放大服务端开销与带宽。
 */
function acquireSummaryChannel(scope: LiveScope, signature: string): SummaryChannel {
  const existing = summaryChannels.get(scope);
  if (existing) {
    existing.refs += 1;
    return existing;
  }
  const channel: SummaryChannel = {
    devices: ref<LiveSummaryRow[]>([]),
    connected: ref(false),
    source: null,
    refs: 1,
    signature
  };
  // 首建即带上设备集合，避免「先空连一次再重连」的无效 SSE 握手
  channel.source = openSummarySource(channel, scope, signature);
  summaryChannels.set(scope, channel);
  return channel;
}

/** 更新 agents scope 的设备集合；集合变化才重连，避免无意义抖动 */
function updateDeviceSignature(
  channel: SummaryChannel,
  scope: LiveScope,
  ids: readonly string[] | undefined
): void {
  if (scope !== "agents") {
    return;
  }
  const signature = normalizeDeviceIds(ids);
  if (channel.signature === signature) {
    return;
  }
  channel.signature = signature;
  channel.connected.value = false;
  // 保留上一帧数据，重连期间列表不会闪空
  channel.source?.close();
  channel.source = openSummarySource(channel, scope, signature);
}

/**
 * 订阅实时摘要。`enabled=false`（角色无主机监控权限）时不建连接也不复用，
 * 返回空快照，避免 EventSource 对 403 无限重连。
 */
export function useLiveAgents(enabled = true, options: LiveAgentsOptions = {}) {
  if (!enabled) {
    return { devices: ref<LiveSummaryRow[]>([]), connected: ref(false) };
  }
  const scope = options.scope ?? "agents";
  // 首建签名取当前值；Ref 的后续变化由下方 watch（immediate）对齐
  const initialSignature = isRef(options.deviceIds)
    ? normalizeDeviceIds(options.deviceIds.value)
    : normalizeDeviceIds(options.deviceIds);
  const channel = acquireSummaryChannel(scope, initialSignature);

  if (options.deviceIds !== undefined) {
    if (isRef(options.deviceIds)) {
      watch(
        options.deviceIds,
        (ids) => updateDeviceSignature(channel, scope, ids),
        { immediate: true }
      );
    } else {
      updateDeviceSignature(channel, scope, options.deviceIds);
    }
  }

  onUnmounted(() => {
    channel.refs -= 1;
    if (channel.refs === 0) {
      channel.source?.close();
      // 仅当仍是当前共享实例时释放，避免误影响后来重建的通道
      if (summaryChannels.get(scope) === channel) {
        summaryChannels.delete(scope);
      }
    }
  });
  return { devices: channel.devices, connected: channel.connected };
}

/**
 * 单设备实时明细订阅（端口状态/速率），用于设备端口弹窗与详情弹窗。
 *
 * 该连接同时把设备登记为 `Detail` 档（1s、全端口），是「选中设备实时采集」
 * 的唯一入口；`deviceId` 支持 Ref，弹窗内切换到同 IP 的另一来源时自动重连。
 */
export function useLiveDevicePorts(
  deviceId: Ref<string> | string,
  enabled: Ref<boolean> | boolean = true
) {
  const detail = ref<LiveDeviceDetail | null>(null);
  const idRef = isRef(deviceId) ? deviceId : ref(deviceId);
  const enabledRef = isRef(enabled) ? enabled : ref(enabled);
  let source: EventSource | null = null;

  function close(): void {
    source?.close();
    source = null;
  }

  watch(
    [idRef, enabledRef],
    ([id, on]) => {
      close();
      detail.value = null;
      if (!on || !id) {
        return;
      }
      const current = new EventSource(
        `${LIVE_STREAM_URL}?device_id=${encodeURIComponent(id)}`
      );
      current.addEventListener("device", (event: MessageEvent) => {
        // 只有当前连接能写回：切换设备后旧连接的迟到事件直接丢弃
        if (source !== current) {
          return;
        }
        const parsed = parseJson(event.data);
        if (parsed && typeof parsed === "object") {
          detail.value = parsed as LiveDeviceDetail;
        }
      });
      source = current;
    },
    { immediate: true }
  );
  onUnmounted(close);

  return { detail };
}
