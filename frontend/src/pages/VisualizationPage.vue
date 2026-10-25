<script setup lang="ts">
/**
 * 可视化页（工位 / 机柜 / 拓扑三画布）。
 *
 * 旧版由 modules/visualization/ 的 11 个命令式类驱动：数据加载散落在
 * SVGDataManager / TopologyDataManager，坐标只存在于 SVG 属性上，保存时反读 DOM，
 * 分组推挤靠删除重建整张图。现在本页只负责「数据编排 + 状态持有」：
 *
 * - 坐标的唯一真相是响应式视图模型：拖动落定由画布 emit 矩形，本页同步写回，
 *   保存时经 `buildLayoutItems` 投影成布局项，不再从 DOM 反读；
 * - 每次加载带渲染代次 token：快速切换房间/组织时旧响应直接丢弃；
 * - 三态返回（stale / error / empty）保留原语义：布局请求失败必须与「确认无
 *   布局」区分，否则自动排布会整表 upsert 覆盖已保存布局；
 * - 机柜分批渲染在途时 `cabinetSettled=false`，此时拒绝整表保存（否则会把尚未
 *   渲染的机柜从布局里抹掉）。
 */
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import WorkstationCanvas from "../components/visualization/WorkstationCanvas.vue";
import CabinetCanvas from "../components/visualization/CabinetCanvas.vue";
import TopologyCanvas from "../components/visualization/TopologyCanvas.vue";
import ConnectionCreateModal from "../components/visualization/ConnectionCreateModal.vue";
import ConnectionDetailModal from "../components/visualization/ConnectionDetailModal.vue";
import ContainerCoordModal from "../components/visualization/ContainerCoordModal.vue";
import DeviceDetailPanel from "../components/visualization/DeviceDetailPanel.vue";
import ConfirmModal from "../components/ConfirmModal.vue";
import WorkstationModal from "../components/workstation/WorkstationModal.vue";
import CabinetModal from "../components/cabinet/CabinetModal.vue";
import PositionModal from "../components/cabinet/PositionModal.vue";
import { apiGet } from "../api/client";
import {
  IP_LIST_PATH,
  autoDiscoverTopology,
  deleteCabinetLayout,
  deleteTopologyNode,
  deleteWorkstationLayout,
  createTopologyConnection,
  getCabinetPositionLayout,
  getOrganizationTree,
  getRooms,
  getRoomCabinets,
  getTopologyConnections,
  getTopologyNodes,
  getWorkstationLayout,
  ipsByPositionIdsPath,
  resultArray,
  saveLayout as requestSaveLayout,
  saveTopologyNodes,
  workstationsByRoomPath,
} from "../api/visualization";
import { showToast } from "../composables/useToast";
import { useSubTabs, type SubTabMeta } from "../composables/useSubTabs";
import { fetchAllPages, notifyLoadFailure } from "../utils/pagedFetch";
import {
  CABINET_BATCH_SIZE,
  CABINET_GROUP_GAP,
  CABINET_HEADER,
  CABINET_PADDING,
  ROOM_GAP,
  ROOM_HEADER,
  ROOM_PADDING,
  applyGroupedLayout,
  arrangeWorkstationRects,
  buildLayoutItems,
  clampNonNegative,
  moveGroupTo,
  roomGroupKey,
  separateGroupBoxes,
  snapToGrid,
} from "../utils/visualization/geometry";
import {
  buildCabinetCanvas,
  buildDoorNode,
  buildTopologyCanvasNodes,
  buildWorkstationCanvas,
  cabinetGroupKeyOf,
  filterNodesByOrg,
  groupTopologyNodes,
  mapIpDetailsByWorkstation,
} from "../utils/visualization/viewModel";
import {
  collectOrgSubtreeIds,
  flattenOrgTree,
  organizationOptionLabel,
  roomsQuery,
} from "../utils/visualization/resources";
import type { IpDetailWithNames } from "../types/api/generated/IpDetailWithNames";
import type { LayoutItem } from "../types/api/generated/LayoutItem";
import type { OrganizationTreeNode } from "../types/api/generated/OrganizationTreeNode";
import type { Room } from "../types/api/generated/Room";
import type { WorkstationWithDetails } from "../types/api/generated/WorkstationWithDetails";
import type { Paged } from "../types/api";
import type {
  CabinetCanvasNode,
  CanvasRect,
  DoorCanvasNode,
  LayoutEntity,
  RoomCabinet,
  TopologyCanvasNode,
  TopologyConnection,
  WorkstationCanvasNode,
} from "../types/visualization";
import type { WorkstationSavedPayload } from "../components/workstation/WorkstationModal.vue";
import type { CabinetWithNetworks } from "../types/api/generated/CabinetWithNetworks";
import type { CabinetPosition } from "../types/api/generated/CabinetPosition";
import type { Workstation } from "../types/api/generated/Workstation";

defineOptions({ name: "VisualizationPage" });

type CanvasPosition = { x: number; y: number } | null;
/** 容器层级：房间外框 / 机柜内框 */
type ContainerKind = "room" | "cabinet";
/** 房间布局加载结果：请求失败与被取代都不得触发自动排布 */
type LayoutLoadState = "saved" | "empty" | "stale" | "error";

const { t } = useI18n();

// ==================== 页签 ====================

/** 页签 id 沿用旧版（localStorage 中已记住的偏好继续有效）；画布页签不参与 hash 深链 */
const TABS = [
  { id: "workstation-visualization", titleKey: "visualization.workstation" },
  { id: "cabinet-visualization", titleKey: "visualization.cabinet" },
  { id: "global-visualization", titleKey: "visualization.device" },
] as const satisfies readonly [SubTabMeta, ...SubTabMeta[]];

const { activeTab, switchTab } = useSubTabs({
  storageKey: "foims_subtab_visualization",
  tabs: TABS,
  syncQuery: false,
  // 切到拓扑时才拉取拓扑数据；离开拓扑页签时关闭已打开的连线详情弹窗
  onSwitch: (tabId) => {
    if (tabId === "global-visualization") {
      void ensureTopologyLoaded();
    } else if (connectionDetail.value) {
      connectionDetail.value = null;
    }
  }
});

// ==================== 组织与房间下拉 ====================

const orgTree = ref<OrganizationTreeNode[]>([]);
const orgOptions = computed(() => flattenOrgTree(orgTree.value));

/** 拓扑分组标题的本地化文案（视图模型不依赖 i18n，由本页注入） */
function topologyGroupLabels(): { noRoom: string; noCabinet: string } {
  return { noRoom: t("viz.group_no_room"), noCabinet: t("viz.group_no_cabinet") };
}

async function loadOrganizationTree(): Promise<void> {
  const result = await getOrganizationTree();
  if (!result.success) {
    notifyLoadFailure(showToast, result.message, "获取组织树");
    orgTree.value = [];
    return;
  }
  orgTree.value = resultArray(result);
}

/**
 * 工位视图默认组织：取第一个组织节点的最后一个下级，无下级时取该节点本身
 * （与旧版一致：刷新后组织下拉停在最常用的末级组织）。
 */
