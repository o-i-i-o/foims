<script setup lang="ts">
/**
 * 网卡配置编辑器（网卡 → 网口 → IP 三级嵌套动态行）。
 * - 房间上下文：watch roomId 拉取房间配置网段，IP 行的区域/网段下拉按房间过滤；
 * - 遗留数据：编辑回显中不属于房间配置的区域/网段以带标记选项保留显示，
 *   由 collectData 与后端校验共同拦截；
 * - 校验链：卡名/端口名必填、VLAN 1-4094、网段必选且属于房间、IP 合法且
 *   落在网段 CIDR 内、每网口至少一条有效 IP、每网卡至少一个网口；
 * - 自动分配：从所选网段的可用列表取第一个既未入库、也未在表单其他 IP 行
 *   填写的地址；候选在响应返回后再与表单比对，并发点击不会落到同一地址。
 * collectData / loadExisting / resetForAdd 经 defineExpose 供父组件调用。
 */
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet } from "../../api/client";
import { showToast } from "../../composables/useToast";
import { CARD_TYPES, INTERFACE_ROLES, PHYSICAL_TYPES } from "../../config/deviceOptions";
import { ipKey, isIPv6, isIpInCidr, isValidIP } from "../../utils/network";
import NavIcon from "../NavIcon.vue";
import type { DeviceInterface } from "../../types/api/generated/DeviceInterface";
import type { IpDetail } from "../../types/api/generated/IpDetail";
import type { IpSyncItem } from "../../types/api/generated/IpSyncItem";
import type { NetworkCard } from "../../types/api/generated/NetworkCard";
import type { NetworkCardSyncItem } from "../../types/api/generated/NetworkCardSyncItem";
import type { NetworkInfo } from "../../types/api/generated/NetworkInfo";
import type { PortSyncItem } from "../../types/api/generated/PortSyncItem";

/** GET /devices/{id} 的网卡树（fetch_device_network_config 组装：网卡 → 网口 → IP 明细） */
type DeviceCardPort = DeviceInterface & { ips: IpDetail[] };
type DeviceCard = NetworkCard & { ports: DeviceCardPort[] };

/** IP 提交载荷：附加 network_region_id（后端 IpSyncItem 未声明该键，serde 忽略；保持原请求体） */
type IpSyncPayload = IpSyncItem & { network_region_id?: string };

/** 网口提交载荷：附加 device_managed（设备表单网口一律托管；后端 PortSyncItem 未声明该键） */
type PortSyncPayload = PortSyncItem & { device_managed: boolean };

/** 网卡提交载荷（网口行携带 device_managed 附加键） */
type CardSyncPayload = Omit<NetworkCardSyncItem, "ports"> & { ports: PortSyncPayload[] };

const props = withDefaults(
  defineProps<{
    /** 设备表单房间下拉当前值：网卡 IP 的区域/网段选项按房间配置网段过滤 */
    roomId?: string;
    /** 编辑回显的原始 cards 数据（含 device_managed=false 的网口，加载时过滤） */
    initialCards?: DeviceCard[] | null;
  }>(),
  { roomId: "", initialCards: () => [] }
);

const { t } = useI18n();

const DEFAULT_PORT_NAME = "eth0";

// 房间上下文（区域/网段选项来源）

const roomNetworks = ref<NetworkInfo[]>([]);
// 遗留数据：不属于当前房间的区域/网段，保留显示并标记
const legacyRegionList = ref<{ id: string; name: string }[]>([]);
const legacyNetworkList = ref<{ id: string; name: string; network_region_id: string | null }[]>(
  []
);

// 请求序号：快速切换房间时旧响应晚到不得覆盖新上下文
let roomSeq = 0;
// setRoomContext 在途 Promise：loadExisting 需等待房间网段就绪后再登记遗留项
let roomContextReady: Promise<void> = Promise.resolve();

async function applyRoomContext(roomId: string): Promise<void> {
  const seq = ++roomSeq;
  roomNetworks.value = [];
  if (!roomId) {
    return;
  }
  const result = await apiGet<NetworkInfo[]>(`/api/resources/rooms/${roomId}/networks`);
  if (seq !== roomSeq) {
    return; // 响应期间已发起新的房间请求，丢弃过期结果
  }
  if (result.success && Array.isArray(result.data)) {
    roomNetworks.value = result.data;
  }
}

