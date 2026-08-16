/** 新增网卡时的默认名称（跟随界面语言）。 */
function defaultCardName() {
  return t("device.default_card_name");
}

/**
 * 网卡配置管理器（网卡 → 网口 → IP）
 * 设备模态框使用，替换原 IpConfigManager 的扁平 IP 列表。
 */
import { apiGet } from "./apiClient.js";
import { showToast } from "./toast.js";
import { t } from "./i18n.js";
import { escapeHtml } from "./ui.js";
import { isIPv6, isIpInCidr, isValidIP } from "./network.js";

const DEFAULT_PORT_NAME = "eth0";

// 唯一 ID 生成器（用于 label-input 显式关联）
let uniqueIdCounter = 0;
function generateUniqueId(prefix = "nc") {
  return `${prefix}_${Date.now()}_${uniqueIdCounter++}`;
}

const CARD_TYPES = [
  { value: "pcie", label: () => t("device.card_type_pcie") },
  { value: "onboard", label: () => t("device.card_type_onboard") },
  { value: "usb", label: () => t("device.card_type_usb") },
  { value: "virtual", label: () => t("device.card_type_virtual") },
  { value: "wwan", label: () => t("device.card_type_wwan") },
  { value: "wifi", label: () => t("device.card_type_wifi") },
  { value: "other", label: () => t("device.card_type_other") }
];

// 物理形态：网口的物理接口规格
const PHYSICAL_TYPES = [
  { value: "rj45", label: () => t("device.physical_type_rj45") },
  { value: "sfp", label: () => t("device.physical_type_sfp") },
  { value: "sfp_plus", label: () => t("device.physical_type_sfp_plus") },
  { value: "sfp28", label: () => t("device.physical_type_sfp28") },
  { value: "qsfp_plus", label: () => t("device.physical_type_qsfp_plus") },
  { value: "qsfp28", label: () => t("device.physical_type_qsfp28") },
  { value: "wifi", label: () => t("device.physical_type_wifi") },
  { value: "virtual", label: () => t("device.physical_type_virtual") },
  { value: "other", label: () => t("device.physical_type_other") }
];

// 接口角色：网口的用途定位
const INTERFACE_ROLES = [
  { value: "management", label: () => t("device.interface_role_management") },
  { value: "business", label: () => t("device.interface_role_business") },
  { value: "loopback", label: () => t("device.interface_role_loopback") },
  { value: "uplink", label: () => t("device.interface_role_uplink") },
  { value: "other", label: () => t("device.interface_role_other") }
];

export class NetworkCardManager {
  constructor() {
    this.containerId = "device-network-cards-container";
    this.addBtnId = "add-network-card-btn";
    this.excludeSwitchId = null;
    this.networks = [];
    this.regions = [];
    this.devicesCache = null;
    this.roomsCache = null;
    this.networksByRegionCache = new Map();
    this.switchInterfacesCache = new Map();
    this.optionsLoaded = false;
    this.addHandler = null;
  }

  getContainer() {
    return document.getElementById(this.containerId);
  }

  setExcludeSwitchId(id) {
    this.excludeSwitchId = id;
  }

  clear() {
    const container = this.getContainer();
    if (container) container.innerHTML = "";
  }

  async ensureOptionsLoaded() {
    if (this.optionsLoaded) return;

    const loadRegions = async () => {
      try {
        const result = await apiGet("/api/resources/network-regions?page_size=1000");
        if (result.success && result.data) {
          this.regions = result.data.items || result.data || [];
        }
      } catch (e) {
        console.error("预取网络区域失败:", e);
      }
    };
    const loadDevices = async () => {
      try {
        const result = await apiGet("/api/resources/devices?page_size=1000");
        this.devicesCache =
          result.success && result.data
            ? Array.isArray(result.data)
              ? result.data
              : result.data.items || []
            : [];
      } catch (e) {
        console.error("预取设备列表失败:", e);
        this.devicesCache = [];
      }
    };
    const loadRooms = async () => {
      try {
        const result = await apiGet("/api/resources/rooms?page_size=1000");
        this.roomsCache =
          result.success && result.data
            ? Array.isArray(result.data)
              ? result.data
              : result.data.items || []
            : [];
      } catch (e) {
        console.error("预取房间列表失败:", e);
        this.roomsCache = [];
      }
    };

    await Promise.all([loadRegions(), loadDevices(), loadRooms()]);
    this.optionsLoaded = true;
  }

