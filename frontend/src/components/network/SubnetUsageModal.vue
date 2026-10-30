<script setup lang="ts">
/**
 * 网段使用情况弹窗：
 * IPv4/IPv6 双页签（单栈隐藏切换）；IPv4 统计 8 项 + 状态过滤 + IP 可视化
 * 网格（掩码 <22 不渲染提示；/22·/23 划分 /24 子网页签；块数上限 256）+
 * IP 列表；IPv6 信息网格 + 列表；刷新按钮重拉 IP 数据并按页签回执提示。
 * 并发打开的过期响应由查询键（含 subnetId）天然隔离，数据随键切换。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { IpDetailWithNames } from "../../types/api/generated/IpDetailWithNames";
import type { Network } from "../../types/api/generated/Network";

const props = withDefaults(defineProps<{ subnetId?: string }>(), { subnetId: "" });

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

/** GET /ip 响应形态兼容：分页 items 与裸 data 键两种载荷 */
type IpsResponse = { data?: IpDetailWithNames[]; items?: IpDetailWithNames[] };

// IP 可视化网格最多渲染 256 个地址块
const MAX_DISPLAY_IPS = 256;

// 数据查询

const networkQuery = useQuery({
  queryKey: computed(() => ["network-detail", props.subnetId]),
  enabled: computed(() => !!props.subnetId),
  queryFn: async (): Promise<Network | null> => {
    const result = await apiGet<Network>(`/api/resources/networks/${props.subnetId}`);
    if (!result.success) {
      throw new Error(result.message || t("network.load_failed"));
    }
    return result.data;
  }
});

const ipsQuery = useQuery({
  queryKey: computed(() => ["subnet-ips", props.subnetId]),
  enabled: computed(() => !!props.subnetId),
  queryFn: async (): Promise<IpDetailWithNames[]> => {
    // 按 subnet_id 过滤拉取：全库拉取在 IP 超过 page_size 上限时还会静默缺数据
    const result = await apiGet<IpsResponse | IpDetailWithNames[]>(
      `/api/resources/ip?subnet_id=${props.subnetId}&page_size=1000`
    );
    if (!result.success) {
      throw new Error(result.message || t("ip.load_failed"));
    }
    // 归一化响应形态：分页对象 / 裸数组
    const data = result.data;
    return Array.isArray(data) ? data : (data?.data ?? data?.items ?? []);
  }
});

const network = computed(() => networkQuery.data.value ?? null);
const allNetworkIps = computed(() =>
  Array.isArray(ipsQuery.data.value) ? ipsQuery.data.value : []
);
const isV6 = (ip: IpDetailWithNames) => ip.ip_address?.includes(":");
const ipv4Ips = computed(() => allNetworkIps.value.filter((ip) => !isV6(ip)));
const ipv6Ips = computed(() => allNetworkIps.value.filter((ip) => isV6(ip)));

const hasIPv4 = computed(() => !!network.value?.ipv4_cidr);
const hasIPv6 = computed(() => !!network.value?.ipv6_cidr);

// IPv4/IPv6 页签

const activeTab = ref("ipv4");

// 单栈网段没有切换意义：隐藏按钮组仅保留网段名
const showTabButtons = computed(() => hasIPv4.value && hasIPv6.value);

// IPv4 统计与可视化

/** 计算网段总 IP 数：2^(32-前缀) - 2；/31、/32 按 0 处理 */
function calculateTotalIps(cidr: string | null | undefined): number {
  if (!cidr) {
    return 0;
  }
  const parts = cidr.split("/");
  if (parts.length !== 2) {
    return 0;
  }
  const prefixLength = Number.parseInt(parts[1] ?? "", 10);
  if (Number.isNaN(prefixLength) || prefixLength < 0 || prefixLength > 32) {
    return 0;
  }
  return Math.max(0, 2 ** (32 - prefixLength) - 2);
}

/**
 * 生成可视化网格的 IP 地址列表：CIDR 基址先按前缀掩码取网络地址
 * （基址可能不是网络地址，如 10.0.0.77/24），从网络地址+1 起递增
 * （跳过网络地址），最多 MAX_DISPLAY_IPS 个
 */
