<script setup lang="ts">
/**
 * 拓扑设备详情浮层（移植自 modals/visualization/topology-detail-modal.html +
 * TopologyModal.js）。
 *
 * 行为契约：
 * - 三页签（端口 / MAC 表 / LLDP 邻居）互斥展示，切换即重新拉取该面板数据
 *   （数据由 SNMP 周期性刷新，缓存会展示过期表）；
 * - 每次加载带代次序号，切页签后旧响应到达即丢弃，避免两面板数据串写；
 * - 坐标区回填画布当前坐标，保存时校验（空串/非有限值/负值拒绝，
 *   `Number("") === 0` 必须显式拦截，否则清空输入会把设备静默移到原点）；
 * - 「移除」把设备移出拓扑（不删设备）。
 *
 * 遮挡与层级沿用 visualization.css 的 .topology-detail-overlay/.topology-detail-modal；
 * 遮罩用 mousedown 关闭：浮层由节点 mouseup 打开，浏览器随后补发的 click
 * 会命中刚出现的全屏遮罩，click 监听会让浮层一闪而过。
 */
import { onBeforeUnmount, ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet } from "../../api/client";
import {
  deviceInterfacesPath,
  getDeviceLldp,
  getDeviceMacs,
  resultList,
} from "../../api/visualization";
import { useModalStack } from "../../composables/useModalStack";
import { showToast } from "../../composables/useToast";
import { fetchAllPages } from "../../utils/pagedFetch";
import type { DeviceInterface } from "../../types/api/generated/DeviceInterface";
import type { DeviceLldp } from "../../types/api/generated/DeviceLldp";
import type { SnmpMac } from "../../types/api/generated/SnmpMac";
import type { Paged } from "../../types/api";

type PanelKey = "ports" | "macs" | "lldp";
/** 面板加载态：ready 有数据 / empty 确认无数据 / error 请求失败 */
type LoadPhase = "loading" | "ready" | "empty" | "error";

interface TabDefinition {
  key: PanelKey;
  labelKey: string;
}

const TABS: readonly TabDefinition[] = [
  { key: "ports", labelKey: "visualization.show_ports" },
  { key: "macs", labelKey: "visualization.show_macs" },
  { key: "lldp", labelKey: "visualization.show_lldp" },
];

const props = defineProps<{
  deviceId: string;
  deviceName: string;
  /** 画布当前坐标（回填输入框；节点缺失时为 null） */
  position: { x: number; y: number } | null;
}>();

const emit = defineEmits<{
  close: [];
  "save-position": [x: number, y: number];
  remove: [];
}>();

const { t } = useI18n();

const activePanel = ref<PanelKey>("ports");
const phases = ref<Record<PanelKey, LoadPhase>>({
  ports: "loading",
  macs: "loading",
  lldp: "loading",
});
const ports = ref<DeviceInterface[]>([]);
const macs = ref<SnmpMac[]>([]);
const neighbors = ref<DeviceLldp[]>([]);
const coordX = ref(String(Math.round(props.position?.x ?? 0)));
const coordY = ref(String(Math.round(props.position?.y ?? 0)));

/** 面板加载代次：切换页签/设备后旧响应到达即作废 */
let loadSeq = 0;

function applyPhase(panel: PanelKey, count: number): void {
  phases.value[panel] = count > 0 ? "ready" : "empty";
}

async function loadPorts(seq: number): Promise<void> {
  const list = await fetchAllPages<DeviceInterface>({
    apiGet: (url) => apiGet<Paged<DeviceInterface>>(url),
    showToast,
    path: deviceInterfacesPath(props.deviceId),
    what: "获取设备端口",
  });
  if (seq !== loadSeq) {
    return;
  }
  ports.value = list;
  applyPhase("ports", list.length);
}

async function loadMacs(seq: number): Promise<void> {
  const result = await getDeviceMacs(props.deviceId);
  if (seq !== loadSeq) {
    return;
  }
  if (!result.success) {
    phases.value.macs = "error";
    return;
  }
  macs.value = resultList(result);
  applyPhase("macs", macs.value.length);
}

async function loadNeighbors(seq: number): Promise<void> {
  const result = await getDeviceLldp(props.deviceId);
  if (seq !== loadSeq) {
    return;
  }
  if (!result.success) {
    phases.value.lldp = "error";
    return;
  }
  neighbors.value = resultList(result);
  applyPhase("lldp", neighbors.value.length);
}

async function loadPanel(panel: PanelKey): Promise<void> {
  const seq = ++loadSeq;
  phases.value[panel] = "loading";
  if (panel === "ports") {
    await loadPorts(seq);
  } else if (panel === "macs") {
    await loadMacs(seq);
  } else {
    await loadNeighbors(seq);
  }
}

/** 页签互斥切换（重复点击当前页签无操作） */
function selectPanel(panel: PanelKey): void {
  if (activePanel.value === panel) {
    return;
  }
  activePanel.value = panel;
  void loadPanel(panel);
}