  async loadNetworksByRegion(regionId) {
    if (!regionId) return [];
    if (this.networksByRegionCache.has(regionId)) {
      return this.networksByRegionCache.get(regionId);
    }
    const result = await apiGet(`/api/resources/networks?region_id=${regionId}&page_size=1000`);
    if (!result.success || !result.data) return [];
    const networks = Array.isArray(result.data) ? result.data : result.data.items || [];
    this.networksByRegionCache.set(regionId, networks);
    return networks;
  }

  getNetworkCidr(networkId, ipAddress) {
    const network = this.networks.find((n) => n.id === networkId);
    if (!network) return { cidr: null, hasCidr: false };
    const isV6 = isIPv6(ipAddress);
    const cidr = isV6 ? network.ipv6_cidr : network.ipv4_cidr;
    return { cidr, hasCidr: !!cidr };
  }

  async init() {
    const container = this.getContainer();
    if (!container) return false;
    container.innerHTML = "";
    this.bindAddButton();
    await this.ensureOptionsLoaded();
    await this.addCard();
    return true;
  }

  bindAddButton() {
    const btn = document.getElementById(this.addBtnId);
    if (!btn) return;
    if (this.addHandler) btn.removeEventListener("click", this.addHandler);
    this.addHandler = () => this.addCard();
    btn.addEventListener("click", this.addHandler);
  }

  async addCard(cardData = null) {
    await this.ensureOptionsLoaded();
    const container = this.getContainer();
    if (!container) return;
    const data = cardData || {};
    const card = this.createCardElement(data);
    container.appendChild(card);
    await this.bindCardEvents(card, data);
  }

  createCardElement(cardData = {}) {
    const div = document.createElement("div");
    div.className = "network-card-item";
    const uid = generateUniqueId("card");

    div.innerHTML = `
      <header class="card-level-bar">
        <h3 id="${uid}-title" class="level-badge level-card">${t("device.network_card")}</h3>
        <div class="level-actions">
          <button type="button" class="btn btn-danger btn-sm remove-card-btn" aria-label="${t("device.delete_network_card")}">${t("common.delete")}</button>
          <button type="button" class="btn btn-secondary btn-sm add-port-btn" aria-label="${t("device.add_network_port")}">${t("device.add_network_port")}</button>
        </div>
      </header>
      <input type="hidden" class="card-id" value="${escapeHtml(cardData.id || "")}" />
      <div class="nc-fields">
        <div class="nc-field">
          <label for="${uid}-name">${t("device.network_card_name")}<abbr title="required" class="required" aria-hidden="true">*</abbr></label>
          <input id="${uid}-name" type="text" class="card-name nc-input" value="${escapeHtml(cardData.name || defaultCardName())}" placeholder="${t("device.network_card_name")}" autocomplete="off" required />
        </div>
        <div class="nc-field">
          <label for="${uid}-type">${t("device.network_card_type")}</label>
          <select id="${uid}-type" class="card-type nc-input">
            ${CARD_TYPES.map((opt) => `<option value="${opt.value}" ${cardData.card_type === opt.value ? "selected" : ""}>${opt.label()}</option>`).join("")}
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-desc">${t("common.description")}</label>
          <input id="${uid}-desc" type="text" class="card-description nc-input" value="${escapeHtml(cardData.description || "")}" autocomplete="off" />
        </div>
      </div>
    `;
    return div;
  }

  async bindCardEvents(card, cardData = {}) {
    card.querySelector(".remove-card-btn")?.addEventListener("click", () => this.removeCard(card));
    card.querySelector(".add-port-btn")?.addEventListener("click", async () => {
      const port = this.createPortElement();
      card.appendChild(port);
      await this.bindPortEvents(port);
    });

    const ports = cardData.ports || [];
    if (ports.length > 0) {
      for (const portData of ports) {
        const port = this.createPortElement(portData);
        card.appendChild(port);
        await this.bindPortEvents(port, portData);
      }
    } else {
      const port = this.createPortElement();
      card.appendChild(port);
      await this.bindPortEvents(port);
    }
  }

  removeCard(card) {
    const container = this.getContainer();
    const cards = container?.querySelectorAll(".network-card-item");
    if (cards && cards.length <= 1) {
      showToast(t("device.at_least_one_card"), "warning");
      return;
    }
    card.remove();
  }

