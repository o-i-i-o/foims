<script setup lang="ts">
/**
 * 定时任务创建/编辑弹窗：
 * 任务类型五选一 + cron 五字段逐段校验（含预览与示例表）+
 * mac_sync 设备复选/子网下拉配置 + log_cleanup / ip_status_sync 天数配置。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiGet } from "../../api/client";
import { createScheduledTask, updateScheduledTask } from "../../api/scheduledTasks";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";
import CronExamplesModal from "./CronExamplesModal.vue";
import type { Paged } from "../../types/api";
import type { JsonValue } from "../../types/api/generated/serde_json/JsonValue";
import type { ScheduledTask } from "../../types/api/generated/ScheduledTask";
import type { ScheduledTaskCreate } from "../../types/api/generated/ScheduledTaskCreate";
import type { Device } from "../../types/api/generated/Device";
import type { Network } from "../../types/api/generated/Network";

/** 设备列表行：generated Device + 列表端点附加的计算字段（未入 generated 模型，边界扩展） */
type DeviceRow = Device & { snmp_configured: boolean; ip_address: string | null };

/** cron 单段边界定义 */
interface CronFieldDef {
  min: number;
  max: number;
  allowLastDay?: boolean;
}

const props = withDefaults(
  defineProps<{
    /** 编辑任务对象；null 表示创建 */
    task?: ScheduledTask | null;
  }>(),
  { task: null }
);

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

const isEdit = computed(() => Boolean(props.task));

// 后端 config 为自由 JsonValue，字段历史数据可能缺省或类型漂移，统一收窄取值

/** 读取 config 中指定键的字符串数组（非数组或元素非字符串一律丢弃） */
function configStringArray(config: JsonValue | null, key: string): string[] {
  if (config !== null && typeof config === "object" && !Array.isArray(config)) {
    const value = config[key];
    if (Array.isArray(value)) {
      return value.filter((item): item is string => typeof item === "string");
    }
  }
  return [];
}

/** 读取 config 中指定键的字符串 */
function configString(config: JsonValue | null, key: string): string {
  if (config !== null && typeof config === "object" && !Array.isArray(config)) {
    const value = config[key];
    if (typeof value === "string") {
      return value;
    }
  }
  return "";
}

/** 读取 config 中指定键的数值（缺省返回 null） */
function configNumber(config: JsonValue | null, key: string): number | null {
  if (config !== null && typeof config === "object" && !Array.isArray(config)) {
    const value = config[key];
    if (typeof value === "number") {
      return value;
    }
  }
  return null;
}

const name = ref(props.task?.name || "");
const taskType = ref(props.task?.task_type || "mac_sync");
const enabled = ref(props.task ? Boolean(props.task.enabled) : true);

// cron 五字段：标准 Linux cron 顺序（分 时 日 月 星期）。
// 兼容历史 6 段（含秒）表达式：新表单不含秒位，去掉秒段后回填
const CRON_FIELD_DEFS: CronFieldDef[] = [
  { min: 0, max: 59 },
  { min: 0, max: 23 },
  { min: 1, max: 31, allowLastDay: true },
  { min: 1, max: 12 },
  { min: 0, max: 7 }
];
const CRON_FIELD_LABEL_KEYS = [
  "scheduled_tasks.cron_fields.minute",
  "scheduled_tasks.cron_fields.hour",
  "scheduled_tasks.cron_fields.day",
  "scheduled_tasks.cron_fields.month",
  "scheduled_tasks.cron_fields.weekday"
];

function parseCronFields(expression: string | null | undefined): string[] {
  const fields = (expression || "").trim().split(/\s+/);
  if (fields.length === 6) {
    fields.shift();
  }
  return CRON_FIELD_DEFS.map((_, i) => fields[i] ?? "*");
}

const cronFields = ref<string[]>(parseCronFields(props.task?.cron_expression));
// 逐字段非法标记（cron-field-invalid 标红）
const cronInvalid = ref<boolean[]>(CRON_FIELD_DEFS.map(() => false));
const showCronExamples = ref(false);

const cronPreview = computed(() => cronFields.value.join(" "));

// 单段语法：*、数字、区间 a-b，均可带 /步进；逗号列表由调用方拆段逐个校验
const CRON_SEGMENT_PATTERN = /^(\*|\d+|\d+-\d+)(\/\d+)?$/;

// 单字段合法性：逐段校验语法，数值端点须落在该字段边界内，区间还须起点≤终点
function isValidCronFieldValue(value: string, field: CronFieldDef): boolean {
  if (!value) {
    return false;
  }
  return value.split(",").every((part) => {
    // 日字段额外支持 L（当月最后一天，后端解析器与触发库 croner 均支持）
    if (field.allowLastDay === true && part === "L") {
      return true;
    }
    if (!CRON_SEGMENT_PATTERN.test(part)) {
      return false;
    }
    const [base = "", step] = part.split("/");
    // 步长必须为正整数：0 步长会被后端拒绝（除零防护）
    if (step !== undefined && Number(step) < 1) {
      return false;
    }
    if (base === "*") {
      return true;
    }
    const bounds = base.split("-").map(Number);
    if (bounds.some((n) => n < field.min || n > field.max)) {
      return false;
    }
    const start = bounds[0];
    const end = bounds[1];
    return bounds.length < 2 || (start !== undefined && end !== undefined && start <= end);
  });
}