function savePosition(): void {
  const x = Number(coordX.value);
  const y = Number(coordY.value);
  if (
    coordX.value.trim() === "" ||
    coordY.value.trim() === "" ||
    !Number.isFinite(x) ||
    !Number.isFinite(y) ||
    x < 0 ||
    y < 0
  ) {
    showToast(t("common.check_input"), "warning");
    return;
  }
  emit("save-position", Math.round(x), Math.round(y));
}

/** MAC 表 VLAN 列（null 显示空串） */
function vlanText(vlanId: number | null): string {
  return vlanId === null ? "" : String(vlanId);
}

/** LLDP 邻居端口：优先端口描述，其次端口 ID */
function neighborPort(entry: DeviceLldp): string {
  return entry.neighbor_port_desc || entry.neighbor_port_id || "";
}

void loadPanel("ports");

// Esc 仅关闭最上层弹窗（浮层可能叠在设备详情等弹窗之上）
onBeforeUnmount(useModalStack(() => emit("close")));
</script>

<template>
  <Teleport to="body">
    <div class="topology-detail-overlay" @mousedown.self="emit('close')"></div>
    <div
      class="topology-detail-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="topology-detail-title"
    >
      <header class="topology-detail-header">
        <h3 id="topology-detail-title">{{ deviceName }}</h3>
        <button
          type="button"
          class="topology-detail-close"
          :aria-label="t('common.close')"
          @click="emit('close')"
        >
          &times;
        </button>
      </header>

      <div class="topology-detail-tabs">
        <button
          v-for="tab in TABS"
          :key="tab.key"
          type="button"
          :class="{ active: activePanel === tab.key }"
          @click="selectPanel(tab.key)"
        >
          {{ t(tab.labelKey) }}
        </button>
      </div>

      <div class="topology-detail-body">
        <div v-if="phases[activePanel] === 'loading'" class="topology-loading">
          {{ t("common.loading") }}
        </div>
        <div v-else-if="phases[activePanel] === 'error'" class="topology-error">
          {{ t("common.load_failed") }}
        </div>

        <template v-else-if="activePanel === 'ports'">
          <div v-if="phases.ports === 'empty'" class="topology-empty">
            {{ t("visualization.no_ports_data") }}
          </div>
          <table v-else class="topology-detail-table">
            <thead>
              <tr>
                <th>{{ t("visualization.port_number") }}</th>
                <th>{{ t("visualization.port_name") }}</th>
                <th>{{ t("visualization.port_status") }}</th>
                <th>{{ t("device.vlan_id") }}</th>
                <th>{{ t("device.speed") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="port in ports" :key="port.id">
                <td>{{ port.name }}</td>
                <td>{{ port.description ?? "" }}</td>
                <td>{{ port.status }}</td>
                <td>{{ vlanText(port.vlan_id) }}</td>
                <td>{{ port.speed ?? "" }}</td>
              </tr>
            </tbody>
          </table>
        </template>

        <template v-else-if="activePanel === 'macs'">
          <div v-if="phases.macs === 'empty'" class="topology-empty">
            {{ t("visualization.no_macs_data") }}
          </div>
          <table v-else class="topology-detail-table">
            <thead>
              <tr>
                <th>{{ t("visualization.mac_address") }}</th>
                <th>{{ t("visualization.mac_vlan") }}</th>
                <th>{{ t("visualization.mac_port") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="entry in macs" :key="entry.id">
                <td>{{ entry.mac_address }}</td>
                <td>{{ vlanText(entry.vlan_id) }}</td>
                <td>{{ entry.interface ?? "" }}</td>
              </tr>
            </tbody>
          </table>
        </template>

        <template v-else>
          <div v-if="phases.lldp === 'empty'" class="topology-empty">
            {{ t("visualization.no_lldp_data") }}
          </div>
          <table v-else class="topology-detail-table">
            <thead>
              <tr>
                <th>{{ t("visualization.lldp_port") }}</th>
                <th>{{ t("visualization.lldp_system") }}</th>
                <th>{{ t("visualization.lldp_neighbor_port") }}</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="entry in neighbors" :key="entry.id">
                <td>{{ entry.local_port }}</td>
                <td>{{ entry.neighbor_sys_name ?? "" }}</td>
                <td>{{ neighborPort(entry) }}</td>
              </tr>
            </tbody>
          </table>
        </template>
      </div>

      <footer class="topology-detail-footer">
        <div class="topology-detail-coords">
          <div class="coord-field">
            <label for="topology-detail-x">{{ t("workstation.position_x") }}</label>
            <input id="topology-detail-x" v-model="coordX" type="number" min="0" step="1" />
          </div>
          <div class="coord-field">
            <label for="topology-detail-y">{{ t("workstation.position_y") }}</label>
            <input id="topology-detail-y" v-model="coordY" type="number" min="0" step="1" />
          </div>
          <button type="button" class="btn btn-primary btn-sm" @click="savePosition">
            {{ t("viz.save_position") }}
          </button>
          <button type="button" class="btn btn-danger btn-sm" @click="emit('remove')">
            {{ t("visualization.remove_from_topology") }}
          </button>
        </div>
      </footer>
    </div>
  </Teleport>
</template>