function generateIpAddresses(cidr: string | null | undefined): string[] {
  if (!cidr) {
    return [];
  }
  const parts = cidr.split("/");
  if (parts.length !== 2) {
    return [];
  }
  const ip = parts[0] ?? "";
  if (!/^(?:\d{1,3}\.){3}\d{1,3}$/.test(ip)) {
    return [];
  }

  const ipParts = ip.split(".").map(Number);
  const [ip0 = 0, ip1 = 0, ip2 = 0, ip3 = 0] = ipParts;
  const prefix = Number.parseInt(parts[1] ?? "", 10);
  let mask = (0xffffffff << (32 - prefix)) >>> 0;
  if (prefix <= 0) {
    mask = 0;
  } else if (prefix >= 32) {
    mask = 0xffffffff;
  }
  const ipInt = (((ip0 << 24) | (ip1 << 16) | (ip2 << 8) | ip3) >>> 0) >>> 0;
  const networkInt = (ipInt & mask) >>> 0;
  const networkParts = [
    (networkInt >>> 24) & 255,
    (networkInt >>> 16) & 255,
    (networkInt >>> 8) & 255,
    networkInt & 255
  ];

  const totalIps = calculateTotalIps(cidr);
  const maxDisplayIps = Math.min(totalIps, MAX_DISPLAY_IPS);

  const ipAddresses: string[] = [];
  for (let i = 1; i <= maxDisplayIps; i++) {
    // 从网络地址起递增（进位传播到高位字节）
    const newIpParts = [...networkParts];
    let carry = i;
    for (let j = 3; j >= 0 && carry > 0; j--) {
      const sum = (newIpParts[j] ?? 0) + carry;
      newIpParts[j] = sum % 256;
      carry = Math.floor(sum / 256);
    }
    ipAddresses.push(newIpParts.join("."));
  }
  return ipAddresses;
}

/** 生成 /22、/23 网段划分为 /24 的子网列表 */
function generateSubnet24List(cidr: string | null | undefined): string[] {
  if (!cidr) {
    return [];
  }
  const parts = cidr.split("/");
  if (parts.length !== 2) {
    return [];
  }
  const prefixLength = Number.parseInt(parts[1] ?? "", 10);
  if (prefixLength !== 22 && prefixLength !== 23) {
    return [];
  }

  const ipParts = (parts[0] ?? "").split(".").map(Number);
  if (ipParts.length !== 4 || ipParts.some((p) => Number.isNaN(p))) {
    return [];
  }

  const [ip0 = 0, ip1 = 0, ip2 = 0] = ipParts;
  // /22 掩码第三段 252，/23 掩码第三段 254
  const maskThird = prefixLength === 22 ? 252 : 254;
  const networkThird = ip2 & maskThird;
  const subnetCount = prefixLength === 22 ? 4 : 2;

  const subnets: string[] = [];
  for (let i = 0; i < subnetCount; i++) {
    subnets.push(`${ip0}.${ip1}.${networkThird + i}.0/24`);
  }
  return subnets;
}

const ipv4Prefix = computed(() => {
  const cidr = network.value?.ipv4_cidr;
  // 非法 CIDR（缺 "/"）时与原实现一致返回 NaN，各处比较均回落 false
  return cidr ? Number.parseInt(cidr.split("/")[1] ?? "", 10) : 32;
});

const subnet24List = computed(() =>
  ipv4Prefix.value === 22 || ipv4Prefix.value === 23
    ? generateSubnet24List(network.value?.ipv4_cidr)
    : []
);

// /24 子网切换态：详情到达（或切换网段）时重置为第一个子网
const activeSubnet24 = ref("");
watch(
  () => network.value?.ipv4_cidr,
  () => {
    activeSubnet24.value = subnet24List.value[0] || "";
  }
);

// 展示 CIDR：/22·/23 用当前选中的 /24 子网，其余用网段自身
const displayCidr = computed(
  () => activeSubnet24.value || network.value?.ipv4_cidr || ""
);

const ipBlocks = computed(() =>
  ipv4Prefix.value < 22 ? [] : generateIpAddresses(displayCidr.value)
);

/** IP 状态映射：ip_address → active/inactive（未记录视为未使用） */
const ipStatusMap = computed(() => {
  const map = new Map<string, string>();
  ipv4Ips.value.forEach((ip) => {
    map.set(ip.ip_address, ip.status || "inactive");
  });
  return map;
});

