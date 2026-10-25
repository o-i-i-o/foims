<script setup lang="ts">
/**
 * 工位新增/编辑弹窗（移植自旧版 modals/workstation/workstation-modal.html
 * + modules/workstation.js openWorkstationModal/submitWorkstationForm）：
 * - 房间（仅办公类：办公/大堂/接待/其他）级联管理人（按房间所属组织取员工）；
 * - 名称/管理人必填；X/Y 坐标不随表单提交，而是经 `saved` 事件携带给
 *   可视化页应用到画布布局（element_layouts）。
 */
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";
import type { Employee } from "../../types/api/generated/Employee";
import type { Paged } from "../../types/api";
import type { Room } from "../../types/api/generated/Room";
import type { Workstation } from "../../types/api/generated/Workstation";
import type { WorkstationCreate } from "../../types/api/generated/WorkstationCreate";
import type { WorkstationUpdate } from "../../types/api/generated/WorkstationUpdate";

const props = withDefaults(
  defineProps<{
    /** null = 新增；对象 = 编辑回显（GET /workstations/{id} 完整数据） */
    workstation?: Workstation | null;
    /** 画布当前坐标（可视化回调传入，用于回填坐标输入框；房间管理入口无画布上下文为 null） */
    position?: { x: number; y: number } | null;
  }>(),
  { workstation: null, position: null }
);

const emit = defineEmits<{
  close: [];
  /** 保存成功：携带元素 id 与坐标（null 表示未填写），由可视化层应用到画布 */
  saved: [payload: WorkstationSavedPayload];
}>();

/** 保存事件载荷（坐标已四舍五入并钳制非负） */
export interface WorkstationSavedPayload {
  id: string;
  roomId: string;
  x: number | null;
  y: number | null;
}

const { t } = useI18n();

// ==================== 表单 ====================

/** 工位表单状态（x/y 为坐标输入框字符串，提交前再解析） */
interface WorkstationFormState {
  roomId: string;
  name: string;
  managerId: string;
  x: string;
  y: string;
  description: string;
}

const form = reactive<WorkstationFormState>({
  roomId: props.workstation?.room_id ?? "",
  name: props.workstation?.name ?? "",
  managerId: props.workstation?.manager_employee_id ?? "",
  x: props.position ? String(Math.round(props.position.x)) : "",
  y: props.position ? String(Math.round(props.position.y)) : "",
  description: props.workstation?.description ?? ""
});

// ==================== 房间下拉（仅办公类，含 org_id 供管理人级联） ====================

