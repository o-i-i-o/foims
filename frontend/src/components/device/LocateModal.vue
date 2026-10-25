<script setup lang="ts">
/**
 * 终端定位弹窗：
 * - MAC / IP 单输入切换（二选一，均空时禁止查询）；
 * - GET /api/snmp/locate?mac=...|ip=... 返回命中行数组，展示设备/端口/VLAN/IP/MAC/登记/来源/时间；
 * - registered=false 标为「未知终端」徽标（warning），source 区分 arp 学习 / static 网口直查；
 * - 从 MAC 表「定位此终端」进入时预填 mac 并立即查询。
 */
import { nextTick, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet } from "../../api/client";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { DeviceLocateResult } from "../../types/api/generated/DeviceLocateResult";

const props = withDefaults(defineProps<{ prefillMac?: string }>(), { prefillMac: "" });

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

type Mode = "mac" | "ip";
const mode = ref<Mode>(props.prefillMac ? "mac" : "ip");
const keyword = ref(props.prefillMac);

type Phase = "idle" | "loading" | "ready" | "error";
const phase = ref<Phase>("idle");
const errorMsg = ref("");
const rows = ref<DeviceLocateResult[]>([]);

async function query(): Promise<void> {
  const value = keyword.value.trim();
  if (!value) {
    return;
  }
  phase.value = "loading";
  errorMsg.value = "";
  const param = mode.value === "mac" ? "mac" : "ip";
  try {
    const result = await apiGet<DeviceLocateResult[]>(
      `/api/snmp/locate?${new URLSearchParams({ [param]: value })}`
    );
    if (!result.success) {
      phase.value = "error";
      errorMsg.value = result.message || t("device.locate.failed");
      return;
    }
    rows.value = result.data ?? [];
    phase.value = "ready";
  } catch (error) {
    phase.value = "error";
    errorMsg.value = error instanceof Error ? error.message : t("common.load_failed");
  }
}

function switchMode(next: Mode): void {
  if (mode.value === next) {
    return;
  }
  mode.value = next;
  keyword.value = "";
  phase.value = "idle";
  rows.value = [];
}

function sourceLabel(source: string): string {
  return source === "static" ? t("device.locate.source_static") : t("device.locate.source_arp");
}

onMounted(() => {
  if (props.prefillMac) {
    // 预填 MAC 时自动执行一次查询（进入即用）
    void nextTick(query);
  }
});
</script>

<template>
  <ModalShell size="lg" @close="emit('close')">
    <template #title>{{ t("device.locate.title") }}</template>
    <div class="modal-body-scroll">
      <div class="locate-toolbar">
        <div class="tab-buttons" role="tablist">
          <button
            type="button"
            class="tab-btn"
            :class="{ active: mode === 'mac' }"
            role="tab"
            :aria-selected="mode === 'mac'"
            @click="switchMode('mac')"
          >
            {{ t("device.locate.by_mac") }}
          </button>
          <button
            type="button"
            class="tab-btn"
            :class="{ active: mode === 'ip' }"
            role="tab"
            :aria-selected="mode === 'ip'"
            @click="switchMode('ip')"
          >
            {{ t("device.locate.by_ip") }}
          </button>
        </div>
        <input
          v-model="keyword"
          type="text"
          class="tab-search locate-input"
          :placeholder="mode === 'mac' ? t('device.locate.placeholder_mac') : t('device.locate.placeholder_ip')"
          :aria-label="mode === 'mac' ? t('device.mac_address') : t('device.ip_address')"
          @keydown.enter.prevent="query"
        >
        <button type="button" class="icon-btn btn-primary" :disabled="!keyword.trim() || phase === 'loading'" :title="t('device.locate.search')" @click="query">
          <NavIcon name="eye" />
        </button>
      </div>

      <div v-if="phase === 'loading'" class="modal-loading" role="status" aria-live="polite">
        <div class="spinner"></div>
        <p class="loading-text">{{ t("device.locate.searching") }}</p>
      </div>
      <div v-else-if="phase === 'error'" class="modal-loading">
        <p class="locate-text-error">{{ errorMsg }}</p>
      </div>
      <div v-else-if="phase === 'ready'" class="locate-result">
        <p class="locate-summary">{{ t("device.locate.hits", { count: rows.length }) }}</p>
        <table v-if="rows.length > 0" class="data-table">
          <thead>
            <tr>
              <th>{{ t("device.locate.col_device") }}</th>
              <th>{{ t("device.locate.col_interface") }}</th>
              <th class="col-center">{{ t("device.vlan_id") }}</th>
              <th>{{ t("device.ip_address") }}</th>
              <th>{{ t("device.mac_address") }}</th>
              <th class="col-center">{{ t("device.locate.col_registered") }}</th>
              <th class="col-center">{{ t("device.locate.col_source") }}</th>
              <th>{{ t("device.locate.col_time") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(row, index) in rows" :key="`${row.device_id}-${row.interface}-${row.ip_address}-${index}`">
              <td>{{ row.device_name || row.device_id }}</td>
              <td>{{ row.interface || "-" }}</td>
              <td class="col-center">{{ row.vlan_id != null ? String(row.vlan_id) : "-" }}</td>
              <td>{{ row.ip_address || "-" }}</td>
              <td>{{ row.mac_address || "-" }}</td>
              <td class="col-center">
                <span class="status-badge" :class="row.registered ? 'status-active' : 'status-warning'">
                  {{ row.registered ? t("device.locate.registered") : t("device.locate.unknown") }}
                </span>
              </td>
              <td class="col-center">{{ sourceLabel(row.source) }}</td>
              <td>{{ row.occurred_at || row.updated_at ? new Date((row.occurred_at ?? row.updated_at) as string).toLocaleString() : "-" }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="locate-empty">{{ t("device.locate.no_result") }}</p>
      </div>
    </div>
  </ModalShell>
</template>

<style scoped>
.locate-toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 12px;
}
.locate-input {
  flex: 1;
}
.locate-text-error {
  color: var(--danger, #d33);
}
.locate-summary {
  margin-bottom: 8px;
  color: var(--text-muted, #888);
  font-size: 13px;
}
.locate-empty {
  color: var(--text-muted, #888);
  text-align: center;
}
</style>
