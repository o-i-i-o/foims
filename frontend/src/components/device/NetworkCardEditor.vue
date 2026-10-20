<script setup>
/**
 * 网卡配置编辑器（移植自旧版 utils/networkCardManager.js 的 DOM 动态构建：
 * 网卡 → 网口 → IP 三级嵌套动态行）。
 * - 房间上下文：watch roomId 拉取房间配置网段，IP 行的区域/网段下拉按房间过滤；
 * - 遗留数据：编辑回显中不属于房间配置的区域/网段以带标记选项保留显示，
 *   由 collectData 与后端校验共同拦截；
 * - 校验链：卡名/端口名必填、VLAN 1-4094、网段必选且属于房间、IP 合法且
 *   落在网段 CIDR 内、每网口至少一条有效 IP、每网卡至少一个网口；
 * - 自动分配：从所选网段取第一个未使用 IP 填入地址框。
 * collectData / loadExisting / resetForAdd 经 defineExpose 供父组件调用。
 */
import { computed, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { apiGet } from "../../api/client.js";
import { showToast } from "../../composables/useToast.js";
import { CARD_TYPES, INTERFACE_ROLES, PHYSICAL_TYPES } from "../../config/deviceOptions.js";
import { isIPv6, isIpInCidr, isValidIP } from "../../utils/network.js";
import NavIcon from "../NavIcon.vue";

const props = defineProps({
  /** 设备表单房间下拉当前值：网卡 IP 的区域/网段选项按房间配置网段过滤 */
  roomId: { type: String, default: "" },
  /** 编辑回显的原始 cards 数据（含 device_managed=false 的网口，加载时过滤） */
  initialCards: { type: Array, default: () => [] }
});

const { t } = useI18n();

const DEFAULT_PORT_NAME = "eth0";

// ==================== 房间上下文（区域/网段选项来源） ====================

const roomNetworks = ref([]);
// 遗留数据：不属于当前房间的区域/网段，保留显示并标记
const legacyRegionList = ref([]);
const legacyNetworkList = ref([]);

// 请求序号：快速切换房间时旧响应晚到不得覆盖新上下文
let roomSeq = 0;
// setRoomContext 在途 Promise：loadExisting 需等待房间网段就绪后再登记遗留项
let roomContextReady = Promise.resolve();

async function applyRoomContext(roomId) {
  const seq = ++roomSeq;
  roomNetworks.value = [];
  if (!roomId) {
    return;
  }
  const result = await apiGet(`/api/resources/rooms/${roomId}/networks`);
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
const regionOptions = computed(() => {
  const map = new Map();
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
  () => new Set(roomNetworks.value.map((n) => n.network_region_id).filter(Boolean))
);

const networksByRegion = computed(() => {
  const map = new Map();
  for (const n of roomNetworks.value) {
    const rid = n.network_region_id;
    if (!map.has(rid)) {
      map.set(rid, []);
    }
    map.get(rid).push(n);
  }
  return map;
});

// CIDR 校验用网段全集（房间网段 + 遗留网段）
const allNetworks = computed(() => [...roomNetworks.value, ...legacyNetworkList.value]);

/** 区域下拉占位文案：未选房间时引导先选房间 */
function regionPlaceholder() {
  return props.roomId ? t("network.select_region") : t("device.select_room_first");
}

/** 网段选项展示文本：名称 (CIDR)，遗留网段追加不属于房间的标记 */
function networkOptionLabel(n, legacy = false) {
  const cidrs = [];
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

/** 单个 IP 行可用网段：所选区域的房间网段 + 该区域的遗留网段（带 legacy 标记） */
function networkOptionsFor(ipRow) {
  const regionId = ipRow.regionId;
  // 浅拷贝后追加遗留网段，避免改动 networksByRegion 缓存数组；
  // 房间网段项不写 legacy 字段（读取处 undefined 与 false 同为无标记）
  const list = [...(regionId ? networksByRegion.value.get(regionId) || [] : [])];
  for (const ln of legacyNetworkList.value) {
    if (ln.network_region_id === regionId && !list.some((n) => n.id === ln.id)) {
      list.push({ ...ln, legacy: true });
    }
  }
  return list;
}

// ==================== 行模型（key 供 v-for 稳定渲染） ====================

let nextKey = 1;

const cards = reactive([]);

function emptyIpRow() {
  return { key: nextKey++, regionId: "", subnetId: "", ipAddress: "", description: "" };
}

/** 新网口默认名：扫描表单全部网口名，取未占用的 ethN（重名会被后端 409 拒绝） */
function nextDefaultPortName() {
  const used = new Set();
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

function emptyPortRow() {
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

function emptyCardRow() {
  return {
    key: nextKey++,
    id: null,
    name: t("device.default_card_name"),
    cardType: "pcie",
    description: "",
    ports: [emptyPortRow()]
  };
}

// ==================== 行增删（每级至少保留一行） ====================

function addCard() {
  cards.push(emptyCardRow());
}

function removeCard(index) {
  if (cards.length <= 1) {
    showToast(t("device.at_least_one_card"), "warning");
    return;
  }
  cards.splice(index, 1);
}

function addPort(card) {
  card.ports.push(emptyPortRow());
}

function removePort(card, index) {
  if (card.ports.length <= 1) {
    showToast(t("device.at_least_one_port"), "warning");
    return;
  }
  card.ports.splice(index, 1);
}

function addIp(port) {
  port.ips.push(emptyIpRow());
}

function removeIp(port, index) {
  if (port.ips.length <= 1) {
    showToast(t("device.at_least_one_ip"), "warning");
    return;
  }
  port.ips.splice(index, 1);
}

// ==================== 遗留数据登记（编辑回显） ====================

function registerLegacy(ipData) {
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

function buildCardRow(cardData, portsData) {
  const card = {
    key: nextKey++,
    id: cardData.id || null,
    name: cardData.name || t("device.default_card_name"),
    cardType: cardData.card_type || "pcie",
    description: cardData.description || "",
    ports: []
  };
  for (const portData of portsData || []) {
    const port = {
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
        description: ipData.description || ""
      });
    }
    if (port.ips.length === 0) {
      port.ips.push(emptyIpRow());
    }
    card.ports.push(port);
  }
  return card;
}

// ==================== 对外接口（父组件调用） ====================

/** 新增路径：清空遗留记录并放置一张空网卡 */
function resetForAdd() {
  legacyRegionList.value = [];
  legacyNetworkList.value = [];
  cards.splice(0, cards.length, emptyCardRow());
}

/**
 * 编辑路径回显：等待房间上下文就绪后登记遗留区域/网段并构建行模型。
 * 仅加载托管网口（device_managed=false 的 SNMP 同步网口不在设备表单展示）；
 * 空网卡（板卡）整体跳过；全部被过滤时回退为一张空网卡。
 */
async function loadExisting(cardsData) {
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
function collectData() {
  if (!props.roomId) {
    return { cards: [], errors: [t("device.select_room_first")] };
  }
  const out = [];
  const errors = [];
  cards.forEach((card, cardIdx) => {
    const cardNum = cardIdx + 1;
    const cardName = card.name.trim();
    if (!cardName) {
      errors.push(`${t("device.network_card")} ${cardNum}: ${t("device.card_name_required")}`);
      return;
    }

    const ports = [];
    card.ports.forEach((port, portIdx) => {
      const portNum = portIdx + 1;
      const portName = port.name.trim();
      const portVlan = port.vlanId === "" ? null : parseInt(port.vlanId, 10);

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

      const ips = [];
      port.ips.forEach((ipRow, ipIdx) => {
        const ipNum = ipIdx + 1;
        const subnetId = ipRow.subnetId || null;
        const ipAddress = ipRow.ipAddress.trim();
        const ipDescription = ipRow.description.trim() || null;

        if (!subnetId && !ipAddress && !ipDescription) {
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

        const ipData = {
          subnet_id: subnetId,
          ip_address: ipAddress,
          description: ipDescription
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

// ==================== 自动分配 IP ====================

async function autoAssignIp(ipRow) {
  if (!ipRow.subnetId) {
    showToast(t("device.auto_assign_select_network"), "warning");
    return;
  }
  if (ipRow.assigning) {
    return; // 请求期间禁用：连点时慢的旧响应后到会覆盖新分配的结果
  }
  ipRow.assigning = true;
  try {
    const result = await apiGet(`/api/resources/ip/available/${encodeURIComponent(ipRow.subnetId)}`);
    const first = result.success ? (result.data?.available_ips || [])[0] : null;
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
// （旧版 refreshIpRowDropdowns 的"值不在选项集中即失效"语义）
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
              <div class="nc-field">
                <label :for="`nc-ip-desc-${ipRow.key}`">{{ t("common.description") }}</label>
                <input
                  :id="`nc-ip-desc-${ipRow.key}`"
                  v-model="ipRow.description"
                  type="text"
                  class="nc-input"
                  autocomplete="off"
                >
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
