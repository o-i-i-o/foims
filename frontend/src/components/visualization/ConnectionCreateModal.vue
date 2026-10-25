<script setup lang="ts">
/**
 * 创建拓扑连线弹窗（移植自 modals/visualization/topology-connection-modal.html +
 * visualizationManager.openTopologyConnectionModal）。
 *
 * 行为契约：
 * - 手动物理连线只需两端设备；逻辑连线（链路聚合）额外要求两侧各选成员端口；
 * - 端口下拉在切换到逻辑类型或切换设备时重新加载，带独立代次序号：
 *   快速切换设备时旧响应晚到会把过期端口写进选择框，提交出端口与设备不匹配的组合；
 * - 设备变更时清空该侧已选端口，避免残留不可见的 option 值被提交；
 * - 提交前做重复连线预检（同设备对，含方向翻转）与自连拦截。
 */
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import ModalShell from "../ModalShell.vue";
import { apiGet } from "../../api/client";
import { createTopologyConnection, deviceInterfacesPath } from "../../api/visualization";
import { showToast } from "../../composables/useToast";
import type { DeviceInterface } from "../../types/api/generated/DeviceInterface";
import type { TopologyConnectionRequest } from "../../types/api/generated/TopologyConnectionRequest";
import type { TopologyConnectionView } from "../../types/api/generated/TopologyConnectionView";
import type { TopologyNodeWithDevice } from "../../types/api/generated/TopologyNodeWithDevice";
import type { Paged } from "../../types/api";

type ConnectionType = "physical" | "logical";
type DeviceSide = "source" | "target";

const props = defineProps<{
  /** 候选设备（画布上的拓扑节点） */
  nodes: TopologyNodeWithDevice[];
  /** 已有连线（重复预检） */
  connections: TopologyConnectionView[];
}>();

const emit = defineEmits<{ close: []; created: [] }>();

const { t } = useI18n();

const type = ref<ConnectionType>("physical");
const sourceDeviceId = ref("");
const targetDeviceId = ref("");
const sourcePortIds = ref<string[]>([]);
const targetPortIds = ref<string[]>([]);
const label = ref("");
const sourcePorts = ref<DeviceInterface[]>([]);
const targetPorts = ref<DeviceInterface[]>([]);
const submitting = ref(false);

/** 端口加载代次：每侧独立计数，切换设备后旧响应作废 */
let sourceSeq = 0;
let targetSeq = 0;

/** 端口 option 文案：名称（描述），缺名称时回退 id */
function portLabel(port: DeviceInterface): string {
  if (port.description) {
    return `${port.name} (${port.description})`;
  }
  return port.name || port.id;
}

async function loadPorts(side: DeviceSide): Promise<void> {
  const deviceId = side === "source" ? sourceDeviceId.value : targetDeviceId.value;
  let seq: number;
  if (side === "source") {
    sourceSeq += 1;
    seq = sourceSeq;
    sourcePorts.value = [];
  } else {
    targetSeq += 1;
    seq = targetSeq;
    targetPorts.value = [];
  }
  if (!deviceId) {
    return;
  }
  const result = await apiGet<Paged<DeviceInterface>>(
    `${deviceInterfacesPath(deviceId)}?page_size=200`,
  );
  const current = () => (side === "source" ? sourceSeq : targetSeq);
  if (seq !== current()) {
    return;
  }
  if (!result.success) {
    console.error("加载设备端口失败:", result.message);
    return;
  }
  const items = result.data?.items ?? [];
  if (side === "source") {
    sourcePorts.value = items;
  } else {
    targetPorts.value = items;
  }
}

/** 逻辑连线才需要端口，其它情形保持下拉为空 */
function reloadPortsIfLogical(): void {
  if (type.value !== "logical") {
    return;
  }
  void loadPorts("source");
  void loadPorts("target");
}

watch(type, reloadPortsIfLogical);

watch(sourceDeviceId, () => {
  sourcePortIds.value = [];
  reloadPortsIfLogical();
});

watch(targetDeviceId, () => {
  targetPortIds.value = [];
  reloadPortsIfLogical();
});

/** 同设备对（含方向翻转）是否已存在连线 */
function isDuplicate(sourceId: string, targetId: string): boolean {
  return props.connections.some(
    (conn) =>
      (conn.source_device_id === sourceId && conn.target_device_id === targetId) ||
      (conn.source_device_id === targetId && conn.target_device_id === sourceId),
  );
}

