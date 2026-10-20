<script setup>
/**
 * 资源管理页（移植自旧版 main.html resources 分区 + resourceTabs.js，
 * 按页签分批迁移：本批 = 网络区域 + 子网两个页签）：
 * 页签记忆（route.query.tab 优先 → localStorage → 默认网络区域，与
 * 旧版 setActiveSubtab 同键名 foims_subtab_resources）。
 * 数据层为 TanStack Query：两张表均分页 20/页、列排序、page_size 可选
 * （10/20/50/100，切换重置页码）；子网页签 4 列表头搜索弹层防抖 300ms
 * 服务端过滤；行内操作（使用情况/编辑/删除）与两张弹窗、删除确认。
 */
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiGet } from "../api/client.js";
import { formatDateTime } from "../utils/formatter.js";
import { showToast } from "../composables/useToast.js";
import ConfirmModal from "../components/ConfirmModal.vue";
import NavIcon from "../components/NavIcon.vue";
import Pagination from "../components/Pagination.vue";
import ThSearch from "../components/ThSearch.vue";
import NetworkModal from "../components/network/NetworkModal.vue";
import NetworkRegionModal from "../components/network/NetworkRegionModal.vue";
import SubnetUsageModal from "../components/network/SubnetUsageModal.vue";

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 页签状态（记忆 + 深链，与 LogsPage 同模式） ====================

// 本批已迁移页签：其余页签（rooms/cabinets/devices/cable-links）随批次追加
const TABS = [
  { id: "network-regions", titleKey: "resources.network_regions" },
  { id: "networks", titleKey: "resources.network_segments" }
];
const SUBTAB_KEY = "foims_subtab_resources";

const route = useRoute();
const router = useRouter();

function readInitialTab() {
  // 深链 #/resources?tab=networks 优先，其次恢复上次记忆的页签，默认网络区域
  const fromQuery = String(route.query.tab ?? "");
  if (TABS.some((tab) => tab.id === fromQuery)) {
    return fromQuery;
  }
  try {
    const saved = localStorage.getItem(SUBTAB_KEY);
    if (TABS.some((tab) => tab.id === saved)) {
      return saved;
    }
  } catch {
    // 存储被禁用时忽略
  }
  return "network-regions";
}

const activeTab = ref(readInitialTab());

function switchTab(tabId) {
  if (tabId === activeTab.value) {
    return;
  }
  activeTab.value = tabId;
  try {
    localStorage.setItem(SUBTAB_KEY, tabId);
  } catch {
    // 存储被禁用时仅会话内生效
  }
  // hash query 记录页签，便于书签直达（replace 不产生历史记录）
  router.replace({ query: { ...route.query, tab: tabId } });
}

// 浏览器前进/后退时按 URL 同步页签
watch(
  () => route.query.tab,
  (value) => {
    const tabId = String(value ?? "");
    if (TABS.some((tab) => tab.id === tabId) && tabId !== activeTab.value) {
      activeTab.value = tabId;
    }
  }
);

// ==================== 分页列表封装（排序 + 越界回退 + 可变页大小） ====================

const PAGE_SIZE_OPTIONS = [10, 20, 50, 100];

/**
 * 分页列表封装：created_at 倒序为默认排序，排序/页大小切换重置页码；
 * 空页且页码越界（末页数据删除后）自动回退最后一页。
 */
function usePagedList({ tag, defaultPageSize, enabled, buildUrl, extraKey }) {
  const page = ref(1);
  const pageSize = ref(defaultPageSize);
  const sort = reactive({ by: "created_at", order: "desc" });

  const query = useQuery({
    queryKey: computed(() => [
      tag,
      {
        page: page.value,
        pageSize: pageSize.value,
        sortBy: sort.by,
        sortOrder: sort.order,
        ...extraKey?.value
      }
    ]),
    queryFn: async () => {
      const result = await apiGet(buildUrl(page.value, pageSize.value, sort.by, sort.order));
      if (!result.success) {
        // apiClient 已将失败归一为 { success:false } 不抛异常，
        // 此处显式抛出让查询进入 error 态（isError/error.message 可渲染）
        throw new Error(result.message || t("common.load_failed"));
      }
      return result.data;
    },
    enabled
  });

  watch(
    () => query.data.value,
    (data) => {
      if (!data || !Array.isArray(data.items) || data.items.length !== 0 || page.value <= 1) {
        return;
      }
      const totalPages = data.total_pages || Math.ceil((data.total || 0) / pageSize.value);
      if (totalPages > 0 && page.value > totalPages) {
        page.value = totalPages;
      }
    }
  );

  function toggleSort(key) {
    if (sort.by === key) {
      sort.order = sort.order === "asc" ? "desc" : "asc";
    } else {
      sort.by = key;
      sort.order = "asc";
    }
    page.value = 1;
  }

  function changePageSize(size) {
    pageSize.value = size;
    page.value = 1;
  }

  return reactive({
    page,
    pageSize,
    sort,
    toggleSort,
    changePageSize,
    pageSizeOptions: PAGE_SIZE_OPTIONS,
    items: computed(() => (Array.isArray(query.data.value?.items) ? query.data.value.items : [])),
    total: computed(() => query.data.value?.total ?? 0),
    totalPages: computed(() => query.data.value?.total_pages ?? 1),
    isError: computed(() => query.isError.value),
    errorMessage: computed(() => query.error.value?.message || t("common.load_failed"))
  });
}

