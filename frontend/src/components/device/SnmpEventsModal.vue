<script setup lang="ts">
/**
 * SNMP 观测事件弹窗：
 * - GET /api/snmp/events 分页列表（occurred_at 倒序），列：序号/时间/设备/类型/网口/级别/详情；
 * - 筛选：设备（下拉）、事件类型、严重级别、时间范围（起止，前端转 ISO-8601）；
 * - 从设备行进入时预填 device_id 过滤；severity 徽标沿用既有 status-* 配色；
 * - detail 为 JSONB，按事件类型摘要展示关键字段（方向/停机时长/IP/MAC/VLAN）。
 */
import { computed, onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../../api/client";
import { usePagedList, seqIndex } from "../../composables/usePagedList";
import { queryKeys } from "../../utils/queryKeys";
import ModalShell from "../ModalShell.vue";
import Pagination from "../Pagination.vue";
import type { SnmpEvent } from "../../types/api/generated/SnmpEvent";
import type { DeviceWithDetails } from "../../types/api/generated/DeviceWithDetails";
import type { Paged } from "../../types/api";

const props = withDefaults(defineProps<{ deviceId?: string; embedded?: boolean }>(), {
  deviceId: "",
  embedded: false
});

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

const filters = reactive({
  device_id: props.deviceId,
  event_type: "",
  severity: "",
  start: "",
  end: ""
});

/** 起止时间转 ISO-8601（datetime-local 无时区，按本地时区补全；空值不下发） */
function toIso(value: string): string {
  if (!value) {
    return "";
  }
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? "" : parsed.toISOString();
}

/** 附加查询键：筛选变化触发重新查询（仅纳入非空项） */
const extraKey = computed(() => ({
  device_id: filters.device_id,
  event_type: filters.event_type,
  severity: filters.severity,
  start: filters.start,
  end: filters.end
}));

const list = usePagedList<SnmpEvent>({
  tag: queryKeys.snmpEvents,
  defaultPageSize: 20,
  defaultSortBy: "occurred_at",
  defaultSortOrder: "desc",
  extraKey,
  buildUrl: (page, pageSize) => {
    const params = new URLSearchParams({ page: String(page), page_size: String(pageSize) });
    if (filters.device_id) {
      params.set("device_id", filters.device_id);
    }
    if (filters.event_type) {
      params.set("event_type", filters.event_type);
    }
    if (filters.severity) {
      params.set("severity", filters.severity);
    }
    const start = toIso(filters.start);
    const end = toIso(filters.end);
    if (start) {
      params.set("start", start);
    }
    if (end) {
      params.set("end", end);
    }
    return `/api/snmp/events?${params}`;
  }
});

const devicesQuery = useQuery({
  queryKey: queryKeys.snmpEventDevices,
  queryFn: async () => {
    const result = await apiGet<Paged<DeviceWithDetails>>(
      `/api/resources/devices?${new URLSearchParams({ page: "1", page_size: "500", sort_by: "name", sort_order: "asc" })}`
    );
    return result.success && result.data ? result.data.items : [];
  }
});

const deviceNameMap = computed<Map<string, string>>(() => {
  const map = new Map<string, string>();
  for (const device of devicesQuery.data.value ?? []) {
    if (device.id != null) {
      map.set(device.id, device.name || device.id);
    }
  }
  return map;
});

function deviceName(id: string): string {
  return deviceNameMap.value.get(id) || id;
}

const EVENT_TYPES = [
  "link_flip",
  "device_reboot",
  "arp_added",
  "arp_removed",
  "arp_conflict",
  "unknown_device"
] as const;

const SEVERITIES = ["info", "warning", "critical"] as const;

function eventTypeLabel(type: string): string {
  return t(`device.snmp_events.type_${type}`);
}

function severityClass(severity: string): string {
  if (severity === "critical") {
    return "status-danger";
  }
  if (severity === "warning") {
    return "status-warning";
  }
  return "status-info";
}

/** detail JSONB 摘要：按类型挑关键字段拼一行文案 */
function detailText(event: SnmpEvent): string {
  const detail = event.detail as Record<string, unknown> | null;
  if (!detail || typeof detail !== "object") {
    return "";
  }
  const parts: string[] = [];
  const push = (labelKey: string, value: unknown) => {
    if (value != null && value !== "") {
      parts.push(`${t(labelKey)}: ${String(value)}`);
    }
  };
  push("device.direction", detail.direction);
  push("device.downtime", detail.downtime);
  push("device.ip_address", detail.ip);
  push("device.mac_address", detail.mac);
  push("device.vlan_id", detail.vlan_id);
  return parts.join(" · ");
}

const resetting = ref(false);
function resetFilters(): void {
  resetting.value = true;
  filters.event_type = "";
  filters.severity = "";
  filters.start = "";
  filters.end = "";
  resetting.value = false;
}

onMounted(() => {
  // 设备下拉与首屏列表由 useQuery/usePagedList 自动加载
});
</script>

<template>
  <ModalShell size="lg" :embedded="embedded" @close="emit('close')">
    <template #title>{{ t("device.snmp_events.title") }}</template>
    <div class="modal-body-scroll">
      <!-- 筛选工具区 -->
      <div class="snmp-events-filters">
        <label class="snmp-events-filter">
          <span>{{ t("device.snmp_events.filter_device") }}</span>
          <select v-model="filters.device_id">
            <option value="">{{ t("device.snmp_events.all_devices") }}</option>
            <option v-for="device in devicesQuery.data.value ?? []" :key="device.id" :value="device.id">
              {{ device.name || device.id }}
            </option>
          </select>
        </label>
        <label class="snmp-events-filter">
          <span>{{ t("device.snmp_events.filter_type") }}</span>
          <select v-model="filters.event_type">
            <option value="">{{ t("device.snmp_events.all_types") }}</option>
            <option v-for="type in EVENT_TYPES" :key="type" :value="type">{{ eventTypeLabel(type) }}</option>
          </select>
        </label>
        <label class="snmp-events-filter">
          <span>{{ t("device.snmp_events.filter_severity") }}</span>
          <select v-model="filters.severity">
            <option value="">{{ t("device.snmp_events.all_severities") }}</option>
            <option v-for="sev in SEVERITIES" :key="sev" :value="sev">{{ t(`device.snmp_events.sev_${sev}`) }}</option>
          </select>
        </label>
        <label class="snmp-events-filter">
          <span>{{ t("device.snmp_events.filter_start") }}</span>
          <input v-model="filters.start" type="datetime-local">
        </label>
        <label class="snmp-events-filter">
          <span>{{ t("device.snmp_events.filter_end") }}</span>
          <input v-model="filters.end" type="datetime-local">
        </label>
        <button type="button" class="btn btn-secondary btn-sm" :disabled="resetting" @click="resetFilters">
          {{ t("common.reset") }}
        </button>
      </div>

      <p v-if="list.isError" class="snmp-events-error">{{ list.errorMessage }}</p>

      <table class="data-table">
        <thead>
          <tr>
            <th class="index-column">#</th>
            <th>{{ t("device.snmp_events.col_time") }}</th>
            <th>{{ t("device.snmp_events.col_device") }}</th>
            <th>{{ t("device.snmp_events.col_type") }}</th>
            <th>{{ t("device.snmp_events.col_interface") }}</th>
            <th class="col-center">{{ t("device.snmp_events.col_severity") }}</th>
            <th>{{ t("device.snmp_events.col_detail") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="list.items.length === 0 && !list.isError">
            <td colspan="7" class="text-center">{{ t("common.no_data") }}</td>
          </tr>
          <tr v-for="(event, index) in list.items" v-else :key="event.id">
            <td class="index-column">{{ seqIndex(list, index) }}</td>
            <td>{{ new Date(event.occurred_at).toLocaleString() }}</td>
            <td>{{ deviceName(event.device_id) }}</td>
            <td>{{ eventTypeLabel(event.event_type) }}</td>
            <td>{{ event.interface || "-" }}</td>
            <td class="col-center">
              <span class="status-badge" :class="severityClass(event.severity)">
                {{ t(`device.snmp_events.sev_${event.severity}`) }}
              </span>
            </td>
            <td>{{ detailText(event) || "-" }}</td>
          </tr>
        </tbody>
      </table>

      <Pagination
        :page="list.page"
        :total-pages="list.totalPages"
        :total="list.total"
        :page-size="list.pageSize"
        :page-size-options="list.pageSizeOptions"
        @page-change="(p) => (list.page = p)"
        @page-size-change="(s) => list.changePageSize(s)"
        @hover-page="list.prefetch"
      />
    </div>
  </ModalShell>
</template>

<style scoped>
.snmp-events-filters {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: flex-end;
  margin-bottom: 12px;
}
.snmp-events-filter {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
}
.snmp-events-filter span {
  color: var(--text-muted, #888);
}
.snmp-events-error {
  color: var(--danger, #d33);
  margin-bottom: 8px;
}
</style>
