<script setup lang="ts">
/**
 * 主机监控页：状态/关键字筛选 + 分页列表 + 30s 自动刷新（窗口失焦自动暂停，
 * TanStack refetchInterval 默认语义）+ 行内管理
 * 操作（详情/禁用/启用/删除）+ 告警阈值配置弹窗 + SNMP 观测事件/终端定位
 * 两个全局弹窗（均不限单台主机，与 SNMP 采集源同属本页职责）。
 *
 * 使用率列复用 AgentUsageBar；关键字输入为草稿态（回车/查询按钮提交），
 * 刷新与自动轮询沿用已提交筛选，不覆盖未确认的输入。
 */
import { computed, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiPatch } from "../api/client";
import { usePagedList } from "../composables/usePagedList";
import { formatTime } from "../utils/formatter";
import { showToast } from "../composables/useToast";
import { queryKeys } from "../utils/queryKeys";
import ConfirmModal from "../components/ConfirmModal.vue";
import NavIcon from "../components/NavIcon.vue";
import Pagination from "../components/Pagination.vue";
import AgentDetailModal from "../components/agents/AgentDetailModal.vue";
import AgentThresholdModal from "../components/agents/AgentThresholdModal.vue";
import AgentUsageBar from "../components/agents/AgentUsageBar.vue";
import SnmpEventsModal from "../components/device/SnmpEventsModal.vue";
import LocateModal from "../components/device/LocateModal.vue";
import { useLiveAgents } from "../composables/useLiveAgents";
import { useAuthStore } from "../stores/auth";
import type { LiveSummaryRow } from "../types/api/generated/LiveSummaryRow";

// 自动刷新周期与每页条数
const REFRESH_INTERVAL_MS = 30000;
const DEFAULT_PAGE_SIZE = 20;

const { t } = useI18n();
const queryClient = useQueryClient();
const auth = useAuthStore();

// 列表行类型：/api/agents 聚合 Agent 上报与 SNMP 采集源，字段超集未入
// generated（id 兼容数值与字符串、source/label 为联查可选键、用量与
// 温度可能缺报）
interface AgentListItem {
  id: number | string;
  /** SNMP 采集行关联的设备 ID（实时快照按此匹配；Agent 行为空） */
  device_id?: string | null;
  status: string;
  hostname: string | null;
  source?: string | null;
  label?: string | null;
  ip: string | null;
  os: string | null;
  arch: string | null;
  cpu_usage: number | string | null;
  mem_usage_pct: number | string | null;
  disk_usage_pct: number | string | null;
  max_temp: number | string | null;
  agent_version: string | null;
  last_seen: string | null;
}

// 状态筛选直接提交（下拉单选无中间态）；关键字为草稿态
const status = ref("");
const keywordDraft = ref("");
const keyword = ref("");

// 分页/越界回退/轮询统一由 usePagedList 承载；/api/agents 无排序参数，
// buildUrl 忽略排序入参（queryKey 仍含排序态，不影响正确性）
const list = usePagedList<AgentListItem>({
  tag: queryKeys.agents,
  defaultPageSize: DEFAULT_PAGE_SIZE,
  refetchInterval: REFRESH_INTERVAL_MS,
  extraKey: computed(() => ({ status: status.value, keyword: keyword.value })),
  buildUrl: (page, pageSize) => {
    const params = new URLSearchParams({ page: String(page), page_size: String(pageSize) });
    if (status.value) {
      params.set("status", status.value);
    }
    if (keyword.value) {
      params.set("keyword", keyword.value);
    }
    return `/api/agents?${params.toString()}`;
  }
});

/**
 * 已提交的查询签名（状态/关键字/页码）。用于判定 applyFilters 是否改变了 queryKey：
 * 改变时 TanStack 随 queryKey 自动重取，未变时（重复提交同一筛选）手动重拉，
 * 避免同一提交既触发 queryKey 变更又触发 refetch 的双拉。
 */
function querySignature(): string {
  return `${status.value}\u0000${keyword.value}\u0000${list.page}`;
}
let committedSignature = querySignature();

/** 提交关键字草稿并回到第 1 页；queryKey 未变时手动重拉，变化时交由自动重取 */
function applyFilters() {
  keyword.value = keywordDraft.value.trim();
  list.page = 1;
  const signature = querySignature();
  if (signature === committedSignature) {
    list.refetch();
  }
  committedSignature = signature;
}

// 状态元数据：状态点样式类 + 名称 i18n 键（active/offline 由上报自动翻转）
const STATUS_META: Record<string, { dot: string; labelKey: string }> = {
  active: { dot: "agent-dot-active", labelKey: "agents.status_active" },
  offline: { dot: "agent-dot-offline", labelKey: "agents.status_offline" },
  disabled: { dot: "agent-dot-disabled", labelKey: "agents.status_disabled" }
};

function statusMeta(agent: AgentListItem): { dot: string; labelKey: string } | undefined {
  return STATUS_META[agent.status];
}

function statusText(agent: AgentListItem): string {
  const meta = STATUS_META[agent.status];
  return meta ? t(meta.labelKey) : String(agent.status ?? "-");
}

