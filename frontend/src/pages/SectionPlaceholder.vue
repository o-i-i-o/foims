<script setup>
/**
 * 未迁移分区占位页：一级分区从原生 JS 向 Vue 迁移的过渡展示。
 * 根元素沿用 content-section active 类，保持与布局层 CSS（分页贴底、
 * 页面留白等）的既有约定；分区标题追加只读徽标（auditor/user 等）。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute } from "vue-router";
import { useAuthStore } from "../stores/auth.js";

const route = useRoute();
const { t } = useI18n();
const auth = useAuthStore();

const title = computed(() => t(route.meta.title ?? "nav.dashboard"));
const isReadonly = computed(() => auth.readOnlySections.includes(String(route.name)));
</script>

<template>
  <section class="content-section active section-placeholder">
    <h2 class="placeholder-title">
      {{ title }}
      <span v-if="isReadonly" class="ro-badge">{{ t("common.readonly_badge") }}</span>
    </h2>
    <div class="placeholder-card">
      <p>{{ t("common.section_migrating") }}</p>
    </div>
  </section>
</template>

<style scoped>
.section-placeholder {
  padding: var(--space-md) var(--space-lg);
}

.placeholder-title {
  margin-bottom: var(--space-md);
  font-size: 1.25rem;
  color: var(--color-text);
}

.ro-badge {
  display: inline-block;
  margin-left: var(--space-sm);
  padding: 0 0.5rem;
  border: 1px solid var(--color-warning);
  border-radius: var(--radius-sm);
  color: var(--color-warning);
  font-size: var(--font-size-sm);
  font-weight: 400;
  vertical-align: middle;
}

.placeholder-card {
  padding: var(--space-xl);
  background: var(--color-bg-white);
  border: 1px dashed var(--color-border);
  border-radius: var(--radius-lg);
  color: var(--color-text-muted);
}
</style>