function applyDefaultWorkstationOrg(): void {
  const firstRoot = orgTree.value[0];
  if (!firstRoot) {
    return;
  }
  const children = firstRoot.children;
  const target = children.length > 0 ? children[children.length - 1] : firstRoot;
  workstationOrgId.value = target?.id ?? "";
}

// ==================== 工位画布 ====================

const workstationOrgId = ref("");
/** 房间类型过滤（空串 = 办公类全集） */
const workstationRoomType = ref("");
const workstationRooms = ref<Room[]>([]);
const workstationRoomId = ref("");
const workstationNodes = ref<WorkstationCanvasNode[]>([]);
const workstationDoor = ref<DoorCanvasNode | null>(null);
const workstationFitKey = ref(0);

/** 工位渲染代次：切换房间时旧响应丢弃，避免画布混入两个房间的工位 */
let workstationToken = 0;
let workstationAutoSaveTimer: number | null = null;

const WORKSTATION_ROOM_TYPES = [
  { value: "", labelKey: "viz.all_room_types" },
  { value: "office", labelKey: "room.type_office" },
  { value: "lobby", labelKey: "room.type_lobby" },
  { value: "reception", labelKey: "room.type_reception" },
  { value: "other", labelKey: "room.type_other" },
] as const;

const CABINET_ROOM_TYPES = [
  { value: "", labelKey: "viz.all_room_types" },
  { value: "data_center", labelKey: "room.type_datacenter" },
  { value: "telecom_closet", labelKey: "room.type_telecom_closet" },
  { value: "other", labelKey: "room.type_other" },
] as const;

function clearWorkstationCanvas(): void {
  workstationToken += 1;
  workstationNodes.value = [];
  workstationDoor.value = null;
  workstationFitKey.value += 1;
}

/**
 * 加载房间工位布局。
 *
 * `stale`（已被更新的加载取代）与 `error`（布局请求失败）都不允许触发自动
 * 排布落库——一次瞬时故障若被当成「无布局」，整表 upsert 会覆盖该房间已保存
 * 的布局。
 */
async function loadWorkstationRoom(roomId: string): Promise<LayoutLoadState> {
  workstationRoomId.value = roomId;
  const token = ++workstationToken;
  workstationNodes.value = [];
  workstationDoor.value = null;

  const [layoutResult, workstations, ipDetails] = await Promise.all([
    getWorkstationLayout(roomId),
    fetchAllPages<WorkstationWithDetails>({
      apiGet,
      showToast,
      path: workstationsByRoomPath(roomId),
      what: "获取工位数据",
    }),
    fetchAllPages<IpDetailWithNames>({
      apiGet,
      showToast,
      path: IP_LIST_PATH,
      what: "获取IP",
    }),
  ]);
  if (token !== workstationToken) {
    return "stale";
  }
  if (!layoutResult.success) {
    notifyLoadFailure(showToast, layoutResult.message, "获取房间布局");
    return "error";
  }

  const layout = resultArray(layoutResult);
  const canvas = buildWorkstationCanvas({ workstations, layout, ipDetails });
  workstationDoor.value = canvas.door;
  // 无已保存布局时只落门元素：工位由随后的自动排布生成（列数按数量自适应）
  workstationNodes.value = layout.length > 0 ? canvas.nodes : [];
  workstationFitKey.value += 1;
  return layout.length > 0 ? "saved" : "empty";
}

/** 选择房间即出图：有布局直接渲染，确认无布局时自动排布并落库一次 */
async function showWorkstationRoom(roomId: string): Promise<void> {
  const state = await loadWorkstationRoom(roomId);
  if (state !== "empty") {
    return;
  }
  await autoDrawWorkstations(roomId, { skipReload: true });
  await saveWorkstationLayout();
}

/**
 * 按网格自动排布工位。
 * @param options.skipReload 调用方已确认无已保存布局时跳过二次布局加载
 */
async function autoDrawWorkstations(roomId: string, options: { skipReload?: boolean } = {}): Promise<void> {
  if (!options.skipReload) {
    const state = await loadWorkstationRoom(roomId);
    if (state === "stale" || state === "error" || state === "saved") {
      return;
    }
  }

  const token = ++workstationToken;
  const [workstations, ipDetails] = await Promise.all([
    fetchAllPages<WorkstationWithDetails>({
      apiGet,
      showToast,
      path: workstationsByRoomPath(roomId),
      what: "获取工位数据",
    }),
    fetchAllPages<IpDetailWithNames>({ apiGet, showToast, path: IP_LIST_PATH, what: "获取IP" }),
  ]);
  if (token !== workstationToken) {
    return;
  }
  if (workstations.length === 0) {
    showToast(t("viz.no_workstation_data"), "info");
    return;
  }

  const rects = arrangeWorkstationRects(workstations.length);
  const ipMap = mapIpDetailsByWorkstation(ipDetails);
  const nodes: WorkstationCanvasNode[] = [];
  for (const [index, workstation] of workstations.entries()) {
    nodes.push({
      ...workstation,
      rect: rects[index] ?? { x: 0, y: 0, width: 0, height: 0 },
      ipDetail: ipMap.get(workstation.id) ?? null,
    });
  }
  workstationNodes.value = nodes;
  workstationDoor.value = buildDoorNode([]);
  workstationFitKey.value += 1;
}

async function refreshWorkstationRooms(): Promise<void> {
  const result = await getRooms(roomsQuery(workstationRoomType.value || "office", workstationOrgId.value || null));
  if (!result.success) {
    notifyLoadFailure(showToast, result.message, "获取房间列表");
    workstationRooms.value = [];
    clearWorkstationCanvas();
    return;
  }
  workstationRooms.value = result.data?.items ?? [];
  const first = workstationRooms.value[0];
  if (!first) {
    clearWorkstationCanvas();
    return;
  }
  await showWorkstationRoom(first.id);
}

function onWorkstationRoomChange(event: Event): void {
  const roomId = (event.target as HTMLSelectElement).value;
  if (!roomId || roomId === workstationRoomId.value) {
    return;
  }
  void showWorkstationRoom(roomId);
}

function onWorkstationOrgChange(event: Event): void {
  workstationOrgId.value = (event.target as HTMLSelectElement).value;
  void refreshWorkstationRooms();
}

function onWorkstationRoomTypeChange(event: Event): void {
  workstationRoomType.value = (event.target as HTMLSelectElement).value;
  void refreshWorkstationRooms();
}

/** 拖动落定：同步写回视图模型（预览与数据必须同帧一致），再防抖保存 */
function onWorkstationNodeCommit(id: string, rect: CanvasRect): void {
  const node = workstationNodes.value.find((item) => item.id === id);
  if (!node) {
    return;
  }
  node.rect = rect;
  scheduleWorkstationAutoSave();
}

function onDoorCommit(id: string, rect: CanvasRect): void {
  const door = workstationDoor.value;
  if (!door || door.id !== id) {
    return;
  }
  workstationDoor.value = { ...door, rect };
  scheduleWorkstationAutoSave();
}

