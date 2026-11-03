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
      <table class="data-table" :aria-label="t('organization.rooms_title')">
        <thead>
          <tr>
            <th class="index-column">{{ t("common.index") }}</th>
            <th>{{ t("room.name") }}</th>
            <th class="col-center">{{ t("room.type") }}</th>
            <th>{{ t("room.org") }}</th>
            <th class="col-center">{{ t("room.workstation_count") }}</th>
            <th class="col-center">{{ t("organization.room_cabinet_count") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="roomsQuery.isError.value">
            <td colspan="6" class="table-error-cell">{{ roomsQuery.error.value?.message }}</td>
          </tr>
          <tr v-else-if="rooms.length === 0" class="empty-row">
            <td colspan="6">{{ t("common.no_data") }}</td>
          </tr>
          <template v-else>
            <tr v-for="(room, index) in rooms" :key="room.id">
              <td class="index-column">{{ index + 1 }}</td>
              <td>{{ room.name }}</td>
              <td class="col-center">{{ getRoomTypeName(room.room_type) }}</td>
              <td>{{ room.org_name }}</td>
              <td class="col-center">{{ room.workstation_count }}</td>
              <td class="col-center">{{ room.cabinet_count }}</td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>
  </div>
</template>
