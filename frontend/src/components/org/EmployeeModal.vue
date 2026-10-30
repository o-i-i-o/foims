<script setup lang="ts">
/**
 * 人员管理弹窗：
 * 列出组织节点下的员工，支持新增/编辑/删除（工位管理人的员工引用由后端
 * 自动解绑）。员工列表接口返回普通数组（非分页）。
 */
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiGet, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import NavIcon from "../NavIcon.vue";
import ConfirmModal from "../ConfirmModal.vue";
import ModalShell from "../ModalShell.vue";
import type { Employee } from "../../types/api/generated/Employee";
import type { EmployeeCreate } from "../../types/api/generated/EmployeeCreate";
import type { EmployeeUpdate } from "../../types/api/generated/EmployeeUpdate";

const props = withDefaults(
  defineProps<{
    open?: boolean;
    orgId?: string;
    orgName?: string;
  }>(),
  { open: false, orgId: "", orgName: "" }
);

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

const employeesQuery = useQuery({
  queryKey: computed(() => ["employees", props.orgId]),
  queryFn: async (): Promise<Employee[]> => {
    const result = await apiGet<Employee[]>(
      `/api/resources/employees?org_id=${encodeURIComponent(props.orgId)}`
    );
    if (!result.success) {
      throw new Error(result.message || t("employee.load_failed"));
    }
    // 员工列表接口返回普通数组（非分页），非数组响应按空列表兜底
    return Array.isArray(result.data) ? result.data : [];
  },
  enabled: computed(() => props.open && Boolean(props.orgId))
});

const employees = computed(() =>
  employeesQuery.isError.value ? [] : (employeesQuery.data.value ?? [])
);

/** 性别取值显示文案 */
function genderText(gender: string): string {
  if (gender === "male") {
    return t("employee.gender_male");
  }
  if (gender === "female") {
    return t("employee.gender_female");
  }
  return t("employee.gender_unknown");
}

/** 编辑表单状态（gender 保持 string：select v-model 会回写 string 值） */
interface EmployeeFormState {
  id: string;
  name: string;
  gender: string;
  phone: string;
  email: string;
  hireDate: string;
}

const editOpen = ref(false);
const editForm = reactive<EmployeeFormState>({
  id: "",
  name: "",
  gender: "unknown",
  phone: "",
  email: "",
  hireDate: ""
});

function openCreate(): void {
  Object.assign(editForm, { id: "", name: "", gender: "unknown", phone: "", email: "", hireDate: "" });
  editOpen.value = true;
}

function openEdit(employee: Employee): void {
  Object.assign(editForm, {
    id: employee.id,
    name: employee.name || "",
    gender: employee.gender || "unknown",
    phone: employee.phone || "",
    email: employee.email || "",
    hireDate: employee.hire_date || ""
  });
  editOpen.value = true;
}

