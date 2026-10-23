<script setup lang="ts">
/**
 * 设备新增/编辑弹窗（移植自旧版 modals/device/device-modal.html +
 * device.js openDeviceModal/submitDeviceForm/fillDeviceFormForEdit +
 * deviceSnmp.js 测试/取信息）：
 * - 基础字段：名称/类型/模板/序列号/主机名（只读，SNMP 拉取）/品牌/型号/
 *   经销商/位置/功耗（W|kW）/描述；
 * - 位置级联：组织/房型 → 房间 → 工位与机柜（互斥）→ 机位（依赖机柜）；
 * - 网卡配置：NetworkCardEditor 三级嵌套（网卡 → 网口 → IP）；
 * - SNMP 双版本：v2c 团体字 / v3 用户名+认证+加密，密码掩码回显；
 * - 底部：测试 SNMP / 从 SNMP 获取信息 / 保存。
 * 编辑回显：先取房间详情对齐组织/房型筛选，再五路并行加载下拉与房间网段。
 */
import { onMounted, reactive, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useMutation, useQueryClient } from "@tanstack/vue-query";
import { apiGet, apiPost, apiPut } from "../../api/client";
import { showToast } from "../../composables/useToast";
import { DEVICE_TYPE_OPTIONS } from "../../config/deviceOptions";
import { readPowerWattsInput } from "../../utils/power";
import ModalShell from "../ModalShell.vue";
import NavIcon from "../NavIcon.vue";
import NetworkCardEditor from "./NetworkCardEditor.vue";
import DeviceTemplateModal from "./DeviceTemplateModal.vue";
import type { ApiResult, Paged } from "../../types/api";
import type { CabinetPosition } from "../../types/api/generated/CabinetPosition";
import type { DeviceInterface } from "../../types/api/generated/DeviceInterface";
import type { DeviceTemplate } from "../../types/api/generated/DeviceTemplate";
import type { DeviceTemplateSummary } from "../../types/api/generated/DeviceTemplateSummary";
import type { DeviceUpdate } from "../../types/api/generated/DeviceUpdate";
import type { DeviceWithDetails } from "../../types/api/generated/DeviceWithDetails";
import type { IpDetail } from "../../types/api/generated/IpDetail";
import type { CabinetWithNetworks } from "../../types/api/generated/CabinetWithNetworks";
import type { NetworkCard } from "../../types/api/generated/NetworkCard";
import type { NetworkCardSyncItem } from "../../types/api/generated/NetworkCardSyncItem";
import type { OrganizationTreeNode } from "../../types/api/generated/OrganizationTreeNode";
import type { PortSyncItem } from "../../types/api/generated/PortSyncItem";
import type { RoomWithNetworks } from "../../types/api/generated/RoomWithNetworks";
import type { SnmpTestRequest } from "../../types/api/generated/SnmpTestRequest";
import type { WorkstationWithDetails } from "../../types/api/generated/WorkstationWithDetails";

/** GET /devices/{id} 的网卡树（fetch_device_network_config 组装：网卡 → 网口 → IP 明细） */
type DeviceCardPort = DeviceInterface & { ips: IpDetail[] };
type DeviceCard = NetworkCard & { ports: DeviceCardPort[] };

/** 网口提交载荷：附加 device_managed（设备表单网口一律托管；后端 PortSyncItem 未声明该键） */
type PortSyncPayload = PortSyncItem & { device_managed: boolean };

/** 网卡提交载荷（网口行携带 device_managed 附加键） */
type CardSyncPayload = Omit<NetworkCardSyncItem, "ports"> & { ports: PortSyncPayload[] };

const props = withDefaults(
  defineProps<{
    /** null = 新增；对象 = 编辑回显（GET /devices/{id} 完整数据，含 cards） */
    device?: (DeviceWithDetails & { cards?: DeviceCard[] }) | null;
  }>(),
  { device: null }
);

const emit = defineEmits<{ close: []; saved: [] }>();

const { t } = useI18n();
const queryClient = useQueryClient();

// ==================== 常量选项 ====================

const ROOM_TYPE_FILTER_OPTIONS: { value: string; labelKey: string }[] = [
  { value: "office", labelKey: "room.type_office" },
  { value: "lobby", labelKey: "room.type_lobby" },
  { value: "reception", labelKey: "room.type_reception" },
  { value: "data_center", labelKey: "room.type_datacenter" },
  { value: "telecom_closet", labelKey: "room.type_telecom_closet" },
  { value: "other", labelKey: "room.type_other" }
];

