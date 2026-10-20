<script setup>
/**
 * 定时任务页签（移植自旧版 scheduledTaskManager.js）：无分页列表
 * （8 列 + 表头排序）+ 立即执行/启停/编辑/日志/删除五行操作。
 * 编辑走详情接口回填（列表行数据不含 config 明细场景由弹窗兜底）。
 */
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiGet, apiPost } from "../../api/client.js";
import { formatDateTime } from "../../utils/formatter.js";
import { isSectionReadonly } from "../../utils/roleCapabilities.js";
import { showToast } from "../../composables/useToast.js";
import { onThSort, sortClass } from "../../composables/usePagedList.js";
import ConfirmModal from "../ConfirmModal.vue";
import NavIcon from "../NavIcon.vue";
import ScheduledTaskModal from "./ScheduledTaskModal.vue";
import TaskLogsModal from "./TaskLogsModal.vue";

const props = defineProps({
  active: { type: Boolean, default: false }
});

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

// ==================== 列表（无分页，仅排序） ====================

const sort = reactive({ by: "created_at", order: "desc" });
// 兼容 sortClass/onThSort 的 list 形态：无分页列表复用排序工具类
const sortList = reactive({
  sort,
  toggleSort(key) {
    if (sort.by === key) {
      sort.order = sort.order === "asc" ? "desc" : "asc";
    } else {
      sort.by = key;
      sort.order = "asc";
    }
  }
});

