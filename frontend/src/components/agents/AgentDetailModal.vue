<script setup lang="ts">
/**
 * 主机详情弹窗：分组卡片（系统/CPU/内存/磁盘/网络/温度风扇）+ 近 24h SVG 迷你趋势五卡。
 * 详情与历史各走一条 useQuery；曲线为声明式渲染，无延迟测量时序依赖
 * （SVG 按 viewBox 等比缩放，容器宽度不影响绘制）。
 */
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import { useLiveAgents } from "../../composables/useLiveAgents";
import { formatBps, formatDateTime } from "../../utils/formatter";
import ModalShell from "../ModalShell.vue";
import AgentSparkline from "./AgentSparkline.vue";
import AgentUsageBar from "./AgentUsageBar.vue";

/** 主机详情载荷（GET /api/agents/{id}，仅声明本弹窗消费的字段） */
interface AgentDetail {
  status: string;
  source: string | null;
  /** SNMP 主机关联的设备 id（实时快照按此匹配；Agent 主机为 null） */
  device_id: string | null;
  hostname: string | null;
  ip: string | null;
  os: string | null;
  arch: string | null;
  agent_version: string | null;
  uptime_secs: number | null;
  mem_usage_pct: number | null;
  max_temp: number | null;
  raw_metrics: RawMetrics | null;
}

/** IPMI 单个传感器读数（_ipmi_state.temps/fans/volts 元素，与 LiveSensorRow 同构） */
interface IpmiSensorEntry {
  name?: string | null;
  value?: number | string | null;
  unit?: string | null;
}

/** IPMI 私有明细（落库 raw_metrics._ipmi_state，collect.to_report 口径） */
interface IpmiState {
  power_on?: boolean | null;
  manuf_id?: number | null;
  product_id?: number | null;
  fw_revision?: string | null;
  temps?: IpmiSensorEntry[] | null;
  fans?: IpmiSensorEntry[] | null;
  volts?: IpmiSensorEntry[] | null;
}

/** raw_metrics 上报 JSON 的消费字段（全部可选：agent 上报可能缺键） */
interface RawMetrics {
  machine_id?: string | null;
  collected_at?: string | null;
  system?: {
    os?: string | null;
    kernel?: string | null;
    arch?: string | null;
    uptime_secs?: number | null;
  } | null;
  cpu?: {
    usage_pct?: number | null;
    load1?: number | null;
    load5?: number | null;
    load15?: number | null;
    cores?: number | null;
  } | null;
  memory?: {
    total?: number | null;
    used?: number | null;
    swap_total?: number | null;
    swap_used?: number | null;
  } | null;
  disks?: Array<{
    device?: string | null;
    mount?: string | null;
    total?: number | null;
    used?: number | null;
    read_iops?: number | null;
    write_iops?: number | null;
    util_pct?: number | null;
  }> | null;
  nets?: Array<{
    iface?: string | null;
    rx_bps?: number | null;
    tx_bps?: number | null;
    status?: string | null;
    admin_status?: string | null;
    speed_mbps?: number | null;
    errors?: {
      in?: number | null;
      out?: number | null;
      in_discards?: number | null;
      out_discards?: number | null;
    } | null;
  }> | null;
  sensors?: Array<{
    label?: string | null;
    kind?: string | null;
    value?: number | string | null;
  }> | null;
  /** IPMI 主机专用：BMC 传感器与电源状态（SNMP/Agent 行无此键） */
  _ipmi_state?: IpmiState | null;
}

/** 24h 曲线点（后端补齐六键，缺值为 null） */
interface AgentHistoryPoint {
  cpu: number | null;
  mem: number | null;
  disk: number | null;
  temp: number | null;
  rx_bps: number | null;
  tx_bps: number | null;
}

/** 曲线序列键 */
type HistoryKey = "cpu" | "mem" | "disk" | "temp" | "rx_bps" | "tx_bps";

const props = defineProps<{
  agentId: number | string;
}>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

