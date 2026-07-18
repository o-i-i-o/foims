/**
 * 网卡配置管理器（网卡 → 网口 → IP）
 * 设备模态框使用，替换原 IpConfigManager 的扁平 IP 列表。
 */
import { apiGet } from "./apiClient.js";
import { showToast } from "./toast.js";
import { t } from "./i18n.js";
import { escapeHtml } from "./ui.js";

const DEFAULT_CARD_NAME = '网卡1';
const DEFAULT_PORT_NAME = 'eth0';

// 唯一 ID 生成器（用于 label-input 显式关联）
let uniqueIdCounter = 0;
function generateUniqueId(prefix = 'nc') {
  return `${prefix}_${Date.now()}_${uniqueIdCounter++}`;
}

const CARD_TYPES = [
  { value: 'pcie', label: () => t('device.card_type_pcie') || 'PCIe' },
  { value: 'onboard', label: () => t('device.card_type_onboard') || 'Onboard' },
  { value: 'usb', label: () => t('device.card_type_usb') || 'USB' },
  { value: 'virtual', label: () => t('device.card_type_virtual') || 'Virtual' },
  { value: 'wwan', label: () => t('device.card_type_wwan') || 'WWAN' },
  { value: 'wifi', label: () => t('device.card_type_wifi') || 'WiFi' },
  { value: 'other', label: () => t('device.card_type_other') || 'Other' },
];

const PORT_TYPES = [
  { value: 'physical', label: () => t('device.port_type_physical') || 'Physical' },
  { value: 'svi', label: () => t('device.port_type_svi') || 'SVI' },
  { value: 'management', label: () => t('device.port_type_management') || 'Management' },
  { value: 'loopback', label: () => t('device.port_type_loopback') || 'Loopback' },
  { value: 'wifi', label: () => t('device.port_type_wifi') || 'WiFi' },
];

function isValidIPv4(ip) {
  const parts = ip.split('.');
  if (parts.length !== 4) return false;
  for (const part of parts) {
    if (!/^\d+$/.test(part)) return false;
    const num = parseInt(part, 10);
    if (isNaN(num) || num < 0 || num > 255) return false;
  }
  return true;
}

function isValidIPv6(ip) {
  if (!ip.includes(':')) return false;
  let addr = ip.split('%')[0];
  if (addr === '::') return true;
  const parts = addr.split(':');
  if (parts.length > 8) return false;
  const doubleColonCount = parts.filter(p => p === '').length;
  if (doubleColonCount > 1) return false;
  if (doubleColonCount === 1 && parts.length >= 8) return false;
  for (const part of parts) {
    if (part === '') continue;
    if (!/^[0-9a-fA-F]{1,4}$/.test(part)) return false;
  }
  return true;
}

function isValidIP(ip) {
  if (!ip || typeof ip !== 'string') return false;
  return isValidIPv4(ip) || isValidIPv6(ip);
}

function isIPv6(ip) {
  return ip.includes(':');
}

function ipv4ToInt(ip) {
  const parts = ip.split('.').map(p => parseInt(p, 10));
  if (parts.length !== 4 || parts.some(p => isNaN(p) || p < 0 || p > 255)) return null;
  return (BigInt(parts[0]) << 24n) + (BigInt(parts[1]) << 16n) + (BigInt(parts[2]) << 8n) + BigInt(parts[3]);
}

function ipv6ToInt(ipv6) {
  let ip = ipv6.split('%')[0];
  if (ip === '::') ip = '::0';
  const parts = ip.split(':');
  if (parts.length > 8) return null;
  const doubleColonIndex = parts.indexOf('');
  if (doubleColonIndex !== -1) {
    const missingCount = 8 - parts.length + 1;
    parts.splice(doubleColonIndex, 1, ...Array(missingCount).fill('0'));
  }
  if (parts.length !== 8) return null;
  let result = BigInt(0);
  for (const part of parts) {
    const val = part === '' ? 0 : parseInt(part, 16);
    if (isNaN(val) || val < 0 || val > 65535) return null;
    result = result * BigInt(65536) + BigInt(val);
  }
  return result;
}

