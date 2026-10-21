<script setup lang="ts">
/**
 * MAC 表弹窗（移植自旧版 modals/device/arp-modal.html +
 * deviceMacLldp.js viewArpTable/renderMacTable/groupByNetwork/filterEntries/
 * bindCollapseEvents/syncMacData）：
 * - 加载设备名 + GET /devices/{id}/macs，按 IPv4（含"."）与 IPv6（含":"）
 *   分页签展示，页签标题带条数；
 * - 表格按网段分组折叠（默认收起）：IPv4 归 /24，IPv6 按 fe80/ULA/240e 等
 *   归类，组间按网段名排序、组内按 IP 排序；
 * - 搜索：IP/MAC 包含匹配，300ms 防抖（MAC 表可达数千行）；
 * - 同步：POST /devices/{id}/macs/sync 后重新加载；空数据态提供同步按钮。
 */
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, apiPost } from "../../api/client";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { DeviceWithDetails } from "../../types/api/generated/DeviceWithDetails";
import type { SnmpMac } from "../../types/api/generated/SnmpMac";

const props = defineProps<{ deviceId: string }>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

// ==================== 加载状态机 ====================
// loading 初始加载 / syncing 同步中 / ready 就绪 / empty 无数据 / error 失败
type LoadPhase = "loading" | "syncing" | "ready" | "empty" | "error";

const phase = ref<LoadPhase>("loading");
const errorMsg = ref("");
const deviceName = ref("");
const entries = ref<SnmpMac[]>([]);

