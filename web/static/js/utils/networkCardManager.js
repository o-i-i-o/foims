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

const CARD_TYPES = [
  { value: 'physical', label: 'physical' },
  { value: 'management', label: 'management' },
  { value: 'wifi', label: 'wifi' },
  { value: 'fiber', label: 'fiber' },
  { value: 'other', label: 'other' },
];

const PORT_TYPES = [
  { value: 'physical', label: 'physical' },
  { value: 'svi', label: 'svi' },
  { value: 'management', label: 'management' },
  { value: 'loopback', label: 'loopback' },
  { value: 'wifi', label: 'wifi' },
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
    const card = this.createCardElement(cardData);
    container.appendChild(card);
    await this.bindCardEvents(card, cardData);
  }

  createCardElement(cardData = {}) {
    const div = document.createElement('div');
    div.className = 'network-card-item';
    div.innerHTML = `
      <div class="card-level-bar">
        <span class="level-badge level-card">${t('device.network_card') || '网卡'}</span>
        <div class="level-actions">
          <button type="button" class="btn btn-danger btn-sm remove-card-btn">${t('common.delete') || '删除'}</button>
          <button type="button" class="btn btn-secondary btn-sm add-port-btn">${t('device.add_network_port') || '添加网口'}</button>
        </div>
      </div>
      <div class="nc-fields">
        <div class="nc-field">
          <label>${t('device.network_card_name') || '网卡名称'}<span class="required">*</span></label>
          <input type="hidden" class="card-id" value="${escapeHtml(cardData.id || '')}" />
          <input type="text" class="card-name nc-input" value="${escapeHtml(cardData.name || DEFAULT_CARD_NAME)}" placeholder="${t('device.network_card_name') || '网卡名称'}" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label>${t('device.network_card_type') || '网卡类型'}</label>
          <select class="card-type nc-input">
            ${CARD_TYPES.map(opt => `<option value="${opt.value}" ${cardData.card_type === opt.value ? 'selected' : ''}>${opt.label}</option>`).join('')}
          </select>
        </div>
        <div class="nc-field">
          <label>${t('ip.mac_address') || 'MAC地址'}</label>
          <input type="text" class="card-mac nc-input" value="${escapeHtml(cardData.mac_address || '')}" placeholder="00:11:22:33:44:55" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label>${t('common.description') || '描述'}</label>
          <input type="text" class="card-description nc-input" value="${escapeHtml(cardData.description || '')}" autocomplete="off" />
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
    div.className = 'port-item';
    div.innerHTML = `
      <div class="port-level-bar">
        <span class="level-badge level-port">${t('device.network_port') || '网口'}</span>
        <div class="level-actions">
          <button type="button" class="btn btn-danger btn-sm remove-port-btn">${t('common.delete') || '删除'}</button>
          <button type="button" class="btn btn-secondary btn-sm add-ip-btn">${t('ip.add_ip') || '添加IP'}</button>
        </div>
      </div>
      <div class="nc-fields">
        <div class="nc-field">
          <label>${t('device.network_port_name') || '网口名称'}<span class="required">*</span></label>
          <input type="hidden" class="port-id" value="${escapeHtml(portData.id || '')}" />
          <input type="text" class="port-name nc-input" value="${escapeHtml(portData.name || DEFAULT_PORT_NAME)}" placeholder="${t('device.network_port_name') || '网口名称'}" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label>${t('device.network_port_type') || '网口类型'}</label>
          <select class="port-type nc-input">
            ${PORT_TYPES.map(opt => `<option value="${opt.value}" ${portData.interface_type === opt.value ? 'selected' : ''}>${opt.label}</option>`).join('')}
          </select>
        </div>
        <div class="nc-field">
          <label>${t('device.vlan_id') || 'VLAN ID'}</label>
          <input type="number" class="port-vlan nc-input" value="${portData.vlan_id ?? ''}" min="1" max="4094" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label>${t('ip.mac_address') || 'MAC地址'}</label>
          <input type="text" class="port-mac nc-input" value="${escapeHtml(portData.mac_address || '')}" placeholder="00:11:22:33:44:55" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label>${t('common.description') || '描述'}</label>
          <input type="text" class="port-description nc-input" value="${escapeHtml(portData.description || '')}" autocomplete="off" />
        </div>
      </div>
      <div class="port-ips-container"></div>
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

  removePort(port) {
    const card = port.closest('.network-card-item');
    if (!card) return;
    const ports = card.querySelectorAll('.port-item');
    if (ports.length <= 1) {
      showToast(t('device.at_least_one_port') || '至少需要保留一个网口', 'warning');
      return;
    }
    port.remove();
  }

  async createIpRowElement(ipData = null) {
    await this.ensureOptionsLoaded();
    const div = document.createElement('div');
    div.className = 'ip-item';

    const regionOptions = this.regions.map(r => `<option value="${r.id}">${escapeHtml(r.name)}</option>`).join('');

    div.innerHTML = `
      <div class="ip-level-bar">
        <span class="level-badge level-ip">IP</span>
        <div class="level-actions">
          <button type="button" class="btn btn-danger btn-sm remove-ip-btn">${t('common.delete') || '删除'}</button>
        </div>
      </div>
      <div class="nc-fields">
        <div class="nc-field">
          <label>${t('network.region') || '网络区域'}</label>
          <select class="ip-region nc-input">
            <option value="">${t('network.select_region') || '选择网络区域'}</option>
            ${regionOptions}
          </select>
        </div>
        <div class="nc-field">
          <label>${t('network.name') || '网络'}<span class="required">*</span></label>
          <select class="ip-network nc-input" required>
            <option value="">${t('network.select_network') || '选择网络'}</option>
          </select>
        </div>
        <div class="nc-field">
          <label>${t('ip.ip_address') || 'IP地址'}<span class="required">*</span></label>
          <input type="text" class="ip-address nc-input" value="${escapeHtml(ipData?.ip_address || '')}" placeholder="192.168.1.100" autocomplete="off" />
        </div>
      </div>
      <div class="nc-fields">
        <div class="nc-field">
          <label>${t('ip.mac_address') || 'MAC地址'}</label>
          <input type="text" class="ip-mac nc-input" value="${escapeHtml(ipData?.mac_address || '')}" placeholder="00:11:22:33:44:55" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label>${t('ip.hostname') || '主机名'}</label>
          <input type="text" class="ip-hostname nc-input" value="${escapeHtml(ipData?.hostname || '')}" autocomplete="off" />
        </div>
        <div class="nc-field">
          <label>${t('device.upstream_device') || '上级设备'}</label>
          <select class="ip-switch nc-input">
            <option value="">${t('device.select_upstream_device') || '选择设备'}</option>
          </select>
        </div>
        <div class="nc-field">
          <label>${t('device.upstream_port') || '上级端口'}</label>
          <select class="ip-port nc-input">
            <option value="">${t('device.select_upstream_port') || '选择端口'}</option>
          </select>
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
    const switchSelect = element.querySelector('.ip-switch');
    const portSelect = element.querySelector('.ip-port');

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

    await this.loadSwitches(switchSelect, portSelect);

    if (ipData) {
      if (ipData.network_region_id && regionSelect) {
        regionSelect.value = ipData.network_region_id;
        regionSelect.dispatchEvent(new Event('change'));
      }
      if (ipData.network_id && networkSelect) {
        networkSelect.value = ipData.network_id;
      }
      if (ipData.switch_id && switchSelect) {
        switchSelect.value = ipData.switch_id;
        await this.handleSwitchChange(switchSelect, portSelect);
        if (ipData.device_interface_id && portSelect) {
          portSelect.value = ipData.device_interface_id;
        }
      }
    }
  }

  removeIp(ipElement) {
    const port = ipElement.closest('.port-item');
    if (!port) return;
    const ips = port.querySelectorAll('.ip-item');
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
      const cardType = cardEl.querySelector('.card-type')?.value || 'physical';
      const cardMac = cardEl.querySelector('.card-mac')?.value?.trim() || null;
      const cardDesc = cardEl.querySelector('.card-description')?.value?.trim() || null;

      if (!cardName) {
        errors.push(`${t('device.network_card') || '网卡'} ${cardNum}: ${t('device.card_name_required') || '网卡名称不能为空'}`);
        return;
      }

      const ports = [];
      const portElements = cardEl.querySelectorAll('.port-item');
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
        const ipElements = portEl.querySelectorAll('.ip-item');
        ipElements.forEach((ipEl, ipIdx) => {
          const ipNum = ipIdx + 1;
          const networkId = ipEl.querySelector('.ip-network')?.value || null;
          const ipAddress = ipEl.querySelector('.ip-address')?.value?.trim() || '';
          const ipMac = ipEl.querySelector('.ip-mac')?.value?.trim() || null;
          const ipHostname = ipEl.querySelector('.ip-hostname')?.value?.trim() || null;
          const switchId = ipEl.querySelector('.ip-switch')?.value || null;
          const portId = ipEl.querySelector('.ip-port')?.value || null;
          const networkRegionId = ipEl.querySelector('.ip-region')?.value || null;

          if (!networkId && !ipAddress && !ipMac) return;

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
            mac_address: ipMac,
            hostname: ipHostname || null,
          };
          if (networkRegionId) ipData.network_region_id = networkRegionId;
          if (switchId) ipData.switch_id = switchId;
          if (portId) ipData.device_interface_id = portId;
          ips.push(ipData);
        });

        if (ips.length === 0) {
          errors.push(`${t('device.network_card') || '网卡'} ${cardNum} - ${t('device.network_port') || '网口'} ${portNum}: ${t('device.at_least_one_ip') || '至少需要保留一个IP'}`);
        }

        ports.push({
          id: portId,
          name: portName,
          interface_type: portType,
          mac_address: portMac,
          vlan_id: portVlan,
          description: portDesc,
          ips,
        });
      });

      if (ports.length === 0) {
        errors.push(`${t('device.network_card') || '网卡'} ${cardNum}: ${t('device.at_least_one_port') || '至少需要保留一个网口'}`);
      }

      cards.push({
        id: cardId,
        name: cardName,
        card_type: cardType,
        mac_address: cardMac,
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