/** 工位拖拽自动保存：600ms 内连续拖动只落一次，成功不提示 */
function scheduleWorkstationAutoSave(): void {
  if (workstationAutoSaveTimer !== null) {
    window.clearTimeout(workstationAutoSaveTimer);
  }
  workstationAutoSaveTimer = window.setTimeout(() => {
    workstationAutoSaveTimer = null;
    void saveWorkstationLayout({ silent: true });
  }, 600);
}

async function saveWorkstationLayout(options: { silent?: boolean } = {}): Promise<void> {
  const door = workstationDoor.value;
  if (!door || !workstationRoomId.value) {
    if (!options.silent) {
      showToast(t("viz.select_room_first"), "warning");
    }
    return;
  }
  const entities: LayoutEntity[] = [
    { id: door.id, elementType: "door", rect: door.rect },
    ...workstationNodes.value.map((node) => ({ id: node.id, elementType: "workstation", rect: node.rect })),
  ];
  const layout = buildLayoutItems(entities);
  if (layout.length === 0) {
    if (!options.silent) {
      showToast(t("viz.no_workstation_data"), "info");
    }
    return;
  }
  const result = await requestSaveLayout({
    type: "workstation",
    room_id: workstationRoomId.value,
    layout,
  });
  if (!result.success) {
    showToast(`${t("viz.layout_save_failed")}: ${result.message}`, "error");
    return;
  }
  if (!options.silent) {
    showToast(t("viz.layout_save_success"), "success");
  }
}

async function autoDrawWorkstationFromToolbar(): Promise<void> {
  if (!workstationRoomId.value) {
    showToast(t("viz.select_room_first"), "warning");
    return;
  }
  await autoDrawWorkstations(workstationRoomId.value);
  await saveWorkstationLayout();
}

// ==================== 机柜画布 ====================

const cabinetOrgId = ref("");
const cabinetRoomType = ref("");
const cabinetRooms = ref<Room[]>([]);
const cabinetRoomId = ref("");
const cabinetNodes = ref<CabinetCanvasNode[]>([]);
const cabinetTotalWidth = ref(0);
const cabinetCanvasHeight = ref(0);
const cabinetFitKey = ref(0);
/** 画布容器实测尺寸（容器 resize 后本地重排复用，不回服务器重载） */
const cabinetSize = ref({ width: 0, height: 0 });
/** 分批渲染完成标志：渲染在途时整表 upsert 会把未渲染机柜从布局里抹掉 */
const cabinetSettled = ref(true);

let cabinetToken = 0;
let cabinetRelayoutTimer: number | null = null;
/** 最近一次渲染输入：容器 resize 后本地重排复用 */
let lastCabinetInput: { cabinets: RoomCabinet[]; layout: LayoutItem[] } | null = null;
/** 机位 IP 缓存（position_id → 明细）：重排零请求，新一次完整加载清空 */
let cabinetIpCache = new Map<string, IpDetailWithNames>();

function clearCabinetCanvas(): void {
  cabinetToken += 1;
  cabinetNodes.value = [];
  cabinetTotalWidth.value = 0;
  cabinetSettled.value = true;
  lastCabinetInput = null;
  cabinetIpCache = new Map();
  cabinetFitKey.value += 1;
}

/** 等一帧后复查容器高度：页签刚切出时测量值可能仍为 0 */
async function waitForContainerHeight(): Promise<number> {
  const measure = (): number => cabinetSize.value.height;
  let height = measure();
  if (height < 100) {
    await new Promise<void>((resolve) => {
      window.requestAnimationFrame(() => resolve());
    });
    height = measure();
  }
  return height >= 100 ? height : 600;
}

/** 机位 IP 批量拉取：命中缓存的部分不再请求（重排路径零请求） */
async function fetchIpMapByPositions(positionIds: string[]): Promise<Map<string, IpDetailWithNames>> {
  const map = new Map<string, IpDetailWithNames>();
  const missing: string[] = [];
  for (const id of new Set(positionIds)) {
    const cached = cabinetIpCache.get(id);
    if (cached) {
      map.set(id, cached);
    } else {
      missing.push(id);
    }
  }
  if (missing.length === 0) {
    return map;
  }
  const result = await apiGet<Paged<IpDetailWithNames>>(ipsByPositionIdsPath(missing));
  if (!result.success) {
    notifyLoadFailure(showToast, result.message, "批量获取机位IP");
    return map;
  }
  for (const detail of result.data?.items ?? []) {
    if (detail.position_id) {
      cabinetIpCache.set(detail.position_id, detail);
      map.set(detail.position_id, detail);
    }
  }
  return map;
}

/**
 * 分批渲染机柜：每批（16 柜）渲染后统一拉取该批机位 IP 再回填，
 * 避免逐柜请求；机柜多时用户可先看到并滚动前几批。
 * 递归逐批渲染（而非 for+await）以便批间做代次校验。
 */
async function renderCabinetBatches(nodes: CabinetCanvasNode[], token: number): Promise<boolean> {
  const renderFrom = async (start: number): Promise<boolean> => {
    if (token !== cabinetToken) {
      return false;
    }
    if (start >= nodes.length) {
      cabinetSettled.value = true;
      return true;
    }
    const batch = nodes.slice(start, start + CABINET_BATCH_SIZE);
    const startIndex = cabinetNodes.value.length;
    cabinetNodes.value.push(...batch);

    const positionIds = batch.flatMap((cabinet) => cabinet.positions.map((position) => position.id));
    const ipMap = positionIds.length > 0 ? await fetchIpMapByPositions(positionIds) : new Map();
    if (token !== cabinetToken) {
      return false;
    }
    // 经数组回写（而不是批对象本身）：批对象是原始数据，改写不触发响应式
    for (let offset = 0; offset < batch.length; offset += 1) {
      const stored = cabinetNodes.value[startIndex + offset];
      if (!stored) {
        continue;
      }
      for (const position of stored.positions) {
        position.ipDetail = ipMap.get(position.id) ?? null;
      }
    }
    return renderFrom(start + CABINET_BATCH_SIZE);
  };
  return renderFrom(0);
}

/** 计算机柜位置并定型画布，再分批渲染（首帧即最终布局，无先画后纠正的闪变） */
async function layoutAndRenderCabinets(
  cabinets: RoomCabinet[],
  layout: LayoutItem[],
  token: number,
): Promise<boolean> {
  lastCabinetInput = { cabinets, layout };
  const containerHeight = await waitForContainerHeight();
  if (token !== cabinetToken) {
    return false;
  }
  cabinetCanvasHeight.value = containerHeight;
  const canvas = buildCabinetCanvas({ cabinets, layout, containerHeight });
  cabinetTotalWidth.value = canvas.totalWidth;
  cabinetNodes.value = [];
  cabinetFitKey.value += 1;
  return renderCabinetBatches(canvas.nodes, token);
}