/** 最高温度单元格：未上报显示 "-" */
function formatTemp(temp: number | string | null): string {
  if (temp == null || !Number.isFinite(Number(temp))) {
    return "-";
  }
  return `${Number(temp).toFixed(1)}℃`;
}

// SSE 推送的全部实时快照（source='snmp' 与 source='ipmi' 两路）：按
// `source|device_id` 复合键覆盖当前页对应采集行（同设备 SNMP/IPMI 各自快照
// 独立），列表结构（分页/状态/关键字）仍由 30s 轮询维护
const { devices: liveDevices, connected: liveConnected } = useLiveAgents(
  !auth.hiddenSections.includes("agents")
);
const liveBySourceDevice = computed(() => {
  const map = new Map<string, LiveSummaryRow>();
  for (const row of liveDevices.value) {
    map.set(`${row.source}|${row.device_id}`, row);
  }
  return map;
});

function liveRow(agent: AgentListItem): LiveSummaryRow | undefined {
  return agent.device_id ? liveBySourceDevice.value.get(`${agent.source}|${agent.device_id}`) : undefined;
}

// 实时值缺失（部分采集失败）时回退列表快照值，避免列闪空
function liveCpu(agent: AgentListItem): number | string | null {
  return liveRow(agent)?.cpu_usage_pct ?? agent.cpu_usage;
}

function liveMem(agent: AgentListItem): number | string | null {
  return liveRow(agent)?.mem_usage_pct ?? agent.mem_usage_pct;
}

function liveTempValue(agent: AgentListItem): number | string | null {
  return liveRow(agent)?.temperature_c ?? agent.max_temp;
}

/** 最近采集时间：有实时快照时取快照时间（秒级刷新），否则回退落库时间 */
function liveLastSeen(agent: AgentListItem): string | null {
  return liveRow(agent)?.collected_at ?? agent.last_seen;
}

// 行内操作语义：detail 打开详情弹窗，disable/enable 经 PATCH 切换状态
//（禁用后该机上报被 403 拒收，再启用即恢复），delete 需确认后 DELETE
const detailId = ref<number | string | null>(null);
const showThreshold = ref(false);
const deleteTarget = ref<AgentListItem | null>(null);

const statusMutation = useMutation({
  mutationFn: ({ id, next }: { id: number | string; next: string }) =>
    apiPatch(`/api/agents/${id}`, { status: next }),
  onSuccess: (result) => {
    if (result.success) {
      showToast(t("agents.status_updated"), "success");
      queryClient.invalidateQueries({ queryKey: queryKeys.agents });
      return;
    }
    showToast(result.message || t("common.operation_failed"), "error");
  }
});

const deleteMutation = useMutation({
  mutationFn: (id: number | string) => apiDelete(`/api/agents/${id}`),
  onSuccess: (result) => {
    if (result.success) {
      showToast(t("common.delete_success"), "success");
      queryClient.invalidateQueries({ queryKey: queryKeys.agents });
      return;
    }
    showToast(result.message || t("common.delete_failed"), "error");
  }
});

function handleDeleteConfirmed() {
  const id = deleteTarget.value?.id;
  deleteTarget.value = null;
  if (id != null) {
    deleteMutation.mutate(id);
  }
}

// SNMP 观测事件（不预填设备）与终端定位：均不依赖具体设备行，后端两接口
// 为库内只读查询，与 SNMP 采集源同属本页职责，故由本页顶栏承载
const eventsModal = reactive({ open: false });
const locateModal = reactive({ open: false });
</script>

