<script setup lang="ts">
/**
 * 连线详情弹窗（移植自 modals/visualization/topology-connection-detail-modal.html +
 * TopologyVisualization._openConnectionDetail 的渲染段）。
 *
 * 展示三档连线：手动物理示意 / 逻辑链路聚合 / 线路派生物理连线。
 * 逻辑连线列出两侧成员端口，派生连线列出途经线路并给出只读提示；
 * 仅存储连线（手动）可删除，派生物理连线由线路管理维护。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import ConfirmModal from "../ConfirmModal.vue";
import ModalShell from "../ModalShell.vue";
import { deleteTopologyConnection } from "../../api/visualization";
import { showToast } from "../../composables/useToast";
import type { TopologyCableSegment } from "../../types/api/generated/TopologyCableSegment";
import type { TopologyConnectionView } from "../../types/api/generated/TopologyConnectionView";
import type { TopologyMemberPort } from "../../types/api/generated/TopologyMemberPort";

const props = defineProps<{ connection: TopologyConnectionView }>();

const emit = defineEmits<{ close: []; deleted: [] }>();

const { t } = useI18n();

const confirmingDelete = ref(false);
const deleting = ref(false);

/** 连线类型文案：派生 > 逻辑 > 物理 */
const typeLabel = computed(() => {
  if (props.connection.derived) {
    return t("viz.connection_type_derived");
  }
  if (props.connection.connection_type === "logical") {
    return t("viz.connection_type_logical");
  }
  return t("viz.connection_type_physical");
});

/** 逻辑连线成员端口：端口号为 null 时退回 id 前 8 位 */
const hasMembers = computed(
  () =>
    props.connection.connection_type === "logical" &&
    (props.connection.source_members.length > 0 || props.connection.target_members.length > 0),
);

const sourceMembers = computed(() => formatMembers(props.connection.source_members));
const targetMembers = computed(() => formatMembers(props.connection.target_members));

/** 途经线路：段数为 hops + 1，派生物理连线才有 */
const cablePath = computed(() => formatCables(props.connection.cables));

function formatMembers(members: TopologyMemberPort[]): string {
  return members
    .map((member) => member.port_number || member.port_id.slice(0, 8) || t("viz.no_port"))
    .join(", ");
}

function formatCables(cables: TopologyCableSegment[]): string {
  return cables.map((cable) => cable.cable_label || cable.cable_id.slice(0, 8) || t("common.unknown")).join("  →  ");
}

async function confirmDelete(): Promise<void> {
  confirmingDelete.value = false;
  deleting.value = true;
  const result = await deleteTopologyConnection(props.connection.id);
  deleting.value = false;
  if (!result.success) {
    showToast(`${t("viz.link_delete_failed")}: ${result.message}`, "error");
    return;
  }
  showToast(t("viz.link_delete_success"), "success");
  emit("deleted");
}
</script>

<template>
  <ModalShell :title="t('viz.connection_detail')" @close="emit('close')">
    <div class="connection-detail-type">
      <span class="badge">{{ typeLabel }}</span>
      <span v-if="connection.label" class="conn-detail-label">{{ connection.label }}</span>
    </div>

    <div class="connection-detail-endpoints">
      <div class="connection-detail-endpoint">
        <div class="endpoint-title">{{ t("viz.connection_source") }}</div>
        <div class="endpoint-device">
          {{ connection.source_device_name || connection.source_device_id }}
        </div>
        <div class="endpoint-port">
          <span>{{ t("viz.connection_port") }}</span>:
          <span>{{ connection.source_port_label || t("viz.no_port") }}</span>
        </div>
      </div>
      <div class="connection-detail-arrow">&rarr;</div>
      <div class="connection-detail-endpoint">
        <div class="endpoint-title">{{ t("viz.connection_target") }}</div>
        <div class="endpoint-device">
          {{ connection.target_device_name || connection.target_device_id }}
        </div>
        <div class="endpoint-port">
          <span>{{ t("viz.connection_port") }}</span>:
          <span>{{ connection.target_port_label || t("viz.no_port") }}</span>
        </div>
      </div>
    </div>

    <div v-if="hasMembers" class="connection-detail-section">
      <div class="section-title">{{ t("viz.connection_members") }}</div>
      <div class="section-body">{{ sourceMembers }}</div>
      <div class="section-body">{{ targetMembers }}</div>
    </div>

    <div v-if="connection.cables.length > 0" class="connection-detail-section">
      <div class="section-title">{{ t("viz.connection_path") }}</div>
      <div class="section-body">{{ cablePath }}</div>
    </div>

    <p v-if="connection.derived" class="hint-text">{{ t("viz.physical_derived_hint") }}</p>

    <template #footer>
      <button
        v-if="!connection.derived"
        type="button"
        class="btn-danger btn-sm"
        :disabled="deleting"
        @click="confirmingDelete = true"
      >
        {{ t("viz.delete_connection") }}
      </button>
      <button type="button" class="btn-secondary btn-sm" @click="emit('close')">
        {{ t("common.close") }}
      </button>
    </template>

    <ConfirmModal
      v-if="confirmingDelete"
      :message="t('viz.connection_delete_confirm')"
      danger
      @confirm="confirmDelete"
      @cancel="confirmingDelete = false"
    />
  </ModalShell>
</template>
