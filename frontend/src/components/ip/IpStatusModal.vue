<script setup lang="ts">
/**
 * IP 状态人工维护弹窗（PATCH /api/resources/ip/{id}）：
 * - 状态三选一：active（使用中）/ inactive（停用）/ reserved（保留）；
 * - reserved 为人工保留位：自动判活与陈旧停用都会跳过该地址，是「地址仍归属
 *   本网口但暂时不用」的唯一人工入口；
 * - 地址、网口归属与子网迁移由设备网卡的网口/IP 同步维护，不在此弹窗改动。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { apiPatch } from "../../api/client";
import { showToast } from "../../composables/useToast";
import ModalShell from "../ModalShell.vue";

const props = defineProps<{
  /** interface_ips.id */
  ipId: string;
  /** 列表行展示用的地址文本（弹窗提示语内引用） */
  ipAddress: string;
  /** 当前状态，作为下拉初值 */
  status: string;
}>();

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();

/** 状态选项：文案复用全局 status.* 键，说明文案解释该状态对自动化的影响 */
const STATUS_OPTIONS = [
  { value: "active", labelKey: "status.active", tipKey: "ip.status_active_tip" },
  { value: "inactive", labelKey: "status.inactive", tipKey: "ip.status_inactive_tip" },
  { value: "reserved", labelKey: "status.reserved", tipKey: "ip.status_reserved_tip" }
];

const selected = ref(props.status);
const saving = ref(false);

/** 当前选中状态的说明文案键 */
const selectedTipKey = computed(
  () => STATUS_OPTIONS.find((option) => option.value === selected.value)?.tipKey ?? ""
);

async function submit(): Promise<void> {
  if (saving.value) {
    return;
  }
  saving.value = true;
  try {
    const result = await apiPatch<unknown, { status: string }>(
      `/api/resources/ip/${props.ipId}`,
      { status: selected.value }
    );
    if (!result.success) {
      showToast(result.message, "error");
      return;
    }
    showToast(result.message, "success");
    emit("saved");
    emit("close");
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <ModalShell size="sm" @close="emit('close')">
    <template #title>{{ t("ip.status_edit_title") }}</template>
    <form @submit.prevent="submit">
      <p class="hint-text">{{ t("ip.status_edit_hint", { ip: props.ipAddress }) }}</p>
      <div class="form-group">
        <label for="ip-status-select">{{ t("ip.status") }}</label>
        <select id="ip-status-select" v-model="selected">
          <option v-for="option in STATUS_OPTIONS" :key="option.value" :value="option.value">
            {{ t(option.labelKey) }}
          </option>
        </select>
      </div>
      <p class="hint-text">{{ t(selectedTipKey) }}</p>
    </form>
    <template #footer>
      <button type="button" class="btn-secondary" :disabled="saving" @click="emit('close')">
        {{ t("common.cancel") }}
      </button>
      <button
        type="button"
        class="btn-primary"
        :disabled="saving || selected === props.status"
        @click="submit"
      >
        {{ t("common.save") }}
      </button>
    </template>
  </ModalShell>
</template>