watch(
  () => props.roomId,
  (roomId) => {
    roomContextReady = applyRoomContext(roomId);
  },
  { immediate: true }
);

// 房间配置的区域（去重后 {id, name}，追加遗留区域去重保留）
const regionOptions = computed<{ id: string; name: string }[]>(() => {
  const map = new Map<string, { id: string; name: string }>();
  for (const n of roomNetworks.value) {
    const rid = n.network_region_id;
    if (rid && !map.has(rid)) {
      map.set(rid, { id: rid, name: n.network_region });
    }
  }
  const list = Array.from(map.values());
  for (const lr of legacyRegionList.value) {
    if (!list.some((r) => r.id === lr.id)) {
      list.push(lr);
    }
  }
  return list;
});

// 房间允许的网段 ID 集合
const roomSubnetIds = computed(() => new Set(roomNetworks.value.map((n) => n.id)));

// 房间区域 id 集合（遗留区域登记判定用）
const roomRegionIds = computed(
  () =>
    new Set(
      roomNetworks.value.map((n) => n.network_region_id).filter((id): id is string => Boolean(id))
    )
);

/** 网段下拉行：房间配置网段（NetworkInfo）或遗留网段（无 CIDR，仅展示与标记） */
interface NetworkOptionRow {
  id: string;
  name: string;
  network_region_id: string | null;
  ipv4_cidr?: string | null;
  ipv6_cidr?: string | null;
  /** 遗留网段标记（不属于房间配置） */
  legacy?: boolean;
}

// CIDR 校验用网段全集（房间网段 + 遗留网段）
const allNetworks = computed<NetworkOptionRow[]>(() => [
  ...roomNetworks.value,
  ...legacyNetworkList.value
]);

/** IP 行的网段下拉选项：房间配置网段 + 遗留网段，按 IP 行所选区域过滤 */
function networkOptionsFor(ipRow: IpRowState): NetworkOptionRow[] {
  const roomRows: NetworkOptionRow[] = roomNetworks.value.map((n) => ({
    id: n.id,
    name: n.name,
    network_region_id: n.network_region_id,
    ipv4_cidr: n.ipv4_cidr,
    ipv6_cidr: n.ipv6_cidr
  }));
  const legacyRows: NetworkOptionRow[] = legacyNetworkList.value.map((n) => ({
    ...n,
    legacy: true
  }));
  const rows = [...roomRows, ...legacyRows];
  // 未选区域时展示全部选项（含遗留），选择后仅保留该区域的网段
  if (!ipRow.regionId) {
    return rows;
  }
  return rows.filter((n) => n.network_region_id === ipRow.regionId);
}

/** 区域下拉占位文案：未选房间时引导先选房间 */
function regionPlaceholder(): string {
  return props.roomId ? t("network.select_region") : t("device.select_room_first");
}

/** 网段选项展示文本：名称 (CIDR)，遗留网段追加不属于房间的标记 */
function networkOptionLabel(n: NetworkOptionRow, legacy = false): string {
  const cidrs: string[] = [];
  if (n.ipv4_cidr) {
    cidrs.push(n.ipv4_cidr);
  }
  if (n.ipv6_cidr) {
    cidrs.push(n.ipv6_cidr);
  }
  const cidrStr = cidrs.length > 0 ? ` (${cidrs.join(" / ")})` : "";
  const mark = legacy ? ` [${t("device.network_not_in_room")}]` : "";
  return `${n.name}${cidrStr}${mark}`;
}

// 行模型（key 供 v-for 稳定渲染）

interface IpRowState {
  key: number;
  regionId: string;
  subnetId: string;
  ipAddress: string;
  /** 自动分配请求进行中（防连点覆盖） */
  assigning: boolean;
}

interface PortRowState {
  key: number;
  id: string | null;
  name: string;
  physicalType: string;
  interfaceRole: string;
  macAddress: string;
  /** 数字输入 v-model：清空时为空串 */
  vlanId: number | "";
  description: string;
  ips: IpRowState[];
}

