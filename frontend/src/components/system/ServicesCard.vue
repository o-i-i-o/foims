<script setup lang="ts">
/**
 * 服务管理卡片（移植自旧版 systemManager.js 服务段）：foims / nginx 双行
 * 状态表（systemd 注册态/运行态/自启/运行时长），操作按钮按当前状态禁用
 * 无意义项。后端对 stop/restart 先响应后延迟执行：重启由前端轮询确认
 * 恢复（foims 恢复后整页刷新，nginx 仅刷新状态表），停止仅提示恢复方式。
 */
import { computed, onUnmounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { getSystemServices, runServiceOp, type ServiceStatusItem } from "../../api/system";
import { formatUptime } from "../../utils/formatter";
import { isSectionReadonly } from "../../utils/roleCapabilities";
import { showToast } from "../../composables/useToast";
import ConfirmModal from "../ConfirmModal.vue";

const props = withDefaults(defineProps<{ active?: boolean }>(), { active: false });

/** 服务操作类型（与后端路由段一致） */
type ServiceOp = "start" | "stop" | "restart" | "reload" | "enable" | "disable";

const { t } = useI18n();

const readonly = isSectionReadonly("system");

const servicesQuery = useQuery({
  queryKey: ["system-services"],
  queryFn: async () => {
    const result = await getSystemServices();
    if (!result.success) {
      throw new Error(result.message || t("services.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const services = computed(() => (Array.isArray(servicesQuery.data.value) ? servicesQuery.data.value : []));

// 状态徽章分级：未注册 / 运行中 / 失败 / 已停止，title 展示原始 state 与补充说明
function badgeMeta(item: ServiceStatusItem): { cls: string; text: string; title: string } {
  const baseTitle = `${item.active_state || "-"} / ${item.sub_state || "-"}`;
  const title = item.status_text ? `${baseTitle}\n${item.status_text}` : baseTitle;
  if (!item.registered) {
    return { cls: "svc-unregistered", text: t("services.status_unregistered"), title: t("services.unregistered_hint") };
  }
  if (item.active) {
    return { cls: "svc-active", text: t("services.status_active"), title };
  }
  if (item.active_state === "failed") {
    return { cls: "svc-failed", text: t("services.status_failed"), title };
  }
  return { cls: "svc-inactive", text: t("services.status_inactive"), title };
}

/** systemd 单元名（受管服务固定 foims / nginx 两个） */
function unitName(service: string): string {
  return service === "foims" ? "foims.service" : "nginx.service";
}

// ==================== 操作（stop/restart 需二次确认） ====================

const opInFlight = ref(false);
const confirmOp = ref<null | { service: string; op: ServiceOp }>(null);
let recoveryTimer: ReturnType<typeof setInterval> | null = null;

function requestOp(service: string, op: ServiceOp): void {
  if (opInFlight.value) {
    return;
  }
  if (op === "stop" || op === "restart") {
    confirmOp.value = { service, op };
    return;
  }
  void runOp(service, op);
}

function confirmMessage(): string {
  const target = confirmOp.value;
  if (!target) {
    return "";
  }
  const name = t(`services.name_${target.service}`);
  const unit = unitName(target.service);
  if (target.op === "stop") {
    const extraKey = target.service === "foims" ? "services.confirm_stop_foims_extra" : "services.confirm_stop_nginx_extra";
    return t("services.confirm_stop", { name, unit, extra: t(extraKey) });
  }
  return t("services.confirm_restart", { name, unit });
}

function onOpConfirmed(): void {
  const target = confirmOp.value;
  confirmOp.value = null;
  if (target) {
    void runOp(target.service, target.op);
  }
}

async function runOp(service: string, op: ServiceOp): Promise<void> {
  if (opInFlight.value) {
    return;
  }
  opInFlight.value = true;
  try {
    const result = await runServiceOp(service, op);
    if (!result.success) {
      showToast(result.message, "error");
      return;
    }
    if (op === "restart") {
      // 命令已送达、后台延迟执行：轮询等待服务恢复
      showToast(t("services.restart_sent"), "info");
      waitForServiceRecovery(service);
      return;
    }
    if (op === "stop") {
      // 服务即将停止，状态刷新必然失败：仅提示服务器侧恢复方式
      showToast(service === "foims" ? t("services.stop_done_foims") : t("services.stop_sent_nginx"), "warning");
      return;
    }
    showToast(t("services.op_done", { name: t(`services.name_${service}`), op: t(`services.op_${op}`) }), "success");
    servicesQuery.refetch();
  } finally {
    opInFlight.value = false;
  }
}

// 重启恢复轮询：重启窗口期连接失败属预期继续轮询；超时提示手动刷新
function waitForServiceRecovery(service: string): void {
  const deadline = Date.now() + 40000;
  recoveryTimer = setInterval(() => {
    if (Date.now() > deadline) {
      if (recoveryTimer !== null) {
        clearInterval(recoveryTimer);
      }
      recoveryTimer = null;
      showToast(t("services.restart_timeout"), "warning");
      return;
    }
    getSystemServices()
      .then((result) => {
        const target =
          result.success && Array.isArray(result.data)
            ? result.data.find((item) => item.name === service)
            : null;
        if (target?.active) {
          if (recoveryTimer !== null) {
            clearInterval(recoveryTimer);
          }
          recoveryTimer = null;
          showToast(t("services.restart_recovered"), "success");
          if (service === "foims") {
            // foims 恢复后整页刷新（重建会话状态）
            setTimeout(() => location.reload(), 800);
          } else {
            servicesQuery.refetch();
          }
        }
      })
      .catch(() => {
        // 重启窗口期后端不可达：保持轮询
      });
  }, 2000);
}

onUnmounted(() => {
  if (recoveryTimer) {
    clearInterval(recoveryTimer);
    recoveryTimer = null;
  }
});
</script>

<template>
  <section class="config-card">
    <header class="config-card-header">
      <div class="config-card-title">
        <span class="icon">⚙️</span>
        <span>{{ t("services.title") }}</span>
      </div>
      <div class="card-header-actions">
        <button type="button" class="btn btn-secondary btn-sm" @click="servicesQuery.refetch()">
          {{ t("common.refresh") }}
        </button>
      </div>
    </header>
    <div class="table-container cert-table-container">
      <table class="data-table" :aria-label="t('services.title')">
        <thead>
          <tr>
            <th>{{ t("services.col_service") }}</th>
            <th>{{ t("services.col_unit") }}</th>
            <th>{{ t("services.col_status") }}</th>
            <th>{{ t("services.col_enabled") }}</th>
            <th>{{ t("services.col_uptime") }}</th>
            <th>{{ t("common.actions") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="servicesQuery.isError.value" class="empty-row">
            <td colspan="6" class="text-center">{{ servicesQuery.error.value?.message || t("services.load_failed") }}</td>
          </tr>
          <tr v-else-if="services.length === 0" class="empty-row">
            <td colspan="6" class="text-center">{{ t("common.no_data") }}</td>
          </tr>
          <tr v-for="item in services" v-else :key="item.name">
            <td>{{ t(`services.name_${item.name}`) }}</td>
            <td>{{ item.unit }}</td>
            <td>
              <span class="svc-badge" :class="badgeMeta(item).cls" :title="badgeMeta(item).title">{{ badgeMeta(item).text }}</span>
            </td>
            <td>
              <span v-if="item.registered" class="svc-badge" :class="item.enabled ? 'svc-active' : 'svc-inactive'">
                {{ item.enabled ? t("services.enabled_on") : t("services.enabled_off") }}
              </span>
              <span v-else class="svc-unregistered-hint">{{ t("services.uptime_unavailable") }}</span>
            </td>
            <td>{{ item.uptime_seconds != null ? formatUptime(item.uptime_seconds) : t("services.uptime_unavailable") }}</td>
            <td class="svc-actions-cell">
              <span v-if="!item.registered" class="svc-unregistered-hint">{{ t("services.unregistered_hint") }}</span>
              <template v-else-if="!readonly">
                <button type="button" class="btn btn-primary btn-sm" :disabled="item.active" @click="requestOp(item.name, 'start')">
                  {{ t("services.op_start") }}
                </button>
                <button type="button" class="btn btn-danger btn-sm" :disabled="!item.active" @click="requestOp(item.name, 'stop')">
                  {{ t("services.op_stop") }}
                </button>
                <button type="button" class="btn btn-secondary btn-sm" @click="requestOp(item.name, 'restart')">
                  {{ t("services.op_restart") }}
                </button>
                <button v-if="item.reload_supported" type="button" class="btn btn-secondary btn-sm" :disabled="!item.active" @click="requestOp(item.name, 'reload')">
                  {{ t("services.op_reload") }}
                </button>
                <button type="button" class="btn btn-secondary btn-sm" :disabled="item.enabled" @click="requestOp(item.name, 'enable')">
                  {{ t("services.op_enable") }}
                </button>
                <button type="button" class="btn btn-secondary btn-sm" :disabled="!item.enabled" @click="requestOp(item.name, 'disable')">
                  {{ t("services.op_disable") }}
                </button>
              </template>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <ConfirmModal
      v-if="confirmOp"
      :message="confirmMessage()"
      :danger="confirmOp.op === 'stop'"
      @confirm="onOpConfirmed"
      @cancel="confirmOp = null"
    />
  </section>
</template>
