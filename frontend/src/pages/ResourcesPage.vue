<script setup lang="ts">
/**
 * 资源管理页：网络区域/子网/房间/机柜/设备/线路六个页签。
 * 页签记忆由 useSubTabs 统一承载（route.query.tab 优先 → localStorage →
 * 默认网络区域，记忆键 foims_subtab_resources）。
 * 数据层为 TanStack Query：各表分页 20/页、列排序、page_size 可选
 * （10/20/50/100，切换重置页码）；子网页签 4 列表头搜索弹层防抖 300ms
 * 服务端过滤；行内操作（使用情况/子项列表/信息点/机位/编辑/删除）
 * 与各弹窗、删除确认。
 */
import { computed, defineAsyncComponent, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQueryClient } from "@tanstack/vue-query";
import { apiDelete, apiGet } from "../api/client";
import { onThSort, seqIndex, sortClass, usePagedList } from "../composables/usePagedList";
import { useSubTabs, type SubTabMeta } from "../composables/useSubTabs";
import { formatDateTime, formatPowerValue, getRoomTypeName } from "../utils/formatter";
import { getDeviceTypeName } from "../config/deviceOptions";
import { showToast } from "../composables/useToast";
import type { CabinetWithNetworks } from "../types/api/generated/CabinetWithNetworks";
import type { CableLinkWithDetails } from "../types/api/generated/CableLinkWithDetails";
import type { DeviceInterface } from "../types/api/generated/DeviceInterface";
import type { DeviceWithDetails } from "../types/api/generated/DeviceWithDetails";
import type { IpDetail } from "../types/api/generated/IpDetail";
import type { Network } from "../types/api/generated/Network";
import type { NetworkCard } from "../types/api/generated/NetworkCard";
import type { NetworkRegion } from "../types/api/generated/NetworkRegion";
import type { RoomWithNetworks } from "../types/api/generated/RoomWithNetworks";
import ConfirmModal from "../components/ConfirmModal.vue";
import NavIcon from "../components/NavIcon.vue";
import Pagination from "../components/Pagination.vue";
import ThSearch from "../components/ThSearch.vue";
import SimpleListModal from "../components/SimpleListModal.vue";
import NetworkModal from "../components/network/NetworkModal.vue";
import NetworkRegionModal from "../components/network/NetworkRegionModal.vue";
import DeviceSnmpModal from "../components/device/DeviceSnmpModal.vue";
import CableLabelPrintModal from "../components/cable/CableLabelPrintModal.vue";
import CablePrintArea from "../components/cable/CablePrintArea.vue";
import type { RoomBrief } from "../types/api/contracts";
import { invalidateDeviceWrite, queryKeys } from "../utils/queryKeys";

// 重弹窗（编辑器级，源码 ≥15KB）拆独立 chunk：defineAsyncComponent + 模板既有
// v-if 条件挂载 = 打开弹窗时才下载执行，进资源页首渲染无需解析全部弹窗代码；
// 异步包装仅改装载时机，props/事件透传与静态引入完全一致，模板零改动。
// 小弹窗（<15KB）保持静态引入，避免 chunk 碎片化与打开时的额外请求。
// onError 必须显式上抛反馈：部署更新（旧 hash chunk 404）或网络故障时，
// 生产环境 defineAsyncComponent 默认静默渲染空节点，用户无从感知原因。
function onAsyncChunkError(error: unknown): void {
  console.error("弹窗组件 chunk 加载失败:", error);
  showToast(t("common.chunk_load_failed"), "error");
}
const DeviceModal = defineAsyncComponent({
  loader: () => import("../components/device/DeviceModal.vue"),
  onError: onAsyncChunkError
});
const SubnetUsageModal = defineAsyncComponent({
  loader: () => import("../components/network/SubnetUsageModal.vue"),
  onError: onAsyncChunkError
});
const RoomModal = defineAsyncComponent({
  loader: () => import("../components/room/RoomModal.vue"),
  onError: onAsyncChunkError
});
const CabinetModal = defineAsyncComponent({
  loader: () => import("../components/cabinet/CabinetModal.vue"),
  onError: onAsyncChunkError
});
const CableLinkModal = defineAsyncComponent({
  loader: () => import("../components/cable/CableLinkModal.vue"),
  onError: onAsyncChunkError
});

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 页签状态（记忆 + 深链） ====================

// 页签元数据：id 为深链/记忆键，titleKey 为标题 i18n 键
const TABS = [
  { id: "network-regions", titleKey: "resources.network_regions" },
  { id: "networks", titleKey: "resources.network_segments" },
  { id: "rooms", titleKey: "resources.rooms" },
  { id: "cabinets", titleKey: "resources.cabinets" },
  { id: "devices", titleKey: "resources.devices" },
  { id: "cable-links", titleKey: "resources.cable_links" }
] as const satisfies readonly [SubTabMeta, ...SubTabMeta[]];

const { activeTab, switchTab } = useSubTabs({
  storageKey: "foims_subtab_resources",
  tabs: TABS
});

// ==================== 分页列表封装（共享 composable，见 composables/usePagedList.js） ====================

