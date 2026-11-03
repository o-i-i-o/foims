<script setup lang="ts">
/**
 * 系统页统一分组卡片：真实根节点（section）自持结构与组间距
 * （.config-card + .config-card，见 system.css），替代旧"模板片段 +
 * 父级兄弟选择器撑间距"的上下文耦合。组头 = 图标 + 标题 + 右侧动作位
 * （#actions 具名槽，可空）；内容经默认槽直落 config-card-body。
 * 二级分组在卡内使用 .section-title-row（同一渲染路径，仅此两级标题）。
 */
import NavIcon from "../NavIcon.vue";

defineProps<{
  /** NavIcon 图标名（组头圆片） */
  icon: string;
  /** 组头标题（i18n 文案由调用方传入） */
  title: string;
}>();
</script>

<template>
  <section class="config-card">
    <header class="config-card-header">
      <div class="config-card-title">
        <span class="icon"><NavIcon :name="icon" /></span>
        <span>{{ title }}</span>
      </div>
      <slot name="actions" />
    </header>
    <div class="config-card-body">
      <slot />
    </div>
  </section>
</template>
