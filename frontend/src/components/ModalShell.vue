<script setup lang="ts">
/**
 * 通用弹窗外壳（视觉样式见全局 styles/components/modals.css）：遮罩 + 深色页头（标题竖条 +
 * 关闭按钮）+ body/footer 插槽。Esc 与点击遮罩视为关闭；size 为宽度
 * 档位（sm 480 / md 600 / lg 900 / xl 90vw，min() 保证窄视口不溢出）。
 * embedded 为内联模式：供汇聚弹窗（如设备 SNMP 标签弹窗）把某个子弹窗
 * 作为标签页内容内嵌渲染——此时不套遮罩/页头，也不注册 Esc 栈（层级由外层
 * 弹窗统一承载），仅渲染 body/footer 插槽为普通内联块。
 */
import { onBeforeUnmount } from "vue";
import { useI18n } from "vue-i18n";
import { useModalStack } from "../composables/useModalStack";

const props = withDefaults(
  defineProps<{
    /** 标题文案；提供 #title 插槽时忽略（用于标题含富内容的弹窗） */
    title?: string;
    size?: string;
    /** 内联模式：无遮罩/页头/Esc 栈，仅内联渲染内容（供标签页嵌入） */
    embedded?: boolean;
  }>(),
  { title: "", size: "", embedded: false }
);

const emit = defineEmits<{ close: [] }>();

const { t } = useI18n();

// Esc 仅关闭最上层弹窗（嵌套子弹窗场景）；调用即注册，卸载时注销。
// 内联模式不弹出独立层，跳过注册避免抢占栈顶、误关内联内容。
if (!props.embedded) {
  onBeforeUnmount(useModalStack(() => emit("close")));
}
</script>

<template>
  <!-- 内联模式：仅渲染 body/footer 为普通内联块，供汇聚弹窗作为标签页内容嵌入 -->
  <div v-if="embedded" class="modal modal-embedded">
    <div class="modal-embedded-body">
      <slot />
    </div>
    <footer v-if="$slots.footer" class="modal-footer">
      <slot name="footer" />
    </footer>
  </div>
  <div v-else class="modal-overlay" @click.self="emit('close')">
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
