import { apiGet } from "./apiClient.js";
import { showToast } from "./toast.js";
import { escapeHtml } from "./helpers.js";
import { t } from "./i18n.js";

const NETWORK_REGION_CHANGED_EVENT = 'ipma:network-region-changed';
const NETWORK_CHANGED_EVENT = 'ipma:network-changed';

const networkChangeCallbacks = new Set();

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

function dispatchNetworkChange(networkId, networkName, networkRegionId) {
  for (const callback of networkChangeCallbacks) {
    try {
      callback(networkId, networkName, networkRegionId);
    } catch (e) {
      console.error("Network change callback error:", e);
    }
  }
}

const CONFIG = {
  device: {
    containerId: 'device-ips-container',
    classPrefix: 'device',
    networksApi: (regionId) => regionId ? `/api/resources/networks?region_id=${regionId}&page_size=1000` : null,
    excludeSwitchId: null,
    parentSwitchRequired: false,
    switchLabel: t('device.upstream_device'),
    portLabel: t('device.upstream_port'),
    loadNetworksByRegion: true
  }
};

let devicesCache = null;

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
          throw new Error(t('ipconfig.load_region_failed'));
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
          const networkResults = await Promise.allSettled(
            uniqueNetworkIds.map(id => apiGet(`/api/resources/networks/${id}`))
          );
          for (const result of networkResults) {
            if (result.status === 'fulfilled' && result.value?.success && result.value.data) {
              const network = result.value.data;
              if (network.network_region_id) {
                regionIds.add(network.network_region_id);
              }
              allNetworks.push(network);
            } else if (result.status === 'rejected') {
              console.error('网络数据获取失败:', result.reason);
            }
          }
        }

        if (regionIds.size > 0) {
          const networkUrls = Array.from(regionIds)
            .map(regionId => this.config.networksApi(regionId))
            .filter(Boolean);

          const networkResults = await Promise.allSettled(
            networkUrls.map(url => apiGet(url))
          );
          for (const result of networkResults) {
            if (result.status === 'fulfilled' && result.value?.success && result.value.data) {
              const regionNetworks = result.value.data.items || result.value.data || [];
              allNetworks = allNetworks.concat(regionNetworks);
            } else if (result.status === 'rejected') {
              console.error('网络数据获取失败:', result.reason);
            }
          }
        }
      } else if (this.config.idSelector) {
        let id = document.getElementById(this.config.idSelector)?.value;
        
        if (!id && ips && ips.length > 0 && ips[0].network_id) {
          const networkIds = [...new Set(ips.filter(ip => ip.network_id).map(ip => ip.network_id))];
          if (networkIds.length > 0) {
            const networkResults = await Promise.allSettled(
              networkIds.map(nid => apiGet(`/api/resources/networks/${nid}`))
            );
            for (const result of networkResults) {
              if (result.status === 'fulfilled' && result.value?.success && result.value.data) {
                allNetworks.push(result.value.data);
              } else if (result.status === 'rejected') {
                console.error('网络数据获取失败:', result.reason);
              }
            }
          }
        } else if (!id) {
          throw new Error(t('ipconfig.select_first', { name: this.config.idName }));
        }
        
        if (id && allNetworks.length === 0) {
          const url = this.config.networksApi(id);
          if (url) {
            const result = await apiGet(url);
            if (!result.success) {
              throw new Error(t('ipconfig.load_network_failed'));
            }
            allNetworks = result.data?.items || result.data || [];
          }
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
      showToast(error.message || t('ipconfig.load_ip_failed'), "error");
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
      const switchSelect = row.querySelector(`.${this.config.classPrefix}-ip-switch-select`);
      const portSelect = row.querySelector(`.${this.config.classPrefix}-ip-port-select`);
      
      if (networkSelect && ipAddressInput) {
        const networkId = networkSelect.value;
        const ipAddress = ipAddressInput.value ? ipAddressInput.value.trim() : '';
        
        if (networkId && ipAddress) {
          const ipData = {
            network_id: networkId,
            ip_address: ipAddress,
            mac_address: macAddressInput && macAddressInput.value ? macAddressInput.value.trim() : null,
          };

          if (networkRegionSelect && networkRegionSelect.value) {
            ipData.network_region_id = networkRegionSelect.value;
          }

          if (switchSelect && switchSelect.value) {
            ipData.switch_id = switchSelect.value;
          }

          if (portSelect && portSelect.value) {
            ipData.device_interface_id = portSelect.value;
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
      const networkId = networkSelect?.value || '';
      const ipAddress = ipAddressInput?.value?.trim() || '';
      const macAddress = macAddressInput?.value?.trim() || null;
      const networkRegionId = networkRegionSelect?.value || '';

      const hasData = networkId || ipAddress || macAddress;
      
      if (!hasData) return;

      if (!networkId) {
        errors.push(t('ipconfig.row_select_network', { row: rowNum }));
      }
      if (!ipAddress) {
        errors.push(t('ipconfig.row_ip_required', { row: rowNum }));
      } else if (!isValidIP(ipAddress)) {
        errors.push(t('ipconfig.row_ip_invalid', { row: rowNum }));
      }
      
      if (networkId && ipAddress) {
        const result = this.getCidrForIp(networkId, ipAddress);
        if (!result.hasCidr) {
          const ipType = result.isV6 ? 'IPv6' : 'IPv4';
          errors.push(t('ipconfig.row_network_not_supported', { row: rowNum, type: ipType }));
        } else if (result.cidr && !isIpInCidr(ipAddress, result.cidr)) {
          errors.push(t('ipconfig.row_ip_not_in_cidr', { row: rowNum, ip: ipAddress, cidr: result.cidr }));
        }
      }

      if (this.config.parentSwitchRequired) {
        if (!networkId) {
          errors.push(t('ipconfig.row_select_network', { row: rowNum }));
        }
      }

      if (networkId && ipAddress) {
        const ipData = {
          network_id: networkId,
          ip_address: ipAddress,
          mac_address: macAddress || null,
        };

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
        showToast(t('ipconfig.select_first', { name: this.config.idName }), "warning");
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
      showToast(t('ipconfig.load_options_failed'), "error");
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
    
    const regionOptions = regions.map(r => `<option value="${escapeHtml(r.id)}">${escapeHtml(r.name)}</option>`).join('');
    const networkOptions = networks.map(n => {
      const cidrs = [];
      if (n.ipv4_cidr) cidrs.push(n.ipv4_cidr);
      if (n.ipv6_cidr) cidrs.push(n.ipv6_cidr);
      const cidrStr = cidrs.length > 0 ? cidrs.join(' / ') : t('ipconfig.no_cidr');
      return `<option value="${escapeHtml(n.id)}">${escapeHtml(n.name)} (${escapeHtml(cidrStr)})</option>`;
    }).join('');

    const switchLabel = this.config.switchLabel || t('device.switch');
    const portLabel = this.config.portLabel || t('device.ports');

    div.innerHTML = `
      <div class="form-row">
        <div class="form-group">
          <label>${t('network.region')}</label>
          <select class="${this.config.classPrefix}-ip-network-region-select form-control">
            <option value="">${t('network.select_region')}</option>
            ${regionOptions}
          </select>
        </div>
        <div class="form-group">
          <label>${t('ipconfig.network')} <span class="required">*</span></label>
          <select class="${this.config.classPrefix}-ip-network-select form-control">
            <option value="">${t('network.select_network')}</option>
            ${networkOptions}
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label>${t('ip.ip_address')} <span class="required">*</span></label>
          <input type="text" class="${this.config.classPrefix}-ip-address-input form-control" placeholder="${t('ipconfig.ip_example')}">
        </div>
        <div class="form-group">
          <label>${t('ip.mac_address')}</label>
          <input type="text" class="${this.config.classPrefix}-ip-mac-address-input form-control" placeholder="${t('ipconfig.mac_example')}">
        </div>
      </div>
      <div class="form-row">
        <div class="form-group">
          <label>${switchLabel}</label>
          <select class="${this.config.classPrefix}-ip-switch-select form-control">
            <option value="">${t('device.select_upstream_device')}</option>
          </select>
        </div>
        <div class="form-group">
          <label>${portLabel}</label>
          <select class="${this.config.classPrefix}-ip-port-select form-control">
            <option value="">${t('device.select_upstream_port')}</option>
          </select>
        </div>
      </div>
      <div class="form-row">
        <div class="form-group" style="display: flex; align-items: flex-end; gap: 8px;">
          <button type="button" class="btn btn-danger btn-sm remove-ip-btn">${t('common.delete')}</button>
          <button type="button" class="btn btn-secondary btn-sm add-ip-btn" data-i18n="ip.add_ip">${t('ip.add_ip')}</button>
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
        showToast(t('device.at_least_one_ip'), "warning");
      }
    });
    
    row.querySelector(".add-ip-btn")?.addEventListener("click", () => this.addIpRow());

    const regionSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-region-select`);
    const networkSelect = row.querySelector(`.${this.config.classPrefix}-ip-network-select`);
    const ipInput = row.querySelector(`.${this.config.classPrefix}-ip-address-input`);
    const macInput = row.querySelector(`.${this.config.classPrefix}-ip-mac-address-input`);
    const switchSelect = row.querySelector(`.${this.config.classPrefix}-ip-switch-select`);
    const portSelect = row.querySelector(`.${this.config.classPrefix}-ip-port-select`);

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
        
        networkSelect.innerHTML = '<option value="">' + t('network.select_network') + '</option>' +
          filtered.map(n => {
            const cidrs = [];
            if (n.ipv4_cidr) cidrs.push(n.ipv4_cidr);
            if (n.ipv6_cidr) cidrs.push(n.ipv6_cidr);
            const cidrStr = cidrs.length > 0 ? cidrs.join(' / ') : t('ipconfig.no_cidr');
            return `<option value="${escapeHtml(n.id)}">${escapeHtml(n.name)} (${escapeHtml(cidrStr)})</option>`;
          }).join('');
        
        if (filtered.length === 1) {
          networkSelect.value = filtered[0].id;
        }
        
        if (this.resourceType === 'device') {
          dispatchNetworkRegionChange(regionId, regionName);
        }
      });
    }

    if (networkSelect) {
      networkSelect.addEventListener("change", () => {
        const networkId = networkSelect.value;
        const network = this.networks.find(n => n.id === networkId);
        if (network) {
          if (this.resourceType === 'device') {
            dispatchNetworkChange(networkId, network.name, network.network_region_id);
          }
        }
      });
    }

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
              networkSelect.innerHTML = '<option value="">' + t('network.select_network') + '</option>' +
                regionNetworks.map(n => {
                  const cidrs = [];
                  if (n.ipv4_cidr) cidrs.push(n.ipv4_cidr);
                  if (n.ipv6_cidr) cidrs.push(n.ipv6_cidr);
                  const cidrStr = cidrs.length > 0 ? cidrs.join(' / ') : t('ipconfig.no_cidr');
                  return `<option value="${escapeHtml(n.id)}">${escapeHtml(n.name)} (${escapeHtml(cidrStr)})</option>`;
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
      
      if (switchSelect && portSelect) {
        await this.loadSwitches(switchSelect, portSelect);
        
        if (initialData.switch_id && switchSelect) {
          switchSelect.value = initialData.switch_id;
          await this.handleSwitchChange(switchSelect, portSelect);
          if (initialData.device_interface_id && portSelect) {
            portSelect.value = initialData.device_interface_id;
          }
        }
      }
    } else {
      if (regionSelect) {
        regionSelect.dispatchEvent(new Event('change'));
      }
      if (switchSelect && portSelect) {
        await this.loadSwitches(switchSelect, portSelect);
      }
    }
  }

  async loadSwitches(switchSelect, portSelect) {
    if (!switchSelect || !portSelect) return;

    try {
      const result = await apiGet("/api/resources/devices?page_size=1000");
      let devicesData = [];
      if (result.success && result.data) {
        if (Array.isArray(result.data)) {
          devicesData = result.data;
        } else if (result.data.items && Array.isArray(result.data.items)) {
          devicesData = result.data.items;
        }
      }
      devicesCache = devicesData;

      switchSelect.innerHTML = '<option value="">' + t('device.select_upstream_device') + '</option>';

      let filteredDevices = devicesCache;
      if (this.config.excludeSwitchId) {
        filteredDevices = devicesCache.filter(dev => dev.id !== this.config.excludeSwitchId);
      }

      filteredDevices.forEach(dev => {
        const option = document.createElement("option");
        option.value = dev.id;
        option.textContent = dev.name;
        switchSelect.appendChild(option);
      });

      switchSelect.addEventListener("change", () => this.handleSwitchChange(switchSelect, portSelect));
    } catch (error) {
      console.error("加载设备列表失败:", error);
    }
  }

  filterSwitchesByRegion(switchSelect, portSelect, regionId) {
    if (!switchSelect || !devicesCache) return;

    if (portSelect) {
      portSelect.innerHTML = '<option value="">' + t('device.select_upstream_port') + '</option>';
    }

    const currentSwitchId = switchSelect.value;

    switchSelect.innerHTML = '<option value="">' + t('device.select_upstream_device') + '</option>';

    let filteredDevices = devicesCache;

    if (regionId) {
      filteredDevices = devicesCache.filter(dev => dev.network_region_id === regionId);
    }

    if (this.config.excludeSwitchId) {
      filteredDevices = filteredDevices.filter(dev => dev.id !== this.config.excludeSwitchId);
    }

    filteredDevices.forEach(dev => {
      const option = document.createElement("option");
      option.value = dev.id;
      option.textContent = dev.name;
      switchSelect.appendChild(option);
    });

    if (currentSwitchId && filteredDevices.some(dev => dev.id === currentSwitchId)) {
      switchSelect.value = currentSwitchId;
    }
  }

  async handleSwitchChange(switchSelect, portSelect) {
    const deviceId = switchSelect.value;
    portSelect.innerHTML = '<option value="">' + t('device.select_upstream_port') + '</option>';

    if (!deviceId) return;

    try {
      const result = await apiGet(`/api/resources/devices/${deviceId}/interfaces`);
      if (result.success && result.data) {
          let interfaces;
          if (Array.isArray(result.data)) {
            interfaces = result.data;
          } else if (result.data.items && Array.isArray(result.data.items)) {
            interfaces = result.data.items;
          } else {
            interfaces = [];
          }

          interfaces.forEach(iface => {
            const option = document.createElement("option");
            option.value = iface.id;
            const typeMark = iface.interface_role ? `[${t(`device.interface_role_${iface.interface_role}`)}]` : '';
            option.textContent = `${iface.name}${typeMark}`;
            portSelect.appendChild(option);
          });
      }
    } catch (error) {
      console.error("加载设备接口失败:", error);
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

export { getManager };
