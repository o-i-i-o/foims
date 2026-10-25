<script setup lang="ts">
/**
 * 机柜新增/编辑弹窗：
 * - 基础字段：房间（仅机房类：机房/弱电井/其他）/ 名称 / 容量 / 电源功率（W|kW）/ 描述；
 * - 机位动态行：名称 + 起始 U/结束 U（1-48，结束不小于起始）+ 描述；
 * - 配线架动态行：仅名称。
 * 提交链：POST/PUT cabinets → PUT cabinets/{id}/positions →
 * PUT cabinets/{id}/patch-panels，保存期间禁用提交按钮防重复提交。
 */
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import type { Cabinet } from "../../types/api/generated/Cabinet";
import type { CabinetCreate } from "../../types/api/generated/CabinetCreate";
import type { CabinetPatchPanelsSync } from "../../types/api/generated/CabinetPatchPanelsSync";
import type { CabinetPositionsSync } from "../../types/api/generated/CabinetPositionsSync";
import type { CabinetWithNetworks } from "../../types/api/generated/CabinetWithNetworks";
import type { Room } from "../../types/api/generated/Room";
import type { Paged } from "../../types/api";
import { apiGet, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import { readPowerWattsInput } from "../../utils/power";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";

const props = withDefaults(
  defineProps<{
    /** null = 新增；对象 = 编辑回显（GET /cabinets/{id} 完整数据） */
    cabinet?: CabinetWithNetworks | null;
  }>(),
  { cabinet: null }
);

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 基础表单 ====================

// totalPower 为输入框原始值（未换算），保留 string | number 联合
const form = reactive<{
  roomId: string;
  name: string;
  capacity: number;
  totalPower: string | number;
  powerUnit: string;
  description: string;
}>({
  roomId: props.cabinet?.room_id ?? "",
  name: props.cabinet?.name ?? "",
  capacity: props.cabinet?.capacity || 42,
  totalPower: props.cabinet?.total_power_watts ?? "",
  powerUnit: "W",
  description: props.cabinet?.description ?? ""
});

// ==================== 机位/配线架动态行 ====================

/** 机位动态行（key 为前端行键，id 为已有机位主键） */
interface PositionRow {
  key: number;
  id: string | null;
  name: string;
  startU: number;
  endU: number;
  description: string;
}

/** 配线架动态行（key 为前端行键，id 为已有配线架主键） */
interface PatchPanelRow {
  key: number;
  id: string | null;
  name: string;
}

let nextRowKey = 1;

const positions = reactive<PositionRow[]>(
  (props.cabinet?.positions ?? []).map((pos) => ({
    key: nextRowKey++,
    id: pos.id,
    name: pos.name || "",
    startU: pos.start_u ?? 1,
    endU: pos.end_u ?? 1,
    description: pos.description || ""
  }))
);

const patchPanels = reactive<PatchPanelRow[]>(
  (props.cabinet?.patch_panels ?? []).map((panel) => ({
    key: nextRowKey++,
    id: panel.id,
    name: panel.name || ""
  }))
);

function addPosition() {
  positions.push({ key: nextRowKey++, id: null, name: "", startU: 1, endU: 1, description: "" });
}

function addPatchPanel() {
  patchPanels.push({ key: nextRowKey++, id: null, name: "" });
}

// ==================== 下拉选项（机房类房间） ====================

// 机柜的网络配置由所属房间自动继承，房间下拉仅列机房类（机房/弱电井/其他）
const roomsQuery = useQuery({
  queryKey: ["options", "datacenter-rooms"],
  queryFn: async (): Promise<Paged<Room> | null> => {
    const params = new URLSearchParams({ page_size: "1000", room_type: "data_center,telecom_closet,other" });
    const result = await apiGet<Paged<Room>>(`/api/resources/rooms?${params}`);
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

const roomOptions = computed<Room[]>(() => {
  const items = roomsQuery.data.value?.items;
  return Array.isArray(items) ? items : [];
});

// ==================== 提交 ====================

const saving = ref(false);

/** 机位校验：名称非空，U 位 1-48 整数且结束不小于起始 */
function validatePositions(): string | null {
  for (const pos of positions) {
    if (!pos.name.trim()) {
      return t("cabinet.position_name_required");
    }
    if (
      !Number.isInteger(pos.startU) ||
      pos.startU < 1 ||
      pos.startU > 48 ||
      !Number.isInteger(pos.endU) ||
      pos.endU < 1 ||
      pos.endU > 48 ||
      pos.endU < pos.startU
    ) {
      return t("cabinet.position_u_invalid");
    }
  }
  return null;
}

const saveMutation = useMutation({
  mutationFn: async (): Promise<string> => {
    if (!form.name.trim()) {
      throw new Error(t("cabinet.name_required"));
    }
    if (!form.roomId) {
      throw new Error(t("cabinet.room_required"));
    }
    if (!Number.isInteger(form.capacity) || form.capacity <= 0) {
      throw new Error(t("cabinet.capacity_invalid"));
    }
    const totalPowerWatts = readPowerWattsInput(form.totalPower, form.powerUnit);
    // readPowerWattsInput 空输入返回 null（= 不限制功率），仅 NaN 为非法值
    if (totalPowerWatts !== null && Number.isNaN(totalPowerWatts)) {
      throw new Error(t("cabinet.total_power_invalid"));
    }
    const positionsError = validatePositions();
    if (positionsError) {
      throw new Error(positionsError);
    }

    const cabinetData: CabinetCreate = {
      name: form.name.trim(),
      room_id: form.roomId,
      capacity: form.capacity,
      total_power_watts: totalPowerWatts,
      description: form.description.trim() || null
    };

    let cabinetId = props.cabinet?.id;
    if (cabinetId) {
      const result = await apiPut<Cabinet, CabinetCreate>(`/api/resources/cabinets/${cabinetId}`, cabinetData);
      if (!result.success) {
        throw new Error(result.message || t("cabinet.save_failed"));
      }
    } else {
      const result = await apiPost<Cabinet, CabinetCreate>("/api/resources/cabinets", cabinetData);
      if (!result.success) {
        throw new Error(result.message || t("cabinet.save_failed"));
      }
      cabinetId = result.data?.id;
      if (!cabinetId) {
        throw new Error(t("cabinet.save_failed"));
      }
    }

    // 同步机位
    const syncResult = await apiPut<unknown, CabinetPositionsSync>(`/api/resources/cabinets/${cabinetId}/positions`, {
      positions: positions.map((pos) => ({
        id: pos.id,
        name: pos.name.trim(),
        start_u: pos.startU,
        end_u: pos.endU,
        description: pos.description.trim() || null
      }))
    });
    if (!syncResult.success) {
      throw new Error(syncResult.message || t("cabinet.positions_save_failed"));
    }

    // 同步配线架
    const ppResult = await apiPut<unknown, CabinetPatchPanelsSync>(`/api/resources/cabinets/${cabinetId}/patch-panels`, {
      patch_panels: patchPanels.map((panel) => ({
        id: panel.id,
        name: panel.name.trim()
      }))
    });
    if (!ppResult.success) {
      throw new Error(ppResult.message || t("cabinet.patch_panels_save_failed"));
    }

    return cabinetId;
  },
  onSuccess: () => {
    queryClient.invalidateQueries({ queryKey: ["cabinets"] });
    showToast(t("cabinet.save_success"), "success");
    emit("saved");
    emit("close");
  },
  onError: (error) => {
    showToast(`${t("common.operation_failed")}: ${error.message}`, "error");
  },
  onSettled: () => {
    saving.value = false;
  }
});

function submit() {
  if (saving.value) {
    return;
  }
  saving.value = true;
  saveMutation.mutate();
}
</script>

<template>
  <ModalShell :title="props.cabinet ? t('cabinet.edit_cabinet') : t('cabinet.add_cabinet')" @close="emit('close')">
    <form @submit.prevent="submit">
      <div class="form-row">
        <div class="form-group">
          <label for="cabinet-room">
            <span>{{ t("cabinet.room") }}</span><span class="required">*</span>
          </label>
          <select id="cabinet-room" v-model="form.roomId" required>
            <option value="">
              {{ roomOptions.length === 0 ? t("room.no_datacenter_data") : t("room.select_datacenter") }}
            </option>
            <option v-for="option in roomOptions" :key="option.id" :value="option.id">
              {{ option.name }}
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="cabinet-name">
            <span>{{ t("cabinet.name") }}</span><span class="required">*</span>
          </label>
          <input id="cabinet-name" v-model="form.name" type="text" autocomplete="off" required>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="cabinet-capacity">
            <span>{{ t("cabinet.capacity") }}</span><span class="required">*</span>
          </label>
          <input id="cabinet-capacity" v-model.number="form.capacity" type="number" min="1" required>
        </div>
        <div class="form-group power-inline">
          <label for="cabinet-total-power">{{ t("cabinet.total_power") }}</label>
          <div class="power-input-row">
            <input
              id="cabinet-total-power"
              v-model="form.totalPower"
              type="number"
              min="0"
              step="any"
              autocomplete="off"
            >
            <select v-model="form.powerUnit" class="form-control" :aria-label="t('common.unit')">
              <option value="W">W</option>
              <option value="kW">kW</option>
            </select>
          </div>
        </div>
      </div>

      <!-- 机位动态区域 -->
      <div class="form-row">
        <div class="form-group">
          <div class="list-header">
            <span class="list-title">{{ t("cabinet.positions") }}</span>
            <button
              type="button"
              class="icon-btn btn-success list-add-btn"
              :data-tooltip="t('cabinet.add_position')"
              :aria-label="t('cabinet.add_position')"
              @click="addPosition"
            >
              <NavIcon name="plus" />
            </button>
          </div>
          <div class="dyn-list">
            <div class="cabinet-columns-header" aria-hidden="true">
              <span>{{ t("cabinet_position.name") }}</span>
              <span>{{ t("cabinet_position.start_u") }}</span>
              <span>{{ t("cabinet_position.end_u") }}</span>
              <span>{{ t("cabinet_position.description") }}</span>
              <span class="cabinet-item-actions" />
            </div>
            <div v-if="positions.length === 0" class="text-center hint-text">
              {{ t("cabinet.no_positions_hint") }}
            </div>
            <template v-else>
              <div v-for="(pos, index) in positions" :key="pos.key" class="cabinet-position-item dyn-list-item">
                <div class="form-row">
                  <div class="form-group">
                    <input
                      v-model="pos.name"
                      type="text"
                      :placeholder="t('cabinet_position.name')"
                      autocomplete="off"
                    >
                  </div>
                  <div class="form-group">
                    <input v-model.number="pos.startU" type="number" min="1" max="48" :placeholder="t('cabinet_position.start_u')">
                  </div>
                  <div class="form-group">
                    <input v-model.number="pos.endU" type="number" min="1" max="48" :placeholder="t('cabinet_position.end_u')">
                  </div>
                  <div class="form-group">
                    <input
                      v-model="pos.description"
                      type="text"
                      :placeholder="t('cabinet_position.description')"
                      autocomplete="off"
                    >
                  </div>
                  <div class="form-group cabinet-item-actions">
                    <button
                      type="button"
                      class="icon-btn btn-danger"
                      :data-tooltip="t('common.delete')"
                      :aria-label="t('common.delete')"
                      @click="positions.splice(index, 1)"
                    >
                      <NavIcon name="trash" />
                    </button>
                  </div>
                </div>
              </div>
            </template>
          </div>
        </div>
      </div>

      <!-- 配线架动态区域 -->
      <div class="form-row">
        <div class="form-group">
          <div class="list-header">
            <span class="list-title">{{ t("cabinet.patch_panels") }}</span>
            <button
              type="button"
              class="icon-btn btn-success list-add-btn"
              :data-tooltip="t('cabinet.add_patch_panel')"
              :aria-label="t('cabinet.add_patch_panel')"
              @click="addPatchPanel"
            >
              <NavIcon name="plus" />
            </button>
          </div>
          <div class="dyn-list">
            <div class="dyn-columns-header" aria-hidden="true">
              <span>{{ t("cabinet.patch_panel_name") }}</span>
              <span class="dyn-columns-actions" />
            </div>
            <div v-if="patchPanels.length === 0" class="text-center hint-text">
              {{ t("cabinet.no_patch_panels_hint") }}
            </div>
            <template v-else>
              <div v-for="(panel, index) in patchPanels" :key="panel.key" class="cabinet-patch-panel-item dyn-list-item">
                <div class="form-row">
                  <div class="form-group">
                    <input
                      v-model="panel.name"
                      type="text"
                      :placeholder="t('cabinet.patch_panel_name')"
                      autocomplete="off"
                    >
                  </div>
                  <div class="form-group cabinet-item-actions">
                    <button
                      type="button"
                      class="icon-btn btn-danger"
                      :data-tooltip="t('common.delete')"
                      :aria-label="t('common.delete')"
                      @click="patchPanels.splice(index, 1)"
                    >
                      <NavIcon name="trash" />
                    </button>
                  </div>
                </div>
              </div>
            </template>
          </div>
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