function ipv4CidrToRange(cidr) {
  const [ip, prefixLen] = cidr.split('/');
  const prefix = parseInt(prefixLen, 10);
  if (!ip || isNaN(prefix) || prefix < 0 || prefix > 32) return null;
  const ipInt = ipv4ToInt(ip);
  if (ipInt === null) return null;
  const max = (1n << 32n) - 1n;
  const mask = prefix === 0 ? 0n : max ^ ((1n << BigInt(32 - prefix)) - 1n);
  const network = ipInt & mask;
  const broadcast = network | (max ^ mask);
  return { start: network, end: broadcast };
}

function ipv6CidrToRange(cidr) {
  const [ip, prefixLen] = cidr.split('/');
  const prefix = parseInt(prefixLen, 10);
  if (!ip || isNaN(prefix) || prefix < 0 || prefix > 128) return null;
  const ipInt = ipv6ToInt(ip);
  if (ipInt === null) return null;
  const max = (1n << 128n) - 1n;
  const mask = prefix === 0 ? 0n : max ^ ((1n << BigInt(128 - prefix)) - 1n);
  const network = ipInt & mask;
  const broadcast = network | (max ^ mask);
  return { start: network, end: broadcast };
}

function isIpInCidr(ipAddress, cidr) {
  if (!cidr) return true;
  if (isIPv6(ipAddress)) {
    const ipInt = ipv6ToInt(ipAddress);
    if (ipInt === null) return false;
    const range = ipv6CidrToRange(cidr);
    if (!range) return false;
    return ipInt >= range.start && ipInt <= range.end;
  }
  const ipInt = ipv4ToInt(ipAddress);
  if (ipInt === null) return false;
  const range = ipv4CidrToRange(cidr);
  if (!range) return false;
  return ipInt >= range.start && ipInt <= range.end;
}

export class NetworkCardManager {
  constructor() {
    this.containerId = 'device-network-cards-container';
    this.addBtnId = 'add-network-card-btn';
    this.excludeSwitchId = null;
    this.networks = [];
    this.regions = [];
    this.devicesCache = null;
    this.roomsCache = null;
    this.outletsCache = [];
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
    if (container) container.innerHTML = '';
  }

  async ensureOptionsLoaded() {
    if (this.regions.length === 0) {
      const result = await apiGet('/api/resources/network-regions?page_size=1000');
      if (result.success && result.data) {
        this.regions = result.data.items || result.data || [];
      }
    }
    if (this.devicesCache === null) {
      const result = await apiGet('/api/resources/devices?page_size=1000');
      if (result.success && result.data) {
        this.devicesCache = Array.isArray(result.data) ? result.data : (result.data.items || []);
      } else {
        this.devicesCache = [];
      }
    }
    if (this.roomsCache === null) {
      const result = await apiGet('/api/resources/rooms?page_size=1000');
      if (result.success && result.data) {
        this.roomsCache = Array.isArray(result.data) ? result.data : (result.data.items || []);
      } else {
        this.roomsCache = [];
      }
    }
  }

  async loadNetworksByRegion(regionId) {
    if (!regionId) return [];
    const result = await apiGet(`/api/resources/networks?region_id=${regionId}&page_size=1000`);
    if (!result.success || !result.data) return [];
    return Array.isArray(result.data) ? result.data : (result.data.items || []);
  }

  getNetworkCidr(networkId, ipAddress) {
    const network = this.networks.find(n => n.id === networkId);
    if (!network) return { cidr: null, hasCidr: false };
    const isV6 = isIPv6(ipAddress);
    const cidr = isV6 ? network.ipv6_cidr : network.ipv4_cidr;
    return { cidr, hasCidr: !!cidr };
  }

  async init() {
    const container = this.getContainer();
    if (!container) return false;
    container.innerHTML = '';
    this.bindAddButton();
    await this.ensureOptionsLoaded();
    await this.addCard();
    return true;
  }

