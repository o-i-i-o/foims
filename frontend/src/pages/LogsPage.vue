<script setup>
/**
 * 日志管理页（移植自旧版 web/static/main.html logs 分区 + modules/log.js）：
 * 操作日志 / 登录日志 / 通知三个页签，排序 + 分页 + 操作日志搜索。
 * 数据层为 TanStack Query 首次实战接线：queryFn 内对归一化失败（success:false）
 * 抛出以进入 error 态；通知已读操作经 useMutation + 失效缓存刷新。
 */
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiGet, apiPut } from "../api/client.js";
import {
  formatDateTime,
  getOperationTypeText,
  getResourceTypeText,
  translateNotificationContent,
  translateServerKey
} from "../utils/formatter.js";
import { showToast } from "../composables/useToast.js";
import ConfirmModal from "../components/ConfirmModal.vue";
import LogDetailsModal from "../components/LogDetailsModal.vue";
import NavIcon from "../components/NavIcon.vue";
import Pagination from "../components/Pagination.vue";

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 页签状态（记忆 + 深链） ====================

const TABS = [
  { id: "operation", titleKey: "logs.operation_logs" },
  { id: "login", titleKey: "logs.login_logs" },
  { id: "notifications", titleKey: "logs.notifications" }
];
const SUBTAB_KEY = "foims_subtab_logs";

const route = useRoute();
const router = useRouter();

function readInitialTab() {
  // 深链 #/logs?tab=login 优先，其次恢复上次记忆的页签，默认操作日志
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
  return "operation";
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

// ==================== 分页列表（排序 + 越界回退） ====================

const OPERATION_PAGE_SIZE = 50;
const LOGIN_PAGE_SIZE = 50;
const NOTIFICATION_PAGE_SIZE = 20;

/**
 * 分页列表封装：created_at 倒序为默认排序，排序切换重置页码；
 * 空页且页码越界（末页数据删除后）自动回退最后一页。
 */
function usePagedList({ tag, pageSize, enabled, buildUrl, extraKey }) {
  const page = ref(1);
  const sort = reactive({ by: "created_at", order: "desc" });

  const query = useQuery({
    queryKey: computed(() => [
      tag,
      { page: page.value, sortBy: sort.by, sortOrder: sort.order, ...extraKey?.value }
    ]),
    queryFn: async () => {
      const result = await apiGet(buildUrl(page.value, sort.by, sort.order));
      if (!result.success) {
        // apiClient 已将失败归一为 { success:false } 不抛异常，
        // 此处显式抛出让查询进入 error 态（isError/error.message 可渲染）
        throw new Error(result.message || t("common.load_failed"));
      }
      return result.data;
    },
    enabled
  });

  watch(
    () => query.data.value,
    (data) => {
      if (!data || !Array.isArray(data.items) || data.items.length !== 0 || page.value <= 1) {
        return;
      }
      const totalPages = data.total_pages || Math.ceil((data.total || 0) / pageSize);
      if (totalPages > 0 && page.value > totalPages) {
        page.value = totalPages;
      }
    }
  );

  function toggleSort(key) {
    if (sort.by === key) {
      sort.order = sort.order === "asc" ? "desc" : "asc";
    } else {
      sort.by = key;
      sort.order = "asc";
    }
    page.value = 1;
  }

  return reactive({
    page,
    sort,
    toggleSort,
    pageSize,
    items: computed(() => (Array.isArray(query.data.value?.items) ? query.data.value.items : [])),
    total: computed(() => query.data.value?.total ?? 0),
    totalPages: computed(() => query.data.value?.total_pages ?? 1),
    isError: computed(() => query.isError.value),
    errorMessage: computed(() => query.error.value?.message || t("common.load_failed"))
  });
}

// ==================== 操作日志（含搜索） ====================

const operationSearch = ref("");

const operation = usePagedList({
  tag: "logs-operation",
  pageSize: OPERATION_PAGE_SIZE,
  enabled: computed(() => activeTab.value === "operation"),
  extraKey: computed(() => ({ search: operationSearch.value })),
  buildUrl: (page, sortBy, sortOrder) => {
    const params = new URLSearchParams({
      page,
      page_size: OPERATION_PAGE_SIZE,
      sort_by: sortBy,
      sort_order: sortOrder
    });
    if (operationSearch.value) {
      params.set("action", operationSearch.value);
    }
    return `/api/logs/operation?${params}`;
  }
});

// 搜索框位于操作日志表头：防抖 500ms，搜索词变化重置页码
const searchInput = ref("");
let searchTimer = null;

watch(searchInput, (value) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    operation.page = 1;
    operationSearch.value = value;
  }, 500);
});