  createPortElement(portData = {}) {
    const div = document.createElement("div");
    div.className = "nc-port-item";
    const uid = generateUniqueId("port");

    div.innerHTML = `
      <header class="port-level-bar">
        <h3 id="${uid}-title" class="level-badge level-port">${t("device.network_port")}</h3>
        <div class="level-actions">
          <button type="button" class="btn btn-danger btn-sm remove-port-btn" aria-label="${t("device.delete_network_port")}">${t("common.delete")}</button>
          <button type="button" class="btn btn-secondary btn-sm add-ip-btn" aria-label="${t("ip.add_ip")}">${t("ip.add_ip")}</button>
        </div>
      </header>
      <input type="hidden" class="port-id" value="${escapeHtml(portData.id || "")}" />
      <div class="nc-fields">
        <div class="nc-field">
          <label for="${uid}-name">${t("device.network_port_name")}<abbr title="required" class="required" aria-hidden="true">*</abbr></label>
          <input id="${uid}-name" type="text" class="port-name nc-input" value="${escapeHtml(portData.name || DEFAULT_PORT_NAME)}" placeholder="${t("device.network_port_name")}" autocomplete="off" required />
        </div>
        <div class="nc-field">
          <label for="${uid}-ptype">${t("device.physical_type")}</label>
          <select id="${uid}-ptype" class="port-physical-type nc-input">
            ${PHYSICAL_TYPES.map((opt) => `<option value="${opt.value}" ${portData.physical_type === opt.value ? "selected" : ""}>${opt.label()}</option>`).join("")}
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-role">${t("device.interface_role")}</label>
          <select id="${uid}-role" class="port-role nc-input">
            ${INTERFACE_ROLES.map((opt) => `<option value="${opt.value}" ${portData.interface_role === opt.value ? "selected" : ""}>${opt.label()}</option>`).join("")}
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-vlan">${t("device.vlan_id")}</label>
          <input id="${uid}-vlan" type="number" class="port-vlan nc-input" value="${portData.vlan_id ?? ""}" min="1" max="4094" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label for="${uid}-mac">${t("ip.mac_address")}</label>
          <input id="${uid}-mac" type="text" class="port-mac nc-input" value="${escapeHtml(portData.mac_address || "")}" placeholder="00:11:22:33:44:55" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label for="${uid}-desc">${t("common.description")}</label>
          <input id="${uid}-desc" type="text" class="port-description nc-input" value="${escapeHtml(portData.description || "")}" autocomplete="off" />
        </div>
      </div>
      <div class="nc-fields">
        <div class="nc-field">
          <label for="${uid}-switch">${t("device.upstream_device")}</label>
          <select id="${uid}-switch" class="port-switch nc-input">
            <option value="">${t("device.select_upstream_device")}</option>
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-port">${t("device.upstream_port")}</label>
          <select id="${uid}-port" class="port-port nc-input">
            <option value="">${t("device.select_upstream_port")}</option>
          </select>
        </div>
      </div>
      <div class="port-ips-container" aria-label="${t("device.ip_list")}"></div>
    `;
    return div;
  }

  async bindPortEvents(port, portData = {}) {
    port.querySelector(".remove-port-btn")?.addEventListener("click", () => this.removePort(port));
    port.querySelector(".add-ip-btn")?.addEventListener("click", async () => {
      const ipsContainer = port.querySelector(".port-ips-container");
      const ipRow = await this.createIpRowElement();
      ipsContainer.appendChild(ipRow.element);
      await this.bindIpRowEvents(ipRow);
    });

    const switchSelect = port.querySelector(".port-switch");
    const portSelect = port.querySelector(".port-port");

    await this.loadSwitches(switchSelect, portSelect);

    if (portData.switch_id && switchSelect) {
      switchSelect.value = portData.switch_id;
      await this.handleSwitchChange(switchSelect, portSelect);
      if (portData.uplink_interface_id && portSelect) {
        portSelect.value = portData.uplink_interface_id;
      }
    }

    const ipsContainer = port.querySelector(".port-ips-container");
    const ips = portData.ips || [];
    if (ips.length > 0) {
      for (const ipData of ips) {
        const ipRow = await this.createIpRowElement(ipData);
        ipsContainer.appendChild(ipRow.element);
        await this.bindIpRowEvents(ipRow, ipData);
      }
    } else {
      const ipRow = await this.createIpRowElement();
      ipsContainer.appendChild(ipRow.element);
      await this.bindIpRowEvents(ipRow);
    }
  }