async function loadCabinetRoom(roomId: string): Promise<LayoutLoadState> {
  cabinetRoomId.value = roomId;
  const token = ++cabinetToken;
  cabinetNodes.value = [];
  cabinetTotalWidth.value = 0;
  cabinetSettled.value = false;
  lastCabinetInput = null;
  cabinetIpCache = new Map();

  const [layoutResult, cabinets] = await Promise.all([
    getCabinetPositionLayout(roomId),
    getRoomCabinets(roomId),
  ]);
  if (token !== cabinetToken) {
    return "stale";
  }
  if (!layoutResult.success) {
    notifyLoadFailure(showToast, layoutResult.message, "获取机柜布局");
    // 本次不会有渲染跟进：恢复完成标志，避免保存被永久拒绝
    cabinetSettled.value = true;
    cabinetFitKey.value += 1;
    return "error";
  }

  const layout = resultArray(layoutResult);
  if (layout.length === 0) {
    showToast(t("viz.no_room_layout_data"), "info");
  }
  const rooms = resultArray(cabinets);
  if (layout.length === 0 || rooms.length === 0) {
    // 无布局/无机柜：空画布（自动排布由工具栏按钮显式触发）
    cabinetSettled.value = true;
    cabinetFitKey.value += 1;
    return layout.length > 0 ? "saved" : "empty";
  }
  await layoutAndRenderCabinets(rooms, layout, token);
  return "saved";
}

async function refreshCabinetRooms(): Promise<void> {
  const result = await getRooms(roomsQuery(cabinetRoomType.value || "datacenter", cabinetOrgId.value || null));
  if (!result.success) {
    notifyLoadFailure(showToast, result.message, "获取房间列表");
    cabinetRooms.value = [];
    clearCabinetCanvas();
    return;
  }
  cabinetRooms.value = result.data?.items ?? [];
  const first = cabinetRooms.value[0];
  if (!first) {
    clearCabinetCanvas();
    return;
  }
  await loadCabinetRoom(first.id);
}

function onCabinetRoomChange(event: Event): void {
  const roomId = (event.target as HTMLSelectElement).value;
  if (!roomId || roomId === cabinetRoomId.value) {
    return;
  }
  void loadCabinetRoom(roomId);
}

function onCabinetOrgChange(event: Event): void {
  cabinetOrgId.value = (event.target as HTMLSelectElement).value;
  void refreshCabinetRooms();
}

function onCabinetRoomTypeChange(event: Event): void {
  cabinetRoomType.value = (event.target as HTMLSelectElement).value;
  void refreshCabinetRooms();
}

/**
 * 容器尺寸变化：防抖 150ms 后按新高度重算 U 高与柜底贴底位置。
 * 渲染在途或容器隐藏（其他页签）时跳过，切回可见后 resize 会再触发。
 */
function onCabinetResize(size: { width: number; height: number }): void {
  cabinetSize.value = size;
  if (!size.width || !size.height) {
    return;
  }
  if (cabinetRelayoutTimer !== null) {
    window.clearTimeout(cabinetRelayoutTimer);
  }
  cabinetRelayoutTimer = window.setTimeout(() => {
    cabinetRelayoutTimer = null;
    if (!cabinetSettled.value || !lastCabinetInput) {
      return;
    }
    cabinetSettled.value = false;
    void layoutAndRenderCabinets(lastCabinetInput.cabinets, lastCabinetInput.layout, cabinetToken);
  }, 150);
}

function onCabinetCommit(id: string, rect: CanvasRect): void {
  const node = cabinetNodes.value.find((item) => item.id === id);
  if (!node) {
    return;
  }
  // 机位矩形由柜体 + U 区间实时推导，无需同步子元素
  node.rect = rect;
}

/** 机柜/机位编辑弹窗保存后重载：容量与 U 区间变化需重算 U 高与机位矩形 */
async function reloadCabinetRoom(): Promise<void> {
  if (!cabinetRoomId.value) {
    return;
  }
  await loadCabinetRoom(cabinetRoomId.value);
}

async function autoDrawCabinetFromToolbar(): Promise<void> {
  if (!cabinetRoomId.value) {
    showToast(t("viz.select_room_first"), "warning");
    return;
  }
  const token = ++cabinetToken;
  cabinetSettled.value = false;
  cabinetNodes.value = [];
  cabinetTotalWidth.value = 0;
  cabinetIpCache = new Map();
  const cabinets = await getRoomCabinets(cabinetRoomId.value);
  if (token !== cabinetToken) {
    return;
  }
  const rooms = resultArray(cabinets);
  if (rooms.length === 0) {
    cabinetSettled.value = true;
    cabinetFitKey.value += 1;
    showToast(t("viz.no_cabinet_data"), "info");
    return;
  }
  await layoutAndRenderCabinets(rooms, [], token);
  await saveCabinetLayout();
}

async function saveCabinetLayout(options: { silent?: boolean } = {}): Promise<void> {
  if (!cabinetSettled.value) {
    showToast(t("viz.rendering_in_progress"), "warning");
    return;
  }
  if (!cabinetRoomId.value) {
    if (!options.silent) {
      showToast(t("viz.select_room_first"), "warning");
    }
    return;
  }
  const entities: LayoutEntity[] = cabinetNodes.value.map((node) => ({
    id: node.id,
    elementType: "network_device",
    rect: node.rect,
  }));
  const layout = buildLayoutItems(entities);
  if (layout.length === 0) {
    if (!options.silent) {
      showToast(t("viz.no_cabinet_data"), "info");
    }
    return;
  }
  const result = await requestSaveLayout({ type: "cabinet", room_id: cabinetRoomId.value, layout });
  if (!result.success) {
    showToast(`${t("viz.layout_save_failed")}: ${result.message}`, "error");
    return;
  }
  if (!options.silent) {
    showToast(t("viz.layout_save_success"), "success");
  }
}

// ==================== 拓扑画布 ====================

const topologyCanvas = ref<InstanceType<typeof TopologyCanvas> | null>(null);
const topologyOrgId = ref("");
const topologyNodes = ref<TopologyCanvasNode[]>([]);
const topologyConnections = ref<TopologyConnection[]>([]);
const topologyFitKey = ref(0);
/** 组织筛选集合：null 显示全部（一个组织一套布局） */
const topologyOrgFilter = ref<Set<string> | null>(null);
const topologyLoaded = ref(false);
const zoomScale = ref(1);
const connectionMode = ref(false);

let topologyToken = 0;
let topologyAutoSaveTimer: number | null = null;

const visibleTopologyNodes = computed(() => filterNodesByOrg(topologyNodes.value, topologyOrgFilter.value));
const visibleTopologyConnections = computed(() => {
  const ids = new Set(visibleTopologyNodes.value.map((node) => node.device_id));
  return topologyConnections.value.filter(
    (connection) => ids.has(connection.source_device_id) && ids.has(connection.target_device_id),
  );
});

const zoomPercent = computed(() => Math.round(zoomScale.value * 100));
const ZOOM_PRESETS = [0.25, 0.5, 0.75, 1, 1.5, 2, 3] as const;
/** 当前比例命中预设（±2%）时回显预设，否则回显动态百分比项 */
const zoomSelectValue = computed(() => {
  const preset = ZOOM_PRESETS.find(
    (value) => Math.abs(value - zoomScale.value) / zoomScale.value <= 0.02,
  );
  return preset === undefined ? "current" : String(preset);
});

