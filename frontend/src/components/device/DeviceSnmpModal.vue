<script setup lang="ts">
/**
 * 设备 SNMP 汇聚弹窗：把设备行原本散落的 9 个 SNMP 相关按钮（端口/MAC 表/
 * LLDP/硬件清单/VLAN/CDP/LAG/PoE/观测事件）收敛为单一入口，弹窗内以顶部标签
 * 切换各功能。
 * - 每个功能复用既有子弹窗组件，传 `embedded` 让其以内联模式渲染（无遮罩/页头/
 *   Esc 栈，仅内联 body/footer），层级由本弹窗统一承载；
 * - 仅挂载当前标签对应的子组件（v-if），切换时按需加载，避免一次性触发 9 路请求。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import ModalShell from "../ModalShell.vue";
import UnifiedDevicePortsModal from "./UnifiedDevicePortsModal.vue";
import ArpModal from "./ArpModal.vue";
import LldpModal from "./LldpModal.vue";
import EntityModal from "./EntityModal.vue";
import VlanLedgerModal from "./VlanLedgerModal.vue";
import CdpModal from "./CdpModal.vue";
import LagModal from "./LagModal.vue";
import PoeModal from "./PoeModal.vue";
import SnmpEventsModal from "./SnmpEventsModal.vue";

const props = defineProps<{ deviceId: string; deviceName?: string }>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

// 标签元数据：id 决定挂载哪个子组件，labelKey 为标签文案 i18n 键（与设备行原按钮同源）
const TABS = [
  { id: "ports", labelKey: "device.ports" },
  { id: "mac", labelKey: "device.mac_table" },
  { id: "lldp", labelKey: "device.lldp" },
  { id: "entity", labelKey: "device.entity.menu" },
  { id: "vlan", labelKey: "device.vlan_ledger.menu" },
  { id: "cdp", labelKey: "device.cdp.menu" },
  { id: "lag", labelKey: "device.lag.menu" },
  { id: "poe", labelKey: "device.poe.menu" },
  { id: "events", labelKey: "device.snmp_events.title" }
] as const;

type TabId = (typeof TABS)[number]["id"];

const activeTab = ref<TabId>("ports");
</script>

<template>
  <ModalShell size="xl" @close="emit('close')">
    <template #title>{{ props.deviceName }} - {{ t("device.snmp_hub.title") }}</template>

    <!-- 顶部功能标签条：沿用页面级 .tabs/.tab-btn 视觉 -->
    <div class="tabs snmp-hub-tabs" role="tablist">
      <button
        v-for="tab in TABS"
        :key="tab.id"
        type="button"
        class="tab-btn"
        :class="{ active: activeTab === tab.id }"
        role="tab"
        :aria-selected="activeTab === tab.id"
        @click="activeTab = tab.id"
      >
        {{ t(tab.labelKey) }}
      </button>
    </div>

    <!-- 当前标签对应的子功能（内联模式挂载，切换即重新加载） -->
    <UnifiedDevicePortsModal
      v-if="activeTab === 'ports'"
      embedded
      :device-id="props.deviceId"
      :device-name="props.deviceName"
    />
    <ArpModal v-else-if="activeTab === 'mac'" embedded :device-id="props.deviceId" />
    <LldpModal v-else-if="activeTab === 'lldp'" embedded :device-id="props.deviceId" />
    <EntityModal v-else-if="activeTab === 'entity'" embedded :device-id="props.deviceId" />
    <VlanLedgerModal v-else-if="activeTab === 'vlan'" embedded :device-id="props.deviceId" />
    <CdpModal v-else-if="activeTab === 'cdp'" embedded :device-id="props.deviceId" />
    <LagModal v-else-if="activeTab === 'lag'" embedded :device-id="props.deviceId" />
    <PoeModal v-else-if="activeTab === 'poe'" embedded :device-id="props.deviceId" />
    <SnmpEventsModal v-else-if="activeTab === 'events'" embedded :device-id="props.deviceId" />
  </ModalShell>
</template>

<style scoped>
/* 9 个标签在窄弹窗内换行排布，覆盖全局 .tabs 的 flex:1 等分（避免每个标签过窄） */
.snmp-hub-tabs {
  flex-wrap: wrap;
  margin-bottom: 0.625rem;
}

.snmp-hub-tabs .tab-btn {
  flex: 0 0 auto;
}
</style>