// 逐字段校验并标红非法输入：全部合法时返回拼接后的表达式，否则返回 null
function validateCronFields(): string | null {
  let allValid = true;
  const parts: string[] = [];
  cronFields.value.forEach((value, index) => {
    const field = CRON_FIELD_DEFS[index];
    const valid = field !== undefined && isValidCronFieldValue(value, field);
    cronInvalid.value[index] = !valid;
    if (!valid) {
      allValid = false;
    }
    parts.push(value);
  });
  return allValid ? parts.join(" ") : null;
}

function clearCronInvalid(index: number): void {
  cronInvalid.value[index] = false;
}

const devicesQuery = useQuery({
  queryKey: ["st-devices"],
  queryFn: async () => {
    const result = await apiGet<Paged<DeviceRow>>("/api/resources/devices?page_size=1000");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  // 仅 mac_sync 需要设备复选列表
  enabled: computed(() => taskType.value === "mac_sync")
});

// 与 IP 页拉取模态框同口径：仅列出配置了 SNMP 的设备
const snmpDevices = computed<DeviceRow[]>(() => {
  const items = devicesQuery.data.value?.items ?? [];
  return items.filter((dev) => dev.snmp_configured);
});

const networksQuery = useQuery({
  queryKey: ["st-networks"],
  queryFn: async () => {
    const result = await apiGet<Paged<Network> | Network[]>("/api/resources/networks");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => taskType.value === "mac_sync")
});

const networks = computed<Network[]>(() => {
  const data = networksQuery.data.value;
  return Array.isArray(data) ? data : Array.isArray(data?.items) ? data.items : [];
});

// 勾选设备集合：多设备依次同步、后设备观测覆盖先设备写入
const selectedDeviceIds = ref<Set<string>>(new Set(configStringArray(props.task?.config ?? null, "device_ids")));
const subnetId = ref(configString(props.task?.config ?? null, "subnet_id"));

function toggleDevice(id: string): void {
  const next = new Set(selectedDeviceIds.value);
  if (next.has(id)) {
    next.delete(id);
  } else {
    next.add(id);
  }
  selectedDeviceIds.value = next;
}

const keepDays = ref(String(configNumber(props.task?.config ?? null, "days") ?? 30));
const staleDays = ref(String(configNumber(props.task?.config ?? null, "days") ?? 30));

const submitInFlight = ref(false);

async function handleSubmit(): Promise<void> {
  if (submitInFlight.value) {
    return;
  }
  if (!name.value.trim() || !taskType.value) {
    showToast(t("scheduled_tasks.required_fields"), "warning");
    return;
  }
  // cron 逐字段校验，非法时提前拦截：后端对创建仅告警落库，任务会静默永不执行
  const cronExpression = validateCronFields();
  if (!cronExpression) {
    showToast(t("scheduled_tasks.cron_invalid"), "warning");
    return;
  }

  let config: Record<string, JsonValue> = {};
  if (taskType.value === "mac_sync") {
    // 子网可缺省：空值表示全部子网，设备至少勾选一台
    const deviceIds = [...selectedDeviceIds.value];
    if (deviceIds.length === 0) {
      showToast(t("scheduled_tasks.required_fields"), "warning");
      return;
    }
    config = subnetId.value ? { device_ids: deviceIds, subnet_id: subnetId.value } : { device_ids: deviceIds };
  } else if (taskType.value === "log_cleanup") {
    config = { days: Number.parseInt(keepDays.value, 10) || 30 };
  } else if (taskType.value === "ip_status_sync") {
    // 判停阈值下限为 1 天：0 会在下一轮把全部地址判停
    const days = Number.parseInt(staleDays.value, 10) || 30;
    if (days < 1) {
      showToast(t("scheduled_tasks.config_fields.stale_days_invalid"), "warning");
      return;
    }
    config = { days };
  }

  const data: ScheduledTaskCreate = {
    name: name.value.trim(),
    task_type: taskType.value,
    cron_expression: cronExpression,
    enabled: enabled.value,
    config
  };

  submitInFlight.value = true;
  try {
    const result = isEdit.value && props.task
      ? await updateScheduledTask(props.task.id, data)
      : await createScheduledTask(data);
    if (result.success) {
      showToast(isEdit.value ? t("scheduled_tasks.update_success") : t("scheduled_tasks.create_success"), "success");
      queryClient.invalidateQueries({ queryKey: ["scheduled-tasks"] });
      emit("saved");
      emit("close");
    } else {
      showToast(`${t("scheduled_tasks.save_failed")}: ${result.message || ""}`, "error");
    }
  } finally {
    submitInFlight.value = false;
  }
}
</script>

<template>
  <ModalShell :title="t(isEdit ? 'scheduled_tasks.edit_task' : 'scheduled_tasks.create_task')" size="md" @close="emit('close')">
    <form @submit.prevent="handleSubmit">
      <div class="form-group">
        <label for="scheduled-task-name"><span>{{ t("scheduled_tasks.name") }}</span><span class="required">*</span></label>
        <input id="scheduled-task-name" v-model="name" type="text" required autocomplete="off">
      </div>
      <div class="form-group">
        <label for="scheduled-task-type"><span>{{ t("scheduled_tasks.type") }}</span><span class="required">*</span></label>
        <select id="scheduled-task-type" v-model="taskType" required>
          <option value="mac_sync">{{ t("scheduled_tasks.task_types.mac_sync") }}</option>
          <option value="token_cleanup">{{ t("scheduled_tasks.task_types.token_cleanup") }}</option>
          <option value="backup">{{ t("scheduled_tasks.task_types.backup") }}</option>
          <option value="log_cleanup">{{ t("scheduled_tasks.task_types.log_cleanup") }}</option>
          <option value="ip_status_sync">{{ t("scheduled_tasks.task_types.ip_status_sync") }}</option>
        </select>
      </div>
      <div class="form-group cron-field-group">
        <label><span>{{ t("scheduled_tasks.cron") }}</span><span class="required">*</span></label>
        <!-- 标准 Linux cron 五段格式：分 时 日 月 星期，提交时以空格拼成完整表达式 -->
        <div class="cron-field-row">
          <div v-for="(labelKey, index) in CRON_FIELD_LABEL_KEYS" :key="labelKey" class="cron-field">
            <input
              v-model="cronFields[index]"
              type="text"
              required
              :title="t(labelKey)"
              :class="{ 'cron-field-invalid': cronInvalid[index] }"
              @input="clearCronInvalid(index)"
            >
            <span class="cron-field-label">{{ t(labelKey) }}</span>
          </div>
        </div>
        <small class="hint-text cron-preview-hint">
          <span>{{ t("scheduled_tasks.cron_preview") }}</span>:
          <code>{{ cronPreview }}</code>
          <a href="#" @click.prevent="showCronExamples = true">{{ t("scheduled_tasks.cron_hint_link") }}</a>
        </small>
      </div>
      <!-- MAC 同步配置：两列布局，左侧设备复选列表，右侧子网下拉
          （与 IP 页拉取模态框同布局；窄屏由 form-row auto-fit 自动回落单列） -->
      <div v-show="taskType === 'mac_sync'" class="form-row">
        <div class="form-group form-group-block">
          <span class="group-label"><span>{{ t("scheduled_tasks.config_fields.device") }}</span><span class="required">*</span></span>
          <div class="users-checkbox-list">
            <div v-if="devicesQuery.isError.value" class="user-checkbox-item">{{ t("ip.load_failed_short") }}</div>
            <div v-else-if="snmpDevices.length === 0" class="user-checkbox-item">{{ t("ip.no_snmp_device") }}</div>
            <div v-for="dev in snmpDevices" v-else :key="dev.id" class="user-checkbox-item">
              <input
                :id="`scheduled-task-device-${dev.id}`"
                type="checkbox"
                :checked="selectedDeviceIds.has(dev.id)"
                @change="toggleDevice(dev.id)"
              >
              <label :for="`scheduled-task-device-${dev.id}`" class="user-info">
                <span class="user-name">{{ dev.name || dev.hostname || dev.id }}</span>
                <span class="user-email">{{ dev.ip_address || "-" }}</span>
              </label>
            </div>
          </div>
        </div>
        <div class="form-group form-group-block">
          <label for="scheduled-task-subnet-id"><span>{{ t("scheduled_tasks.config_fields.network") }}</span></label>
          <select id="scheduled-task-subnet-id" v-model="subnetId">
            <!-- 空值选项即“全部子网”：不选具体子网时同步该设备的全部子网 -->
            <option value="">{{ t("scheduled_tasks.config_fields.all_networks") }}</option>
            <option v-for="net in networks" :key="net.id" :value="net.id">{{ net.name || net.id }}</option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div v-show="taskType === 'log_cleanup'" class="form-group">
          <label for="scheduled-task-keep-days"><span>{{ t("scheduled_tasks.config_fields.keep_days") }}</span></label>
          <input id="scheduled-task-keep-days" v-model="keepDays" type="number" min="0">
        </div>
        <div v-show="taskType === 'ip_status_sync'" class="form-group">
          <label for="scheduled-task-stale-days"><span>{{ t("scheduled_tasks.config_fields.stale_days") }}</span></label>
          <input id="scheduled-task-stale-days" v-model="staleDays" type="number" min="1">
        </div>
      </div>
    </form>
    <template #footer>
      <label class="checkbox-label">
        <input v-model="enabled" type="checkbox">
        <span>{{ t("scheduled_tasks.enabled") }}</span>
      </label>
      <button type="button" class="btn-primary" :disabled="submitInFlight" @click="handleSubmit">
        {{ t("common.save") }}
      </button>
    </template>
  </ModalShell>
  <CronExamplesModal v-if="showCronExamples" @close="showCronExamples = false" />
</template>