// 状态元数据：徽章配色类 + 名称 i18n 键（active/offline 由上报自动翻转）
const STATUS_META: Record<string, { badge: string; labelKey: string }> = {
  active: { badge: "agent-badge-active", labelKey: "agents.status_active" },
  offline: { badge: "agent-badge-offline", labelKey: "agents.status_offline" },
  disabled: { badge: "agent-badge-disabled", labelKey: "agents.status_disabled" }
};

const detailQuery = useQuery({
  queryKey: computed(() => ["agent-detail", props.agentId]),
  queryFn: async (): Promise<AgentDetail | null> => {
    const result = await apiGet<AgentDetail>(`/api/agents/${props.agentId}`);
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

const historyQuery = useQuery({
  queryKey: computed(() => ["agent-history", props.agentId]),
  queryFn: async (): Promise<AgentHistoryPoint[]> => {
    const result = await apiGet<{ items?: AgentHistoryPoint[] }>(
      `/api/agents/${props.agentId}/history?hours=24`
    );
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    const items = result.data?.items;
    return Array.isArray(items) ? items : [];
  }
});

const agent = computed(() => detailQuery.data.value);

const raw = computed<RawMetrics>(() => agent.value?.raw_metrics ?? {});

// ===== SNMP 实时快照（秒级 CPU/内存/温度/流量）：只读内存注册表，不落库 =====
// 弹窗自带一条 SSE 订阅：SNMP 主机按 device_id 匹配，Agent 主机按 IP 兜底
const { devices: liveDevices, connected: liveConnected } = useLiveAgents();

/** 快照新鲜窗口：采样与 SSE 节拍均为 1s，留 10s 容忍丢包/短暂重连 */
const LIVE_FRESH_MS = 10_000;

/** 秒级时钟：驱动新鲜度重算，SSE 断流后按时回落而非永久停留旧读数 */
const nowTick = ref(Date.now());

/**
 * 本端收到该设备新快照的本地时刻：不拿服务端 collected_at 和本地时钟做差，
 * 否则两端时钟不同步（偏快超过新鲜窗口）会把实时读数误判为陈旧。
 */
const liveSeenAt = ref(0);

let clock: number | undefined;
onMounted(() => {
  clock = window.setInterval(() => {
    nowTick.value = Date.now();
  }, 1000);
});
onUnmounted(() => {
  if (clock !== undefined) {
    window.clearInterval(clock);
  }
});

const live = computed(() => {
  const deviceId = agent.value?.device_id;
  if (deviceId) {
    // 同设备可能存在 SNMP 与 IPMI 两路快照（各自独立采集），按行来源优先匹配
    const rows = liveDevices.value.filter((row) => row.device_id === deviceId);
    return rows.find((row) => row.source === agent.value?.source) ?? rows[0];
  }
  const ip = agent.value?.ip;
  return ip ? liveDevices.value.find((row) => row.ip === ip) : undefined;
});

// 采集时间推进 = 新快照到手；离线设备保留的陈旧快照不会推进，自然过期
watch(
  () => live.value?.collected_at,
  (collectedAt) => {
    if (collectedAt) {
      liveSeenAt.value = Date.now();
    }
  }
);

/** 快照 10s 内视为实时；陈旧时回落 5 分钟全量口径，避免展示冻结读数 */
const liveFresh = computed(
  () => live.value != null && nowTick.value - liveSeenAt.value < LIVE_FRESH_MS
);

/** 最近一次实时快照的采集时间（有快照即展示，不受新鲜度影响） */
const liveCollectedAt = computed(() => live.value?.collected_at ?? null);

/** 已收到过快照但 SSE 连接断开：提示读数已停止刷新 */
const liveLost = computed(() => !liveConnected.value && live.value != null);

const liveCpu = computed(() => (liveFresh.value ? (live.value?.cpu_usage_pct ?? null) : null));
const liveMemPct = computed(() => (liveFresh.value ? (live.value?.mem_usage_pct ?? null) : null));
const liveMemUsed = computed(() => (liveFresh.value ? (live.value?.mem_used ?? null) : null));
const liveMemTotal = computed(() => (liveFresh.value ? (live.value?.mem_total ?? null) : null));
const liveTemp = computed(() => (liveFresh.value ? (live.value?.temperature_c ?? null) : null));

const displayCpu = computed(() => liveCpu.value ?? raw.value.cpu?.usage_pct ?? null);
const displayMemPct = computed(() => liveMemPct.value ?? agent.value?.mem_usage_pct ?? null);
const displayMemUsed = computed(() => liveMemUsed.value ?? raw.value.memory?.used ?? null);
const displayMemTotal = computed(() => liveMemTotal.value ?? raw.value.memory?.total ?? null);

// ===== IPMI 硬件视图：BMC 无 CPU/内存/磁盘/网络语义，详情改展温度/风扇/电压与电源状态 =====
const isIpmi = computed(() => agent.value?.source === "ipmi");
const ipmiState = computed<IpmiState>(() => raw.value._ipmi_state ?? {});
/** 电源状态：优先秒级实时快照（Get Chassis Status），回落 5 分钟轮询落库值 */
const ipmiPowerOn = computed<boolean | null>(() => {
  if (liveFresh.value && live.value?.power_on != null) {
    return live.value.power_on;
  }
  return ipmiState.value.power_on ?? null;
});
const ipmiTemps = computed(() => ipmiState.value.temps ?? []);
const ipmiFans = computed(() => ipmiState.value.fans ?? []);
const ipmiVolts = computed(() => ipmiState.value.volts ?? []);
const ipmiHasSensors = computed(
  () => ipmiTemps.value.length + ipmiFans.value.length + ipmiVolts.value.length > 0
);

/** 传感器行文案：RPM 取整、其余保留一位小数，附单位 */
function formatIpmiSensor(entry: IpmiSensorEntry): string {
  const value = Number(entry.value);
  if (!Number.isFinite(value)) {
    return "-";
  }
  const unit = entry.unit ?? "";
  const text = /rpm/i.test(unit) ? String(Math.round(value)) : value.toFixed(1);
  return `${text} ${unit}`.trim();
}

/** 采集端口选项（GET /api/agents/{id}/monitor-port） */
interface MonitorPortOption {
  name: string;
  status: string;
  speed: string | null;
  monitored: boolean;
}

interface MonitorPortPayload {
  device_id: string;
  if_name: string | null;
  ports: MonitorPortOption[];
}

// 仅 SNMP 主机有采集端口概念：Agent 主机不发起该请求
const monitorPortQuery = useQuery({
  queryKey: computed(() => ["agent-monitor-port", props.agentId]),
  queryFn: async (): Promise<MonitorPortPayload | null> => {
    const result = await apiGet<MonitorPortPayload>(`/api/agents/${props.agentId}/monitor-port`);
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => agent.value?.source === "snmp")
});

const monitorPortOptions = computed(() => monitorPortQuery.data.value?.ports ?? []);

/** 下拉当前值：空串 = 未指定，采集器回落设备首端口 */
const selectedPort = ref("");
const savingPort = ref(false);

watch(
  monitorPortQuery.data,
  (data) => {
    selectedPort.value = data?.if_name ?? "";
  },
  { immediate: true }
);

async function saveMonitorPort(): Promise<void> {
  if (savingPort.value) {
    return;
  }
  savingPort.value = true;
  try {
    const result = await apiPut<MonitorPortPayload, { if_name: string | null }>(
      `/api/agents/${props.agentId}/monitor-port`,
      { if_name: selectedPort.value || null }
    );
    if (!result.success) {
      showToast(result.message || t("common.save_failed"), "error");
      return;
    }
    showToast(t("agents.monitor_port_saved"), "success");
    await monitorPortQuery.refetch();
  } finally {
    savingPort.value = false;
  }
}

/** 下拉变更即保存（无额外按钮）：以事件目标值兜底，避免与 v-model 时序耦合 */
async function onPortChange(event: Event): Promise<void> {
  const target = event.target as HTMLSelectElement | null;
  selectedPort.value = target?.value ?? "";
  await saveMonitorPort();
}

const detailError = computed(() => detailQuery.error.value?.message || t("common.load_failed"));

const statusMeta = computed(() => {
  const status = agent.value?.status;
  return status ? STATUS_META[status] : undefined;
});
const statusText = computed(() =>
  statusMeta.value ? t(statusMeta.value.labelKey) : String(agent.value?.status ?? "-")
);

/** CPU 负载：1/5/15 分钟负载拼接展示 */
function formatLoads(cpu: RawMetrics["cpu"]): string {
  const loads = [cpu?.load1, cpu?.load5, cpu?.load15];
  if (loads.every((v) => v == null)) {
    return "-";
  }
  return loads.map((v) => (v == null ? "-" : Number(v).toFixed(2))).join(" / ");
}

/** 字节容量：自动选 B/KB/MB/GB/TB 单位 */
function formatBytes(bytes: unknown): string {
  const value = Number(bytes);
  if (!Number.isFinite(value) || value <= 0) {
    return "-";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = value;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  return `${unitIndex === 0 ? size : size.toFixed(1)} ${units[unitIndex]}`;
}

/** 端口链路状态徽标类：up=绿、down=红、其余中性 */
function netStatusClass(status: string | null | undefined): string {
  if (status === "up") {
    return "status-up";
  }
  if (status === "down") {
    return "status-down";
  }
  return "status-unknown";
}

/** 本轮错误增量（in+out，缺省计 0） */
function netErrDelta(errors: { in?: number | null; out?: number | null } | null | undefined): number {
  return (errors?.in ?? 0) + (errors?.out ?? 0);
}

/** 本轮丢弃增量（in_discards+out_discards，缺省计 0） */
function netDiscDelta(
  errors: { in_discards?: number | null; out_discards?: number | null } | null | undefined
): number {
  return (errors?.in_discards ?? 0) + (errors?.out_discards ?? 0);
}

/** 运行时长：>1 天 "x天y小时"、>1 小时 "x小时y分"、其余 "x分钟" */
function formatUptime(secs: unknown): string {
  const total = Number(secs);
  if (!Number.isFinite(total) || total <= 0) {
    return "-";
  }
  const d = Math.floor(total / 86400);
  const h = Math.floor((total % 86400) / 3600);
  const m = Math.floor((total % 3600) / 60);
  if (d > 0) {
    return t("agents.uptime_dh", { d, h });
  }
  if (h > 0) {
    return t("agents.uptime_hm", { h, m });
  }
  return t("agents.uptime_m", { m });
}

/** 交换分区：已用 / 总量（均未上报时 "-"） */
function formatSwap(memory: NonNullable<RawMetrics["memory"]>): string {
  if (memory.swap_total == null && memory.swap_used == null) {
    return "-";
  }
  return `${formatBytes(memory.swap_used ?? 0)} / ${formatBytes(memory.swap_total ?? 0)}`;
}

/** 读写 IOPS 数值：未上报返回 "-" */
function formatIops(disk: NonNullable<RawMetrics["disks"]>[number]): string {
  if (disk.read_iops == null && disk.write_iops == null) {
    return "-";
  }
  const read = disk.read_iops == null ? "-" : Number(disk.read_iops).toFixed(1);
  const write = disk.write_iops == null ? "-" : Number(disk.write_iops).toFixed(1);
  return `${read} / ${write}`;
}

/** 使用率分档色（容量条配色，与列表迷你条同口径） */
function usageLevel(value: number): string {
  if (value > 85) {
    return "danger";
  }
  if (value > 60) {
    return "warning";
  }
  return "ok";
}

/** 磁盘容量百分比：总量为 0 时按 0 处理 */
function diskPct(disk: NonNullable<RawMetrics["disks"]>[number]): number {
  const total = Number(disk.total) || 0;
  const used = Number(disk.used) || 0;
  return total > 0 ? Math.min(100, (used / total) * 100) : 0;
}

/** 温度/风扇行文案：类型（i18n 名）+ 值（℃ 或 RPM） */
function sensorText(sensor: NonNullable<RawMetrics["sensors"]>[number]): {
  kindLabel: string;
  value: string;
} {
  const kind = String(sensor.kind || "").toLowerCase();
  let kindLabel = kind || t("common.unknown");
  if (kind === "temp") {
    kindLabel = t("agents.sensor_temp");
  } else if (kind === "fan") {
    kindLabel = t("agents.sensor_fan");
  }
  let value: string;
  if (sensor.value == null) {
    value = "-";
  } else if (kind === "temp") {
    value = `${Number(sensor.value).toFixed(1)} ℃`;
  } else if (kind === "fan") {
    value = `${Math.round(Number(sensor.value))} RPM`;
  } else {
    value = String(sensor.value);
  }
  return { kindLabel, value };
}

/** 曲线 min/max 标注数值：整数去小数，其余保留一位 */
function formatChartNum(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

/** 按指标拆分 24h 历史序列（cpu/mem/disk/temp/rx/tx 均可能为 null） */
const sparkSeries = computed(() => {
  const items = historyQuery.data.value ?? [];
  const column = (key: HistoryKey) => items.map((item) => item?.[key] ?? null);
  return {
    cpu: [{ values: column("cpu"), lineClass: "agent-chart-line-cpu" }],
    memory: [{ values: column("mem"), lineClass: "agent-chart-line-memory" }],
    disk: [{ values: column("disk"), lineClass: "agent-chart-line-disk" }],
    temp: [{ values: column("temp"), lineClass: "agent-chart-line-temp" }],
    net: [
      { values: column("rx_bps"), lineClass: "agent-chart-line-rx" },
      { values: column("tx_bps"), lineClass: "agent-chart-line-tx" }
    ]
  };
});

/** 百分比/温度卡的数值标注（单位后缀） */
function formatUnit(unit: string): (value: number) => string {
  return (value) => formatChartNum(value) + unit;
}
</script>

<template>
  <ModalShell size="lg" @close="emit('close')">
    <template #title>
      {{ t("agents.detail_title") }}<span v-if="agent?.hostname" class="agent-title-host"> · {{ agent.hostname }}</span>
    </template>

    <p v-if="detailQuery.isPending.value" class="agent-detail-empty">{{ t("common.loading") }}</p>
    <p v-else-if="detailQuery.isError.value" class="agent-detail-empty">{{ detailError }}</p>
    <template v-else-if="agent">
      <!-- 头部概要：状态徽章 + IP -->
      <div class="agent-detail-summary">
        <span class="status-badge agent-detail-badge" :class="statusMeta?.badge">{{ statusText }}</span>
        <span
          v-if="agent.source === 'snmp' || agent.source === 'ipmi'"
          class="agent-source-badge"
        >{{ agent.source === "ipmi" ? t("agents.source_ipmi") : t("agents.source_snmp") }}</span>
        <span class="agent-detail-ip">{{ agent.ip || "-" }}</span>
      </div>

      <!-- 分组卡片网格：系统信息 / CPU / 内存 / 磁盘 / 网络 / 温度风扇 -->
      <div class="agent-detail-groups">
        <section class="agent-detail-card">
          <h4 class="agent-detail-card-title">{{ t("agents.group_system") }}</h4>
          <dl class="agent-kv">
            <dt>{{ t("agents.label_os") }}</dt><dd>{{ raw.system?.os || agent.os || "-" }}</dd>
            <dt>{{ t("agents.label_kernel") }}</dt><dd>{{ raw.system?.kernel || "-" }}</dd>
            <dt>{{ t("agents.label_arch") }}</dt><dd>{{ raw.system?.arch || agent.arch || "-" }}</dd>
            <dt>{{ t("agents.label_uptime") }}</dt><dd>{{ formatUptime(raw.system?.uptime_secs ?? agent.uptime_secs) }}</dd>
            <dt>{{ t("agents.label_version") }}</dt><dd>{{ agent.agent_version || "-" }}</dd>
            <dt>{{ t("agents.label_machine_id") }}</dt><dd>{{ raw.machine_id || "-" }}</dd>
            <dt>{{ t("agents.label_collected_at") }}</dt><dd>{{ raw.collected_at ? formatDateTime(raw.collected_at) : "-" }}</dd>
          </dl>
        </section>

        <section v-if="isIpmi" class="agent-detail-card agent-detail-card-wide">
          <h4 class="agent-detail-card-title">{{ t("agents.group_ipmi") }}<span v-if="liveFresh && live?.source === 'ipmi'" class="agent-live-badge">{{ t("agents.live_realtime") }}</span></h4>
          <dl class="agent-kv">
            <dt>{{ t("agents.ipmi_power") }}</dt>
            <dd>
              <span v-if="ipmiPowerOn != null" class="status-badge" :class="ipmiPowerOn ? 'status-up' : 'status-down'">
                {{ ipmiPowerOn ? t("agents.ipmi_power_on") : t("agents.ipmi_power_off") }}
              </span>
              <span v-else>-</span>
            </dd>
            <dt>{{ t("agents.ipmi_vendor") }}</dt><dd>{{ agent.os || "-" }}</dd>
            <dt>{{ t("agents.ipmi_fw") }}</dt><dd>{{ ipmiState.fw_revision || "-" }}</dd>
          </dl>
          <div class="agent-rows">
            <p v-if="!ipmiHasSensors" class="agent-detail-empty">{{ t("common.no_data") }}</p>
            <template v-else>
              <div v-for="(s, index) in ipmiTemps" :key="`t-${index}`" class="agent-sensor-row">
                <span class="agent-sensor-label">{{ s.name || "-" }}</span>
                <span class="agent-sensor-kind">{{ t("agents.sensor_temp") }}</span>
                <span class="agent-sensor-value">{{ formatIpmiSensor(s) }}</span>
              </div>
              <div v-for="(s, index) in ipmiFans" :key="`f-${index}`" class="agent-sensor-row">
                <span class="agent-sensor-label">{{ s.name || "-" }}</span>
                <span class="agent-sensor-kind">{{ t("agents.sensor_fan") }}</span>
                <span class="agent-sensor-value">{{ formatIpmiSensor(s) }}</span>
              </div>
              <div v-for="(s, index) in ipmiVolts" :key="`v-${index}`" class="agent-sensor-row">
                <span class="agent-sensor-label">{{ s.name || "-" }}</span>
                <span class="agent-sensor-kind">{{ t("agents.sensor_volt") }}</span>
                <span class="agent-sensor-value">{{ formatIpmiSensor(s) }}</span>
              </div>
            </template>
          </div>
        </section>

        <section v-if="!isIpmi" class="agent-detail-card">
          <h4 class="agent-detail-card-title">{{ t("agents.group_cpu") }}<span v-if="liveCpu != null" class="agent-live-badge">{{ t("agents.live_realtime") }}</span></h4>
          <div class="agent-cpu-big">{{ displayCpu == null ? "-" : `${Number(displayCpu).toFixed(1)}%` }}</div>
          <dl class="agent-kv">
            <dt>{{ t("agents.label_load") }}</dt><dd>{{ formatLoads(raw.cpu || {}) }}</dd>
            <dt>{{ t("agents.label_cores") }}</dt><dd>{{ raw.cpu?.cores ?? "-" }}</dd>
          </dl>
        </section>

        <section v-if="!isIpmi" class="agent-detail-card">
          <h4 class="agent-detail-card-title">{{ t("agents.group_memory") }}<span v-if="liveMemPct != null" class="agent-live-badge">{{ t("agents.live_realtime") }}</span></h4>
          <div class="agent-mem-bar">
            <AgentUsageBar :value="displayMemPct" />
          </div>
          <dl class="agent-kv">
            <dt>{{ t("agents.label_total") }}</dt><dd>{{ formatBytes(displayMemTotal) }}</dd>
            <dt>{{ t("agents.label_used") }}</dt><dd>{{ formatBytes(displayMemUsed) }}</dd>
            <dt>{{ t("agents.label_swap") }}</dt><dd>{{ formatSwap(raw.memory || {}) }}</dd>
          </dl>
        </section>

        <section v-if="!isIpmi" class="agent-detail-card agent-detail-card-wide">
          <h4 class="agent-detail-card-title">{{ t("agents.group_disk") }}</h4>
          <div class="agent-rows">
            <p v-if="!(raw.disks ?? []).length" class="agent-detail-empty">{{ t("common.no_data") }}</p>
            <div v-for="(disk, index) in raw.disks" v-else :key="index" class="agent-disk-row">
              <span class="agent-disk-name" :title="disk.mount || ''">{{ disk.device || "-" }}</span>
              <span class="agent-disk-mount">{{ disk.mount || "-" }}</span>
              <span class="agent-bar">
                <span class="agent-bar-fill" :class="`agent-usage-${usageLevel(diskPct(disk))}`" :style="{ width: diskPct(disk).toFixed(1) + '%' }"></span>
              </span>
              <span class="agent-disk-cap">{{ formatBytes(disk.used) }} / {{ formatBytes(disk.total) }}</span>
              <span class="agent-disk-iops">{{ t("agents.label_iops") }}: {{ formatIops(disk) }}</span>
              <span class="agent-disk-util">{{ t("agents.label_util") }}: {{ disk.util_pct == null ? "-" : `${Number(disk.util_pct).toFixed(1)}%` }}</span>
            </div>
          </div>
        </section>

        <section v-if="!isIpmi" class="agent-detail-card">
          <h4 class="agent-detail-card-title">{{ t("agents.group_network") }}<span v-if="liveFresh" class="agent-live-badge">{{ t("agents.live_realtime") }}</span></h4>
          <!-- SNMP：每台设备只采一个端口（默认首端口），速率取秒级实时快照 -->
          <template v-if="agent.source === 'snmp'">
            <dl class="agent-kv">
              <dt>{{ t("agents.monitor_port_label") }}</dt>
              <dd>
                <select
                  v-model="selectedPort"
                  class="agent-monitor-select"
                  :disabled="savingPort || monitorPortQuery.isPending.value"
                  @change="onPortChange"
                >
                  <option value="">{{ t("agents.monitor_port_default") }}</option>
                  <option v-for="port in monitorPortOptions" :key="port.name" :value="port.name">
                    {{ port.name }} · {{ port.status }}
                  </option>
                </select>
              </dd>
              <dt>{{ t("agents.monitor_port_traffic") }}</dt>
              <dd v-if="liveFresh">
                <span class="agent-live-rx">↓ {{ formatBps(live?.rx_bps) }}</span>
                <span class="agent-live-tx">↑ {{ formatBps(live?.tx_bps) }}</span>
              </dd>
              <dd v-else>{{ t("agents.monitor_port_waiting") }}</dd>
              <dt>{{ t("agents.monitor_port_active") }}</dt>
              <dd>{{ live?.monitor_if || "-" }}</dd>
              <dt>{{ t("agents.monitor_port_links") }}</dt>
              <dd>{{ live ? `${live.ports_up} / ${live.ports_total}` : "-" }}</dd>
              <dt>{{ t("agents.live_updated_at") }}</dt>
              <dd>{{ liveCollectedAt ? formatDateTime(liveCollectedAt) : "-" }}</dd>
            </dl>
            <p v-if="liveLost" class="agent-detail-hint">{{ t("agents.live_stream_lost") }}</p>
            <p class="agent-detail-hint">{{ t("agents.monitor_port_hint") }}</p>
          </template>
          <div v-else class="agent-rows">
            <p v-if="!(raw.nets ?? []).length" class="agent-detail-empty">{{ t("common.no_data") }}</p>
            <table v-else class="agent-nets-table">
              <thead>
                <tr>
                  <th>{{ t("agents.nets_table.iface") }}</th>
                  <th class="col-center">{{ t("agents.nets_table.status") }}</th>
                  <th class="col-center">{{ t("agents.nets_table.speed") }}</th>
                  <th class="col-center">↓ {{ t("agents.nets_table.rx") }}</th>
                  <th class="col-center">↑ {{ t("agents.nets_table.tx") }}</th>
                  <th class="col-center">{{ t("agents.nets_table.errors") }}</th>
                  <th class="col-center">{{ t("agents.nets_table.discards") }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="(net, index) in raw.nets" :key="index">
                  <td>{{ net.iface || "-" }}</td>
                  <td class="col-center">
                    <span class="status-badge" :class="netStatusClass(net.status)">{{ net.status || "-" }}</span>
                  </td>
                  <td class="col-center">{{ net.speed_mbps != null ? `${net.speed_mbps} Mbps` : "-" }}</td>
                  <td class="col-center">{{ formatBps(net.rx_bps) }}</td>
                  <td class="col-center">{{ formatBps(net.tx_bps) }}</td>
                  <td class="col-center">{{ netErrDelta(net.errors) }}</td>
                  <td class="col-center">{{ netDiscDelta(net.errors) }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </section>

        <section v-if="!isIpmi" class="agent-detail-card">
          <h4 class="agent-detail-card-title">{{ t("agents.group_sensors") }}</h4>
          <div class="agent-rows">
            <div v-if="liveTemp != null" class="agent-sensor-row">
              <span class="agent-sensor-label">{{ t("agents.sensor_temp") }}</span>
              <span class="agent-sensor-kind">{{ t("agents.live_realtime") }}</span>
              <span class="agent-sensor-value">{{ liveTemp.toFixed(1) }} ℃</span>
            </div>
            <p
              v-if="!(raw.sensors ?? []).length && liveTemp == null"
              class="agent-detail-empty"
            >
              {{ t("common.no_data") }}
            </p>
            <div v-for="(sensor, index) in raw.sensors" :key="index" class="agent-sensor-row">
              <span class="agent-sensor-label">{{ sensor.label || "-" }}</span>
              <span class="agent-sensor-kind">{{ sensorText(sensor).kindLabel }}</span>
              <span class="agent-sensor-value">{{ sensorText(sensor).value }}</span>
            </div>
          </div>
        </section>
      </div>

      <!-- 近 24 小时迷你曲线（2×2，SVG 自绘） -->
      <h4 class="agent-spark-heading">{{ t("agents.chart_heading") }}</h4>
      <div class="agent-spark-grid">
        <div v-if="!isIpmi" class="agent-spark-card">
          <h5>{{ t("agents.chart_cpu") }}</h5>
          <AgentSparkline :series="sparkSeries.cpu" :format-value="formatUnit('%')" :aria-label="t('agents.chart_cpu')" />
        </div>
        <div v-if="!isIpmi" class="agent-spark-card">
          <h5>{{ t("agents.chart_memory") }}</h5>
          <AgentSparkline :series="sparkSeries.memory" :format-value="formatUnit('%')" :aria-label="t('agents.chart_memory')" />
        </div>
        <div v-if="!isIpmi" class="agent-spark-card">
          <h5>{{ t("agents.chart_disk") }}</h5>
          <AgentSparkline :series="sparkSeries.disk" :format-value="formatUnit('%')" :aria-label="t('agents.chart_disk')" />
        </div>
        <div class="agent-spark-card">
          <h5>{{ t("agents.chart_temp") }}</h5>
          <AgentSparkline :series="sparkSeries.temp" :format-value="formatUnit('℃')" :aria-label="t('agents.chart_temp')" />
        </div>
        <div v-if="!isIpmi" class="agent-spark-card">
          <h5 class="agent-spark-title">
            <span>{{ t("agents.chart_net") }}</span>
            <span class="agent-net-legend">
              <i class="agent-legend-dot agent-legend-rx"></i><span>{{ t("agents.chart_net_rx") }}</span>
              <i class="agent-legend-dot agent-legend-tx"></i><span>{{ t("agents.chart_net_tx") }}</span>
            </span>
          </h5>
          <AgentSparkline :series="sparkSeries.net" :format-value="formatBps" :aria-label="t('agents.chart_net')" />
        </div>
      </div>
    </template>
  </ModalShell>
</template>
