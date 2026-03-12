import { apiGet } from "./apiClient.js";
import { showToast } from "./toast.js";

const NETWORK_REGION_CHANGED_EVENT = 'ipma:network-region-changed';

const regionChangeCallbacks = new WeakMap();

function isIPv6(ip) {
  return ip.includes(':');
}

function isValidIPv4(ip) {
  const parts = ip.split('.');
  if (parts.length !== 4) return false;
  for (const part of parts) {
    if (!/^\d+$/.test(part)) return false;
    const num = parseInt(part, 10);
    if (isNaN(num) || num < 0 || num > 255) return false;
    if (part.length > 1 && part.startsWith('0') && num !== 0) return false;
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
  
  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];
    if (part === '') continue;
    
    if (!/^[0-9a-fA-F]{1,4}$/.test(part)) return false;
  }
  
  return true;
}

function isValidIP(ip) {
  if (!ip || typeof ip !== 'string') return false;
  return isValidIPv4(ip) || isValidIPv6(ip);
}

function ipv4ToInt(ip) {
  const parts = ip.split('.').map(p => parseInt(p, 10));
  if (parts.length !== 4 || parts.some(p => isNaN(p) || p < 0 || p > 255)) {
    return null;
  }
  return (BigInt(parts[0]) << 24n) + (BigInt(parts[1]) << 16n) + (BigInt(parts[2]) << 8n) + BigInt(parts[3]);
}

function ipv6ToInt(ipv6) {
  let ip = ipv6.split('%')[0];
  
  if (ip === '::') {
    ip = '::0';
  }
  
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
  } else {
    const ipInt = ipv4ToInt(ipAddress);
    if (ipInt === null) return false;
    
    const range = ipv4CidrToRange(cidr);
    if (!range) return false;
    
    return ipInt >= range.start && ipInt <= range.end;
  }
}

export function dispatchNetworkRegionChange(regionId, regionName = '') {
  document.dispatchEvent(new CustomEvent(NETWORK_REGION_CHANGED_EVENT, {
    detail: { regionId, regionName }
  }));
}

export function onNetworkRegionChange(callback) {
  if (typeof callback !== 'function') {
    console.error("onNetworkRegionChange: callback must be a function");
    return;
  }
  const wrapper = (e) => callback(e.detail.regionId, e.detail.regionName);
  regionChangeCallbacks.set(callback, wrapper);
  document.addEventListener(NETWORK_REGION_CHANGED_EVENT, wrapper);
}

export function offNetworkRegionChange(callback) {
  const wrapper = regionChangeCallbacks.get(callback);
  if (wrapper) {
    document.removeEventListener(NETWORK_REGION_CHANGED_EVENT, wrapper);
    regionChangeCallbacks.delete(callback);
  }
}

const CONFIG = {
  workstation: {
    containerId: 'workstation-ips-container',
    classPrefix: 'workstation',
    networksApi: (id) => `/api/resources/rooms/${id}/networks`,
    idSelector: 'workstation-room',
    idName: '房间',
    parentSwitchRequired: false,
    switchLabel: '上级交换机',
    portLabel: '上级端口'
  },
  'cabinet-position': {
    containerId: 'cabinet-position-ips-container',
    classPrefix: 'cabinet-position',
    networksApi: (id) => `/api/resources/cabinets/${id}/networks`,
    idSelector: 'cabinet-position-cabinet',
    idName: '机柜',
    parentSwitchRequired: false,
    switchLabel: '上级交换机',
    portLabel: '上级端口'
  },
  switch: {
    containerId: 'switch-ips-container',
    classPrefix: 'switch',
    networksApi: (regionId) => regionId ? `/api/resources/networks?region_id=${regionId}&page_size=1000` : null,
    excludeSwitchId: null,
    parentSwitchRequired: false,
    switchLabel: '上级交换机',
    portLabel: '上级端口',
    loadNetworksByRegion: true
  }
};

let switchesCache = null;

