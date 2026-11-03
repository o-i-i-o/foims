<script setup lang="ts">
/**
 * 系统信息页签：
 * 系统信息只读字段（名称/数据库状态/系统时间）+ 商业许可占位卡
 * （注册/购买入口暂未开放，不绑定任何真实连接）+ 关于卡
 * （版本/作者/联系方式/开源组件清单弹窗）。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import NavIcon from "../NavIcon.vue";
import { useQuery } from "@tanstack/vue-query";
import { getSystemInfo } from "../../api/system";
import { formatDateTime } from "../../utils/formatter";
import { useNowTick } from "../../composables/useNowTick";
import type { SystemInfo } from "../../types/api/generated/SystemInfo";
import OpenSourceModal from "./OpenSourceModal.vue";

const props = withDefaults(
  defineProps<{
    /** 页签是否激活（激活时才发起查询） */
    active?: boolean;
  }>(),
  { active: false }
);

const { t } = useI18n();

const infoQuery = useQuery({
  queryKey: ["system-info"],
  queryFn: async () => {
    const result = await getSystemInfo();
    if (!result.success) {
      // apiClient 已将失败归一为 { success:false } 不抛异常，
      // 此处显式抛出让查询进入 error 态
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  },
  enabled: computed(() => props.active)
});

// 局部字段缺省时模板回退展示占位符
const info = computed<Partial<SystemInfo>>(() => infoQuery.data.value ?? {});

// 数据库状态文案：connected 绿色，disconnected* 红色（区分无权限/异常），其余原样
const databaseStatusText = computed<string>(() => {
  const status = info.value.database_status || "-";
  if (status === "connected") {
    return t("system.connected");
  }
  if (String(status).startsWith("disconnected")) {
    return t("system.disconnected");
  }
  return status;
});

// 系统时间前端推进：以服务端基准时刻（timestamp）加本地流逝毫秒推算服务端当前时刻，
// 两端偏移在取数瞬间一次性折算（基准 epoch − 本地取数 epoch），逐秒推进不重复累积；
// 后端未携带基准（取数失败/加载中）时回退本机时钟，同样逐秒推进
const nowTick = useNowTick();
const systemTimeText = computed(() => {
  const baseMs = info.value.timestamp ? Date.parse(info.value.timestamp) : NaN;
  const fetchedAt = infoQuery.dataUpdatedAt.value;
  const nowMs = nowTick.value;
  if (Number.isNaN(baseMs) || !fetchedAt) {
    return formatDateTime(new Date(nowMs).toISOString());
  }
  return formatDateTime(new Date(baseMs + (nowMs - fetchedAt)).toISOString());
});

const showOpenSource = ref(false);
</script>

<template>
  <div class="tab-content" :class="{ active: props.active }">
    <div class="system-config-section">
      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon"><NavIcon name="agents" /></span>
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
        </div>
      </section>

      <section class="config-card">
        <header class="config-card-header">
          <div class="config-card-title">
            <span class="icon"><NavIcon name="list" /></span>
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
            <span class="icon"><NavIcon name="system" /></span>
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