/** 序号列：跨页连续（(page-1)*pageSize + 行内序） */
function seqIndex(list, index) {
  return (list.page - 1) * list.pageSize + index + 1;
}

// ==================== 网络区域页签 ====================

const regions = usePagedList({
  tag: "network-regions",
  defaultPageSize: 20,
  enabled: computed(() => activeTab.value === "network-regions"),
  buildUrl: (page, pageSize, sortBy, sortOrder) =>
    `/api/resources/network-regions?${new URLSearchParams({
      page,
      page_size: pageSize,
      sort_by: sortBy,
      sort_order: sortOrder
    })}`
});

// ==================== 子网页签（4 列表头搜索过滤） ====================

// 输入态（表头搜索框 v-model）与查询参数分离：输入经防抖 300ms 同步进
// 查询参数（旧版 applyNetworkFilters 防抖语义），queryKey 仅引用查询参数
const networkFilters = reactive({ name: "", region: "", ipv4: "", ipv6: "" });
const networkFilterParams = reactive({ name: "", region: "", ipv4: "", ipv6: "" });

const networks = usePagedList({
  tag: "networks",
  defaultPageSize: 20,
  enabled: computed(() => activeTab.value === "networks"),
  extraKey: computed(() => ({ ...networkFilterParams })),
  buildUrl: (page, pageSize, sortBy, sortOrder) => {
    const params = new URLSearchParams({
      page,
      page_size: pageSize,
      sort_by: sortBy,
      sort_order: sortOrder
    });
    // URL 与 queryKey 同源：一律取防抖后的查询参数，避免防抖窗口内
    // 排序/翻页触发重查时读到未防抖的输入值
    if (networkFilterParams.name) {
      params.set("name", networkFilterParams.name);
    }
    if (networkFilterParams.region) {
      params.set("network_region", networkFilterParams.region);
    }
    if (networkFilterParams.ipv4) {
      params.set("ipv4_cidr", networkFilterParams.ipv4);
    }
    if (networkFilterParams.ipv6) {
      params.set("ipv6_cidr", networkFilterParams.ipv6);
    }
    return `/api/resources/networks?${params}`;
  }
});

// 表头搜索输入防抖 300ms：同步到查询参数并重置页码
let filterTimer = null;
watch(networkFilters, () => {
  clearTimeout(filterTimer);
  filterTimer = setTimeout(() => {
    Object.assign(networkFilterParams, { ...networkFilters });
    networks.page = 1;
  }, 300);
});

// ==================== 弹窗与删除 ====================

const regionModal = reactive({ open: false, region: null });
const networkModal = reactive({ open: false, network: null });
const usageModal = reactive({ open: false, subnetId: "" });
const deletingRegion = ref(null);
const deletingNetwork = ref(null);

function openRegionModal(region = null) {
  regionModal.region = region;
  regionModal.open = true;
}

function openNetworkModal(network = null) {
  networkModal.network = network;
  networkModal.open = true;
}

const deleteRegionMutation = useMutation({
  mutationFn: (id) => apiDelete(`/api/resources/network-regions/${id}`),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: ["network-regions"] });
      showToast(t("network.region_delete_success"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
    deletingRegion.value = null;
  }
});

const deleteNetworkMutation = useMutation({
  mutationFn: (id) => apiDelete(`/api/resources/networks/${id}`),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: ["networks"] });
      showToast(t("network.delete_success"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
    deletingNetwork.value = null;
  }
});

