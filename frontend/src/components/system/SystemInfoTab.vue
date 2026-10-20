<script setup>
/**
 * 系统信息页签（移植自旧版 system-info-tab + systemManager.loadSystemInfo）：
 * 系统信息只读字段（名称/数据库状态/系统时间/运行时长）+ 商业许可占位卡
 * （注册/购买入口暂未开放，不绑定任何真实连接，与旧版一致）+ 关于卡
 * （版本/作者/联系方式/开源组件清单弹窗）。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../../api/client.js";
import { formatDateTime, formatUptime } from "../../utils/formatter.js";
import OpenSourceModal from "./OpenSourceModal.vue";

const props = defineProps({ active: { type: Boolean, default: false } });

const { t } = useI18n();

const infoQuery = useQuery({
  queryKey: ["system-info"],
  queryFn: async () => {
    const result = await apiGet("/api/system/info");
    if (!result.success) {
      // apiClient 已将失败归一为 { success:false } 不抛异常，
      // 此处显式抛出让查询进入 error 态
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

const info = computed(() => infoQuery.data.value || {});

const databaseStatusText = computed(() => {
  const status = info.value.database_status || "-";
  if (status === "connected") {
    return t("system.connected");
  }
  if (String(status).startsWith("disconnected")) {
    return t("system.disconnected");
  }
  return status;
});

// 系统时间为即时快照：后端未携带时回退本机时间
const systemTimeText = computed(() =>
  formatDateTime(info.value.timestamp || new Date().toISOString())
);

const uptimeText = computed(() => formatUptime(info.value.uptime_seconds || 0));

const showOpenSource = ref(false);
</script>

<template>
  <div class="tab-content" :class="{ active: props.active }">
    <div class="system-config-section">
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🖥️</span>
            <span>{{ t("system.info") }}</span>
          </div>
        </header>
        <!-- 只读字段 dl 化：dl/dt/dd 语义化键值对（dd 默认边距已在 CSS 重置） -->
        <div class="form-row">
          <dl class="config-field">
            <dt>{{ t("system.system_name") }}</dt>
            <dd class="field-value">{{ info.name || "FOIMS" }}</dd>
          </dl>
          <dl class="config-field">
            <dt>{{ t("system.database_status") }}</dt>
            <dd class="field-value">{{ databaseStatusText }}</dd>
          </dl>
          <dl class="config-field">
            <dt>{{ t("system.system_time") }}</dt>
            <dd class="field-value">{{ systemTimeText }}</dd>
          </dl>
          <dl class="config-field">
            <dt>{{ t("system.uptime") }}</dt>
            <dd class="field-value">{{ uptimeText }}</dd>
          </dl>
        </div>
      </section>

      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">🧾</span>
            <span>{{ t("system.license") }}</span>
          </div>
        </header>
        <!-- 占位按钮：注册/购买入口暂未开放，不绑定任何真实连接 -->
        <div class="license-actions">
          <button type="button" class="btn btn-primary btn-sm">{{ t("system.license_register") }}</button>
          <button type="button" class="btn btn-secondary btn-sm">{{ t("system.license_purchase") }}</button>
        </div>
      </section>

      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon">ℹ️</span>
            <span>{{ t("system.about") }}</span>
          </div>
        </header>
        <div class="form-row">
          <dl class="config-field">
            <dt>{{ t("system.version") }}</dt>
            <dd class="field-value">{{ info.version || "-" }}</dd>
          </dl>
          <dl class="config-field">
            <dt>{{ t("system.author") }}</dt>
            <dd class="field-value">oi-io</dd>
          </dl>
          <dl class="config-field">
            <dt>{{ t("system.contact") }}</dt>
            <dd class="field-value">boss@oi-io.cc</dd>
          </dl>
        </div>
        <!-- 原模拟链接按钮化：.about-link 已补按钮 reset，视觉不变 -->
        <button type="button" class="about-link" @click="showOpenSource = true">
          {{ t("system.open_source_components") }}
        </button>
      </section>
    </div>

    <OpenSourceModal v-if="showOpenSource" @close="showOpenSource = false" />
  </div>
</template>
