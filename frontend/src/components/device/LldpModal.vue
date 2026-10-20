<script setup>
/**
 * LLDP 邻居弹窗（移植自旧版 modals/device/lldp-modal.html +
 * deviceMacLldp.js viewLldpNeighbors/renderLldpTable/syncLldpData）：
 * - 加载设备名 + GET /devices/{id}/lldp-neighbors，表格六列：序号/本地端口/
 *   邻居设备/邻居端口/Chassis ID/系统描述；
 * - 邻居端口展示：neighbor_port_id 非 MAC 格式时优先取 port_id，
 *   否则退回 port_desc，再退回 port_id（与旧版一致）；
 * - 系统描述列 title 悬停展示全文；
 * - 同步：POST /devices/{id}/lldp/sync 后重新加载；空数据态提供同步按钮。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, apiPost } from "../../api/client.js";
import { showToast } from "../../composables/useToast.js";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";

const props = defineProps({
  deviceId: { type: String, required: true }
});

const emit = defineEmits(["close"]);

const { t } = useI18n();

// ==================== 加载状态机（与 ArpModal 同构） ====================

const phase = ref("loading");
const errorMsg = ref("");
const deviceName = ref("");
const neighbors = ref([]);

async function loadData() {
  phase.value = "loading";
  try {
    const deviceResult = await apiGet(`/api/resources/devices/${props.deviceId}`);
    if (!deviceResult.success) {
      phase.value = "error";
      errorMsg.value = t("device.load_failed");
      return;
    }
    deviceName.value = deviceResult.data?.name || "";

    const result = await apiGet(`/api/resources/devices/${props.deviceId}/lldp-neighbors`);
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.load_lldp_failed")}: ${result.message || ""}`;
      return;
    }
    neighbors.value = result.data || [];
    phase.value = neighbors.value.length === 0 ? "empty" : "ready";
  } catch (error) {
    phase.value = "error";
    errorMsg.value = `${t("common.load_failed")}: ${error.message}`;
  }
}

async function syncLldpData() {
  phase.value = "syncing";
  try {
    const result = await apiPost(`/api/resources/devices/${props.deviceId}/lldp/sync`, {});
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.sync_failed")}: ${result.message || ""}`;
      return;
    }
    await loadData();
  } catch (error) {
    phase.value = "error";
    errorMsg.value = `${t("device.sync_failed")}: ${error.message}`;
  }
}

// ==================== 邻居端口展示逻辑（与旧版一致） ====================

function isMac(str) {
  return /^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/.test(str);
}

function neighborPortOf(n) {
  if (n.neighbor_port_id && !isMac(n.neighbor_port_id)) {
    return n.neighbor_port_id;
  }
  if (n.neighbor_port_desc) {
    return n.neighbor_port_desc;
  }
  return n.neighbor_port_id || "-";
}

const rows = computed(() =>
  neighbors.value.map((n, i) => ({
    index: i + 1,
    localPort: n.local_port || "-",
    sysName: n.neighbor_sys_name || "-",
    port: neighborPortOf(n),
    chassisId: n.neighbor_chassis_id || "-",
    sysDesc: n.neighbor_sys_desc || "",
    sysDescText: n.neighbor_sys_desc || "-"
  }))
);

loadData();
</script>

<template>
  <ModalShell size="lg" @close="emit('close')">
    <template #title>
      {{ t("device.lldp_neighbors") }} - {{ deviceName }}
      <template v-if="phase === 'ready' || phase === 'empty'">
        ({{ neighbors.length }} {{ t("common.records") }})
      </template>
    </template>
    <div class="modal-body-scroll">
      <div v-if="phase === 'loading' || phase === 'syncing'" class="modal-loading" role="status" aria-live="polite">
        <div class="spinner"></div>
        <p class="loading-text">
          {{ phase === "syncing" ? t("device.syncing_lldp") : t("device.loading_lldp") }}
        </p>
      </div>
      <div v-else-if="phase === 'error'" class="modal-loading">
        <p class="mac-lldp-text-error">{{ errorMsg }}</p>
      </div>
      <div v-else-if="phase === 'empty'" class="modal-loading">
        <p class="mac-lldp-text-muted">{{ t("device.no_lldp_data") }}</p>
        <button type="button" class="icon-btn btn-primary" :title="t('device.sync_from_snmp')" @click="syncLldpData">
          <NavIcon name="refresh" />
        </button>
      </div>
      <table v-else class="lldp-table">
        <thead>
          <tr>
            <th class="lldp-col-index">{{ t("common.index") }}</th>
            <th>{{ t("device.local_port") }}</th>
            <th>{{ t("device.neighbor_device") }}</th>
            <th>{{ t("device.neighbor_port") }}</th>
            <th>Chassis ID</th>
            <th>{{ t("device.system_description") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.index">
            <td class="lldp-index">{{ row.index }}</td>
            <td class="lldp-port">{{ row.localPort }}</td>
            <td>{{ row.sysName }}</td>
            <td>{{ row.port }}</td>
            <td class="lldp-chassis">{{ row.chassisId }}</td>
            <td class="lldp-desc" :title="row.sysDesc">{{ row.sysDescText }}</td>
          </tr>
        </tbody>
      </table>
    </div>
    <template #footer>
      <button type="button" class="btn btn-primary" @click="syncLldpData">
        {{ t("device.sync_from_snmp") }}
      </button>
    </template>
  </ModalShell>
</template>
