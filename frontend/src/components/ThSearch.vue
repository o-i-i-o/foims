<script setup lang="ts">
/**
 * 表头搜索弹层（列头图标按钮 + 弹出搜索面板的通用交互）：
 * 多根组件，须直接置于 th.th-searchable 内（与列名 span 并列，样式与
 * absolute 定位均由 tables.css 的 .data-table th 后代选择器提供）。
 * 放大镜按钮开合弹层；同一时刻全页仅展开一个（模块级登记当前展开实例）；
 * 点击按钮与弹层之外收起；Esc 关闭；输入非空时按钮保持高亮。
 * 输入值经 v-model 上抛，由调用方防抖后过滤。
 */
import { onBeforeUnmount, ref } from "vue";
import { useI18n } from "vue-i18n";

const props = withDefaults(
  defineProps<{
    modelValue?: string;
    /** 输入框无障碍标签（按列语义传入，如 "Search name"） */
    ariaLabel?: string;
  }>(),
  { modelValue: "", ariaLabel: "" }
);

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

const { t } = useI18n();

const toggleEl = ref<HTMLElement | null>(null);
const popoverEl = ref<HTMLElement | null>(null);
const inputEl = ref<HTMLInputElement | null>(null);
const open = ref(false);

// 全页同一时刻只展开一个弹层：登记当前展开实例的关闭函数
let openCloser: (() => void) | null = null;

function close() {
  if (!open.value) {
    return;
  }
  open.value = false;
  if (openCloser === close) {
    openCloser = null;
  }
}

function toggle() {
  if (open.value) {
    close();
    return;
  }
  // 关闭其它已展开实例后展开自身
  openCloser?.();
  open.value = true;
  openCloser = close;
  inputEl.value?.focus();
}

function onInput(event: Event) {
  const target = event.target;
  if (target instanceof HTMLInputElement) {
    emit("update:modelValue", target.value);
  }
}

function onDocumentClick(event: Event) {
  const target = event.target;
  if (
    open.value &&
    target instanceof Node &&
    !toggleEl.value?.contains(target) &&
    !popoverEl.value?.contains(target)
  ) {
    close();
  }
}

document.addEventListener("click", onDocumentClick);
onBeforeUnmount(() => {
  document.removeEventListener("click", onDocumentClick);
  // 卸载时若仍处于展开态，释放全局登记
  if (openCloser === close) {
    openCloser = null;
  }
});
</script>

<template>
  <button
    ref="toggleEl"
    type="button"
    class="th-search-toggle"
    :class="{ active: props.modelValue.trim() !== '' }"
    :aria-label="t('common.search')"
    @click.stop="toggle"
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="8" /><line x1="21" y1="21" x2="16.65" y2="16.65" /></svg>
  </button>
  <div ref="popoverEl" class="th-search-popover" :class="{ open }">
    <input
      ref="inputEl"
      :value="props.modelValue"
      type="text"
      class="form-control form-control-sm"
      :placeholder="t('common.search_placeholder')"
      :aria-label="props.ariaLabel || t('common.search_placeholder')"
      @input="onInput"
      @keydown.esc.stop="close"
    >
  </div>
</template>
