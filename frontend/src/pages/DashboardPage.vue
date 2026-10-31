<script setup lang="ts">
/**
 * 仪表盘：5 张统计卡（dashboard-stats 单端点）+ 6 张榜单卡（5 个分页接口 +
 * 主机负载）。主机负载卡同时消费 agent 快照（top-metrics）与 SNMP 实时
 * 快照（SSE），CPU/内存/温度/流量四页签 10s 轮播（悬停暂停、手动切换
 * 后永久停止）。dashboard-stats 失败后不做计数兜底：
 * 兜底与主端点同源依赖数据库，失败场景下同样不可用，仅会以 0 掩盖错误。
 * 查询统一 TanStack Query（staleTime 60s）。
 */
import { computed, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../api/client";
import { useAuthStore } from "../stores/auth";
import {
  getActionIcon,
  getOperationTypeText,
  getRoomTypeName,
  getResourceTypeText,
  getStatusText,
  formatBps,
  formatTime
} from "../utils/formatter";
import { useLiveAgents } from "../composables/useLiveAgents";
import type { AlertThresholds } from "../types/api/generated/AlertThresholds";
import type { LiveSummaryRow } from "../types/api/generated/LiveSummaryRow";
import type { Paged } from "../types/api";
import type { CabinetWithNetworks } from "../types/api/generated/CabinetWithNetworks";
import type { DashboardStats } from "../types/api/generated/DashboardStats";
import type { IpDetailWithNames } from "../types/api/generated/IpDetailWithNames";
import type { Network } from "../types/api/generated/Network";
import type { OperationLog } from "../types/api/generated/OperationLog";
import type { RoomWithNetworks } from "../types/api/generated/RoomWithNetworks";

const { t } = useI18n();
const router = useRouter();
const auth = useAuthStore();

const CACHE_TTL = 60 * 1000;

/** 榜单查询工厂：分页接口归一为 items 数组（data 归一为非空数组以保持模板旧语义） */
function useTopList<T>(tag: string, url: string) {
  const query = useQuery({
    queryKey: [tag],
    queryFn: async (): Promise<T[]> => {
      const result = await apiGet<Paged<T>>(url);
      if (!result.success) {
        throw new Error(result.message || t("common.load_failed"));
      }
      return Array.isArray(result.data?.items) ? result.data.items : [];
    },
    staleTime: CACHE_TTL
  });
  // query 的 data 在加载/失败期为 undefined，收窄为空数组，模板取值逻辑不变
  return { ...query, data: computed(() => query.data.value ?? []) };
}

// 统计卡

const statsQuery = useQuery({
  queryKey: ["dashboard-stats"],
  queryFn: async (): Promise<DashboardStats | null> => {
    const result = await apiGet<DashboardStats>("/api/system/dashboard-stats");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  staleTime: CACHE_TTL
});

const statValues = computed(() => {
  // 加载中展示 0，失败如实展示 "-"
  if (statsQuery.isError.value) {
    return { networks: "-", regions: "-", ips: "-", active: "-", ops: "-" };
  }
  const data = statsQuery.data.value;
  return {
    networks: data?.networks?.networks || 0,
    regions: data?.networks?.regions || 0,
    ips: data?.ips?.total || 0,
    active: data?.ips?.active || 0,
    ops: data?.activity?.operations_24h || 0
  };
});

// 榜单卡

// 网络榜单行：联查附加的区域名称键未入 generated Network
type NetworkTopItem = Network & { network_region_name?: string | null };

const networksQuery = useTopList<NetworkTopItem>("dashboard-top-networks", "/api/resources/networks?page_size=5");
const ipsQuery = useTopList<IpDetailWithNames>("dashboard-top-ips", "/api/resources/ip?page_size=5");
const roomsQuery = useTopList<RoomWithNetworks>("dashboard-top-rooms", "/api/resources/rooms?page_size=5");
const cabinetsQuery = useTopList<CabinetWithNetworks>("dashboard-top-cabinets", "/api/resources/cabinets?page_size=5");
const logsQuery = useTopList<OperationLog>("dashboard-top-logs", "/api/logs/operation?page_size=5");

/** IP 榜单条目的归属行：区域 / 子网 */
function ipRegionText(ip: IpDetailWithNames): string {
  return [ip.network_region, ip.network_name].filter(Boolean).join(" / ") || "-";
}

/** 最近动态条目的操作人行 */
function logMetaText(log: OperationLog): string {
  return `${log.username || "-"} · ${formatTime(log.created_at)}`;
}

// 主机负载卡片（agent 快照 + SNMP 实时快照）

/** 主机负载条目（/api/agents/top-metrics 行：主机名/IP/标签/指标值） */
interface HostMetricItem {
  hostname: string | null;
  ip: string | null;
  label: string | null;
  value: number | string | null;
}

/** 主机负载行：合并 agent 快照与 SNMP 实时快照后的统一视图模型 */
interface HostLoadItem {
  name: string;
  meta: string;
  /** IP 用于跨源去重（agent 行与实时行指向同一设备时取实时值） */
  ip: string | null;
  value: number;
  /** 实时快照超过 30s 未更新（agent 快照本就是分钟级，不标记） */
  stale: boolean;
  /** 达到告警阈值（仅配置了该指标阈值时） */
  abnormal: boolean;
  /** 来源实时行（流量页签渲染读/写速率，agent 行无此字段） */
  live: LiveSummaryRow | null;
}

/** 指标键 → 榜单数组（端点按需返回，未启用键可能缺失） */
type HostMetrics = Partial<Record<"cpu" | "memory" | "temperature", HostMetricItem[]>>;

/** 页签指标键：前三类同时来自 agent 快照与 SNMP 实时快照，流量仅 SNMP 实时提供 */
const HOST_METRIC_KEYS = ["cpu", "memory", "temperature", "traffic"] as const;
type HostMetricKey = (typeof HOST_METRIC_KEYS)[number];
const HOST_METRIC_ROTATE_MS = 10 * 1000;
const HOST_TOP_LIMIT = 5;
/** 快照超过该秒数视为陈旧（采样 5s + 推送节拍 2s 的 3 倍余量） */
const LIVE_STALE_SECS = 30;

const hostMetricsQuery = useQuery({
  queryKey: ["dashboard-host-metrics"],
  queryFn: async (): Promise<HostMetrics | null> => {
    const result = await apiGet<HostMetrics>("/api/agents/top-metrics?limit=5");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  // 主机监控分区不可见角色不请求（该端点仅 admin/secadmin 可访问）
  enabled: computed(() => !auth.hiddenSections.includes("agents")),
  staleTime: CACHE_TTL
});

const { devices: liveDevices, connected: liveConnected } = useLiveAgents(
  !auth.hiddenSections.includes("agents")
);
const hostMetricActive = ref<HostMetricKey>("cpu");
const hostOnlyAbnormal = ref(false);

// 阈值复用主机告警配置（CPU/内存/温度；流量无阈值）
const thresholdsQuery = useQuery({
  queryKey: ["agent-alert-thresholds"],
  queryFn: async (): Promise<AlertThresholds | null> => {
    const result = await apiGet<AlertThresholds>("/api/agents/alert-thresholds");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => !auth.hiddenSections.includes("agents")),
  staleTime: CACHE_TTL
});

function liveMetricValue(row: LiveSummaryRow, metric: HostMetricKey): number | null {
  switch (metric) {
    case "cpu":
      return row.cpu_usage_pct;
    case "memory":
      return row.mem_usage_pct;
    case "temperature":
      return row.temperature_c;
    case "traffic":
      return row.rx_bps == null && row.tx_bps == null
        ? null
        : (row.rx_bps ?? 0) + (row.tx_bps ?? 0);
  }
}

function hostThreshold(metric: HostMetricKey): number | null {
  const thresholds = thresholdsQuery.data.value;
  if (!thresholds?.enabled) {
    return null;
  }
  switch (metric) {
    case "cpu":
      return thresholds.cpu_pct;
    case "memory":
      return thresholds.mem_pct;
    case "temperature":
      return thresholds.temp_c;
    case "traffic":
      return null;
  }
}

/** 当前页签是否配置了告警阈值（流量无阈值） */
const hostHasThreshold = computed(() => hostThreshold(hostMetricActive.value) != null);

/** 实时快照按 IP 建索引：同 IP 取当前页签有值的最优来源行（ipmi > agent > snmp）覆盖分钟级快照 */
const SOURCE_RANK: Record<string, number> = { ipmi: 0, agent: 1, snmp: 2 };
const liveByIp = computed(() => {
  const metric = hostMetricActive.value;
  const map = new Map<string, LiveSummaryRow>();
  const rows = [...liveDevices.value].sort(
    (a, b) => (SOURCE_RANK[a.source] ?? 9) - (SOURCE_RANK[b.source] ?? 9),
  );
  for (const row of rows) {
    if (!row.ip || liveMetricValue(row, metric) == null) {
      continue;
    }
    if (!map.has(row.ip)) {
      map.set(row.ip, row);
    }
  }
  return map;
});

const hostLoadItems = computed<HostLoadItem[]>(() => {
  const metric = hostMetricActive.value;
  const threshold = hostThreshold(metric);
  const now = Date.now();
  const items: HostLoadItem[] = [];

  // agent 快照：流量页签无此来源（只有 SNMP 实时推送带速率）
  const restItems = metric === "traffic" ? [] : (hostMetricsQuery.data.value?.[metric] ?? []);
  for (const row of restItems) {
    const value = Number(row.value);
    if (!Number.isFinite(value) || (row.ip && liveByIp.value.has(row.ip))) {
      continue;
    }
    items.push({
      name: row.hostname || row.ip || "-",
      meta: [row.label, row.ip].filter(Boolean).join(" · ") || "-",
      ip: row.ip,
      value,
      stale: false,
      abnormal: threshold != null && value >= threshold,
      live: null
    });
  }

  for (const row of liveDevices.value) {
    const value = liveMetricValue(row, metric);
    if (value == null) {
      continue;
    }
    // 同 IP 存在更优来源的实时行则跳过（与后端 SOURCE_RANK_SQL 口径对齐）
    if (row.ip && liveByIp.value.get(row.ip) !== row) {
      continue;
    }
    const collected = Date.parse(row.collected_at);
    // IPMI 行无 CPU/内存/流量语义，仅温度页签带值；meta 前置来源便于区分 BMC 视角
    const sourceLabel = row.source === "ipmi" ? t("agents.source_ipmi") : null;
    items.push({
      name: row.device_name || row.ip || "-",
      meta: [sourceLabel, t("dashboard.live_source"), row.ip, row.vendor].filter(Boolean).join(" · "),
      ip: row.ip || null,
      value,
      stale: !Number.isFinite(collected) || now - collected > LIVE_STALE_SECS * 1000,
      abnormal: threshold != null && value >= threshold,
      live: row
    });
  }

  const filtered = hostOnlyAbnormal.value ? items.filter((item) => item.abnormal) : items;
  filtered.sort((a, b) => b.value - a.value);
  return filtered.slice(0, HOST_TOP_LIMIT);
});

const hostEmptyKey = computed(() => {
  // 快照与实时来源均为空且实时连接未建立：等待首帧推送
  const restEmpty = !hostMetricsQuery.data.value;
  if (liveDevices.value.length === 0 && !liveConnected.value && restEmpty) {
    return "dashboard.live_waiting";
  }
  if (hostOnlyAbnormal.value) {
    return "dashboard.no_live_abnormal";
  }
  return hostMetricActive.value === "traffic" ? "dashboard.no_live_data" : "dashboard.no_host_metrics_data";
});

function selectHostMetric(metric: HostMetricKey): void {
  hostMetricActive.value = metric;
  // 流量无阈值配置：切走时关闭「仅异常」避免筛出空列表
  if (hostThreshold(metric) == null) {
    hostOnlyAbnormal.value = false;
  }
}

// 手动切换页签后置 true：用户意图优先，自动轮播永久停止
let hostMetricManual = false;
let rotateTimer: number | undefined;

function startRotation() {
  if (rotateTimer || hostMetricManual) {
    return;
  }
  rotateTimer = setInterval(() => {
    const index = HOST_METRIC_KEYS.indexOf(hostMetricActive.value);
    // 模运算索引恒在键数组界内，收窄 noUncheckedIndexedAccess 的 undefined 分支
    hostMetricActive.value = HOST_METRIC_KEYS[(index + 1) % HOST_METRIC_KEYS.length] as HostMetricKey;
  }, HOST_METRIC_ROTATE_MS);
}

function stopRotation() {
  if (rotateTimer) {
    clearInterval(rotateTimer);
    rotateTimer = undefined;
  }
}

function toggleHostMetric(metric: HostMetricKey) {
  hostMetricManual = true;
  stopRotation();
  selectHostMetric(metric);
}

function onCardMouseEnter() {
  stopRotation();
}

function onCardMouseLeave() {
  if (!hostMetricManual) {
    startRotation();
  }
}

startRotation();
onUnmounted(stopRotation);

/** 使用率迷你条分档（复用 agent-usage 约定：>85 红 >60 橙） */
function usageLevel(value: number | string | null): string {
  const num = Number(value);
  if (!Number.isFinite(num)) {
    return "none";
  }
  if (num > 85) {
    return "danger";
  }
  if (num > 60) {
    return "warning";
  }
  return "ok";
}

function usageWidth(value: number | string | null): string {
  const num = Number(value);
  if (!Number.isFinite(num)) {
    return "0%";
  }
  return `${Math.min(100, Math.max(0, num)).toFixed(1)}%`;
}

function usageNum(value: number | string | null): string {
  const num = Number(value);
  return Number.isFinite(num) ? Math.min(100, Math.max(0, num)).toFixed(1) : "-";
}

/** 温度榜值：保留一位小数（℃） */
function tempText(value: number | string | null): string {
  const num = Number(value);
  return Number.isFinite(num) ? `${num.toFixed(1)}℃` : "-";
}

/** 流量榜值：读/写速率（bps） */
function trafficText(item: HostLoadItem): string {
  return item.live ? `↓ ${formatBps(item.live.rx_bps)} ↑ ${formatBps(item.live.tx_bps)}` : "-";
}

/**
 * 卡片点击跳转（声明式事件处理）：命中卡片内页签等按钮时不跳转；
 * 携带 tab 的卡片以 query 形式深链，目标分区迁移后按 query 定位页签。
 */
function openSection(event: MouseEvent, name: string, tab?: string): void {
  // 事件目标由卡片内 DOM 触发，按 Element 收窄以使用 closest 守卫
  const target = event.target as Element;
  if (target.closest("a") || target.closest("button")) {
    return;
  }
  router.push(tab ? { name, query: { tab } } : { name });
}
</script>

<template>
  <section id="dashboard" class="content-section active" :aria-label="t('nav.dashboard')">
    <div class="dashboard-stats" role="status" aria-live="polite">
      <article class="stat-card clickable" @click="openSection($event, 'resources', 'networks')">
        <div class="stat-icon">🌐</div>
        <div class="stat-content">
          <h3>{{ t("dashboard.total_networks") }}</h3>
          <div class="stat-value">{{ statValues.networks }}</div>
        </div>
      </article>
      <article class="stat-card clickable" @click="openSection($event, 'resources', 'network-regions')">
        <div class="stat-icon">📍</div>
        <div class="stat-content">
          <h3>{{ t("dashboard.total_network_types") }}</h3>
          <div class="stat-value">{{ statValues.regions }}</div>
        </div>
      </article>
      <!-- ip 区无页签结构，点击直接跳转设备详情页 -->
      <article class="stat-card clickable" @click="openSection($event, 'ip')">
        <div class="stat-icon">🔗</div>
        <div class="stat-content">
          <h3>{{ t("dashboard.total_ip_addresses") }}</h3>
          <div class="stat-value">{{ statValues.ips }}</div>
        </div>
      </article>
      <article class="stat-card stat-success clickable" @click="openSection($event, 'ip')">
        <div class="stat-icon">✅</div>
        <div class="stat-content">
          <h3>{{ t("dashboard.active_ips") }}</h3>
          <div class="stat-value">{{ statValues.active }}</div>
        </div>
      </article>
      <article class="stat-card stat-info clickable" @click="openSection($event, 'logs')">
        <div class="stat-icon">📊</div>
        <div class="stat-content">
          <h3>{{ t("dashboard.today_operations") }}</h3>
          <div class="stat-value">{{ statValues.ops }}</div>
        </div>
      </article>
    </div>

    <div class="dashboard-grid">
      <!-- 主机负载：CPU/内存/温度/流量 四页签轮播，agent 快照与 SNMP 实时
           快照合并展示（同一设备实时值优先），超阈值标红、可只看异常 -->
      <article
        v-if="!auth.hiddenSections.includes('agents')"
        class="dashboard-card host-metrics-card"
        @click="openSection($event, 'agents')"
        @mouseenter="onCardMouseEnter"
        @mouseleave="onCardMouseLeave"
      >
        <header class="dashboard-card-header">
          <h3><span class="card-icon">🖥️</span><span>{{ t("dashboard.host_metrics") }}</span></h3>
          <span class="live-stream-state" :class="{ on: liveConnected }">
            {{ liveConnected ? t("dashboard.live_on") : t("dashboard.live_off") }}
          </span>
        </header>
        <div class="dashboard-card-body host-metrics-body">
          <div class="host-metrics-tabs" role="tablist" :aria-label="t('dashboard.host_metrics')">
            <button
              v-for="metric in HOST_METRIC_KEYS"
              :key="metric"
              type="button"
              class="host-metrics-tab"
              :class="{ active: hostMetricActive === metric }"
              role="tab"
              :aria-selected="hostMetricActive === metric ? 'true' : 'false'"
              @click="toggleHostMetric(metric)"
            >
              {{ t(`dashboard.metric_${metric}`) }}
            </button>
            <label v-if="hostHasThreshold" class="live-abnormal-toggle" @click.stop>
              <input v-model="hostOnlyAbnormal" type="checkbox">
              {{ t("dashboard.only_abnormal") }}
            </label>
          </div>
          <ul class="top-list">
            <!-- 实时快照已到达时先展示，避免分钟级快照加载期挡住实时数据 -->
            <template v-if="hostMetricsQuery.isLoading.value && liveDevices.length === 0">
              <li v-for="n in 3" :key="'sk' + n" class="loading-item"><span class="skeleton"></span></li>
            </template>
            <li v-else-if="hostMetricsQuery.isError.value" class="empty-list-item">{{ hostMetricsQuery.error.value?.message }}</li>
            <li v-else-if="hostLoadItems.length === 0" class="empty-list-item">{{ t(hostEmptyKey) }}</li>
            <template v-else>
              <li
                v-for="(item, index) in hostLoadItems"
                :key="`${item.ip}-${item.name}-${index}`"
                :class="{ 'live-stale': item.stale }"
              >
                <div class="item-name">
                  <span class="item-icon">{{ index + 1 }}</span>
                  <div>
                    <div>{{ item.name }}</div>
                    <div class="item-meta">
                      {{ item.meta }}<template v-if="item.stale"> · {{ t("dashboard.live_stale") }}</template>
                    </div>
                  </div>
                </div>
                <span v-if="hostMetricActive === 'traffic'" class="item-value live-traffic">{{ trafficText(item) }}</span>
                <span
                  v-else-if="hostMetricActive === 'temperature'"
                  class="item-value"
                  :class="{ 'live-abnormal': item.abnormal }"
                >{{ tempText(item.value) }}</span>
                <span v-else class="agent-usage" :class="{ 'live-abnormal': item.abnormal }">
                  <span class="agent-usage-bar">
                    <span class="agent-usage-fill" :class="`agent-usage-${usageLevel(item.value)}`" :style="{ width: usageWidth(item.value) }"></span>
                  </span>
                  <span class="agent-usage-num">{{ usageNum(item.value) }}%</span>
                </span>
              </li>
            </template>
          </ul>
        </div>
      </article>

      <article class="dashboard-card" @click="openSection($event, 'ip')">
        <header class="dashboard-card-header">
          <h3><span class="card-icon">🔗</span><span>{{ t("dashboard.recent_ips") }}</span></h3>
        </header>
        <div class="dashboard-card-body">
          <ul class="top-list">
            <template v-if="ipsQuery.isLoading.value">
              <li v-for="n in 3" :key="'sk' + n" class="loading-item"><span class="skeleton"></span></li>
            </template>
            <li v-else-if="ipsQuery.isError.value" class="empty-list-item">{{ ipsQuery.error.value?.message }}</li>
            <li v-else-if="ipsQuery.data.value.length === 0" class="empty-list-item">{{ t("dashboard.no_ip_data") }}</li>
            <template v-else>
              <li v-for="ip in ipsQuery.data.value" :key="ip.ip_address">
                <div class="item-name">
                  <span class="item-icon">🔗</span>
                  <div>
                    <div>{{ ip.ip_address || "-" }}</div>
                    <div class="item-meta">{{ ip.hostname || ip.device_name || "-" }}</div>
                    <div class="item-meta">{{ ipRegionText(ip) }}</div>
                  </div>
                </div>
                <span class="item-status" :class="`status-${ip.status || 'inactive'}`">{{ getStatusText(ip.status) }}</span>
              </li>
            </template>
          </ul>
        </div>
      </article>

      <article class="dashboard-card" @click="openSection($event, 'logs')">
        <header class="dashboard-card-header">
          <h3><span class="card-icon">📋</span><span>{{ t("dashboard.recent_logs") }}</span></h3>
        </header>
        <div class="dashboard-card-body">
          <ul class="top-list">
            <template v-if="logsQuery.isLoading.value">
              <li v-for="n in 3" :key="'sk' + n" class="loading-item"><span class="skeleton"></span></li>
            </template>
            <li v-else-if="logsQuery.isError.value" class="empty-list-item">{{ logsQuery.error.value?.message }}</li>
            <li v-else-if="logsQuery.data.value.length === 0" class="empty-list-item">{{ t("dashboard.no_log_data") }}</li>
            <template v-else>
              <li v-for="(log, index) in logsQuery.data.value" :key="log.id ?? index">
                <div class="item-name">
                  <span class="item-icon">{{ getActionIcon(log.action) }}</span>
                  <div>
                    <div>{{ getOperationTypeText(log.action) }}</div>
                    <div class="item-meta">{{ logMetaText(log) }}</div>
                  </div>
                </div>
                <span class="item-value">{{ getResourceTypeText(log.resource_type) }}</span>
              </li>
            </template>
          </ul>
        </div>
      </article>

      <article class="dashboard-card" @click="openSection($event, 'resources', 'networks')">
        <header class="dashboard-card-header">
          <h3><span class="card-icon">🌐</span><span>{{ t("dashboard.top_networks") }}</span></h3>
        </header>
        <div class="dashboard-card-body">
          <ul class="top-list">
            <template v-if="networksQuery.isLoading.value">
              <li v-for="n in 3" :key="'sk' + n" class="loading-item"><span class="skeleton"></span></li>
            </template>
            <li v-else-if="networksQuery.isError.value" class="empty-list-item">{{ networksQuery.error.value?.message }}</li>
            <li v-else-if="networksQuery.data.value.length === 0" class="empty-list-item">{{ t("dashboard.no_network_data") }}</li>
            <template v-else>
              <li v-for="network in networksQuery.data.value" :key="network.name">
                <div class="item-name">
                  <span class="item-icon">🌐</span>
                  <div>
                    <div>{{ network.name || "-" }}</div>
                    <div class="item-meta">{{ network.ipv4_cidr || network.ipv6_cidr || "-" }}</div>
                  </div>
                </div>
                <span class="item-value">{{ network.network_region_name || "-" }}</span>
              </li>
            </template>
          </ul>
        </div>
      </article>

      <article class="dashboard-card" @click="openSection($event, 'resources', 'rooms')">
        <header class="dashboard-card-header">
          <h3><span class="card-icon">🏠</span><span>{{ t("dashboard.top_rooms") }}</span></h3>
        </header>
        <div class="dashboard-card-body">
          <ul class="top-list">
            <template v-if="roomsQuery.isLoading.value">
              <li v-for="n in 3" :key="'sk' + n" class="loading-item"><span class="skeleton"></span></li>
            </template>
            <li v-else-if="roomsQuery.isError.value" class="empty-list-item">{{ roomsQuery.error.value?.message }}</li>
            <li v-else-if="roomsQuery.data.value.length === 0" class="empty-list-item">{{ t("dashboard.no_room_data") }}</li>
            <template v-else>
              <li v-for="room in roomsQuery.data.value" :key="room.name">
                <div class="item-name">
                  <span class="item-icon">🏠</span>
                  <div>
                    <div>{{ room.name || "-" }}</div>
                    <div class="item-meta">{{ getRoomTypeName(room.room_type) }}</div>
                  </div>
                </div>
                <span class="item-value">{{ room.workstation_count || 0 }} {{ t("dashboard.unit_workstation") }}</span>
              </li>
            </template>
          </ul>
        </div>
      </article>

      <article class="dashboard-card" @click="openSection($event, 'resources', 'cabinets')">
        <header class="dashboard-card-header">
          <h3><span class="card-icon">🗄️</span><span>{{ t("dashboard.top_cabinets") }}</span></h3>
        </header>
        <div class="dashboard-card-body">
          <ul class="top-list">
            <template v-if="cabinetsQuery.isLoading.value">
              <li v-for="n in 3" :key="'sk' + n" class="loading-item"><span class="skeleton"></span></li>
            </template>
            <li v-else-if="cabinetsQuery.isError.value" class="empty-list-item">{{ cabinetsQuery.error.value?.message }}</li>
            <li v-else-if="cabinetsQuery.data.value.length === 0" class="empty-list-item">{{ t("dashboard.no_cabinet_data") }}</li>
            <template v-else>
              <li v-for="cabinet in cabinetsQuery.data.value" :key="cabinet.name">
                <div class="item-name">
                  <span class="item-icon">🗄️</span>
                  <div>
                    <div>{{ cabinet.name || "-" }}</div>
                    <div class="item-meta">{{ cabinet.room_name || "-" }}</div>
                  </div>
                </div>
                <span class="item-value">{{ cabinet.position_count || 0 }} {{ t("dashboard.unit_position") }}</span>
              </li>
            </template>
          </ul>
        </div>
      </article>

    </div>
  </section>
</template>

<style scoped>
/* 页面 scoped 样式（底色由布局层 main-content.css 的 .main-content 提供）；
 骨架条与 agent-usage 迷你条为本页使用到的动画/进度片段，
 以 scoped 形式内联（unlayered 优先级与原 pages 层语义一致）。*/

.dashboard-stats {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: var(--space-lg);
  margin-bottom: var(--space-lg);
}

.dashboard-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(350px, 1fr));
  gap: var(--space-lg);
  margin-bottom: var(--space-xl);
}

/* 卡片基样式（白底/边框/圆角/阴影）由 components/cards.css 的分组规则提供 */
.dashboard-card {
  transition:
    box-shadow var(--transition-fast),
    transform var(--transition-fast),
    border-color var(--transition-fast);
  cursor: pointer;
}

.dashboard-card:hover {
  box-shadow: var(--shadow-md);
  transform: translateY(-2px);

  /* hover 边框用实色主色：--color-primary-light 是 10% tint，作边框几乎不可见 */
  border-color: var(--color-primary);
}

.dashboard-card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--space-md) var(--space-lg);
  background: #f4fce3;
  border-bottom: 1px solid var(--color-border-light);
}