const saveMutation = useMutation({
  mutationFn: () => {
    const body: EmployeeUpdate = {
      name: editForm.name.trim(),
      gender: editForm.gender || "unknown",
      phone: editForm.phone.trim(),
      email: editForm.email.trim(),
      hire_date: editForm.hireDate || null
    };
    if (editForm.id) {
      return apiPut<unknown, EmployeeUpdate>(`/api/resources/employees/${editForm.id}`, body);
    }
    // 新增载荷键集与编辑不同（EmployeeCreate.name 必填 string），独立构造
    const createBody: EmployeeCreate = {
      name: editForm.name.trim(),
      gender: editForm.gender || "unknown",
      phone: editForm.phone.trim(),
      email: editForm.email.trim(),
      hire_date: editForm.hireDate || null,
      org_id: props.orgId
    };
    return apiPost<unknown, EmployeeCreate>("/api/resources/employees", createBody);
  },
  onSuccess: (result) => {
    if (result.success) {
      showToast(result.message, "success");
      editOpen.value = false;
      queryClient.invalidateQueries({ queryKey: ["employees", props.orgId] });
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  }
});

function handleSubmit(): void {
  if (!editForm.name.trim()) {
    showToast(t("employee.name_required"), "warning");
    return;
  }
  saveMutation.mutate();
}

const deletingEmployee = ref<Employee | null>(null);

const deleteMutation = useMutation({
  mutationFn: (employee: Employee) => apiDelete(`/api/resources/employees/${employee.id}`),
  onSuccess: (result) => {
    if (result.success) {
      showToast(result.message, "success");
      queryClient.invalidateQueries({ queryKey: ["employees", props.orgId] });
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  }
});

function handleDeleteConfirmed(): void {
  const employee = deletingEmployee.value;
  deletingEmployee.value = null;
  if (employee) {
    deleteMutation.mutate(employee);
  }
}

const saving = computed(() => saveMutation.isPending.value);

const editTitle = computed(() => (editForm.id ? t("employee.edit_title") : t("employee.add_title")));
</script>

<template>
  <ModalShell v-if="open" size="lg" @close="emit('close')">
    <template #title>
      <span>{{ t("employee.manager_title") }}</span>
      <span v-if="orgName" class="employee-modal-org"> - {{ orgName }}</span>
    </template>
    <div class="modal-toolbar">
      <button type="button" class="icon-btn btn-success" :title="t('employee.add')" :aria-label="t('employee.add')" @click="openCreate">
        <NavIcon name="plus" />
      </button>
    </div>

    <div class="table-container employee-table-container">
      <table class="data-table" :aria-label="t('employee.manager_title')">
        <thead>
          <tr>
            <th>{{ t("employee.col_name") }}</th>
            <th>{{ t("employee.col_gender") }}</th>
            <th>{{ t("employee.col_phone") }}</th>
            <th>{{ t("employee.col_email") }}</th>
            <th>{{ t("employee.col_hire_date") }}</th>
            <th class="col-center">{{ t("common.actions") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="employeesQuery.isLoading.value || employeesQuery.isError.value">
            <td colspan="6" class="text-center">
              {{ employeesQuery.isError.value ? employeesQuery.error.value?.message : t("common.no_data") }}
            </td>
          </tr>
          <tr v-else-if="employees.length === 0">
            <td colspan="6" class="text-center">{{ t("common.no_data") }}</td>
          </tr>
          <template v-else>
            <tr v-for="employee in employees" :key="employee.id">
              <td>{{ employee.name || "-" }}</td>
              <td>{{ genderText(employee.gender) }}</td>
              <td>{{ employee.phone || "-" }}</td>
              <td>{{ employee.email || "-" }}</td>
              <td>{{ employee.hire_date || "-" }}</td>
              <td class="col-center">
                <span class="employee-actions-cell">
                  <button type="button" class="icon-btn btn-edit" :title="t('common.edit')" :aria-label="t('common.edit')" @click="openEdit(employee)">
                    <NavIcon name="edit" />
                  </button>
                  <button type="button" class="icon-btn btn-delete" :title="t('common.delete')" :aria-label="t('common.delete')" @click="deletingEmployee = employee">
                    <NavIcon name="trash" />
                  </button>
                </span>
              </td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>

    <!-- 员工新增/编辑子弹窗 -->
    <ModalShell v-if="editOpen" :title="editTitle" size="sm" @close="editOpen = false">
      <form @submit.prevent="handleSubmit">
        <div class="form-group">
          <label for="employee-edit-name">
            <span>{{ t("employee.name") }}</span><span class="required">*</span>
          </label>
          <input id="employee-edit-name" v-model="editForm.name" type="text" maxlength="50" required autocomplete="off">
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="employee-edit-gender">{{ t("employee.gender") }}</label>
            <select id="employee-edit-gender" v-model="editForm.gender">
              <option value="unknown">{{ t("employee.gender_unknown") }}</option>
              <option value="male">{{ t("employee.gender_male") }}</option>
              <option value="female">{{ t("employee.gender_female") }}</option>
            </select>
          </div>
          <div class="form-group">
            <label for="employee-edit-phone">{{ t("employee.phone") }}</label>
            <input id="employee-edit-phone" v-model="editForm.phone" type="tel" maxlength="20" autocomplete="off">
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="employee-edit-email">{{ t("employee.email") }}</label>
            <input id="employee-edit-email" v-model="editForm.email" type="email" maxlength="100" autocomplete="off">
          </div>
          <div class="form-group">
            <label for="employee-edit-hire-date">{{ t("employee.hire_date") }}</label>
            <input id="employee-edit-hire-date" v-model="editForm.hireDate" type="date">
          </div>
        </div>
      </form>
      <template #footer>
        <button type="button" class="btn-primary" :disabled="saving" @click="handleSubmit">
          {{ saving ? t("common.saving") : t("common.save") }}
        </button>
      </template>
    </ModalShell>

    <!-- 删除确认 -->
    <ConfirmModal
      v-if="deletingEmployee"
      :message="t('employee.delete_confirm', { name: deletingEmployee.name })"
      danger
      @confirm="handleDeleteConfirmed"
      @cancel="deletingEmployee = null"
    />
  </ModalShell>
</template>