interface CardRowState {
  key: number;
  id: string | null;
  name: string;
  cardType: string;
  description: string;
  ports: PortRowState[];
}

let nextKey = 1;

const cards = reactive<CardRowState[]>([]);

function emptyIpRow(): IpRowState {
  return {
    key: nextKey++,
    regionId: "",
    subnetId: "",
    ipAddress: "",
    assigning: false
  };
}

/** 新网口默认名：扫描表单全部网口名，取未占用的 ethN（重名会被后端 409 拒绝） */
function nextDefaultPortName(): string {
  const used = new Set<string>();
  for (const card of cards) {
    for (const port of card.ports) {
      const value = port.name.trim();
      if (value) {
        used.add(value);
      }
    }
  }
  let index = 0;
  while (used.has(`${DEFAULT_PORT_NAME}${index}`)) {
    index++;
  }
  return `${DEFAULT_PORT_NAME}${index}`;
}

function emptyPortRow(): PortRowState {
  return {
    key: nextKey++,
    id: null,
    name: nextDefaultPortName(),
    physicalType: "rj45",
    interfaceRole: "business",
    macAddress: "",
    vlanId: "",
    description: "",
    ips: [emptyIpRow()]
  };
}

function emptyCardRow(): CardRowState {
  return {
    key: nextKey++,
    id: null,
    name: t("device.default_card_name"),
    cardType: "pcie",
    description: "",
    ports: [emptyPortRow()]
  };
}

// 行增删（每级至少保留一行）

function addCard(): void {
  cards.push(emptyCardRow());
}

function removeCard(index: number): void {
  if (cards.length <= 1) {
    showToast(t("device.at_least_one_card"), "warning");
    return;
  }
  cards.splice(index, 1);
}

function addPort(card: CardRowState): void {
  card.ports.push(emptyPortRow());
}

function removePort(card: CardRowState, index: number): void {
  if (card.ports.length <= 1) {
    showToast(t("device.at_least_one_port"), "warning");
    return;
  }
  card.ports.splice(index, 1);
}

function addIp(port: PortRowState): void {
  port.ips.push(emptyIpRow());
}

function removeIp(port: PortRowState, index: number): void {
  if (port.ips.length <= 1) {
    showToast(t("device.at_least_one_ip"), "warning");
    return;
  }
  port.ips.splice(index, 1);
}

// 遗留数据登记（编辑回显）

function registerLegacy(ipData: IpDetail): void {
  if (
    ipData.network_region_id &&
    !roomRegionIds.value.has(ipData.network_region_id) &&
    !legacyRegionList.value.some((r) => r.id === ipData.network_region_id)
  ) {
    legacyRegionList.value.push({
      id: ipData.network_region_id,
      name: ipData.network_region || ipData.network_region_id
    });
  }
  if (
    ipData.subnet_id &&
    !roomSubnetIds.value.has(ipData.subnet_id) &&
    !legacyNetworkList.value.some((n) => n.id === ipData.subnet_id)
  ) {
    legacyNetworkList.value.push({
      id: ipData.subnet_id,
      name: ipData.network_name || ipData.subnet_id,
      network_region_id: ipData.network_region_id || null
    });
  }
}

function buildCardRow(cardData: DeviceCard, portsData: DeviceCardPort[]): CardRowState {
  const card: CardRowState = {
    key: nextKey++,
    id: cardData.id || null,
    name: cardData.name || t("device.default_card_name"),
    cardType: cardData.card_type || "pcie",
    description: cardData.description || "",
    ports: []
  };
  for (const portData of portsData || []) {
    const port: PortRowState = {
      key: nextKey++,
      id: portData.id || null,
      name: portData.name || nextDefaultPortName(),
      physicalType: portData.physical_type || "rj45",
      interfaceRole: portData.interface_role || "business",
      macAddress: portData.mac_address || "",
      vlanId: portData.vlan_id ?? "",
      description: portData.description || "",
      ips: []
    };
    for (const ipData of portData.ips || []) {
      registerLegacy(ipData);
      port.ips.push({
        key: nextKey++,
        regionId: ipData.network_region_id || "",
        subnetId: ipData.subnet_id || "",
        ipAddress: ipData.ip_address || "",
        assigning: false
      });
    }
    if (port.ips.length === 0) {
      port.ips.push(emptyIpRow());
    }
    card.ports.push(port);
  }
  return card;
}