// ==================== 登录日志 / 通知 ====================

const login = usePagedList({
  tag: "logs-login",
  pageSize: LOGIN_PAGE_SIZE,
  enabled: computed(() => activeTab.value === "login"),
  buildUrl: (page, sortBy, sortOrder) =>
    `/api/logs/login?${new URLSearchParams({
      page,
      page_size: LOGIN_PAGE_SIZE,
      sort_by: sortBy,
      sort_order: sortOrder
    })}`
});

const notifications = usePagedList({
  tag: "notifications",
  pageSize: NOTIFICATION_PAGE_SIZE,
  enabled: computed(() => activeTab.value === "notifications"),
  buildUrl: (page, sortBy, sortOrder) =>
    `/api/notifications?${new URLSearchParams({
      page,
      page_size: NOTIFICATION_PAGE_SIZE,
      sort_by: sortBy,
      sort_order: sortOrder
    })}`
});

// ==================== 通知已读操作 ====================

const markReadMutation = useMutation({
  mutationFn: (id) => apiPut(`/api/notifications/${id}/read`, {}),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: ["notifications"] });
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  }
});

const showMarkAllConfirm = ref(false);

const markAllMutation = useMutation({
  mutationFn: () => apiPut("/api/notifications/mark-all-read", {}),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: ["notifications"] });
      showToast(t("notifications.marked_all_read"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  }
});

function handleMarkAllConfirmed() {
  showMarkAllConfirm.value = false;
  markAllMutation.mutate();
}

// ==================== 操作日志详情 ====================

const detailsLog = ref(null);
</script>

