<script setup lang="ts">
/**
 * VLAN 台账弹窗（Q-BRIDGE-MIB dot1qVlanStaticTable）：
 * - GET /devices/{id}/vlans，表格列：VLAN ID/名称/成员端口数/Untagged端口数/类型；
 * - 成员端口以 JSONB 数组落库（JsonValue），此处按 number[] 收窄取长度；
 * - 同步：POST /devices/{id}/vlans/sync 后重新加载。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, apiPost } from "../../api/client";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { DeviceVlan } from "../../types/api/generated/DeviceVlan";
import type { DeviceWithDetails } from "../../types/api/generated/DeviceWithDetails";

const props = defineProps<{ deviceId: string; embedded?: boolean }>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

type LoadPhase = "loading" | "syncing" | "ready" | "empty" | "error";

const phase = ref<LoadPhase>("loading");
const errorMsg = ref("");
const deviceName = ref("");
const vlans = ref<DeviceVlan[]>([]);

// JSONB 数组长度（成员端口 ifIndex 数组，列已生成 unknown，取长度前先 Array.isArray 收窄）
function arrayLength(value: unknown): number {
  return Array.isArray(value) ? value.length : 0;
}

async function loadData(): Promise<void> {
  phase.value = "loading";
  try {
    const deviceResult = await apiGet<DeviceWithDetails>(`/api/resources/devices/${props.deviceId}`);
    if (!deviceResult.success) {
      phase.value = "error";
      errorMsg.value = t("device.vlan_ledger.load_failed");
      return;
    }
    deviceName.value = deviceResult.data?.name || "";

    const result = await apiGet<DeviceVlan[]>(`/api/resources/devices/${props.deviceId}/vlans`);
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.vlan_ledger.load_failed")}: ${result.message || ""}`;
      return;
    }
    vlans.value = result.data || [];
    phase.value = vlans.value.length === 0 ? "empty" : "ready";
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("common.load_failed")}: ${message}`;
  }
}

async function syncData(): Promise<void> {
  phase.value = "syncing";
  try {
    const result = await apiPost<unknown>(`/api/resources/devices/${props.deviceId}/vlans/sync`, {});
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.vlan_ledger.sync_failed")}: ${result.message || ""}`;
      return;
    }
    await loadData();
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("device.vlan_ledger.sync_failed")}: ${message}`;
  }
}

function memberCount(v: DeviceVlan): number {
  return arrayLength(v.member_ports);
}

function untaggedCount(v: DeviceVlan): number {
  return arrayLength(v.untagged_ports);
}

loadData();
</script>

<template>
  <ModalShell size="lg" :embedded="embedded" @close="emit('close')">
    <template #title>
      {{ t("device.vlan_ledger.title") }} - {{ deviceName }}
      <template v-if="phase === 'ready' || phase === 'empty'">
        ({{ vlans.length }} {{ t("common.records") }})
      </template>
    </template>
    <div class="modal-body-scroll">
      <div v-if="phase === 'loading' || phase === 'syncing'" class="modal-loading" role="status" aria-live="polite">
        <div class="spinner"></div>
        <p class="loading-text">
          {{ phase === "syncing" ? t("device.vlan_ledger.syncing") : t("device.vlan_ledger.loading") }}
        </p>
      </div>
      <div v-else-if="phase === 'error'" class="modal-loading">
        <p class="mac-lldp-text-error">{{ errorMsg }}</p>
      </div>
      <div v-else-if="phase === 'empty'" class="modal-loading">
        <p class="mac-lldp-text-muted">{{ t("device.vlan_ledger.empty") }}</p>
        <button type="button" class="icon-btn btn-primary" :title="t('device.sync_from_snmp')" @click="syncData">
          <NavIcon name="refresh" />
        </button>
      </div>
      <table v-else class="lldp-table">
        <thead>
          <tr>
            <th class="lldp-col-index">{{ t("device.vlan_ledger.col_vlan_id") }}</th>
            <th>{{ t("device.vlan_ledger.col_name") }}</th>
            <th class="lldp-col-index">{{ t("device.vlan_ledger.col_members") }}</th>
            <th class="lldp-col-index">{{ t("device.vlan_ledger.col_untagged") }}</th>
            <th>{{ t("device.vlan_ledger.col_type") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="v in vlans" :key="v.vlan_id">
            <td class="lldp-index">{{ v.vlan_id }}</td>
            <td class="lldp-port">{{ v.name || "-" }}</td>
            <td class="lldp-index">{{ t("device.vlan_ledger.member_count", { count: memberCount(v) }) }}</td>
            <td class="lldp-index">{{ t("device.vlan_ledger.member_count", { count: untaggedCount(v) }) }}</td>
            <td>
              <span class="status-badge" :class="untaggedCount(v) === 0 ? 'status-reserved' : 'status-active'">
                {{ untaggedCount(v) === 0 ? t("device.vlan_ledger.type_trunk") : t("device.vlan_ledger.type_access") }}
              </span>
            </td>
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
