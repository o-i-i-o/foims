<script setup lang="ts">
/**
 * 日志管理页：操作日志 / 登录日志 / 运行日志 / 通知四个页签，
 * 排序 + 分页 + 操作日志搜索；运行日志读取本轮日志文件尾部、
 * SSE 通知驱动自动刷新（不走分页查询）。
 * 数据层为 TanStack Query 首次实战接线：queryFn 内对归一化失败（success:false）
 * 抛出以进入 error 态；通知已读操作经 useMutation + 失效缓存刷新。
 */
import { computed, nextTick, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQueryClient } from "@tanstack/vue-query";
import { loginLogsUrl, markAllNotificationsRead, markNotificationRead, notificationsUrl, operationLogsUrl } from "../api/logs";
import {
  formatDateTime,
  getOperationTypeText,
  getResourceTypeText,
  translateNotificationContent,
  translateServerKey
} from "../utils/formatter";
import { showToast } from "../composables/useToast";
import { onThSortKeydown, sortAria, usePagedList } from "../composables/usePagedList";
import { useSubTabs, type SubTabMeta } from "../composables/useSubTabs";
import { useLiveLogs } from "../composables/useLiveLogs";
import { queryKeys } from "../utils/queryKeys";
import type { LoginLog } from "../types/api/generated/LoginLog";
import type { Notification } from "../types/api/generated/Notification";
import type { OperationLog } from "../types/api/generated/OperationLog";
import ConfirmModal from "../components/ConfirmModal.vue";
import LogDetailsModal from "../components/LogDetailsModal.vue";
import NavIcon from "../components/NavIcon.vue";
import Pagination from "../components/Pagination.vue";

const { t } = useI18n();
const queryClient = useQueryClient();

// 页签元数据：id 为深链/记忆键，titleKey 为标题 i18n 键
const TABS = [
  { id: "operation", titleKey: "logs.operation_logs" },
  { id: "login", titleKey: "logs.login_logs" },
  { id: "runtime", titleKey: "logs.runtime_logs" },
  { id: "notifications", titleKey: "logs.notifications" }
] as const satisfies readonly [SubTabMeta, ...SubTabMeta[]];

const { activeTab, switchTab } = useSubTabs({
  storageKey: "foims_subtab_logs",
  tabs: TABS
});

const OPERATION_PAGE_SIZE = 50;
const LOGIN_PAGE_SIZE = 50;
const NOTIFICATION_PAGE_SIZE = 20;

const operationSearch = ref("");

const operation = usePagedList<OperationLog>({
  tag: queryKeys.logsOperation,
  defaultPageSize: OPERATION_PAGE_SIZE,
  enabled: computed(() => activeTab.value === "operation"),
  extraKey: computed(() => ({ search: operationSearch.value })),
  buildUrl: (page, pageSize, sortBy, sortOrder) => operationLogsUrl({ page, pageSize, sortBy, sortOrder }, operationSearch.value)
});

// 搜索框位于操作日志表头：防抖 500ms，搜索词变化重置页码
const searchInput = ref("");
let searchTimer: number | undefined;

watch(searchInput, (value) => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(() => {
    operation.page = 1;
    operationSearch.value = value;
  }, 500);
});

const login = usePagedList<LoginLog>({
  tag: queryKeys.logsLogin,
  defaultPageSize: LOGIN_PAGE_SIZE,
  enabled: computed(() => activeTab.value === "login"),
  buildUrl: (page, pageSize, sortBy, sortOrder) => loginLogsUrl({ page, pageSize, sortBy, sortOrder })
});

// 运行日志：页签激活时拉取本轮日志文件尾部并建 SSE 通知流，
// 收到「有新日志」通知后防抖刷新；切离页签即断开
const {
  logs: liveLogs,
  connected: liveConnected,
  paused: livePaused,
  clear: clearLiveLogs
} = useLiveLogs(computed(() => activeTab.value === "runtime"));

/** 解析后的日志行：raw 保留原文兜底，其余字段供结构化渲染（空串表示该行缺失此段） */
interface ParsedLogLine {
  raw: string;
  time: string;
  level: string;
  target: string;
  message: string;
}