export class IpConfigManager {
  constructor(resourceType) {
    this.resourceType = resourceType;
    if (!CONFIG[resourceType]) {
      console.error(`Invalid resource type: ${resourceType}`);
      this.config = null;
    } else {
      this.config = { ...CONFIG[resourceType] };
    }
    this.networks = [];
  }

  getContainer() {
    return document.getElementById(this.config.containerId);
  }

  clear() {
    const container = this.getContainer();
    if (container) {
      container.innerHTML = '';
    }
  }

  setExcludeSwitchId(switchId) {
    this.config.excludeSwitchId = switchId;
  }
  
  setNetworks(networks) {
    this.networks = networks || [];
  }
  
  getNetworkCidr(networkId) {
    const network = this.networks.find(n => n.id === networkId);
    return network?.ipv4_cidr || network?.ipv6_cidr || null;
  }

  getCidrForIp(networkId, ipAddress) {
    if (!networkId || !ipAddress) {
      return { cidr: null, hasCidr: false, isV6: false };
    }
    
    const network = this.networks.find(n => n.id === networkId);
    if (!network) return { cidr: null, hasCidr: false, isV6: false };
    
    const isV6 = isIPv6(ipAddress);
    const cidr = isV6 ? network.ipv6_cidr : network.ipv4_cidr;
    const hasCidr = !!cidr;
    
    return { cidr, hasCidr, isV6 };
  }

  async loadIps(ips) {
    this.clear();
    if (!ips || ips.length === 0) {
      await this.addIpRow();
      return;
    }

    try {
      let regions = [];
      let allNetworks = [];
      
      if (this.config.loadNetworksByRegion) {
        const regionsResult = await apiGet('/api/resources/network-regions?page_size=1000');
        if (!regionsResult.success) {
          throw new Error('加载网络区域失败');
        }
        regions = regionsResult.data?.items || regionsResult.data || [];
        
        const regionIds = new Set();
        for (const ip of ips) {
          if (ip.network_region_id) {
            regionIds.add(ip.network_region_id);
          }
        }
        
        if (regionIds.size === 0 && ips.some(ip => ip.network_id)) {
          const networkIds = ips.filter(ip => ip.network_id).map(ip => ip.network_id);
          const uniqueNetworkIds = [...new Set(networkIds)];
          const networkPromises = uniqueNetworkIds.map(id => 
            apiGet(`/api/resources/networks/${id}`).catch(() => null)
          );
          const networkResults = await Promise.all(networkPromises);
          for (const result of networkResults) {
            if (result?.success && result.data) {
              const network = result.data;
              if (network.network_region_id) {
                regionIds.add(network.network_region_id);
              }
              allNetworks.push(network);
            }
          }
        }
        
        if (regionIds.size > 0) {
          const networkPromises = Array.from(regionIds).map(regionId => {
            const url = this.config.networksApi(regionId);
            return url ? apiGet(url).catch(() => null) : null;
          }).filter(Boolean);
          
          const networkResults = await Promise.all(networkPromises);
          for (const result of networkResults) {
            if (result?.success && result.data) {
              const regionNetworks = result.data.items || result.data || [];
              allNetworks = allNetworks.concat(regionNetworks);
            }
          }
        }
      } else if (this.config.idSelector) {
        const id = document.getElementById(this.config.idSelector)?.value;
        if (!id) {
          throw new Error(`请先选择${this.config.idName}`);
        }
        const url = this.config.networksApi(id);
        if (url) {
          const result = await apiGet(url);
          if (!result.success) {
            throw new Error('加载网络数据失败');
          }
          allNetworks = result.data?.items || result.data || [];
        }
        
        const regionMap = new Map();
        for (const network of allNetworks) {
          if (network.network_region_id && network.network_region) {
            regionMap.set(network.network_region_id, {
              id: network.network_region_id,
              name: network.network_region
            });
          }
        }
        regions = Array.from(regionMap.values());
      }
      
      if (allNetworks.length > 0) {
        this.networks = allNetworks;
      }
      
      const container = this.getContainer();
      const fragment = document.createDocumentFragment();
      const rowElements = [];
      
      for (const ip of ips) {
        const row = this.createIpRowElement(regions, allNetworks);
        rowElements.push(row);
        fragment.appendChild(row);
      }
      
      container.appendChild(fragment);
      
      for (let i = 0; i < rowElements.length; i++) {
        await this.bindRowEvents(rowElements[i], allNetworks, ips[i]);
      }
      
    } catch (error) {
      console.error("加载IP失败:", error);
      showToast(error.message || "加载IP数据失败", "error");
      await this.addIpRow();
    }
  }

