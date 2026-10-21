<script setup lang="ts">
/**
 * 主机监控页（移植自旧版 main.html agents 分区 + modules/agents.js）：
 * 状态/关键字筛选 + 分页列表 + 30s 自动刷新（窗口失焦自动暂停，TanStack
 * refetchInterval 默认语义，与旧版 document.hidden 跳过一致）+ 行内管理
 * 操作（详情/禁用/启用/删除）+ 告警阈值配置弹窗。
 *
 * 使用率列复用 AgentUsageBar；关键字输入为草稿态（回车/查询按钮提交），
 * 刷新与自动轮询沿用已提交筛选，不覆盖未确认的输入。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiGet, request } from "../api/client";
import { formatTime } from "../utils/formatter";
import { showToast } from "../composables/useToast";
import type { Paged } from "../types/api";
import ConfirmModal from "../components/ConfirmModal.vue";
import NavIcon from "../components/NavIcon.vue";
import Pagination from "../components/Pagination.vue";
import AgentDetailModal from "../components/agents/AgentDetailModal.vue";
import AgentThresholdModal from "../components/agents/AgentThresholdModal.vue";
import AgentUsageBar from "../components/agents/AgentUsageBar.vue";

// 自动刷新周期与每页条数（与旧版一致）
const REFRESH_INTERVAL_MS = 30000;
const PAGE_SIZE = 20;

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 列表查询（筛选 + 分页 + 轮询） ====================

// 列表行类型：/api/agents 聚合 Agent 上报与 SNMP 采集源，字段超集未入
// generated（id 兼容数值与字符串、source/label 为联查可选键、用量与
// 温度可能缺报）
interface AgentListItem {
  id: number | string;
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
const page = ref(1);

const listQuery = useQuery({
  queryKey: computed(() => ["agents", { page: page.value, status: status.value, keyword: keyword.value }]),
  queryFn: async (): Promise<Paged<AgentListItem> | null> => {
    const params = new URLSearchParams({ page: String(page.value), page_size: String(PAGE_SIZE) });
    if (status.value) {
      params.set("status", status.value);
    }
    if (keyword.value) {
      params.set("keyword", keyword.value);
    }
    const result = await apiGet<Paged<AgentListItem>>(`/api/agents?${params.toString()}`);
    if (!result.success) {
      // apiClient 已将失败归一为 { success:false } 不抛异常，
      // 此处显式抛出让查询进入 error 态（isError/error.message 可渲染）
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  // 30s 轮询沿用当前页与筛选；窗口失焦时 TanStack 默认暂停（同旧版）
  refetchInterval: REFRESH_INTERVAL_MS
});

const items = computed(() => (Array.isArray(listQuery.data.value?.items) ? listQuery.data.value.items : []));
const total = computed(() => listQuery.data.value?.total ?? 0);
const totalPages = computed(() => listQuery.data.value?.total_pages ?? 1);

// 空页且页码越界（末页数据删除/筛选变化后）自动回退最后一页
watch(
  () => listQuery.data.value,
  (data) => {
    if (!data || !Array.isArray(data.items) || data.items.length !== 0 || page.value <= 1) {
      return;
    }
    const pages = data.total_pages || Math.ceil((data.total || 0) / PAGE_SIZE);
    if (pages > 0 && page.value > pages) {
      page.value = pages;
    }
  }
);

/** 提交关键字草稿并回到第 1 页；筛选值未变化时 queryKey 不变，手动重拉 */
function applyFilters() {
  keyword.value = keywordDraft.value.trim();
  page.value = 1;
  listQuery.refetch();
}

// ==================== 列表单元格渲染辅助 ====================

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

// ==================== 行内管理操作 ====================

// 行内操作语义：detail 打开详情弹窗，disable/enable 经 PATCH 切换状态
//（禁用后该机上报被 403 拒收，再启用即恢复），delete 需确认后 DELETE
const detailId = ref<number | string | null>(null);
const showThreshold = ref(false);
const deleteTarget = ref<AgentListItem | null>(null);

const statusMutation = useMutation({
  mutationFn: ({ id, next }: { id: number | string; next: string }) =>
    request(`/api/agents/${id}`, { method: "PATCH", body: JSON.stringify({ status: next }) }),
  onSuccess: (result) => {
    if (result.success) {
      showToast(t("agents.status_updated"), "success");
      queryClient.invalidateQueries({ queryKey: ["agents"] });
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
      queryClient.invalidateQueries({ queryKey: ["agents"] });
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
      <button type="button" class="btn btn-secondary btn-sm" @click="listQuery.refetch()">{{ t("common.refresh") }}</button>
      <button type="button" class="btn btn-secondary btn-sm" @click="showThreshold = true">{{ t("agents.alert_threshold") }}</button>
    </div>
    <div class="table-container">
      <table id="agents-table" class="data-table" aria-label="Agent hosts">
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
          <tr v-if="listQuery.isError.value" class="empty-row">
            <td colspan="12" class="text-center">{{ listQuery.error.value?.message || t("common.load_failed") }}</td>
          </tr>
          <tr v-else-if="items.length === 0" class="empty-row">
            <td colspan="12" class="text-center">{{ t("agents.no_data") }}</td>
          </tr>
          <tr v-for="agent in items" v-else :key="agent.id">
            <td class="col-center">
              <span class="agent-status-wrap">
                <span class="agent-status-dot" :class="statusMeta(agent)?.dot" aria-hidden="true"></span>{{ statusText(agent) }}
              </span>
            </td>
            <td>
              {{ agent.hostname || "-" }}<span
                v-if="agent.source === 'snmp'"
                class="agent-source-badge"
              >{{ t("agents.source_snmp") }}</span><span v-if="agent.label" class="agent-host-label">{{ agent.label }}</span>
            </td>
            <td>{{ agent.ip || "-" }}</td>
            <td>{{ agent.os || "-" }}</td>
            <td class="col-center">{{ agent.arch || "-" }}</td>
            <td><AgentUsageBar :value="agent.cpu_usage" /></td>
            <td><AgentUsageBar :value="agent.mem_usage_pct" /></td>
            <td><AgentUsageBar :value="agent.disk_usage_pct" /></td>
            <td class="col-center">{{ formatTemp(agent.max_temp) }}</td>
            <td>{{ agent.agent_version || "-" }}</td>
            <td>{{ agent.last_seen ? formatTime(agent.last_seen) : "-" }}</td>
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
              <!-- SNMP 采集行不经上报鉴权，禁用/启用仅对 Agent 行有意义（旧版语义） -->
              <button
                v-if="agent.source !== 'snmp' && agent.status === 'active'"
                type="button"
                class="icon-btn btn-warning"
                :data-tooltip="t('agents.btn_disable')"
                :aria-label="t('agents.btn_disable')"
                @click="statusMutation.mutate({ id: agent.id, next: 'disabled' })"
              >
                <NavIcon name="lock" />
              </button>
              <button
                v-if="agent.source !== 'snmp' && agent.status === 'disabled'"
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
      v-if="totalPages > 1"
      :page="page"
      :total-pages="totalPages"
      :total="total"
      @page-change="(p) => (page = p)"
    />

    <AgentDetailModal v-if="detailId != null" :agent-id="detailId" @close="detailId = null" />
    <AgentThresholdModal v-if="showThreshold" @close="showThreshold = false" />
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