async function loadData(): Promise<void> {
  phase.value = "loading";
  try {
    const deviceResult = await apiGet<DeviceWithDetails>(`/api/resources/devices/${props.deviceId}`);
    if (!deviceResult.success) {
      phase.value = "error";
      errorMsg.value = t("device.load_failed");
      return;
    }
    deviceName.value = deviceResult.data?.name || "";

    const result = await apiGet<SnmpMac[]>(`/api/resources/devices/${props.deviceId}/macs`);
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.load_mac_failed")}: ${result.message || ""}`;
      return;
    }
    entries.value = result.data || [];
    phase.value = entries.value.length === 0 ? "empty" : "ready";
  } catch (error) {
    // apiGet/apiPost 不 reject，此分支为防御性保留；非 Error 抛出值按空串回退
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("common.load_failed")}: ${message}`;
  }
}

async function syncMacData(): Promise<void> {
  phase.value = "syncing";
  try {
    const result = await apiPost<unknown>(`/api/resources/devices/${props.deviceId}/macs/sync`, {});
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.sync_failed")}: ${result.message || ""}`;
      return;
    }
    await loadData();
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("device.sync_failed")}: ${message}`;
  }
}

// ==================== IPv4/IPv6 页签与搜索 ====================

const activeTab = ref<"ipv4" | "ipv6">("ipv4");

function switchTab(tab: "ipv4" | "ipv6"): void {
  activeTab.value = tab;
}

const ipv4SearchInput = ref("");
const ipv6SearchInput = ref("");
// 防抖生效值：输入 300ms 后更新（MAC 表大，避免每次按键全量重算）
const ipv4Search = ref("");
const ipv6Search = ref("");

let searchTimer: ReturnType<typeof setTimeout> | null = null;
watch([ipv4SearchInput, ipv6SearchInput], () => {
  if (searchTimer) {
    clearTimeout(searchTimer);
  }
  searchTimer = setTimeout(() => {
    ipv4Search.value = ipv4SearchInput.value;
    ipv6Search.value = ipv6SearchInput.value;
  }, 300);
});
onBeforeUnmount(() => {
  if (searchTimer) {
    clearTimeout(searchTimer);
  }
});

function filterEntries(list: SnmpMac[], term: string): SnmpMac[] {
  if (!term) {
    return list;
  }
  const lower = term.toLowerCase();
  return list.filter(
    (e) => e.ip_address.toLowerCase().includes(lower) || e.mac_address.toLowerCase().includes(lower)
  );
}

const ipv4Entries = computed(() => entries.value.filter((e) => e.ip_address?.includes(".")));
const ipv6Entries = computed(() => entries.value.filter((e) => e.ip_address?.includes(":")));

// ==================== 网段分组（与旧版 groupByNetwork 一致） ====================

function groupNetworkOf(entry: SnmpMac, type: "ipv4" | "ipv6"): string {
  if (type === "ipv4") {
    const parts = entry.ip_address.split(".");
    // parts.length >= 3 已保证前三段存在，逐段判空仅为 noUncheckedIndexedAccess 收窄
    const [p0, p1, p2] = parts;
    return p0 !== undefined && p1 !== undefined && p2 !== undefined
      ? `${p0}.${p1}.${p2}.0/24`
      : t("device.unknown_network");
  }
  const parts = entry.ip_address.split(":");
  if (parts.length < 2) {
    return t("device.unknown_network");
  }
  if (entry.ip_address.startsWith("fe80")) {
    return `fe80::/10 (${t("device.link_local")})`;
  }
  if (entry.ip_address.startsWith("fd") || entry.ip_address.startsWith("fc")) {
    // parts.length >= 2 已保证首段存在，?? 仅用于类型收窄
    return `${parts[0] ?? ""}::/16 (ULA)`;
  }
  const prefix = parts.slice(0, 4).join(":");
  if (parts[0] === "240e" || parts[0] === "2409" || parts[0] === "2408") {
    return `${prefix}::/64 (${t("device.public_network")})`;
  }
  return `${prefix}::/64`;
}

function sortEntries(list: SnmpMac[], type: "ipv4" | "ipv6"): SnmpMac[] {
  // toSorted 返回新数组，不改动调用方持有的原列表
  return list.toSorted((a, b) => {
    if (type === "ipv4") {
      const aParts = a.ip_address.split(".").map(Number);
      const bParts = b.ip_address.split(".").map(Number);
      for (let i = 0; i < 4; i++) {
        const av = aParts[i];
        const bv = bParts[i];
        if (av !== bv) {
          // 缺段（畸形数据）时以 NaN 兜底，与原实现 undefined 参与减法的结果一致
          return (av ?? Number.NaN) - (bv ?? Number.NaN);
        }
      }
      return 0;
    }
    return a.ip_address.localeCompare(b.ip_address);
  });
}

/** 分组并排序：组间按网段名排序，组内按 IP 排序 */
function buildGroups(list: SnmpMac[], type: "ipv4" | "ipv6") {
  const map = new Map<string, SnmpMac[]>();
  for (const entry of list) {
    const network = groupNetworkOf(entry, type);
    if (!map.has(network)) {
      map.set(network, []);
    }
    // 键刚 set 过必有值，?.push 仅用于 noUncheckedIndexedAccess 收窄
    map.get(network)?.push(entry);
  }
  // 展开仅物化 Map 键迭代器，toSorted 生成排序副本
  return [...map.keys()]
    .toSorted()
    .map((network) => ({ network, items: sortEntries(map.get(network) ?? [], type) }));
}

const ipv4Groups = computed(() => buildGroups(filterEntries(ipv4Entries.value, ipv4Search.value), "ipv4"));
const ipv6Groups = computed(() => buildGroups(filterEntries(ipv6Entries.value, ipv6Search.value), "ipv6"));

// ==================== 分组折叠（默认全部收起，与旧版一致） ====================

const expandedGroups = ref(new Set<string>());

function toggleGroup(network: string): void {
  const next = new Set(expandedGroups.value);
  if (next.has(network)) {
    next.delete(network);
  } else {
    next.add(network);
  }
  expandedGroups.value = next;
}

loadData();
</script>

<template>
  <ModalShell size="lg" @close="emit('close')">
    <template #title>
      {{ t("device.mac_table") }} - {{ deviceName }}
      <template v-if="phase === 'ready' || phase === 'empty'">
        ({{ entries.length }} {{ t("common.records") }})
      </template>
    </template>
    <div class="modal-body-scroll">
      <!-- 加载中/同步中状态提示（非对话框）：用 status 角色向辅助技术播报 -->
      <div v-if="phase === 'loading' || phase === 'syncing'" class="modal-loading" role="status" aria-live="polite">
        <div class="spinner"></div>
        <p class="loading-text">
          {{ phase === "syncing" ? t("device.syncing_mac") : t("device.loading_mac_table") }}
        </p>
      </div>
      <div v-else-if="phase === 'error'" class="modal-loading">
        <p class="mac-lldp-text-error">{{ errorMsg }}</p>
      </div>
      <div v-else-if="phase === 'empty'" class="modal-loading">
        <p class="mac-lldp-text-muted">{{ t("device.no_mac_data") }}</p>
        <button type="button" class="icon-btn btn-primary" :title="t('device.sync_from_snmp')" @click="syncMacData">
          <NavIcon name="refresh" />
        </button>
      </div>
      <div v-else>
        <div class="tab-toolbar">
          <div class="tab-buttons" role="tablist">
            <button
              type="button"
              class="tab-btn"
              :class="{ active: activeTab === 'ipv4' }"
              role="tab"
              :aria-selected="activeTab === 'ipv4'"
              @click="switchTab('ipv4')"
            >
              IPv4 ({{ ipv4Entries.length }})
            </button>
            <button
              type="button"
              class="tab-btn"
              :class="{ active: activeTab === 'ipv6' }"
              role="tab"
              :aria-selected="activeTab === 'ipv6'"
              @click="switchTab('ipv6')"
            >
              IPv6 ({{ ipv6Entries.length }})
            </button>
          </div>
          <button
            type="button"
            class="icon-btn btn-primary"
            :data-tooltip="t('device.sync_from_snmp')"
            :aria-label="t('device.sync_from_snmp')"
            @click="syncMacData"
          >
            <NavIcon name="refresh" />
          </button>
        </div>
        <div v-show="activeTab === 'ipv4'" role="tabpanel" :aria-hidden="activeTab !== 'ipv4'">
          <input
            v-model="ipv4SearchInput"
            type="text"
            class="tab-search"
            :placeholder="t('device.search_ip_mac')"
            :aria-label="t('device.search_ip_mac')"
          />
          <div class="tab-table-container">
            <p v-if="ipv4Groups.length === 0" class="mac-table-empty">IPv4 {{ t("common.no_data") }}</p>
            <div v-for="group in ipv4Groups" :key="group.network" class="mac-network-group">
              <div class="network-group-header" @click="toggleGroup(group.network)">
                <span>{{ group.network }} ({{ group.items.length }} {{ t("common.records") }})</span>
                <span class="collapse-icon mac-collapse-icon" :class="{ expanded: expandedGroups.has(group.network) }">▼</span>
              </div>
              <div v-show="expandedGroups.has(group.network)" class="network-group-content">
                <table class="mac-group-table">
                  <tr>
                    <th>{{ t("device.ip_address") }}</th>
                    <th>{{ t("device.mac_address") }}</th>
                    <th>{{ t("device.vlan_id") }}</th>
                  </tr>
                  <tr v-for="entry in group.items" :key="entry.ip_address + entry.mac_address">
                    <td>{{ entry.ip_address }}</td>
                    <td>{{ entry.mac_address }}</td>
                    <td>{{ entry.vlan_id != null ? String(entry.vlan_id) : "-" }}</td>
                  </tr>
                </table>
              </div>
            </div>
          </div>
        </div>
        <div v-show="activeTab === 'ipv6'" role="tabpanel" :aria-hidden="activeTab !== 'ipv6'">
          <input
            v-model="ipv6SearchInput"
            type="text"
            class="tab-search"
            :placeholder="t('device.search_ip_mac')"
            :aria-label="t('device.search_ip_mac')"
          />
          <div class="tab-table-container">
            <p v-if="ipv6Groups.length === 0" class="mac-table-empty">IPv6 {{ t("common.no_data") }}</p>
            <div v-for="group in ipv6Groups" :key="group.network" class="mac-network-group">
              <div class="network-group-header" @click="toggleGroup(group.network)">
                <span>{{ group.network }} ({{ group.items.length }} {{ t("common.records") }})</span>
                <span class="collapse-icon mac-collapse-icon" :class="{ expanded: expandedGroups.has(group.network) }">▼</span>
              </div>
              <div v-show="expandedGroups.has(group.network)" class="network-group-content">
                <table class="mac-group-table">
                  <tr>
                    <th>{{ t("device.ip_address") }}</th>
                    <th>{{ t("device.mac_address") }}</th>
                    <th>{{ t("device.vlan_id") }}</th>
                  </tr>
                  <tr v-for="entry in group.items" :key="entry.ip_address + entry.mac_address">
                    <td>{{ entry.ip_address }}</td>
                    <td>{{ entry.mac_address }}</td>
                    <td>{{ entry.vlan_id != null ? String(entry.vlan_id) : "-" }}</td>
                  </tr>
                </table>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </ModalShell>
</template>