/** IP 块三态样式类（未使用/活跃/非活跃） */
function ipBlockClass(ip: string): string {
  const status = ipStatusMap.value.get(ip);
  const isUsed = ipStatusMap.value.has(ip);
  if (!isUsed) {
    return "ip-unused";
  }
  return status === "active" ? "ip-used-active" : "ip-used-inactive";
}

/** IP 块状态文案 */
function ipStateText(ip: string): string {
  const isUsed = ipStatusMap.value.has(ip);
  if (!isUsed) {
    return t("network.unused");
  }
  return ipStatusMap.value.get(ip) === "active" ? t("status.active") : t("status.inactive");
}

const totalIps = computed(() => calculateTotalIps(network.value?.ipv4_cidr));
const usedIps = computed(() => ipv4Ips.value.length);
const unusedIps = computed(() => Math.max(0, totalIps.value - usedIps.value));
const usageRate = computed(() =>
  totalIps.value > 0 ? ((usedIps.value / totalIps.value) * 100).toFixed(2) : "0.00"
);

// 状态过滤：仅作用于可视化网格显隐，不影响下方列表
const ipFilter = ref("all");

function ipBlockVisible(ip: string): boolean {
  if (ipFilter.value === "all") {
    return true;
  }
  const isUsed = ipStatusMap.value.has(ip);
  return ipFilter.value === "used" ? isUsed : !isUsed;
}

// IPv6 统计

const ipv6Stats = computed(() => {
  const activeIps = ipv6Ips.value.filter((ip) => ip.status === "active").length;
  return {
    totalAssigned: ipv6Ips.value.length,
    active: activeIps,
    inactive: ipv6Ips.value.length - activeIps
  };
});

// 刷新

async function refreshUsage() {
  await ipsQuery.refetch();
  showToast(
    activeTab.value === "ipv4" ? t("network.ipv4_usage_updated") : t("network.ipv6_usage_updated"),
    "success"
  );
}

function formatDns(dns: string[] | null | undefined): string {
  return Array.isArray(dns) ? dns.join(", ") : dns || "-";
}
</script>

