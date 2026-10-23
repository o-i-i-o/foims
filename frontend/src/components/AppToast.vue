<script setup lang="ts">
// 全局通知渲染组件：挂载于各入口根组件，展示 toastState 队列。
// 样式复用旧版 components/toasts.css 的 .toast 类族（@layer components），
// 进出场过渡由 TransitionGroup 的 toast-enter/toast-leave 类驱动。
import { toastState, dismissToast } from "../composables/useToast";
</script>

<template>
  <TransitionGroup tag="div" name="toast" class="toast-container" aria-live="polite">
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
  </TransitionGroup>
</template>