// tracing fmt 文件行格式：`2026-11-02T19:30:45.123456  INFO 目标: 消息 k=v`
// （级别右对齐 5 列，故级别前为多个空格）
const LOG_LINE_PATTERN = /^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?)\s+(ERROR|WARN|INFO|DEBUG|TRACE)\s+(\S+):\s?(.*)$/;

// 解析单行日志；不符合格式（如多行消息的续行）时字段留空、按原文渲染
function parseLogLine(raw: string): ParsedLogLine {
  const match = LOG_LINE_PATTERN.exec(raw);
  if (!match) {
    return { raw, time: "", level: "", target: "", message: raw };
  }
  return {
    raw,
    time: match[1] ?? "",
    level: match[2] ?? "",
    target: match[3] ?? "",
    message: match[4] ?? ""
  };
}

// 时间列只显示时刻部分（本轮日志同属一天），微秒截断到毫秒降噪
function formatLogTime(time: string): string {
  const clock = /\d{2}:\d{2}:\d{2}(?:\.\d+)?/.exec(time)?.[0] ?? time;
  const dot = clock.indexOf(".");
  return dot === -1 ? clock : clock.slice(0, dot + 4);
}

// 级别筛选键：TRACE 归入 DEBUG；无级别行不参与级别过滤（各级别下都保留）
type LiveLevelKey = "error" | "warn" | "info" | "debug";
const LIVE_LEVELS: { key: LiveLevelKey; label: string; titleKey: string }[] = [
  { key: "error", label: "ERROR", titleKey: "logs.live_level_error" },
  { key: "warn", label: "WARN", titleKey: "logs.live_level_warn" },
  { key: "info", label: "INFO", titleKey: "logs.live_level_info" },
  { key: "debug", label: "DEBUG", titleKey: "logs.live_level_debug" }
];

const parsedLogs = computed(() => liveLogs.value.map(parseLogLine));

function levelKeyOf(line: ParsedLogLine): LiveLevelKey | null {
  if (!line.level) {
    return null;
  }
  const level = line.level.toLowerCase();
  return level === "trace" ? "debug" : (level as LiveLevelKey);
}

const levelFilter = ref<"all" | LiveLevelKey>("all");
const liveFilter = ref("");

// 各级行数（级别筛选 chips 计数），all 为总行数
const levelCounts = computed<Record<"all" | LiveLevelKey, number>>(() => {
  const counts: Record<"all" | LiveLevelKey, number> = {
    all: parsedLogs.value.length,
    error: 0,
    warn: 0,
    info: 0,
    debug: 0
  };
  for (const line of parsedLogs.value) {
    const key = levelKeyOf(line);
    if (key) {
      counts[key] += 1;
    }
  }
  return counts;
});

// 显示行 = 级别筛选 + 关键字子串过滤（大小写不敏感，作用于原文）
const visibleLogs = computed(() => {
  const keyword = liveFilter.value.trim().toLowerCase();
  return parsedLogs.value.filter((line) => {
    const key = levelKeyOf(line);
    if (levelFilter.value !== "all" && key !== null && key !== levelFilter.value) {
      return false;
    }
    return !keyword || line.raw.toLowerCase().includes(keyword);
  });
});

// 再次点击已选级别回到「全部」
function toggleLevelFilter(key: LiveLevelKey): void {
  levelFilter.value = levelFilter.value === key ? "all" : key;
}

// 跟随滚动：仅在用户本就停在底部时跟随新日志下滚，上翻查看历史不抢滚动条
const livePane = ref<HTMLElement | null>(null);
const livePinned = ref(true);

function onLiveScroll(): void {
  const pane = livePane.value;
  if (pane) {
    livePinned.value = pane.scrollHeight - pane.scrollTop - pane.clientHeight < 30;
  }
}

async function scrollLiveToBottom(): Promise<void> {
  await nextTick();
  const pane = livePane.value;
  if (pane) {
    pane.scrollTop = pane.scrollHeight;
  }
}

// 上翻后一键回到底部并恢复跟随
async function jumpToLatest(): Promise<void> {
  livePinned.value = true;
  await scrollLiveToBottom();
}