/** SNMP 密码字段回显掩码：非空即显示圆点，提交时 maskToNull 还原 */
const PASSWORD_MASK = "••••••••";

function maskToNull(value: string): string | null {
  if (!value || value === PASSWORD_MASK || value.trim() === "") {
    return null;
  }
  return value;
}

/**
 * SNMP 掩码字段的提交载荷（双层 Option 语义）：掩码=用户未改动（返回
 * undefined，JSON.stringify 省略键，后端按键缺失处理=保留原值）；空串=
 * 主动清空（null，后端 SET NULL）；有值=更新新值。掩码误发 null 会把
 * 已存配置清空（设备 SNMP 信息"无法保存"的根因）。
 */
function snmpSecretPayload(value: string): string | null | undefined {
  if (value === PASSWORD_MASK) {
    return undefined;
  }
  return maskToNull(value);
}

// ==================== 表单状态 ====================

/** 表单状态；snmpPort/powerWatts 为数字输入 v-model，清空时为空串 */
interface DeviceFormState {
  name: string;
  hostname: string;
  deviceType: string;
  templateId: string;
  saveAsTemplate: boolean;
  templateName: string;
  serialNumber: string;
  brand: string;
  model: string;
  seller: string;
  location: string;
  powerWatts: number | "";
  powerUnit: string;
  orgId: string;
  roomType: string;
  roomId: string;
  workstationId: string;
  cabinetId: string;
  positionId: string;
  description: string;
  snmpVersion: string;
  snmpPort: number | "";
  snmpCommunity: string;
  snmpUsername: string;
  snmpAuthProtocol: string;
  snmpAuthPassword: string;
  snmpPrivProtocol: string;
  snmpPrivPassword: string;
}

const form = reactive<DeviceFormState>({
  name: "",
  hostname: "",
  deviceType: "desktop",
  templateId: "",
  saveAsTemplate: false,
  templateName: "",
  serialNumber: "",
  brand: "",
  model: "",
  seller: "",
  location: "",
  powerWatts: "",
  powerUnit: "W",
  orgId: "",
  roomType: "",
  roomId: "",
  workstationId: "",
  cabinetId: "",
  positionId: "",
  description: "",
  snmpVersion: "v2c",
  snmpPort: 161,
  snmpCommunity: "",
  snmpUsername: "",
  snmpAuthProtocol: "",
  snmpAuthPassword: "",
  snmpPrivProtocol: "",
  snmpPrivPassword: ""
});

const cardEditor = ref<InstanceType<typeof NetworkCardEditor> | null>(null);
const templateModalOpen = ref(false);

/** 模板引用断言：NetworkCardEditor 为同步子组件，挂载后引用必然就绪 */
function editor(): InstanceType<typeof NetworkCardEditor> {
  return cardEditor.value as InstanceType<typeof NetworkCardEditor>;
}

// ==================== 下拉选项与级联加载 ====================

// 设备模板选项（编辑回显与模板自动填充共用）
const templateOptions = ref<DeviceTemplateSummary[]>([]);

async function loadTemplateOptions(): Promise<void> {
  const result = await apiGet<{ items?: DeviceTemplateSummary[] } | DeviceTemplateSummary[]>(
    "/api/resources/device-templates"
  );
  const data = result.success ? result.data : null;
  templateOptions.value = Array.isArray(data) ? data : Array.isArray(data?.items) ? data.items : [];
}

// 组织树展平为带缩进深度的列表（下拉展示用）
interface OrgOptionRow {
  id: string;
  name: string;
  orgType: string;
  depth: number;
}

const orgOptions = ref<OrgOptionRow[]>([]);

async function loadOrgOptions(): Promise<void> {
  const result = await apiGet<OrganizationTreeNode[]>("/api/resources/organizations/tree");
  const flat: OrgOptionRow[] = [];
  const walk = (nodes: OrganizationTreeNode[] | null, depth: number): void => {
    for (const node of nodes || []) {
      flat.push({ id: node.id, name: node.name, orgType: node.org_type, depth });
      if (node.children && node.children.length > 0) {
        walk(node.children, depth + 1);
      }
    }
  };
  if (result.success && result.data) {
    walk(result.data, 0);
  }
  orgOptions.value = flat;
}

const roomOptions = ref<RoomWithNetworks[]>([]);
const workstationOptions = ref<WorkstationWithDetails[]>([]);
const cabinetOptions = ref<CabinetWithNetworks[]>([]);
const positionOptions = ref<CabinetPosition[]>([]);

