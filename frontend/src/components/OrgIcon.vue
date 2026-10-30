<script setup lang="ts">
/**
 * 组织节点图标渲染：
 * 已知 key 渲染带语义配色的 SVG（结构化形状，避免 v-html）；
 * 历史 emoji 值原样文本输出（文本插值自动转义）；空值用默认图标。
 */
import { computed } from "vue";
import { DEFAULT_ORG_ICON, ORG_ICONS, ORG_ICON_COLORS } from "../config/orgConfig";
import type { OrgIconShape } from "../config/orgConfig";

const props = withDefaults(
  defineProps<{
    icon?: string;
  }>(),
  { icon: "" }
);

// 已知 key → 对应形状；空值回退默认图标；未知值（历史 emoji）走文本分支
const shapes = computed<OrgIconShape[] | null>(() => {
  const known = props.icon ? ORG_ICONS[props.icon] : undefined;
  if (known) {
    return known;
  }
  // 默认图标键在配置中恒有定义（?? null 仅用于收窄索引访问类型）
  return props.icon ? null : (ORG_ICONS[DEFAULT_ORG_ICON] ?? null);
});

const color = computed(
  () => ORG_ICON_COLORS[props.icon] || ORG_ICON_COLORS[DEFAULT_ORG_ICON]
);
</script>

<template>
  <span v-if="shapes" class="org-icon" :style="{ color }" aria-hidden="true">
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <component :is="shape.tag" v-for="(shape, index) in shapes" :key="index" v-bind="shape.attrs" />
    </svg>
  </span>
  <!-- 历史 emoji 图标：非已知 key，原样文本渲染 -->
  <span v-else class="org-icon-legacy">{{ icon }}</span>
</template>

<style scoped>
.org-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 1;
}

.org-icon svg {
  width: 100%;
  height: 100%;
}

/* 历史 emoji 与 SVG 图标共用底衬，保持行内视觉尺寸一致 */
.org-icon-legacy {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 1;
  font-style: normal;
}
</style>
