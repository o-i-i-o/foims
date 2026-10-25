<script setup lang="ts">
/**
 * 房间网段配置行：
 * 区域下拉联动网段下拉（options 端点按 region_id 过滤，TanStack 缓存），
 * 行间 excludeIds 去重防重复选同一网段；两个 select 均 required，
 * 未选择时由 dynamic-lists.css 的 :invalid 规则显示必填星号。
 */
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useQuery } from "@tanstack/vue-query";
import { apiGet } from "../../api/client";
import NavIcon from "../NavIcon.vue";
import type { ResourceOption } from "../../types/api/generated/ResourceOption";

/** 行内双向绑定值：区域 + 网段 */
interface NetworkConfigValue {
  regionId: string;
  networkId: string;
}

const props = defineProps<{
  /** 其他行已选中的网段 id 集合（当前行选项中被过滤） */
  excludedIds: string[];
}>();

// 行内值双向绑定：{ regionId, networkId }
const model = defineModel<NetworkConfigValue>({ required: true });

const emit = defineEmits<{ remove: [] }>();

const { t } = useI18n();

// 区域选项：与同弹窗其他行共享同一 query 缓存
const regionsQuery = useQuery({
  queryKey: ["options", "network-regions"],
  queryFn: async (): Promise<ResourceOption[] | null> => {
    const result = await apiGet<ResourceOption[]>("/api/resources/options/network-regions");
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

// 网段选项：选定区域后按 region_id 拉取（未选区域时不发请求）
const networksQuery = useQuery({
  queryKey: computed(() => ["options", "networks", model.value.regionId]),
  enabled: computed(() => Boolean(model.value.regionId)),
  queryFn: async (): Promise<ResourceOption[] | null> => {
    const url = `/api/resources/options/networks?region_id=${model.value.regionId}`;
    const result = await apiGet<ResourceOption[]>(url);
    if (!result.success) {
      throw new Error(result.message || t("common.load_failed"));
    }
    return result.data;
  }
});

const regionOptions = computed(() =>
  Array.isArray(regionsQuery.data.value) ? regionsQuery.data.value : []
);

// 过滤其他行已选网段（当前行自身选择不受影响：调用方构造 excludedIds 时已排除）
const networkOptions = computed(() => {
  const items = Array.isArray(networksQuery.data.value) ? networksQuery.data.value : [];
  return items.filter((item) => !props.excludedIds.includes(item.id));
});

function onRegionChange(): void {
  // 切换区域后原网段不再属于新区域，清空待重选
  model.value = { ...model.value, networkId: "" };
}
</script>

<template>
  <div class="network-config-item dyn-list-item">
    <div class="form-row">
      <div class="form-group">
        <select v-model="model.regionId" required @change="onRegionChange">
          <option value="">{{ t("network.select_region") }}</option>
          <option v-for="option in regionOptions" :key="option.id" :value="option.id">
            {{ option.name }}
          </option>
        </select>
      </div>
      <div class="form-group">
        <select v-model="model.networkId" required :disabled="!model.regionId">
          <option value="">{{ t("network.select_segment") }}</option>
          <option v-for="option in networkOptions" :key="option.id" :value="option.id">
            {{ option.name }}
          </option>
        </select>
      </div>
      <div class="form-group network-config-actions">
        <button
          type="button"
          class="icon-btn btn-danger"
          :data-tooltip="t('common.delete')"
          :aria-label="t('common.delete')"
          @click="emit('remove')"
        >
          <NavIcon name="trash" />
        </button>
      </div>
    </div>
  </div>
</template>