// 级联加载序号：快速切换过滤条件时旧响应晚到不得覆盖新选项
let roomsSeq = 0;
let workstationsSeq = 0;
let cabinetsSeq = 0;
let positionsSeq = 0;

/** 兼容数组与分页两种载荷的条目提取 */
function extractItems<T>(result: ApiResult<T[] | Paged<T>>): T[] {
  const data = result.success ? result.data : null;
  if (Array.isArray(data)) {
    return data;
  }
  return Array.isArray(data?.items) ? data.items : [];
}

/** 按组织/房型过滤加载房间选项；原选中值失效时重置（旧版 fillSelect 语义） */
async function reloadRooms(): Promise<void> {
  const seq = ++roomsSeq;
  const params = new URLSearchParams({ page_size: "1000" });
  if (form.orgId) {
    params.set("org_id", form.orgId);
  }
  if (form.roomType) {
    params.set("room_type", form.roomType);
  }
  const result = await apiGet<Paged<RoomWithNetworks>>(`/api/resources/rooms?${params}`);
  if (seq !== roomsSeq) {
    return;
  }
  roomOptions.value = extractItems(result);
  if (form.roomId && !roomOptions.value.some((room) => room.id === form.roomId)) {
    form.roomId = "";
  }
}

/** 工位必属于房间：未选定房间时不发请求，仅展示"请先选择房间"占位 */
async function reloadWorkstations(roomId: string = form.roomId): Promise<void> {
  const seq = ++workstationsSeq;
  if (!roomId) {
    workstationOptions.value = [];
    return;
  }
  const result = await apiGet<Paged<WorkstationWithDetails>>(
    `/api/resources/workstations?room_id=${roomId}&page_size=1000`
  );
  if (seq !== workstationsSeq) {
    return;
  }
  workstationOptions.value = extractItems(result);
  if (form.workstationId && !workstationOptions.value.some((ws) => ws.id === form.workstationId)) {
    form.workstationId = "";
  }
}

/** 机柜必属于房间：未选定房间时不发请求 */
async function reloadCabinets(roomId: string = form.roomId): Promise<void> {
  const seq = ++cabinetsSeq;
  if (!roomId) {
    cabinetOptions.value = [];
    return;
  }
  const result = await apiGet<Paged<CabinetWithNetworks>>(
    `/api/resources/cabinets?room_id=${roomId}&page_size=1000`
  );
  if (seq !== cabinetsSeq) {
    return;
  }
  cabinetOptions.value = extractItems(result);
  if (form.cabinetId && !cabinetOptions.value.some((cab) => cab.id === form.cabinetId)) {
    form.cabinetId = "";
  }
}

/** 机位必须先选定机柜；roomId 仅用于回显无机柜的历史机位；都无时仅占位 */
async function reloadPositions(
  { cabinetId = null, roomId = null }: { cabinetId?: string | null; roomId?: string | null } = {}
): Promise<void> {
  const seq = ++positionsSeq;
  if (!cabinetId && !roomId) {
    positionOptions.value = [];
    return;
  }
  const params = new URLSearchParams({ page_size: "1000" });
  if (cabinetId) {
    params.set("cabinet_id", cabinetId);
  } else if (roomId) {
    // 前置守卫保证 cabinetId 为空时 roomId 必有值
    params.set("room_id", roomId);
  }
  const result = await apiGet<Paged<CabinetPosition>>(`/api/resources/positions?${params}`);
  if (seq !== positionsSeq) {
    return;
  }
  positionOptions.value = extractItems(result);
  if (form.positionId && !positionOptions.value.some((pos) => pos.id === form.positionId)) {
    form.positionId = "";
  }
}

/** 组织/房型筛选变化：按条件重载房间选项并重载下游级联 */
async function reloadDeviceRooms(): Promise<void> {
  await reloadRooms();
  await reloadWorkstations();
  await reloadCabinets();
  if (form.cabinetId || form.roomId) {
    await reloadPositions(form.cabinetId ? { cabinetId: form.cabinetId } : { roomId: form.roomId });
  } else {
    positionOptions.value = [];
  }
}

function onRoomChange(): void {
  // 房间变更：工位/机柜按新房间重载，机位依赖机柜选择一并重置
  reloadWorkstations();
  reloadCabinets();
  form.positionId = "";
  positionOptions.value = [];
}

function onCabinetChange(): void {
  // 机柜变更：重置机位并按机柜重载
  form.positionId = "";
  reloadPositions(form.cabinetId ? { cabinetId: form.cabinetId } : {});
}