<template>
  <ModalShell :title="t('network.usage')" size="lg" @close="emit('close')">
    <div class="network-usage-container">
      <div class="usage-tabs">
        <span class="usage-network-name">{{ network?.name }}</span>
        <div v-if="showTabButtons" class="usage-tab-buttons" role="tablist">
          <button
            type="button"
            class="usage-tab-btn"
            :class="{ active: activeTab === 'ipv4' }"
            role="tab"
            :aria-selected="activeTab === 'ipv4'"
            @click="activeTab = 'ipv4'"
          >
            IPv4
          </button>
          <button
            type="button"
            class="usage-tab-btn"
            :class="{ active: activeTab === 'ipv6' }"
            role="tab"
            :aria-selected="activeTab === 'ipv6'"
            @click="activeTab = 'ipv6'"
          >
            IPv6
          </button>
        </div>
      </div>

      <div v-if="networkQuery.isError.value || ipsQuery.isError.value" class="no-network-info" role="alert">
        <p>{{ networkQuery.error.value?.message || ipsQuery.error.value?.message }}</p>
      </div>

      <!-- ==================== IPv4 页签 ==================== -->
      <div v-else-if="activeTab === 'ipv4'" role="tabpanel">
        <div class="usage-stats">
          <div class="stat-item">
            <span class="stat-label">{{ t("network.region") }}:</span>
            <span class="stat-value">{{ network?.network_region || "-" }}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">{{ t("network.ipv4_cidr") }}:</span>
            <span class="stat-value">{{ network?.ipv4_cidr || "-" }}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">{{ t("network.ipv4_gateway") }}:</span>
            <span class="stat-value">{{ network?.ipv4_gateway || "-" }}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">{{ t("network.ipv4_dns") }}:</span>
            <span class="stat-value">{{ formatDns(network?.ipv4_dns) }}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">{{ t("network.total_ips") }}:</span>
            <span class="stat-value">{{ totalIps }}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">{{ t("network.used_ips") }}:</span>
            <span class="stat-value">{{ usedIps }}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">{{ t("network.unused_ips") }}:</span>
            <span class="stat-value">{{ unusedIps }}</span>
          </div>
          <div class="stat-item">
            <span class="stat-label">{{ t("network.usage_rate") }}:</span>
            <span class="stat-value">{{ usageRate }}%</span>
          </div>
        </div>

        <div class="usage-controls">
          <div class="filter-controls">
            <label>{{ t("common.filter") }}: </label>
            <select v-model="ipFilter" class="form-control form-control-sm">
              <option value="all">{{ t("common.all") }}</option>
              <option value="used">{{ t("network.used") }}</option>
              <option value="unused">{{ t("network.unused") }}</option>
            </select>
            <button
              type="button"
              class="icon-btn btn-secondary"
              :disabled="ipsQuery.isFetching.value"
              :data-tooltip="t('common.refresh')"
              :aria-label="t('common.refresh')"
              @click="refreshUsage"
            >
              <span v-if="ipsQuery.isFetching.value" class="usage-loading" role="status" aria-hidden="true" />
              <NavIcon v-else name="refresh" />
            </button>
          </div>
        </div>

        <div class="usage-visualization">
          <h5>{{ t("network.ip_visualization") }}</h5>
          <!-- 掩码 < 22：IP 数量过多，不生成网格 -->
          <div v-if="ipv4Prefix < 22" class="ip-grid-empty" role="alert">
            {{ t("network.visualization_too_many_ips") }}
          </div>
          <template v-else>
            <div v-if="subnet24List.length > 0" class="subnet-24-tabs" role="tablist">
              <button
                v-for="sub in subnet24List"
                :key="sub"
                type="button"
                class="subnet-24-btn"
                :class="{ active: displayCidr === sub }"
                role="tab"
                @click="activeSubnet24 = sub; ipFilter = 'all'"
              >
                {{ sub }}
              </button>
            </div>
            <div class="ip-grid">
              <div
                v-for="ip in ipBlocks"
                :key="ip"
                v-show="ipBlockVisible(ip)"
                class="ip-block"
                :class="ipBlockClass(ip)"
                :title="`${ip} (${ipStateText(ip)})`"
              >
                <span class="ip-label">{{ ip.split(".").pop() }}</span>
              </div>
            </div>
          </template>
        </div>

        <div class="usage-ips">
          <h5>{{ t("network.ipv4_list") }}</h5>
          <div class="table-responsive usage-table">
            <table class="table">
              <thead>
                <tr>
                  <th>{{ t("ip.ip_address") }}</th>
                  <th>{{ t("ip.status") }}</th>
                  <th>{{ t("ip.location") }}</th>
                  <th>{{ t("ip.mac_address") }}</th>
                  <th>{{ t("ip.hostname") }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-if="ipv4Ips.length === 0">
                  <td colspan="5" class="text-center">{{ t("network.no_ipv4_records") }}</td>
                </tr>
                <template v-else>
                  <tr v-for="ip in ipv4Ips" :key="ip.ip_address">
                    <td>{{ ip.ip_address }}</td>
                    <td>
                      <span class="status-badge" :class="ip.status === 'active' ? 'status-active' : 'status-inactive'">
                        {{ ip.status }}
                      </span>
                    </td>
                    <td>{{ ip.workstation_name || ip.cabinet_position_name || "-" }}</td>
                    <td>{{ ip.mac_address || "-" }}</td>
                    <td>{{ ip.hostname || "-" }}</td>
                  </tr>
                </template>
              </tbody>
            </table>
          </div>
        </div>
      </div>

      <!-- ==================== IPv6 页签 ==================== -->
      <div v-else role="tabpanel">
        <!-- 未配置 IPv6：空态提示 -->
        <div v-if="!hasIPv6" class="no-ipv6-info">
          <div class="no-ipv6-icon">📡</div>
          <p>{{ t("network.no_ipv6_config") }}</p>
          <p class="no-ipv6-hint">{{ t("network.add_ipv6_hint") }}</p>
        </div>
        <template v-else>
          <div class="ipv6-info-section">
            <h5>{{ t("network.ipv6_info") }}</h5>
            <div class="ipv6-info-grid">
              <div class="ipv6-info-item">
                <span class="ipv6-info-label">{{ t("network.ipv6_cidr") }}</span>
                <span class="ipv6-info-value ipv6-address">{{ network?.ipv6_cidr }}</span>
              </div>
              <div class="ipv6-info-item">
                <span class="ipv6-info-label">{{ t("network.ipv6_gateway") }}</span>
                <span class="ipv6-info-value ipv6-address">{{ network?.ipv6_gateway || "-" }}</span>
              </div>
              <div class="ipv6-info-item">
                <span class="ipv6-info-label">DNS</span>
                <span class="ipv6-info-value ipv6-address">{{ formatDns(network?.ipv6_dns) }}</span>
              </div>
              <div class="ipv6-info-item">
                <span class="ipv6-info-label">{{ t("network.connection_status") }}</span>
                <span class="ipv6-info-value">
                  <span class="connection-status" :class="ipv6Stats.totalAssigned > 0 ? 'status-enabled' : 'status-disabled'">
                    {{ ipv6Stats.totalAssigned > 0 ? t("network.enabled") : t("network.unused") }}
                  </span>
                </span>
              </div>
              <div class="ipv6-info-item">
                <span class="ipv6-info-label">{{ t("network.assigned") }}</span>
                <span class="ipv6-info-value">{{ ipv6Stats.totalAssigned }}</span>
              </div>
              <div class="ipv6-info-item">
                <span class="ipv6-info-label">{{ t("status.active") }}</span>
                <span class="ipv6-info-value">{{ ipv6Stats.active }}</span>
              </div>
              <div class="ipv6-info-item">
                <span class="ipv6-info-label">{{ t("status.inactive") }}</span>
                <span class="ipv6-info-value">{{ ipv6Stats.inactive }}</span>
              </div>
            </div>
          </div>

          <div class="ipv6-list-section">
            <div class="ipv6-list-header">
              <h5>{{ t("network.ipv6_list") }}</h5>
              <button
                type="button"
                class="icon-btn btn-secondary"
                :disabled="ipsQuery.isFetching.value"
                :data-tooltip="t('common.refresh')"
                :aria-label="t('common.refresh')"
                @click="refreshUsage"
              >
                <span v-if="ipsQuery.isFetching.value" class="usage-loading" role="status" aria-hidden="true" />
                <NavIcon v-else name="refresh" />
              </button>
            </div>
            <div class="table-responsive usage-table">
              <table class="table">
                <thead>
                  <tr>
                    <th>{{ t("ip.ip_address") }}</th>
                    <th>{{ t("ip.status") }}</th>
                    <th>{{ t("ip.location") }}</th>
                    <th>{{ t("ip.mac_address") }}</th>
                    <th>{{ t("ip.hostname") }}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr v-if="ipv6Ips.length === 0">
                    <td colspan="5" class="text-center">{{ t("network.no_ipv6_records") }}</td>
                  </tr>
                  <template v-else>
                    <tr v-for="ip in ipv6Ips" :key="ip.ip_address">
                      <td class="ipv6-address-cell">{{ ip.ip_address }}</td>
                      <td>
                        <span class="status-badge" :class="ip.status === 'active' ? 'status-active' : 'status-inactive'">
                          {{ ip.status }}
                        </span>
                      </td>
                      <td>{{ ip.workstation_name || ip.cabinet_position_name || "-" }}</td>
                      <td>{{ ip.mac_address || "-" }}</td>
                      <td>{{ ip.hostname || "-" }}</td>
                    </tr>
                  </template>
                </tbody>
              </table>
            </div>
          </div>
        </template>
      </div>
    </div>
  </ModalShell>
</template>

<style scoped>
/* 网段使用情况弹窗样式：
 全部类仅命中本组件模板（含 v-for 渲染的表格行，ModalShell 无 Teleport），
 改 scoped 消除 .table/.ipv6-address 等通用类名的全局泄漏 */
.network-usage-container {
  font-size: 0.875rem;
}

.usage-tabs {
  display: flex;
  align-items: flex-end;
  gap: 0.9375rem;
  margin-bottom: 1.25rem;
  border-bottom: 2px solid var(--color-border-dark);
}

/* 网段名：置于 IPv4/IPv6 切换按钮左侧的纯文本，仅标识当前网段 */
.usage-network-name {
  font-weight: 600;
  font-size: 1rem;
  color: var(--color-text-light);
  padding-bottom: 0.625rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.usage-tab-buttons {
  display: flex;
  gap: 0.3125rem;
}

.usage-tab-btn {
  padding: 0.625rem 1.25rem;
  border: none;
  background: none;
  cursor: pointer;
  font-size: 0.875rem;
  font-weight: 500;
  color: var(--color-text-muted);
  border-bottom: 2px solid transparent;
  margin-bottom: -2px;
  transition:
    color var(--transition-normal),
    border-bottom-color var(--transition-normal);
}

.usage-tab-btn:hover {
  color: var(--color-text-light);
}

.usage-tab-btn.active {
  color: var(--color-blue);
  border-bottom-color: var(--color-blue);
}

/* 固定 4 列网格容器：IPv4 统计与 IPv6 信息共用同款外观；
 间距各自控制（.usage-stats 独占一块，.ipv6-info-grid 由 .ipv6-info-section 提供间距）。
 8 项统计按顺序前 4 后 4 分两行；窄屏在下方媒体查询降为 2 列 */
.usage-stats,
.ipv6-info-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 0.625rem;
  padding: 0.9375rem;
  background-color: var(--color-bg);
  border-radius: var(--radius-lg);
}

.usage-stats {
  margin-bottom: 1.25rem;
}

/* 作用域限定在 .usage-stats 内，避免与 cards.css 的 .stat-value 冲突 */
.usage-stats .stat-item {
  display: flex;
  align-items: baseline;
  gap: 0.5rem;
  padding: 0.25rem 0;
}

.usage-stats .stat-label {
  font-weight: 500;
  color: var(--color-text-muted);
  font-size: 0.8125rem;
  white-space: nowrap;
}

.usage-stats .stat-value {
  font-weight: 600;
  font-size: 0.875rem;
  color: var(--color-text-light);
}

.usage-controls {
  margin-bottom: 1.25rem;
}

/* 过滤行（IPv4 页签控件区） */
.usage-controls .filter-controls {
  display: flex;
  align-items: center;
  gap: var(--space-sm);
}

.usage-visualization {
  margin-bottom: 1.25rem;
}

.ip-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(2.8rem, 1fr));
  gap: 0.375rem;
  padding: 0.75rem;
  background-color: var(--color-bg);
  border-radius: var(--radius-lg);
  max-height: 18.75rem;
  overflow-y: auto;
}

