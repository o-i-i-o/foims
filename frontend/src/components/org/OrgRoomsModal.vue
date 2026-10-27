<script setup lang="ts">
/**
 * 组织节点房间列表弹窗（只读）：
 * 列出该节点及其全部下级组织的房间，带直属组织与管理人、工位/机柜计数。
 * 房间是「组织 → 房间 → 工位/机柜/设备」链上的空间归属点，
 * 房间的增删改统一在资源页完成，此处只做归属关系的查看入口。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../../api/client";
import { getRoomTypeName } from "../../utils/formatter";
import ModalShell from "../ModalShell.vue";
import type { RoomSummary } from "../../types/api/generated/RoomSummary";

const props = withDefaults(
  defineProps<{
    open?: boolean;
    orgId?: string;
    orgName?: string;
  }>(),
  { open: false, orgId: "", orgName: "" }
);

const emit = defineEmits<{ close: [] }>();

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
  enabled: computed(() => props.open && Boolean(props.orgId))
});

const rooms = computed(() =>
  roomsQuery.isError.value ? [] : (roomsQuery.data.value ?? [])
);
</script>

<template>
  <ModalShell v-if="open" size="lg" @close="emit('close')">
    <template #title>
      <span>{{ t("organization.rooms_title") }}</span>
      <span v-if="orgName" class="org-room-modal-org"> - {{ orgName }}</span>
    </template>
    <div class="table-container org-room-table-container">
      <table class="data-table">
        <thead>
          <tr>
            <th>{{ t("common.index") }}</th>
            <th>{{ t("room.name") }}</th>
            <th>{{ t("room.type") }}</th>
            <th>{{ t("room.org") }}</th>
            <th class="col-center">{{ t("room.workstation_count") }}</th>
            <th class="col-center">{{ t("organization.room_cabinet_count") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="roomsQuery.isError.value">
            <td colspan="6" class="text-center">{{ roomsQuery.error.value?.message }}</td>
          </tr>
          <tr v-else-if="rooms.length === 0">
            <td colspan="6" class="text-center">{{ t("common.no_data") }}</td>
          </tr>
          <template v-else>
            <tr v-for="(room, index) in rooms" :key="room.id">
              <td class="index-column">{{ index + 1 }}</td>
              <td>{{ room.name }}</td>
              <td>{{ getRoomTypeName(room.room_type) }}</td>
              <td>{{ room.org_name }}</td>
              <td class="col-center">{{ room.workstation_count }}</td>
              <td class="col-center">{{ room.cabinet_count }}</td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>
  </ModalShell>
</template>
