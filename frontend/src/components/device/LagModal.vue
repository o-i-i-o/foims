<script setup lang="ts">
/**
 * LAG 聚合口台账弹窗（IEEE8023-LAG-MIB）：
 * - GET /devices/{id}/lag，表格列：聚合口/成员端口/LACP 状态；
 * - 成员端口名以 JSONB 数组落库（JsonValue），此处收窄为 string[] 后拼接展示；
 * - 同步：POST /devices/{id}/lag/sync 后重新加载。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, apiPost } from "../../api/client";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { DeviceLag } from "../../types/api/generated/DeviceLag";
import type { DeviceWithDetails } from "../../types/api/generated/DeviceWithDetails";

const props = defineProps<{ deviceId: string; embedded?: boolean }>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

type LoadPhase = "loading" | "syncing" | "ready" | "empty" | "error";

const phase = ref<LoadPhase>("loading");
const errorMsg = ref("");
const deviceName = ref("");
const lags = ref<DeviceLag[]>([]);

// 成员端口名（JSONB string[] 优先，退回 ifIndex 数字数组）；
// 先将 JsonValue 列赋给 unknown 变量再判断，避免递归类型参与推断（仿 SnmpEventsModal detailText）
function memberLabels(lag: DeviceLag): string {
  const names: unknown = lag.member_names;
  if (Array.isArray(names) && names.length > 0) {
    return names.join(", ");
  }
  const indexes: unknown = lag.member_ifindexes;
  return Array.isArray(indexes) && indexes.length > 0 ? indexes.join(", ") : "-";
}

// 成员端口数量（取 member_ifindexes JSONB 数组长度）
function memberCount(lag: DeviceLag): number {
  const indexes: unknown = lag.member_ifindexes;
  return Array.isArray(indexes) ? indexes.length : 0;
}

async function loadData(): Promise<void> {
  phase.value = "loading";
  const deviceResult = await apiGet<DeviceWithDetails>(`/api/resources/devices/${props.deviceId}`);
  if (!deviceResult.success) {
    phase.value = "error";
    errorMsg.value = t("device.lag.load_failed");
    return;
  }
  deviceName.value = deviceResult.data?.name || "";

  const result = await apiGet<DeviceLag[]>(`/api/resources/devices/${props.deviceId}/lag`);
  if (!result.success) {
    phase.value = "error";
    errorMsg.value = `${t("device.lag.load_failed")}: ${result.message || ""}`;
    return;
  }
  lags.value = result.data || [];
  phase.value = lags.value.length === 0 ? "empty" : "ready";
}

async function syncData(): Promise<void> {
  phase.value = "syncing";
  const result = await apiPost<unknown>(`/api/resources/devices/${props.deviceId}/lag/sync`, {});
  if (!result.success) {
    phase.value = "error";
    errorMsg.value = `${t("device.lag.sync_failed")}: ${result.message || ""}`;
    return;
  }
  await loadData();
}

loadData();
</script>

<template>
  <ModalShell size="lg" :embedded="embedded" @close="emit('close')">
    <template #title>
      {{ t("device.lag.title") }} - {{ deviceName }}
      <template v-if="phase === 'ready' || phase === 'empty'">
        ({{ lags.length }} {{ t("common.records") }})
      </template>
    </template>
    <div class="modal-body-scroll">
      <div v-if="phase === 'loading' || phase === 'syncing'" class="modal-loading" role="status" aria-live="polite">
        <div class="spinner"></div>
        <p class="loading-text">
          {{ phase === "syncing" ? t("device.lag.syncing") : t("device.lag.loading") }}
        </p>
      </div>
      <div v-else-if="phase === 'error'" class="modal-loading">
        <p class="mac-lldp-text-error">{{ errorMsg }}</p>
      </div>
      <div v-else-if="phase === 'empty'" class="modal-loading">
        <p class="mac-lldp-text-muted">{{ t("device.lag.empty") }}</p>
        <button type="button" class="icon-btn btn-primary" :title="t('device.sync_from_snmp')" @click="syncData">
          <NavIcon name="refresh" />
        </button>
      </div>
      <table v-else class="lldp-table">
        <thead>
          <tr>
            <th class="lldp-col-index">{{ t("device.lag.col_agg") }}</th>
            <th>{{ t("device.lag.col_members") }}</th>
            <th>{{ t("device.lag.col_lacp") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="lag in lags" :key="lag.agg_ifindex">
            <td class="lldp-port">
              {{ lag.agg_name || `#${lag.agg_ifindex}` }}
              <span class="lldp-chassis">({{ t("device.lag.member_count", { count: memberCount(lag) }) }})</span>
            </td>
            <td class="lldp-desc" :title="memberLabels(lag)">{{ memberLabels(lag) }}</td>
            <td>{{ lag.lacp_state || "-" }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <template #footer>
      <button type="button" class="btn-primary" :disabled="phase === 'syncing'" @click="syncData">
        {{ t("device.sync_from_snmp") }}
      </button>
    </template>
  </ModalShell>
</template>