function zoomOptionLabel(scale: number): string {
  return `${Math.round(scale * 100)}%`;
}

function onZoomChange(scale: number): void {
  zoomScale.value = scale;
}

async function ensureTopologyLoaded(): Promise<void> {
  if (topologyLoaded.value) {
    return;
  }
  await loadTopology();
}

async function loadTopology(): Promise<void> {
  const token = ++topologyToken;
  topologyNodes.value = [];
  topologyConnections.value = [];
  const [nodesResult, connectionsResult] = await Promise.all([
    getTopologyNodes(),
    getTopologyConnections(),
  ]);
  if (token !== topologyToken) {
    return;
  }
  if (!nodesResult.success) {
    notifyLoadFailure(showToast, nodesResult.message, "获取拓扑节点");
  }
  if (!connectionsResult.success) {
    notifyLoadFailure(showToast, connectionsResult.message, "获取拓扑连线");
  }
  topologyNodes.value = buildTopologyCanvasNodes(resultArray(nodesResult));
  topologyConnections.value = resultArray(connectionsResult);
  // 修正历史布局中房间包围盒相互覆盖的情况（整体平移分组，不动单个设备）
  separateTopologyGroups(null, null);
  topologyFitKey.value += 1;
  topologyLoaded.value = true;
}

/**
 * 推开包围盒相交的分组：先房间级、再房间内机柜级。
 * `fixedRoomKey` / `fixedCabinetKey` 指定的分组保持不动（拖拽落定的容器不位移）。
 */
function separateTopologyGroups(fixedRoomKey: string | null, fixedCabinetKey: string | null): boolean {
  const rooms = groupTopologyNodes(visibleTopologyNodes.value, topologyGroupLabels());
  let moved = separateGroupBoxes(
    rooms.map((room) => ({
      key: room.key,
      nodes: room.nodes,
      padding: ROOM_PADDING,
      header: ROOM_HEADER,
    })),
    ROOM_GAP,
    fixedRoomKey,
  );
  for (const room of rooms) {
    const separated = separateGroupBoxes(
      room.cabinets.map((cabinet) => ({
        key: cabinet.key,
        nodes: cabinet.nodes,
        padding: CABINET_PADDING,
        header: CABINET_HEADER,
      })),
      CABINET_GROUP_GAP,
      fixedCabinetKey,
    );
    moved = moved || separated;
  }
  return moved;
}

function findGroupLabel(kind: ContainerKind, key: string): string {
  for (const room of groupTopologyNodes(visibleTopologyNodes.value, topologyGroupLabels())) {
    if (kind === "room" && room.key === key) {
      return room.label;
    }
    if (kind === "cabinet") {
      const cabinet = room.cabinets.find((item) => item.key === key);
      if (cabinet) {
        return cabinet.label;
      }
    }
  }
  return "";
}

function onTopologyOrgChange(event: Event): void {
  topologyOrgId.value = (event.target as HTMLSelectElement).value;
  topologyOrgFilter.value = topologyOrgId.value
    ? collectOrgSubtreeIds(orgTree.value, topologyOrgId.value)
    : null;
  topologyFitKey.value += 1;
}

function onTopologyNodeDragEnd(deviceId: string, rect: CanvasRect): void {
  const node = topologyNodes.value.find((item) => item.device_id === deviceId);
  if (!node) {
    return;
  }
  node.rect = rect;
  // 节点拖动只固定自身所属房间；推挤真的移动了其他分组时重新适配视野
  if (separateTopologyGroups(roomGroupKey(node.room_id), null)) {
    topologyFitKey.value += 1;
  }
  scheduleTopologyAutoSave();
}

/**
 * 容器拖动落定：`fixedGroupKey` 非空即机柜级拖动（fixedKey 为所属房间），
 * 为空则是房间级拖动（fixedKey 为被拖房间）。
 */
function onTopologyContainerDragEnd(
  fixedKey: string,
  fixedGroupKey: string | null,
  offset: { x: number; y: number },
): void {
  const members = visibleTopologyNodes.value.filter((node) =>
    fixedGroupKey ? cabinetGroupKeyOf(node) === fixedGroupKey : roomGroupKey(node.room_id) === fixedKey,
  );
  if (members.length === 0) {
    return;
  }
  for (const node of members) {
    node.rect = {
      ...node.rect,
      x: Math.round(node.rect.x + offset.x),
      y: Math.round(node.rect.y + offset.y),
    };
  }
  if (separateTopologyGroups(fixedKey, fixedGroupKey)) {
    topologyFitKey.value += 1;
  }
  scheduleTopologyAutoSave();
}

/** 拖拽自动保存：500ms 内连续拖动只落一次 */
function scheduleTopologyAutoSave(): void {
  if (topologyAutoSaveTimer !== null) {
    window.clearTimeout(topologyAutoSaveTimer);
  }
  topologyAutoSaveTimer = window.setTimeout(() => {
    topologyAutoSaveTimer = null;
    void saveTopologyLayout({ silent: true });
  }, 500);
}

async function saveTopologyLayout(options: { silent?: boolean } = {}): Promise<void> {
  const nodes = visibleTopologyNodes.value.map((node) => ({
    device_id: node.device_id,
    x: node.rect.x,
    y: node.rect.y,
    width: node.rect.width,
    height: node.rect.height,
  }));
  if (nodes.length === 0) {
    if (!options.silent) {
      showToast(t("viz.no_nodes_to_save"), "warning");
    }
    return;
  }
  const result = await saveTopologyNodes(nodes);
  if (!result.success) {
    showToast(`${t("viz.node_save_failed")}: ${result.message}`, "error");
    return;
  }
  if (!options.silent) {
    showToast(t("viz.node_save_success"), "success");
  }
}

function autoLayoutTopology(): void {
  if (topologyNodes.value.length === 0) {
    showToast(t("viz.no_nodes_to_layout"), "warning");
    return;
  }
  const rooms = groupTopologyNodes(visibleTopologyNodes.value, topologyGroupLabels());
  applyGroupedLayout(rooms);
  topologyFitKey.value += 1;
  void saveTopologyLayout();
}

async function autoDiscoverTopologyFromToolbar(): Promise<void> {
  const result = await autoDiscoverTopology();
  if (!result.success) {
    showToast(`${t("viz.auto_discover_failed")}: ${result.message}`, "error");
    return;
  }
  await loadTopology();
  const added = result.data?.added_nodes ?? 0;
  const discovered = result.data?.discovered_connections ?? 0;
  showToast(
    `${t("viz.auto_discover_done", { count: added })}，${discovered} ${t("viz.connections_unit")}`,
    "success",
  );
}

function toggleConnectionMode(): void {
  connectionMode.value = !connectionMode.value;
  topologyCanvas.value?.setConnectionMode(connectionMode.value);
}