.dashboard-card-header h3 {
  margin: 0;
  font-size: 0.95rem;
  font-weight: 600;
  color: var(--color-text);
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.card-icon {
  font-size: 1.1rem;
}

.dashboard-card-body {
  padding: var(--space-md) var(--space-lg);
  min-height: 300px;
}

.top-list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.top-list li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-sm) 0;
  border-bottom: 1px solid var(--color-border-light);
  transition:
    background var(--transition-fast),
    margin var(--transition-fast),
    padding var(--transition-fast);
}

.top-list li:last-child {
  border-bottom: none;
}

.top-list li:hover {
  background: var(--color-bg);
  margin: 0 calc(-1 * var(--space-lg));
  padding: var(--space-sm) var(--space-lg);
}

.top-list .item-name {
  font-weight: 500;
  color: var(--color-text);
  font-size: 0.9rem;
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.top-list .item-name .item-icon {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  background: var(--color-primary-light);
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 0.75rem;
  color: var(--color-primary);
}

.top-list .item-meta {
  font-size: 0.75rem;
  color: var(--color-text-muted);
}

.top-list .item-value {
  font-size: 0.85rem;
  color: var(--color-text-secondary);
  font-weight: 500;
}

.top-list .item-status {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 0.75rem;
  font-weight: 500;
}

/* 状态色由 components/badges.css 的 .status-active/.status-inactive/.status-reserved 全局提供 */

/* 主机负载卡片（CPU/内存/温度/流量 页签轮播） */

.host-metrics-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-sm);
  margin-bottom: var(--space-md);
}

