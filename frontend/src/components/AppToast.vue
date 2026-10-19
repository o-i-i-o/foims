<script setup>
// 全局通知渲染组件：挂载于各入口根组件，展示 toastState 队列。
// 样式复用旧版 components/toasts.css 的 .toast 类族（@layer components）。
import { toastState, dismissToast } from "../composables/useToast.js";
</script>

<template>
  <div class="toast-container" aria-live="polite">
    <div
      v-for="toast in toastState.toasts"
      :key="toast.id"
      class="toast"
      :class="`toast-${toast.type}`"
      role="status"
      @click="dismissToast(toast.id)"
    >
      {{ toast.message }}
    </div>
  </div>
</template>