.ip-grid-empty {
  padding: 0.75rem 1rem;
  background-color: var(--color-warning-bg);
  border: 1px solid var(--color-warning-border);
  border-radius: var(--radius-lg);
  color: var(--color-warning-text);
  font-size: 0.875rem;
  line-height: 1.5;
}

.subnet-24-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
  margin-bottom: 0.75rem;
}

.subnet-24-btn {
  padding: 0.25rem 0.625rem;
  font-size: 0.75rem;
  line-height: 1.4;
  color: var(--color-text-secondary);
  background-color: var(--color-bg-secondary);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition:
    color var(--transition-normal),
    border-color var(--transition-normal),
    background-color var(--transition-normal);
  font-family: var(--font-mono);
}

.subnet-24-btn:hover {
  color: var(--color-primary);
  border-color: var(--color-primary);
  background-color: var(--color-primary-light);
}

.subnet-24-btn.active {
  color: var(--color-bg-white);
  background-color: var(--color-primary);
  border-color: var(--color-primary);
}

.ip-block {
  width: 100%;
  height: 2rem;
  border-radius: var(--radius-sm);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition:
    transform var(--transition-normal),
    box-shadow var(--transition-normal);
  position: relative;
}

.ip-label {
  font-size: 0.6875rem;
  font-weight: 500;
  color: var(--color-text-light);
}

