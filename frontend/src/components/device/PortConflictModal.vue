<script setup lang="ts">
/**
 * SNMP 端口名称冲突确认弹窗：
 * - 汇总条：冲突数与新增数（device.conflict_summary）；
 * - 冲突行：端口名 + 「现有描述 → SNMP 描述」+ 逐项决策下拉（跳过/覆盖），
 *   默认全部跳过；
 * - 底部动作：全部跳过 / 全部覆盖（同步所有下拉显示）；确认后把决策交给
 *   父组件执行同步（覆盖仅更新运行属性，不改 device_managed）。
 */
import { reactive } from "vue";
import { useI18n } from "vue-i18n";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import type { DeviceInterface } from "../../types/api/generated/DeviceInterface";
import type { SnmpPort } from "../../types/api/generated/SnmpPort";

/** 冲突对：SNMP 实时端口与现有网口 */
interface ConflictPair {
  snmpPort: SnmpPort;
  existingPort: DeviceInterface;
}

/** 逐项决策：skip=跳过 / overwrite=覆盖 */
type PortDecision = "skip" | "overwrite";

const props = defineProps<{
  /** 本轮冲突列表：{ snmpPort, existingPort } */
  conflicts: ConflictPair[];
  /** 本轮无冲突待新增端口数（汇总展示） */
  addCount: number;
}>();

const emit = defineEmits<{ close: []; confirm: [decisions: Record<string, PortDecision>] }>();

const { t } = useI18n();

// 逐项决策：{ 端口名: "skip" | "overwrite" }，默认跳过
const decisions = reactive<Record<string, PortDecision>>({});
for (const c of props.conflicts) {
  decisions[c.snmpPort.name] = "skip";
}

function setAll(value: PortDecision): void {
  for (const c of props.conflicts) {
    decisions[c.snmpPort.name] = value;
  }
}

/** 确认：把决策原样交给父组件计算覆盖/跳过并执行同步 */
function confirmDecisions(): void {
  emit("confirm", { ...decisions });
}
</script>

<template>
  <ModalShell :title="t('device.conflict_title')" @close="emit('close')">
    <div class="port-conflict-summary">
      {{ t("device.conflict_summary", { conflict: props.conflicts.length, add: props.addCount }) }}
    </div>
    <div class="port-conflict-list">
      <div v-for="c in props.conflicts" :key="c.snmpPort.name" class="port-conflict-row">
        <div class="conflict-port-number">{{ c.snmpPort.name }}</div>
        <div class="conflict-port-detail">
          {{ t("device.conflict_existing") }}: {{ c.existingPort.description || c.existingPort.name }}
          → {{ t("device.conflict_new") }}: {{ c.snmpPort.description || c.snmpPort.name }}
        </div>
        <select v-model="decisions[c.snmpPort.name]" class="conflict-select">
          <option value="skip">{{ t("device.conflict_skip") }}</option>
          <option value="overwrite">{{ t("device.conflict_overwrite") }}</option>
        </select>
      </div>
    </div>
    <div class="port-conflict-actions">
      <button
        type="button"
        class="icon-btn btn-secondary"
        :data-tooltip="t('device.conflict_skip_all')"
        :aria-label="t('device.conflict_skip_all')"
        @click="setAll('skip')"
      >
        <NavIcon name="x" />
      </button>
      <button
        type="button"
        class="icon-btn btn-edit"
        :data-tooltip="t('device.conflict_overwrite_all')"
        :aria-label="t('device.conflict_overwrite_all')"
        @click="setAll('overwrite')"
      >
        <NavIcon name="edit" />
      </button>
    </div>
    <template #footer>
      <button type="button" class="btn-primary" @click="confirmDecisions">
        {{ t("device.conflict_confirm") }}
      </button>
    </template>
  </ModalShell>
</template>
