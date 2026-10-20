<script setup>
/**
 * 层级模板类型节点（递归组件，移植自旧版 createTypeNode + 树形编辑器）：
 * 图标选择（按分组的 SVG 图标面板，同屏仅一个面板打开）、类型输入
 * （实时徽标预览）、添加子层级/删除。节点数据为响应式对象，增删经注入的
 * orgTplEditorCtx 操作；根节点展示根徽标且不可删除。
 */
import { computed, inject } from "vue";
import { useI18n } from "vue-i18n";
import { DEFAULT_ORG_ICON, getOrgTypeLabel, ORG_ICON_GROUPS } from "../../config/orgConfig.js";
import NavIcon from "../NavIcon.vue";
import OrgIcon from "../OrgIcon.vue";

const props = defineProps({
  node: { type: Object, required: true },
  isRoot: { type: Boolean, default: false }
});

const { t } = useI18n();

const ctx = inject("orgTplEditorCtx");

const pickerOpen = computed(() => ctx.activePicker.value === props.node.uid);

function togglePicker() {
  ctx.activePicker.value = pickerOpen.value ? null : props.node.uid;
}

function selectIcon(key) {
  props.node.icon = key;
  ctx.activePicker.value = null;
}
</script>

<template>
  <div class="org-template-type-node" :class="{ 'org-template-type-root': isRoot }">
    <div class="org-template-type-row">
      <span v-if="isRoot" class="org-template-root-badge">{{ t("org_template.root") }}</span>

      <!-- 图标选择按钮：面板锚点由 position:relative 提供；stop 防止 document 级关闭监听立即收起 -->
      <button type="button" class="org-template-icon-btn" :title="t('org_template.select_icon')" @click.stop="togglePicker">
        <OrgIcon :icon="node.icon || DEFAULT_ORG_ICON" />
        <div v-if="pickerOpen" class="org-icon-picker" @click.stop>
          <div v-for="group in ORG_ICON_GROUPS" :key="group.groupKey" class="org-icon-picker-group">
            <div class="org-icon-picker-group-title">{{ t(group.groupKey) }}</div>
            <div class="org-icon-picker-grid">
              <button
                v-for="item in group.icons"
                :key="item.key"
                type="button"
                class="org-icon-picker-item"
                :class="{ active: (node.icon || DEFAULT_ORG_ICON) === item.key }"
                :title="t(item.labelKey)"
                @click="selectIcon(item.key)"
              >
                <OrgIcon :icon="item.key" />
              </button>
            </div>
          </div>
        </div>
      </button>

      <input
        v-model="node.type"
        type="text"
        class="form-control org-template-type-input"
        :placeholder="t('org_template.type_placeholder')"
        maxlength="50"
      >

      <!-- 类型徽标实时预览（输入变化即更新） -->
      <span v-if="node.type" class="org-node-type-badge" :data-type="node.type">{{ getOrgTypeLabel(node.type) }}</span>

      <button type="button" class="icon-btn btn-add-child" :title="t('org_template.add_child_type')" :aria-label="t('org_template.add_child_type')" @click="ctx.addChild(node)">
        <NavIcon name="plus" />
      </button>
      <button v-if="!isRoot" type="button" class="icon-btn btn-delete" :title="t('common.delete')" :aria-label="t('common.delete')" @click="ctx.removeNode(node)">
        <NavIcon name="x" />
      </button>
    </div>

    <div class="org-template-type-children">
      <TemplateTypeNode v-for="child in node.children" :key="child.uid" :node="child" :is-root="false" />
    </div>
  </div>
</template>