// 工位与机位互斥：选中一方时清空另一方
function onWorkstationChange(): void {
  if (form.workstationId) {
    form.positionId = "";
  }
}

function onPositionChange(): void {
  if (form.positionId) {
    form.workstationId = "";
  }
}

/** 模板选择变化：拉取模板详情自动填充类型/品牌/型号 */
async function onTemplateChange(): Promise<void> {
  if (!form.templateId) {
    return;
  }
  const result = await apiGet<DeviceTemplate>(`/api/resources/device-templates/${form.templateId}`);
  if (result.success && result.data) {
    const tmpl = result.data;
    if (tmpl.device_type) {
      form.deviceType = tmpl.device_type;
    }
    if (tmpl.brand) {
      form.brand = tmpl.brand;
    }
    if (tmpl.model) {
      form.model = tmpl.model;
    }
  }
}

// ==================== 编辑回显 ====================

function setSnmpFieldValues(device: DeviceWithDetails): void {
  form.snmpVersion = device.snmp_version || "v2c";
  form.snmpPort = device.snmp_port || 161;
  form.snmpCommunity = device.snmp_community ? PASSWORD_MASK : "";
  form.snmpUsername = device.snmp_username || "";
  form.snmpAuthProtocol = device.snmp_auth_protocol || "";
  form.snmpAuthPassword = device.snmp_auth_password ? PASSWORD_MASK : "";
  form.snmpPrivProtocol = device.snmp_priv_protocol || "";
  form.snmpPrivPassword = device.snmp_priv_password ? PASSWORD_MASK : "";
}

/** 编辑路径回显：基础字段 → 房间上下文（下拉五路并行）→ 网卡数据 */
async function fillDeviceFormForEdit(
  device: DeviceWithDetails & { cards?: DeviceCard[] }
): Promise<void> {
  form.name = device.name || "";
  form.hostname = device.hostname || "";
  form.deviceType = device.device_type || "other";
  form.brand = device.brand || "";
  form.model = device.model || "";
  form.seller = device.seller || "";
  form.location = device.location || "";
  form.powerWatts = device.power_watts ?? "";
  form.powerUnit = "W";
  form.serialNumber = device.serial_number || "";
  form.description = device.description || "";
  form.templateId = device.template_id || "";
  setSnmpFieldValues(device);

  if (!device.room_id) {
    await editor().loadExisting(device.cards || []);
    return;
  }

  // 编辑回显：按房间所属组织/类型对齐筛选条件后重载房间列表，确保目标房间在列
  const roomResult = await apiGet<RoomWithNetworks>(`/api/resources/rooms/${device.room_id}`);
  const room = roomResult.success && roomResult.data ? roomResult.data : null;
  if (!room) {
    // 房间详情加载失败：提示错误并联动清空房间/工位/机柜/机位选择，
    // 避免回显出"有机位无房间"的不一致数据（用户可重新选择后保存）
    showToast(t("common.load_failed"), "error");
    form.roomId = "";
    form.workstationId = "";
    form.cabinetId = "";
    form.positionId = "";
    await editor().loadExisting(device.cards || []);
    return;
  }

  if (room.org_id) {
    form.orgId = room.org_id;
  }
  if (room.room_type) {
    form.roomType = room.room_type.toLowerCase();
  }

  // 房间值先就位（网卡编辑器按 roomId 拉取房间网段），五路并行加载下拉
  form.roomId = device.room_id;
  // 机位下拉：优先按机柜过滤；历史数据机位未挂机柜时按房间回退；否则全量占位
  const positionsLoad = device.cabinet_id
    ? reloadPositions({ cabinetId: device.cabinet_id })
    : device.position_id
      ? reloadPositions({ roomId: device.room_id })
      : reloadPositions({});
  await Promise.all([
    reloadRooms(),
    reloadWorkstations(device.room_id),
    reloadCabinets(device.room_id),
    positionsLoad
  ]);

  if (device.cabinet_id) {
    form.cabinetId = device.cabinet_id;
  }
  if (device.workstation_id) {
    form.workstationId = device.workstation_id;
  }
  if (device.position_id) {
    form.positionId = device.position_id;
  }

  await editor().loadExisting(device.cards || []);
}

// ==================== SNMP 测试 / 信息拉取 ====================

