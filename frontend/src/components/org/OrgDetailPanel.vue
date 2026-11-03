<script setup lang="ts">
/**
 * 组织详情右栏：展示选中组织节点的人员/房间，两视图经分段按钮切换（同时只
 * 显示一个），切换状态面板内聚。未选中节点时显示占位提示。
 */
import { ref } from "vue";
import { useI18n } from "vue-i18n";
import OrgEmployeeList from "./OrgEmployeeList.vue";
import OrgRoomList from "./OrgRoomList.vue";
import type { OrganizationTreeNode } from "../../types/api/generated/OrganizationTreeNode";

defineProps<{
  node: OrganizationTreeNode | null;
}>();

/** 当前视图：人员 / 房间 */
const tab = ref<"employees" | "rooms">("employees");

const { t } = useI18n();
</script>

<template>
  <aside class="org-detail-panel" :aria-label="t('organization.detail_title')">
    <template v-if="node">
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
