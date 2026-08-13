// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  debounce,
  handleError,
  appendPaginationToTable,
  escapeHtml,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";

import {
  loadNetworkTypeOptions
} from "../utils/resources.js";

import { elementCache } from "../utils/helpers.js";

let currentNetworkTypePage = 1;
const NETWORK_TYPE_PAGE_SIZE = 20;

// 加载网络区域数据并填充表格
export async function loadNetworkTypesData(page = 1) {
  currentNetworkTypePage = page;
  try {
    const result = await apiGet(`/api/resources/network-regions?page=${page}&page_size=${NETWORK_TYPE_PAGE_SIZE}`);
    const data = result.success ? result.data : { items: [], total: 0 };
    const items = data.items || data;

    renderTable("#network-types-table", {
      data: items,
      columns: [
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'description', render: (v) => escapeHtml(v) || '-' },
        { field: 'ipv4_cidrs', render: (v) => {
          if (!v || v.length === 0) return '-';
          return v.map(cidr => escapeHtml(cidr)).join('<br>');
        }},
        { field: 'ipv6_cidrs', render: (v) => {
          if (!v || v.length === 0) return '-';
          return v.map(cidr => escapeHtml(cidr)).join('<br>');
        }},
        { field: 'created_at', render: (v) => formatDateTime(v) },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">删除</button>
        ` }
      ],
      emptyMessage: '暂无网络区域数据'
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#network-types-table", data, loadNetworkTypesData);
    }
  } catch (error) {
    console.error("加载网络区域数据失败:", error);
    renderTable("#network-types-table", {
      data: [],
      columns: [],
      emptyMessage: "加载失败，请刷新页面重试"
    });
  }
}

let currentNetworkPage = 1;
const NETWORK_PAGE_SIZE = 20;

let currentFilters = {
  name: '',
  region: '',
  ipv4: '',
  ipv6: ''
};

export async function loadNetworksData(page = 1, filters = currentFilters) {
  currentNetworkPage = page;
  currentFilters = filters;
  
  try {
    const params = new URLSearchParams({
      page: page.toString(),
      page_size: NETWORK_PAGE_SIZE.toString()
    });
    
    if (filters.name) params.append('name', filters.name);
    if (filters.region) params.append('network_region', filters.region);
    if (filters.ipv4) params.append('ipv4_cidr', filters.ipv4);
    if (filters.ipv6) params.append('ipv6_cidr', filters.ipv6);
    
    const url = `/api/resources/networks?${params.toString()}`;
    const result = await apiGet(url);
    const data = result.success ? result.data : { items: [], total: 0 };
    const networks = data.items || data;
    const startIndex = (page - 1) * NETWORK_PAGE_SIZE;

    renderTable("#networks-table", {
      data: networks,
      columns: [
        { field: 'id', render: (v, row, index) => startIndex + index + 1, className: 'index-column' },
        { field: 'name', render: (v) => escapeHtml(v) },
        { field: 'network_region', render: (v) => escapeHtml(v) },
        { field: 'ipv4_cidr', render: (v) => escapeHtml(v) || '-' },
        { field: 'ipv6_cidr', render: (v) => escapeHtml(v) || '-' },
        { field: 'created_at', render: (v) => new Date(v).toLocaleString() },
        { field: 'id', render: (v) => `
          <button class="btn btn-sm btn-secondary btn-usage" data-id="${v}">使用情况</button>
          <button class="btn btn-sm btn-edit" data-id="${v}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">删除</button>
        ` }
      ],
      emptyMessage: '没有找到匹配的网段数据'
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#networks-table", data, (p) => loadNetworksData(p, filters));
    }
  } catch (error) {
    handleError(error, "加载网络数据失败", () => {
      renderTable("#networks-table", { data: [], columns: [], emptyMessage: "加载失败，请刷新页面重试" });
    });
  }
}

export function initNetworksFilters() {
  const filterIds = [
    'network-name-filter',
    'network-region-filter', 
    'network-ipv4-filter',
    'network-ipv6-filter'
  ];
  
  const debouncedFilter = debounce(applyNetworkFilters, 300);
  
  filterIds.forEach(filterId => {
    const filterElement = document.getElementById(filterId);
    if (filterElement) {
      filterElement.addEventListener('input', debouncedFilter);
    }
  });
}

function applyNetworkFilters() {
  const filters = {
    name: getElementValue('network-name-filter') || '',
    region: getElementValue('network-region-filter') || '',
    ipv4: getElementValue('network-ipv4-filter') || '',
    ipv6: getElementValue('network-ipv6-filter') || ''
  };
  
  loadNetworksData(1, filters);
}

// 计算网段的总IP数量
function calculateTotalIps(cidr) {
  if (!cidr) return 0;
  
  try {
    // 解析CIDR，提取子网掩码长度
    const parts = cidr.split('/');
    if (parts.length !== 2) return 0;
    
    const prefixLength = parseInt(parts[1]);
    // 计算总IP数量：2^(32 - 子网掩码长度) - 2（减去网络地址和广播地址）
    return Math.pow(2, 32 - prefixLength) - 2;
  } catch (error) {
    console.error("计算总IP数量失败:", error);
    return 0;
  }
}

// 生成网段的所有IP地址
function generateIpAddresses(cidr) {
  if (!cidr) return [];
  
  try {
    // 解析CIDR
    const parts = cidr.split('/');
    if (parts.length !== 2) return [];
    
    const ip = parts[0];
    const prefixLength = parseInt(parts[1]);
    
    // 简单实现：只处理IPv4地址
    if (!/^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/.test(ip)) return [];
    
    // 计算网络地址
    const ipParts = ip.split('.').map(Number);
    const networkAddress = ipParts.join('.');
    
    // 计算总IP数量
    const totalIps = calculateTotalIps(cidr);
    
    // 生成IP地址列表（简化实现，只生成部分IP用于演示）
    const ipAddresses = [];
    const maxDisplayIps = Math.min(totalIps, 256); // 最多显示256个IP
    
    for (let i = 1; i <= maxDisplayIps; i++) {
      // 简单实现：递增最后一位IP地址
      const newIpParts = [...ipParts];
      let carry = i;
      
      for (let j = 3; j >= 0 && carry > 0; j--) {
        const sum = newIpParts[j] + carry;
        newIpParts[j] = sum % 256;
        carry = Math.floor(sum / 256);
      }
      
      ipAddresses.push(newIpParts.join('.'));
    }
    
    return ipAddresses;
  } catch (error) {
    console.error("生成IP地址失败:", error);
    return [];
  }
}

// 生成 /22 或 /23 网段划分为 /24 的子网列表
function generateSubnet24List(cidr) {
  if (!cidr) return [];
  const parts = cidr.split('/');
  if (parts.length !== 2) return [];
  const prefixLength = parseInt(parts[1], 10);
  if (prefixLength !== 22 && prefixLength !== 23) return [];

  const ip = parts[0];
  const ipParts = ip.split('.').map(Number);
  if (ipParts.length !== 4 || ipParts.some(p => isNaN(p))) return [];

  // /22 掩码第三段 252，/23 掩码第三段 254
  const maskThird = prefixLength === 22 ? 252 : 254;
  const networkThird = ipParts[2] & maskThird;
  const subnetCount = prefixLength === 22 ? 4 : 2;

  const subnets = [];
  for (let i = 0; i < subnetCount; i++) {
    subnets.push(`${ipParts[0]}.${ipParts[1]}.${networkThird + i}.0/24`);
  }
  return subnets;
}

// 渲染 IP 可视化块
function renderIpBlocks(ipAddresses, ipStatusMap) {
  return ipAddresses.map(ip => {
    const isUsed = ipStatusMap.has(ip);
    const status = ipStatusMap.get(ip) || "unused";
    const statusClass = isUsed ? (status === "active" ? "ip-used-active" : "ip-used-inactive") : "ip-unused";
    const tooltipText = `${escapeHtml(ip)} (${isUsed ? status === "active" ? t('status.active') : t('status.inactive') : t('network.unused')})`;
    return `
      <div class="ip-block ${statusClass}" data-ip="${escapeHtml(ip)}" data-status="${isUsed ? status : "unused"}" title="${tooltipText}">
        <span class="ip-label">${ip.split('.').pop()}</span>
      </div>
    `;
  }).join('');
}

// 显示网段使用情况
export async function showNetworkUsage(id) {
  try {
    const networkResult = await apiGet(`/api/resources/networks/${id}`);
    if (!networkResult.success) {
      showToast(t('network.load_failed'), "error");
      return;
    }
    
    const network = networkResult.data;
    const hasIPv4 = !!network.ipv4_cidr;
    const hasIPv6 = !!network.ipv6_cidr;
    
    const ipResult = await apiGet(`/api/resources/ip?network_id=${id}&page_size=1000`);
    if (!ipResult.success) {
      showToast(t('ip.load_failed'), "error");
      return;
    }
    
    const ipData = ipResult.data?.data || ipResult.data?.items || ipResult.data || [];
    const allNetworkIps = Array.isArray(ipData) ? ipData : [];
    const isIPv6 = (ip) => ip.ip_address && ip.ip_address.includes(':');
    const ipv4Ips = allNetworkIps.filter(ip => !isIPv6(ip));
    const ipv6Ips = allNetworkIps.filter(ip => isIPv6(ip));
    
    let tabsHtml = '';
    let contentHtml = '';
    
    if (hasIPv4 && hasIPv6) {
      tabsHtml = `
        <div class="usage-tabs">
          <button class="usage-tab-btn active" data-tab="ipv4">IPv4</button>
          <button class="usage-tab-btn" data-tab="ipv6">IPv6</button>
        </div>
      `;
      contentHtml = `
        <div class="usage-tab-content active" id="ipv4-content">
          ${buildIPv4Content(network, ipv4Ips, id)}
        </div>
        <div class="usage-tab-content" id="ipv6-content">
          ${buildIPv6Content(network, ipv6Ips, id)}
        </div>
      `;
    } else if (hasIPv4) {
      contentHtml = `
        <div class="usage-tab-content active">
          ${buildIPv4Content(network, ipv4Ips, id)}
        </div>
      `;
    } else if (hasIPv6) {
      contentHtml = `
        <div class="usage-tab-content active">
          ${buildIPv6Content(network, ipv6Ips, id)}
        </div>
      `;
    } else {
      contentHtml = `
        <div class="usage-tab-content active">
          <div class="no-network-info">
            <p>${t('network.no_ip_config')}</p>
          </div>
        </div>
      `;
    }
    
    const usageHtml = `
      <div class="network-usage-container">
        <h4>${escapeHtml(network.name)} - ${t('network.usage')}</h4>
        ${tabsHtml}
        ${contentHtml}
      </div>
    `;
    
    const modalContainer = document.createElement('div');
    modalContainer.className = 'modal active';
    modalContainer.id = 'network-usage-modal';
    modalContainer.setAttribute('role', 'dialog');
    modalContainer.setAttribute('aria-modal', 'true');
    modalContainer.innerHTML = `
      <div class="modal-content modal-lg">
        <header class="modal-header">
          <h3 class="modal-title">${t('network.usage')}</h3>
          <button type="button" class="close" data-modal-id="network-usage-modal" aria-label="Close">&times;</button>
        </header>
        <div class="modal-body">
          ${usageHtml}
        </div>
        <footer class="modal-footer">
          <button type="button" class="btn btn-secondary" data-modal-id="network-usage-modal">${t('common.close')}</button>
        </footer>
      </div>
    `;
    
    document.body.appendChild(modalContainer);
    document.body.style.overflow = 'hidden';
    
    const cleanup = () => {
      modalContainer.remove();
      document.body.style.overflow = '';
    };
    
    modalContainer.addEventListener('click', (e) => {
      if (e.target === modalContainer || e.target.hasAttribute('data-modal-id')) {
        cleanup();
      }
    });
    
    const tabButtons = modalContainer.querySelectorAll('.usage-tab-btn');
    tabButtons.forEach(btn => {
      btn.addEventListener('click', () => {
        tabButtons.forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        
        const tabId = btn.dataset.tab;
        const contents = modalContainer.querySelectorAll('.usage-tab-content');
        contents.forEach(content => {
          content.classList.remove('active');
          if (content.id === `${tabId}-content`) {
            content.classList.add('active');
          }
        });
      });
    });
    
    bindIPv4Events(modalContainer, network, ipv4Ips, id);
    bindIPv6Events(modalContainer, network, ipv6Ips, id);
    
  } catch (error) {
    console.error("获取网段使用情况失败:", error);
    showToast(t('common.load_failed_retry'), "error");
  }
}

function buildIPv4Content(network, networkIps, networkId) {
  const cidr = network.ipv4_cidr;
  const totalIps = calculateTotalIps(cidr);
  const prefixLength = cidr ? parseInt(cidr.split('/')[1], 10) : 32;

  // 掩码 < 22：IP 数量过多，不生成
  // 掩码 22/23：划分为 /24 子网，默认显示第一个子网
  // 掩码 >= 24：直接生成
  const subnet24List = (prefixLength === 22 || prefixLength === 23) ? generateSubnet24List(cidr) : [];
  const activeSubnet24 = subnet24List[0] || null;
  const displayCidr = activeSubnet24 || cidr;
  const allIpAddresses = prefixLength < 22 ? [] : generateIpAddresses(displayCidr);
  
  const ipStatusMap = new Map();
  networkIps.forEach(ip => {
    ipStatusMap.set(ip.ip_address, ip.status || "inactive");
  });
  
  const usedIps = networkIps.length;
  const unusedIps = totalIps - usedIps;
  const usageRate = totalIps > 0 ? ((usedIps / totalIps) * 100).toFixed(2) : "0.00";
  
  return `
    <div class="usage-stats">
      <div class="stat-item">
        <span class="stat-label">${t('network.ipv4_cidr')}:</span>
        <span class="stat-value">${escapeHtml(cidr) || "-"}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">${t('network.region')}:</span>
        <span class="stat-value">${escapeHtml(network.network_region) || "-"}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">${t('network.ipv4_gateway')}:</span>
        <span class="stat-value">${escapeHtml(network.ipv4_gateway) || "-"}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">${t('network.ipv4_dns')}:</span>
        <span class="stat-value">${Array.isArray(network.ipv4_dns) ? network.ipv4_dns.map(d => escapeHtml(d)).join(', ') : (escapeHtml(network.ipv4_dns) || "-")}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">${t('network.total_ips')}:</span>
        <span class="stat-value">${totalIps}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">${t('network.used_ips')}:</span>
        <span class="stat-value">${usedIps}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">${t('network.unused_ips')}:</span>
        <span class="stat-value">${unusedIps}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">${t('network.usage_rate')}:</span>
        <span class="stat-value">${usageRate}%</span>
      </div>
    </div>
    
    <div class="usage-controls">
      <div class="filter-controls">
        <label>${t('common.filter')}: </label>
        <select id="ip-status-filter" class="form-control form-control-sm d-inline-block w-auto mr-2">
          <option value="all">${t('common.all')}</option>
          <option value="used">${t('network.used')}</option>
          <option value="unused">${t('network.unused')}</option>
        </select>
        <button id="refresh-ipv4-usage" class="btn btn-sm btn-secondary">${t('common.refresh')}</button>
      </div>
    </div>
    
    <div class="usage-visualization">
      <h5>${t('network.ip_visualization')}</h5>
      ${
        prefixLength < 22
          ? `<div class="ip-grid-empty" role="alert">${t('network.visualization_too_many_ips')}</div>`
          : subnet24List.length > 0
            ? `<div class="subnet-24-tabs" role="tablist">
                ${subnet24List.map((sub, idx) => `
                  <button type="button" class="subnet-24-btn${idx === 0 ? ' active' : ''}" data-cidr="${escapeHtml(sub)}" role="tab">${escapeHtml(sub)}</button>
                `).join('')}
              </div>
              <div class="ip-grid" id="ip-grid" data-active-cidr="${escapeHtml(activeSubnet24 || '')}">
                ${renderIpBlocks(allIpAddresses, ipStatusMap)}
              </div>`
            : `<div class="ip-grid" id="ip-grid">
                ${renderIpBlocks(allIpAddresses, ipStatusMap)}
              </div>`
      }
    </div>
    
    <div class="usage-ips">
      <h5>${t('network.ipv4_list')}</h5>
      <div class="table-responsive">
        <table class="table table-sm">
          <thead>
            <tr>
              <th>${t('ip.ip_address')}</th>
              <th>${t('ip.status')}</th>
              <th>${t('ip.location')}</th>
              <th>${t('ip.mac_address')}</th>
              <th>${t('ip.hostname')}</th>
            </tr>
          </thead>
          <tbody id="ipv4-list-body">
            ${networkIps.length > 0 ? networkIps.map(ip => `
              <tr>
                <td>${escapeHtml(ip.ip_address)}</td>
                <td>
                  <span class="status-badge ${ip.status === "active" ? "status-active" : "status-inactive"}">
                    ${escapeHtml(ip.status)}
                  </span>
                </td>
                <td>${escapeHtml(ip.workstation_name || ip.cabinet_position_name) || "-"}</td>
                <td>${escapeHtml(ip.mac_address) || "-"}</td>
                <td>${escapeHtml(ip.hostname) || "-"}</td>
              </tr>
            `).join('') : `<tr><td colspan="5" class="text-center">${t('network.no_ipv4_records')}</td></tr>`}
          </tbody>
        </table>
      </div>
    </div>
  `;
}

function buildIPv6Content(network, networkIps, networkId) {
  const hasIPv6Config = !!network.ipv6_cidr;
  
  if (!hasIPv6Config) {
    return `
      <div class="no-ipv6-info">
        <div class="no-ipv6-icon">📡</div>
        <p>${t('network.no_ipv6_config')}</p>
        <p class="no-ipv6-hint">${t('network.add_ipv6_hint')}</p>
      </div>
    `;
  }
  
  const activeIps = networkIps.filter(ip => ip.status === "active").length;
  const inactiveIps = networkIps.filter(ip => ip.status !== "active").length;
  const totalAssigned = networkIps.length;
  
  return `
    <div class="ipv6-info-section">
      <h5>${t('network.ipv6_info')}</h5>
      <div class="ipv6-info-grid">
        <div class="ipv6-info-item">
          <span class="info-label">${t('network.ipv6_cidr')}</span>
          <span class="info-value ipv6-address">${escapeHtml(network.ipv6_cidr)}</span>
        </div>
        <div class="ipv6-info-item">
          <span class="info-label">${t('network.ipv6_gateway')}</span>
          <span class="info-value ipv6-address">${escapeHtml(network.ipv6_gateway) || "-"}</span>
        </div>
        <div class="ipv6-info-item">
          <span class="info-label">DNS</span>
          <span class="info-value ipv6-address">${Array.isArray(network.ipv6_dns) ? network.ipv6_dns.map(d => escapeHtml(d)).join(', ') : (escapeHtml(network.ipv6_dns) || "-")}</span>
        </div>
        <div class="ipv6-info-item">
          <span class="info-label">${t('network.connection_status')}</span>
          <span class="info-value">
            <span class="connection-status ${totalAssigned > 0 ? 'status-enabled' : 'status-disabled'}">
              ${totalAssigned > 0 ? t('network.enabled') : t('network.unused')}
            </span>
          </span>
        </div>
      </div>
    </div>
    
    <div class="ipv6-stats-section">
      <h5>${t('network.ipv6_stats')}</h5>
      <div class="ipv6-stats-grid">
        <div class="ipv6-stat-card">
          <div class="stat-number">${totalAssigned}</div>
          <div class="stat-desc">${t('network.assigned')}</div>
        </div>
        <div class="ipv6-stat-card active">
          <div class="stat-number">${activeIps}</div>
          <div class="stat-desc">${t('status.active')}</div>
        </div>
        <div class="ipv6-stat-card inactive">
          <div class="stat-number">${inactiveIps}</div>
          <div class="stat-desc">${t('status.inactive')}</div>
        </div>
      </div>
    </div>
    
    <div class="ipv6-list-section">
      <div class="ipv6-list-header">
        <h5>${t('network.ipv6_list')}</h5>
        <button id="refresh-ipv6-usage" class="btn btn-sm btn-secondary">${t('common.refresh')}</button>
      </div>
      <div class="table-responsive">
        <table class="table table-sm">
          <thead>
            <tr>
              <th>${t('ip.ip_address')}</th>
              <th>${t('ip.status')}</th>
              <th>${t('ip.location')}</th>
              <th>${t('ip.mac_address')}</th>
              <th>${t('ip.hostname')}</th>
            </tr>
          </thead>
          <tbody id="ipv6-list-body">
            ${networkIps.length > 0 ? networkIps.map(ip => `
              <tr>
                <td class="ipv6-address-cell">${escapeHtml(ip.ip_address)}</td>
                <td>
                  <span class="status-badge ${ip.status === "active" ? "status-active" : "status-inactive"}">
                    ${escapeHtml(ip.status)}
                  </span>
                </td>
                <td>${escapeHtml(ip.workstation_name || ip.cabinet_position_name || ip.port_device_name) || "-"}</td>
                <td>${escapeHtml(ip.mac_address) || "-"}</td>
                <td>${escapeHtml(ip.hostname) || "-"}</td>
              </tr>
            `).join('') : `<tr><td colspan="5" class="text-center">${t('network.no_ipv6_records')}</td></tr>`}
          </tbody>
        </table>
      </div>
    </div>
  `;
}

function bindIPv4Events(modalContainer, network, networkIps, networkId) {
  const filterSelect = modalContainer.querySelector('#ip-status-filter');
  const ipGrid = modalContainer.querySelector('#ip-grid');
  const ipListBody = modalContainer.querySelector('#ipv4-list-body');

  // 构建 IP 状态映射，供子网切换时使用（刷新后会被替换为最新数据）
  let ipStatusMap = new Map();
  networkIps.forEach(ip => {
    ipStatusMap.set(ip.ip_address, ip.status || "inactive");
  });

  // /24 子网切换
  const subnet24Buttons = modalContainer.querySelectorAll('.subnet-24-btn');
  if (subnet24Buttons.length > 0 && ipGrid) {
    subnet24Buttons.forEach(btn => {
      btn.addEventListener('click', () => {
        const subCidr = btn.dataset.cidr;
        if (!subCidr) return;
        const subIps = generateIpAddresses(subCidr);
        subnet24Buttons.forEach(b => b.classList.remove('active'));
        btn.classList.add('active');
        ipGrid.dataset.activeCidr = subCidr;
        ipGrid.innerHTML = renderIpBlocks(subIps, ipStatusMap);
        // 重置过滤器
        if (filterSelect) filterSelect.value = 'all';
      });
    });
  }

  if (filterSelect && ipGrid) {
    filterSelect.addEventListener('change', () => {
      const filterValue = filterSelect.value;
      const ipBlocks = ipGrid.querySelectorAll('.ip-block');
      
      ipBlocks.forEach(block => {
        const status = block.dataset.status;
        const isUsed = status !== "unused";
        
        if (filterValue === "all") {
          block.style.display = "block";
        } else if (filterValue === "used" && isUsed) {
          block.style.display = "block";
        } else if (filterValue === "unused" && !isUsed) {
          block.style.display = "block";
        } else {
          block.style.display = "none";
        }
      });
    });
  }
  
  const refreshButton = modalContainer.querySelector('#refresh-ipv4-usage');
  if (refreshButton) {
    refreshButton.addEventListener('click', async () => {
      refreshButton.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span> 刷新中...';
      refreshButton.disabled = true;
      
      try {
        const refreshIpResult = await apiGet("/api/resources/ip?page_size=1000");
        if (refreshIpResult.success) {
          const allIps = refreshIpResult.data.data || refreshIpResult.data || [];
          const isIPv6 = (ip) => ip.ip_address.includes(':');
          const refreshedNetworkIps = allIps.filter(ip => ip.network_id === networkId && !isIPv6(ip));
          
          const newIpStatusMap = new Map();
          refreshedNetworkIps.forEach(ip => {
            newIpStatusMap.set(ip.ip_address, ip.status || "inactive");
          });
          // 更新闭包内的 ipStatusMap，使后续子网切换使用最新数据
          ipStatusMap = newIpStatusMap;
          
          if (ipGrid) {
            const ipBlocks = ipGrid.querySelectorAll('.ip-block');
            ipBlocks.forEach(block => {
              const ip = block.dataset.ip;
              const isUsed = newIpStatusMap.has(ip);
              const status = newIpStatusMap.get(ip) || "unused";
              const statusClass = isUsed ? (status === "active" ? "ip-used-active" : "ip-used-inactive") : "ip-unused";
              const tooltipText = `${ip} (${isUsed ? status === "active" ? "活跃" : "非活跃" : "未使用"})`;
              
              block.className = `ip-block ${statusClass}`;
              block.dataset.status = isUsed ? status : "unused";
              block.title = tooltipText;
            });
          }
          
          if (ipListBody) {
            ipListBody.innerHTML = refreshedNetworkIps.length > 0 ? refreshedNetworkIps.map(ip => `
              <tr>
                <td>${escapeHtml(ip.ip_address)}</td>
                <td>
                  <span class="status-badge ${ip.status === "active" ? "status-active" : "status-inactive"}">
                    ${escapeHtml(ip.status)}
                  </span>
                </td>
                <td>${escapeHtml(ip.workstation_name || ip.cabinet_position_name || "-")}</td>
                <td>${escapeHtml(ip.mac_address || "-")}</td>
                <td>${escapeHtml(ip.hostname || "-")}</td>
              </tr>
            `).join('') : '<tr><td colspan="5" class="text-center">暂无IPv4地址记录</td></tr>';
          }
          
          showToast("IPv4使用情况已更新", "success");
        }
      } catch (error) {
        console.error("刷新IPv4使用情况失败:", error);
        showToast("刷新失败，请重试", "error");
      } finally {
        refreshButton.innerHTML = '刷新';
        refreshButton.disabled = false;
      }
    });
  }
}

function bindIPv6Events(modalContainer, network, networkIps, networkId) {
  const refreshButton = modalContainer.querySelector('#refresh-ipv6-usage');
  const ipListBody = modalContainer.querySelector('#ipv6-list-body');
  
  if (refreshButton) {
    refreshButton.addEventListener('click', async () => {
      refreshButton.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span> 刷新中...';
      refreshButton.disabled = true;
      
      try {
        const refreshIpResult = await apiGet("/api/resources/ip?page_size=1000");
        if (refreshIpResult.success) {
          const allIps = refreshIpResult.data.data || refreshIpResult.data || [];
          const isIPv6 = (ip) => ip.ip_address.includes(':');
          const refreshedNetworkIps = allIps.filter(ip => ip.network_id === networkId && isIPv6(ip));
          
          if (ipListBody) {
            ipListBody.innerHTML = refreshedNetworkIps.length > 0 ? refreshedNetworkIps.map(ip => `
              <tr>
                <td class="ipv6-address-cell">${escapeHtml(ip.ip_address)}</td>
                <td>
                  <span class="status-badge ${ip.status === "active" ? "status-active" : "status-inactive"}">
                    ${escapeHtml(ip.status)}
                  </span>
                </td>
                <td>${escapeHtml(ip.workstation_name || ip.cabinet_position_name || ip.port_device_name || "-")}</td>
                <td>${escapeHtml(ip.mac_address || "-")}</td>
                <td>${escapeHtml(ip.hostname || "-")}</td>
              </tr>
            `).join('') : '<tr><td colspan="5" class="text-center">暂无IPv6地址记录</td></tr>';
          }
          
          const activeIps = refreshedNetworkIps.filter(ip => ip.status === "active").length;
          const inactiveIps = refreshedNetworkIps.filter(ip => ip.status !== "active").length;
          
          const statCards = modalContainer.querySelectorAll('.ipv6-stat-card');
          if (statCards.length >= 3) {
            statCards[0].querySelector('.stat-number').textContent = refreshedNetworkIps.length;
            statCards[1].querySelector('.stat-number').textContent = activeIps;
            statCards[2].querySelector('.stat-number').textContent = inactiveIps;
          }
          
          showToast("IPv6使用情况已更新", "success");
        }
      } catch (error) {
        console.error("刷新IPv6使用情况失败:", error);
        showToast("刷新失败，请重试", "error");
      } finally {
        refreshButton.innerHTML = '刷新';
        refreshButton.disabled = false;
      }
    });
  }
}

// 编辑网络
export async function editNetwork(id) {
  try {
    // 根据ID获取网络数据
    const result = await apiGet(`/api/resources/networks/${id}`);
    if (result.success) {
      // 打开编辑模态框
      openNetworkModal(result.data);
    } else {
      showToast(`获取网络数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取网络数据失败");
  }
}

// 删除网络
export async function deleteNetwork(id) {
  await handleDelete(id, "/api/resources/networks", "网络删除成功", loadNetworksData);
}
// 编辑网络区域
export async function editNetworkType(id) {
  try {
    const result = await apiGet(`/api/resources/network-regions/${id}`);
    if (result.success) {
      openNetworkTypeModal(result.data);
    } else {
      showToast(`获取网络区域数据失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "获取网络区域数据失败");
  }
}

// 删除网络区域
export async function deleteNetworkType(id) {
  await handleDelete(id, "/api/resources/network-regions", "网络区域删除成功", loadNetworkTypesData);
}

// ====== CIDR 动态输入框管理 ======
function addCidrInputRow(containerId, cidrType, value = '') {
  const container = document.getElementById(containerId);
  if (!container) return;

  const row = document.createElement('div');
  row.className = 'cidr-input-row';

  const input = document.createElement('input');
  input.type = 'text';
  input.className = 'cidr-input';
  input.value = value;
  input.placeholder = cidrType === 'ipv4' ? '例如: 10.0.0.0/8' : '例如: 2001:db8::/32';

  const btnGroup = document.createElement('div');
  btnGroup.className = 'cidr-btn-group';

  const addBtn = document.createElement('button');
  addBtn.type = 'button';
  addBtn.className = 'btn btn-icon-sm btn-success cidr-add-btn';
  addBtn.textContent = '+';
  addBtn.setAttribute('aria-label', t('common.add', 'Add'));
  addBtn.addEventListener('click', () => {
    addCidrInputRow(containerId, cidrType);
  });

  const removeBtn = document.createElement('button');
  removeBtn.type = 'button';
  removeBtn.className = 'btn btn-icon-sm btn-danger cidr-remove-btn';
  removeBtn.textContent = '−';
  removeBtn.setAttribute('aria-label', t('common.delete', 'Delete'));
  removeBtn.addEventListener('click', () => {
    if (container.children.length > 1) {
      row.remove();
    } else {
      input.value = '';
    }
  });

  btnGroup.appendChild(addBtn);
  btnGroup.appendChild(removeBtn);
  row.appendChild(input);
  row.appendChild(btnGroup);
  container.appendChild(row);
}

function getCidrValues(containerId) {
  const container = document.getElementById(containerId);
  if (!container) return null;

  const values = [];
  container.querySelectorAll('.cidr-input').forEach(input => {
    const val = input.value.trim();
    if (val) values.push(val);
  });

  return values.length > 0 ? values : null;
}

function clearCidrInputs(containerId) {
  const container = document.getElementById(containerId);
  if (container) container.innerHTML = '';
}

// ====== 提交网络区域表单 ======
export async function submitNetworkTypeForm() {
  const id = getElementValue("network-type-id");
  const name = getElementValue("network-type-name");
  const description = getElementValue("network-type-description");
  const ipv4_cidrs = getCidrValues('network-type-ipv4-cidrs-list');
  const ipv6_cidrs = getCidrValues('network-type-ipv6-cidrs-list');

  if (!name) {
    showToast("网络区域名称不能为空", "warning");
    return;
  }

  const networkTypeData = {
    name,
    description: description || null,
    ipv4_cidrs,
    ipv6_cidrs,
  };

  const success = await handleFormSubmit({
    formData: networkTypeData,
    id,
    baseUrl: "/api/resources/network-regions",
    successMessage: "网络区域保存成功",
    modalId: "network-type-modal",
    reloadFunction: () => {
      loadNetworkTypesData();
      loadNetworkTypeOptions();
    }
  });

  return success;
}

// ====== 提交网络表单 ======
export async function submitNetworkForm() {
  const id = getElementValue("network-id");
  const name = getElementValue("network-name");
  const networkType = getElementValue("network-type");
  const ipv4_cidr = getElementValue("network-ipv4-cidr");
  const ipv6_cidr = getElementValue("network-ipv6-cidr");
  const ipv4_gateway = getElementValue("network-ipv4-gateway");
  const ipv6_gateway = getElementValue("network-ipv6-gateway");
  const ipv4_dns_str = getElementValue("network-ipv4-dns");
  const ipv6_dns_str = getElementValue("network-ipv6-dns");
  const description = getElementValue("network-description");

  if (!name) {
    showToast("网络名称不能为空", "warning");
    return;
  }

  if (!networkType) {
    showToast("请选择网络区域", "warning");
    return;
  }

  if (!ipv4_cidr && !ipv6_cidr) {
    showToast("至少需要提供一个有效的IPv4或IPv6 CIDR", "warning");
    return;
  }

  const parseDnsList = (dnsStr) => {
    if (!dnsStr || !dnsStr.trim()) return null;
    const dnsList = dnsStr.split(/[,\s]+/).map(dns => dns.trim()).filter(dns => dns.length > 0);
    if (dnsList.length > 5) {
      showToast("DNS服务器数量不能超过5个", "warning");
      return null;
    }
    return dnsList;
  };

  const ipv4_dns = parseDnsList(ipv4_dns_str);
  const ipv6_dns = parseDnsList(ipv6_dns_str);

  if (ipv4_dns === null && ipv4_dns_str && ipv4_dns_str.trim()) return;
  if (ipv6_dns === null && ipv6_dns_str && ipv6_dns_str.trim()) return;

  const networkData = {
    name,
    network_region_id: networkType,
    ipv4_cidr: ipv4_cidr || null,
    ipv6_cidr: ipv6_cidr || null,
    ipv4_gateway: ipv4_gateway || null,
    ipv6_gateway: ipv6_gateway || null,
    ipv4_dns,
    ipv6_dns,
    description: description || null,
  };

  const success = await handleFormSubmit({
    formData: networkData,
    id,
    baseUrl: "/api/resources/networks",
    successMessage: "网络保存成功",
    modalId: "network-modal",
    reloadFunction: loadNetworksData
  });

  return success;
}

// ====== 网络区域管理模态框 ======
export async function openNetworkTypeModal(networkType = null) {
  await openModal("network-type-modal");

  const title = elementCache.get('network-type-modal-title');
  const form = elementCache.get('network-type-form');

  clearCidrInputs('network-type-ipv4-cidrs-list');
  clearCidrInputs('network-type-ipv6-cidrs-list');

  if (networkType) {
    title.textContent = "编辑网络区域";
    elementCache.setValue('network-type-id', networkType.id);
    elementCache.setValue('network-type-name', networkType.name);
    elementCache.setValue('network-type-description', networkType.description || "");

    const ipv4Cidrs = Array.isArray(networkType.ipv4_cidrs) ? networkType.ipv4_cidrs : [];
    const ipv6Cidrs = Array.isArray(networkType.ipv6_cidrs) ? networkType.ipv6_cidrs : [];

    if (ipv4Cidrs.length > 0) {
      ipv4Cidrs.forEach(cidr => addCidrInputRow('network-type-ipv4-cidrs-list', 'ipv4', cidr));
    } else {
      addCidrInputRow('network-type-ipv4-cidrs-list', 'ipv4');
    }

    if (ipv6Cidrs.length > 0) {
      ipv6Cidrs.forEach(cidr => addCidrInputRow('network-type-ipv6-cidrs-list', 'ipv6', cidr));
    } else {
      addCidrInputRow('network-type-ipv6-cidrs-list', 'ipv6');
    }
  } else {
    title.textContent = "添加网络区域";
    if (form) form.reset();
    elementCache.setValue('network-type-id', '');
    addCidrInputRow('network-type-ipv4-cidrs-list', 'ipv4');
    addCidrInputRow('network-type-ipv6-cidrs-list', 'ipv6');
  }
}

// ====== 网络管理模态框 ======
export async function openNetworkModal(network = null) {
  await openModal("network-modal");
  
  const title = elementCache.get('network-modal-title');
  const form = elementCache.get('network-form');

  await loadNetworkTypeOptions();

  if (network) {
    title.textContent = "编辑网络";
    elementCache.setValue('network-id', network.id);
    elementCache.setValue('network-name', network.name);
    elementCache.setValue('network-type', network.network_region_id);
    elementCache.setValue('network-ipv4-cidr', network.ipv4_cidr || "");
    elementCache.setValue('network-ipv6-cidr', network.ipv6_cidr || "");
    elementCache.setValue('network-ipv4-gateway', network.ipv4_gateway || "");
    elementCache.setValue('network-ipv6-gateway', network.ipv6_gateway || "");
    elementCache.setValue('network-ipv4-dns', Array.isArray(network.ipv4_dns) ? network.ipv4_dns.join(', ') : (network.ipv4_dns || ""));
    elementCache.setValue('network-ipv6-dns', Array.isArray(network.ipv6_dns) ? network.ipv6_dns.join(', ') : (network.ipv6_dns || ""));
    elementCache.setValue('network-description', network.description || "");
  } else {
    title.textContent = "添加网络";
    if (form) form.reset();
    elementCache.setValue('network-id', '');
  }
}
