<script setup lang="ts">
/**
 * IP 详情页（移植自旧版 main.html ip 分区 + ipDetail.js）：只读列表展示
 * 各设备网口的 IP 观测数据，11 列（位置/设备/类型/子网/IP/MAC/端口主机名/
 * 状态/最后在线/创建时间），4 列表头搜索弹层（防抖 300ms 服务端过滤），
 * MAC 列头「拉取」按钮打开拉取 MAC 弹窗。默认按 updated_at 倒序、每页
 * 100 条（旧版 IP_PAGE_SIZE），页大小可选 10/20/50/100。
 */
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useQueryClient } from "@tanstack/vue-query";
import { formatDateTime } from "../utils/formatter";
import { getDeviceTypeName } from "../config/deviceOptions";
import { onThSort, seqIndex, sortClass, usePagedList } from "../composables/usePagedList";
import type { IpDetailWithNames } from "../types/api/generated/IpDetailWithNames";
import Pagination from "../components/Pagination.vue";
import ThSearch from "../components/ThSearch.vue";
import PullMacModal from "../components/ip/PullMacModal.vue";

// 列表行类型：generated IpDetailWithNames（/api/resources/ip 行形状）
type IpRow = IpDetailWithNames;

const { t } = useI18n();
const queryClient = useQueryClient();

// 位置列：工位 > 机柜 > 观测设备（旧版优先级语义）
function locationText(row: IpRow): string {
  return row.workstation_name || row.cabinet_name || row.device_name || "-";
}

// 子网列：网段名（区域名），未知时统一回显「未知」
function networkText(row: IpRow): string {
  return `${row.network_name || t("common.unknown")} (${row.network_region || t("common.unknown")})`;
}

// ==================== 表头搜索过滤（输入态与查询参数分离，防抖 300ms） ====================

const ipFilters = reactive({ device_name: "", device_type: "", network: "", ip_address: "" });
const ipFilterParams = reactive({ device_name: "", device_type: "", network: "", ip_address: "" });

const ipList = usePagedList<IpRow>({
  tag: "ip",
  defaultPageSize: 100,
  // 旧版 ipTableState 默认 updated_at 倒序
  defaultSortBy: "updated_at",
  defaultSortOrder: "desc",
  extraKey: computed(() => ({ ...ipFilterParams })),
  buildUrl: (page, pageSize, sortBy, sortOrder) => {
    const params = new URLSearchParams({
      page: String(page),
      page_size: String(pageSize),
      sort_by: sortBy,
      sort_order: sortOrder
    });
    // URL 与 queryKey 同源：一律取防抖后的查询参数
    if (ipFilterParams.device_name) {
      params.set("device_name", ipFilterParams.device_name);
    }
    if (ipFilterParams.device_type) {
      params.set("device_type", ipFilterParams.device_type);
    }
    if (ipFilterParams.network) {
      params.set("network", ipFilterParams.network);
    }
    if (ipFilterParams.ip_address) {
      params.set("ip_address", ipFilterParams.ip_address);
    }
    return `/api/resources/ip?${params}`;
  }
});

// 表头搜索输入防抖 300ms：同步到查询参数并重置页码（排序保持不变）
let filterTimer: number | undefined;
watch(ipFilters, () => {
  clearTimeout(filterTimer);
  filterTimer = setTimeout(() => {
    Object.assign(ipFilterParams, { ...ipFilters });
    ipList.page = 1;
  }, 300);
});

// ==================== 拉取 MAC 弹窗 ====================

const pullMacOpen = ref(false);

function onPullSaved() {
  queryClient.invalidateQueries({ queryKey: ["ip"] });
}
</script>

