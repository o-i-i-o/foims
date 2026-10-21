<script setup lang="ts">
/**
 * 操作日志详情弹窗（移植自旧版 showLogDetails + modals/log/log-details-modal.html）。
 * log 对象由调用方直接传入（不再经 JSON 序列化挂 data 属性）。
 * 样式为 SFC 自包含（模态壳复用改密弹窗骨架 + 详情字段网格取自旧版 pages/logs.css）。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { formatDateTime, getOperationTypeText, getResourceTypeText } from "../utils/formatter";
import type { OperationLog } from "../types/api/generated/OperationLog";

const props = defineProps<{
  log: OperationLog;
}>();

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

const detailsText = computed<string | null>(() => {
  if (!props.log.details) {
    return null;
  }
  try {
    const details = typeof props.log.details === "string" ? JSON.parse(props.log.details) : props.log.details;
    return JSON.stringify(details, null, 2);
  } catch (error) {
    // details 非合法 JSON 时退回纯文本展示
    console.error("日志详情解析失败:", error);
    return String(props.log.details);
  }
});
</script>

<template>
  <div class="modal-overlay" @click.self="emit('close')">
    <div class="modal-dialog" role="dialog" aria-modal="true" :aria-label="t('logs.details')">
      <header class="modal-header">
        <h3>{{ t("logs.details") }}</h3>
        <button type="button" class="modal-close" :aria-label="t('common.close')" @click="emit('close')">&times;</button>
      </header>
      <div class="modal-body">
        <div class="log-detail-grid">
          <dl class="log-detail-item">
            <dt>{{ t("logs.operation_time") }}</dt>
            <dd>{{ formatDateTime(log.created_at) }}</dd>
          </dl>
          <dl class="log-detail-item">
            <dt>{{ t("logs.operator") }}</dt>
            <dd>{{ log.username || "-" }}</dd>
          </dl>
          <dl class="log-detail-item">
            <dt>{{ t("logs.action") }}</dt>
            <dd>{{ getOperationTypeText(log.action) }}</dd>
          </dl>
          <dl class="log-detail-item">
            <dt>{{ t("logs.result") }}</dt>
            <dd>
              <span class="status-badge" :class="log.success ? 'status-active' : 'status-inactive'">
                {{ log.success ? t("common.success") : t("common.failed") }}
              </span>
            </dd>
          </dl>
          <dl class="log-detail-item">
            <dt>{{ t("logs.resource_type") }}</dt>
            <dd>{{ getResourceTypeText(log.resource_type) || "-" }}</dd>
          </dl>
          <dl class="log-detail-item">
            <dt>{{ t("logs.resource_id") }}</dt>
            <dd>{{ log.resource_id ? String(log.resource_id) : "-" }}</dd>
          </dl>
          <dl class="log-detail-item">
            <dt>{{ t("logs.ip_address") }}</dt>
            <dd>{{ log.ip_address || "-" }}</dd>
          </dl>
        </div>
        <div class="log-detail-section">
          <h4 class="section-subtitle">{{ t("logs.details") }}</h4>
          <pre v-if="detailsText" class="log-details-json">{{ detailsText }}</pre>
          <p v-else class="text-muted">{{ t("logs.no_detail") }}</p>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgb(0 0 0 / 50%);
  padding: var(--space-md);
}

.modal-dialog {
  width: 100%;
  max-width: 560px;
  max-height: 85vh;
  overflow-y: auto;
  background: var(--color-bg-white);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: var(--space-md) var(--space-lg);
  border-bottom: 1px solid var(--color-border);
}

.modal-header h3 {
  margin: 0;
  font-size: 1.05rem;
  color: var(--color-text);
}

.modal-close {
  border: none;
  background: transparent;
  color: var(--color-text-muted);
  font-size: 1.5rem;
  line-height: 1;
  cursor: pointer;
}

.modal-close:hover {
  color: var(--color-danger);
}

.modal-body {
  padding: var(--space-lg);
}

/* ===== 日志详情弹窗字段网格（旧版 pages/logs.css 移植） ===== */
.log-detail-grid {
  display: grid;
  grid-template-columns: repeat(2, 1fr);
  gap: 10px 16px;
  margin-bottom: 16px;
}

dl.log-detail-item {
  margin: 0;
}

.log-detail-item dt {
  display: block;
  margin-bottom: 2px;
  font-weight: 500;
  font-size: 0.85rem;
  color: var(--color-text-muted);
}

.log-detail-item dd {
  margin: 0;
  font-size: 0.9rem;
  color: var(--color-text-light);
  word-break: break-all;
  line-height: 1.6;
}

.log-detail-section {
  margin-bottom: 12px;
}

.section-subtitle {
  margin: 0 0 8px;
  font-size: 0.95rem;
  color: var(--color-text);
}

.log-details-json {
  margin: 0;
  padding: 0.75rem;
  border: 1px solid var(--color-border-light);
  border-radius: var(--radius-md);
  background: var(--color-bg);
  font-size: 0.8rem;
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-all;
}

.text-muted {
  color: var(--color-text-muted);
}

@media (width <= 576px) {
  .log-detail-grid {
    grid-template-columns: 1fr;
  }
}
</style>