/** 从网卡编辑器收集首个已填 IP（SNMP 请求只使用首个 IP） */
function firstCardIpAddress(): string | null {
  const { cards } = editor().collectData();
  for (const card of cards) {
    for (const port of card.ports) {
      for (const ip of port.ips) {
        if (ip.ip_address) {
          return ip.ip_address;
        }
      }
    }
  }
  return null;
}

const testingSnmp = ref(false);

async function testSnmpConnection(): Promise<void> {
  const ip = firstCardIpAddress();
  if (!ip) {
    showToast(t("device.ip_required"), "warning");
    return;
  }
  testingSnmp.value = true;
  try {
    // SnmpTestRequest 全键必填（均可空）：显式 null 与键缺省在后端 serde Option 下等价
    const data: SnmpTestRequest = {
      device_id: props.device?.id ?? null,
      ip_address: ip,
      snmp_version: form.snmpVersion,
      snmp_community: null,
      snmp_username: null,
      snmp_auth_protocol: null,
      snmp_auth_password: null,
      snmp_priv_protocol: null,
      snmp_priv_password: null,
      snmp_port: Number.parseInt(String(form.snmpPort), 10) || 161
    };
    if (form.snmpVersion === "v3") {
      data.snmp_username = form.snmpUsername || null;
      data.snmp_auth_protocol = form.snmpAuthProtocol || null;
      const authPassword = maskToNull(form.snmpAuthPassword);
      if (authPassword) {
        data.snmp_auth_password = authPassword;
      }
      data.snmp_priv_protocol = form.snmpPrivProtocol || null;
      const privPassword = maskToNull(form.snmpPrivPassword);
      if (privPassword) {
        data.snmp_priv_password = privPassword;
      }
    } else {
      const community = maskToNull(form.snmpCommunity);
      if (community) {
        data.snmp_community = community;
      }
    }
    const result = await apiPost("/api/resources/devices/test-snmp", data);
    if (result.success) {
      showToast(t("device.snmp_test_success"), "success");
    } else {
      showToast(`${t("device.snmp_test_failed")}: ${result.message}`, "error");
    }
  } finally {
    testingSnmp.value = false;
  }
}

const fetchingSnmp = ref(false);

/** GET /devices/{id}/snmp-info 载荷 */
interface SnmpInfo {
  brand: string | null;
  model: string | null;
  hostname: string | null;
}

async function getDeviceInfoFromSnmp(): Promise<void> {
  if (!props.device?.id) {
    // 未保存设备：有 IP 时提示先保存，无 IP 时提示先填 IP
    if (!firstCardIpAddress()) {
      showToast(t("device.ip_required"), "warning");
      return;
    }
    showToast(t("device.save_before_snmp"), "warning");
    return;
  }
  fetchingSnmp.value = true;
  try {
    const result = await apiGet<SnmpInfo>(`/api/resources/devices/${props.device.id}/snmp-info`);
    if (result.success && result.data) {
      const info = result.data;
      if (info.brand) {
        form.brand = info.brand;
      }
      if (info.model) {
        form.model = info.model;
      }
      // 主机名只读，仅由 SNMP 拉取填充
      if (info.hostname) {
        form.hostname = info.hostname;
      }
      showToast(t("device.snmp_info_success"), "success");
    } else {
      showToast(`${t("device.snmp_info_failed")}: ${result.message}`, "error");
    }
  } finally {
    fetchingSnmp.value = false;
  }
}

// ==================== 提交 ====================

const saving = ref(false);