/** 同设备对（含方向翻转）即重复：与模态入口同口径 */
function isDuplicatePair(sourceDeviceId: string, targetDeviceId: string): boolean {
  return topologyConnections.value.some(
    (connection) =>
      (connection.source_device_id === sourceDeviceId && connection.target_device_id === targetDeviceId) ||
      (connection.source_device_id === targetDeviceId && connection.target_device_id === sourceDeviceId),
  );
}

async function onConnectionComplete(
  sourceDeviceId: string,
  sourcePortId: string | null,
  targetDeviceId: string,
  targetPortId: string | null,
): Promise<void> {
  if (isDuplicatePair(sourceDeviceId, targetDeviceId)) {
    showToast(t("server.visualization.connection_duplicate"), "warning");
    return;
  }
  const result = await createTopologyConnection({
    connection_type: "physical",
    source_device_id: sourceDeviceId,
    target_device_id: targetDeviceId,
    source_device_port_id: sourcePortId,
    target_device_port_id: targetPortId,
    label: null,
    source_port_ids: null,
    target_port_ids: null,
  });
  if (!result.success) {
    showToast(`${t("viz.link_create_failed")}: ${result.message}`, "error");
    return;
  }
  showToast(t("viz.link_create_success"), "success");
  await loadTopology();
}

/** 连线弹窗创建成功：关闭弹窗并重载（连线端点位置以最新数据为准） */
async function onConnectionCreated(): Promise<void> {
  connectionCreateOpen.value = false;
  await loadTopology();
}

function onZoomSelectChange(event: Event): void {
  // 注意："current" 是回显当前的动态档位，不代表用户选择，直接忽略
  const value = (event.target as HTMLSelectElement).value;
  if (value === "fit") {
    topologyCanvas.value?.fitView();
    return;
  }
  if (value === "current") {
    return;
  }
  const scale = Number(value);
  if (Number.isFinite(scale) && scale > 0) {
    topologyCanvas.value?.setZoom(scale);
  }
}

// ==================== 拓扑弹窗 ====================

const connectionCreateOpen = ref(false);
const connectionDetail = ref<TopologyConnection | null>(null);
const devicePanel = reactive({
  open: false,
  deviceId: "",
  deviceName: "",
  position: null as CanvasPosition,
});
const containerCoord = reactive({
  open: false,
  kind: "room" as ContainerKind,
  key: "",
  label: "",
  x: 0,
  y: 0,
});

function openDeviceDetail(deviceId: string): void {
  const node = topologyNodes.value.find((item) => item.device_id === deviceId);
  devicePanel.deviceId = deviceId;
  devicePanel.deviceName = node?.device_name || deviceId;
  devicePanel.position = node ? { x: node.rect.x, y: node.rect.y } : null;
  devicePanel.open = true;
}

function onDevicePositionSave(x: number, y: number): void {
  const node = topologyNodes.value.find((item) => item.device_id === devicePanel.deviceId);
  if (!node) {
    return;
  }
  node.rect = {
    ...node.rect,
    x: clampNonNegative(Math.round(snapToGrid(x))),
    y: clampNonNegative(Math.round(snapToGrid(y))),
  };
  if (separateTopologyGroups(roomGroupKey(node.room_id), null)) {
    topologyFitKey.value += 1;
  }
  scheduleTopologyAutoSave();
}

function openContainerCoord(kind: ContainerKind, key: string, box: CanvasRect): void {
  containerCoord.open = true;
  containerCoord.kind = kind;
  containerCoord.key = key;
  containerCoord.label = findGroupLabel(kind, key);
  containerCoord.x = Math.round(box.x);
  containerCoord.y = Math.round(box.y);
}

function onContainerCoordSave(x: number, y: number): void {
  const { kind, key } = containerCoord;
  containerCoord.open = false;
  const members = visibleTopologyNodes.value.filter((node) =>
    kind === "cabinet" ? cabinetGroupKeyOf(node) === key : roomGroupKey(node.room_id) === key,
  );
  const moved = moveGroupTo(
    members,
    kind === "room" ? ROOM_PADDING : CABINET_PADDING,
    kind === "room" ? ROOM_HEADER : CABINET_HEADER,
    x,
    y,
  );
  if (!moved) {
    return;
  }
  topologyFitKey.value += 1;
  void saveTopologyLayout();
}

function onConnectionClick(connectionId: string): void {
  const connection = topologyConnections.value.find((item) => item.id === connectionId);
  connectionDetail.value = connection ?? null;
}

async function onConnectionDeleted(): Promise<void> {
  connectionDetail.value = null;
  await loadTopology();
}

/** 设备详情面板请求移除：交给确认弹窗，确认后才真正删除 */
function requestRemoveDevice(): void {
  confirmDeviceId.value = devicePanel.deviceId;
  confirmTarget.value = "remove-device";
}

async function removeTopologyDevice(deviceId: string): Promise<void> {
  const result = await deleteTopologyNode(deviceId);
  if (!result.success) {
    showToast(`${t("viz.node_delete_failed")}: ${result.message}`, "error");
    return;
  }
  showToast(t("viz.device_removed"), "success");
  devicePanel.open = false;
  await loadTopology();
}

// ==================== 确认弹窗 ====================

type ConfirmTarget = "workstation-layout" | "cabinet-layout" | "remove-device";

const confirmTarget = ref<ConfirmTarget | null>(null);
const confirmDeviceId = ref("");

const confirmMessage = computed(() => {
  switch (confirmTarget.value) {
    case "workstation-layout":
      return t("viz.confirm_delete_workstation_layout");
    case "cabinet-layout":
      return t("viz.confirm_delete_cabinet_layout");
    case "remove-device":
      return t("viz.confirm_remove_device");
    default:
      return "";
  }
});

async function runConfirm(): Promise<void> {
  const target = confirmTarget.value;
  confirmTarget.value = null;
  if (target === "workstation-layout") {
    await deleteWorkstationLayoutConfirmed();
  } else if (target === "cabinet-layout") {
    await deleteCabinetLayoutConfirmed();
  } else if (target === "remove-device") {
    await removeTopologyDevice(confirmDeviceId.value);
  }
}

async function deleteWorkstationLayoutConfirmed(): Promise<void> {
  if (!workstationRoomId.value) {
    showToast(t("viz.select_room_first"), "warning");
    return;
  }
  const result = await deleteWorkstationLayout(workstationRoomId.value);
  if (!result.success) {
    showToast(`${t("viz.layout_delete_failed")}: ${result.message}`, "error");
    return;
  }
  clearWorkstationCanvas();
  showToast(t("viz.layout_delete_success"), "success");
}

async function deleteCabinetLayoutConfirmed(): Promise<void> {
  if (!cabinetRoomId.value) {
    showToast(t("viz.select_room_first"), "warning");
    return;
  }
  const result = await deleteCabinetLayout(cabinetRoomId.value);
  if (!result.success) {
    showToast(`${t("viz.layout_delete_failed")}: ${result.message}`, "error");
    return;
  }
  clearCabinetCanvas();
  showToast(t("viz.layout_delete_success"), "success");
}

