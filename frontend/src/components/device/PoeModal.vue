<script setup lang="ts">
/**
 * PoE 供电端口弹窗（IEEE802.3af PFN-MIB pethPsePortTable）：
 * - GET /devices/{id}/poe，表格列：端口/管理状态/供电状态/功率类别/实测功率；
 * - detection_status 徽标配色：3=供电中(绿)、2=检测中(黄)、1/4=禁用/故障(红)；
 * - 同步：POST /devices/{id}/poe/sync（同时产出供电中断事件）后重新加载。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, apiPost } from "../../api/client";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { DevicePoePort } from "../../types/api/generated/DevicePoePort";
import type { DeviceWithDetails } from "../../types/api/generated/DeviceWithDetails";

const props = defineProps<{ deviceId: string; embedded?: boolean }>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

type LoadPhase = "loading" | "syncing" | "ready" | "empty" | "error";

const phase = ref<LoadPhase>("loading");
const errorMsg = ref("");
const deviceName = ref("");
const ports = ref<DevicePoePort[]>([]);

async function loadData(): Promise<void> {
  phase.value = "loading";
  try {
    const deviceResult = await apiGet<DeviceWithDetails>(`/api/resources/devices/${props.deviceId}`);
    if (!deviceResult.success) {
      phase.value = "error";
      errorMsg.value = t("device.poe.load_failed");
      return;
    }
    deviceName.value = deviceResult.data?.name || "";

    const result = await apiGet<DevicePoePort[]>(`/api/resources/devices/${props.deviceId}/poe`);
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.poe.load_failed")}: ${result.message || ""}`;
      return;
    }
    ports.value = result.data || [];
    phase.value = ports.value.length === 0 ? "empty" : "ready";
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("common.load_failed")}: ${message}`;
  }
}

async function syncData(): Promise<void> {
  phase.value = "syncing";
  try {
    const result = await apiPost<unknown>(`/api/resources/devices/${props.deviceId}/poe/sync`, {});
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.poe.sync_failed")}: ${result.message || ""}`;
      return;
    }
    await loadData();
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("device.poe.sync_failed")}: ${message}`;
  }
}

// detection_status → 文案键与徽标颜色类
function detectLabel(status: number | null): string {
  switch (status) {
    case 2:
      return t("device.poe.detect_searching");
    case 3:
      return t("device.poe.detect_delivering");
    case 4:
      return t("device.poe.detect_faulty");
    case 1:
      return t("device.poe.detect_disabled");
    default:
      return "-";
  }
}

function detectClass(status: number | null): string {
  switch (status) {
    case 3:
      return "status-active";
    case 2:
      return "status-reserved";
    case 1:
    case 4:
      return "status-inactive";
    default:
      return "status-reserved";
  }
}

function adminLabel(status: number | null): string {
  if (status === 1) {
    return t("device.poe.admin_disabled");
  }
  if (status === 2) {
    return t("device.poe.admin_enabled");
  }
  return "-";
}

function powerText(port: DevicePoePort): string {
  return port.measured_power_w === null ? "-" : `${port.measured_power_w} ${t("device.poe.power_unit")}`;
}

loadData();
</script>

<template>
  <ModalShell size="lg" :embedded="embedded" @close="emit('close')">
    <template #title>
      {{ t("device.poe.title") }} - {{ deviceName }}
      <template v-if="phase === 'ready' || phase === 'empty'">
        ({{ ports.length }} {{ t("common.records") }})
      </template>
    </template>
    <div class="modal-body-scroll">
      <div v-if="phase === 'loading' || phase === 'syncing'" class="modal-loading" role="status" aria-live="polite">
        <div class="spinner"></div>
        <p class="loading-text">
          {{ phase === "syncing" ? t("device.poe.syncing") : t("device.poe.loading") }}
        </p>
      </div>
      <div v-else-if="phase === 'error'" class="modal-loading">
        <p class="mac-lldp-text-error">{{ errorMsg }}</p>
      </div>
      <div v-else-if="phase === 'empty'" class="modal-loading">
        <p class="mac-lldp-text-muted">{{ t("device.poe.empty") }}</p>
        <button type="button" class="icon-btn btn-primary" :title="t('device.sync_from_snmp')" @click="syncData">
          <NavIcon name="refresh" />
        </button>
      </div>
      <table v-else class="lldp-table">
        <thead>
          <tr>
            <th>{{ t("device.poe.col_port") }}</th>
            <th>{{ t("device.poe.col_admin") }}</th>
            <th>{{ t("device.poe.col_detect") }}</th>
            <th class="lldp-col-index">{{ t("device.poe.col_class") }}</th>
            <th>{{ t("device.poe.col_power") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="p in ports" :key="p.if_index">
            <td class="lldp-port">{{ p.if_name || `#${p.if_index}` }}</td>
            <td>{{ adminLabel(p.admin_status) }}</td>
            <td>
              <span class="status-badge" :class="detectClass(p.detection_status)">{{ detectLabel(p.detection_status) }}</span>
            </td>
            <td class="lldp-index">{{ p.power_class === null ? "-" : p.power_class }}</td>
            <td>{{ powerText(p) }}</td>
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