// 新日志到达、筛选变化或回到底部时，处于跟随态即滚到底
watch([visibleLogs, livePinned], async () => {
  if (!livePinned.value || livePaused.value) {
    return;
  }
  await scrollLiveToBottom();
});

const notifications = usePagedList<Notification>({
  tag: queryKeys.notifications,
  defaultPageSize: NOTIFICATION_PAGE_SIZE,
  enabled: computed(() => activeTab.value === "notifications"),
  buildUrl: (page, pageSize, sortBy, sortOrder) => notificationsUrl({ page, pageSize, sortBy, sortOrder })
});

const markReadMutation = useMutation({
  mutationFn: (id: string) => markNotificationRead(id),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: queryKeys.notifications });
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
  }
});

const showMarkAllConfirm = ref(false);

const markAllMutation = useMutation({
  mutationFn: () => markAllNotificationsRead(),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: queryKeys.notifications });
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

const detailsLog = ref<OperationLog | null>(null);
</script>

<template>
  <section id="logs" class="content-section active" :class="{ 'runtime-fill': activeTab === 'runtime' }" :aria-label="t('nav.logs')">
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

    <div id="operation-tab" v-show="activeTab === 'operation'" class="tab-content" :class="{ active: activeTab === 'operation' }" role="tabpanel">
      <div class="table-container">
        <table id="operation-logs-table" class="data-table" :aria-label="t('logs.operation_logs')">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: operation.sort.by === 'created_at', asc: operation.sort.by === 'created_at' && operation.sort.order === 'asc', desc: operation.sort.by === 'created_at' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('created_at')" tabindex="0" :aria-sort="sortAria(operation, 'created_at')" @keydown="onThSortKeydown($event, operation, 'created_at')"
              >
                {{ t("logs.operation_time") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: operation.sort.by === 'username', asc: operation.sort.by === 'username' && operation.sort.order === 'asc', desc: operation.sort.by === 'username' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('username')" tabindex="0" :aria-sort="sortAria(operation, 'username')" @keydown="onThSortKeydown($event, operation, 'username')"
              >
                {{ t("logs.operator") }}
              </th>
              <th
                class="sortable col-center"
                :class="{ sorted: operation.sort.by === 'action', asc: operation.sort.by === 'action' && operation.sort.order === 'asc', desc: operation.sort.by === 'action' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('action')" tabindex="0" :aria-sort="sortAria(operation, 'action')" @keydown="onThSortKeydown($event, operation, 'action')"
              >
                {{ t("logs.action") }}
              </th>
              <th
                class="sortable col-center"
                :class="{ sorted: operation.sort.by === 'resource_type', asc: operation.sort.by === 'resource_type' && operation.sort.order === 'asc', desc: operation.sort.by === 'resource_type' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('resource_type')" tabindex="0" :aria-sort="sortAria(operation, 'resource_type')" @keydown="onThSortKeydown($event, operation, 'resource_type')"
              >
                {{ t("logs.resource_type") }}
              </th>
              <th
                class="sortable col-center"
                :class="{ sorted: operation.sort.by === 'result', asc: operation.sort.by === 'result' && operation.sort.order === 'asc', desc: operation.sort.by === 'result' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('result')" tabindex="0" :aria-sort="sortAria(operation, 'result')" @keydown="onThSortKeydown($event, operation, 'result')"
              >
                {{ t("logs.result") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: operation.sort.by === 'ip_address', asc: operation.sort.by === 'ip_address' && operation.sort.order === 'asc', desc: operation.sort.by === 'ip_address' && operation.sort.order === 'desc' }"
                @click="operation.toggleSort('ip_address')" tabindex="0" :aria-sort="sortAria(operation, 'ip_address')" @keydown="onThSortKeydown($event, operation, 'ip_address')"
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
        @hover-page="operation.prefetch"
      />
    </div>

    <div id="login-tab" v-show="activeTab === 'login'" class="tab-content" :class="{ active: activeTab === 'login' }" role="tabpanel">
      <div class="table-container">
        <table id="login-logs-table" class="data-table" :aria-label="t('logs.login_logs')">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: login.sort.by === 'created_at', asc: login.sort.by === 'created_at' && login.sort.order === 'asc', desc: login.sort.by === 'created_at' && login.sort.order === 'desc' }"
                @click="login.toggleSort('created_at')" tabindex="0" :aria-sort="sortAria(login, 'created_at')" @keydown="onThSortKeydown($event, login, 'created_at')"
              >
                {{ t("logs.login_time") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: login.sort.by === 'username', asc: login.sort.by === 'username' && login.sort.order === 'asc', desc: login.sort.by === 'username' && login.sort.order === 'desc' }"
                @click="login.toggleSort('username')" tabindex="0" :aria-sort="sortAria(login, 'username')" @keydown="onThSortKeydown($event, login, 'username')"
              >
                {{ t("auth.username") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: login.sort.by === 'ip_address', asc: login.sort.by === 'ip_address' && login.sort.order === 'asc', desc: login.sort.by === 'ip_address' && login.sort.order === 'desc' }"
                @click="login.toggleSort('ip_address')" tabindex="0" :aria-sort="sortAria(login, 'ip_address')" @keydown="onThSortKeydown($event, login, 'ip_address')"
              >
                {{ t("logs.ip_address") }}
              </th>
              <th>{{ t("logs.user_agent") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: login.sort.by === 'success', asc: login.sort.by === 'success' && login.sort.order === 'asc', desc: login.sort.by === 'success' && login.sort.order === 'desc' }"
                @click="login.toggleSort('success')" tabindex="0" :aria-sort="sortAria(login, 'success')" @keydown="onThSortKeydown($event, login, 'success')"
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
        @hover-page="login.prefetch"
      />
    </div>

    <div id="runtime-tab" v-show="activeTab === 'runtime'" class="tab-content" :class="{ active: activeTab === 'runtime' }" role="tabpanel">
      <div class="live-toolbar">
        <span class="live-status" :class="{ connected: liveConnected }">
          <span class="live-dot" aria-hidden="true"></span>
          {{ liveConnected ? t("logs.live_connected") : t("logs.live_disconnected") }}
        </span>
        <span v-if="livePaused" class="live-paused-hint">{{ t("logs.live_paused_hint") }}</span>
        <div class="live-levels" role="group" :aria-label="t('logs.live_levels')">
          <button
            type="button"
            class="live-chip"
            :class="{ active: levelFilter === 'all' }"
            :aria-pressed="levelFilter === 'all'"
            @click="levelFilter = 'all'"
          >
            {{ t("logs.live_level_all") }}
            <span class="chip-count">{{ levelCounts.all }}</span>
          </button>
          <button
            v-for="level in LIVE_LEVELS"
            :key="level.key"
            type="button"
            class="live-chip"
            :class="{ active: levelFilter === level.key }"
            :title="t(level.titleKey)"
            :aria-pressed="levelFilter === level.key"
            @click="toggleLevelFilter(level.key)"
          >
            <span class="chip-dot" :class="`chip-dot-${level.key}`" aria-hidden="true"></span>
            {{ level.label }}
            <span class="chip-count">{{ levelCounts[level.key] }}</span>
          </button>
        </div>
        <div class="live-actions">
          <input
            v-model="liveFilter"
            type="text"
            class="form-control form-control-sm live-filter-input"
            :placeholder="t('logs.live_filter_placeholder')"
            :aria-label="t('logs.live_filter_placeholder')"
          >
          <button type="button" class="btn btn-secondary btn-sm" @click="livePaused = !livePaused">
            {{ livePaused ? t("logs.live_resume") : t("logs.live_pause") }}
          </button>
          <button type="button" class="btn btn-secondary btn-sm" @click="clearLiveLogs">
            {{ t("logs.live_clear") }}
          </button>
        </div>
      </div>
      <div class="live-wrap">
        <div ref="livePane" class="live-pane" @scroll="onLiveScroll">
          <div v-if="liveLogs.length === 0" class="live-empty">
            <p class="live-empty-title">{{ t("logs.live_empty") }}</p>
            <p v-if="liveConnected && !livePaused" class="live-empty-hint">{{ t("logs.live_empty_hint") }}</p>
          </div>
          <p v-else-if="visibleLogs.length === 0" class="live-filter-empty">{{ t("logs.live_filter_empty") }}</p>
          <template v-else>
            <p
              v-for="(line, index) in visibleLogs"
              :key="index"
              class="live-line"
              :class="line.level ? `live-${line.level.toLowerCase()}` : undefined"
            >
              <span v-if="line.time" class="live-time">{{ formatLogTime(line.time) }}</span>
              <span
                v-if="line.level"
                class="live-level-badge"
                :class="`badge-${line.level.toLowerCase()}`"
              >{{ line.level }}</span>
              <span v-if="line.target" class="live-target">{{ line.target }}</span>
              <span class="live-message">{{ line.message }}</span>
            </p>
          </template>
        </div>
        <button v-if="!livePinned" type="button" class="live-jump" @click="jumpToLatest">
          {{ t("logs.live_follow") }}
        </button>
      </div>
    </div>

    <div id="notifications-tab" v-show="activeTab === 'notifications'" class="tab-content" :class="{ active: activeTab === 'notifications' }" role="tabpanel">
      <div class="table-container">
        <table id="notifications-table" class="data-table" :aria-label="t('logs.notifications')">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: notifications.sort.by === 'created_at', asc: notifications.sort.by === 'created_at' && notifications.sort.order === 'asc', desc: notifications.sort.by === 'created_at' && notifications.sort.order === 'desc' }"
                @click="notifications.toggleSort('created_at')" tabindex="0" :aria-sort="sortAria(notifications, 'created_at')" @keydown="onThSortKeydown($event, notifications, 'created_at')"
              >
                {{ t("notifications.time") }}
              </th>
              <th
                class="sortable"
                :class="{ sorted: notifications.sort.by === 'title', asc: notifications.sort.by === 'title' && notifications.sort.order === 'asc', desc: notifications.sort.by === 'title' && notifications.sort.order === 'desc' }"
                @click="notifications.toggleSort('title')" tabindex="0" :aria-sort="sortAria(notifications, 'title')" @keydown="onThSortKeydown($event, notifications, 'title')"
              >
                {{ t("notifications.title") }}
              </th>
              <th>{{ t("notifications.content") }}</th>
              <th
                class="sortable col-center"
                :class="{ sorted: notifications.sort.by === 'read', asc: notifications.sort.by === 'read' && notifications.sort.order === 'asc', desc: notifications.sort.by === 'read' && notifications.sort.order === 'desc' }"
                @click="notifications.toggleSort('read')" tabindex="0" :aria-sort="sortAria(notifications, 'read')" @keydown="onThSortKeydown($event, notifications, 'read')"
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
        @hover-page="notifications.prefetch"
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
 让出宽度给含按钮与搜索框的最后一列 */
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

/* 运行日志：工具栏（状态/级别筛选/过滤/操作）+ 终端风格滚动窗（结构化行渲染） */
.live-toolbar {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  margin-bottom: 8px;
}

.live-status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--font-size-sm);
  color: var(--color-text-muted);
  white-space: nowrap;
}

.live-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--color-danger);
}