.ip-block:hover {
  transform: translateY(-0.125rem);
  box-shadow: var(--shadow-sm);
}

.ip-unused {
  background-color: var(--color-border-light);
  border: 1px solid var(--color-border-dark);
}

.ip-used-active {
  background-color: var(--color-success-bg);
  border: 1px solid var(--color-success);
}

.ip-used-inactive {
  background-color: var(--color-warning-bg);
  border: 1px solid var(--color-warning);
}

.usage-ips,
.ipv6-list-section {
  margin-top: 1.25rem;
}

.usage-ips h5,
.ipv6-list-header h5 {
  margin-bottom: 0.75rem;
  margin-top: 0;
  font-size: 0.9375rem;
  font-weight: 600;
  color: var(--color-text-light);
}

.ipv6-list-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 0.75rem;
}

/* IP / IPv6 列表表格容器（组合类统一两处列表表格样式） */
.usage-table {
  width: 100%;
  border-radius: var(--radius-md);
  background: var(--color-bg);
  box-shadow: var(--shadow-sm);
}

.usage-table .table {
  width: 100%;
  margin-bottom: 0;
  border-collapse: collapse;
  font-size: 0.8125rem;
}

.usage-table th,
.usage-table td {
  padding: 0.5rem 0.75rem;
  text-align: left;
  border-bottom: 1px solid var(--color-border);
}