  getIps() {
    const container = this.getContainer();
    if (!container) return [];

    const rows = container.querySelectorAll(`.${this.config.classPrefix}-ip-row`);
    const ips = [];

    rows.forEach(row => {
      const networkRegionSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-region-select`);
      const networkSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-select`);
      const ipAddressInput = row.querySelector(`.${this.config.classPrefix}-ip-address-input`);
      const macAddressInput = row.querySelector(`.${this.config.classPrefix}-ip-mac-address-input`);
      const switchSelect = row.querySelector(`.${this.config.classPrefix}-switch-select`);
      const switchPortSelect = row.querySelector(`.${this.config.classPrefix}-switch-port-select`);

      if (networkSelect && ipAddressInput) {
        const networkId = networkSelect.value;
        const ipAddress = ipAddressInput.value ? ipAddressInput.value.trim() : '';
        
        if (networkId && ipAddress) {
          const ipData = {
            network_id: networkId,
            ip_address: ipAddress,
            mac_address: macAddressInput && macAddressInput.value ? macAddressInput.value.trim() : null,
          };

          const switchId = switchSelect?.value || '';
          const portId = switchPortSelect?.value || '';

          if (this.resourceType === 'switch') {
            ipData.device_type = 'switch';
            if (switchId) ipData.parent_switch_id = switchId;
            if (portId) ipData.parent_port_id = portId;
          } else if (this.resourceType === 'cabinet-position') {
            if (switchId) {
              ipData.device_type = 'switch';
              ipData.switch_id = switchId;
              if (portId) ipData.switch_port_id = portId;
            } else {
              ipData.device_type = 'cabinet_position';
            }
          } else if (this.resourceType === 'workstation') {
            ipData.device_type = 'workstation';
            if (switchId) ipData.switch_id = switchId;
            if (portId) ipData.switch_port_id = portId;
          }
          
          if (networkRegionSelect && networkRegionSelect.value) {
            ipData.network_region_id = networkRegionSelect.value;
          }

          ips.push(ipData);
        }
      }
    });