.live-status.connected .live-dot {
  background: var(--color-success);
}

/* 连接正常时状态点呼吸闪烁；用户偏好减少动效时不启用 */
@media (prefers-reduced-motion: no-preference) {
  .live-status.connected .live-dot {
    animation: live-pulse 2s ease-in-out infinite;
  }
}

@keyframes live-pulse {
  0%,
  100% {
    opacity: 1;
  }

  50% {
    opacity: 0.4;
  }
}

.live-paused-hint {
  font-size: var(--font-size-sm);
  color: var(--color-warning-text);
}

.live-levels {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}

.live-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 3px 10px;
  border: 1px solid var(--control-border);
  border-radius: 999px;
  background: var(--control-bg);
  color: var(--color-text-secondary);
  font-size: var(--font-size-xs);
  font-family: var(--font-mono);
  cursor: pointer;
  transition: var(--transition-fast);
}

.live-chip:hover {
  border-color: var(--control-border-hover);
  background: var(--color-bg-hover);
}

.live-chip.active {
  border-color: var(--color-primary);
  background: var(--color-primary-light);
  color: var(--color-primary-hover);
  font-weight: 600;
}

.chip-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}

.chip-dot-error {
  background: var(--color-danger);
}

.chip-dot-warn {
  background: var(--color-warning);
}

