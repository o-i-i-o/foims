<script setup>
/**
 * 组织树节点（递归组件，移植自旧版 modules/organization.js renderTreeNode
 * 与树交互逻辑）：展开/折叠（展开/收起全部按钮经全局展开态联动）、搜索
 * 过滤（子树命中才可见，纯祖先命中强制展开）、行内操作按钮（悬停显形）。
 * 操作经注入的 orgTreeCtx.onAction 上抛页面处理；图标经注入的
 * orgTreeCtx.resolveIcon 按所属模板优先解析。
 */
import { computed, inject, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { getOrgTypeLabel } from "../../config/orgConfig.js";
import NavIcon from "../NavIcon.vue";
import OrgIcon from "../OrgIcon.vue";

const props = defineProps({
  node: { type: Object, required: true }
});

const { t } = useI18n();

const ctx = inject("orgTreeCtx");

const children = computed(() => props.node.children ?? []);
const hasChildren = computed(() => children.value.length > 0);
const typeLabel = computed(() => getOrgTypeLabel(props.node.org_type));
const nodeIcon = computed(() => ctx.resolveIcon(props.node.org_type, props.node.template_id));

// 展开状态：初始跟随全局展开态（新渲染的节点与旧版 allExpanded 初始渲染一致），
// 全局展开/收起时同步所有节点；单节点点击独立切换
const selfExpanded = ref(ctx.allExpanded.value);
watch(ctx.allExpanded, (value) => {
  selfExpanded.value = value;
});

// 搜索态（keyword 为 null 表示未搜索）：visible 由页面按命中子树计算；
// 纯祖先命中（自身不匹配但后代匹配）强制展开
const visible = computed(
  () => !ctx.search.value || ctx.search.value.visible.has(props.node.id)
);
const forceExpanded = computed(() => ctx.search.value?.expand.has(props.node.id) ?? false);
const effectiveExpanded = computed(() => forceExpanded.value || selfExpanded.value);

function onToggle() {
  selfExpanded.value = !selfExpanded.value;
}

function onToggleKeydown(event) {
  // 键盘可达性：假按钮（span[role=button]）Enter/Space 等效点击
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    onToggle();
  }
}

function dispatch(action) {
  ctx.onAction(props.node, action);
}
</script>

<template>
  <div v-show="visible" class="org-node-wrapper">
    <div class="org-node">
      <span
        v-if="hasChildren"
        class="org-toggle"
        :class="{ expanded: effectiveExpanded }"
        role="button"
        tabindex="0"
        :aria-expanded="String(effectiveExpanded)"
        :aria-label="node.name"
        @click="onToggle"
        @keydown="onToggleKeydown"
      >
        <svg class="org-toggle-icon" width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
          <path d="M6 4l4 4-4 4" fill="none" stroke="currentColor" stroke-width="2" />
        </svg>
      </span>
      <span v-else class="org-toggle-placeholder"></span>

      <span class="org-node-icon"><OrgIcon :icon="nodeIcon" /></span>
      <span class="org-node-name">{{ node.name }}</span>
      <span class="org-node-type-badge" :data-type="node.org_type">{{ typeLabel }}</span>
      <span v-if="node.description" class="org-node-desc" :title="node.description">{{ node.description }}</span>

      <span class="org-node-actions">
        <button type="button" class="icon-btn btn-add-child" :title="t('organization.add_child')" :aria-label="t('organization.add_child')" @click="dispatch('add-child')">
          <NavIcon name="plusCircle" />
        </button>
        <button type="button" class="icon-btn btn-edit" :title="t('common.edit')" :aria-label="t('common.edit')" @click="dispatch('edit')">
          <NavIcon name="edit" />
        </button>
        <button type="button" class="icon-btn btn-employees" :title="t('organization.manage_employees')" :aria-label="t('organization.manage_employees')" @click="dispatch('employees')">
          <NavIcon name="users" />
        </button>
        <button type="button" class="icon-btn btn-delete" :title="t('common.delete')" :aria-label="t('common.delete')" @click="dispatch('delete')">
          <NavIcon name="trash" />
        </button>
      </span>
    </div>

    <!-- 缩进由 .org-children 嵌套 padding-left 逐层累加，无需按深度计算 -->
    <div v-if="hasChildren" v-show="effectiveExpanded" class="org-children">
      <OrgTreeNode v-for="child in children" :key="child.id" :node="child" />
    </div>
  </div>
</template>