// ==================== 资源编辑弹窗 ====================

const workstationModal = reactive({
  open: false,
  workstation: null as Workstation | null,
  position: null as CanvasPosition,
});
const cabinetModal = reactive({ open: false, cabinet: null as CabinetWithNetworks | null });
const cabinetPositionModal = reactive({ open: false, position: null as CabinetPosition | null });

async function openWorkstationEditor(id: string, position: CanvasPosition = null): Promise<void> {
  try {
    const result = await apiGet<Workstation>(`/api/resources/workstations/${encodeURIComponent(id)}`);
    if (!result.success) {
      showToast(`${t("workstation.load_failed")}: ${result.message}`, "error");
      return;
    }
    workstationModal.workstation = result.data;
    workstationModal.position = position;
    workstationModal.open = true;
  } catch (error) {
    console.error("加载工位详情失败:", error);
    showToast(t("workstation.load_failed"), "error");
  }
}

async function openCabinetEditor(id: string): Promise<void> {
  try {
    const result = await apiGet<CabinetWithNetworks>(`/api/resources/cabinets/${encodeURIComponent(id)}`);
    if (!result.success || !result.data) {
      showToast(`${t("cabinet.load_failed")}: ${result.message}`, "error");
      return;
    }
    cabinetModal.cabinet = result.data;
    cabinetModal.open = true;
  } catch (error) {
    console.error("加载机柜详情失败:", error);
    showToast(t("cabinet.load_failed"), "error");
  }
}

async function openCabinetPositionEditor(id: string): Promise<void> {
  try {
    const result = await apiGet<CabinetPosition>(`/api/resources/positions/${encodeURIComponent(id)}`);
    if (!result.success) {
      showToast(`${t("cabinet_position.load_failed")}: ${result.message}`, "error");
      return;
    }
    cabinetPositionModal.position = result.data;
    cabinetPositionModal.open = true;
  } catch (error) {
    console.error("加载机位详情失败:", error);
    showToast(t("cabinet_position.load_failed"), "error");
  }
}

/**
 * 工位弹窗保存：携带坐标时先把画布元素移到该位置并静默落库；
 * 名称与所属房间可能同时被修改，随后按当前房间重载以同步标签与归属
 * （工位被移到其他房间时，节点随重载从本画布移除）。
 */
async function onWorkstationSaved(payload: WorkstationSavedPayload): Promise<void> {
  const node = workstationNodes.value.find((item) => item.id === payload.id);
  if (node && payload.x !== null && payload.y !== null) {
    node.rect = {
      ...node.rect,
      x: clampNonNegative(Math.round(snapToGrid(payload.x))),
      y: clampNonNegative(Math.round(snapToGrid(payload.y))),
    };
    await saveWorkstationLayout({ silent: true });
  }
  if (workstationRoomId.value) {
    await loadWorkstationRoom(workstationRoomId.value);
  }
}

// ==================== 初始化 ====================

let initialized = false;

async function initPage(): Promise<void> {
  if (initialized) {
    return;
  }
  initialized = true;
  await loadOrganizationTree();
  applyDefaultWorkstationOrg();
  await Promise.all([refreshWorkstationRooms(), refreshCabinetRooms()]);
  if (activeTab.value === "global-visualization") {
    await loadTopology();
  }
}

onMounted(() => {
  void initPage();
});

// 保底：页面卸载时清掉未落库的防抖计时器（避免离开页面后又发保存请求）
onBeforeUnmount(() => {
  if (workstationAutoSaveTimer !== null) {
    window.clearTimeout(workstationAutoSaveTimer);
  }
  if (topologyAutoSaveTimer !== null) {
    window.clearTimeout(topologyAutoSaveTimer);
  }
  if (cabinetRelayoutTimer !== null) {
    window.clearTimeout(cabinetRelayoutTimer);
  }
});
</script>