async function submit(): Promise<void> {
  const sourceId = sourceDeviceId.value;
  const targetId = targetDeviceId.value;
  if (!sourceId || !targetId) {
    showToast(t("viz.connection_device_required"), "warning");
    return;
  }
  if (sourceId === targetId) {
    showToast(t("viz.no_self_connection"), "warning");
    return;
  }

  const request: TopologyConnectionRequest = {
    connection_type: type.value,
    source_device_id: sourceId,
    target_device_id: targetId,
    source_device_port_id: null,
    target_device_port_id: null,
    label: label.value.trim() || null,
    source_port_ids: null,
    target_port_ids: null,
  };

  if (type.value === "logical") {
    if (sourcePortIds.value.length === 0 || targetPortIds.value.length === 0) {
      showToast(t("viz.logical_members_required"), "warning");
      return;
    }
    request.source_port_ids = [...sourcePortIds.value];
    request.target_port_ids = [...targetPortIds.value];
  }

  if (isDuplicate(sourceId, targetId)) {
    showToast(t("server.visualization.connection_duplicate"), "warning");
    return;
  }

  submitting.value = true;
  const result = await createTopologyConnection(request);
  submitting.value = false;
  if (!result.success) {
    showToast(`${t("viz.link_create_failed")}: ${result.message}`, "error");
    return;
  }
  showToast(t("viz.link_create_success"), "success");
  emit("created");
}
</script>

<template>
  <ModalShell :title="t('viz.create_connection_title')" @close="emit('close')">
    <form id="topology-connection-form" @submit.prevent="submit">
      <div class="form-group">
        <label for="topo-conn-type">{{ t("viz.connection_type") }}</label>
        <select id="topo-conn-type" v-model="type" class="form-control">
          <option value="physical">{{ t("viz.connection_type_physical") }}</option>
          <option value="logical">{{ t("viz.connection_type_logical") }}</option>
        </select>
      </div>

      <div class="form-group">
        <label for="topo-conn-source-device">{{ t("viz.source_device") }}</label>
        <select id="topo-conn-source-device" v-model="sourceDeviceId" class="form-control">
          <option value=""></option>
          <option v-for="node in nodes" :key="node.device_id" :value="node.device_id">
            {{ node.device_name || node.device_id }}
          </option>
        </select>
      </div>

      <div class="form-group">
        <label for="topo-conn-target-device">{{ t("viz.target_device") }}</label>
        <select id="topo-conn-target-device" v-model="targetDeviceId" class="form-control">
          <option value=""></option>
          <option v-for="node in nodes" :key="node.device_id" :value="node.device_id">
            {{ node.device_name || node.device_id }}
          </option>
        </select>
      </div>

      <div v-if="type === 'logical'" class="form-group topo-conn-ports-group">
        <label for="topo-conn-source-ports">{{ t("viz.source_member_ports") }}</label>
        <select
          id="topo-conn-source-ports"
          v-model="sourcePortIds"
          class="form-control"
          multiple
          size="6"
        >
          <option v-for="port in sourcePorts" :key="port.id" :value="port.id">
            {{ portLabel(port) }}
          </option>
        </select>
      </div>

      <div v-if="type === 'logical'" class="form-group topo-conn-ports-group">
        <label for="topo-conn-target-ports">{{ t("viz.target_member_ports") }}</label>
        <select
          id="topo-conn-target-ports"
          v-model="targetPortIds"
          class="form-control"
          multiple
          size="6"
        >
          <option v-for="port in targetPorts" :key="port.id" :value="port.id">
            {{ portLabel(port) }}
          </option>
        </select>
      </div>

      <div class="form-group">
        <label for="topo-conn-label">{{ t("viz.connection_label") }}</label>
        <input
          id="topo-conn-label"
          v-model="label"
          type="text"
          maxlength="100"
          :placeholder="t('viz.connection_label_placeholder')"
        />
      </div>
    </form>

    <template #footer>
      <button
        type="submit"
        class="btn btn-primary"
        form="topology-connection-form"
        :disabled="submitting"
      >
        {{ t("common.confirm") }}
      </button>
    </template>
  </ModalShell>
</template>