.host-metrics-tab {
  flex: 1;
  padding: 0.25rem 0.5rem;
  border: 1px solid var(--color-border-light);
  border-radius: 12px;
  background: transparent;
  color: var(--color-text-secondary);
  font-size: 0.78rem;
  font-weight: 500;
  cursor: pointer;
  transition:
    background var(--transition-fast),
    color var(--transition-fast),
    border-color var(--transition-fast);
}

.host-metrics-tab:hover {
  border-color: var(--color-primary);
  color: var(--color-primary);
}

.host-metrics-tab.active {
  background: var(--color-primary);
  border-color: var(--color-primary);
  color: #fff;
}

/* 实时快照相关：SSE 连接状态 / 仅异常开关 / 超阈值与陈旧样式 */

.live-stream-state {
  display: inline-flex;
  align-items: center;
  gap: 0.3rem;
  font-size: 0.75rem;
  color: var(--color-text-muted);
}

.live-stream-state::before {
  content: "";
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--color-text-muted);
}

.live-stream-state.on {
  color: var(--color-success-text);
}

.live-stream-state.on::before {
  background: var(--color-success);
}

.live-abnormal-toggle {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  font-size: 0.75rem;
  color: var(--color-text-secondary);
  white-space: nowrap;
  cursor: pointer;
}

