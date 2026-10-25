<script setup lang="ts">
/**
 * 通用弹窗外壳（视觉样式见全局 styles/components/modals.css）：遮罩 + 深色页头（标题竖条 +
 * 关闭按钮）+ body/footer 插槽。Esc 与点击遮罩视为关闭；size 为宽度
 * 档位（sm 480 / md 600 / lg 900 / xl 90vw，min() 保证窄视口不溢出）。
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
        <h3 class="modal-title">
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