  removePort(port) {
    const card = port.closest(".network-card-item");
    if (!card) return;
    const ports = card.querySelectorAll(".nc-port-item");
    if (ports.length <= 1) {
      showToast(t("device.at_least_one_port"), "warning");
      return;
    }
    port.remove();
  }

  async createIpRowElement(ipData = null) {
    await this.ensureOptionsLoaded();
    const div = document.createElement("div");
    div.className = "nc-ip-item";
    const uid = generateUniqueId("ip");

    const regionOptions = this.regions
      .map((r) => `<option value="${r.id}">${escapeHtml(r.name)}</option>`)
      .join("");

    div.innerHTML = `
      <header class="ip-level-bar">
        <h3 id="${uid}-title" class="level-badge level-ip">IP</h3>
        <div class="level-actions">
          <button type="button" class="btn btn-danger btn-sm remove-ip-btn" aria-label="${t("ip.delete_ip")}">${t("common.delete")}</button>
        </div>
      </header>
      <div class="nc-fields">
        <div class="nc-field">
          <label for="${uid}-region">${t("network.region")}</label>
          <select id="${uid}-region" class="ip-region nc-input">
            <option value="">${t("network.select_region")}</option>
            ${regionOptions}
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-network">${t("network.name")}<abbr title="required" class="required" aria-hidden="true">*</abbr></label>
          <select id="${uid}-network" class="ip-network nc-input" required>
            <option value="">${t("network.select_network")}</option>
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-address">${t("ip.ip_address")}<abbr title="required" class="required" aria-hidden="true">*</abbr></label>
          <input id="${uid}-address" type="text" class="ip-address nc-input" value="${escapeHtml(ipData?.ip_address || "")}" placeholder="192.168.1.100" autocomplete="off" required />
        </div>
        <div class="nc-field">
          <label for="${uid}-description">${t("common.description")}</label>
          <input id="${uid}-description" type="text" class="ip-description nc-input" value="${escapeHtml(ipData?.description || "")}" autocomplete="off" />
        </div>
      </div>
    `;

    return { element: div };
  }

  async bindIpRowEvents(ipRow, ipData = null) {
    const { element } = ipRow;
    element
      .querySelector(".remove-ip-btn")
      ?.addEventListener("click", () => this.removeIp(element));

    const regionSelect = element.querySelector(".ip-region");
    const networkSelect = element.querySelector(".ip-network");

    regionSelect?.addEventListener("change", async () => {
      const regionId = regionSelect.value;
      networkSelect.innerHTML = `<option value="">${t("network.select_network")}</option>`;
      if (!regionId) return;
      const networks = await this.loadNetworksByRegion(regionId);
      this.networks = this.mergeNetworks(networks);
      networkSelect.innerHTML =
        `<option value="">${t("network.select_network")}</option>` +
        networks
          .map((n) => {
            const cidrs = [];
            if (n.ipv4_cidr) cidrs.push(n.ipv4_cidr);
            if (n.ipv6_cidr) cidrs.push(n.ipv6_cidr);
            const cidrStr = cidrs.length > 0 ? cidrs.join(" / ") : "";
            return `<option value="${n.id}">${escapeHtml(n.name)}${cidrStr ? ` (${cidrStr})` : ""}</option>`;
          })
          .join("");
    });

    networkSelect?.addEventListener("change", () => {
      const networkId = networkSelect.value;
      const network = this.networks.find((n) => n.id === networkId);
      if (network) {
        const regionId = network.network_region_id;
        if (regionId && regionSelect.value !== regionId) {
          regionSelect.value = regionId;
        }
      }
    });

    if (ipData) {
      if (ipData.network_region_id && regionSelect) {
        regionSelect.value = ipData.network_region_id;
        regionSelect.dispatchEvent(new Event("change"));
      }
      if (ipData.network_id && networkSelect) {
        networkSelect.value = ipData.network_id;
      }
    }
  }

  removeIp(ipElement) {
    const port = ipElement.closest(".nc-port-item");
    if (!port) return;
    const ips = port.querySelectorAll(".nc-ip-item");
    if (ips.length <= 1) {
      showToast(t("device.at_least_one_ip"), "warning");
      return;
    }
    ipElement.remove();
  }

