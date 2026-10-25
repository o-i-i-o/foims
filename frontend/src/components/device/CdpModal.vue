<script setup lang="ts">
/**
 * CDP 邻居弹窗（CISCO-CDP-MIB cdpCacheTable，仿 LldpModal）：
 * - GET /devices/{id}/cdp，表格列：本地端口/邻居设备/邻居端口/管理地址/平台；
 * - 同步：POST /devices/{id}/cdp/sync（同时产出邻居增删事件）后重新加载；
 * - 非 Cisco 设备返回空集，展示空态。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, apiPost } from "../../api/client";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { DeviceCdp } from "../../types/api/generated/DeviceCdp";
import type { DeviceWithDetails } from "../../types/api/generated/DeviceWithDetails";

const props = defineProps<{ deviceId: string }>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

type LoadPhase = "loading" | "syncing" | "ready" | "empty" | "error";

const phase = ref<LoadPhase>("loading");
const errorMsg = ref("");
const deviceName = ref("");
const neighbors = ref<DeviceCdp[]>([]);

async function loadData(): Promise<void> {
  phase.value = "loading";
  try {
    const deviceResult = await apiGet<DeviceWithDetails>(`/api/resources/devices/${props.deviceId}`);
    if (!deviceResult.success) {
      phase.value = "error";
      errorMsg.value = t("device.cdp.load_failed");
      return;
    }
    deviceName.value = deviceResult.data?.name || "";

    const result = await apiGet<DeviceCdp[]>(`/api/resources/devices/${props.deviceId}/cdp`);
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.cdp.load_failed")}: ${result.message || ""}`;
      return;
    }
    neighbors.value = result.data || [];
    phase.value = neighbors.value.length === 0 ? "empty" : "ready";
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("common.load_failed")}: ${message}`;
  }
}

async function syncData(): Promise<void> {
  phase.value = "syncing";
  try {
    const result = await apiPost<unknown>(`/api/resources/devices/${props.deviceId}/cdp/sync`, {});
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.cdp.sync_failed")}: ${result.message || ""}`;
      return;
    }
    await loadData();
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("device.cdp.sync_failed")}: ${message}`;
  }
}

function orDash(value: string | null | undefined): string {
  return value || "-";
}

loadData();
</script>

<template>
  <ModalShell size="lg" @close="emit('close')">
    <template #title>
      {{ t("device.cdp.title") }} - {{ deviceName }}
      <template v-if="phase === 'ready' || phase === 'empty'">
        ({{ neighbors.length }} {{ t("common.records") }})
      </template>
    </template>
    <div class="modal-body-scroll">
      <div v-if="phase === 'loading' || phase === 'syncing'" class="modal-loading" role="status" aria-live="polite">
        <div class="spinner"></div>
        <p class="loading-text">
          {{ phase === "syncing" ? t("device.cdp.syncing") : t("device.cdp.loading") }}
        </p>
      </div>
      <div v-else-if="phase === 'error'" class="modal-loading">
        <p class="mac-lldp-text-error">{{ errorMsg }}</p>
      </div>
      <div v-else-if="phase === 'empty'" class="modal-loading">
        <p class="mac-lldp-text-muted">{{ t("device.cdp.empty") }}</p>
        <button type="button" class="icon-btn btn-primary" :title="t('device.sync_from_snmp')" @click="syncData">
          <NavIcon name="refresh" />
        </button>
      </div>
      <table v-else class="lldp-table">
        <thead>
          <tr>
            <th>{{ t("device.cdp.col_local_port") }}</th>
            <th>{{ t("device.cdp.col_neighbor") }}</th>
            <th>{{ t("device.cdp.col_neighbor_port") }}</th>
            <th>{{ t("device.cdp.col_address") }}</th>
            <th>{{ t("device.cdp.col_platform") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(n, i) in neighbors" :key="`${n.local_port}-${i}`">
            <td class="lldp-port">{{ n.local_port }}</td>
            <td>{{ orDash(n.neighbor_device_id) }}</td>
            <td>{{ orDash(n.neighbor_port) }}</td>
            <td class="lldp-chassis">{{ orDash(n.neighbor_address) }}</td>
            <td class="lldp-desc">{{ orDash(n.neighbor_platform) }}</td>
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
