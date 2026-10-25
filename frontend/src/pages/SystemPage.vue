<script setup lang="ts">
/**
 * 系统管理页（移植自旧版 main.html system 分区七页签壳）：
 * 系统信息 / 用户管理 / 系统配置 / SMTP 配置 / 导入导出 / 定时任务 / 安全。
 * 页签常驻挂载（v-show）以保证各子页签查询组件的 enabled 按需加载语义，
 * 页签记忆 + hash query 深链由 useSubTabs 统一承载（与日志/资源分区同机制）。
 */
import { useI18n } from "vue-i18n";
import { useSubTabs, type SubTabMeta } from "../composables/useSubTabs";
import DataManagementTab from "../components/system/DataManagementTab.vue";
import NotificationTab from "../components/system/NotificationTab.vue";
import ScheduledTasksTab from "../components/system/ScheduledTasksTab.vue";
import SecurityTab from "../components/system/SecurityTab.vue";
import SystemConfigTab from "../components/system/SystemConfigTab.vue";
import SystemInfoTab from "../components/system/SystemInfoTab.vue";
import UsersTab from "../components/system/UsersTab.vue";

const { t } = useI18n();

// ==================== 页签状态（记忆 + 深链） ====================

// 页签元数据：id 为深链/记忆键，titleKey 为标题 i18n 键
const TABS = [
  { id: "system-info", titleKey: "system.info" },
  { id: "users", titleKey: "system.users" },
  { id: "system-config", titleKey: "system.config" },
  { id: "system-notification", titleKey: "system.notification" },
  { id: "data-management", titleKey: "system.data-management" },
  { id: "scheduled-tasks", titleKey: "system.scheduled_tasks" },
  { id: "security", titleKey: "system.security" }
] as const satisfies readonly [SubTabMeta, ...SubTabMeta[]];

const { activeTab, switchTab } = useSubTabs({
  storageKey: "foims_subtab_system",
  tabs: TABS
});
</script>

<template>
  <section id="system" class="content-section active" :aria-label="t('nav.system')">
    <div class="tabs" role="tablist">
      <button
        v-for="tab in TABS"
        :key="tab.id"
        type="button"
        class="tab-btn"
        :class="{ active: activeTab === tab.id }"
        role="tab"
        :aria-selected="activeTab === tab.id"
        :aria-controls="`${tab.id}-tab`"
        :data-tab="tab.id"
        @click="switchTab(tab.id)"
      >
        {{ t(tab.titleKey) }}
      </button>
    </div>

    <!-- 页签常驻：子组件按 props.active 自行控制查询启停 -->
    <SystemInfoTab v-show="activeTab === 'system-info'" :active="activeTab === 'system-info'" />
    <UsersTab v-show="activeTab === 'users'" :active="activeTab === 'users'" />
    <SystemConfigTab v-show="activeTab === 'system-config'" :active="activeTab === 'system-config'" />
    <NotificationTab v-show="activeTab === 'system-notification'" :active="activeTab === 'system-notification'" />
    <DataManagementTab v-show="activeTab === 'data-management'" :active="activeTab === 'data-management'" />
    <ScheduledTasksTab v-show="activeTab === 'scheduled-tasks'" :active="activeTab === 'scheduled-tasks'" />
    <SecurityTab v-show="activeTab === 'security'" :active="activeTab === 'security'" />
  </section>
</template>
