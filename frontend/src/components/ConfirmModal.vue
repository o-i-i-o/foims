<script setup lang="ts">
/**
 * 通用确认弹窗（移植自旧版 utils/confirm.js + modals/common/confirm-modal.html）。
 * v-if 控制挂载，确认/取消经事件通知调用方；Esc 与点击遮罩视为取消。
 * 样式为 SFC 自包含（shell 未迁移 modals.css，避免引入整包组件样式）。
 */
import { onBeforeUnmount } from "vue";
import { useI18n } from "vue-i18n";
import { useModalStack } from "../composables/useModalStack";

withDefaults(
  defineProps<{
    message: string;
    title?: string;
    danger?: boolean;
  }>(),
  { title: "", danger: false }
);

const emit = defineEmits<{ confirm: []; cancel: [] }>();

const { t } = useI18n();

// Esc 仅取消最上层弹窗（嵌套在业务弹窗内时不应连带关闭父弹窗）
onBeforeUnmount(useModalStack(() => emit("cancel")));
</script>

<template>
  <div class="modal-overlay" @click.self="emit('cancel')">
    <div class="modal-dialog" role="dialog" aria-modal="true" :aria-label="title || t('common.confirm')">
      <header class="modal-header">
        <h3 class="modal-title">{{ title || t("common.confirm") }}</h3>
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
/* 外壳/页头/页脚样式见全局 styles/components/modals.css；此处仅保留
   确认框专属宽度与正文文案样式（按钮走全局 buttons.css） */
.modal-dialog {
  max-width: 380px;
}

.modal-body p {
  margin: 0;
  color: var(--color-text);
}
</style>