  bindAddButton() {
    const btn = document.getElementById(this.addBtnId);
    if (!btn) return;
    if (this.addHandler) btn.removeEventListener('click', this.addHandler);
    this.addHandler = () => this.addCard();
    btn.addEventListener('click', this.addHandler);
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
    const div = document.createElement('div');
    div.className = 'network-card-item';
    const uid = generateUniqueId('card');

    div.innerHTML = `
      <header class="card-level-bar">
        <h3 id="${uid}-title" class="level-badge level-card">${t('device.network_card') || '网卡'}</h3>
        <div class="level-actions">
          <button type="button" class="btn btn-danger btn-sm remove-card-btn" aria-label="${t('device.delete_network_card') || '删除此网卡'}">${t('common.delete') || '删除'}</button>
          <button type="button" class="btn btn-secondary btn-sm add-port-btn" aria-label="${t('device.add_network_port') || '添加网口'}">${t('device.add_network_port') || '添加网口'}</button>
        </div>
      </header>
      <input type="hidden" class="card-id" value="${escapeHtml(cardData.id || '')}" />
      <div class="nc-fields">
        <div class="nc-field">
          <label for="${uid}-name">${t('device.network_card_name') || '网卡名称'}<abbr title="required" class="required" aria-hidden="true">*</abbr></label>
          <input id="${uid}-name" type="text" class="card-name nc-input" value="${escapeHtml(cardData.name || DEFAULT_CARD_NAME)}" placeholder="${t('device.network_card_name') || '网卡名称'}" autocomplete="off" required />
        </div>
        <div class="nc-field">
          <label for="${uid}-type">${t('device.network_card_type') || '网卡类型'}</label>
          <select id="${uid}-type" class="card-type nc-input">
            ${CARD_TYPES.map(opt => `<option value="${opt.value}" ${cardData.card_type === opt.value ? 'selected' : ''}>${opt.label()}</option>`).join('')}
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-desc">${t('common.description') || '描述'}</label>
          <input id="${uid}-desc" type="text" class="card-description nc-input" value="${escapeHtml(cardData.description || '')}" autocomplete="off" />
        </div>
      </div>
    `;
    return div;
  }