const saveMutation = useMutation({
  // PUT 走 DeviceUpdate 双层 Option 语义（键缺失=不修改、null=清空、
  // 值=更新，掩码键经 snmpSecretPayload 归一），POST 复用同一键集
  // （DeviceCreate 为单层 Option，null/缺省均不配置）。template_id 为
  // 原代码即发送的附加键（后端 DeviceUpdate 未声明，serde 静默忽略）。
  mutationFn: async ({
    cards,
    powerWatts
  }: {
    cards: CardSyncPayload[];
    powerWatts: number | null;
  }) => {
    // 三个掩码键放宽为可缺省：undefined 经 JSON.stringify 省略键，后端
    // 双层 Option 按键缺失处理（不修改）；null 仍表示主动清空
    const deviceData: Omit<DeviceUpdate, "snmp_community" | "snmp_auth_password" | "snmp_priv_password"> & {
      template_id: string | null;
      snmp_community?: string | null;
      snmp_auth_password?: string | null;
      snmp_priv_password?: string | null;
    } = {
      name: form.name.trim(),
      hostname: form.hostname || null,
      device_type: form.deviceType || "other",
      brand: form.brand.trim() || null,
      model: form.model.trim() || null,
      seller: form.seller.trim() || null,
      location: form.location.trim() || null,
      serial_number: form.serialNumber.trim() || null,
      template_id: form.templateId || null,
      workstation_id: form.workstationId || null,
      position_id: form.positionId || null,
      room_id: form.roomId || null,
      description: form.description.trim() || null,
      save_as_template: form.saveAsTemplate,
      template_name: form.saveAsTemplate ? form.templateName.trim() || form.name.trim() : null,
      snmp_version: form.snmpVersion || "v2c",
      snmp_port: Number.parseInt(String(form.snmpPort), 10) || 161,
      power_watts: powerWatts,
      snmp_community: snmpSecretPayload(form.snmpCommunity),
      snmp_username: form.snmpUsername || null,
      snmp_auth_protocol: form.snmpAuthProtocol || null,
      snmp_auth_password: snmpSecretPayload(form.snmpAuthPassword),
      snmp_priv_protocol: form.snmpPrivProtocol || null,
      snmp_priv_password: snmpSecretPayload(form.snmpPrivPassword),
      cards
    };
    const result = props.device?.id
      ? await apiPut(`/api/resources/devices/${props.device.id}`, deviceData)
      : await apiPost("/api/resources/devices", deviceData);
    if (!result.success) {
      throw new Error(result.message || t("common.failed"));
    }
    return result.data;
  },
  onSuccess: () => {
    queryClient.invalidateQueries({ queryKey: ["devices"] });
    showToast(t("device.save_success"), "success");
    emit("saved");
    emit("close");
  },
  onError: (error) => {
    showToast(`${t("common.operation_failed")}: ${error.message}`, "error");
  },
  onSettled: () => {
    saving.value = false;
  }
});

/** 提交前校验（名称/工位机位互斥/网卡/IP 功耗），全部通过后进入 mutation */
function submit(): void {
  if (saving.value) {
    return;
  }
  if (!form.name.trim()) {
    showToast(t("device.name_required"), "warning");
    return;
  }
  if (form.workstationId && form.positionId) {
    showToast(t("device.position_mutual_exclusive"), "warning");
    return;
  }
  const { cards, errors: cardErrors } = editor().collectData();
  if (cardErrors.length > 0) {
    showToast(cardErrors.join("\n"), "warning");
    return;
  }
  // 功耗：空串=不记录（null）；按所选单位换算为瓦后校验，非法值直接拦截
  const powerWatts = readPowerWattsInput(form.powerWatts, form.powerUnit);
  if (Number.isNaN(powerWatts)) {
    showToast(t("device.power_watts_invalid"), "warning");
    return;
  }
  saving.value = true;
  saveMutation.mutate({ cards, powerWatts });
}

// ==================== 初始化 ====================

onMounted(async () => {
  // 模板下拉、组织下拉、房间选项（新增路径）互不依赖，并行加载
  const tasks: Promise<void>[] = [loadTemplateOptions(), loadOrgOptions()];
  if (!props.device?.room_id) {
    tasks.push(reloadRooms());
  }
  await Promise.all(tasks);
  if (props.device) {
    await fillDeviceFormForEdit(props.device);
  } else {
    editor().resetForAdd();
  }
});
</script>