// ==================== 网络区域页签 ====================

const regions = usePagedList<NetworkRegion>({
  tag: queryKeys.networkRegions,
  defaultPageSize: 20,
  enabled: computed(() => activeTab.value === "network-regions"),
  buildUrl: (page, pageSize, sortBy, sortOrder) =>
    `/api/resources/network-regions?${new URLSearchParams({
      page: String(page),
      page_size: String(pageSize),
      sort_by: sortBy,
      sort_order: sortOrder
    })}`
});

// ==================== 子网页签（4 列表头搜索过滤） ====================

// 输入态（表头搜索框 v-model）与查询参数分离：输入经防抖 300ms 同步进
// 查询参数，queryKey 仅引用查询参数
const networkFilters = reactive({ name: "", region: "", ipv4: "", ipv6: "" });
const networkFilterParams = reactive({ name: "", region: "", ipv4: "", ipv6: "" });

const networks = usePagedList<Network>({
  tag: queryKeys.networks,
  defaultPageSize: 20,
  enabled: computed(() => activeTab.value === "networks"),
  extraKey: computed(() => ({ ...networkFilterParams })),
  buildUrl: (page, pageSize, sortBy, sortOrder) => {
    const params = new URLSearchParams({
      page: String(page),
      page_size: String(pageSize),
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
let filterTimer: number | undefined;
watch(networkFilters, () => {
  clearTimeout(filterTimer);
  filterTimer = setTimeout(() => {
    Object.assign(networkFilterParams, { ...networkFilters });
    networks.page = 1;
  }, 300);
});

// ==================== 房间页签 ====================

const rooms = usePagedList<RoomWithNetworks>({
  tag: queryKeys.rooms,
  defaultPageSize: 20,
  enabled: computed(() => activeTab.value === "rooms"),
  // 房间列表默认按名称升序
  defaultSortBy: "name",
  defaultSortOrder: "asc",
  buildUrl: (page, pageSize, sortBy, sortOrder) =>
    `/api/resources/rooms?${new URLSearchParams({
      page: String(page),
      page_size: String(pageSize),
      sort_by: sortBy,
      sort_order: sortOrder
    })}`
});

// ==================== 机柜页签 ====================

const cabinets = usePagedList<CabinetWithNetworks>({
  tag: queryKeys.cabinets,
  defaultPageSize: 20,
  enabled: computed(() => activeTab.value === "cabinets"),
  // 机柜列表默认按名称升序
  defaultSortBy: "name",
  defaultSortOrder: "asc",
  buildUrl: (page, pageSize, sortBy, sortOrder) =>
    `/api/resources/cabinets?${new URLSearchParams({
      page: String(page),
      page_size: String(pageSize),
      sort_by: sortBy,
      sort_order: sortOrder
    })}`
});

// ==================== 设备页签 ====================

const devices = usePagedList<DeviceWithDetails>({
  tag: queryKeys.devices,
  defaultPageSize: 20,
  enabled: computed(() => activeTab.value === "devices"),
  // 设备列表默认按名称升序
  defaultSortBy: "name",
  defaultSortOrder: "asc",
  buildUrl: (page, pageSize, sortBy, sortOrder) =>
    `/api/resources/devices?${new URLSearchParams({
      page: String(page),
      page_size: String(pageSize),
      sort_by: sortBy,
      sort_order: sortOrder
    })}`
});

// ==================== 线路页签 ====================

const cableLinks = usePagedList<CableLinkWithDetails>({
  tag: queryKeys.cableLinks,
  defaultPageSize: 20,
  enabled: computed(() => activeTab.value === "cable-links"),
  // 线路列表不指定排序参数，沿用 usePagedList 默认（created_at 倒序）
  buildUrl: (page, pageSize, sortBy, sortOrder) =>
    `/api/resources/cable-links?${new URLSearchParams({
      page: String(page),
      page_size: String(pageSize),
      sort_by: sortBy,
      sort_order: sortOrder
    })}`
});

// ==================== 弹窗与删除 ====================

const regionModal = reactive<{ open: boolean; region: NetworkRegion | null }>({ open: false, region: null });
const networkModal = reactive<{ open: boolean; network: Network | null }>({ open: false, network: null });
const usageModal = reactive({ open: false, subnetId: "" });
const roomModal = reactive<{ open: boolean; room: RoomBrief | null }>({ open: false, room: null });
const cabinetModal = reactive<{ open: boolean; cabinet: CabinetWithNetworks | null }>({ open: false, cabinet: null });
/** 设备编辑回显载荷：GET /devices/{id} 在 DeviceWithDetails 基础上附加后端脱敏
 * 层注入的 snmp_configured 计算标记与 cards 网卡树（边界扩展，未入 generated
 * 模型；网卡→网口→IP 三级结构与 DeviceModal 的 DeviceCard 同构（cards 与
 * ports/ips 后端恒返回数组，故声明为必填），prop 传递时由 vue-tsc 校验 */
type DeviceEditCardPort = DeviceInterface & { ips: IpDetail[] };
type DeviceEditCard = NetworkCard & { ports: DeviceEditCardPort[] };
type DeviceEditData = DeviceWithDetails & {
  snmp_configured: boolean;
  cards?: DeviceEditCard[];
};

// 设备弹窗：device 为 null = 新增，对象 = 编辑回显（GET /devices/{id} 完整数据）
const deviceModal = reactive<{ open: boolean; device: DeviceEditData | null }>({ open: false, device: null });
// 设备 SNMP 汇聚弹窗：原设备行的端口/MAC表/LLDP/硬件清单/VLAN/CDP/LAG/PoE/
// 观测事件 9 个 SNMP 入口收敛为单按钮唤起，弹窗内以标签切换（deviceId +
// deviceName 供标题与各子功能接口调用）
const snmpHub = reactive({ open: false, deviceId: "", deviceName: "" });
// 线路弹窗：linkId 为 "" = 新增，非空 = 编辑（弹窗内取详情回填）
const cableLinkModal = reactive({ open: false, linkId: "" });
// 标签打印弹窗：items 取当前页列表数据（行内单行打印直接经 CablePrintArea）
const labelPrintModal = reactive<{ open: boolean; items: CableLinkWithDetails[] }>({ open: false, items: [] });
// 通用简单列表弹窗（房间子项/信息点、机柜机位共用）
const simpleList = reactive<{ open: boolean; title: string; columns: string[]; rows: string[][] }>({
  open: false,
  title: "",
  columns: [],
  rows: []
});
const deletingRegion = ref<NetworkRegion | null>(null);
const deletingNetwork = ref<Network | null>(null);
const deletingRoom = ref<RoomWithNetworks | null>(null);
const deletingCabinet = ref<CabinetWithNetworks | null>(null);
const deletingDevice = ref<DeviceWithDetails | null>(null);
const deletingCableLink = ref<CableLinkWithDetails | null>(null);

function openRegionModal(region: NetworkRegion | null = null): void {
  regionModal.region = region;
  regionModal.open = true;
}

function openNetworkModal(network: Network | null = null): void {
  networkModal.network = network;
  networkModal.open = true;
}

function openRoomModal(room: RoomBrief | null = null): void {
  roomModal.room = room;
  roomModal.open = true;
}

function openCabinetModal(cabinet: CabinetWithNetworks | null = null): void {
  cabinetModal.cabinet = cabinet;
  cabinetModal.open = true;
}

// 编辑设备：先取完整详情（含 cards 与 snmp_configured），成功后打开弹窗回显
async function editDevice(id: string): Promise<void> {
  const result = await apiGet<DeviceEditData>(`/api/resources/devices/${id}`);
  if (!result.success || !result.data) {
    showToast(`${t("device.load_failed")}: ${result.message}`, "error");
    return;
  }
  deviceModal.device = result.data;
  deviceModal.open = true;
}

// 行内单行打印：直接以该行两端编号调起打印（不经弹窗）；
// seq 变化强制重挂载打印区，保证重复点击同一行也能再次调起打印
const directPrint = reactive<{ seq: number; items: CableLinkWithDetails[] | null }>({ seq: 0, items: null });

function printRowLabels(item: CableLinkWithDetails): void {
  directPrint.items = [item];
  directPrint.seq++;
}

// 编辑线路：详情由 CableLinkModal 内部查询回填
function editCableLink(id: string): void {
  cableLinkModal.linkId = id;
  cableLinkModal.open = true;
}

// 批量打印：列出当前页线路供勾选
function openCableLabelPrintModal(): void {
  if (cableLinks.items.length === 0) {
    showToast(t("cable_link.print_empty"), "warning");
    return;
  }
  labelPrintModal.items = cableLinks.items;
  labelPrintModal.open = true;
}

// 编辑房间：先取轻量 brief（一次并行查询取齐基础字段与子项），成功后打开弹窗
async function editRoom(id: string): Promise<void> {
  const result = await apiGet<RoomBrief>(`/api/resources/rooms/${id}/brief`);
  if (!result.success || !result.data) {
    showToast(`${t("room.fetch_failed")}: ${result.message}`, "error");
    return;
  }
  openRoomModal(result.data);
}

// 编辑机柜：完整详情含机位/配线架子项，成功后打开弹窗
async function editCabinet(id: string): Promise<void> {
  const result = await apiGet<CabinetWithNetworks>(`/api/resources/cabinets/${id}`);
  if (!result.success || !result.data) {
    showToast(`${t("cabinet.load_failed")}: ${result.message}`, "error");
    return;
  }
  openCabinetModal(result.data);
}

/**
 * 房间子项列表弹窗："其他"房间工位与机柜均可能存在，优先展示有数据的
 * 一类。
 */
async function openRoomChildrenListModal(roomId: string): Promise<void> {
  const result = await apiGet<RoomWithNetworks>(`/api/resources/rooms/${roomId}`);
  if (!result.success || !result.data) {
    showToast(result.message || t("room.load_failed"), "error");
    return;
  }
  const room = result.data;
  const roomType = (room.room_type || "").toLowerCase();
  const isCabinetList =
    roomType === "data_center" ||
    roomType === "telecom_closet" ||
    (roomType === "other" && !(room.workstations || []).length);

  if (!isCabinetList) {
    simpleList.title = `${room.name} - ${t("room.workstations")}`;
    simpleList.columns = [t("common.name"), t("workstation.manager")];
    simpleList.rows = (room.workstations || []).map((ws) => [ws.name || "-", ws.manager_name || "-"]);
  } else {
    simpleList.title = `${room.name} - ${t("room.cabinets")}`;
    simpleList.columns = [t("common.name"), t("cabinet.capacity")];
    simpleList.rows = (room.cabinets || []).map((cab) => [cab.name || "-", String(cab.capacity ?? "-")]);
  }
  simpleList.open = true;
}

/** 房间信息点列表弹窗 */
async function openRoomNetOutletsListModal(roomId: string): Promise<void> {
  const result = await apiGet<RoomWithNetworks>(`/api/resources/rooms/${roomId}`);
  if (!result.success || !result.data) {
    showToast(result.message || t("room.load_failed"), "error");
    return;
  }
  const room = result.data;
  simpleList.title = `${room.name} - ${t("room.net_outlets")}`;
  simpleList.columns = [t("common.name")];
  simpleList.rows = (room.net_outlets || []).map((outlet) => [outlet.name || "-"]);
  simpleList.open = true;
}

/** 机柜机位列表弹窗 */
async function openCabinetPositionsListModal(cabinetId: string): Promise<void> {
  const result = await apiGet<CabinetWithNetworks>(`/api/resources/cabinets/${cabinetId}`);
  if (!result.success || !result.data) {
    showToast(result.message || t("cabinet.load_failed"), "error");
    return;
  }
  const cabinet = result.data;
  simpleList.title = `${cabinet.name} - ${t("cabinet.positions_list")}`;
  simpleList.columns = [
    t("common.name"),
    t("cabinet_position.start_u"),
    t("cabinet_position.end_u"),
    t("common.description")
  ];
  simpleList.rows = (cabinet.positions || []).map((pos) => [
    pos.name || "-",
    String(pos.start_u ?? "-"),
    String(pos.end_u ?? "-"),
    pos.description || "-"
  ]);
  simpleList.open = true;
}

const deleteRegionMutation = useMutation({
  mutationFn: (id: string) => apiDelete(`/api/resources/network-regions/${id}`),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: queryKeys.networkRegions });
      showToast(t("network.region_delete_success"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
    deletingRegion.value = null;
  }
});

const deleteNetworkMutation = useMutation({
  mutationFn: (id: string) => apiDelete(`/api/resources/networks/${id}`),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: queryKeys.networks });
      showToast(t("network.delete_success"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
    deletingNetwork.value = null;
  }
});

const deleteRoomMutation = useMutation({
  mutationFn: (id: string) => apiDelete(`/api/resources/rooms/${id}`),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: queryKeys.rooms });
      showToast(t("room.delete_success"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
    deletingRoom.value = null;
  }
});

const deleteCabinetMutation = useMutation({
  mutationFn: (id: string) => apiDelete(`/api/resources/cabinets/${id}`),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: queryKeys.cabinets });
      showToast(t("cabinet.delete_success"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
    deletingCabinet.value = null;
  }
});

const deleteDeviceMutation = useMutation({
  mutationFn: (id: string) => apiDelete(`/api/resources/devices/${id}`),
  onSuccess: (result) => {
    if (result.success) {
      // 删设备会连带删除其网口/IP 与相关连线，并且改变房间与机柜已分配功率
      invalidateDeviceWrite(queryClient);
      showToast(t("device.delete_success"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
    deletingDevice.value = null;
  }
});

const deleteCableLinkMutation = useMutation({
  mutationFn: (id: string) => apiDelete(`/api/resources/cable-links/${id}`),
  onSuccess: (result) => {
    if (result.success) {
      queryClient.invalidateQueries({ queryKey: queryKeys.cableLinks });
      showToast(t("cable_link.delete_success"), "success");
    } else {
      showToast(`${t("common.operation_failed")}: ${result.message}`, "error");
    }
    deletingCableLink.value = null;
  }
});

/** 房型是否机房类（行内子项列表按钮文案随房型切换：机房显机柜、其余显工位） */
function isCabinetRoomType(roomType: string): boolean {
  const type = (roomType || "").toLowerCase();
  return type === "data_center" || type === "telecom_closet";
}

// 端点/链路类型标签：未知枚举原样回显
function endpointTypeLabel(type: string): string {
  const labels: Record<string, string> = {
    net_outlet: t("cable_link.endpoint_net_outlet"),
    device_interface: t("cable_link.endpoint_device_interface"),
    patch_panel: t("cable_link.endpoint_patch_panel")
  };
  return labels[type] || type;
}

function linkTypeLabel(type: string): string {
  const labels: Record<string, string> = {
    ethernet: t("cable_link.link_type_ethernet"),
    fiber: t("cable_link.link_type_fiber"),
    console: t("cable_link.link_type_console")
  };
  return labels[type] || type;
}

/** 设备位置列：工位优先，其次机柜+U 区间（两者皆有才显示区间） */
function deviceLocationText(item: DeviceWithDetails): string {
  if (item.workstation_name) {
    return `${t("device.workstation")}: ${item.workstation_name}`;
  }
  if (item.cabinet_name) {
    const uRange =
      item.start_u != null && item.end_u != null ? ` ${item.start_u}-${item.end_u}U` : "";
    return `${t("device.position")}: ${item.cabinet_name}${uRange}`;
  }
  return "-";
}

/** 排序列头公共类绑定与表头排序守卫：共享 composable（usePagedList.js） */

</script>

<template>
  <section id="resources" class="content-section active" :aria-label="t('nav.resources')">
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
      :class="{ active: activeTab === 'network-regions' }"
      role="tabpanel"
    >
      <div class="table-container">
        <table id="network-regions-table" class="data-table" :aria-label="t('resources.network_regions')">
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
        @hover-page="regions.prefetch"
      />
    </div>

    <!-- ==================== 子网页签 ==================== -->
    <div v-show="activeTab === 'networks'" id="networks-tab" class="tab-content" :class="{ active: activeTab === 'networks' }" role="tabpanel">
      <div class="table-container">
        <table id="networks-table" class="data-table" :aria-label="t('resources.network_segments')">
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
              <th>
                <button type="button" class="btn btn-primary btn-sm" @click="openNetworkModal()">
                  {{ t("network.add_network") }}
                </button>
              </th>
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
        @hover-page="networks.prefetch"
      />
    </div>

    <!-- ==================== 房间页签 ==================== -->
    <div v-show="activeTab === 'rooms'" id="rooms-tab" class="tab-content" :class="{ active: activeTab === 'rooms' }" role="tabpanel">
      <div class="table-container">
        <table id="rooms-table" class="data-table" :aria-label="t('resources.rooms')">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th class="sortable" :class="sortClass(rooms, 'name')" @click="onThSort($event, rooms, 'name')">
                {{ t("common.name") }}
              </th>
              <th class="sortable col-center" :class="sortClass(rooms, 'room_type')" @click="onThSort($event, rooms, 'room_type')">
                {{ t("common.type") }}
              </th>
              <th class="sortable" :class="sortClass(rooms, 'org_name')" @click="onThSort($event, rooms, 'org_name')">
                {{ t("organization.parent_org") }}
              </th>
              <th class="col-center">{{ t("room.workstation_count") }}</th>
              <th class="col-center">{{ t("room.power") }}</th>
              <th class="col-center">{{ t("room.power_supply") }}</th>
              <th>{{ t("room.networks") }}</th>
              <th>{{ t("common.description") }}</th>
              <th class="sortable col-center" :class="sortClass(rooms, 'created_at')" @click="onThSort($event, rooms, 'created_at')">
                {{ t("common.created_at") }}
              </th>
              <th>
                <button type="button" class="btn btn-primary btn-sm" @click="openRoomModal()">
                  {{ t("room.add_room") }}
                </button>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="rooms.isError">
              <td colspan="11" class="text-center">{{ rooms.errorMessage }}</td>
            </tr>
            <tr v-else-if="rooms.items.length === 0">
              <td colspan="11" class="text-center">{{ t("common.no_data") }}</td>
            </tr>
            <template v-else>
              <tr v-for="(item, index) in rooms.items" :key="item.id">
                <td class="index-column">{{ seqIndex(rooms, index) }}</td>
                <td>{{ item.name }}</td>
                <td class="col-center">{{ getRoomTypeName(item.room_type) }}</td>
                <!-- 所属组织：非空外键，联查恒有值 -->
                <td>{{ item.org_name }}</td>
                <td class="col-center">{{ item.workstation_count ?? "0" }}</td>
                <!-- 当前功率：该房间全部设备功耗之和（固定以瓦显示） -->
                <td class="col-center">{{ formatPowerValue(item.allocated_power_watts ?? 0, "W") }}</td>
                <!-- 电源功率：供电上限（未设置显示 "-"） -->
                <td class="col-center">{{ formatPowerValue(item.total_power_watts, "W") }}</td>
                <td>
                  <template v-if="item.networks && item.networks.length > 0">
                    <div v-for="network in item.networks" :key="network.id">
                      {{ network.name }} ({{ network.network_region }})
                    </div>
                  </template>
                  <template v-else>-</template>
                </td>
                <td>{{ item.description || "-" }}</td>
                <td class="col-center">{{ formatDateTime(item.created_at) }}</td>
                <td>
                  <button
                    type="button"
                    class="icon-btn btn-edit"
                    :data-tooltip="t('common.edit')"
                    :aria-label="t('common.edit')"
                    @click="editRoom(item.id)"
                  >
                    <NavIcon name="edit" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-primary btn-room-children-list"
                    :data-tooltip="isCabinetRoomType(item.room_type) ? t('room.cabinets') : t('room.workstations')"
                    :aria-label="isCabinetRoomType(item.room_type) ? t('room.cabinets') : t('room.workstations')"
                    @click="openRoomChildrenListModal(item.id)"
                  >
                    <NavIcon name="list" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-success"
                    :data-tooltip="t('room.net_outlets')"
                    :aria-label="t('room.net_outlets')"
                    @click="openRoomNetOutletsListModal(item.id)"
                  >
                    <NavIcon name="list" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-delete"
                    :data-tooltip="t('common.delete')"
                    :aria-label="t('common.delete')"
                    @click="deletingRoom = item"
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
        :page="rooms.page"
        :total-pages="rooms.totalPages"
        :total="rooms.total"
        :page-size="rooms.pageSize"
        :page-size-options="rooms.pageSizeOptions"
        @page-change="(p) => (rooms.page = p)"
        @page-size-change="rooms.changePageSize"
        @hover-page="rooms.prefetch"
      />
    </div>

    <!-- ==================== 机柜页签 ==================== -->
    <div v-show="activeTab === 'cabinets'" id="cabinets-tab" class="tab-content" :class="{ active: activeTab === 'cabinets' }" role="tabpanel">
      <div class="table-container">
        <table id="cabinets-table" class="data-table" :aria-label="t('resources.cabinets')">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th class="sortable" :class="sortClass(cabinets, 'room_name')" @click="onThSort($event, cabinets, 'room_name')">
                {{ t("cabinet.room") }}
              </th>
              <th class="sortable" :class="sortClass(cabinets, 'name')" @click="onThSort($event, cabinets, 'name')">
                {{ t("common.name") }}
              </th>
              <th class="sortable col-center" :class="sortClass(cabinets, 'capacity')" @click="onThSort($event, cabinets, 'capacity')">
                {{ t("cabinet.capacity") }}
              </th>
              <th class="col-center">{{ t("cabinet.power") }}</th>
              <th class="col-center">{{ t("cabinet.power_supply") }}</th>
              <th class="col-center">{{ t("cabinet.position_count") }}</th>
              <th>{{ t("common.description") }}</th>
              <th class="sortable col-center" :class="sortClass(cabinets, 'created_at')" @click="onThSort($event, cabinets, 'created_at')">
                {{ t("common.created_at") }}
              </th>
              <th>
                <button type="button" class="btn btn-primary btn-sm" @click="openCabinetModal()">
                  {{ t("cabinet.add_cabinet") }}
                </button>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="cabinets.isError">
              <td colspan="10" class="text-center">{{ cabinets.errorMessage }}</td>
            </tr>
            <tr v-else-if="cabinets.items.length === 0">
              <td colspan="10" class="text-center">{{ t("common.no_data") }}</td>
            </tr>
            <template v-else>
              <tr v-for="(item, index) in cabinets.items" :key="item.id">
                <td class="index-column">{{ seqIndex(cabinets, index) }}</td>
                <td>{{ item.room_name || "-" }}</td>
                <td>{{ item.name }}</td>
                <td class="col-center">{{ item.capacity ?? "-" }}</td>
                <!-- 当前功率：柜内机位关联设备功耗之和（固定以瓦显示） -->
                <td class="col-center">{{ formatPowerValue(item.allocated_power_watts ?? 0, "W") }}</td>
                <!-- 电源功率：供电上限（未设置显示 "-"） -->
                <td class="col-center">{{ formatPowerValue(item.total_power_watts, "W") }}</td>
                <td class="col-center">{{ item.position_count ?? 0 }}</td>
                <td>{{ item.description || "-" }}</td>
                <td class="col-center">{{ formatDateTime(item.created_at) }}</td>
                <td>
                  <button
                    type="button"
                    class="icon-btn btn-edit"
                    :data-tooltip="t('common.edit')"
                    :aria-label="t('common.edit')"
                    @click="editCabinet(item.id)"
                  >
                    <NavIcon name="edit" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-primary"
                    :data-tooltip="t('cabinet.positions_list')"
                    :aria-label="t('cabinet.positions_list')"
                    @click="openCabinetPositionsListModal(item.id)"
                  >
                    <NavIcon name="list" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-delete"
                    :data-tooltip="t('common.delete')"
                    :aria-label="t('common.delete')"
                    @click="deletingCabinet = item"
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
        :page="cabinets.page"
        :total-pages="cabinets.totalPages"
        :total="cabinets.total"
        :page-size="cabinets.pageSize"
        :page-size-options="cabinets.pageSizeOptions"
        @page-change="(p) => (cabinets.page = p)"
        @page-size-change="cabinets.changePageSize"
        @hover-page="cabinets.prefetch"
      />
    </div>

    <!-- ==================== 设备页签 ==================== -->
    <div v-show="activeTab === 'devices'" id="devices-tab" class="tab-content" :class="{ active: activeTab === 'devices' }" role="tabpanel">
      <div class="table-container">
        <table id="devices-table" class="data-table" :aria-label="t('resources.devices')">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th class="sortable" :class="sortClass(devices, 'name')" @click="onThSort($event, devices, 'name')">
                {{ t("common.name") }}
              </th>
              <th class="sortable col-center" :class="sortClass(devices, 'device_type')" @click="onThSort($event, devices, 'device_type')">
                {{ t("common.type") }}
              </th>
              <th class="sortable" :class="sortClass(devices, 'brand')" @click="onThSort($event, devices, 'brand')">
                {{ t("device.brand") }}
              </th>
              <th class="sortable" :class="sortClass(devices, 'model')" @click="onThSort($event, devices, 'model')">
                {{ t("device.model") }}
              </th>
              <th class="col-center">{{ t("device.power_watts") }}</th>
              <th>{{ t("device.location") }}</th>
              <th class="sortable" :class="sortClass(devices, 'room_name')" @click="onThSort($event, devices, 'room_name')">
                {{ t("device.room") }}
              </th>
              <th>{{ t("common.description") }}</th>
              <th>
                <div class="device-toolbar-actions">
                  <button type="button" class="btn btn-primary btn-sm" @click="deviceModal.device = null; deviceModal.open = true">
                    {{ t("device.add") }}
                  </button>
                </div>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="devices.isError">
              <td colspan="10" class="text-center">{{ devices.errorMessage }}</td>
            </tr>
            <tr v-else-if="devices.items.length === 0">
              <td colspan="10" class="text-center">{{ t("common.no_data") }}</td>
            </tr>
            <template v-else>
              <tr v-for="(item, index) in devices.items" :key="item.id">
                <td class="index-column">{{ seqIndex(devices, index) }}</td>
                <td>{{ item.name }}</td>
                <td class="col-center">{{ getDeviceTypeName(t, item.device_type) }}</td>
                <td>{{ item.brand || "-" }}</td>
                <td>{{ item.model || "-" }}</td>
                <td class="col-center">{{ item.power_watts ?? "-" }}</td>
                <td>{{ deviceLocationText(item) }}</td>
                <td>{{ item.room_name || "-" }}</td>
                <td>{{ item.description || "-" }}</td>
                <td>
                  <button
                    type="button"
                    class="icon-btn btn-edit"
                    :data-tooltip="t('common.edit')"
                    :aria-label="t('common.edit')"
                    @click="editDevice(item.id)"
                  >
                    <NavIcon name="edit" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-secondary"
                    :data-tooltip="t('device.snmp_hub.title')"
                    :aria-label="t('device.snmp_hub.title')"
                    @click="snmpHub.deviceId = item.id; snmpHub.deviceName = item.name || ''; snmpHub.open = true"
                  >
                    <NavIcon name="snmp" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-delete"
                    :data-tooltip="t('common.delete')"
                    :aria-label="t('common.delete')"
                    @click="deletingDevice = item"
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
        :page="devices.page"
        :total-pages="devices.totalPages"
        :total="devices.total"
        :page-size="devices.pageSize"
        :page-size-options="devices.pageSizeOptions"
        @page-change="(p) => (devices.page = p)"
        @page-size-change="devices.changePageSize"
        @hover-page="devices.prefetch"
      />
    </div>

    <!-- ==================== 线路页签 ==================== -->
    <div v-show="activeTab === 'cable-links'" id="cable-links-tab" class="tab-content" :class="{ active: activeTab === 'cable-links' }" role="tabpanel">
      <div class="table-container">
        <table id="cable-links-table" class="data-table" :aria-label="t('resources.cable_links')">
          <thead>
            <tr>
              <th class="index-column">{{ t("common.index") }}</th>
              <th>{{ t("cable_link.a_endpoint_type") }}</th>
              <th>{{ t("cable_link.a_endpoint_no") }}</th>
              <th>{{ t("cable_link.b_endpoint_type") }}</th>
              <th>{{ t("cable_link.b_endpoint_no") }}</th>
              <th class="sortable col-center" :class="sortClass(cableLinks, 'link_type')" @click="onThSort($event, cableLinks, 'link_type')">
                {{ t("common.type") }}
              </th>
              <th>
                <span class="sortable" :class="sortClass(cableLinks, 'cable_label')" @click="onThSort($event, cableLinks, 'cable_label')">
                  {{ t("cable_link.cable_label") }}
                </span>
                <button type="button" class="btn btn-secondary btn-sm" @click="openCableLabelPrintModal">
                  {{ t("cable_link.print") }}
                </button>
              </th>
              <th class="sortable col-center" :class="sortClass(cableLinks, 'length_m')" @click="onThSort($event, cableLinks, 'length_m')">
                {{ t("cable_link.length_m") }}
              </th>
              <th class="sortable col-center" :class="sortClass(cableLinks, 'tested')" @click="onThSort($event, cableLinks, 'tested')">
                {{ t("cable_link.tested") }}
              </th>
              <th>
                <button type="button" class="btn btn-primary btn-sm" @click="cableLinkModal.linkId = ''; cableLinkModal.open = true">
                  {{ t("cable_link.add") }}
                </button>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-if="cableLinks.isError">
              <td colspan="10" class="text-center">{{ cableLinks.errorMessage }}</td>
            </tr>
            <tr v-else-if="cableLinks.items.length === 0">
              <td colspan="10" class="text-center">{{ t("common.no_data") }}</td>
            </tr>
            <template v-else>
              <tr v-for="(item, index) in cableLinks.items" :key="item.id">
                <td class="index-column">{{ seqIndex(cableLinks, index) }}</td>
                <td class="col-center">{{ endpointTypeLabel(item.a_endpoint_type) }}</td>
                <td>{{ item.a_endpoint_label || "-" }}</td>
                <td class="col-center">{{ endpointTypeLabel(item.b_endpoint_type) }}</td>
                <td>{{ item.b_endpoint_label || "-" }}</td>
                <td class="col-center">{{ linkTypeLabel(item.link_type) }}</td>
                <td>{{ item.cable_label || "-" }}</td>
                <td class="col-center">{{ item.length_m != null ? `${item.length_m}m` : "-" }}</td>
                <td class="col-center">{{ item.tested ? t("cable_link.tested_yes") : t("cable_link.tested_no") }}</td>
                <td>
                  <button
                    type="button"
                    class="icon-btn btn-edit"
                    :data-tooltip="t('common.edit')"
                    :aria-label="t('common.edit')"
                    @click="editCableLink(item.id)"
                  >
                    <NavIcon name="edit" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-print-cable"
                    :data-tooltip="t('cable_link.print')"
                    :aria-label="t('cable_link.print')"
                    @click="printRowLabels(item)"
                  >
                    <NavIcon name="printer" />
                  </button>
                  <button
                    type="button"
                    class="icon-btn btn-delete"
                    :data-tooltip="t('common.delete')"
                    :aria-label="t('common.delete')"
                    @click="deletingCableLink = item"
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
        :page="cableLinks.page"
        :total-pages="cableLinks.totalPages"
        :total="cableLinks.total"
        :page-size="cableLinks.pageSize"
        :page-size-options="cableLinks.pageSizeOptions"
        @page-change="(p) => (cableLinks.page = p)"
        @page-size-change="cableLinks.changePageSize"
        @hover-page="cableLinks.prefetch"
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
    <RoomModal
      v-if="roomModal.open"
      :room="roomModal.room"
      @close="roomModal.open = false"
    />
    <CabinetModal
      v-if="cabinetModal.open"
      :cabinet="cabinetModal.cabinet"
      @close="cabinetModal.open = false"
    />
    <SimpleListModal
      v-if="simpleList.open"
      :title="simpleList.title"
      :columns="simpleList.columns"
      :rows="simpleList.rows"
      @close="simpleList.open = false"
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
    <ConfirmModal
      v-if="deletingRoom"
      :message="t('common.delete_confirm')"
      danger
      @confirm="deleteRoomMutation.mutate(deletingRoom.id)"
      @cancel="deletingRoom = null"
    />
    <ConfirmModal
      v-if="deletingCabinet"
      :message="t('common.delete_confirm')"
      danger
      @confirm="deleteCabinetMutation.mutate(deletingCabinet.id)"
      @cancel="deletingCabinet = null"
    />

    <!-- 设备编辑弹窗：保存成功后弹窗内先 emit("saved") 再 emit("close")，
         失效放在 saved 上，仅取消关闭时不触发无谓重查。
         设备写会连带改写网口/IP/连线与房间机柜功率，统一失效整片台账 -->
    <DeviceModal
      v-if="deviceModal.open"
      :device="deviceModal.device"
      @close="deviceModal.open = false"
      @saved="invalidateDeviceWrite(queryClient)"
    />
    <DeviceSnmpModal
      v-if="snmpHub.open"
      :device-id="snmpHub.deviceId"
      :device-name="snmpHub.deviceName"
      @close="snmpHub.open = false"
    />
    <!-- 线路编辑弹窗：保存后仅 emit("saved")，需在此关闭并失效列表 -->
    <CableLinkModal
      v-if="cableLinkModal.open"
      :link-id="cableLinkModal.linkId"
      @close="cableLinkModal.open = false"
      @saved="
        cableLinkModal.open = false;
        queryClient.invalidateQueries({ queryKey: queryKeys.cableLinks });
        queryClient.invalidateQueries({ queryKey: queryKeys.devices });
      "
    />
    <CableLabelPrintModal
      v-if="labelPrintModal.open"
      :items="labelPrintModal.items"
      @close="labelPrintModal.open = false"
    />
    <!-- 行内单行打印：seq 变化强制重挂载，打印结束清理 -->
    <CablePrintArea
      v-if="directPrint.items"
      :key="directPrint.seq"
      :items="directPrint.items"
      @done="directPrint.items = null"
    />
    <ConfirmModal
      v-if="deletingDevice"
      :message="t('common.delete_confirm')"
      danger
      @confirm="deleteDeviceMutation.mutate(deletingDevice.id)"
      @cancel="deletingDevice = null"
    />
    <ConfirmModal
      v-if="deletingCableLink"
      :message="t('common.delete_confirm')"
      danger
      @confirm="deleteCableLinkMutation.mutate(deletingCableLink.id)"
      @cancel="deletingCableLink = null"
    />
  </section>
</template>
