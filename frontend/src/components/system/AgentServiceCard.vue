<script setup lang="ts">
/**
 * Agent 配置卡片（移植自旧版 data-management 分区 agent-service 段 +
 * systemManager.js Agent 分发段）：上报接收服务参数（监听地址/上报与
 * 采集间隔/上报地址模式）+ Agent 设置（版本/注册令牌/配置包分发）。
 * 与会话设置共用 ["system-config"]（保存仅替换 agent 段并带回其余字段）。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiGet, apiPost, apiPut, request } from "../../api/client";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import ConfirmModal from "../ConfirmModal.vue";
import { downloadBlobResult } from "../../utils/download";
import type { EnrollmentTokenResponse } from "../../types/api/generated/EnrollmentTokenResponse";
import type { SystemInfo } from "../../types/api/generated/SystemInfo";

/** agent 段配置（GET /api/system/config 的 agent 段，后端未生成 ts-rs 类型） */
interface AgentConfigSection {
  enabled: boolean;
  /** 监听地址主机部分（IPv4/主机名/方括号 IPv6），端口见 port */
  bind_addr: string;
  port: number;
  report_interval_secs: number;
  collect_interval_secs: number;
  use_public_url: boolean;
  report_url_override: string | null;
  /** 以下三项保存时原样带回（仅在配置加载后才有值） */
  max_report_bytes?: number;
  history_retention_days?: number;
  offline_factor?: number;
}

/** 系统配置载荷（本卡片仅消费 agent 段） */
interface SystemConfigPayload {
  agent?: AgentConfigSection | null;
}

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

// ==================== 上报接收服务参数（agent 段） ====================

// 最近一次加载的 agent 段基础参数（与监听无关的字段）：PUT 整体替换，
// 保存时原样带回避免丢配置（未加载时为空对象，展开结果与 null 等价）
let agentBase: Pick<AgentConfigSection, "max_report_bytes" | "history_retention_days" | "offline_factor"> = {};

const agentEnabled = ref(false);
const agentBindHost = ref("[::]");
const agentBindPort = ref("9100");
const agentInterval = ref("60");
const agentCollectInterval = ref("5");
const agentUsePublicUrl = ref(false);
const agentReportOverride = ref("");