<template>
  <ModalShell
    size="lg"
    :title="props.device ? t('device.edit') : t('device.add')"
    @close="emit('close')"
  >
    <form @submit.prevent="submit">
      <div class="form-row">
        <div class="form-group">
          <label for="device-name">
            <span>{{ t("device.name") }}</span><span class="required">*</span>
          </label>
          <input id="device-name" v-model="form.name" type="text" autocomplete="off" required>
        </div>
        <div class="form-group">
          <label for="device-type">{{ t("device.device_type") }}</label>
          <select id="device-type" v-model="form.deviceType">
            <option v-for="opt in DEVICE_TYPE_OPTIONS" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-template-id">{{ t("device.template") }}</label>
          <select id="device-template-id" v-model="form.templateId" @change="onTemplateChange">
            <option value="">{{ t("device.select_template") }}</option>
            <option v-for="tmpl in templateOptions" :key="tmpl.id" :value="tmpl.id">
              {{ tmpl.name }}
            </option>
          </select>
        </div>
        <div class="form-group form-group-inline">
          <label for="device-save-as-template">{{ t("device.save_as_template") }}</label>
          <input id="device-save-as-template" v-model="form.saveAsTemplate" type="checkbox" class="form-checkbox">
          <button
            type="button"
            class="icon-btn btn-secondary btn-gap-left"
            :data-tooltip="t('device_template.manage')"
            :aria-label="t('device_template.manage')"
            @click="templateModalOpen = true"
          >
            <NavIcon name="list" />
          </button>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-serial-number">{{ t("device.serial_number") }}</label>
          <input id="device-serial-number" v-model="form.serialNumber" type="text" autocomplete="off">
        </div>
        <div class="form-group">
          <label
            for="device-hostname"
            :data-tooltip="t('device.hostname_placeholder')"
            :title="t('device.hostname_placeholder')"
          >
            {{ t("device.hostname") }}
          </label>
          <input
            id="device-hostname"
            v-model="form.hostname"
            type="text"
            autocomplete="off"
            readonly
            :placeholder="t('device.hostname_placeholder')"
          >
        </div>
      </div>
      <!-- 模板名称独占整行：勾选"保存为模板"时显示 -->
      <div class="form-row" :class="{ hidden: !form.saveAsTemplate }">
        <div class="form-group">
          <label for="device-template-name">{{ t("device.template_name") }}</label>
          <input
            id="device-template-name"
            v-model="form.templateName"
            type="text"
            autocomplete="off"
            :placeholder="t('device.template_name_placeholder')"
          >
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-brand">{{ t("device.brand") }}</label>
          <input id="device-brand" v-model="form.brand" type="text" autocomplete="off">
        </div>
        <div class="form-group">
          <label for="device-model">{{ t("device.model") }}</label>
          <input id="device-model" v-model="form.model" type="text" autocomplete="off">
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-seller">{{ t("device.seller") }}</label>
          <input id="device-seller" v-model="form.seller" type="text" autocomplete="off">
        </div>
        <div class="form-group">
          <label for="device-location">{{ t("device.location") }}</label>
          <input id="device-location" v-model="form.location" type="text" autocomplete="off">
        </div>
        <div class="form-group power-inline">
          <label for="device-power-watts">{{ t("device.power_input") }}</label>
          <div class="power-input-row">
            <input
              id="device-power-watts"
              v-model="form.powerWatts"
              type="number"
              min="0"
              step="any"
              autocomplete="off"
            >
            <select v-model="form.powerUnit" class="form-control" aria-label="Unit">
              <option value="W">W</option>
              <option value="kW">kW</option>
            </select>
          </div>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-org-id">{{ t("organization.parent_org") }}</label>
          <select id="device-org-id" v-model="form.orgId" @change="reloadDeviceRooms">
            <option value="">{{ t("organization.select_org") }}</option>
            <option v-for="org in orgOptions" :key="org.id" :value="org.id">
              {{ " ".repeat(org.depth * 4) + org.name }} ({{ org.orgType }})
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="device-room-type">{{ t("device.room_type_filter") }}</label>
          <select id="device-room-type" v-model="form.roomType" @change="reloadDeviceRooms">
            <option value="">{{ t("device.all_room_types") }}</option>
            <option v-for="opt in ROOM_TYPE_FILTER_OPTIONS" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-room-id">{{ t("device.room") }}</label>
          <select id="device-room-id" v-model="form.roomId" @change="onRoomChange">
            <option value="">{{ t("room.select_room") }}</option>
            <option v-for="room in roomOptions" :key="room.id" :value="room.id">
              {{ room.name }}
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="device-workstation-id">{{ t("device.workstation") }}</label>
          <select id="device-workstation-id" v-model="form.workstationId" @change="onWorkstationChange">
            <option value="">
              {{ form.roomId ? t("device.select_workstation") : t("device.select_room_first") }}
            </option>
            <option v-for="ws in workstationOptions" :key="ws.id" :value="ws.id">
              {{ ws.name }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label for="device-cabinet-id">{{ t("device.cabinet") }}</label>
          <select id="device-cabinet-id" v-model="form.cabinetId" @change="onCabinetChange">
            <option value="">
              {{ form.roomId ? t("device.select_cabinet") : t("device.select_room_first") }}
            </option>
            <option v-for="cab in cabinetOptions" :key="cab.id" :value="cab.id">
              {{ cab.name }}
            </option>
          </select>
        </div>
        <div class="form-group">
          <label for="device-position-id">{{ t("device.position") }}</label>
          <select id="device-position-id" v-model="form.positionId" @change="onPositionChange">
            <option value="">{{ t("device.select_position") }}</option>
            <option v-for="pos in positionOptions" :key="pos.id" :value="pos.id">
              {{ pos.name }}
            </option>
          </select>
        </div>
      </div>
      <div class="form-group form-group-block">
        <textarea
          v-model="form.description"
          :aria-label="t('common.description')"
          :placeholder="t('common.description_placeholder')"
        />
      </div>

      <hr class="section-divider">
      <h4 class="section-subtitle">{{ t("device.network_card_config") }}</h4>
      <NetworkCardEditor ref="cardEditor" :room-id="form.roomId" :initial-cards="props.device?.cards || []" />

      <hr class="section-divider">
      <h4 class="section-subtitle">{{ t("device.snmp_config") }}</h4>
      <div class="form-row">
        <div class="form-group">
          <label for="device-snmp-version">{{ t("device.snmp_version") }}</label>
          <select id="device-snmp-version" v-model="form.snmpVersion">
            <option value="v2c">{{ t("device.snmp_v2c") }}</option>
            <option value="v3">{{ t("device.snmp_v3") }}</option>
          </select>
        </div>
        <div class="form-group">
          <label for="device-snmp-port">{{ t("device.snmp_port") }}</label>
          <input id="device-snmp-port" v-model="form.snmpPort" type="number" autocomplete="off">
        </div>
      </div>
      <div v-show="form.snmpVersion !== 'v3'" class="form-row">
        <div class="form-group flex-1">
          <label for="device-snmp-community">{{ t("device.snmp_community") }}</label>
          <input
            id="device-snmp-community"
            v-model="form.snmpCommunity"
            type="text"
            placeholder="public"
            autocomplete="off"
          >
        </div>
      </div>
      <div v-show="form.snmpVersion === 'v3'">
        <div class="form-row">
          <div class="form-group">
            <label for="device-snmp-username">{{ t("device.snmp_username") }}</label>
            <input id="device-snmp-username" v-model="form.snmpUsername" type="text" autocomplete="off">
          </div>
          <div class="form-group">
            <label for="device-snmp-auth-protocol">{{ t("device.snmp_auth_protocol") }}</label>
            <select id="device-snmp-auth-protocol" v-model="form.snmpAuthProtocol">
              <option value="">{{ t("device.snmp_no_auth") }}</option>
              <option value="MD5">MD5</option>
              <option value="SHA">SHA</option>
              <option value="SHA-224">SHA-224</option>
              <option value="SHA-256">SHA-256</option>
              <option value="SHA-384">SHA-384</option>
              <option value="SHA-512">SHA-512</option>
            </select>
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="device-snmp-auth-password">{{ t("device.snmp_auth_password") }}</label>
            <input id="device-snmp-auth-password" v-model="form.snmpAuthPassword" type="password" autocomplete="new-password">
          </div>
          <div class="form-group">
            <label for="device-snmp-priv-protocol">{{ t("device.snmp_priv_protocol") }}</label>
            <select id="device-snmp-priv-protocol" v-model="form.snmpPrivProtocol">
              <option value="">{{ t("device.snmp_no_priv") }}</option>
              <option value="DES">DES</option>
              <option value="AES">AES</option>
              <option value="AES-128">AES-128</option>
              <option value="AES-192">AES-192</option>
              <option value="AES-256">AES-256</option>
            </select>
          </div>
        </div>
        <div class="form-row">
          <div class="form-group">
            <label for="device-snmp-priv-password">{{ t("device.snmp_priv_password") }}</label>
            <input id="device-snmp-priv-password" v-model="form.snmpPrivPassword" type="password" autocomplete="new-password">
          </div>
        </div>
      </div>
    </form>

    <template #footer>
      <button type="button" class="btn btn-secondary" :disabled="testingSnmp" @click="testSnmpConnection">
        {{ testingSnmp ? t("common.testing") : t("device.test_snmp") }}
      </button>
      <button type="button" class="btn btn-primary" :disabled="fetchingSnmp" @click="getDeviceInfoFromSnmp">
        {{ fetchingSnmp ? t("device.fetching_snmp") : t("device.get_snmp_info") }}
      </button>
      <button type="button" class="btn btn-primary" :disabled="saving" @click="submit">
        {{ saving ? t("common.saving") : t("common.save") }}
      </button>
    </template>

    <!-- 设备模板管理：层叠弹窗（模板变更后刷新模板下拉） -->
    <DeviceTemplateModal
      v-if="templateModalOpen"
      @close="templateModalOpen = false"
      @changed="loadTemplateOptions"
    />
  </ModalShell>
</template>