.live-abnormal {
  color: var(--color-danger);
  font-weight: 600;
}

/* 超阈值时进度条同步标红（阈值可能低于通用 85% 分档） */
.live-abnormal .agent-usage-fill {
  background: var(--color-danger);
}

/* 快照超过 30s 未更新：降低不透明度提示数据陈旧 */
.live-stale {
  opacity: 0.55;
}

.live-traffic {
  font-size: 0.72rem;
  white-space: nowrap;
}

/* 使用率迷你进度条（>85% 红、>60% 橙） */
.agent-usage {
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  min-width: 90px;
}

.agent-usage-bar {
  flex: 1;
  min-width: 46px;
  max-width: 110px;
  height: 6px;
  border-radius: 3px;
  background: var(--color-border-light);
  overflow: hidden;
}

.agent-usage-fill {
  display: block;
  height: 100%;
  border-radius: 3px;
  transition: width 0.3s;
}

.agent-usage-ok {
  background: var(--color-success);
}

.agent-usage-warning {
  background: var(--color-warning);
}

.agent-usage-danger {
  background: var(--color-danger);
}

.agent-usage-num {
  font-size: 0.78rem;
  color: var(--color-text-secondary);
  white-space: nowrap;
}

/* 加载骨架条 shimmer 动画占位 */
.skeleton {
  display: inline-block;
  width: 100%;
  height: 14px;
  background: linear-gradient(90deg, #f1f3f5 25%, #e9ecef 50%, #f1f3f5 75%);
  background-size: 200% 100%;
  animation: skeleton-loading 1.5s infinite;
  border-radius: var(--radius-sm);
}

@keyframes skeleton-loading {
  0% {
    background-position: 200% 0;
  }

  100% {
    background-position: -200% 0;
  }
}

.loading-item {
  padding: var(--space-sm) 0;
}

.empty-list-item {
  text-align: center;
  padding: var(--space-lg);
  color: var(--color-text-muted);
  font-size: 0.9rem;
}

@media (width <= 768px) {
  .dashboard-stats {
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  }

  .dashboard-grid {
    grid-template-columns: 1fr;
  }

  /* 统计卡片压缩（原 responsive.css 组件级规则迁回） */
  .stat-card {
    padding: 1rem;
  }

  .stat-card .stat-icon {
    font-size: 2rem;
  }
}

@media (width <= 480px) {
  /* 超窄屏：统计数字进一步收缩（原 responsive.css 组件级规则迁回） */
  .stat-card h3 {
    font-size: 0.8rem;
  }

  .stat-card .stat-value {
    font-size: 1.5rem;
  }
}
</style>
