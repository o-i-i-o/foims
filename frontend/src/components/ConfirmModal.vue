<script setup>
/**
 * 通用确认弹窗（移植自旧版 utils/confirm.js + modals/common/confirm-modal.html）。
 * v-if 控制挂载，确认/取消经事件通知调用方；Esc 与点击遮罩视为取消。
 * 样式为 SFC 自包含（shell 未迁移 modals.css，避免引入整包组件样式）。
 */
import { onBeforeUnmount, onMounted } from "vue";
import { useI18n } from "vue-i18n";

const props = defineProps({
  message: { type: String, required: true },
  title: { type: String, default: "" },
  danger: { type: Boolean, default: false }
});

const emit = defineEmits(["confirm", "cancel"]);

const { t } = useI18n();

function onKeydown(event) {
  if (event.key === "Escape") {
    emit("cancel");
  }
}

onMounted(() => {
  document.addEventListener("keydown", onKeydown);
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <div class="modal-overlay" @click.self="emit('cancel')">
    <div class="modal-dialog" role="dialog" aria-modal="true" :aria-label="title || t('common.confirm')">
      <header class="modal-header">
        <h3>{{ title || t("common.confirm") }}</h3>
        <button type="button" class="modal-close" :aria-label="t('common.close')" @click="emit('cancel')">&times;</button>
      </header>
      <div class="modal-body">
        <p>{{ message }}</p>
      </div>
      <footer class="modal-footer">
        <button type="button" class="btn-secondary" @click="emit('cancel')">
          {{ t("common.cancel") }}
        </button>
        <button type="button" class="btn-primary" :class="{ 'btn-danger': danger }" @click="emit('confirm')">
          {{ t("common.confirm") }}
        </button>
      </footer>
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
  max-width: 380px;
  background: var(--color-bg-white);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
  overflow: hidden;
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

.modal-body p {
  margin: 0;
  color: var(--color-text);
}

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-sm);
  padding: 0 var(--space-lg) var(--space-lg);
}

.btn-primary,
.btn-secondary {
  padding: 0.5rem 1rem;
  border-radius: var(--radius-md);
  border: 1px solid transparent;
  font-size: var(--font-size-sm);
  cursor: pointer;
  transition:
    background-color var(--transition-fast),
    border-color var(--transition-fast),
    color var(--transition-fast);
}

.btn-primary {
  background: var(--color-primary);
  color: var(--color-bg-white);
}

.btn-primary:hover {
  background: var(--color-primary-hover);
}

.btn-primary.btn-danger {
  background: var(--color-danger);
}

.btn-secondary {
  background: var(--color-bg-white);
  border-color: var(--control-border);
  color: var(--color-text-secondary);
}

.btn-secondary:hover {
  border-color: var(--control-border-hover);
  color: var(--color-text);
}
</style>