.chip-dot-info {
  background: var(--color-success);
}

.chip-dot-debug {
  background: var(--color-text-muted);
}

.chip-count {
  font-size: 10px;
  color: var(--color-text-muted);
}

.live-chip.active .chip-count {
  color: var(--color-primary-hover);
}

.live-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
}

.live-filter-input {
  width: 160px;
}

/* 运行日志页签激活：区块锁定一屏高并自带弹性列布局（与可视化页同一套路，
   不依赖 main-content.css 的 :has 分页规则；其他页签仍保持整页滚动），
   标签栏与工具栏占固定高度，日志窗格吃满剩余空间，
   滚动只发生在窗格内部而不是整页 */
#logs.runtime-fill {
  display: flex;
  flex-direction: column;
  height: 100vh;
  height: 100dvh;
  min-height: 0;
}

#logs.runtime-fill > .tabs {
  flex-shrink: 0;
}

#runtime-tab.tab-content.active {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

#runtime-tab.tab-content.active > .live-toolbar {
  flex-shrink: 0;
}

.live-wrap {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.live-pane {
  flex: 1;
  min-height: 360px;
  overflow: auto;
  padding: 10px 14px;
  background: #212529;
  border-radius: var(--radius-md);
  font-family: var(--font-mono);
  font-size: var(--font-size-xs);
  line-height: 1.8;
}

.live-pane .live-empty {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  text-align: center;
}

.live-empty-title {
  margin: 0;
  color: #adb5bd;
}

.live-empty-hint {
  margin: 0;
  font-size: var(--font-size-xs);
  color: var(--color-text-muted);
}

.live-pane .live-filter-empty {
  margin: 16px 0;
  color: #adb5bd;
  text-align: center;
}

.live-line {
  display: flex;
  align-items: baseline;
  gap: 10px;
  margin: 0;
  padding: 0 6px;
  border-radius: var(--radius-sm);
  color: #dee2e6;
}

.live-line:hover {
  background: rgb(255 255 255 / 4%);
}

.live-time {
  flex-shrink: 0;
  color: var(--color-text-muted);
}

.live-level-badge {
  flex-shrink: 0;
  align-self: center;
  padding: 1px 7px;
  border-radius: var(--radius-sm);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.5px;
  line-height: 1.5;
}

.badge-error {
  color: #ffa8a8;
  background: rgb(240 62 62 / 20%);
}

.badge-warn {
  color: #ffec99;
  background: rgb(250 176 5 / 22%);
}

.badge-info {
  color: #8ce99a;
  background: rgb(47 158 68 / 24%);
}

.badge-debug,
.badge-trace {
  color: #adb5bd;
  background: rgb(173 181 189 / 18%);
}

.live-target {
  flex-shrink: 0;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #909296;
}

.live-message {
  word-break: break-all;
  white-space: pre-wrap;
}

.live-line.live-warn .live-message {
  color: #ffec99;
}

.live-line.live-error .live-message {
  color: #ffc9c9;
}

.live-line.live-debug .live-message,
.live-line.live-trace .live-message {
  color: var(--color-text-muted);
}

/* 上翻查看历史时悬浮的「回到最新」按钮 */
.live-jump {
  position: absolute;
  right: 16px;
  bottom: 16px;
  padding: 6px 14px;
  border: 1px solid #495057;
  border-radius: 999px;
  background: #343a40;
  color: #dee2e6;
  font-size: var(--font-size-xs);
  cursor: pointer;
  box-shadow: var(--shadow-md);
  transition: var(--transition-fast);
}

.live-jump:hover {
  background: #495057;
}

@media (max-width: 768px) {
  .live-target {
    display: none;
  }

  .live-filter-input {
    width: 120px;
  }
}
</style>