// 对外接口（父组件调用）

/** 新增路径：清空遗留记录并放置一张空网卡 */
function resetForAdd(): void {
  legacyRegionList.value = [];
  legacyNetworkList.value = [];
  cards.splice(0, cards.length, emptyCardRow());
}

/**
 * 编辑路径回显：等待房间上下文就绪后登记遗留区域/网段并构建行模型。
 * 仅加载托管网口（device_managed=false 的 SNMP 同步网口不在设备表单展示）；
 * 空网卡（板卡）整体跳过；全部被过滤时回退为一张空网卡。
 */
async function loadExisting(cardsData: DeviceCard[] | null): Promise<void> {
  await roomContextReady;
  legacyRegionList.value = [];
  legacyNetworkList.value = [];
  const managedCards = (cardsData || []).filter((card) =>
    (card.ports || []).some((p) => p.device_managed !== false)
  );

  cards.splice(0, cards.length);
  if (managedCards.length === 0) {
    cards.push(emptyCardRow());
    return;
  }
  for (const cardData of managedCards) {
    // 端口过滤结果经参数传入，不改动接口响应对象
    cards.push(
      buildCardRow(
        cardData,
        (cardData.ports || []).filter((p) => p.device_managed !== false)
      )
    );
  }
}

/**
 * 收集校验后的提交数据：返回 { cards, errors }。
 * 完全空置的 IP 行（网段/地址/描述全空）静默跳过；其余逐项校验。
 */
function collectData(): { cards: CardSyncPayload[]; errors: string[] } {
  if (!props.roomId) {
    return { cards: [], errors: [t("device.select_room_first")] };
  }
  const out: CardSyncPayload[] = [];
  const errors: string[] = [];
  cards.forEach((card, cardIdx) => {
    const cardNum = cardIdx + 1;
    const cardName = card.name.trim();
    if (!cardName) {
      errors.push(`${t("device.network_card")} ${cardNum}: ${t("device.card_name_required")}`);
      return;
    }

    const ports: PortSyncPayload[] = [];
    card.ports.forEach((port, portIdx) => {
      const portNum = portIdx + 1;
      const portName = port.name.trim();
      const portVlan = port.vlanId === "" ? null : Number.parseInt(String(port.vlanId), 10);

      if (!portName) {
        errors.push(
          `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum}: ${t("device.port_name_required")}`
        );
        return;
      }
      if (portVlan !== null && (Number.isNaN(portVlan) || portVlan < 1 || portVlan > 4094)) {
        errors.push(
          `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum}: ${t("device.vlan_invalid")}`
        );
        return;
      }

      const ips: IpSyncPayload[] = [];
      port.ips.forEach((ipRow, ipIdx) => {
        const ipNum = ipIdx + 1;
        const subnetId = ipRow.subnetId || null;
        const ipAddress = ipRow.ipAddress.trim();

        if (!subnetId && !ipAddress) {
          return; // 全空行跳过
        }
        if (!subnetId) {
          errors.push(
            `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum} - IP ${ipNum}: ${t("device.network_required")}`
          );
          return;
        }
        if (!roomSubnetIds.value.has(subnetId)) {
          errors.push(
            `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum} - IP ${ipNum}: ${t("device.network_not_in_room")}`
          );
          return;
        }
        if (!ipAddress) {
          errors.push(
            `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum} - IP ${ipNum}: ${t("device.ip_required")}`
          );
          return;
        }
        if (!isValidIP(ipAddress)) {
          errors.push(
            `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum} - IP ${ipNum}: ${t("device.ip_invalid")}`
          );
          return;
        }

        const network = allNetworks.value.find((n) => n.id === subnetId);
        const cidr = network ? (isIPv6(ipAddress) ? network.ipv6_cidr : network.ipv4_cidr) : null;
        if (!cidr) {
          const ipType = isIPv6(ipAddress) ? "IPv6" : "IPv4";
          errors.push(
            `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum} - IP ${ipNum}: ${t("device.cidr_not_supported")} ${ipType} ${t("device.address")}`
          );
          return;
        }
        if (!isIpInCidr(ipAddress, cidr)) {
          errors.push(
            `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum} - IP ${ipNum}: ${t("device.ip_not_in_cidr")} ${cidr}`
          );
          return;
        }

        const ipData: IpSyncPayload = {
          // 同步语义整体替换，原请求即无 id 键；显式 null 与缺键在 serde Option 下等价
          id: null,
          subnet_id: subnetId,
          ip_address: ipAddress
        };
        if (ipRow.regionId) {
          ipData.network_region_id = ipRow.regionId;
        }
        ips.push(ipData);
      });

      if (ips.length === 0) {
        errors.push(
          `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum}: ${t("device.at_least_one_ip")}`
        );
      }

      ports.push({
        id: port.id,
        name: portName,
        physical_type: port.physicalType || "rj45",
        interface_role: port.interfaceRole || "business",
        mac_address: port.macAddress.trim() || null,
        vlan_id: portVlan,
        description: port.description.trim() || null,
        // 设备表单新建/编辑的网口一律托管（表单不展示该复选框）
        device_managed: true,
        ips
      });
    });

    if (ports.length === 0) {
      errors.push(`${t("device.network_card")} ${cardNum}: ${t("device.at_least_one_port")}`);
    }

    out.push({
      id: card.id,
      name: cardName,
      card_type: card.cardType || "pcie",
      description: card.description.trim() || null,
      ports
    });
  });

  return { cards: out, errors };
}