const tasksQuery = useQuery({
  queryKey: computed(() => ["scheduled-tasks", { sortBy: sort.by, sortOrder: sort.order }]),
  queryFn: async () => {
    const result = await apiGet(`/api/system/scheduled-tasks?sort_by=${sort.by}&sort_order=${sort.order}`);
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const tasks = computed(() => (Array.isArray(tasksQuery.data.value) ? tasksQuery.data.value : []));

// task_type 为动态翻译键：键缺失时 t() 原样返回键名，此时回退原始枚举值
function typeLabel(taskType) {
  const typeKey = `scheduled_tasks.task_types.${taskType}`;
  return t(typeKey) === typeKey ? taskType || "" : t(typeKey);
}

function refetch() {
  queryClient.invalidateQueries({ queryKey: ["scheduled-tasks"] });
}

// ==================== 立即执行 / 启停 / 删除 ====================

const confirmAction = ref(null); // { type: "run" | "delete", task }
const confirmMessage = computed(() => {
  if (!confirmAction.value) {
    return "";
  }
  return confirmAction.value.type === "run"
    ? t("scheduled_tasks.confirm_run")
    : t("scheduled_tasks.confirm_delete");
});

const runningId = ref(null);
const togglingId = ref(null);
const deletingId = ref(null);

function handleConfirm() {
  const action = confirmAction.value;
  confirmAction.value = null;
  if (!action) {
    return;
  }
  if (action.type === "run") {
    doRun(action.task);
  } else {
    doDelete(action.task);
  }
}

async function doRun(task) {
  if (runningId.value) {
    return;
  }
  runningId.value = task.id;
  try {
    const result = await apiPost(`/api/system/scheduled-tasks/${task.id}/run`);
    if (result.success) {
      const runResult = result.data?.result;
      if (runResult?.ok) {
        showToast(`${t("scheduled_tasks.run_success")}: ${runResult.ok}`, "success");
      } else if (runResult?.err) {
        showToast(`${t("scheduled_tasks.run_failed")}: ${runResult.err}`, "error");
      } else {
        showToast(t("scheduled_tasks.run_success"), "success");
      }
      refetch();
    } else {
      showToast(`${t("scheduled_tasks.run_failed")}: ${result.message || ""}`, "error");
    }
  } finally {
    runningId.value = null;
  }
}

// 启停切换：处理期间防抖，连点会在请求返回前再次切换，最终状态被切回原样
async function doToggle(task) {
  if (togglingId.value) {
    return;
  }
  togglingId.value = task.id;
  try {
    const result = await apiPost(`/api/system/scheduled-tasks/${task.id}/toggle`);
    if (result.success) {
      showToast(t("scheduled_tasks.toggle_success"), "success");
      refetch();
    } else {
      showToast(t("scheduled_tasks.toggle_failed"), "error");
    }
  } finally {
    togglingId.value = null;
  }
}

async function doDelete(task) {
  if (deletingId.value) {
    return;
  }
  deletingId.value = task.id;
  try {
    const result = await apiDelete(`/api/system/scheduled-tasks/${task.id}`);
    if (result.success) {
      showToast(t("scheduled_tasks.delete_success"), "success");
      refetch();
    } else {
      showToast(`${t("scheduled_tasks.delete_failed")}: ${result.message || ""}`, "error");
    }
  } finally {
    deletingId.value = null;
  }
}

// ==================== 编辑 / 日志弹窗 ====================

const editingTask = ref(null);
const showTaskModal = ref(false);
const logsTaskName = ref("");
const showLogsModal = ref(false);

async function openEdit(task) {
  // 列表行数据不含完整 config 明细，编辑前拉取详情回填
  const result = await apiGet(`/api/system/scheduled-tasks/${task.id}`);
  if (!result.success) {
    showToast(`${t("common.load_failed")}: ${result.message || ""}`, "error");
    return;
  }
  editingTask.value = result.data;
  showTaskModal.value = true;
}

function openCreate() {
  editingTask.value = null;
  showTaskModal.value = true;
}
</script>

<template>
  <div class="tab-content" :class="{ active: props.active }">
    <div class="table-container">
      <table class="data-table" aria-label="Scheduled tasks">
        <thead>
          <tr>
            <th class="index-column">{{ t("common.index") }}</th>
            <th :class="sortClass(sortList, 'name')" @click="onThSort($event, sortList, 'name')">{{ t("common.name") }}</th>
            <th class="col-center" :class="sortClass(sortList, 'task_type')" @click="onThSort($event, sortList, 'task_type')">{{ t("common.type") }}</th>
            <th>{{ t("scheduled_tasks.cron") }}</th>
            <th class="col-center" :class="sortClass(sortList, 'enabled')" @click="onThSort($event, sortList, 'enabled')">{{ t("scheduled_tasks.status") }}</th>
            <th class="col-center" :class="sortClass(sortList, 'last_run_at')" @click="onThSort($event, sortList, 'last_run_at')">{{ t("scheduled_tasks.last_run") }}</th>
            <th class="col-center" :class="sortClass(sortList, 'last_result')" @click="onThSort($event, sortList, 'last_result')">{{ t("scheduled_tasks.last_result") }}</th>
            <th>
              <button v-if="!readonly" type="button" class="btn btn-primary" @click="openCreate">
                {{ t("scheduled_tasks.create") }}
              </button>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="tasksQuery.isError.value" class="empty-row">
            <td colspan="8" class="table-error-cell">{{ tasksQuery.error.value?.message || t("scheduled_tasks.load_failed") }}</td>
          </tr>
          <tr v-else-if="tasks.length === 0" class="empty-row">
            <td colspan="8" class="no-data">{{ t("scheduled_tasks.no_tasks") }}</td>
          </tr>
          <tr v-for="(task, index) in tasks" v-else :key="task.id">
            <td class="index-column">{{ index + 1 }}</td>
            <td>{{ task.name }}</td>
            <td class="col-center">{{ typeLabel(task.task_type) }}</td>
            <td><code>{{ task.cron_expression }}</code></td>
            <td class="col-center">
              <span class="status-badge" :class="task.enabled ? 'status-active' : 'status-inactive'">
                {{ task.enabled ? t("scheduled_tasks.enabled") : t("scheduled_tasks.disabled") }}
              </span>
            </td>
            <td class="col-center">{{ formatDateTime(task.last_run_at) }}</td>
            <td class="col-center">{{ task.last_result || "-" }}</td>
            <td v-if="!readonly" class="col-center">
              <button
                type="button"
                class="icon-btn btn-success"
                :disabled="runningId === task.id"
                :data-tooltip="t('scheduled_tasks.run_now')"
                :aria-label="t('scheduled_tasks.run_now')"
                @click="confirmAction = { type: 'run', task }"
              >
                <NavIcon name="play" />
              </button>
              <button
                type="button"
                class="icon-btn btn-warning"
                :disabled="togglingId === task.id"
                :data-tooltip="t(task.enabled ? 'scheduled_tasks.disable' : 'scheduled_tasks.enable')"
                :aria-label="t(task.enabled ? 'scheduled_tasks.disable' : 'scheduled_tasks.enable')"
                @click="doToggle(task)"
              >
                <NavIcon name="power" />
              </button>
              <button
                type="button"
                class="icon-btn btn-secondary"
                :data-tooltip="t('common.edit')"
                :aria-label="t('common.edit')"
                @click="openEdit(task)"
              >
                <NavIcon name="edit" />
              </button>
              <button
                type="button"
                class="icon-btn btn-secondary"
                :data-tooltip="t('scheduled_tasks.view_logs')"
                :aria-label="t('scheduled_tasks.view_logs')"
                @click="logsTaskName = task.name; showLogsModal = true"
              >
                <NavIcon name="list" />
              </button>
              <button
                type="button"
                class="icon-btn btn-danger"
                :disabled="deletingId === task.id"
                :data-tooltip="t('common.delete')"
                :aria-label="t('common.delete')"
                @click="confirmAction = { type: 'delete', task }"
              >
                <NavIcon name="trash" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <ScheduledTaskModal v-if="showTaskModal" :task="editingTask" @close="showTaskModal = false" @saved="refetch" />
    <TaskLogsModal v-if="showLogsModal" :task-name="logsTaskName" @close="showLogsModal = false" />
    <ConfirmModal
      v-if="confirmAction"
      :message="confirmMessage"
      :title="t('common.confirm')"
      :danger="confirmAction.type === 'delete'"
      @confirm="handleConfirm"
      @cancel="confirmAction = null"
    />
  </div>
</template>
