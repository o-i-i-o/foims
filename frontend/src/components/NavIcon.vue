<script setup>
/**
 * 线性图标渲染组件：按形状描述数组绘制 SVG。
 * 外观属性集中在根 svg 上（stroke=currentColor、stroke-width=2、圆角线帽），
 * 具体形状由 config/icons.js 提供（{tag, ...attrs} 平面描述，tag 用于选择
 * 元素、其余字段作为 SVG 属性透传），尺寸由外层容器（.nav-icon 等）控制。
 */
import { computed } from "vue";
import { ICONS } from "../config/icons.js";

const props = defineProps({
  name: { type: String, required: true }
});

const shapes = computed(() =>
  (ICONS[props.name] ?? []).map(({ tag, ...attrs }) => ({ tag, attrs }))
);
</script>

<template>
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <component :is="shape.tag" v-for="(shape, index) in shapes" :key="index" v-bind="shape.attrs" />
  </svg>
</template>
