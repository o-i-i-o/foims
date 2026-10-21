<script setup lang="ts">
/**
 * 通用弹窗外壳（移植自旧版 utils/modalLoader.js buildModalShell +
 * components/modals.css 外壳）：遮罩 + 标题栏（含关闭按钮）+ body/footer
 * 插槽。Esc 与点击遮罩视为关闭；size 为宽度档位（与旧版 modal-content
 * 档位一致：默认 640 / sm 480 / md 600 / lg 900 / xl 90vw，min() 保证窄视口不溢出）。
 */
import { onBeforeUnmount } from "vue";
import { useI18n } from "vue-i18n";
import { useModalStack } from "../composables/useModalStack";

withDefaults(
  defineProps<{
    /** 标题文案；提供 #title 插槽时忽略（用于标题含富内容的弹窗） */
    title?: string;
    size?: string;
  }>(),
  { title: "", size: "" }
);

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

// Esc 仅关闭最上层弹窗（嵌套子弹窗场景）；调用即注册，卸载时注销
onBeforeUnmount(useModalStack(() => emit("close")));
</script>

<template>
  <div class="modal-overlay" @click.self="emit('close')">
    <div
      class="modal modal-dialog"
      :class="size ? `modal-${size}` : ''"
      role="dialog"
      aria-modal="true"
      :aria-label="title || t('common.confirm')"
    >
      <header class="modal-header">
        <h3>
          <slot name="title">{{ title }}</slot>
        </h3>
        <button type="button" class="modal-close" :aria-label="t('common.close')" @click="emit('close')">&times;</button>
      </header>
      <div class="modal-body">
        <slot />
      </div>
      <footer v-if="$slots.footer" class="modal-footer">
        <slot name="footer" />
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
  padding: var(--space-md);
  background: rgb(0 0 0 / 60%);
  backdrop-filter: blur(4px);
}

.modal-dialog {
  width: 100%;
  max-width: min(640px, calc(100vw - 2rem));
  max-height: 85vh;
  overflow-y: auto;
  background: var(--color-bg-white);
  border-radius: var(--radius-xl);
  box-shadow:
    0 20px 60px rgb(0 0 0 / 30%),
    0 0 0 1px rgb(0 0 0 / 5%);
}

.modal-dialog.modal-sm {
  max-width: min(480px, calc(100vw - 2rem));
}

.modal-dialog.modal-md {
  max-width: min(600px, calc(100vw - 2rem));
}

.modal-dialog.modal-lg {
  max-width: min(900px, calc(100vw - 2rem));
}

/* xl 档：统一端口管理等宽弹窗（旧版 modal-lg-custom 90vw） */
.modal-dialog.modal-xl {
  max-width: 90vw;
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

.modal-footer {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-sm);
  padding: 0 var(--space-lg) var(--space-lg);
}
</style>
