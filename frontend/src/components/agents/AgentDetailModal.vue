<script setup lang="ts">
/**
 * 主机详情弹窗（移植自旧版 modals/agents/agent-detail-modal.html
 * + agents.js fillAgentDetail/loadAgentHistory/renderAllCharts）：
 * 分组卡片（系统/CPU/内存/磁盘/网络/温度风扇）+ 近 24h SVG 迷你趋势五卡。
 * 详情与历史各走一条 useQuery；曲线为声明式渲染，无需旧版的
 * 延迟测量时序（SVG 按 viewBox 等比缩放，容器宽度不影响绘制）。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../../api/client";
import { formatDateTime } from "../../utils/formatter";
import ModalShell from "../ModalShell.vue";
import AgentSparkline from "./AgentSparkline.vue";
import AgentUsageBar from "./AgentUsageBar.vue";

/** 主机详情载荷（GET /api/agents/{id}，仅声明本弹窗消费的字段） */
interface AgentDetail {
  status: string;
  hostname: string | null;
  ip: string | null;
  os: string | null;
  arch: string | null;
  agent_version: string | null;
  uptime_secs: number | null;
  mem_usage_pct: number | null;
  raw_metrics: RawMetrics | null;
}

/** raw_metrics 上报 JSON 的消费字段（全部可选：旧版 agent 上报可能缺键） */
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
    errors?: number | null;
  }> | null;
  sensors?: Array<{
    label?: string | null;
    kind?: string | null;
    value?: number | string | null;
  }> | null;
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

const raw = computed(() => agent.value?.raw_metrics || {});

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

/** 网络速率：bps → bps/Kbps/Mbps/Gbps */
function formatBps(bps: unknown): string {
  const value = Number(bps);
  if (!Number.isFinite(value)) {
    return "-";
  }
  if (value >= 1e9) {
    return `${(value / 1e9).toFixed(2)} Gbps`;
  }
  if (value >= 1e6) {
    return `${(value / 1e6).toFixed(2)} Mbps`;
  }
  if (value >= 1e3) {
    return `${(value / 1e3).toFixed(1)} Kbps`;
  }
  return `${Math.round(value)} bps`;
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

        <section class="agent-detail-card">
          <h4 class="agent-detail-card-title">{{ t("agents.group_cpu") }}</h4>
          <div class="agent-cpu-big">{{ raw.cpu?.usage_pct == null ? "-" : `${Number(raw.cpu.usage_pct).toFixed(1)}%` }}</div>
          <dl class="agent-kv">
            <dt>{{ t("agents.label_load") }}</dt><dd>{{ formatLoads(raw.cpu || {}) }}</dd>
            <dt>{{ t("agents.label_cores") }}</dt><dd>{{ raw.cpu?.cores ?? "-" }}</dd>
          </dl>
        </section>

        <section class="agent-detail-card">
          <h4 class="agent-detail-card-title">{{ t("agents.group_memory") }}</h4>
          <div class="agent-mem-bar">
            <AgentUsageBar :value="agent.mem_usage_pct" />
          </div>
          <dl class="agent-kv">
            <dt>{{ t("agents.label_total") }}</dt><dd>{{ formatBytes(raw.memory?.total) }}</dd>
            <dt>{{ t("agents.label_used") }}</dt><dd>{{ formatBytes(raw.memory?.used) }}</dd>
            <dt>{{ t("agents.label_swap") }}</dt><dd>{{ formatSwap(raw.memory || {}) }}</dd>
          </dl>
        </section>

        <section class="agent-detail-card agent-detail-card-wide">
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

        <section class="agent-detail-card">
          <h4 class="agent-detail-card-title">{{ t("agents.group_network") }}</h4>
          <div class="agent-rows">
            <p v-if="!(raw.nets ?? []).length" class="agent-detail-empty">{{ t("common.no_data") }}</p>
            <div v-for="(net, index) in raw.nets" v-else :key="index" class="agent-net-row">
              <span class="agent-net-iface">{{ net.iface || "-" }}</span>
              <span class="agent-net-rate">↓ {{ formatBps(net.rx_bps) }} · ↑ {{ formatBps(net.tx_bps) }}</span>
              <span class="agent-net-errors">{{ t("agents.label_errors") }}: {{ net.errors ?? 0 }}</span>
            </div>
          </div>
        </section>

        <section class="agent-detail-card">
          <h4 class="agent-detail-card-title">{{ t("agents.group_sensors") }}</h4>
          <div class="agent-rows">
            <p v-if="!(raw.sensors ?? []).length" class="agent-detail-empty">{{ t("common.no_data") }}</p>
            <div v-for="(sensor, index) in raw.sensors" v-else :key="index" class="agent-sensor-row">
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
        <div class="agent-spark-card">
          <h5>{{ t("agents.chart_cpu") }}</h5>
          <AgentSparkline :series="sparkSeries.cpu" :format-value="formatUnit('%')" :aria-label="t('agents.chart_cpu')" />
        </div>
        <div class="agent-spark-card">
          <h5>{{ t("agents.chart_memory") }}</h5>
          <AgentSparkline :series="sparkSeries.memory" :format-value="formatUnit('%')" :aria-label="t('agents.chart_memory')" />
        </div>
        <div class="agent-spark-card">
          <h5>{{ t("agents.chart_disk") }}</h5>
          <AgentSparkline :series="sparkSeries.disk" :format-value="formatUnit('%')" :aria-label="t('agents.chart_disk')" />
        </div>
        <div class="agent-spark-card">
          <h5>{{ t("agents.chart_temp") }}</h5>
          <AgentSparkline :series="sparkSeries.temp" :format-value="formatUnit('℃')" :aria-label="t('agents.chart_temp')" />
        </div>
        <div class="agent-spark-card">
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