<template>
  <section id="ip" class="content-section active" :aria-label="t('nav.ip_detail')">
    <div class="table-container">
      <table id="ip-table" class="data-table" :aria-label="t('ip.ip_address')">
        <thead>
          <tr>
            <th class="index-column">{{ t("common.index") }}</th>
            <th>{{ t("ip.location") }}</th>
            <th class="sortable th-searchable" :class="sortClass(ipList, 'device_name')" @click="onThSort($event, ipList, 'device_name')">
              <span>{{ t("common.device") }}</span>
              <ThSearch v-model="ipFilters.device_name" :aria-label="t('common.device')" />
            </th>
            <th class="sortable col-center th-searchable" :class="sortClass(ipList, 'device_type')" @click="onThSort($event, ipList, 'device_type')">
              <span>{{ t("common.type") }}</span>
              <ThSearch v-model="ipFilters.device_type" :aria-label="t('common.type')" />
            </th>
            <th class="sortable th-searchable" :class="sortClass(ipList, 'network_name')" @click="onThSort($event, ipList, 'network_name')">
              <span>{{ t("ip.network") }}</span>
              <ThSearch v-model="ipFilters.network" :aria-label="t('ip.network')" />
            </th>
            <th class="sortable th-searchable" :class="sortClass(ipList, 'ip_address')" @click="onThSort($event, ipList, 'ip_address')">
              <span>{{ t("ip.ip_address") }}</span>
              <ThSearch v-model="ipFilters.ip_address" :aria-label="t('ip.ip_address')" />
            </th>
            <th class="sortable th-inline-action" :class="sortClass(ipList, 'mac_address')" @click="onThSort($event, ipList, 'mac_address')">
              <span>{{ t("ip.mac_address") }}</span>
              <button type="button" class="btn btn-secondary btn-sm" @click="pullMacOpen = true">
                {{ t("ip.pull_short") }}
              </button>
            </th>
            <th class="sortable" :class="sortClass(ipList, 'hostname')" @click="onThSort($event, ipList, 'hostname')">
              {{ t("ip.hostname") }}
            </th>
            <th class="sortable col-center" :class="sortClass(ipList, 'status')" @click="onThSort($event, ipList, 'status')">
              {{ t("ip.status") }}
            </th>
            <th class="sortable col-center" :class="sortClass(ipList, 'last_seen')" @click="onThSort($event, ipList, 'last_seen')">
              {{ t("ip.last_seen") }}
            </th>
            <th class="sortable col-center" :class="sortClass(ipList, 'created_at')" @click="onThSort($event, ipList, 'created_at')">
              {{ t("common.created_at") }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-if="ipList.isError">
            <td colspan="11" class="text-center">{{ ipList.errorMessage }}</td>
          </tr>
          <tr v-else-if="ipList.items.length === 0">
            <td colspan="11" class="text-center">{{ t("ip.no_ip_data") }}</td>
          </tr>
          <template v-else>
            <tr v-for="(row, index) in ipList.items" :key="row.id">
              <td class="index-column">{{ seqIndex(ipList, index) }}</td>
              <td>{{ locationText(row) }}</td>
              <td>{{ row.device_name || "-" }}</td>
              <td class="col-center">{{ getDeviceTypeName(t, row.device_type ?? "") }}</td>
              <td>{{ networkText(row) }}</td>
              <td>{{ row.ip_address }}</td>
              <td>{{ row.mac_address || "-" }}</td>
              <td>{{ row.hostname || "-" }}</td>
              <td class="col-center">
                <span class="status-badge" :class="row.status === 'active' ? 'status-active' : 'status-inactive'">{{ row.status }}</span>
              </td>
              <td class="col-center">{{ formatDateTime(row.last_seen) }}</td>
              <td class="col-center">{{ formatDateTime(row.created_at) }}</td>
            </tr>
          </template>
        </tbody>
      </table>
    </div>
    <Pagination
      :page="ipList.page"
      :total-pages="ipList.totalPages"
      :total="ipList.total"
      :page-size="ipList.pageSize"
      :page-size-options="ipList.pageSizeOptions"
      @page-change="ipList.page = $event"
      @page-size-change="ipList.changePageSize"
    />
    <PullMacModal
      v-if="pullMacOpen"
      @close="pullMacOpen = false"
      @saved="onPullSaved"
    />
  </section>
</template>