// 自动分配 IP

/**
 * 表单内已填写的地址键（限同网段，排除本次要改写的行）。
 * 分配结果在保存前不入库，后端可用列表只排除已入库地址，
 * 因此同一容器内的多个网卡/网口必须在此补齐去重。
 */
function formUsedIpKeys(subnetId: string, except: IpRowState): Set<string> {
  const used = new Set<string>();
  for (const card of cards) {
    for (const port of card.ports) {
      for (const row of port.ips) {
        const address = row.ipAddress.trim();
        if (row !== except && row.subnetId === subnetId && address) {
          used.add(ipKey(address));
        }
      }
    }
  }
  return used;
}

async function autoAssignIp(ipRow: IpRowState): Promise<void> {
  if (!ipRow.subnetId) {
    showToast(t("device.auto_assign_select_network"), "warning");
    return;
  }
  if (ipRow.assigning) {
    return; // 请求期间禁用：连点时慢的旧响应后到会覆盖新分配的结果
  }
  ipRow.assigning = true;
  try {
    const result = await apiGet<{ available_ips?: string[] }>(
      `/api/resources/ip/available/${encodeURIComponent(ipRow.subnetId)}`
    );
    // 响应返回后再取表单占用集合：并发点击不同 IP 行时，先返回的分配结果
    // 已写入表单，后返回者据此跳过，避免两行落到同一地址
    const used = formUsedIpKeys(ipRow.subnetId, ipRow);
    const first = result.success
      ? ((result.data?.available_ips || []).find((ip) => !used.has(ipKey(ip))) ?? null)
      : null;
    if (first) {
      ipRow.ipAddress = first;
      showToast(t("device.auto_assign_success", { ip: first }), "success");
    } else {
      showToast(t("device.auto_assign_none"), "warning");
    }
  } finally {
    ipRow.assigning = false;
  }
}

// 房间上下文变化后清洗选择：区域/网段不在新选项集中时重置
// （“值不在选项集中即失效”语义）
watch([regionOptions, roomNetworks], () => {
  for (const card of cards) {
    for (const port of card.ports) {
      for (const ipRow of port.ips) {
        if (ipRow.regionId && !regionOptions.value.some((r) => r.id === ipRow.regionId)) {
          ipRow.regionId = "";
        }
        if (ipRow.subnetId && !networkOptionsFor(ipRow).some((n) => n.id === ipRow.subnetId)) {
          ipRow.subnetId = "";
        }
      }
    }
  }
});