  mergeNetworks(newNetworks) {
    const map = new Map(this.networks.map((n) => [n.id, n]));
    for (const n of newNetworks) map.set(n.id, n);
    return Array.from(map.values());
  }

  async loadSwitches(switchSelect, portSelect) {
    if (!switchSelect || !portSelect) return;
    await this.ensureOptionsLoaded();

    switchSelect.innerHTML = `<option value="">${t("device.select_upstream_device")}</option>`;
    let devices = this.devicesCache || [];
    if (this.excludeSwitchId) {
      devices = devices.filter((d) => d.id !== this.excludeSwitchId);
    }
    devices.forEach((d) => {
      const option = document.createElement("option");
      option.value = d.id;
      option.textContent = d.name;
      switchSelect.appendChild(option);
    });

    switchSelect.addEventListener("change", () =>
      this.handleSwitchChange(switchSelect, portSelect)
    );
  }

  async handleSwitchChange(switchSelect, portSelect) {
    const deviceId = switchSelect.value;
    portSelect.innerHTML = `<option value="">${t("device.select_upstream_port")}</option>`;
    if (!deviceId) return;

    let interfaces;
    if (this.switchInterfacesCache.has(deviceId)) {
      interfaces = this.switchInterfacesCache.get(deviceId);
    } else {
      try {
        const result = await apiGet(`/api/resources/devices/${deviceId}/interfaces`);
        if (result.success && result.data) {
          interfaces = Array.isArray(result.data) ? result.data : result.data.items || [];
          this.switchInterfacesCache.set(deviceId, interfaces);
        } else {
          interfaces = [];
        }
      } catch (error) {
        console.error("加载设备接口失败:", error);
        interfaces = [];
      }
    }

    interfaces.forEach((iface) => {
      const option = document.createElement("option");
      option.value = iface.id;
      const typeMark = iface.interface_role
        ? `[${t(`device.interface_role_${iface.interface_role}`)}]`
        : "";
      option.textContent = `${iface.name}${typeMark}`;
      portSelect.appendChild(option);
    });
  }

  async loadExisting(cards) {
    const container = this.getContainer();
    if (!container) return;
    container.innerHTML = "";
    this.bindAddButton();

    if (!cards || cards.length === 0) {
      await this.ensureOptionsLoaded();
      await this.addCard();
      return;
    }

    // 并行预取所有依赖数据：基础选项（区域/设备/房间）+ 网卡专属数据（信息点/接口/网络）
    await Promise.all([this.ensureOptionsLoaded(), this.prefetchCardData(cards)]);

    for (const cardData of cards) {
      await this.addCard(cardData);
    }
  }

  /**
   * 并行预取渲染所需的所有外部数据（上联设备接口、各区域网段）。
   */
  async prefetchCardData(cards) {
    const switchIds = new Set();
    const regionIds = new Set();

    for (const card of cards) {
      for (const port of card.ports || []) {
        if (port.switch_id) switchIds.add(port.switch_id);
        for (const ip of port.ips || []) {
          if (ip.network_region_id) regionIds.add(ip.network_region_id);
        }
      }
    }

    const toInt = (arr) => (Array.isArray(arr) ? arr : arr.items || []);
    const tasks = [];

    for (const switchId of switchIds) {
      if (!this.switchInterfacesCache.has(switchId)) {
        tasks.push(
          (async () => {
            try {
              const result = await apiGet(`/api/resources/devices/${switchId}/interfaces`);
              if (result.success && result.data) {
                this.switchInterfacesCache.set(switchId, toInt(result.data));
              }
            } catch (error) {
              console.error("预加载设备接口失败:", error);
            }
          })()
        );
      }
    }

    for (const regionId of regionIds) {
      if (!this.networksByRegionCache.has(regionId)) {
        tasks.push(
          (async () => {
            try {
              const result = await apiGet(
                `/api/resources/networks?region_id=${regionId}&page_size=1000`
              );
              if (result.success && result.data) {
                this.networksByRegionCache.set(regionId, toInt(result.data));
              }
            } catch (error) {
              console.error("预加载网络失败:", error);
            }
          })()
        );
      }
    }

    if (tasks.length > 0) {
      await Promise.all(tasks);
    }
  }