const roomsQuery = useQuery({
  queryKey: ["options", "office-rooms"],
  queryFn: async (): Promise<Paged<Room> | null> => {
    const params = new URLSearchParams({ page_size: "1000", room_type: "office,lobby,reception,other" });
    const result = await apiGet<Paged<Room>>(`/api/resources/rooms?${params}`);
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

const roomOptions = computed(() => roomsQuery.data.value?.items ?? []);

// 房间 → 所属组织映射：管理人下拉的唯一数据来源
const roomOrgMap = computed(() => new Map(roomOptions.value.map((room) => [room.id, room.org_id || ""])));

// ==================== 管理人下拉（按房间所属组织取员工） ====================

// 旧版以序号守卫丢弃快速切换房间时的过期员工响应；vue-query 按 orgId 分键
// 缓存天然隔离并发结果，等价替代
const managerOrgId = computed(() => roomOrgMap.value.get(form.roomId) || "");

const employeesQuery = useQuery({
  queryKey: ["options", "org-employees", managerOrgId],
  queryFn: async (): Promise<Employee[]> => {
    const result = await apiGet<Employee[]>(
      `/api/resources/employees?org_id=${encodeURIComponent(managerOrgId.value)}`
    );
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return Array.isArray(result.data) ? result.data : [];
  },
  enabled: computed(() => Boolean(managerOrgId.value))
});

const employeeOptions = computed(() => (Array.isArray(employeesQuery.data.value) ? employeesQuery.data.value : []));

// 房间切换 → 管理人跟随新房间的所属组织刷新，旧选中作废（与旧版一致）
function onRoomChange(): void {
  form.managerId = "";
}

// ==================== 提交 ====================

const saving = ref(false);

function submit(): void {
  if (saving.value) {
    return;
  }

  if (!form.name.trim()) {
    showToast(t("workstation.name_required"), "warning");
    return;
  }
  if (!form.roomId) {
    showToast(t("workstation.room_required"), "warning");
    return;
  }
  // 管理人必填：从组织人员中选择
  if (!form.managerId) {
    showToast(t("workstation.manager_placeholder"), "warning");
    return;
  }

  saving.value = true;

  void submitSave();
}

async function submitSave(): Promise<void> {
  const workstationData: WorkstationCreate = {
    name: form.name.trim(),
    room_id: form.roomId,
    manager_employee_id: form.managerId,
    description: form.description.trim() || null
  };

  try {
    const parsedId = props.workstation?.id ?? null;
    const result = parsedId
      ? await apiPut<Workstation, WorkstationUpdate>(
          `/api/resources/workstations/${parsedId}`,
          workstationData
        )
      : await apiPost<Workstation, WorkstationCreate>("/api/resources/workstations", workstationData);

    if (!result.success) {
      const errorMsg = result.message || t("common.check_input");
      showToast(`${t("common.operation_failed")}: ${errorMsg}`, "error");
      return;
    }

    // 坐标仅影响画布布局（element_layouts），随保存事件交给可视化层应用
    const x = Number(form.x);
    const y = Number(form.y);
    const hasPosition = form.x !== "" && form.y !== "" && !Number.isNaN(x) && !Number.isNaN(y);
    showToast(t("workstation.save_success"), "success");
    emit("saved", {
      id: result.data?.id || parsedId || "",
      roomId: form.roomId,
      x: hasPosition ? Math.max(0, Math.round(x)) : null,
      y: hasPosition ? Math.max(0, Math.round(y)) : null,
    });
    emit("close");
  } catch (error) {
    console.error("提交工位表单失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <ModalShell :title="props.workstation ? t('workstation.edit') : t('workstation.add')" @close="emit('close')">
    <form @submit.prevent="submit">
      <div class="form-group">
        <label for="workstation-room">
          <span>{{ t("workstation.room") }}</span><span class="required">*</span>
        </label>
        <select id="workstation-room" v-model="form.roomId" required @change="onRoomChange">
          <option value="">
            {{ roomOptions.length === 0 ? t("room.no_office_data") : t("room.select_office") }}
          </option>
          <option v-for="room in roomOptions" :key="room.id" :value="room.id">
            {{ room.name }}
          </option>
        </select>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="workstation-name">
            <span>{{ t("workstation.name") }}</span><span class="required">*</span>
          </label>
          <input id="workstation-name" v-model="form.name" type="text" autocomplete="off" required>
        </div>
        <div class="form-group">
          <label for="workstation-manager">{{ t("workstation.manager") }}</label>
          <select id="workstation-manager" v-model="form.managerId" autocomplete="off">
            <option value="" disabled>{{ t("workstation.manager_placeholder") }}</option>
            <option v-for="emp in employeeOptions" :key="emp.id" :value="emp.id">
              {{ emp.name }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="workstation-x">{{ t("workstation.position_x") }}</label>
          <input
            id="workstation-x"
            v-model="form.x"
            type="number"
            min="0"
            step="1"
            :placeholder="t('workstation.position_placeholder')"
          >
        </div>
        <div class="form-group">
          <label for="workstation-y">{{ t("workstation.position_y") }}</label>
          <input
            id="workstation-y"
            v-model="form.y"
            type="number"
            min="0"
            step="1"
            :placeholder="t('workstation.position_placeholder')"
          >
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