<template>
  <section
    id="visualization"
    class="content-section active visualization-page"
    :aria-label="t('nav.visualization')"
  >
    <div class="tabs" role="tablist">
      <button
        v-for="tab in TABS"
        :key="tab.id"
        type="button"
        class="tab-btn"
        :class="{ active: activeTab === tab.id }"
        role="tab"
        :aria-selected="activeTab === tab.id"
        :aria-controls="tab.id"
        @click="switchTab(tab.id)"
      >
        {{ t(tab.titleKey) }}
      </button>
    </div>

    <!-- 工位：拖动即自动保存（600ms 防抖） -->
    <div
      id="workstation-visualization"
      class="tab-content"
      :class="{ active: activeTab === 'workstation-visualization' }"
      role="tabpanel"
    >
      <div class="toolbar">
        <select
          class="form-control"
          :aria-label="t('organization.filter_label')"
          :value="workstationOrgId"
          @change="onWorkstationOrgChange"
        >
          <option value="">{{ t('organization.select_org') }}</option>
          <option v-for="org in orgOptions" :key="org.id" :value="org.id">
            {{ organizationOptionLabel(org) }}
          </option>
        </select>
        <select
          class="form-control"
          :aria-label="t('room.type')"
          :value="workstationRoomType"
          @change="onWorkstationRoomTypeChange"
        >
          <option v-for="option in WORKSTATION_ROOM_TYPES" :key="option.value" :value="option.value">
            {{ t(option.labelKey) }}
          </option>
        </select>
        <select
          class="form-control"
          :aria-label="t('room.select_room')"
          :value="workstationRoomId"
          @change="onWorkstationRoomChange"
        >
          <option value="">{{ t('room.select_room') }}</option>
          <option v-for="room in workstationRooms" :key="room.id" :value="room.id">
            {{ room.name }}
          </option>
        </select>
        <button type="button" class="btn btn-primary btn-sm" @click="autoDrawWorkstationFromToolbar">
          {{ t('visualization.auto_draw') }}
        </button>
        <button type="button" class="btn btn-secondary btn-sm" @click="saveWorkstationLayout()">
          {{ t('visualization.save_layout') }}
        </button>
        <button
          type="button"
          class="btn btn-danger btn-sm"
          @click="confirmTarget = 'workstation-layout'"
        >
          {{ t('visualization.delete_layout') }}
        </button>
      </div>
      <WorkstationCanvas
        v-if="workstationDoor"
        :nodes="workstationNodes"
        :door="workstationDoor"
        :fit-key="workstationFitKey"
        @node-click="openWorkstationEditor"
        @node-commit="onWorkstationNodeCommit"
        @door-commit="onDoorCommit"
      />
      <div v-else class="visualization-canvas visualization-canvas-scroll" />
    </div>

    <!-- 机柜：机位由柜体 + U 区间推导，拖动柜体会员自动跟随 -->
    <div
      id="cabinet-visualization"
      class="tab-content"
      :class="{ active: activeTab === 'cabinet-visualization' }"
      role="tabpanel"
    >
      <div class="toolbar">
        <select
          class="form-control"
          :aria-label="t('organization.filter_label')"
          :value="cabinetOrgId"
          @change="onCabinetOrgChange"
        >
          <option value="">{{ t('organization.select_org') }}</option>
          <option v-for="org in orgOptions" :key="org.id" :value="org.id">
            {{ organizationOptionLabel(org) }}
          </option>
        </select>
        <select
          class="form-control"
          :aria-label="t('room.type')"
          :value="cabinetRoomType"
          @change="onCabinetRoomTypeChange"
        >
          <option v-for="option in CABINET_ROOM_TYPES" :key="option.value" :value="option.value">
            {{ t(option.labelKey) }}
          </option>
        </select>
        <select
          class="form-control"
          :aria-label="t('room.select_room')"
          :value="cabinetRoomId"
          @change="onCabinetRoomChange"
        >
          <option value="">{{ t('room.select_room') }}</option>
          <option v-for="room in cabinetRooms" :key="room.id" :value="room.id">
            {{ room.name }}
          </option>
        </select>
        <button type="button" class="btn btn-primary btn-sm" @click="autoDrawCabinetFromToolbar">
          {{ t('visualization.auto_draw') }}
        </button>
        <button type="button" class="btn btn-danger btn-sm" @click="confirmTarget = 'cabinet-layout'">
          {{ t('visualization.delete_layout') }}
        </button>
        <button type="button" class="btn btn-secondary btn-sm" @click="saveCabinetLayout()">
          {{ t('visualization.save_layout') }}
        </button>
      </div>
      <CabinetCanvas
        :nodes="cabinetNodes"
        :total-width="cabinetTotalWidth"
        :canvas-height="cabinetCanvasHeight"
        :fit-key="cabinetFitKey"
        @resize="onCabinetResize"
        @cabinet-click="openCabinetEditor"
        @position-click="openCabinetPositionEditor"
        @cabinet-commit="onCabinetCommit"
      />
    </div>

    <!-- 拓扑：拖动落定后 500ms 防抖保存；连线经连线模式或创建弹窗录入 -->
    <div
      id="global-visualization"
      class="tab-content"
      :class="{ active: activeTab === 'global-visualization' }"
      role="tabpanel"
    >
      <div class="toolbar">
        <select
          class="form-control"
          :aria-label="t('organization.filter_label')"
          :value="topologyOrgId"
          @change="onTopologyOrgChange"
        >
          <option value="">{{ t('organization.select_org') }}</option>
          <option v-for="org in orgOptions" :key="org.id" :value="org.id">
            {{ organizationOptionLabel(org) }}
          </option>
        </select>
        <button
          type="button"
          class="btn btn-warning btn-sm"
          :class="{ 'active-mode': connectionMode }"
          :aria-pressed="connectionMode"
          @click="toggleConnectionMode"
        >
          {{ t('visualization.connection_mode') }}
        </button>
        <button type="button" class="btn btn-primary btn-sm" @click="connectionCreateOpen = true">
          {{ t('viz.create_connection') }}
        </button>
        <button type="button" class="btn btn-info btn-sm" @click="autoDiscoverTopologyFromToolbar">
          {{ t('visualization.auto_discover') }}
        </button>
        <button type="button" class="btn btn-primary btn-sm" @click="autoLayoutTopology">
          {{ t('visualization.auto_layout') }}
        </button>
        <button type="button" class="btn btn-secondary btn-sm" @click="saveTopologyLayout()">
          {{ t('visualization.save_layout') }}
        </button>
        <div class="zoom-controls">
          <button
            type="button"
            class="btn btn-secondary btn-sm"
            :aria-label="t('viz.zoom_out')"
            :data-tooltip="t('viz.zoom_out')"
            @click="topologyCanvas?.zoomBy(0.8)"
          >
            &minus;
          </button>
          <select
            class="form-control zoom-select"
            :aria-label="t('viz.zoom_level')"
            :value="zoomSelectValue"
            @change="onZoomSelectChange"
          >
            <option v-if="zoomSelectValue === 'current'" value="current">{{ zoomPercent }}%</option>
            <option v-for="scale in ZOOM_PRESETS" :key="scale" :value="String(scale)">
              {{ zoomOptionLabel(scale) }}
            </option>
            <option value="fit">{{ t('viz.zoom_fit') }}</option>
          </select>
          <button
            type="button"
            class="btn btn-secondary btn-sm"
            :aria-label="t('viz.zoom_in')"
            :data-tooltip="t('viz.zoom_in')"
            @click="topologyCanvas?.zoomBy(1.25)"
          >
            +
          </button>
        </div>
      </div>
      <TopologyCanvas
        ref="topologyCanvas"
        :nodes="visibleTopologyNodes"
        :connections="visibleTopologyConnections"
        :fit-key="topologyFitKey"
        @node-click="openDeviceDetail"
        @node-drag-end="onTopologyNodeDragEnd"
        @container-click="openContainerCoord"
        @container-drag-end="onTopologyContainerDragEnd"
        @connection-click="onConnectionClick"
        @connection-complete="onConnectionComplete"
        @canvas-click="connectionDetail = null"
        @zoom-change="onZoomChange"
      />
    </div>

    <!-- 拓扑弹窗宿主 -->
    <ConnectionCreateModal
      v-if="connectionCreateOpen"
      :nodes="topologyNodes"
      :connections="topologyConnections"
      @created="onConnectionCreated"
      @close="connectionCreateOpen = false"
    />
    <ConnectionDetailModal
      v-if="connectionDetail"
      :connection="connectionDetail"
      @deleted="onConnectionDeleted"
      @close="connectionDetail = null"
    />
    <DeviceDetailPanel
      v-if="devicePanel.open"
      :device-id="devicePanel.deviceId"
      :device-name="devicePanel.deviceName"
      :position="devicePanel.position"
      @save-position="onDevicePositionSave"
      @remove="requestRemoveDevice"
      @close="devicePanel.open = false"
    />
    <ContainerCoordModal
      v-if="containerCoord.open"
      :kind="containerCoord.kind"
      :label="containerCoord.label"
      :position="{ x: containerCoord.x, y: containerCoord.y }"
      @save="onContainerCoordSave"
      @close="containerCoord.open = false"
    />

    <!-- 资源编辑弹窗宿主 -->
    <WorkstationModal
      v-if="workstationModal.open"
      :workstation="workstationModal.workstation"
      :position="workstationModal.position"
      @saved="onWorkstationSaved"
      @close="workstationModal.open = false"
    />
    <CabinetModal
      v-if="cabinetModal.open"
      :cabinet="cabinetModal.cabinet"
      @saved="reloadCabinetRoom"
      @close="cabinetModal.open = false"
    />
    <PositionModal
      v-if="cabinetPositionModal.open"
      :position="cabinetPositionModal.position"
      @saved="reloadCabinetRoom"
      @close="cabinetPositionModal.open = false"
    />

    <!-- 统一确认弹窗：删除布局 / 移除拓扑设备 -->
    <ConfirmModal
      v-if="confirmTarget"
      :message="confirmMessage"
      danger
      @confirm="runConfirm"
      @cancel="confirmTarget = null"
    />
  </section>
</template>
