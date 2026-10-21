<script setup lang="ts">
/**
 * 机位新增/编辑弹窗（移植自旧版 modals/cabinet/cabinet-position-modal.html
 * + modules/position.js openCabinetPositionModal/submitCabinetPositionForm）：
 * - 房间/机柜级联：房间选项从机柜数据源去重派生，机柜按房间过滤；
 * - 名称/机柜必填，U 位为有效正整数且起始不大于结束；
 * - 编辑模式通过 cabinet_id 反查所属房间后回显级联。
 */
import { computed, onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import type { CabinetPosition } from "../../types/api/generated/CabinetPosition";
import type { CabinetPositionCreate } from "../../types/api/generated/CabinetPositionCreate";
import type { CabinetWithNetworks } from "../../types/api/generated/CabinetWithNetworks";
import type { Paged } from "../../types/api";
import { apiGet, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";

const props = withDefaults(
  defineProps<{
    /** null = 新增；对象 = 编辑回显（GET /positions/{id} 完整数据） */
    position?: CabinetPosition | null;
  }>(),
  { position: null }
);

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();

// ==================== 表单 ====================

const form = reactive<{
  name: string;
  roomId: string;
  cabinetId: string;
  startU: number;
  endU: number;
  description: string;
}>({
  name: props.position?.name ?? "",
  roomId: "",
  cabinetId: props.position?.cabinet_id ?? "",
  startU: props.position?.start_u ?? 1,
  endU: props.position?.end_u ?? 1,
  description: props.position?.description ?? ""
});

// ==================== 房间/机柜级联 ====================

// 房间选项从机柜数据源只读去重派生（page_size=1000 后端上限，
// 默认 20/页会把第 21 个及之后的机柜静默截断）
const cabinetsSourceQuery = useQuery({
  queryKey: ["options", "cabinet-room-source"],
  queryFn: async (): Promise<CabinetWithNetworks[]> => {
    const result = await apiGet<Paged<CabinetWithNetworks>>("/api/resources/cabinets?page_size=1000");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    const items = result.data?.items;
    return Array.isArray(items) ? items : [];
  }
});

const roomOptions = computed<Array<{ id: string; name: string }>>(() => {
  const roomMap = new Map<string, string>();
  for (const cabinet of cabinetsSourceQuery.data.value ?? []) {
    if (cabinet.room_id && cabinet.room_name && !roomMap.has(cabinet.room_id)) {
      roomMap.set(cabinet.room_id, cabinet.room_name);
    }
  }
  return [...roomMap.entries()].map(([roomId, roomName]) => ({ id: roomId, name: roomName }));
});

const roomCabinetsQuery = useQuery({
  queryKey: ["options", "room-cabinets", () => form.roomId],
  queryFn: async (): Promise<CabinetWithNetworks[]> => {
    const result = await apiGet<Paged<CabinetWithNetworks>>(`/api/resources/cabinets?room_id=${encodeURIComponent(form.roomId)}&page_size=1000`);
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    const items = result.data?.items;
    return Array.isArray(items) ? items : [];
  },
  enabled: computed(() => Boolean(form.roomId))
});

const cabinetOptions = computed<CabinetWithNetworks[]>(() => {
  const data = roomCabinetsQuery.data.value;
  return Array.isArray(data) ? data : [];
});

// 房间切换 → 机柜选项刷新，旧选中作废（与旧版 innerHTML 重建一致）
function onRoomChange() {
  form.cabinetId = "";
}

// ==================== 提交 ====================

const saving = ref(false);

function submit() {
  if (saving.value) {
    return;
  }

  if (!form.name.trim()) {
    showToast(t("cabinet_position.name_required"), "warning");
    return;
  }
  if (!form.cabinetId) {
    showToast(t("cabinet_position.cabinet_required"), "warning");
    return;
  }
  if (Number.isNaN(form.startU) || form.startU <= 0) {
    showToast(t("cabinet_position.start_u_invalid"), "warning");
    return;
  }
  if (Number.isNaN(form.endU) || form.endU <= 0) {
    showToast(t("cabinet_position.end_u_invalid"), "warning");
    return;
  }
  if (form.startU > form.endU) {
    showToast(t("cabinet_position.start_gt_end"), "warning");
    return;
  }

  saving.value = true;

  void submitSave();
}

async function submitSave() {
  const positionData: CabinetPositionCreate = {
    name: form.name.trim(),
    cabinet_id: form.cabinetId,
    start_u: form.startU,
    end_u: form.endU,
    description: form.description.trim() || null
  };

  try {
    const parsedId = props.position?.id ?? null;
    const result = parsedId
      ? await apiPut<CabinetPosition, CabinetPositionCreate>(`/api/resources/positions/${parsedId}`, positionData)
      : await apiPost<CabinetPosition, CabinetPositionCreate>("/api/resources/positions", positionData);

    if (!result.success) {
      const errorMsg = result.message || t("common.check_input");
      showToast(`${t("common.operation_failed")}: ${errorMsg}`, "error");
      return;
    }

    showToast(t("cabinet_position.save_success"), "success");
    emit("saved");
    emit("close");
  } catch (error) {
    console.error("提交机位表单失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  } finally {
    saving.value = false;
  }
}

// 编辑模式：通过 cabinet_id 从机柜数据源只读反查所属房间，
// 回填房间后机柜下拉随 queryKey 变化自动加载并回显选中
onMounted(() => {
  const cabinetId = props.position?.cabinet_id;
  if (!cabinetId) {
    return;
  }
  void (async () => {
    try {
      const result = await apiGet<CabinetWithNetworks>(`/api/resources/cabinets/${cabinetId}`);
      if (result.success && result.data?.room_id) {
        form.roomId = result.data.room_id;
      }
    } catch (error) {
      console.error("从机柜数据获取房间信息失败:", error);
    }
  })();
});
</script>

<template>
  <ModalShell :title="props.position ? t('cabinet_position.edit') : t('cabinet_position.add')" @close="emit('close')">
    <form @submit.prevent="submit">
      <div class="form-row">
        <div class="form-group">
          <label for="cabinet-position-name">
            <span>{{ t("cabinet_position.name") }}</span><span class="required">*</span>
          </label>
          <input id="cabinet-position-name" v-model="form.name" type="text" autocomplete="off" required>
        </div>
        <div class="form-group">
          <label for="cabinet-position-room">
            <span>{{ t("cabinet_position.room") }}</span><span class="required">*</span>
          </label>
          <select id="cabinet-position-room" v-model="form.roomId" required @change="onRoomChange">
            <option value="">{{ t("cabinet_position.select_room") }}</option>
            <option v-for="room in roomOptions" :key="room.id" :value="room.id">
              {{ room.name }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="cabinet-position-cabinet">
            <span>{{ t("cabinet_position.cabinet") }}</span><span class="required">*</span>
          </label>
          <select id="cabinet-position-cabinet" v-model="form.cabinetId" required>
            <option value="">
              {{ form.roomId ? t("cabinet_position.select_cabinet") : t("cabinet_position.select_room_first") }}
            </option>
            <option v-for="cabinet in cabinetOptions" :key="cabinet.id" :value="cabinet.id">
              {{ cabinet.name }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="cabinet-position-start-u">
            <span>{{ t("cabinet_position.start_u") }}</span><span class="required">*</span>
          </label>
          <input id="cabinet-position-start-u" v-model.number="form.startU" type="number" min="1" max="45" required>
        </div>
        <div class="form-group">
          <label for="cabinet-position-end-u">
            <span>{{ t("cabinet_position.end_u") }}</span><span class="required">*</span>
          </label>
          <input id="cabinet-position-end-u" v-model.number="form.endU" type="number" min="1" max="45" required>
        </div>
      </div>
      <div class="form-group form-group-block">
        <textarea
          v-model="form.description"
          :aria-label="t('common.description')"
          :placeholder="t('common.description_placeholder')"
        />
      </div>
    </form>

    <template #footer>
      <button type="button" class="btn-primary" :disabled="saving" @click="submit">
        {{ saving ? t("common.saving") : t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
