<script setup lang="ts">
/**
 * 容器坐标弹窗（移植自 modals/visualization/topology-container-modal.html +
 * TopologyVisualization.openContainerCoordinateModal）。
 *
 * 修改房间/机柜容器左上角原点：组内设备随容器整体平移后自动保存。
 * 输入校验与设备详情一致：空串/非有限值/负值全部拒绝
 * （`Number("") === 0` 会被范围校验放行，必须显式拦截空串）。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import ModalShell from "../ModalShell.vue";
import { showToast } from "../../composables/useToast";

/** 容器层级：房间外框 / 机柜内框 */
type ContainerKind = "room" | "cabinet";

const props = defineProps<{
  kind: ContainerKind;
  /** 容器标题（房间名或机柜名） */
  label: string;
  /** 容器当前左上角坐标 */
  position: { x: number; y: number };
}>();

const emit = defineEmits<{ close: []; save: [x: number, y: number] }>();

const { t } = useI18n();

const coordX = ref(String(Math.round(props.position.x)));
const coordY = ref(String(Math.round(props.position.y)));

function submit(): void {
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
  emit("save", Math.round(x), Math.round(y));
}
</script>

<template>
  <ModalShell :title="t('viz.container_coords')" @close="emit('close')">
    <p class="hint-text">
      {{ t(kind === "room" ? "viz.container_room" : "viz.container_cabinet") }}: {{ label }}
    </p>
    <div class="form-row">
      <div class="form-group">
        <label for="topology-container-x">{{ t("workstation.position_x") }}</label>
        <input
          id="topology-container-x"
          v-model="coordX"
          type="number"
          class="form-control"
          min="0"
          step="1"
        />
      </div>
      <div class="form-group">
        <label for="topology-container-y">{{ t("workstation.position_y") }}</label>
        <input
          id="topology-container-y"
          v-model="coordY"
          type="number"
          class="form-control"
          min="0"
          step="1"
        />
      </div>
    </div>

    <template #footer>
      <button type="button" class="btn btn-secondary" @click="emit('close')">
        {{ t("common.cancel") }}
      </button>
      <button type="button" class="btn btn-primary" @click="submit">
        {{ t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
