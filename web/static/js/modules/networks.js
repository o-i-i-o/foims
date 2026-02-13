// 导入必要的模块
import {
  apiGet,
  apiPost,
  apiPut,
  apiDelete,
  getAccessToken,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  getElementValue,
  handleFormSubmit,
  handleDelete,
  debounce,
  handleError
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";

import {
  loadNetworkTypeOptions
} from "../utils/resources.js";

// 加载网络区域数据并填充表格
export async function loadNetworkTypesData() {
  try {
    const result = await apiGet("/api/resources/network-regions");

    renderTable(
    "#network-types-table",
    result.success ? result.data : [],
    (networkType) => `
                <td>${networkType.name}</td>
                <td>${formatDateTime(networkType.created_at)}</td>
                <td>
                    <button class="btn btn-sm btn-edit" data-id="${networkType.id}">编辑</button>
                    <button class="btn btn-sm btn-delete" data-id="${networkType.id}">删除</button>
                </td>
            `,
    "暂无网络区域数据",
    3,
  );
  } catch (error) {
    console.error("加载网络区域数据失败:", error);
    renderTable(
      "#network-types-table",
      [],
      () => "",
      "加载失败，请刷新页面重试",
      3,
    );
  }
}

// 加载网络数据
export async function loadNetworksData(searchTerm = "") {
  const token = getAccessToken();
  if (!token) {
    return;
  }

  try {
    const response = await fetch("/api/resources/networks", {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const data = await response.json();
    const tbody = document.querySelector("#networks-table tbody");

    if (data.success && data.data.length > 0) {
      // 清除空行
      tbody.innerHTML = "";

      // 根据搜索词过滤数据
      const filteredData = searchTerm.trim()
        ? data.data.filter((network) => {
            const searchLower = searchTerm.toLowerCase();
            return (
              network.name.toLowerCase().includes(searchLower) ||
              network.network_region.toLowerCase().includes(searchLower) ||
              (network.ipv4_cidr &&
                network.ipv4_cidr.toLowerCase().includes(searchLower)) ||
              (network.ipv6_cidr &&
                network.ipv6_cidr.toLowerCase().includes(searchLower)) ||
              (network.description &&
                network.description.toLowerCase().includes(searchLower))
            );
          })
        : data.data;

      if (filteredData.length > 0) {
        // 添加网络数据行
        filteredData.forEach((network) => {
          const row = document.createElement("tr");
          row.innerHTML = `
                        <td>${network.name}</td>
                        <td>${network.network_region}</td>
                        <td>${network.ipv4_cidr || "-"}</td>
                        <td>${network.ipv6_cidr || "-"}</td>
                        <td>${new Date(network.created_at).toLocaleString()}</td>
                        <td>
                    <button class="btn btn-sm btn-secondary btn-usage" data-id="${network.id}">使用情况</button>
                    <button class="btn btn-sm btn-edit" data-id="${network.id}">编辑</button>
                    <button class="btn btn-sm btn-delete" data-id="${network.id}">删除</button>
                </td>
                    `;
          tbody.appendChild(row);
        });
      } else {
        // 显示搜索结果为空
        tbody.innerHTML =
          '<tr class="empty-row"><td colspan="7" class="text-center">没有找到匹配的网段数据</td></tr>';
      }
    } else {
      // 显示空行
      tbody.innerHTML =
        '<tr class="empty-row"><td colspan="7" class="text-center">暂无网络数据</td></tr>';
    }

    // 加载网络区域选项
    await loadNetworkTypeOptions();
  } catch (error) {
    console.error("加载网络数据失败:", error);
    const tbody = document.querySelector("#networks-table tbody");
    tbody.innerHTML =
      '<tr class="empty-row"><td colspan="7" class="text-center">加载失败，请刷新页面重试</td></tr>';
  }
}

// 初始化网段搜索和刷新功能
export function initNetworksSearch() {
  const searchInput = document.getElementById("networks-search");
  const refreshBtn = document.getElementById("networks-refresh-btn");

  if (searchInput) {
    // 使用防抖函数包装搜索函数，延迟300ms
    const debouncedSearch = debounce(function (value) {
      loadNetworksData(value);
    }, 300);

    // 搜索输入事件监听
    searchInput.addEventListener("input", function () {
      debouncedSearch(this.value);
    });
  }

  if (refreshBtn) {
    // 刷新按钮事件监听
    refreshBtn.addEventListener("click", function () {
      // 清空搜索框
      if (searchInput) {
        searchInput.value = "";
      }
      // 重新加载数据
      loadNetworksData();
    });
  }
}

// 计算网段的总IP数量
export function calculateTotalIps(cidr) {
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
export function generateIpAddresses(cidr) {
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

// 显示网段使用情况
export async function showNetworkUsage(id) {
  try {
    const networkResult = await apiGet(`/api/resources/networks/${id}`);
    if (!networkResult.success) {
      showToast("获取网段数据失败", "error");
      return;
    }
    
    const network = networkResult.data;
    const hasIPv4 = !!network.ipv4_cidr;
    const hasIPv6 = !!network.ipv6_cidr;
    
    const ipResult = await apiGet("/api/resources/ip");
    if (!ipResult.success) {
      showToast("获取IP数据失败", "error");
      return;
    }
    
    const ipData = ipResult.data?.data || ipResult.data || [];
    const allNetworkIps = Array.isArray(ipData) ? ipData.filter(ip => ip.network_id === id) : [];
    const ipv4Ips = allNetworkIps.filter(ip => ip.ip_version === 4 || !ip.ip_address.includes(':'));
    const ipv6Ips = allNetworkIps.filter(ip => ip.ip_version === 6 || ip.ip_address.includes(':'));
    
    let tabsHtml = '';
    let contentHtml = '';
    
    if (hasIPv4 && hasIPv6) {
      tabsHtml = `
        <div class="usage-tabs">
          <button class="usage-tab-btn active" data-tab="ipv4">IPv4 信息</button>
          <button class="usage-tab-btn" data-tab="ipv6">IPv6 信息</button>
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
            <p>该网段未配置IPv4或IPv6地址</p>
          </div>
        </div>
      `;
    }
    
    const usageHtml = `
      <div class="network-usage-container">
        <h4>${network.name} 使用情况</h4>
        ${tabsHtml}
        ${contentHtml}
      </div>
    `;
    
    const modalHtml = `
      <div class="modal fade show" id="network-usage-modal" tabindex="-1" role="dialog" style="display: block; background-color: rgba(0,0,0,0.5);">
        <div class="modal-dialog modal-xl" role="document">
          <div class="modal-content">
            <div class="modal-header">
              <h5 class="modal-title">网段使用情况</h5>
              <button type="button" class="close" data-dismiss="modal" aria-label="Close">
                <span aria-hidden="true">&times;</span>
              </button>
            </div>
            <div class="modal-body">
              ${usageHtml}
            </div>
            <div class="modal-footer">
              <button type="button" class="btn btn-secondary" data-dismiss="modal">关闭</button>
            </div>
          </div>
        </div>
      </div>
    `;
    
    const modalContainer = document.createElement('div');
    modalContainer.innerHTML = modalHtml;
    document.body.appendChild(modalContainer);
    
    const closeButtons = modalContainer.querySelectorAll('[data-dismiss="modal"]');
    closeButtons.forEach(button => {
      button.addEventListener('click', () => {
        modalContainer.remove();
      });
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
    
    addUsageStyles(modalContainer);
    
  } catch (error) {
    console.error("获取网段使用情况失败:", error);
    showToast("操作失败，请重试", "error");
  }
}

function buildIPv4Content(network, networkIps, networkId) {
  const cidr = network.ipv4_cidr;
  const totalIps = calculateTotalIps(cidr);
  const allIpAddresses = generateIpAddresses(cidr);
  
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
        <span class="stat-label">IPv4网段:</span>
        <span class="stat-value">${cidr || "未知"}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">网络区域:</span>
        <span class="stat-value">${network.network_region || "未知"}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">网关地址:</span>
        <span class="stat-value">${network.ipv4_gateway || "-"}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">DNS服务器:</span>
        <span class="stat-value">${network.ipv4_dns || "-"}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">总IP数:</span>
        <span class="stat-value">${totalIps}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">已使用IP数:</span>
        <span class="stat-value">${usedIps}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">未使用IP数:</span>
        <span class="stat-value">${unusedIps}</span>
      </div>
      <div class="stat-item">
        <span class="stat-label">使用率:</span>
        <span class="stat-value">${usageRate}%</span>
      </div>
    </div>
    
    <div class="usage-controls">
      <div class="filter-controls">
        <label>筛选: </label>
        <select id="ip-status-filter" class="form-control form-control-sm d-inline-block w-auto mr-2">
          <option value="all">全部</option>
          <option value="used">已使用</option>
          <option value="unused">未使用</option>
        </select>
        <button id="refresh-ipv4-usage" class="btn btn-sm btn-secondary">刷新</button>
      </div>
    </div>
    
    <div class="usage-visualization">
      <h5>IP地址可视化</h5>
      <div class="ip-grid" id="ip-grid">
        ${allIpAddresses.map(ip => {
          const isUsed = ipStatusMap.has(ip);
          const status = ipStatusMap.get(ip) || "unused";
          const statusClass = isUsed ? (status === "active" ? "ip-used-active" : "ip-used-inactive") : "ip-unused";
          const tooltipText = `${ip} (${isUsed ? status === "active" ? "活跃" : "非活跃" : "未使用"})`;
          
          return `
            <div class="ip-block ${statusClass}" data-ip="${ip}" data-status="${isUsed ? status : "unused"}" title="${tooltipText}">
              <span class="ip-label">${ip.split('.').pop()}</span>
            </div>
          `;
        }).join('')}
      </div>
    </div>
    
    <div class="usage-ips">
      <h5>IPv4 地址列表</h5>
      <div class="table-responsive">
        <table class="table table-sm">
          <thead>
            <tr>
              <th>IP地址</th>
              <th>状态</th>
              <th>所属资源</th>
              <th>MAC地址</th>
              <th>主机名</th>
            </tr>
          </thead>
          <tbody id="ipv4-list-body">
            ${networkIps.length > 0 ? networkIps.map(ip => `
              <tr>
                <td>${ip.ip_address}</td>
                <td>
                  <span class="status-badge ${ip.status === "active" ? "status-active" : "status-inactive"}">
                    ${ip.status}
                  </span>
                </td>
                <td>${ip.workstation_name || ip.cabinet_position_name || "-"}</td>
                <td>${ip.mac_address || "-"}</td>
                <td>${ip.hostname || "-"}</td>
              </tr>
            `).join('') : '<tr><td colspan="5" class="text-center">暂无IPv4地址记录</td></tr>'}
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
        <p>该网段未配置IPv6地址</p>
        <p class="no-ipv6-hint">请在网络设置中添加IPv6网段信息</p>
      </div>
    `;
  }
  
  const activeIps = networkIps.filter(ip => ip.status === "active").length;
  const inactiveIps = networkIps.filter(ip => ip.status !== "active").length;
  const totalAssigned = networkIps.length;
  
  return `
    <div class="ipv6-info-section">
      <h5>IPv6 基本信息</h5>
      <div class="ipv6-info-grid">
        <div class="ipv6-info-item">
          <span class="info-label">IPv6网段</span>
          <span class="info-value ipv6-address">${network.ipv6_cidr}</span>
        </div>
        <div class="ipv6-info-item">
          <span class="info-label">网关地址</span>
          <span class="info-value ipv6-address">${network.ipv6_gateway || "-"}</span>
        </div>
        <div class="ipv6-info-item">
          <span class="info-label">DNS服务器</span>
          <span class="info-value ipv6-address">${network.ipv6_dns || "-"}</span>
        </div>
        <div class="ipv6-info-item">
          <span class="info-label">连接状态</span>
          <span class="info-value">
            <span class="connection-status ${totalAssigned > 0 ? 'status-enabled' : 'status-disabled'}">
              ${totalAssigned > 0 ? '● 已启用' : '○ 未使用'}
            </span>
          </span>
        </div>
      </div>
    </div>
    
    <div class="ipv6-stats-section">
      <h5>IPv6 地址使用统计</h5>
      <div class="ipv6-stats-grid">
        <div class="ipv6-stat-card">
          <div class="stat-number">${totalAssigned}</div>
          <div class="stat-desc">已分配地址</div>
        </div>
        <div class="ipv6-stat-card active">
          <div class="stat-number">${activeIps}</div>
          <div class="stat-desc">活跃地址</div>
        </div>
        <div class="ipv6-stat-card inactive">
          <div class="stat-number">${inactiveIps}</div>
          <div class="stat-desc">非活跃地址</div>
        </div>
      </div>
    </div>
    
    <div class="ipv6-list-section">
      <div class="ipv6-list-header">
        <h5>IPv6 地址列表</h5>
        <button id="refresh-ipv6-usage" class="btn btn-sm btn-secondary">刷新</button>
      </div>
      <div class="table-responsive">
        <table class="table table-sm">
          <thead>
            <tr>
              <th>IPv6地址</th>
              <th>状态</th>
              <th>所属资源</th>
              <th>MAC地址</th>
              <th>主机名</th>
            </tr>
          </thead>
          <tbody id="ipv6-list-body">
            ${networkIps.length > 0 ? networkIps.map(ip => `
              <tr>
                <td class="ipv6-address-cell">${ip.ip_address}</td>
                <td>
                  <span class="status-badge ${ip.status === "active" ? "status-active" : "status-inactive"}">
                    ${ip.status}
                  </span>
                </td>
                <td>${ip.workstation_name || ip.cabinet_position_name || ip.switch_name || "-"}</td>
                <td>${ip.mac_address || "-"}</td>
                <td>${ip.hostname || "-"}</td>
              </tr>
            `).join('') : '<tr><td colspan="5" class="text-center">暂无IPv6地址记录</td></tr>'}
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
        const refreshIpResult = await apiGet("/api/resources/ip");
        if (refreshIpResult.success) {
          const refreshedNetworkIps = refreshIpResult.data.filter(ip => ip.network_id === networkId && (ip.ip_version === 4 || !ip.ip_address.includes(':')));
          
          const newIpStatusMap = new Map();
          refreshedNetworkIps.forEach(ip => {
            newIpStatusMap.set(ip.ip_address, ip.status || "inactive");
          });
          
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
                <td>${ip.ip_address}</td>
                <td>
                  <span class="status-badge ${ip.status === "active" ? "status-active" : "status-inactive"}">
                    ${ip.status}
                  </span>
                </td>
                <td>${ip.workstation_name || ip.cabinet_position_name || "-"}</td>
                <td>${ip.mac_address || "-"}</td>
                <td>${ip.hostname || "-"}</td>
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
        const refreshIpResult = await apiGet("/api/resources/ip");
        if (refreshIpResult.success) {
          const refreshedNetworkIps = refreshIpResult.data.filter(ip => ip.network_id === networkId && (ip.ip_version === 6 || ip.ip_address.includes(':')));
          
          if (ipListBody) {
            ipListBody.innerHTML = refreshedNetworkIps.length > 0 ? refreshedNetworkIps.map(ip => `
              <tr>
                <td class="ipv6-address-cell">${ip.ip_address}</td>
                <td>
                  <span class="status-badge ${ip.status === "active" ? "status-active" : "status-inactive"}">
                    ${ip.status}
                  </span>
                </td>
                <td>${ip.workstation_name || ip.cabinet_position_name || ip.switch_name || "-"}</td>
                <td>${ip.mac_address || "-"}</td>
                <td>${ip.hostname || "-"}</td>
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

function addUsageStyles(modalContainer) {
  const styleElement = document.createElement('style');
  styleElement.innerHTML = `
    .network-usage-container {
      font-size: 14px;
    }
    
    .usage-tabs {
      display: flex;
      gap: 5px;
      margin-bottom: 20px;
      border-bottom: 2px solid #dee2e6;
    }
    
    .usage-tab-btn {
      padding: 10px 20px;
      border: none;
      background: none;
      cursor: pointer;
      font-size: 14px;
      font-weight: 500;
      color: #6c757d;
      border-bottom: 2px solid transparent;
      margin-bottom: -2px;
      transition: all 0.2s ease;
    }
    
    .usage-tab-btn:hover {
      color: #495057;
    }
    
    .usage-tab-btn.active {
      color: #007bff;
      border-bottom-color: #007bff;
    }
    
    .usage-tab-content {
      display: none;
    }
    
    .usage-tab-content.active {
      display: block;
    }
    
    .usage-stats {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
      gap: 10px;
      margin-bottom: 20px;
      padding: 15px;
      background-color: #f8f9fa;
      border-radius: 8px;
    }
    
    .stat-item {
      display: flex;
      flex-direction: column;
    }
    
    .stat-label {
      font-weight: 500;
      color: #6c757d;
      font-size: 12px;
    }
    
    .stat-value {
      font-weight: 600;
      font-size: 16px;
      color: #495057;
    }
    
    .usage-controls {
      margin-bottom: 20px;
    }
    
    .filter-controls {
      display: flex;
      align-items: center;
    }
    
    .usage-visualization {
      margin-bottom: 20px;
    }
    
    .ip-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(50px, 1fr));
      gap: 6px;
      padding: 12px;
      background-color: #f8f9fa;
      border-radius: 8px;
      max-height: 300px;
      overflow-y: auto;
    }
    
    .ip-block {
      width: 100%;
      height: 32px;
      border-radius: 3px;
      display: flex;
      align-items: center;
      justify-content: center;
      cursor: pointer;
      transition: all 0.2s ease;
      position: relative;
    }
    
    .ip-label {
      font-size: 11px;
      font-weight: 500;
      color: #495057;
    }
    
    .ip-block:hover {
      transform: translateY(-2px);
      box-shadow: 0 2px 4px rgba(0,0,0,0.1);
    }
    
    .ip-unused {
      background-color: #e9ecef;
      border: 1px solid #dee2e6;
    }
    
    .ip-used-active {
      background-color: #c3e6cb;
      border: 1px solid #28a745;
    }
    
    .ip-used-inactive {
      background-color: #ffeeba;
      border: 1px solid #ffc107;
    }
    
    .usage-ips {
      margin-top: 20px;
    }
    
    .status-badge {
      display: inline-block;
      padding: 2px 8px;
      border-radius: 10px;
      font-size: 12px;
      font-weight: 500;
    }
    
    .status-active {
      background-color: #d4edda;
      color: #155724;
    }
    
    .status-inactive {
      background-color: #fff3cd;
      color: #856404;
    }
    
    /* IPv6 样式 */
    .ipv6-info-section {
      margin-bottom: 20px;
    }
    
    .ipv6-info-section h5 {
      margin-bottom: 15px;
      color: #495057;
      font-weight: 600;
    }
    
    .ipv6-info-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
      gap: 15px;
      padding: 15px;
      background-color: #f8f9fa;
      border-radius: 8px;
      border-left: 4px solid #6f42c1;
    }
    
    .ipv6-info-item {
      display: flex;
      flex-direction: column;
      gap: 5px;
    }
    
    .info-label {
      font-weight: 500;
      color: #6c757d;
      font-size: 12px;
    }
    
    .info-value {
      font-weight: 600;
      font-size: 14px;
      color: #495057;
      word-break: break-all;
    }
    
    .ipv6-address {
      font-family: 'Consolas', 'Monaco', monospace;
      color: #6f42c1;
    }
    
    .connection-status {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      padding: 4px 12px;
      border-radius: 15px;
      font-size: 13px;
      font-weight: 500;
    }
    
    .status-enabled {
      background-color: #d4edda;
      color: #155724;
    }
    
    .status-disabled {
      background-color: #e9ecef;
      color: #6c757d;
    }
    
    .ipv6-stats-section {
      margin-bottom: 20px;
    }
    
    .ipv6-stats-section h5 {
      margin-bottom: 15px;
      color: #495057;
      font-weight: 600;
    }
    
    .ipv6-stats-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
      gap: 15px;
    }
    
    .ipv6-stat-card {
      padding: 20px;
      background-color: #f8f9fa;
      border-radius: 8px;
      text-align: center;
      border: 1px solid #dee2e6;
    }
    
    .ipv6-stat-card.active {
      background-color: #d4edda;
      border-color: #28a745;
    }
    
    .ipv6-stat-card.inactive {
      background-color: #fff3cd;
      border-color: #ffc107;
    }
    
    .stat-number {
      font-size: 28px;
      font-weight: 700;
      color: #495057;
    }
    
    .ipv6-stat-card.active .stat-number {
      color: #155724;
    }
    
    .ipv6-stat-card.inactive .stat-number {
      color: #856404;
    }
    
    .stat-desc {
      font-size: 12px;
      color: #6c757d;
      margin-top: 5px;
    }
    
    .ipv6-list-section {
      margin-top: 20px;
    }
    
    .ipv6-list-header {
      display: flex;
      justify-content: space-between;
      align-items: center;
      margin-bottom: 15px;
    }
    
    .ipv6-list-header h5 {
      margin: 0;
      color: #495057;
      font-weight: 600;
    }
    
    .ipv6-address-cell {
      font-family: 'Consolas', 'Monaco', monospace;
      font-size: 13px;
      color: #6f42c1;
    }
    
    .no-ipv6-info {
      text-align: center;
      padding: 60px 20px;
      color: #6c757d;
    }
    
    .no-ipv6-icon {
      font-size: 48px;
      margin-bottom: 15px;
    }
    
    .no-ipv6-info p {
      margin: 5px 0;
    }
    
    .no-ipv6-hint {
      font-size: 12px;
      color: #adb5bd;
    }
    
    .no-network-info {
      text-align: center;
      padding: 60px 20px;
      color: #6c757d;
    }
    
    .table-responsive {
      overflow-x: auto;
    }
    
    @media (max-width: 768px) {
      .usage-tabs {
        flex-direction: column;
        border-bottom: none;
      }
      
      .usage-tab-btn {
        border-bottom: 1px solid #dee2e6;
        margin-bottom: 0;
      }
      
      .usage-tab-btn.active {
        border-bottom-color: #007bff;
        background-color: #f8f9fa;
      }
      
      .ipv6-info-grid {
        grid-template-columns: 1fr;
      }
      
      .ipv6-stats-grid {
        grid-template-columns: 1fr;
      }
    }
  `;
  modalContainer.appendChild(styleElement);
}

// 编辑网络
export async function editNetwork(id) {
  const token = getAccessToken();
  if (!token) return;

  try {
    // 根据ID获取网络数据
    const response = await fetch(`/api/resources/networks/${id}`, {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const result = await response.json();
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
  const token = getAccessToken();
  if (!token) return;

  try {
    const response = await fetch(`/api/resources/network-regions/${id}`, {
      headers: {
        Authorization: `Bearer ${token}`,
      },
    });

    const result = await response.json();
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

// ====== 提交网络区域表单 ======
export async function submitNetworkTypeForm() {
  const token = getAccessToken();
  if (!token) return;

  // 使用通用工具函数获取表单数据
  const id = getElementValue("network-type-id");
  const name = getElementValue("network-type-name", "trimmed");
  const description = getElementValue("network-type-description", "trimmed");

  // 验证必填字段
  if (!name) {
    showToast("网络区域名称不能为空", "warning");
    return;
  }

  const networkTypeData = {
    name,
    description: description || null,
  };

  // 使用通用表单提交处理函数
  const success = await handleFormSubmit({
    formData: networkTypeData,
    id,
    baseUrl: "/api/resources/network-regions",
    token,
    successMessage: "网络区域保存成功",
    modalId: "network-type-modal",
    reloadFunction: () => {
      loadNetworkTypesData();
      loadNetworkTypeOptions(); // 更新网络区域下拉选择器
    }
  });

  return success;
}

// ====== 提交网络表单 ======
export async function submitNetworkForm() {
  const token = getAccessToken();
  if (!token) return;

  // 使用通用工具函数获取表单数据
  const id = getElementValue("network-id");
  const name = getElementValue("network-name", "trimmed");
  const networkType = getElementValue("network-type"); // 直接获取UUID字符串，不转换为数字
  const ipv4_cidr = getElementValue("network-ipv4-cidr", "trimmed");
  const ipv6_cidr = getElementValue("network-ipv6-cidr", "trimmed");
  const ipv4_gateway = getElementValue("network-ipv4-gateway", "trimmed");
  const ipv6_gateway = getElementValue("network-ipv6-gateway", "trimmed");
  const ipv4_dns = getElementValue("network-ipv4-dns", "trimmed");
  const ipv6_dns = getElementValue("network-ipv6-dns", "trimmed");
  const description = getElementValue("network-description", "trimmed");

  // 验证必填字段
  if (!name) {
    showToast("网络名称不能为空", "warning");
    return;
  }

  if (!networkType) {
    showToast("请选择网络区域", "warning");
    return;
  }

  // 验证至少提供一个CIDR
  if (!ipv4_cidr && !ipv6_cidr) {
    showToast("至少需要提供一个有效的IPv4或IPv6 CIDR", "warning");
    return;
  }

  const networkData = {
    name,
    network_region_id: networkType,
    ipv4_cidr: ipv4_cidr || null,
    ipv6_cidr: ipv6_cidr || null,
    ipv4_gateway: ipv4_gateway || null,
    ipv6_gateway: ipv6_gateway || null,
    ipv4_dns: ipv4_dns || null,
    ipv6_dns: ipv6_dns || null,
    description: description || null,
  };

  // 使用通用表单提交处理函数
  const success = await handleFormSubmit({
    formData: networkData,
    id,
    baseUrl: "/api/resources/networks",
    token,
    successMessage: "网络保存成功",
    modalId: "network-modal",
    reloadFunction: loadNetworksData
  });

  return success;
}

// ====== 网络区域管理模态框 ======
export function openNetworkTypeModal(networkType = null) {
  const modal = document.getElementById("network-type-modal");
  const title = document.getElementById("network-type-modal-title");
  const form = document.getElementById("network-type-form");

  if (networkType) {
    // 编辑模式
    title.textContent = "编辑网络区域";
    document.getElementById("network-type-id").value = networkType.id;
    document.getElementById("network-type-name").value = networkType.name;
    document.getElementById("network-type-description").value =
      networkType.description || "";
  } else {
    // 添加模式
    title.textContent = "添加网络区域";
    form.reset();
    document.getElementById("network-type-id").value = "";
  }

  openModal("network-type-modal");
}

// ====== 网络管理模态框 ======
export async function openNetworkModal(network = null) {
  const modal = document.getElementById("network-modal");
  const title = document.getElementById("network-modal-title");
  const form = document.getElementById("network-form");

  // 加载网络区域选项
  await loadNetworkTypeOptions();

  if (network) {
    // 编辑模式
    title.textContent = "编辑网络";
    document.getElementById("network-id").value = network.id;
    // 直接使用网络名称，不需要从组合名称中提取
    document.getElementById("network-name").value = network.name;
    document.getElementById("network-type").value = network.network_region_id;
    document.getElementById("network-ipv4-cidr").value =
      network.ipv4_cidr || "";
    document.getElementById("network-ipv6-cidr").value =
      network.ipv6_cidr || "";
    document.getElementById("network-ipv4-gateway").value =
      network.ipv4_gateway || "";
    document.getElementById("network-ipv6-gateway").value =
      network.ipv6_gateway || "";
    document.getElementById("network-ipv4-dns").value = network.ipv4_dns || "";
    document.getElementById("network-ipv6-dns").value = network.ipv6_dns || "";
    document.getElementById("network-description").value =
      network.description || "";
  } else {
    // 添加模式
    title.textContent = "添加网络";
    form.reset();
    document.getElementById("network-id").value = "";
  }

  openModal("network-modal");
}
