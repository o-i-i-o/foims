<script setup lang="ts">
/**
 * 组织详情右栏：展示选中组织节点的人员/房间，两视图经分段按钮切换（同时只
 * 显示一个），切换状态面板内聚。未选中节点时显示占位提示。
 */
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { getOrgTypeLabel } from "../../config/orgConfig";
import OrgIcon from "../OrgIcon.vue";
import OrgEmployeeList from "./OrgEmployeeList.vue";
import OrgRoomList from "./OrgRoomList.vue";
import type { OrganizationTreeNode } from "../../types/api/generated/OrganizationTreeNode";

const props = defineProps<{
  node: OrganizationTreeNode | null;
  /** 节点图标（页面按模板优先级解析后传入） */
  icon: string;
}>();

/** 当前视图：人员 / 房间 */
const tab = ref<"employees" | "rooms">("employees");

const { t } = useI18n();

const typeLabel = computed(() => (props.node ? getOrgTypeLabel(props.node.org_type) : ""));
</script>

<template>
  <aside class="org-detail-panel" :aria-label="t('organization.detail_title')">
    <template v-if="node">
      <div class="org-detail-header">
        <span class="org-detail-header-icon"><OrgIcon :icon="icon" /></span>
        <div class="org-detail-header-info">
          <div class="org-detail-header-name">
            <span class="org-detail-header-title">{{ node.name }}</span>
            <span class="org-node-type-badge" :data-type="node.org_type">{{ typeLabel }}</span>
          </div>
          <div v-if="node.description" class="org-detail-header-desc" :title="node.description">
            {{ node.description }}
          </div>
        </div>
      </div>

      <div class="org-detail-tabs" role="tablist" :aria-label="t('organization.detail_title')">
        <button
          type="button"
          role="tab"
          class="org-detail-tab"
          :class="{ active: tab === 'employees' }"
          :aria-selected="tab === 'employees'"
          @click="tab = 'employees'"
        >
          {{ t("organization.tab_employees") }}
        </button>
        <button
          type="button"
          role="tab"
          class="org-detail-tab"
          :class="{ active: tab === 'rooms' }"
          :aria-selected="tab === 'rooms'"
          @click="tab = 'rooms'"
        >
          {{ t("organization.tab_rooms") }}
        </button>
      </div>

      <OrgEmployeeList v-if="tab === 'employees'" :org-id="node.id" />
      <OrgRoomList v-else :org-id="node.id" />
    </template>
    <div v-else class="org-empty-state">{{ t("organization.detail_empty") }}</div>
  </aside>
</template>