    return ips;
  }

  validateIps() {
    const container = this.getContainer();
    if (!container) return { valid: true, ips: [] };

    const rows = container.querySelectorAll(`.${this.config.classPrefix}-ip-row`);
    const ips = [];
    const errors = [];

    rows.forEach((row, index) => {
      const rowNum = index + 1;
      const networkRegionSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-region-select`);
      const networkSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-select`);
      const ipAddressInput = row.querySelector(`.${this.config.classPrefix}-ip-address-input`);
      const macAddressInput = row.querySelector(`.${this.config.classPrefix}-ip-mac-address-input`);
      const switchSelect = row.querySelector(`.${this.config.classPrefix}-switch-select`);
      const switchPortSelect = row.querySelector(`.${this.config.classPrefix}-switch-port-select`);

      const networkId = networkSelect?.value || '';
      const ipAddress = ipAddressInput?.value?.trim() || '';
      const macAddress = macAddressInput?.value?.trim() || null;
      const switchId = switchSelect?.value || '';
      const portId = switchPortSelect?.value || '';
      const networkRegionId = networkRegionSelect?.value || '';

      const hasData = networkId || ipAddress || macAddress || switchId || portId;
      
      if (!hasData) return;

      if (!networkId) {
        errors.push(`第${rowNum}行：请选择网络`);
      }
      if (!ipAddress) {
        errors.push(`第${rowNum}行：请输入IP地址`);
      } else if (!isValidIP(ipAddress)) {
        errors.push(`第${rowNum}行：IP地址格式无效`);
      }
      
      if (networkId && ipAddress) {
        const result = this.getCidrForIp(networkId, ipAddress);
        if (!result.hasCidr) {
          const ipType = result.isV6 ? 'IPv6' : 'IPv4';
          errors.push(`第${rowNum}行：所选网络不支持${ipType}地址`);
        } else if (result.cidr && !isIpInCidr(ipAddress, result.cidr)) {
          errors.push(`第${rowNum}行：IP地址 ${ipAddress} 不在所选网段 ${result.cidr} 内`);
        }
      }

      if (this.config.parentSwitchRequired) {
        if (!switchId) {
          errors.push(`第${rowNum}行：请选择上级交换机`);
        }
        if (!portId) {
          errors.push(`第${rowNum}行：请选择上级端口`);
        }
      }

      if (networkId && ipAddress) {
        const ipData = {
          network_id: networkId,
          ip_address: ipAddress,
          mac_address: macAddress || null,
        };

        if (this.resourceType === 'switch') {
          if (switchId) ipData.parent_switch_id = switchId;
          if (portId) ipData.parent_port_id = portId;
        } else {
          if (switchId) ipData.switch_id = switchId;
          if (portId) ipData.switch_port_id = portId;
        }
        if (networkRegionId) {
          ipData.network_region_id = networkRegionId;
        }

        ips.push(ipData);
      }
    });

    if (errors.length > 0) {
      return { valid: false, errors, ips: [] };
    }

    return { valid: true, ips };
  }

  async addIpRow(initialData = null) {
    const container = this.getContainer();
    if (!container) return;

    if (this.config.idSelector && !initialData) {
      const id = document.getElementById(this.config.idSelector)?.value;
      if (!id) {
        showToast(`请先选择${this.config.idName}`, "warning");
        return;
      }
    }

    try {
      let networks = [];
      let regions = [];
      
      if (this.config.loadNetworksByRegion) {
        const regionsResult = await apiGet('/api/resources/network-regions?page_size=1000');
        if (regionsResult.success && regionsResult.data) {
          regions = regionsResult.data.items || regionsResult.data || [];
        }
        
        if (initialData) {
          networks = await this.loadNetworks(initialData);
          if (networks.length > 0) {
            this.networks = networks;
          }
        }
      } else {
        networks = await this.loadNetworks(initialData);
        if (networks.length > 0) {
          this.networks = networks;
        }
        
        const regionMap = new Map();
        networks.forEach(network => {
          if (network.network_region_id && network.network_region) {
            regionMap.set(network.network_region_id, {
              id: network.network_region_id,
              name: network.network_region
            });
          }
        });
        regions = Array.from(regionMap.values());
      }

      const row = this.createIpRowElement(regions, networks || []);
      container.appendChild(row);

      await this.bindRowEvents(row, networks || [], initialData);

    } catch (error) {
      console.error("添加IP行失败:", error);
      showToast("加载选项失败", "error");
    }
  }

  async loadNetworks(initialData) {
    let url;
    if (this.config.idSelector) {
      const id = document.getElementById(this.config.idSelector)?.value;
      if (id) {
        url = this.config.networksApi(id);
      } else {
        return [];
      }
    } else if (this.config.loadNetworksByRegion) {
      let regionId = initialData?.network_region_id;
      if (!regionId && initialData?.network_id) {
        const allNetworksResult = await apiGet('/api/resources/networks?page_size=10000');
        if (allNetworksResult.success && allNetworksResult.data) {
          const allNetworks = allNetworksResult.data.items || allNetworksResult.data || [];
          const network = allNetworks.find(n => n.id === initialData.network_id);
          if (network) {
            regionId = network.network_region_id;
          }
        }
      }
      if (regionId) {
        url = this.config.networksApi(regionId);
      } else {
        return [];
      }
    } else {
      url = this.config.networksApi();
    }
    
    if (!url) return [];
    
    const result = await apiGet(url);
    if (!result.success || !result.data) return [];
    if (Array.isArray(result.data)) return result.data;
    if (result.data.items && Array.isArray(result.data.items)) return result.data.items;
    return [];
  }

  createIpRowElement(regions, networks) {
    const div = document.createElement("div");
    div.className = `${this.config.classPrefix}-ip-row form-row-container`;
    
    const regionOptions = regions.map(r => `<option value="${r.id}">${r.name}</option>`).join('');
    const networkOptions = networks.map(n => {
      const cidrs = [];
      if (n.ipv4_cidr) cidrs.push(n.ipv4_cidr);
      if (n.ipv6_cidr) cidrs.push(n.ipv6_cidr);
      const cidrStr = cidrs.length > 0 ? cidrs.join(' / ') : '无CIDR';
      return `<option value="${n.id}">${n.name} (${cidrStr})</option>`;
    }).join('');

    const switchLabel = this.config.switchLabel || '上级交换机';
    const portLabel = this.config.portLabel || '上级端口';
    const requiredMark = this.config.parentSwitchRequired ? '<span class="required">*</span>' : '';

    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <label>网络区域</label>
          <select class="${this.config.classPrefix}-ip-network-region-select form-control">
            <option value="">请选择区域</option>
            ${regionOptions}
          </select>
        </div>
        <div class="form-group">
          <label>网络 <span class="required">*</span></label>
          <select class="${this.config.classPrefix}-ip-network-select form-control">
            <option value="">请选择网络</option>
            ${networkOptions}
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label>IP地址 <span class="required">*</span></label>
          <input type="text" class="${this.config.classPrefix}-ip-address-input form-control" placeholder="如: 192.168.1.100">
        </div>
        <div class="form-group">
          <label>MAC地址</label>
          <input type="text" class="${this.config.classPrefix}-ip-mac-address-input form-control" placeholder="如: 00:11:22:33:44:55">
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label>${switchLabel} ${requiredMark}</label>
          <select class="${this.config.classPrefix}-switch-select form-control">
            <option value="">选择交换机</option>
          </select>
        </div>
        <div class="form-group">
          <label>${portLabel} ${requiredMark}</label>
          <select class="${this.config.classPrefix}-switch-port-select form-control">
            <option value="">选择端口</option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group" style="display: flex; align-items: flex-end; gap: 8px;">
          <button type="button" class="btn btn-danger btn-sm remove-ip-btn">删除</button>
          <button type="button" class="btn btn-secondary btn-sm add-ip-btn" data-i18n="ip.add_ip">添加IP地址</button>
        </div>
      </div>
    `;
    return div;
  }

  async bindRowEvents(row, networks, initialData) {
    row.querySelector(".remove-ip-btn")?.addEventListener("click", () => {
      const container = this.getContainer();
      const rows = container?.querySelectorAll(`.${this.config.classPrefix}-ip-row`);
      if (rows && rows.length > 1) {
        row.remove();
      } else {
        showToast("至少需要保留一个IP配置", "warning");
      }
    });
    
    row.querySelector(".add-ip-btn")?.addEventListener("click", () => this.addIpRow());

    const regionSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-region-select`);
    const networkSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-select`);
    const ipInput = row.querySelector(`.${this.config.classPrefix}-ip-address-input`);
    const macInput = row.querySelector(`.${this.config.classPrefix}-ip-mac-address-input`);
    const switchSelect = row.querySelector(`.${this.config.classPrefix}-switch-select`);
    const portSelect = row.querySelector(`.${this.config.classPrefix}-switch-port-select`);

    if (regionSelect && networkSelect) {
      regionSelect.addEventListener("change", async () => {
        const regionId = regionSelect.value;
        const selectedOption = regionSelect.options[regionSelect.selectedIndex];
        const regionName = selectedOption ? selectedOption.text : '';
        
        if (this.config.loadNetworksByRegion && regionId) {
          const url = this.config.networksApi(regionId);
          if (url) {
            const result = await apiGet(url);
            if (result.success && result.data) {
              const regionNetworks = result.data.items || result.data || [];
              networks = regionNetworks;
              this.networks = regionNetworks;
            }
          }
        }
        
        const filtered = networks.filter(n => n.network_region_id === regionId);
        
        networkSelect.innerHTML = '<option value="">请选择网络</option>' + 
          filtered.map(n => {
            const cidrs = [];
            if (n.ipv4_cidr) cidrs.push(n.ipv4_cidr);
            if (n.ipv6_cidr) cidrs.push(n.ipv6_cidr);
            const cidrStr = cidrs.length > 0 ? cidrs.join(' / ') : '无CIDR';
            return `<option value="${n.id}">${n.name} (${cidrStr})</option>`;
          }).join('');
        
        if (filtered.length === 1) {
          networkSelect.value = filtered[0].id;
        }

        this.filterSwitchesByRegion(switchSelect, portSelect, regionId);
        
        if (this.resourceType === 'switch') {
          dispatchNetworkRegionChange(regionId, regionName);
        }
      });
    }

    if (networkSelect) {
      networkSelect.addEventListener("change", () => {
        const networkId = networkSelect.value;
        const network = this.networks.find(n => n.id === networkId);
        if (network) {
          this.filterSwitchesByRegion(switchSelect, portSelect, network.network_region_id);
        }
      });
    }

    await this.loadSwitches(switchSelect, portSelect);

    if (initialData) {
      let regionId = initialData.network_region_id;
      if (!regionId && initialData.network_id) {
        const network = this.networks.find(n => n.id === initialData.network_id);
        if (network) {
          regionId = network.network_region_id;
        }
      }

      if (regionId && regionSelect) {
        regionSelect.value = regionId;
        if (this.config.loadNetworksByRegion) {
          const url = this.config.networksApi(regionId);
          if (url) {
            const result = await apiGet(url);
            if (result.success && result.data) {
              const regionNetworks = result.data.items || result.data || [];
              this.networks = regionNetworks;
              networkSelect.innerHTML = '<option value="">请选择网络</option>' + 
                regionNetworks.map(n => {
                  const cidrs = [];
                  if (n.ipv4_cidr) cidrs.push(n.ipv4_cidr);
                  if (n.ipv6_cidr) cidrs.push(n.ipv6_cidr);
                  const cidrStr = cidrs.length > 0 ? cidrs.join(' / ') : '无CIDR';
                  return `<option value="${n.id}">${n.name} (${cidrStr})</option>`;
                }).join('');
            }
          }
        } else {
          regionSelect.dispatchEvent(new Event('change'));
        }
      }

      if (initialData.network_id && networkSelect) {
        networkSelect.value = initialData.network_id;
      }

      if (initialData.ip_address && ipInput) ipInput.value = initialData.ip_address;
      if (initialData.mac_address && macInput) macInput.value = initialData.mac_address;

      const switchId = initialData.parent_switch_id || initialData.switch_id;
      const portId = initialData.parent_port_id || initialData.switch_port_id || initialData.port_id;

      if (regionId) {
        this.filterSwitchesByRegion(switchSelect, portSelect, regionId);
      }

      if (switchId && switchSelect) {
        const normalizedSwitchId = typeof switchId === 'string' ? switchId.toLowerCase() : switchId;
        
        const switchOptions = switchSelect.querySelectorAll('option');
        for (const opt of switchOptions) {
          if (opt.value && opt.value.toLowerCase() === normalizedSwitchId) {
            switchSelect.value = opt.value;
            break;
          }
        }
      }

      if (portId && portSelect) {
        await this.handleSwitchChange(switchSelect, portSelect);
        const normalizedPortId = typeof portId === 'string' ? portId.toLowerCase() : portId;
        const portOptions = portSelect.querySelectorAll('option');
        for (const opt of portOptions) {
          if (opt.value && opt.value.toLowerCase() === normalizedPortId) {
            portSelect.value = opt.value;
            break;
          }
        }
      }
    } else {
      if (regionSelect) {
        regionSelect.dispatchEvent(new Event('change'));
      }
    }
  }

  async loadSwitches(switchSelect, portSelect) {
    if (!switchSelect || !portSelect) return;

    try {
      const result = await apiGet("/api/switches");
      let switchesData = [];
      if (result.success && result.data) {
        if (Array.isArray(result.data)) {
          switchesData = result.data;
        } else if (result.data.items && Array.isArray(result.data.items)) {
          switchesData = result.data.items;
        }
      }
      switchesCache = switchesData;
      
      switchSelect.innerHTML = '<option value="">选择交换机</option>';
      
      let filteredSwitches = switchesCache;
      if (this.resourceType === 'switch' && this.config.excludeSwitchId) {
        filteredSwitches = switchesCache.filter(sw => sw.id !== this.config.excludeSwitchId);
      }
      
      filteredSwitches.forEach(sw => {
        const option = document.createElement("option");
        option.value = sw.id;
        option.textContent = sw.name;
        switchSelect.appendChild(option);
      });

      switchSelect.addEventListener("change", () => this.handleSwitchChange(switchSelect, portSelect));
    } catch (error) {
      console.error("加载交换机失败:", error);
    }
  }

  filterSwitchesByRegion(switchSelect, portSelect, regionId) {
    if (!switchSelect || !switchesCache) return;

    if (portSelect) {
      portSelect.innerHTML = '<option value="">选择端口</option>';
    }

    const currentSwitchId = switchSelect.value;

    switchSelect.innerHTML = '<option value="">选择交换机</option>';

    let filteredSwitches = switchesCache;
    
    if (regionId) {
      filteredSwitches = switchesCache.filter(sw => sw.network_region_id === regionId);
    }

    if (this.resourceType === 'switch' && this.config.excludeSwitchId) {
      filteredSwitches = filteredSwitches.filter(sw => sw.id !== this.config.excludeSwitchId);
    }

    filteredSwitches.forEach(sw => {
      const option = document.createElement("option");
      option.value = sw.id;
      option.textContent = sw.name;
      switchSelect.appendChild(option);
    });

    if (currentSwitchId && filteredSwitches.some(sw => sw.id === currentSwitchId)) {
      switchSelect.value = currentSwitchId;
    }
  }

  async handleSwitchChange(switchSelect, portSelect) {
    const switchId = switchSelect.value;
    portSelect.innerHTML = '<option value="">选择端口</option>';
    
    if (!switchId) return;
    
    try {
      const result = await apiGet(`/api/switches/${switchId}/ports`);
      if (result.success && result.data) {
          let ports;
          if (Array.isArray(result.data)) {
            ports = result.data;
          } else if (result.data.items && Array.isArray(result.data.items)) {
            ports = result.data.items;
          } else {
            ports = [];
          }
          
          ports.sort((a, b) => {
              const getNum = (s) => {
                  if (typeof s !== 'string' || !s) return 0;
                  const m = s.match(/\d+/g);
                  return m ? parseInt(m[m.length-1]) : 0;
              };
              return getNum(a.port_number) - getNum(b.port_number);
          });

          ports.forEach(port => {
            const option = document.createElement("option");
            option.value = port.id;
            const statusMark = port.status === 'up' ? '🟢' : '🔴';
            option.textContent = `${statusMark} ${port.port_number}${port.port_name ? ` (${port.port_name})` : ""}`;
            portSelect.appendChild(option);
          });
      }
    } catch (error) {
      console.error("加载交换机端口失败:", error);
    }
  }
}

const managers = {};

function getManager(type) {
  if (!managers[type]) {
    managers[type] = new IpConfigManager(type);
  }
  return managers[type];
}

export async function addIpAddressField(resourceType) {
  const manager = getManager(resourceType);
  await manager.addIpRow();
}

export function bindButton(buttonId, resourceType) {
  const button = document.getElementById(buttonId);
  if (!button) return;

  const newButton = button.cloneNode(true);
  button.replaceWith(newButton);
  newButton.addEventListener("click", () => addIpAddressField(resourceType));
}

export const handleWorkstationRoomChange = async () => {
  const manager = getManager('workstation');
  manager.clear();
  await manager.addIpRow();
};

export const handleCabinetPositionCabinetChange = async () => {
  const manager = getManager('cabinet-position');
  manager.clear();
  await manager.addIpRow();
};

export { getManager };
