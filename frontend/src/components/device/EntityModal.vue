<script setup lang="ts">
/**
 * 硬件实体清单弹窗（ENTITY-MIB / ENTITY-SENSOR-MIB）：
 * - 页签「实体」：GET /devices/{id}/entities，表格列：索引/类别/名称/描述/型号/
 *   序列号/制造商/硬件版本/固件版本/软件版本/FRU/父索引；
 * - 页签「传感器」：GET /devices/{id}/sensors，表格列：实体索引/类型/量纲/精度/
 *   读数/状态/名称；
 * - 同步：POST /devices/{id}/entities/sync（一次写实体 + 传感器）后重新加载。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet, apiPost } from "../../api/client";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { DeviceEntity } from "../../types/api/generated/DeviceEntity";
import type { DeviceSensor } from "../../types/api/generated/DeviceSensor";
import type { DeviceWithDetails } from "../../types/api/generated/DeviceWithDetails";

const props = defineProps<{ deviceId: string; embedded?: boolean }>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

// loading 初始加载 / syncing 同步中 / ready 就绪 / empty 无数据 / error 失败
type LoadPhase = "loading" | "syncing" | "ready" | "empty" | "error";

const phase = ref<LoadPhase>("loading");
const errorMsg = ref("");
const deviceName = ref("");
const entities = ref<DeviceEntity[]>([]);
const sensors = ref<DeviceSensor[]>([]);
const tab = ref<"entities" | "sensors">("entities");

async function loadData(): Promise<void> {
  phase.value = "loading";
  try {
    const deviceResult = await apiGet<DeviceWithDetails>(`/api/resources/devices/${props.deviceId}`);
    if (!deviceResult.success) {
      phase.value = "error";
      errorMsg.value = t("device.entity.load_failed");
      return;
    }
    deviceName.value = deviceResult.data?.name || "";

    const entityResult = await apiGet<DeviceEntity[]>(`/api/resources/devices/${props.deviceId}/entities`);
    if (!entityResult.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.entity.load_failed")}: ${entityResult.message || ""}`;
      return;
    }
    const sensorResult = await apiGet<DeviceSensor[]>(`/api/resources/devices/${props.deviceId}/sensors`);
    entities.value = entityResult.data || [];
    sensors.value = sensorResult.success ? sensorResult.data || [] : [];
    phase.value = entities.value.length === 0 ? "empty" : "ready";
  } catch (error) {
    // apiGet/apiPost 不 reject，此分支为防御性保留；非 Error 抛出值按空串回退
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("common.load_failed")}: ${message}`;
  }
}

async function syncData(): Promise<void> {
  phase.value = "syncing";
  try {
    const result = await apiPost<unknown>(`/api/resources/devices/${props.deviceId}/entities/sync`, {});
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = `${t("device.entity.sync_failed")}: ${result.message || ""}`;
      return;
    }
    await loadData();
  } catch (error) {
    const message = error instanceof Error ? error.message : "";
    phase.value = "error";
    errorMsg.value = `${t("device.entity.sync_failed")}: ${message}`;
  }
}

// 空值兜底显示
function orDash(value: string | number | null | undefined): string {
  return value === null || value === undefined || value === "" ? "-" : String(value);
}

loadData();
</script>

<template>
  <ModalShell size="lg" :embedded="embedded" @close="emit('close')">
    <template #title>
      {{ t("device.entity.title") }} - {{ deviceName }}
      <template v-if="phase === 'ready'">
        ({{ entities.length }} {{ t("common.records") }})
      </template>
    </template>
    <div class="modal-body-scroll">
      <div v-if="phase === 'loading' || phase === 'syncing'" class="modal-loading" role="status" aria-live="polite">
        <div class="spinner"></div>
        <p class="loading-text">
          {{ phase === "syncing" ? t("device.entity.syncing") : t("device.entity.loading") }}
        </p>
      </div>
      <div v-else-if="phase === 'error'" class="modal-loading">
        <p class="mac-lldp-text-error">{{ errorMsg }}</p>
      </div>
      <div v-else-if="phase === 'empty'" class="modal-loading">
        <p class="mac-lldp-text-muted">{{ t("device.entity.empty") }}</p>
        <button type="button" class="icon-btn btn-primary" :title="t('device.sync_from_snmp')" @click="syncData">
          <NavIcon name="refresh" />
        </button>
      </div>
      <div v-else>
        <div class="tab-toolbar">
          <div class="tab-buttons">
            <button type="button" class="tab-btn" :class="{ active: tab === 'entities' }" @click="tab = 'entities'">
              {{ t("device.entity.tab_entities") }} ({{ entities.length }})
            </button>
            <button type="button" class="tab-btn" :class="{ active: tab === 'sensors' }" @click="tab = 'sensors'">
              {{ t("device.entity.tab_sensors") }} ({{ sensors.length }})
            </button>
          </div>
        </div>

        <table v-if="tab === 'entities'" class="lldp-table">
          <thead>
            <tr>
              <th class="lldp-col-index">{{ t("device.entity.col_ent_index") }}</th>
              <th>{{ t("device.entity.col_class") }}</th>
              <th>{{ t("device.entity.col_name") }}</th>
              <th>{{ t("device.entity.col_descr") }}</th>
              <th>{{ t("device.entity.col_model") }}</th>
              <th>{{ t("device.entity.col_serial") }}</th>
              <th>{{ t("device.entity.col_mfg") }}</th>
              <th>{{ t("device.entity.col_hw") }}</th>
              <th>{{ t("device.entity.col_fw") }}</th>
              <th>{{ t("device.entity.col_sw") }}</th>
              <th>{{ t("device.entity.col_fru") }}</th>
              <th class="lldp-col-index">{{ t("device.entity.col_parent") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="e in entities" :key="e.ent_index">
              <td class="lldp-index">{{ e.ent_index }}</td>
              <td>
                <span class="status-badge status-reserved">{{ orDash(e.ent_class) }}</span>
              </td>
              <td class="lldp-port">{{ orDash(e.name) }}</td>
              <td class="lldp-desc" :title="e.descr || ''">{{ orDash(e.descr) }}</td>
              <td>{{ orDash(e.model_name) }}</td>
              <td>{{ orDash(e.serial_num) }}</td>
              <td>{{ orDash(e.mfg_name) }}</td>
              <td>{{ orDash(e.hw_rev) }}</td>
              <td>{{ orDash(e.fw_rev) }}</td>
              <td>{{ orDash(e.sw_rev) }}</td>
              <td>{{ e.is_fru === null ? "-" : (e.is_fru ? t("device.entity.fru_yes") : t("device.entity.fru_no")) }}</td>
              <td class="lldp-index">{{ orDash(e.parent_index) }}</td>
            </tr>
          </tbody>
        </table>

        <table v-else class="lldp-table">
          <thead>
            <tr>
              <th class="lldp-col-index">{{ t("device.sensors.col_ent_index") }}</th>
              <th>{{ t("device.sensors.col_name") }}</th>
              <th class="lldp-col-index">{{ t("device.sensors.col_type") }}</th>
              <th class="lldp-col-index">{{ t("device.sensors.col_scale") }}</th>
              <th class="lldp-col-index">{{ t("device.sensors.col_precision") }}</th>
              <th>{{ t("device.sensors.col_value") }}</th>
              <th class="lldp-col-index">{{ t("device.sensors.col_status") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="sensors.length === 0">
              <td colspan="7" class="lldp-desc">{{ t("device.sensors.empty") }}</td>
            </tr>
            <tr v-for="(s, i) in sensors" :key="`${s.ent_index}-${i}`">
              <td class="lldp-index">{{ s.ent_index }}</td>
              <td class="lldp-port">{{ orDash(s.name) }}</td>
              <td class="lldp-index">{{ orDash(s.sensor_type) }}</td>
              <td class="lldp-index">{{ orDash(s.sensor_scale) }}</td>
              <td class="lldp-index">{{ orDash(s.sensor_precision) }}</td>
              <td>{{ orDash(s.sensor_value) }}</td>
              <td class="lldp-index">{{ orDash(s.sensor_status) }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
    <template #footer>
      <button type="button" class="btn-primary" :disabled="phase === 'syncing'" @click="syncData">
        {{ t("device.sync_from_snmp") }}
      </button>
    </template>
  </ModalShell>
</template>