/** 排序列头公共类绑定（sorted/asc/desc 三态，与 LogsPage 表头一致） */
function sortClass(list, key) {
  return {
    sorted: list.sort.by === key,
    asc: list.sort.by === key && list.sort.order === "asc",
    desc: list.sort.by === key && list.sort.order === "desc"
  };
}

/**
 * 表头点击排序：表头内嵌过滤输入框、搜索弹层等控件时不触发排序
 * （旧版 initSortEvents 的 closest 守卫语义）
 */
function onThSort(event, list, key) {
  if (event.target.closest("input, select, textarea, button, a, .th-search-popover")) {
    return;
  }
  list.toggleSort(key);
}
</script>

<template>
  <section id="resources" class="content-section active" aria-label="Resources">
    <div class="tabs" role="tablist">
      <button
        v-for="tab in TABS"
        :key="tab.id"
        type="button"
        class="tab-btn"
        :class="{ active: activeTab === tab.id }"
        role="tab"
        :aria-selected="activeTab === tab.id"
        :aria-controls="`${tab.id}-tab`"
        :data-tab="tab.id"
        @click="switchTab(tab.id)"
      >
        {{ t(tab.titleKey) }}
      </button>
    </div>

    <!-- ==================== 网络区域页签 ==================== -->
    <div
      v-show="activeTab === 'network-regions'"
      id="network-regions-tab"
      class="tab-content"
      role="tabpanel"
    >
      <div class="table-container">
        <table id="network-regions-table" class="data-table" aria-label="Network regions">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th
                class="sortable"
                :class="sortClass(regions, 'name')"
                @click="onThSort($event, regions, 'name')"
              >
                {{ t("common.name") }}
              </th>
              <th
                class="sortable"
                :class="sortClass(regions, 'ipv4_cidrs')"
                @click="onThSort($event, regions, 'ipv4_cidrs')"
              >
                {{ t("network.ipv4_cidrs") }}
              </th>
              <th
                class="sortable"
                :class="sortClass(regions, 'ipv6_cidrs')"
                @click="onThSort($event, regions, 'ipv6_cidrs')"
              >
                {{ t("network.ipv6_cidrs") }}
              </th>
              <th>{{ t("common.description") }}</th>
              <th
                class="sortable col-center"
                :class="sortClass(regions, 'created_at')"
                @click="onThSort($event, regions, 'created_at')"
              >
                {{ t("common.created_at") }}
              </th>
              <th>
                <button type="button" class="btn btn-primary btn-sm" @click="openRegionModal()">
                  {{ t("network.add_region") }}
                </button>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="regions.isError">
              <td colspan="7" class="text-center">{{ regions.errorMessage }}</td>
            </tr>
            <tr v-else-if="regions.items.length === 0">
              <td colspan="7" class="text-center">{{ t("network.no_region_data") }}</td>
            </tr>
            <template v-else>
              <tr v-for="(region, index) in regions.items" :key="region.id">
                <td class="index-column">{{ seqIndex(regions, index) }}</td>
                <td>{{ region.name }}</td>
                <td>
                  <template v-if="region.ipv4_cidrs && region.ipv4_cidrs.length > 0">
                    <div v-for="cidr in region.ipv4_cidrs" :key="cidr">{{ cidr }}</div>
                  </template>
                  <template v-else>-</template>
                </td>
                <td>
                  <template v-if="region.ipv6_cidrs && region.ipv6_cidrs.length > 0">
                    <div v-for="cidr in region.ipv6_cidrs" :key="cidr">{{ cidr }}</div>
                  </template>
                  <template v-else>-</template>
                </td>
                <td>{{ region.description || "-" }}</td>
                <td class="col-center">{{ formatDateTime(region.created_at) }}</td>
                <td>
                  <button
                    type="button"
                    class="icon-btn btn-edit"
                    :data-tooltip="t('common.edit')"
                    :aria-label="t('common.edit')"
                    @click="openRegionModal(region)"
                  >
                    <NavIcon name="edit" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-delete"
                    :data-tooltip="t('common.delete')"
                    :aria-label="t('common.delete')"
                    @click="deletingRegion = region"
                  >
                    <NavIcon name="trash" />
                  </button>
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
      <Pagination
        :page="regions.page"
        :total-pages="regions.totalPages"
        :total="regions.total"
        :page-size="regions.pageSize"
        :page-size-options="regions.pageSizeOptions"
        @page-change="(p) => (regions.page = p)"
        @page-size-change="regions.changePageSize"
      />
    </div>

    <!-- ==================== 子网页签 ==================== -->
    <div v-show="activeTab === 'networks'" id="networks-tab" class="tab-content" role="tabpanel">
      <div class="table-container">
        <table id="networks-table" class="data-table" aria-label="Subnets">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th class="sortable th-searchable" :class="sortClass(networks, 'name')" @click="onThSort($event, networks, 'name')">
                <span>{{ t("common.name") }}</span>
                <ThSearch v-model="networkFilters.name" :aria-label="t('common.name')" />
              </th>
              <th class="sortable th-searchable" :class="sortClass(networks, 'network_region')" @click="onThSort($event, networks, 'network_region')">
                <span>{{ t("network.region") }}</span>
                <ThSearch v-model="networkFilters.region" :aria-label="t('network.region')" />
              </th>
              <th class="sortable th-searchable" :class="sortClass(networks, 'ipv4_cidr')" @click="onThSort($event, networks, 'ipv4_cidr')">
                <span>{{ t("network.ipv4_cidr") }}</span>
                <ThSearch v-model="networkFilters.ipv4" :aria-label="t('network.ipv4_cidr')" />
              </th>
              <th class="sortable th-searchable" :class="sortClass(networks, 'ipv6_cidr')" @click="onThSort($event, networks, 'ipv6_cidr')">
                <span>{{ t("network.ipv6_cidr") }}</span>
                <ThSearch v-model="networkFilters.ipv6" :aria-label="t('network.ipv6_cidr')" />
              </th>
              <th class="sortable col-center" :class="sortClass(networks, 'created_at')" @click="onThSort($event, networks, 'created_at')">
                {{ t("common.created_at") }}
              </th>
              <th>{{ t("common.actions") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="networks.isError">
              <td colspan="7" class="text-center">{{ networks.errorMessage }}</td>
            </tr>
            <tr v-else-if="networks.items.length === 0">
              <td colspan="7" class="text-center">{{ t("network.no_match_data") }}</td>
            </tr>
            <template v-else>
              <tr v-for="(item, index) in networks.items" :key="item.id">
                <td class="index-column">{{ seqIndex(networks, index) }}</td>
                <td>{{ item.name }}</td>
                <td>{{ item.network_region }}</td>
                <td>{{ item.ipv4_cidr || "-" }}</td>
                <td>{{ item.ipv6_cidr || "-" }}</td>
                <td class="col-center">{{ formatDateTime(item.created_at) }}</td>
                <td>
                  <button
                    type="button"
                    class="icon-btn btn-usage"
                    :data-tooltip="t('network.usage')"
                    :aria-label="t('network.usage')"
                    @click="usageModal.subnetId = item.id; usageModal.open = true"
                  >
                    <NavIcon name="chart" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-edit"
                    :data-tooltip="t('common.edit')"
                    :aria-label="t('common.edit')"
                    @click="openNetworkModal(item)"
                  >
                    <NavIcon name="edit" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-delete"
                    :data-tooltip="t('common.delete')"
                    :aria-label="t('common.delete')"
                    @click="deletingNetwork = item"
                  >
                    <NavIcon name="trash" />
                  </button>
                </td>
              </tr>
            </template>
          </tbody>
        </table>
      </div>
      <Pagination
        :page="networks.page"
        :total-pages="networks.totalPages"
        :total="networks.total"
        :page-size="networks.pageSize"
        :page-size-options="networks.pageSizeOptions"
        @page-change="(p) => (networks.page = p)"
        @page-size-change="networks.changePageSize"
      />
    </div>

    <!-- ==================== 弹窗 ==================== -->
    <NetworkRegionModal
      v-if="regionModal.open"
      :region="regionModal.region"
      @close="regionModal.open = false"
    />
    <NetworkModal
      v-if="networkModal.open"
      :network="networkModal.network"
      @close="networkModal.open = false"
    />
    <SubnetUsageModal
      v-if="usageModal.open"
      :subnet-id="usageModal.subnetId"
      @close="usageModal.open = false"
    />
    <ConfirmModal
      v-if="deletingRegion"
      :message="t('common.delete_confirm')"
      danger
      @confirm="deleteRegionMutation.mutate(deletingRegion.id)"
      @cancel="deletingRegion = null"
    />
    <ConfirmModal
      v-if="deletingNetwork"
      :message="t('common.delete_confirm')"
      danger
      @confirm="deleteNetworkMutation.mutate(deletingNetwork.id)"
      @cancel="deletingNetwork = null"
    />
  </section>
</template>
