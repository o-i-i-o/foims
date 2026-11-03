<script setup lang="ts">
/**
 * Agent 配置卡片：上报接收服务参数（监听地址/上报地址模式）+
 * Agent 设置（注册令牌/配置包分发）。采集与上报节奏由 agent 独立维护，
 * 不在本卡片配置。与会话设置共用 ["system-config"]
 * （保存仅替换 agent 段并带回其余字段）。
 */
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery, useQueryClient } from "@tanstack/vue-query";
import { downloadAgentConfigPackage, getEnrollmentToken, regenerateEnrollmentToken } from "../../api/agents";
import { getSystemConfig, updateSystemConfig } from "../../api/system";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import ConfirmModal from "../ConfirmModal.vue";
import SystemSectionCard from "./SystemSectionCard.vue";
import { downloadBlobResult } from "../../utils/download";

/** agent 段配置（GET /api/system/config 的 agent 段，后端未生成 ts-rs 类型） */
interface AgentConfigSection {
  enabled: boolean;
  /** 监听地址主机部分（IPv4/主机名/方括号 IPv6），端口见 port */
  bind_addr: string;
  port: number;
  use_public_url: boolean;
  report_url_override: string | null;
  /** 以下四项保存时原样带回（仅在配置加载后才有值） */
  max_report_bytes?: number;
  history_retention_days?: number;
  snmp_event_retention_days?: number;
  offline_factor?: number;
}

/** GET/PUT /api/system/config 局部载荷（本卡片仅带 agent 段） */
interface AgentConfigPayload {
  agent?: AgentConfigSection | null;
}

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

const { t } = useI18n();
const queryClient = useQueryClient();

const readonly = isSectionReadonly("system");

// 最近一次加载的 agent 段基础参数（与监听无关的字段）：PUT 整体替换，
// 保存时原样带回避免丢配置（未加载时为空对象，展开结果与 null 等价）
let agentBase: Pick<
  AgentConfigSection,
  "max_report_bytes" | "history_retention_days" | "snmp_event_retention_days" | "offline_factor"
> = {};

const agentEnabled = ref(false);
const agentBindHost = ref("[::]");
const agentBindPort = ref("9100");
const agentUsePublicUrl = ref(false);
const agentReportOverride = ref("");

const configQuery = useQuery({
  queryKey: ["system-config"],
  queryFn: async () => {
    const result = await getSystemConfig<AgentConfigPayload>();
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
      snmp_event_retention_days: agent.snmp_event_retention_days,
      offline_factor: agent.offline_factor
    };
    agentEnabled.value = Boolean(agent.enabled);
    agentBindHost.value = agent.bind_addr || "[::]";
    agentBindPort.value = String(agent.port ?? 9100);
    agentUsePublicUrl.value = Boolean(agent.use_public_url);
    agentReportOverride.value = agent.report_url_override || "";
  }
);

const savingAgent = ref(false);