<template>
  <section id="agents" class="content-section active" :aria-label="t('nav.agents')">
    <div class="agents-toolbar">
      <select v-model="status" class="form-control form-control-sm w-auto" :aria-label="t('agents.filter_status')" @change="applyFilters">
        <option value="">{{ t("common.all") }}</option>
        <option value="active">{{ t("agents.status_active") }}</option>
        <option value="offline">{{ t("agents.status_offline") }}</option>
        <option value="disabled">{{ t("agents.status_disabled") }}</option>
      </select>
      <input
        v-model="keywordDraft"
        type="text"
        class="form-control form-control-sm w-auto"
        :placeholder="t('agents.search_placeholder')"
        :aria-label="t('agents.search_placeholder')"
        @keydown.enter="applyFilters"
      >
      <button type="button" class="btn btn-primary btn-sm" @click="applyFilters">{{ t("agents.query") }}</button>
      <button type="button" class="btn btn-secondary btn-sm" @click="list.refetch()">{{ t("common.refresh") }}</button>
      <button type="button" class="btn btn-secondary btn-sm" @click="showThreshold = true">{{ t("agents.alert_threshold") }}</button>
      <button type="button" class="btn btn-secondary btn-sm" @click="eventsModal.open = true">{{ t("device.snmp_events.title") }}</button>
      <button type="button" class="btn btn-secondary btn-sm" @click="locateModal.open = true">{{ t("device.locate.title") }}</button>
      <span class="agent-live-state" :class="liveConnected ? 'is-on' : 'is-off'">
        <span class="agent-live-dot" aria-hidden="true"></span>
        {{ liveConnected ? t("agents.live_connected") : t("agents.live_disconnected") }}
      </span>
    </div>
    <div class="table-container">
      <table id="agents-table" class="data-table" :aria-label="t('nav.agents')">
        <thead>
          <tr>
            <th class="col-center">{{ t("common.status") }}</th>
            <th>{{ t("agents.col_hostname") }}</th>
            <th>{{ t("agents.col_ip") }}</th>
            <th>{{ t("agents.col_os") }}</th>
            <th class="col-center">{{ t("agents.col_arch") }}</th>
            <th>{{ t("agents.col_cpu") }}</th>
            <th>{{ t("agents.col_memory") }}</th>
            <th>{{ t("agents.col_disk") }}</th>
            <th class="col-center">{{ t("agents.col_temp") }}</th>
            <th>{{ t("agents.col_version") }}</th>
            <th>{{ t("agents.col_last_seen") }}</th>
            <th class="col-center">{{ t("common.actions") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="list.isError" class="empty-row">
            <td colspan="12" class="text-center">{{ list.errorMessage }}</td>
          </tr>
          <tr v-else-if="list.items.length === 0" class="empty-row">
            <td colspan="12" class="text-center">{{ t("agents.no_data") }}</td>
          </tr>
          <tr v-for="agent in list.items" v-else :key="agent.id">
            <td class="col-center">
              <span class="agent-status-wrap">
                <span class="agent-status-dot" :class="statusMeta(agent)?.dot" aria-hidden="true"></span>{{ statusText(agent) }}
              </span>
            </td>
            <td>
              {{ agent.hostname || "-" }}<span
                v-if="agent.source === 'snmp' || agent.source === 'ipmi'"
                class="agent-source-badge"
              >{{ agent.source === "ipmi" ? t("agents.source_ipmi") : t("agents.source_snmp") }}</span><span
                v-if="liveRow(agent)"
                class="agent-live-badge"
                :title="t('agents.live_realtime')"
              >{{ t("agents.live_realtime") }}</span><span v-if="agent.label" class="agent-host-label">{{ agent.label }}</span>
            </td>
            <td>{{ agent.ip || "-" }}</td>
            <td>{{ agent.os || "-" }}</td>
            <td class="col-center">{{ agent.arch || "-" }}</td>
            <td><AgentUsageBar :value="liveCpu(agent)" /></td>
            <td><AgentUsageBar :value="liveMem(agent)" /></td>
            <td><AgentUsageBar :value="agent.disk_usage_pct" /></td>
            <td class="col-center">{{ formatTemp(liveTempValue(agent)) }}</td>
            <td>{{ agent.agent_version || "-" }}</td>
            <td>{{ liveLastSeen(agent) ? formatTime(liveLastSeen(agent)) : "-" }}</td>
            <td class="col-center">
              <button
                type="button"
                class="icon-btn btn-info"
                :data-tooltip="t('agents.btn_detail')"
                :aria-label="t('agents.btn_detail')"
                @click="detailId = agent.id"
              >
                <NavIcon name="eye" />
              </button>
              <!-- SNMP/IPMI 采集行不经上报鉴权，禁用/启用仅对 Agent 行有意义 -->
              <button
                v-if="agent.source !== 'snmp' && agent.source !== 'ipmi' && agent.status === 'active'"
                type="button"
                class="icon-btn btn-warning"
                :data-tooltip="t('agents.btn_disable')"
                :aria-label="t('agents.btn_disable')"
                @click="statusMutation.mutate({ id: agent.id, next: 'disabled' })"
              >
                <NavIcon name="lock" />
              </button>
              <button
                v-if="agent.source !== 'snmp' && agent.source !== 'ipmi' && agent.status === 'disabled'"
                type="button"
                class="icon-btn btn-success"
                :data-tooltip="t('agents.btn_enable')"
                :aria-label="t('agents.btn_enable')"
                @click="statusMutation.mutate({ id: agent.id, next: 'active' })"
              >
                <NavIcon name="check" />
              </button>
              <button
                type="button"
                class="icon-btn btn-danger"
                :data-tooltip="t('common.delete')"
                :aria-label="t('common.delete')"
                @click="deleteTarget = agent"
              >
                <NavIcon name="trash" />
              </button>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <Pagination
      v-if="list.totalPages > 1"
      :page="list.page"
      :total-pages="list.totalPages"
      :total="list.total"
      @page-change="(p) => (list.page = p)"
      @hover-page="list.prefetch"
    />

    <AgentDetailModal v-if="detailId != null" :agent-id="detailId" @close="detailId = null" />
    <AgentThresholdModal v-if="showThreshold" @close="showThreshold = false" />
    <SnmpEventsModal v-if="eventsModal.open" @close="eventsModal.open = false" />
    <LocateModal v-if="locateModal.open" @close="locateModal.open = false" />
    <ConfirmModal
      v-if="deleteTarget"
      :message="t('agents.confirm_delete')"
      :title="t('common.delete')"
      :danger="true"
      @confirm="handleDeleteConfirmed"
      @cancel="deleteTarget = null"
    />
  </section>
</template>
