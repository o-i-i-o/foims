<script setup lang="ts">
/**
 * 组织详情面板-房间列表（内联，只读）：列出该节点及其全部下级组织的房间，
 * 带直属组织与工位/机柜计数。房间是「组织 → 房间 → 工位/机柜/设备」链上的
 * 空间归属点，房间的增删改统一在资源页完成，此处只做归属关系的查看入口。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../../api/client";
import { getRoomTypeName } from "../../utils/formatter";
import type { RoomSummary } from "../../types/api/generated/RoomSummary";

const props = defineProps<{
  orgId: string;
}>();

const { t } = useI18n();

const roomsQuery = useQuery({
  queryKey: computed(() => ["org-rooms", props.orgId]),
  queryFn: async (): Promise<RoomSummary[]> => {
    const result = await apiGet<RoomSummary[]>(
      `/api/resources/organizations/${encodeURIComponent(props.orgId)}/rooms`
    );
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    // 接口返回普通数组（非分页），非数组响应按空列表兜底
    return Array.isArray(result.data) ? result.data : [];
  },
  enabled: computed(() => Boolean(props.orgId))
});

const rooms = computed(() =>
  roomsQuery.isError.value ? [] : (roomsQuery.data.value ?? [])
);
</script>

<template>
  <div class="org-detail-list">
    <div class="org-detail-list-body">
      <div v-if="roomsQuery.isError.value" class="org-empty-state">
        {{ roomsQuery.error.value?.message }}
      </div>
      <div v-else-if="rooms.length === 0" class="org-empty-state">
        {{ t("common.no_data") }}
      </div>
      <template v-else>
        <div v-for="room in rooms" :key="room.id" class="org-detail-row">
          <div class="org-detail-row-main">
            <span class="org-detail-row-title">{{ room.name }}</span>
            <span class="org-detail-row-tag">{{ getRoomTypeName(room.room_type) }}</span>
          </div>
          <div class="org-detail-row-meta">
            {{ room.org_name }} ·
            {{ t("organization.stat_workstations", { n: room.workstation_count }) }} ·
            {{ t("organization.stat_cabinets", { n: room.cabinet_count }) }}
          </div>
        </div>
      </template>
    </div>
  </div>
</template>