.usage-table th {
  background-color: var(--color-bg-secondary);
  border-bottom-width: 2px;
  font-weight: 600;
  font-size: 0.75rem;
  color: var(--color-text-muted);
  text-transform: uppercase;
  letter-spacing: 0.025em;
  white-space: nowrap;
}

.usage-table td {
  vertical-align: middle;
  color: var(--color-text-light);
}

.usage-table tbody tr:hover {
  background-color: var(--color-bg-secondary);
}

.usage-table tr:last-child td {
  border-bottom: none;
}

.ipv6-info-section {
  margin-bottom: 1.25rem;
}

.ipv6-info-section h5 {
  margin-bottom: 0.9375rem;
  color: var(--color-text-light);
  font-weight: 600;
}

/* 与 IPv4 页 .usage-stats 同款容器：固定 4 列；项内标签在上、值在下
 （IPv6 地址长，值独占第二行）；窄屏降为整行显示 */
.ipv6-info-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 0.625rem;
  padding: 0.9375rem;
  background-color: var(--color-bg);
  border-radius: var(--radius-lg);
}

.ipv6-info-item {
  display: flex;
  flex-direction: column;
  gap: 0.3125rem;
}

.ipv6-info-label {
  font-weight: 500;
  color: var(--color-text-muted);
  font-size: 0.8125rem;
}

.ipv6-info-value {
  font-weight: 600;
  font-size: 0.875rem;
  color: var(--color-text-light);
  word-break: break-all;
}

.ipv6-address {
  font-family: var(--font-mono);
  color: var(--color-ipv6);
}

/* 连接状态徽章（.status-enabled/.status-disabled） */
.connection-status {
  display: inline-flex;
  align-items: center;
  gap: 0.3125rem;
  padding: 0.25rem 0.75rem;
  border-radius: 0.9375rem;
  font-size: 0.8125rem;
  font-weight: 500;
}

.connection-status.status-enabled {
  background-color: var(--color-success-bg);
  color: var(--color-success-text);
}

.connection-status.status-disabled {
  background-color: var(--color-border-light);
  color: var(--color-text-muted);
}

.ipv6-address-cell {
  font-family: var(--font-mono);
  font-size: 0.8125rem;
  color: var(--color-ipv6);
}

/* 无 IPv6 配置 / 网络不可用空态：共用居中留白样式 */
.no-ipv6-info,
.no-network-info {
  text-align: center;
  padding: 3.75rem 1.25rem;
  color: var(--color-text-muted);
}

.no-ipv6-icon {
  font-size: 3rem;
  margin-bottom: 0.9375rem;
}

.no-ipv6-info p {
  margin: 0.3125rem 0;
}

.no-ipv6-hint {
  font-size: 0.75rem;
  color: var(--color-text-muted);
}

/* 刷新按钮加载态 spinner（仅本弹窗使用） */
.usage-loading {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid rgb(255 255 255 / 30%);
  border-radius: 50%;
  border-top-color: currentColor;
  animation: usage-loading-spin 0.8s ease-in-out infinite;
  vertical-align: middle;
  margin-right: 4px;
}

@keyframes usage-loading-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (width <= 768px) {
  .usage-tabs {
    flex-direction: column;
    align-items: stretch;
    border-bottom: none;
  }

  .usage-network-name {
    padding-bottom: 0;
  }

  .usage-tab-btn {
    border-bottom: 1px solid var(--color-border-dark);
    margin-bottom: 0;
  }

  .usage-tab-btn.active {
    border-bottom-color: var(--color-blue);
    background-color: var(--color-bg);
  }

  .usage-stats {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .ipv6-info-grid {
    grid-template-columns: 1fr;
  }
}
</style>