  collectData() {
    const container = this.getContainer();
    if (!container) return { cards: [], errors: [] };
    const cards = [];
    const errors = [];
    const cardElements = container.querySelectorAll(".network-card-item");

    cardElements.forEach((cardEl, cardIdx) => {
      const cardNum = cardIdx + 1;
      const cardId = cardEl.querySelector(".card-id")?.value || null;
      const cardName = cardEl.querySelector(".card-name")?.value?.trim() || "";
      const cardType = cardEl.querySelector(".card-type")?.value || "pcie";
      const cardDesc = cardEl.querySelector(".card-description")?.value?.trim() || null;

      if (!cardName) {
        errors.push(`${t("device.network_card")} ${cardNum}: ${t("device.card_name_required")}`);
        return;
      }

      const ports = [];
      const portElements = cardEl.querySelectorAll(".nc-port-item");
      portElements.forEach((portEl, portIdx) => {
        const portNum = portIdx + 1;
        const portId = portEl.querySelector(".port-id")?.value || null;
        const portName = portEl.querySelector(".port-name")?.value?.trim() || "";
        const physicalType = portEl.querySelector(".port-physical-type")?.value || "rj45";
        const interfaceRole = portEl.querySelector(".port-role")?.value || "business";
        const portMac = portEl.querySelector(".port-mac")?.value?.trim() || null;
        const portVlanRaw = portEl.querySelector(".port-vlan")?.value?.trim() || "";
        const portVlan = portVlanRaw ? parseInt(portVlanRaw, 10) : null;
        const portDesc = portEl.querySelector(".port-description")?.value?.trim() || null;

        if (!portName) {
          errors.push(
            `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum}: ${t("device.port_name_required")}`
          );
          return;
        }
        if (portVlan !== null && (isNaN(portVlan) || portVlan < 1 || portVlan > 4094)) {
          errors.push(
            `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum}: ${t("device.vlan_invalid")}`
          );
          return;
        }

        const ips = [];
        const ipElements = portEl.querySelectorAll(".nc-ip-item");
        ipElements.forEach((ipEl, ipIdx) => {
          const ipNum = ipIdx + 1;
          const networkId = ipEl.querySelector(".ip-network")?.value || null;
          const ipAddress = ipEl.querySelector(".ip-address")?.value?.trim() || "";
          const ipDescription = ipEl.querySelector(".ip-description")?.value?.trim() || null;
          const networkRegionId = ipEl.querySelector(".ip-region")?.value || null;

          if (!networkId && !ipAddress && !ipDescription) return;

          if (!networkId) {
            errors.push(
              `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum} - IP ${ipNum}: ${t("device.network_required")}`
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

          const cidrResult = this.getNetworkCidr(networkId, ipAddress);
          if (!cidrResult.hasCidr) {
            const ipType = isIPv6(ipAddress) ? "IPv6" : "IPv4";
            errors.push(
              `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum} - IP ${ipNum}: ${t("device.cidr_not_supported")} ${ipType} ${t("device.address")}`
            );
            return;
          }
          if (cidrResult.cidr && !isIpInCidr(ipAddress, cidrResult.cidr)) {
            errors.push(
              `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum} - IP ${ipNum}: ${t("device.ip_not_in_cidr")} ${cidrResult.cidr}`
            );
            return;
          }

          const ipData = {
            network_id: networkId,
            ip_address: ipAddress,
            description: ipDescription
          };
          if (networkRegionId) ipData.network_region_id = networkRegionId;
          ips.push(ipData);
        });

        if (ips.length === 0) {
          errors.push(
            `${t("device.network_card")} ${cardNum} - ${t("device.network_port")} ${portNum}: ${t("device.at_least_one_ip")}`
          );
        }

        const portSwitchId = portEl.querySelector(".port-switch")?.value || null;
        const portUplinkInterfaceId = portEl.querySelector(".port-port")?.value || null;

        ports.push({
          id: portId,
          name: portName,
          physical_type: physicalType,
          interface_role: interfaceRole,
          mac_address: portMac,
          vlan_id: portVlan,
          description: portDesc,
          ips,
          switch_id: portSwitchId,
          uplink_interface_id: portUplinkInterfaceId
        });
      });

      if (ports.length === 0) {
        errors.push(`${t("device.network_card")} ${cardNum}: ${t("device.at_least_one_port")}`);
      }

      cards.push({
        id: cardId,
        name: cardName,
        card_type: cardType,
        description: cardDesc,
        ports
      });
    });

    return { cards, errors };
  }
}

const managerInstance = new NetworkCardManager();

export function getNetworkCardManager() {
  return managerInstance;
}
