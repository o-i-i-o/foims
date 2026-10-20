<script setup>
/**
 * 系统管理页（移植自旧版 main.html system 分区七页签壳）：
 * 系统信息 / 用户管理 / 系统配置 / SMTP 配置 / 导入导出 / 定时任务 / 安全。
 * 页签常驻挂载（v-show）以保证各子页签查询组件的 enabled 按需加载语义，
 * 页签记忆 localStorage + hash query 深链（与 LogsPage 同机制）。
 */
import { ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import DataManagementTab from "../components/system/DataManagementTab.vue";
import NotificationTab from "../components/system/NotificationTab.vue";
import ScheduledTasksTab from "../components/system/ScheduledTasksTab.vue";
import SecurityTab from "../components/system/SecurityTab.vue";
import SystemConfigTab from "../components/system/SystemConfigTab.vue";
import SystemInfoTab from "../components/system/SystemInfoTab.vue";
import UsersTab from "../components/system/UsersTab.vue";

const { t } = useI18n();

// ==================== 页签状态（记忆 + 深链） ====================

const TABS = [
  { id: "system-info", titleKey: "system.info" },
  { id: "users", titleKey: "system.users" },
  { id: "system-config", titleKey: "system.config" },
  { id: "system-notification", titleKey: "system.notification" },
  { id: "data-management", titleKey: "system.data-management" },
  { id: "scheduled-tasks", titleKey: "system.scheduled_tasks" },
  { id: "security", titleKey: "system.security" }
];
const SUBTAB_KEY = "foims_subtab_system";

const route = useRoute();
const router = useRouter();

function readInitialTab() {
  // 深链 #/system?tab=users 优先，其次恢复上次记忆的页签，默认系统信息
  const fromQuery = String(route.query.tab ?? "");
  if (TABS.some((tab) => tab.id === fromQuery)) {
    return fromQuery;
  }
  try {
    const saved = localStorage.getItem(SUBTAB_KEY);
    if (TABS.some((tab) => tab.id === saved)) {
      return saved;
    }
  } catch {
    // 存储被禁用时忽略
  }
  return "system-info";
}

const activeTab = ref(readInitialTab());

function switchTab(tabId) {
  if (tabId === activeTab.value) {
    return;
  }
  activeTab.value = tabId;
  try {
    localStorage.setItem(SUBTAB_KEY, tabId);
  } catch {
    // 存储被禁用时仅会话内生效
  }
  // hash query 记录页签，便于书签直达（replace 不产生历史记录）
  router.replace({ query: { ...route.query, tab: tabId } });
}

// 浏览器前进/后退时按 URL 同步页签
watch(
  () => route.query.tab,
  (value) => {
    const tabId = String(value ?? "");
    if (TABS.some((tab) => tab.id === tabId) && tabId !== activeTab.value) {
      activeTab.value = tabId;
    }
  }
);
</script>

<template>
  <section id="system" class="content-section active" aria-label="System">
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