  async bindCardEvents(card, cardData = {}) {
    card.querySelector('.remove-card-btn')?.addEventListener('click', () => this.removeCard(card));
    card.querySelector('.add-port-btn')?.addEventListener('click', async () => {
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
    const cards = container?.querySelectorAll('.network-card-item');
    if (cards && cards.length <= 1) {
      showToast(t('device.at_least_one_card') || '至少需要保留一张网卡', 'warning');
      return;
    }
    card.remove();
  }

  createPortElement(portData = {}) {
    const div = document.createElement('div');
    div.className = 'nc-port-item';
    const uid = generateUniqueId('port');

    div.innerHTML = `
      <header class="port-level-bar">
        <h3 id="${uid}-title" class="level-badge level-port">${t('device.network_port') || '网口'}</h3>
        <div class="level-actions">
          <button type="button" class="btn btn-danger btn-sm remove-port-btn" aria-label="${t('device.delete_network_port') || '删除此网口'}">${t('common.delete') || '删除'}</button>
          <button type="button" class="btn btn-secondary btn-sm add-ip-btn" aria-label="${t('ip.add_ip') || '添加IP'}">${t('ip.add_ip') || '添加IP'}</button>
        </div>
      </header>
      <input type="hidden" class="port-id" value="${escapeHtml(portData.id || '')}" />
      <div class="nc-fields">
        <div class="nc-field">
          <label for="${uid}-name">${t('device.network_port_name') || '网口名称'}<abbr title="required" class="required" aria-hidden="true">*</abbr></label>
          <input id="${uid}-name" type="text" class="port-name nc-input" value="${escapeHtml(portData.name || DEFAULT_PORT_NAME)}" placeholder="${t('device.network_port_name') || '网口名称'}" autocomplete="off" required />
        </div>
        <div class="nc-field">
          <label for="${uid}-type">${t('device.network_port_type') || '网口类型'}</label>
          <select id="${uid}-type" class="port-type nc-input">
            ${PORT_TYPES.map(opt => `<option value="${opt.value}" ${portData.interface_type === opt.value ? 'selected' : ''}>${opt.label()}</option>`).join('')}
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-vlan">${t('device.vlan_id') || 'VLAN ID'}</label>
          <input id="${uid}-vlan" type="number" class="port-vlan nc-input" value="${portData.vlan_id ?? ''}" min="1" max="4094" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label for="${uid}-mac">${t('ip.mac_address') || 'MAC地址'}</label>
          <input id="${uid}-mac" type="text" class="port-mac nc-input" value="${escapeHtml(portData.mac_address || '')}" placeholder="00:11:22:33:44:55" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label for="${uid}-desc">${t('common.description') || '描述'}</label>
          <input id="${uid}-desc" type="text" class="port-description nc-input" value="${escapeHtml(portData.description || '')}" autocomplete="off" />
        </div>
      </div>
      <div class="nc-fields">
        <div class="nc-field nc-outlet-picker-field">
          <label>${t('device.net_outlets') || '信息点（多选，按顺序连接）'}</label>
          <div class="nc-outlet-picker">
            <div class="nc-outlet-add-row">
              <select class="port-outlet-available nc-input">
                <option value="">${t('device.select_net_outlet_to_add') || '选择信息点添加'}</option>
              </select>
              <button type="button" class="btn btn-secondary btn-sm nc-outlet-add-btn">${t('common.add') || '添加'}</button>
            </div>
            <ul class="nc-outlet-list" aria-label="${t('device.net_outlets_order') || '信息点连接顺序'}"></ul>
          </div>
        </div>
      </div>
      <div class="nc-fields">
        <div class="nc-field">
          <label for="${uid}-switch">${t('device.upstream_device') || '上级设备'}</label>
          <select id="${uid}-switch" class="port-switch nc-input">
            <option value="">${t('device.select_upstream_device') || '选择设备'}</option>
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-port">${t('device.upstream_port') || '上级端口'}</label>
          <select id="${uid}-port" class="port-port nc-input">
            <option value="">${t('device.select_upstream_port') || '选择端口'}</option>
          </select>
        </div>
      </div>
      <div class="port-ips-container" aria-label="${t('device.ip_list') || 'IP地址列表'}"></div>
    `;
    return div;
  }

  async bindPortEvents(port, portData = {}) {
    port.querySelector('.remove-port-btn')?.addEventListener('click', () => this.removePort(port));
    port.querySelector('.add-ip-btn')?.addEventListener('click', async () => {
      const ipsContainer = port.querySelector('.port-ips-container');
      const ipRow = await this.createIpRowElement();
      ipsContainer.appendChild(ipRow.element);
      await this.bindIpRowEvents(ipRow);
    });

    const outletAvailableSelect = port.querySelector('.port-outlet-available');
    const outletList = port.querySelector('.nc-outlet-list');
    const outletAddBtn = port.querySelector('.nc-outlet-add-btn');
    const switchSelect = port.querySelector('.port-switch');
    const portSelect = port.querySelector('.port-port');

    const roomId = document.querySelector('#device-room-id')?.value || null;

    // 加载信息点到"可添加"下拉
    const availableOutlets = (roomId && outletAvailableSelect)
      ? await this.loadOutlets(outletAvailableSelect, roomId)
      : [];

    // 绑定"添加"按钮
    if (outletAddBtn && outletAvailableSelect && outletList) {
      outletAddBtn.addEventListener('click', () => {
        const selectedId = outletAvailableSelect.value;
        if (!selectedId) {
          showToast(t('device.select_net_outlet_first') || '请先选择信息点', 'warning');
          return;
        }
        const selectedOption = outletAvailableSelect.options[outletAvailableSelect.selectedIndex];
        const outletName = selectedOption ? selectedOption.textContent : selectedId;
        this.addOutletItem(outletList, selectedId, outletName);
        // 从下拉中移除已添加项
        outletAvailableSelect.removeChild(selectedOption);
        outletAvailableSelect.value = '';
      });
    }

    // 按顺序回填已选信息点
    if (outletList && Array.isArray(portData.net_outlet_ids)) {
      for (const outletId of portData.net_outlet_ids) {
        // 从已加载的 available 列表中查找名称
        const matched = availableOutlets.find(o => o.id === outletId);
        const outletName = matched ? (matched.name || outletId) : outletId;
        this.addOutletItem(outletList, outletId, outletName);
        // 从下拉中移除已回填项
        if (outletAvailableSelect) {
          const optToRemove = Array.from(outletAvailableSelect.options).find(o => o.value === outletId);
          if (optToRemove) outletAvailableSelect.removeChild(optToRemove);
        }
      }
    }

    await this.loadSwitches(switchSelect, portSelect);

    if (portData.switch_id && switchSelect) {
      switchSelect.value = portData.switch_id;
      await this.handleSwitchChange(switchSelect, portSelect);
      if (portData.uplink_interface_id && portSelect) {
        portSelect.value = portData.uplink_interface_id;
      }
    }

    const ipsContainer = port.querySelector('.port-ips-container');
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

  addOutletItem(outletList, outletId, outletName) {
    const li = document.createElement('li');
    li.className = 'nc-outlet-item';
    li.dataset.outletId = outletId;
    li.innerHTML = `
      <span class="nc-outlet-item-name">${escapeHtml(outletName)}</span>
      <span class="nc-outlet-item-actions">
        <button type="button" class="btn-icon-sm nc-outlet-up-btn" aria-label="${t('common.move_up') || '上移'}">↑</button>
        <button type="button" class="btn-icon-sm nc-outlet-down-btn" aria-label="${t('common.move_down') || '下移'}">↓</button>
        <button type="button" class="btn-icon-sm nc-outlet-remove-btn" aria-label="${t('common.remove') || '移除'}">×</button>
      </span>
    `;
    li.querySelector('.nc-outlet-up-btn')?.addEventListener('click', () => {
      const prev = li.previousElementSibling;
      if (prev) outletList.insertBefore(li, prev);
    });
    li.querySelector('.nc-outlet-down-btn')?.addEventListener('click', () => {
      const next = li.nextElementSibling;
      if (next) outletList.insertBefore(next, li);
    });
    li.querySelector('.nc-outlet-remove-btn')?.addEventListener('click', () => {
      li.remove();
    });
    outletList.appendChild(li);
  }

  removePort(port) {
    const card = port.closest('.network-card-item');
    if (!card) return;
    const ports = card.querySelectorAll('.nc-port-item');
    if (ports.length <= 1) {
      showToast(t('device.at_least_one_port') || '至少需要保留一个网口', 'warning');
      return;
    }
    port.remove();
  }

  async createIpRowElement(ipData = null) {
    await this.ensureOptionsLoaded();
    const div = document.createElement('div');
    div.className = 'nc-ip-item';
    const uid = generateUniqueId('ip');

    const regionOptions = this.regions.map(r => `<option value="${r.id}">${escapeHtml(r.name)}</option>`).join('');

    div.innerHTML = `
      <header class="ip-level-bar">
        <h3 id="${uid}-title" class="level-badge level-ip">IP</h3>
        <div class="level-actions">
          <button type="button" class="btn btn-danger btn-sm remove-ip-btn" aria-label="${t('ip.delete_ip') || '删除此IP'}">${t('common.delete') || '删除'}</button>
        </div>
      </header>
      <div class="nc-fields">
        <div class="nc-field">
          <label for="${uid}-region">${t('network.region') || '网络区域'}</label>
          <select id="${uid}-region" class="ip-region nc-input">
            <option value="">${t('network.select_region') || '选择网络区域'}</option>
            ${regionOptions}
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-network">${t('network.name') || '网络'}<abbr title="required" class="required" aria-hidden="true">*</abbr></label>
          <select id="${uid}-network" class="ip-network nc-input" required>
            <option value="">${t('network.select_network') || '选择网络'}</option>
          </select>
        </div>
        <div class="nc-field">
          <label for="${uid}-address">${t('ip.ip_address') || 'IP地址'}<abbr title="required" class="required" aria-hidden="true">*</abbr></label>
          <input id="${uid}-address" type="text" class="ip-address nc-input" value="${escapeHtml(ipData?.ip_address || '')}" placeholder="192.168.1.100" autocomplete="off" required />
        </div>
        <div class="nc-field">
          <label for="${uid}-description">${t('common.description') || '描述'}</label>
          <input id="${uid}-description" type="text" class="ip-description nc-input" value="${escapeHtml(ipData?.description || '')}" autocomplete="off" />
        </div>
      </div>
    `;

    return { element: div };
  }

  async bindIpRowEvents(ipRow, ipData = null) {
    const { element } = ipRow;
    element.querySelector('.remove-ip-btn')?.addEventListener('click', () => this.removeIp(element));

    const regionSelect = element.querySelector('.ip-region');
    const networkSelect = element.querySelector('.ip-network');

    regionSelect?.addEventListener('change', async () => {
      const regionId = regionSelect.value;
      networkSelect.innerHTML = `<option value="">${t('network.select_network') || '选择网络'}</option>`;
      if (!regionId) return;
      const networks = await this.loadNetworksByRegion(regionId);
      this.networks = this.mergeNetworks(networks);
      networkSelect.innerHTML = `<option value="">${t('network.select_network') || '选择网络'}</option>` +
        networks.map(n => {
          const cidrs = [];
          if (n.ipv4_cidr) cidrs.push(n.ipv4_cidr);
          if (n.ipv6_cidr) cidrs.push(n.ipv6_cidr);
          const cidrStr = cidrs.length > 0 ? cidrs.join(' / ') : '';
          return `<option value="${n.id}">${escapeHtml(n.name)}${cidrStr ? ` (${cidrStr})` : ''}</option>`;
        }).join('');
    });

    networkSelect?.addEventListener('change', () => {
      const networkId = networkSelect.value;
      const network = this.networks.find(n => n.id === networkId);
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
        regionSelect.dispatchEvent(new Event('change'));
      }
      if (ipData.network_id && networkSelect) {
        networkSelect.value = ipData.network_id;
      }
    }
  }

  removeIp(ipElement) {
    const port = ipElement.closest('.nc-port-item');
    if (!port) return;
    const ips = port.querySelectorAll('.nc-ip-item');
    if (ips.length <= 1) {
      showToast(t('device.at_least_one_ip') || '至少需要保留一个IP', 'warning');
      return;
    }
    ipElement.remove();
  }

  mergeNetworks(newNetworks) {
    const map = new Map(this.networks.map(n => [n.id, n]));
    for (const n of newNetworks) map.set(n.id, n);
    return Array.from(map.values());
  }

  async loadOutlets(outletAvailableSelect, roomId) {
    if (!outletAvailableSelect) return [];
    // 保留占位项
    const placeholder = `<option value="">${t('device.select_net_outlet_to_add') || '选择信息点添加'}</option>`;
    outletAvailableSelect.innerHTML = placeholder;
    if (!roomId) return [];
    try {
      const result = await apiGet('/api/resources/net-outlets?room_id=' + roomId + '&page_size=1000');
      if (result.success && result.data) {
        const outlets = Array.isArray(result.data) ? result.data : (result.data.items || []);
        // 缓存信息点数据（含 peer 信息），用于 collectData 验证
        this.outletsCache = outlets;
        outlets.forEach(outlet => {
          const option = document.createElement('option');
          option.value = outlet.id;
          option.textContent = outlet.name || outlet.code || outlet.id;
          outletAvailableSelect.appendChild(option);
        });
        return outlets;
      }
    } catch (error) {
      console.error('加载信息点失败:', error);
    }
    return [];
  }

  async loadSwitches(switchSelect, portSelect) {
    if (!switchSelect || !portSelect) return;
    await this.ensureOptionsLoaded();

    switchSelect.innerHTML = `<option value="">${t('device.select_upstream_device') || '选择设备'}</option>`;
    let devices = this.devicesCache || [];
    if (this.excludeSwitchId) {
      devices = devices.filter(d => d.id !== this.excludeSwitchId);
    }
    devices.forEach(d => {
      const option = document.createElement('option');
      option.value = d.id;
      option.textContent = d.name;
      switchSelect.appendChild(option);
    });

    switchSelect.onchange = () => this.handleSwitchChange(switchSelect, portSelect);
  }

  async handleSwitchChange(switchSelect, portSelect) {
    const deviceId = switchSelect.value;
    portSelect.innerHTML = `<option value="">${t('device.select_upstream_port') || '选择端口'}</option>`;
    if (!deviceId) return;
    try {
      const result = await apiGet(`/api/resources/devices/${deviceId}/interfaces`);
      if (result.success && result.data) {
        const interfaces = Array.isArray(result.data) ? result.data : (result.data.items || []);
        interfaces.forEach(iface => {
          const option = document.createElement('option');
          option.value = iface.id;
          const typeMark = iface.interface_type ? `[${iface.interface_type}]` : '';
          option.textContent = `${iface.name}${typeMark}`;
          portSelect.appendChild(option);
        });
      }
    } catch (error) {
      console.error('加载设备接口失败:', error);
    }
  }

  async loadExisting(cards) {
    const container = this.getContainer();
    if (!container) return;
    container.innerHTML = '';
    this.bindAddButton();
    await this.ensureOptionsLoaded();

    if (!cards || cards.length === 0) {
      await this.addCard();
      return;
    }

    for (const cardData of cards) {
      await this.addCard(cardData);
    }
  }

  collectData() {
    const container = this.getContainer();
    if (!container) return { cards: [], errors: [] };
    const cards = [];
    const errors = [];
    const cardElements = container.querySelectorAll('.network-card-item');

    cardElements.forEach((cardEl, cardIdx) => {
      const cardNum = cardIdx + 1;
      const cardId = cardEl.querySelector('.card-id')?.value || null;
      const cardName = cardEl.querySelector('.card-name')?.value?.trim() || '';
      const cardType = cardEl.querySelector('.card-type')?.value || 'pcie';
      const cardDesc = cardEl.querySelector('.card-description')?.value?.trim() || null;

      if (!cardName) {
        errors.push(`${t('device.network_card') || '网卡'} ${cardNum}: ${t('device.card_name_required') || '网卡名称不能为空'}`);
        return;
      }

      const ports = [];
      const portElements = cardEl.querySelectorAll('.nc-port-item');
      portElements.forEach((portEl, portIdx) => {
        const portNum = portIdx + 1;
        const portId = portEl.querySelector('.port-id')?.value || null;
        const portName = portEl.querySelector('.port-name')?.value?.trim() || '';
        const portType = portEl.querySelector('.port-type')?.value || 'physical';
        const portMac = portEl.querySelector('.port-mac')?.value?.trim() || null;
        const portVlanRaw = portEl.querySelector('.port-vlan')?.value?.trim() || '';
        const portVlan = portVlanRaw ? parseInt(portVlanRaw, 10) : null;
        const portDesc = portEl.querySelector('.port-description')?.value?.trim() || null;

        if (!portName) {
          errors.push(`${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum}: ${t('device.port_name_required') || '网口名称不能为空'}`);
          return;
        }
        if (portVlan !== null && (isNaN(portVlan) || portVlan < 1 || portVlan > 4094)) {
          errors.push(`${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum}: ${t('device.vlan_invalid') || 'VLAN ID 必须为 1-4094'}`);
          return;
        }

        const ips = [];
        const ipElements = portEl.querySelectorAll('.nc-ip-item');
        ipElements.forEach((ipEl, ipIdx) => {
          const ipNum = ipIdx + 1;
          const networkId = ipEl.querySelector('.ip-network')?.value || null;
          const ipAddress = ipEl.querySelector('.ip-address')?.value?.trim() || '';
          const ipDescription = ipEl.querySelector('.ip-description')?.value?.trim() || null;
          const networkRegionId = ipEl.querySelector('.ip-region')?.value || null;

          if (!networkId && !ipAddress && !ipDescription) return;

          if (!networkId) {
            errors.push(`${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum} - IP ${ipNum}: ${t('device.network_required') || '请选择网络'}`);
            return;
          }
          if (!ipAddress) {
            errors.push(`${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum} - IP ${ipNum}: ${t('device.ip_required') || '请输入IP地址'}`);
            return;
          }
          if (!isValidIP(ipAddress)) {
            errors.push(`${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum} - IP ${ipNum}: ${t('device.ip_invalid') || 'IP地址格式无效'}`);
            return;
          }

          const cidrResult = this.getNetworkCidr(networkId, ipAddress);
          if (!cidrResult.hasCidr) {
            const ipType = isIPv6(ipAddress) ? 'IPv6' : 'IPv4';
            errors.push(`${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum} - IP ${ipNum}: ${t('device.cidr_not_supported') || '所选网络不支持该'} ${ipType} ${t('device.address') || '地址'}`);
            return;
          }
          if (cidrResult.cidr && !isIpInCidr(ipAddress, cidrResult.cidr)) {
            errors.push(`${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum} - IP ${ipNum}: ${t('device.ip_not_in_cidr') || 'IP地址不在网段内'} ${cidrResult.cidr}`);
            return;
          }

          const ipData = {
            network_id: networkId,
            ip_address: ipAddress,
            description: ipDescription,
          };
          if (networkRegionId) ipData.network_region_id = networkRegionId;
          ips.push(ipData);
        });

        if (ips.length === 0) {
          errors.push(`${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum}: ${t('device.at_least_one_ip') || '至少需要保留一个IP'}`);
        }

        const portSwitchId = portEl.querySelector('.port-switch')?.value || null;
        const portUplinkInterfaceId = portEl.querySelector('.port-port')?.value || null;
        // 从有序列表中读取 net_outlet_ids
        const outletItems = portEl.querySelectorAll('.nc-outlet-item');
        const portOutletIds = Array.from(outletItems).map(li => li.dataset.outletId).filter(Boolean);

        // 信息点链验证：非最后信息点必须有对端（peer_type 不为空）
        // 上级端口的自动推导由后端在保存时根据最后一个信息点的 peer_switch_port_id 完成
        if (portOutletIds.length > 1) {
          for (let i = 0; i < portOutletIds.length - 1; i++) {
            const outletData = this.outletsCache.find(o => o.id === portOutletIds[i]);
            if (outletData && !outletData.peer_type) {
              errors.push(
                `${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum}: ` +
                `${t('device.outlet_must_have_peer') || '链路中除最后一个信息点外，其他信息点必须配置对端'} (${outletData.name || outletData.id})`
              );
            }
          }
        }

        ports.push({
          id: portId,
          name: portName,
          interface_type: portType,
          mac_address: portMac,
          vlan_id: portVlan,
          description: portDesc,
          ips,
          switch_id: portSwitchId,
          uplink_interface_id: portUplinkInterfaceId,
          net_outlet_ids: portOutletIds,
        });
      });

      if (ports.length === 0) {
        errors.push(`${t('device.network_card') || '网卡'} ${cardNum}: ${t('device.at_least_one_port') || '至少需要保留一个网口'}`);
      }

      cards.push({
        id: cardId,
        name: cardName,
        card_type: cardType,
        description: cardDesc,
        ports,
      });
    });

    return { cards, errors };
  }
}

const managerInstance = new NetworkCardManager();

export function getNetworkCardManager() {
  return managerInstance;
}