async function saveAgentConfig(): Promise<void> {
  if (savingAgent.value) {
    return;
  }
  const bindHost = agentBindHost.value.trim();
  const bindPort = Number.parseInt(agentBindPort.value, 10);
  // 地址与端口分开校验、分别提示（先地址后端口，命中即提示并终止）
  if (!/^(?:\[[0-9a-fA-F:]+\]|[0-9A-Za-z.-]+)$/.test(bindHost)) {
    showToast(t("agent_service.invalid_bind_addr", { host: bindHost }), "warning");
    return;
  }
  // 端口仅非特权 1025-65534（服务以普通用户运行无特权端口绑定能力，
  // 与后端 Config::load / 保存校验同口径）
  if (!Number.isInteger(bindPort) || bindPort < 1025 || bindPort > 65534) {
    // v-model 对 type=number 输入自动 looseToNumber：合法输入为 number、
    // 非法残留可能是 string，统一 String() 后再 trim 防止对 number 调字符串方法
    showToast(t("agent_service.invalid_port", { port: String(agentBindPort.value).trim() }), "warning");
    return;
  }
  // 上报地址模式：开关关时手动指定主机（端口由 Agent 监听端口决定，不在
  // 覆盖地址中填写），留空回退监听地址推导
  const reportOverride = agentReportOverride.value.trim();
  if (
    !agentUsePublicUrl.value &&
    reportOverride &&
    !/^(?:\[[0-9a-fA-F:]+\]|[0-9A-Za-z.-]+)$/.test(reportOverride)
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
      use_public_url: agentUsePublicUrl.value,
      report_url_override: reportOverride
    };
    const result = await updateSystemConfig({ agent });
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
    const result = await getEnrollmentToken();
    if (!result.success) {
      throw new Error(result.message || t("system.agent_token_load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const enrollmentToken = computed(() => tokenQuery.data.value?.enrollment_token || "");

const distDownloading = ref(false);

async function downloadConfigPackage(): Promise<void> {
  if (distDownloading.value || !distFormat.value) {
    return;
  }
  distDownloading.value = true;
  try {
    const format = distFormat.value;
    const result = await downloadAgentConfigPackage(format);
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
    const result = await regenerateEnrollmentToken();
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
  <SystemSectionCard icon="agents" :title="t('system.agent_config_card')">
    <template #actions>
      <div v-if="!readonly" class="config-switch">
        <label for="agent-service-enabled">
          <input id="agent-service-enabled" v-model="agentEnabled" type="checkbox" :aria-label="t('agent_service.enabled')">
          <span>{{ t("agent_service.enabled") }}</span>
        </label>
        <button type="button" class="btn btn-primary btn-sm" :disabled="savingAgent" @click="saveAgentConfig">
          {{ t("common.save") }}
        </button>
      </div>
    </template>

    <!-- 上报接收服务参数：监听地址作为默认上报地址来源（采集/上报节奏
         由 agent 独立维护，不在服务端配置） -->
    <div class="section-title-row">
      <h3>{{ t("agent_service.section_service") }}</h3>
    </div>
    <div class="form-row form-row-2col">
      <div class="form-group">
        <label for="agent-service-bind-addr">{{ t("agent_service.bind_addr") }}</label>
        <input id="agent-service-bind-addr" v-model="agentBindHost" type="text" :placeholder="t('agent_service.bind_addr_placeholder')" autocomplete="off" :aria-label="t('agent_service.bind_addr')">
      </div>
      <div class="form-group">
        <label for="agent-service-bind-port">{{ t("agent_service.bind_port") }}</label>
        <input id="agent-service-bind-port" v-model="agentBindPort" type="number" min="1025" max="65534" placeholder="9100" :aria-label="t('agent_service.bind_port')">
      </div>
    </div>

    <!-- Agent 客户端配置：上报地址模式 / 注册令牌 / 分发下载 -->
    <div class="section-title-row">
      <h3>{{ t("agent_service.section_agent") }}</h3>
    </div>
    <div class="form-row form-row-2col">
      <div class="form-group">
        <label class="checkbox-label" for="agent-use-public-url">
          <input id="agent-use-public-url" v-model="agentUsePublicUrl" type="checkbox" class="form-checkbox">
          <span>{{ t("agent_service.use_public_url") }}</span>
        </label>
      </div>
      <div v-show="!agentUsePublicUrl" class="form-group">
        <label for="agent-report-url-override">{{ t("agent_service.report_url_override") }}</label>
        <input id="agent-report-url-override" v-model="agentReportOverride" type="text" placeholder="agent.example.com" autocomplete="off" :aria-label="t('agent_service.report_url_override')">
      </div>
    </div>
    <!-- 注册令牌：字段撑满、重新生成动作右置（field-action-row 通用模式） -->
    <div class="field-action-row">
      <dl class="config-field">
        <dt>{{ t("system.agent_enrollment_token") }}</dt>
        <!-- 加载失败显式提示（渲染为字段文案，避免查询重试风暴） -->
        <dd class="field-value">{{ tokenQuery.isError.value ? t("system.agent_token_load_failed") : enrollmentToken || t("system.agent_token_missing") }}</dd>
      </dl>
      <button v-if="!readonly" type="button" class="btn btn-secondary btn-sm" :disabled="regenerating" @click="showRegenerateConfirm = true">
        {{ t("system.agent_token_regenerate") }}
      </button>
    </div>
    <div class="form-row">
      <div class="form-group">
        <label for="agent-dist-os">{{ t("system.agent_os") }}</label>
        <select id="agent-dist-os" v-model="distOs" class="form-control" :aria-label="t('system.agent_os')" @change="onOsChange">
          <option value="linux">linux</option>
          <option value="windows">windows</option>
        </select>
      </div>
      <div class="form-group">
        <label for="agent-dist-format">{{ t("system.agent_format") }}</label>
        <select id="agent-dist-format" v-model="distFormat" class="form-control" :disabled="formats.length <= 1" :aria-label="t('system.agent_format')">
          <option v-for="format in formats" :key="format" :value="format">{{ format.toUpperCase() }}</option>
        </select>
      </div>
      <div class="form-group">
        <button type="button" class="btn btn-primary" :disabled="!formats.length || distDownloading" @click="downloadConfigPackage">
          {{ t("system.agent_download_config") }}
        </button>
      </div>
    </div>
  </SystemSectionCard>

  <!-- 重新生成注册令牌确认（旧令牌即刻失效） -->
  <ConfirmModal
    v-if="showRegenerateConfirm"
    :message="t('system.agent_token_regenerate_confirm')"
    :title="t('system.agent_token_regenerate')"
    danger
    @confirm="regenerateToken"
    @cancel="showRegenerateConfirm = false"
  />
</template>