const configQuery = useQuery({
  queryKey: ["system-config"],
  queryFn: async () => {
    const result = await apiGet<SystemConfigPayload>("/api/system/config");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

watch(
  () => configQuery.data.value,
  (config) => {
    const agent = config?.agent;
    if (!agent) {
      return;
    }
    agentBase = {
      max_report_bytes: agent.max_report_bytes,
      history_retention_days: agent.history_retention_days,
      offline_factor: agent.offline_factor
    };
    agentEnabled.value = Boolean(agent.enabled);
    agentBindHost.value = agent.bind_addr || "[::]";
    agentBindPort.value = String(agent.port ?? 9100);
    agentInterval.value = String(agent.report_interval_secs ?? 60);
    agentCollectInterval.value = String(agent.collect_interval_secs ?? 5);
    agentUsePublicUrl.value = Boolean(agent.use_public_url);
    agentReportOverride.value = agent.report_url_override || "";
  }
);

const savingAgent = ref(false);

async function saveAgentConfig(): Promise<void> {
  if (savingAgent.value) {
    return;
  }
  // 地址为 IPv4/主机名或方括号 IPv6；端口 1-65535
  const bindHost = agentBindHost.value.trim();
  const bindPort = Number.parseInt(agentBindPort.value, 10);
  const bindHostValid = /^(?:\[[0-9a-fA-F:]+\]|[0-9A-Za-z.-]+)$/.test(bindHost);
  const bindPortValid = Number.isInteger(bindPort) && bindPort >= 1 && bindPort <= 65535;
  if (!bindHostValid || !bindPortValid) {
    showToast(t("agent_service.invalid_bind_addr"), "warning");
    return;
  }
  // 上报间隔 clamp 10-3600（与后端校验及 agent 端 clamp 对齐）
  const interval = Math.min(Math.max(Number.parseInt(agentInterval.value, 10) || 0, 10), 3600);
  // 采集间隔 clamp 1-60（与 agent 端 clamp 对齐）
  const collectInterval = Math.min(Math.max(Number.parseInt(agentCollectInterval.value, 10) || 0, 1), 60);
  // 上报地址模式：开关关时允许手动覆盖（host:port，留空回退监听地址推导）
  const reportOverride = agentReportOverride.value.trim();
  if (
    !agentUsePublicUrl.value &&
    reportOverride &&
    !/^(?:\[[0-9a-fA-F:]+\]|[0-9A-Za-z.-]+)(?::\d+)?$/.test(reportOverride)
  ) {
    showToast(t("agent_service.invalid_report_override"), "warning");
    return;
  }

  savingAgent.value = true;
  try {
    const agent: AgentConfigSection = {
      ...agentBase,
      enabled: agentEnabled.value,
      bind_addr: bindHost,
      port: bindPort,
      report_interval_secs: interval,
      collect_interval_secs: collectInterval,
      use_public_url: agentUsePublicUrl.value,
      report_url_override: reportOverride
    };
    const result = await apiPut<SystemConfigPayload, { agent: AgentConfigSection }>("/api/system/config", { agent });
    if (result.success) {
      showToast(t("agent_service.save_success"), "success");
      // 接收器随服务启动，监听地址/开关保存后需重启生效
      showToast(t("system.config_saved_restart_needed"), "warning");
      queryClient.invalidateQueries({ queryKey: ["system-config"] });
    } else {
      showToast(`${t("agent_service.save_failed")}: ${result.message}`, "error");
    }
  } finally {
    savingAgent.value = false;
  }
}

// ==================== Agent 分发（版本 / 注册令牌 / 配置包） ====================

// 操作系统 → 可用包格式（deb/rpm 为 Linux 系统包格式，zip 为 Windows 通用包）
const AGENT_OS_FORMATS: Record<string, string[]> = {
  linux: ["deb", "rpm"],
  windows: ["zip"]
};

const distOs = ref("linux");
const distFormat = ref("deb");

const formats = computed<string[]>(() => AGENT_OS_FORMATS[distOs.value] ?? []);

function onOsChange(): void {
  // 操作系统切换后重置格式下拉（Windows 仅 zip，单项时禁用切换）
  distFormat.value = formats.value[0] ?? "";
}

const tokenQuery = useQuery({
  queryKey: ["enrollment-token"],
  queryFn: async () => {
    const result = await request<EnrollmentTokenResponse>("/api/agents/enrollment-token");
    if (!result.success) {
      throw new Error(result.message || t("system.agent_token_load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const enrollmentToken = computed(() => tokenQuery.data.value?.enrollment_token || "");

// 版本与服务端一致，复用系统信息查询（SystemInfoTab 同键共享缓存）
const infoQuery = useQuery({
  queryKey: ["system-info"],
  queryFn: async () => {
    const result = await apiGet<SystemInfo>("/api/system/info");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const distDownloading = ref(false);

async function downloadConfigPackage(): Promise<void> {
  if (distDownloading.value || !distFormat.value) {
    return;
  }
  distDownloading.value = true;
  try {
    const format = distFormat.value;
    const result = await request(`/api/agents/config-package?format=${encodeURIComponent(format)}`);
    if (!result.success) {
      showToast(`${t("system.agent_config_download_failed")}: ${result.message || ""}`, "error");
      return;
    }
    // agent.toml 与证书由服务端现场组包，文件名优先取 Content-Disposition
    downloadBlobResult(result, `foims-agent-config.${format}`);
  } finally {
    distDownloading.value = false;
  }
}

// 重新生成注册令牌：旧令牌即刻失效，已部署 agent 需重新下载配置包覆盖安装
const showRegenerateConfirm = ref(false);
const regenerating = ref(false);

async function regenerateToken(): Promise<void> {
  showRegenerateConfirm.value = false;
  if (regenerating.value) {
    return;
  }
  regenerating.value = true;
  try {
    const result = await apiPost<EnrollmentTokenResponse>("/api/agents/enrollment-token", {});
    if (result.success && result.data?.enrollment_token) {
      showToast(t("system.agent_token_regenerated"), "success");
      queryClient.invalidateQueries({ queryKey: ["enrollment-token"] });
      return;
    }
    showToast(result.message || t("common.operation_failed"), "error");
  } finally {
    regenerating.value = false;
  }
}
</script>

<template>
  <section class="config-card">
    <header class="config-card-header">
      <div class="config-card-title">
        <span class="icon">📦</span>
        <span>{{ t("system.agent_config_card") }}</span>
      </div>
      <div v-if="!readonly" class="config-switch">
        <label for="agent-service-enabled">
          <input id="agent-service-enabled" v-model="agentEnabled" type="checkbox" aria-label="Enable agent report receiver">
          <span>{{ t("agent_service.enabled") }}</span>
        </label>
        <button type="button" class="btn btn-primary btn-sm" :disabled="savingAgent" @click="saveAgentConfig">
          {{ t("common.save") }}
        </button>
      </div>
    </header>
    <div class="data-management-container">
      <!-- 上报接收服务参数：监听地址作为默认上报地址来源 -->
      <section class="section">
        <h3>{{ t("agent_service.section_service") }}</h3>
        <div class="form-row">
          <div class="form-group">
            <label for="agent-service-bind-addr">{{ t("agent_service.bind_addr") }}</label>
            <input id="agent-service-bind-addr" v-model="agentBindHost" type="text" :placeholder="t('agent_service.bind_addr_placeholder')" autocomplete="off" aria-label="Agent listen address">
          </div>
          <div class="form-group">
            <label for="agent-service-bind-port">{{ t("agent_service.bind_port") }}</label>
            <input id="agent-service-bind-port" v-model="agentBindPort" type="number" min="1" max="65535" placeholder="9100" aria-label="Agent listen port">
          </div>
          <div class="form-group">
            <label for="agent-service-interval">{{ t("agent_service.report_interval") }}</label>
            <input id="agent-service-interval" v-model="agentInterval" type="number" min="10" max="3600" placeholder="60" aria-label="Agent report interval in seconds">
          </div>
          <div class="form-group">
            <label for="agent-service-collect-interval">{{ t("agent_service.collect_interval") }}</label>
            <input id="agent-service-collect-interval" v-model="agentCollectInterval" type="number" min="1" max="60" placeholder="5" aria-label="Agent collect interval in seconds">
          </div>
        </div>
      </section>
      <!-- Agent 客户端配置：上报地址模式 / 版本 / 分发下载 -->
      <section class="section">
        <h3>{{ t("agent_service.section_agent") }}</h3>
        <div class="form-row">
          <div class="form-group">
            <label class="checkbox-label" for="agent-use-public-url">
              <input id="agent-use-public-url" v-model="agentUsePublicUrl" type="checkbox" class="form-checkbox">
              <span>{{ t("agent_service.use_public_url") }}</span>
            </label>
          </div>
          <div v-show="!agentUsePublicUrl" class="form-group">
            <label for="agent-report-url-override">{{ t("agent_service.report_url_override") }}</label>
            <input id="agent-report-url-override" v-model="agentReportOverride" type="text" placeholder="agent.example.com:9100" autocomplete="off" aria-label="Agent report address override">
          </div>
        </div>
        <div class="form-row">
          <dl class="config-field">
            <dt>{{ t("system.agent_dist_version") }}</dt>
            <dd class="field-value">{{ infoQuery.data.value?.version || "-" }}</dd>
          </dl>
        </div>
        <div class="form-row">
          <dl class="config-field">
            <dt>{{ t("system.agent_enrollment_token") }}</dt>
            <!-- 加载失败显式提示（旧版为 error toast，此处渲染为字段文案避免查询重试风暴） -->
            <dd class="field-value">{{ tokenQuery.isError.value ? t("system.agent_token_load_failed") : enrollmentToken || t("system.agent_token_missing") }}</dd>
          </dl>
          <div v-if="!readonly" class="form-group">
            <button type="button" class="btn btn-secondary btn-sm" :disabled="regenerating" @click="showRegenerateConfirm = true">
              {{ t("system.agent_token_regenerate") }}
            </button>
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="agent-dist-os">{{ t("system.agent_os") }}</label>
            <select id="agent-dist-os" v-model="distOs" class="form-control" aria-label="Agent operating system" @change="onOsChange">
              <option value="linux">linux</option>
              <option value="windows">windows</option>
            </select>
          </div>
          <div class="form-group">
            <label for="agent-dist-format">{{ t("system.agent_format") }}</label>
            <select id="agent-dist-format" v-model="distFormat" class="form-control" :disabled="formats.length <= 1" aria-label="Agent package format">
              <option v-for="format in formats" :key="format" :value="format">{{ format.toUpperCase() }}</option>
            </select>
          </div>
          <div class="form-group">
            <button type="button" class="btn btn-primary" :disabled="!formats.length || distDownloading" @click="downloadConfigPackage">
              {{ t("system.agent_download_config") }}
            </button>
          </div>
        </div>
      </section>
    </div>

    <!-- 重新生成注册令牌确认（旧令牌即刻失效） -->
    <ConfirmModal
      v-if="showRegenerateConfirm"
      :message="t('system.agent_token_regenerate_confirm')"
      :title="t('system.agent_token_regenerate')"
      danger
      @confirm="regenerateToken"
      @cancel="showRegenerateConfirm = false"
    />
  </section>
</template>