<template>
  <section id="logs" class="content-section active" aria-label="Logs">
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

    <div id="operation-tab" v-show="activeTab === 'operation'" class="tab-content" role="tabpanel">
      <div class="table-container">
        <table id="operation-logs-table" class="data-table" aria-label="Operation logs">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: operation.sort.by === 'created_at', asc: operation.sort.by === 'created_at' && operation.sort.order === 'asc', desc: operation.sort.by === 'created_at' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('created_at')"
              >
                {{ t("logs.operation_time") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: operation.sort.by === 'username', asc: operation.sort.by === 'username' && operation.sort.order === 'asc', desc: operation.sort.by === 'username' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('username')"
              >
                {{ t("logs.operator") }}
              </th>
              <th
                class="sortable col-center"
                :class="{ sorted: operation.sort.by === 'action', asc: operation.sort.by === 'action' && operation.sort.order === 'asc', desc: operation.sort.by === 'action' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('action')"
              >
                {{ t("logs.action") }}
              </th>
              <th
                class="sortable col-center"
                :class="{ sorted: operation.sort.by === 'resource_type', asc: operation.sort.by === 'resource_type' && operation.sort.order === 'asc', desc: operation.sort.by === 'resource_type' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('resource_type')"
              >
                {{ t("logs.resource_type") }}
              </th>
              <th
                class="sortable col-center"
                :class="{ sorted: operation.sort.by === 'result', asc: operation.sort.by === 'result' && operation.sort.order === 'asc', desc: operation.sort.by === 'result' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('result')"
              >
                {{ t("logs.result") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: operation.sort.by === 'ip_address', asc: operation.sort.by === 'ip_address' && operation.sort.order === 'asc', desc: operation.sort.by === 'ip_address' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('ip_address')"
              >
                {{ t("logs.ip_address") }}
              </th>
              <th>
                <input
                  v-model="searchInput"
                  type="text"
                  class="form-control form-control-sm w-auto"
                  :placeholder="t('logs.search_placeholder')"
                  :aria-label="t('logs.search_placeholder')"
                >
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="operation.isError" class="empty-row">
              <td colspan="8" class="text-center">{{ operation.errorMessage }}</td>
            </tr>
            <tr v-else-if="operation.items.length === 0" class="empty-row">
              <td colspan="8" class="text-center">{{ t("common.no_data") }}</td>
            </tr>
            <tr v-for="(log, index) in operation.items" v-else :key="log.id ?? index">
              <td class="index-column">{{ (operation.page - 1) * operation.pageSize + index + 1 }}</td>
              <td class="col-center col-time">{{ formatDateTime(log.created_at) }}</td>
              <td class="col-operator">{{ log.username || "-" }}</td>
              <td class="col-center">{{ getOperationTypeText(log.action) }}</td>
              <td class="col-center">{{ getResourceTypeText(log.resource_type) || "-" }}</td>
              <td class="col-center col-result">{{ log.success ? t("common.success") : t("common.failed") }}</td>
              <td>{{ log.ip_address || "-" }}</td>
              <td class="col-center">
                <button
                  type="button"
                  class="icon-btn btn-info"
                  :data-tooltip="t('logs.details')"
                  :aria-label="t('logs.details')"
                  @click="detailsLog = log"
                >
                  <NavIcon name="eye" />
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <Pagination
        v-if="operation.totalPages > 1"
        :page="operation.page"
        :total-pages="operation.totalPages"
        :total="operation.total"
        @page-change="(p) => (operation.page = p)"
      />
    </div>

    <div id="login-tab" v-show="activeTab === 'login'" class="tab-content" role="tabpanel">
      <div class="table-container">
        <table id="login-logs-table" class="data-table" aria-label="Login logs">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: login.sort.by === 'created_at', asc: login.sort.by === 'created_at' && login.sort.order === 'asc', desc: login.sort.by === 'created_at' && login.sort.order === 'desc' }"
                @click="login.toggleSort('created_at')"
              >
                {{ t("logs.login_time") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: login.sort.by === 'username', asc: login.sort.by === 'username' && login.sort.order === 'asc', desc: login.sort.by === 'username' && login.sort.order === 'desc' }"
                @click="login.toggleSort('username')"
              >
                {{ t("auth.username") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: login.sort.by === 'ip_address', asc: login.sort.by === 'ip_address' && login.sort.order === 'asc', desc: login.sort.by === 'ip_address' && login.sort.order === 'desc' }"
                @click="login.toggleSort('ip_address')"
              >
                {{ t("logs.ip_address") }}
              </th>
              <th>{{ t("logs.user_agent") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: login.sort.by === 'success', asc: login.sort.by === 'success' && login.sort.order === 'asc', desc: login.sort.by === 'success' && login.sort.order === 'desc' }"
                @click="login.toggleSort('success')"
              >
                {{ t("logs.login_result") }}
              </th>
              <th>{{ t("logs.error_message") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="login.isError" class="empty-row">
              <td colspan="7" class="text-center">{{ login.errorMessage }}</td>
            </tr>
            <tr v-else-if="login.items.length === 0" class="empty-row">
              <td colspan="7" class="text-center">{{ t("common.no_data") }}</td>
            </tr>
            <tr v-for="(log, index) in login.items" v-else :key="log.id ?? index">
              <td class="index-column">{{ (login.page - 1) * login.pageSize + index + 1 }}</td>
              <td class="col-center">{{ formatDateTime(log.created_at) }}</td>
              <td>{{ log.username }}</td>
              <td>{{ log.ip_address }}</td>
              <td>{{ log.user_agent || "-" }}</td>
              <td class="col-center">{{ log.success ? t("common.success") : t("common.failed") }}</td>
              <td>{{ translateServerKey(log.error_message) || "-" }}</td>
            </tr>
          </tbody>
        </table>
      </div>
      <Pagination
        v-if="login.totalPages > 1"
        :page="login.page"
        :total-pages="login.totalPages"
        :total="login.total"
        @page-change="(p) => (login.page = p)"
      />
    </div>

    <div id="notifications-tab" v-show="activeTab === 'notifications'" class="tab-content" role="tabpanel">
      <div class="table-container">
        <table id="notifications-table" class="data-table" aria-label="Notifications list">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: notifications.sort.by === 'created_at', asc: notifications.sort.by === 'created_at' && notifications.sort.order === 'asc', desc: notifications.sort.by === 'created_at' && notifications.sort.order === 'desc' }"
                @click="notifications.toggleSort('created_at')"
              >
                {{ t("notifications.time") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: notifications.sort.by === 'title', asc: notifications.sort.by === 'title' && notifications.sort.order === 'asc', desc: notifications.sort.by === 'title' && notifications.sort.order === 'desc' }"
                @click="notifications.toggleSort('title')"
              >
                {{ t("notifications.title") }}
              </th>
              <th>{{ t("notifications.content") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: notifications.sort.by === 'read', asc: notifications.sort.by === 'read' && notifications.sort.order === 'asc', desc: notifications.sort.by === 'read' && notifications.sort.order === 'desc' }"
                @click="notifications.toggleSort('read')"
              >
                {{ t("notifications.status") }}
              </th>
              <th>
                <div class="notifications-toolbar">
                  <button type="button" class="btn btn-secondary btn-sm" @click="showMarkAllConfirm = true">
                    {{ t("notifications.mark_all_read") }}
                  </button>
                </div>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="notifications.isError" class="empty-row">
              <td colspan="6" class="text-center">{{ notifications.errorMessage }}</td>
            </tr>
            <tr v-else-if="notifications.items.length === 0" class="empty-row">
              <td colspan="6" class="text-center">{{ t("notifications.no_data") }}</td>
            </tr>
            <tr v-for="(notification, index) in notifications.items" v-else :key="notification.id ?? index">
              <td class="index-column">{{ (notifications.page - 1) * notifications.pageSize + index + 1 }}</td>
              <td class="col-center">{{ formatDateTime(notification.created_at) }}</td>
              <td>{{ translateServerKey(notification.title) }}</td>
              <td>{{ translateNotificationContent(notification.content) }}</td>
              <td class="col-center">
                <span class="status-badge" :class="notification.read ? 'status-active' : 'status-inactive'">
                  {{ notification.read ? t("notifications.read") : t("notifications.unread") }}
                </span>
              </td>
              <td class="col-center">
                <button
                  v-if="!notification.read"
                  type="button"
                  class="icon-btn btn-primary"
                  :data-tooltip="t('notifications.mark_read')"
                  :aria-label="t('notifications.mark_read')"
                  @click="markReadMutation.mutate(notification.id)"
                >
                  <NavIcon name="check" />
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
      <Pagination
        v-if="notifications.totalPages > 1"
        :page="notifications.page"
        :total-pages="notifications.totalPages"
        :total="notifications.total"
        @page-change="(p) => (notifications.page = p)"
      />
    </div>

    <LogDetailsModal v-if="detailsLog" :log="detailsLog" @close="detailsLog = null" />
    <ConfirmModal
      v-if="showMarkAllConfirm"
      :message="t('notifications.confirm_mark_all_read')"
      @confirm="handleMarkAllConfirmed"
      @cancel="showMarkAllConfirm = false"
    />
  </section>
</template>

<style scoped>
/* 操作日志表：搜索框取代原操作列表头，收窄时间/操作人/结果列，
 让出宽度给含按钮与搜索框的最后一列（旧版 pages/logs.css 移植） */
#operation-logs-table td.col-time {
  width: 170px;
  white-space: nowrap;
}

#operation-logs-table td.col-operator {
  width: 120px;
  white-space: nowrap;
}

#operation-logs-table td.col-result {
  width: 80px;
  white-space: nowrap;
}

/* 通知表工具栏：全部标记已读按钮 */
.notifications-toolbar {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  flex-wrap: wrap;
}
</style>