defineExpose({ collectData, loadExisting, resetForAdd });
</script>

<template>
  <div>
    <div v-for="(card, cardIdx) in cards" :key="card.key" class="network-card-item">
      <header class="card-level-bar">
        <h3 :id="`nc-card-${card.key}`" class="level-badge level-card">{{ t("device.network_card") }}</h3>
        <div class="level-actions">
          <button
            type="button"
            class="icon-btn btn-danger"
            :data-tooltip="t('device.delete_network_card')"
            :aria-label="t('device.delete_network_card')"
            @click="removeCard(cardIdx)"
          >
            <NavIcon name="trash" />
          </button>
          <button
            type="button"
            class="icon-btn btn-success"
            :data-tooltip="t('device.add_network_card')"
            :aria-label="t('device.add_network_card')"
            @click="addCard"
          >
            <NavIcon name="plus" />
          </button>
        </div>
      </header>
      <div class="nc-fields">
        <div class="nc-field">
          <label :for="`nc-card-name-${card.key}`">
            {{ t("device.network_card_name") }}<span class="required" aria-hidden="true">*</span>
          </label>
          <input
            :id="`nc-card-name-${card.key}`"
            v-model="card.name"
            type="text"
            class="nc-input"
            :placeholder="t('device.network_card_name')"
            autocomplete="off"
            required
          >
        </div>
        <div class="nc-field">
          <label :for="`nc-card-type-${card.key}`">{{ t("device.network_card_type") }}</label>
          <select :id="`nc-card-type-${card.key}`" v-model="card.cardType" class="nc-input">
            <option v-for="opt in CARD_TYPES" :key="opt.value" :value="opt.value">
              {{ t(opt.labelKey) }}
            </option>
          </select>
        </div>
        <div class="nc-field">
          <label :for="`nc-card-desc-${card.key}`">{{ t("common.description") }}</label>
          <input
            :id="`nc-card-desc-${card.key}`"
            v-model="card.description"
            type="text"
            class="nc-input"
            autocomplete="off"
          >
        </div>
      </div>

      <div v-for="(port, portIdx) in card.ports" :key="port.key" class="nc-port-item">
        <header class="port-level-bar">
          <h3 :id="`nc-port-${port.key}`" class="level-badge level-port">{{ t("device.network_port") }}</h3>
          <div class="level-actions">
            <button
              type="button"
              class="icon-btn btn-danger"
              :data-tooltip="t('device.delete_network_port')"
              :aria-label="t('device.delete_network_port')"
              @click="removePort(card, portIdx)"
            >
              <NavIcon name="trash" />
            </button>
            <button
              type="button"
              class="icon-btn btn-success"
              :data-tooltip="t('device.add_network_port')"
              :aria-label="t('device.add_network_port')"
              @click="addPort(card)"
            >
              <NavIcon name="plus" />
            </button>
          </div>
        </header>
        <div class="nc-fields">
          <div class="nc-field">
            <label :for="`nc-port-name-${port.key}`">
              {{ t("device.network_port_name") }}<span class="required" aria-hidden="true">*</span>
            </label>
            <input
              :id="`nc-port-name-${port.key}`"
              v-model="port.name"
              type="text"
              class="nc-input"
              :placeholder="t('device.network_port_name')"
              autocomplete="off"
              required
            >
          </div>
          <div class="nc-field">
            <label :for="`nc-port-ptype-${port.key}`">{{ t("device.physical_type") }}</label>
            <select :id="`nc-port-ptype-${port.key}`" v-model="port.physicalType" class="nc-input">
              <option v-for="opt in PHYSICAL_TYPES" :key="opt.value" :value="opt.value">
                {{ t(opt.labelKey) }}
              </option>
            </select>
          </div>
          <div class="nc-field">
            <label :for="`nc-port-role-${port.key}`">{{ t("device.interface_role") }}</label>
            <select :id="`nc-port-role-${port.key}`" v-model="port.interfaceRole" class="nc-input">
              <option v-for="opt in INTERFACE_ROLES" :key="opt.value" :value="opt.value">
                {{ t(opt.labelKey) }}
              </option>
            </select>
          </div>
          <div class="nc-field">
            <label :for="`nc-port-vlan-${port.key}`">{{ t("device.vlan_id") }}</label>
            <input
              :id="`nc-port-vlan-${port.key}`"
              v-model="port.vlanId"
              type="number"
              class="nc-input"
              min="1"
              max="4094"
              autocomplete="off"
            >
          </div>
          <div class="nc-field">
            <label :for="`nc-port-mac-${port.key}`">{{ t("ip.mac_address") }}</label>
            <input
              :id="`nc-port-mac-${port.key}`"
              v-model="port.macAddress"
              type="text"
              class="nc-input"
              placeholder="00:11:22:33:44:55"
              autocomplete="off"
            >
          </div>
          <div class="nc-field">
            <label :for="`nc-port-desc-${port.key}`">{{ t("common.description") }}</label>
            <input
              :id="`nc-port-desc-${port.key}`"
              v-model="port.description"
              type="text"
              class="nc-input"
              autocomplete="off"
            >
          </div>
        </div>

        <div class="port-ips-container" :aria-label="t('device.ip_list')">
          <div v-for="ipRow in port.ips" :key="ipRow.key" class="nc-ip-item">
            <header class="ip-level-bar">
              <h3 :id="`nc-ip-${ipRow.key}`" class="level-badge level-ip">IP</h3>
              <div class="level-actions">
                <button
                  type="button"
                  class="icon-btn btn-danger"
                  :data-tooltip="t('ip.delete_ip')"
                  :aria-label="t('ip.delete_ip')"
                  @click="removeIp(port, port.ips.indexOf(ipRow))"
                >
                  <NavIcon name="trash" />
                </button>
                <button
                  type="button"
                  class="icon-btn btn-success"
                  :data-tooltip="t('ip.add_ip')"
                  :aria-label="t('ip.add_ip')"
                  @click="addIp(port)"
                >
                  <NavIcon name="plus" />
                </button>
              </div>
            </header>
            <div class="nc-fields">
              <div class="nc-field">
                <label :for="`nc-ip-region-${ipRow.key}`">{{ t("network.region") }}</label>
                <select :id="`nc-ip-region-${ipRow.key}`" v-model="ipRow.regionId" class="nc-input">
                  <option value="">{{ regionPlaceholder() }}</option>
                  <option v-for="region in regionOptions" :key="region.id" :value="region.id">
                    {{ region.name }}
                  </option>
                </select>
              </div>
              <div class="nc-field">
                <label :for="`nc-ip-network-${ipRow.key}`">
                  {{ t("network.name") }}<span class="required" aria-hidden="true">*</span>
                </label>
                <select
                  :id="`nc-ip-network-${ipRow.key}`"
                  v-model="ipRow.subnetId"
                  class="nc-input"
                  required
                >
                  <option value="">{{ t("network.select_network") }}</option>
                  <option
                    v-for="network in networkOptionsFor(ipRow)"
                    :key="network.id"
                    :value="network.id"
                  >
                    {{ networkOptionLabel(network, network.legacy) }}
                  </option>
                </select>
              </div>
              <div class="nc-field">
                <label :for="`nc-ip-address-${ipRow.key}`">
                  {{ t("ip.ip_address") }}<span class="required" aria-hidden="true">*</span>
                </label>
                <div class="nc-address-group">
                  <input
                    :id="`nc-ip-address-${ipRow.key}`"
                    v-model="ipRow.ipAddress"
                    type="text"
                    class="nc-input"
                    placeholder="192.168.1.100"
                    autocomplete="off"
                    required
                  >
                  <button
                    type="button"
                    class="icon-btn btn-primary"
                    :data-tooltip="t('device.auto_assign_ip')"
                    :aria-label="t('device.auto_assign_ip')"
                    :disabled="ipRow.assigning"
                    @click="autoAssignIp(ipRow)"
                  >
                    <NavIcon name="zap" />
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
