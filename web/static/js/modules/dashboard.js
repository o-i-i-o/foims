import { apiGet } from "../utils/apiClient.js";
import { showToast } from "../utils/ui.js";
import { cache, safeAsync, nextFrame } from "../utils/helpers.js";
import { getStatusText, getDeviceTypeName, getRoomTypeName, getActionIcon, formatTime } from "../utils/formatter.js";

const CACHE_KEY_STATS = "dashboard_stats";
const CACHE_KEY_TOP_LISTS = "dashboard_top_lists";
const CACHE_TTL = 60 * 1000;

export async function loadDashboardData(forceRefresh = false) {
  if (!forceRefresh) {
    const cached = cache.get(CACHE_KEY_STATS);
    if (cached) {
      updateDashboardUI(cached);
      renderCharts(cached);
      await loadTopLists();
      initDashboardClickHandlers();
      return;
    }
  }
  
  const result = await safeAsync(
    () => apiGet("/api/system/dashboard-stats"),
    "加载仪表盘数据",
    { showToast: false }
  );
  
  if (!result || !result.success || !result.data) {
    const fallbackData = await loadFallbackData();
    if (fallbackData) {
      updateDashboardUI(fallbackData);
      initDashboardClickHandlers();
    }
    return;
  }
  
  cache.set(CACHE_KEY_STATS, result.data, CACHE_TTL);
  updateDashboardUI(result.data);
  renderCharts(result.data);
  await loadTopLists();
  initDashboardClickHandlers();
}

function updateDashboardUI(data) {
  const elements = {
    "total-networks": data.networks?.networks || 0,
    "total-network-types": data.networks?.regions || 0,
    "total-ip-addresses": data.ips?.total || 0,
    "active-devices": data.ips?.active || 0,
    "today-operations": data.activity?.operations_24h || 0,
    "total-users": data.users?.total || 0,
    "active-users": data.users?.active || 0,
    "total-rooms": data.locations?.rooms || 0,
    "total-cabinets": data.locations?.cabinets || 0,
    "total-workstations": data.locations?.workstations || 0,
    "total-positions": data.locations?.positions || 0,
    "total-switches": data.switches || 0,
    "logins-24h": data.activity?.logins_24h || 0
  };
  
  for (const [id, value] of Object.entries(elements)) {
    const el = document.getElementById(id);
    if (el) {
      el.textContent = value;
    }
  }
}

async function loadTopLists() {
  const cached = cache.get(CACHE_KEY_TOP_LISTS);
  if (cached) {
    renderTopLists(cached);
    return;
  }
  
  const results = await Promise.allSettled([
    fetchTopData("/api/resources/networks?page_size=5", "networks"),
    fetchTopData("/api/resources/ip?page_size=5", "ips"),
    fetchTopData("/api/resources/rooms?page_size=5", "rooms"),
    fetchTopData("/api/switches?page_size=5", "switches"),
    fetchTopData("/api/resources/cabinets?page_size=5", "cabinets"),
    fetchTopData("/api/logs/operation?page_size=5", "logs")
  ]);
  
  const topLists = {
    networks: results[0].status === "fulfilled" ? results[0].value : [],
    ips: results[1].status === "fulfilled" ? results[1].value : [],
    rooms: results[2].status === "fulfilled" ? results[2].value : [],
    switches: results[3].status === "fulfilled" ? results[3].value : [],
    cabinets: results[4].status === "fulfilled" ? results[4].value : [],
    logs: results[5].status === "fulfilled" ? results[5].value : []
  };
  
  cache.set(CACHE_KEY_TOP_LISTS, topLists, CACHE_TTL);
  renderTopLists(topLists);
}

async function fetchTopData(url, type) {
  const result = await apiGet(url);
  if (result.success && result.data) {
    if (Array.isArray(result.data)) return result.data;
    if (result.data.items) return result.data.items;
    if (result.data.data) return result.data.data;
  }
  return [];
}

function renderTopLists(data) {
  renderTopNetworks(data.networks);
  renderTopIPs(data.ips);
  renderTopRooms(data.rooms);
  renderTopSwitches(data.switches);
  renderTopCabinets(data.cabinets);
  renderTopLogs(data.logs);
}

function renderTopNetworks(items) {
  const container = document.getElementById("top-networks-list");
  if (!container) return;
  
  if (!items || items.length === 0) {
    container.innerHTML = '<li class="empty-list-item">暂无网络数据</li>';
    return;
  }
  
  container.innerHTML = items.slice(0, 5).map(network => `
    <li>
      <div class="item-name">
        <span class="item-icon">🌐</span>
        <div>
          <div>${network.name || '-'}</div>
          <div class="item-meta">${network.ipv4_cidr || network.ipv6_cidr || '-'}</div>
        </div>
      </div>
      <span class="item-value">${network.network_region_name || '-'}</span>
    </li>
  `).join('');
}

function renderTopIPs(items) {
  const container = document.getElementById("top-ips-list");
  if (!container) return;
  
  if (!items || items.length === 0) {
    container.innerHTML = '<li class="empty-list-item">暂无IP数据</li>';
    return;
  }
  
  container.innerHTML = items.slice(0, 5).map(ip => `
    <li>
      <div class="item-name">
        <span class="item-icon">🔗</span>
        <div>
          <div>${ip.ip_address || '-'}</div>
          <div class="item-meta">${ip.hostname || ip.device_name || '-'}</div>
        </div>
      </div>
      <span class="item-status status-${ip.status || 'inactive'}">${getStatusText(ip.status)}</span>
    </li>
  `).join('');
}

function renderTopRooms(items) {
  const container = document.getElementById("top-rooms-list");
  if (!container) return;
  
  if (!items || items.length === 0) {
    container.innerHTML = '<li class="empty-list-item">暂无房间数据</li>';
    return;
  }
  
  container.innerHTML = items.slice(0, 5).map(room => `
    <li>
      <div class="item-name">
        <span class="item-icon">🏠</span>
        <div>
          <div>${room.name || '-'}</div>
          <div class="item-meta">${getRoomTypeName(room.room_type)}</div>
        </div>
      </div>
      <span class="item-value">${room.workstation_count || 0} 工位</span>
    </li>
  `).join('');
}

function renderTopSwitches(items) {
  const container = document.getElementById("top-switches-list");
  if (!container) return;
  
  if (!items || items.length === 0) {
    container.innerHTML = '<li class="empty-list-item">暂无交换机数据</li>';
    return;
  }
  
  container.innerHTML = items.slice(0, 5).map(sw => `
    <li>
      <div class="item-name">
        <span class="item-icon">🔀</span>
        <div>
          <div>${sw.name || '-'}</div>
          <div class="item-meta">${sw.ip_address || '-'}</div>
        </div>
      </div>
      <span class="item-value">${sw.vendor || '-'}</span>
    </li>
  `).join('');
}

function renderTopCabinets(items) {
  const container = document.getElementById("top-cabinets-list");
  if (!container) return;
  
  if (!items || items.length === 0) {
    container.innerHTML = '<li class="empty-list-item">暂无机柜数据</li>';
    return;
  }
  
  container.innerHTML = items.slice(0, 5).map(cabinet => `
    <li>
      <div class="item-name">
        <span class="item-icon">🗄️</span>
        <div>
          <div>${cabinet.name || '-'}</div>
          <div class="item-meta">${cabinet.room_name || '-'}</div>
        </div>
      </div>
      <span class="item-value">${cabinet.position_count || 0} 机位</span>
    </li>
  `).join('');
}

function renderTopLogs(items) {
  const container = document.getElementById("top-logs-list");
  if (!container) return;
  
  if (!items || items.length === 0) {
    container.innerHTML = '<li class="empty-list-item">暂无操作日志</li>';
    return;
  }
  
  container.innerHTML = items.slice(0, 5).map(log => `
    <li>
      <div class="item-name">
        <span class="item-icon">${getActionIcon(log.action)}</span>
        <div>
          <div>${log.action || '-'}</div>
          <div class="item-meta">${log.username || '-'} · ${formatTime(log.created_at)}</div>
        </div>
      </div>
      <span class="item-value">${log.resource_type || '-'}</span>
    </li>
  `).join('');
}

function initDashboardClickHandlers() {
  const dashboard = document.getElementById('dashboard');
  if (!dashboard) return;
  
  if (dashboard.dataset.clickInitialized === 'true') return;
  dashboard.dataset.clickInitialized = 'true';
  
  dashboard.addEventListener('click', function(e) {
    const card = e.target.closest('.stat-card.clickable, .dashboard-card');
    if (!card) return;
    
    // 如果点击的是链接或按钮，不进行卡片级别的跳转
    if (e.target.closest('a') || e.target.closest('button')) {
      return;
    }
    
    e.preventDefault();
    
    const nav = card.getAttribute('data-nav');
    const tab = card.getAttribute('data-tab');
    
    if (nav) {
      window.location.hash = nav;
      
      if (tab) {
        // 使用 setTimeout 确保页面切换后再切换 tab
        setTimeout(() => {
          // 确保只选择标签按钮，避免选择到 dashboard card 本身（因为它也有 data-tab 属性）
          const tabBtn = document.querySelector(`.tab-btn[data-tab="${tab}"]`);
          if (tabBtn) {
            tabBtn.click();
          }
        }, 100);
      }
    }
  });
}

function renderCharts(data) {
  renderDeviceTypeChart(data.ips?.by_device_type || {});
  renderIpStatusChart(data.ips?.by_status || {});
  renderRoomTypeChart(data.rooms_by_type || {});
}

function renderDeviceTypeChart(deviceTypes) {
  const container = document.getElementById("device-type-chart");
  if (!container) return;
  
  const total = Object.values(deviceTypes).reduce((a, b) => a + b, 0);
  if (total === 0) {
    container.innerHTML = '<div class="chart-empty">暂无数据</div>';
    return;
  }
  
  const colors = ['#667eea', '#764ba2', '#f093fb'];
  let html = '<div class="chart-bars">';
  
  let index = 0;
  for (const [type, count] of Object.entries(deviceTypes)) {
    const percentage = ((count / total) * 100).toFixed(1);
    const name = getDeviceTypeName(type);
    html += `
      <div class="chart-bar-item">
        <div class="chart-bar-label">${name}</div>
        <div class="chart-bar-container">
          <div class="chart-bar-fill" style="width: ${percentage}%; background-color: ${colors[index % colors.length]}"></div>
        </div>
        <div class="chart-bar-value">${count} (${percentage}%)</div>
      </div>
    `;
    index++;
  }
  
  html += '</div>';
  container.innerHTML = html;
}

function renderIpStatusChart(statusData) {
  const container = document.getElementById("ip-status-chart");
  if (!container) return;
  
  const total = Object.values(statusData).reduce((a, b) => a + b, 0);
  if (total === 0) {
    container.innerHTML = '<div class="chart-empty">暂无数据</div>';
    return;
  }
  
  const statusNames = {
    'active': '活跃',
    'inactive': '不活跃',
    'reserved': '保留'
  };
  
  const colors = {
    'active': '#28a745',
    'inactive': '#dc3545',
    'reserved': '#ffc107'
  };
  
  let html = '<div class="chart-pie">';
  html += '<div class="chart-pie-legend">';
  
  for (const [status, count] of Object.entries(statusData)) {
    const percentage = ((count / total) * 100).toFixed(1);
    const name = statusNames[status] || status;
    const color = colors[status] || '#6c757d';
    
    html += `
      <div class="chart-legend-item">
        <span class="chart-legend-color" style="background-color: ${color}"></span>
        <span class="chart-legend-label">${name}: ${count} (${percentage}%)</span>
      </div>
    `;
  }
  
  html += '</div></div>';
  container.innerHTML = html;
}

function renderRoomTypeChart(roomTypes) {
  const container = document.getElementById("room-type-chart");
  if (!container) return;
  
  const total = Object.values(roomTypes).reduce((a, b) => a + b, 0);
  if (total === 0) {
    container.innerHTML = '<div class="chart-empty">暂无数据</div>';
    return;
  }
  
  const typeNames = {
    'office': '办公室',
    'data_center': '机房'
  };
  
  const colors = {
    'office': '#17a2b8',
    'data_center': '#fd7e14'
  };
  
  let html = '<div class="chart-donut">';
  
  for (const [type, count] of Object.entries(roomTypes)) {
    const percentage = ((count / total) * 100).toFixed(1);
    const name = typeNames[type] || type;
    const color = colors[type] || '#6c757d';
    
    html += `
      <div class="chart-donut-item">
        <div class="chart-donut-segment" style="background-color: ${color}">
          <span class="chart-donut-value">${count}</span>
        </div>
        <div class="chart-donut-label">${name} (${percentage}%)</div>
      </div>
    `;
  }
  
  html += '</div>';
  container.innerHTML = html;
}

async function loadFallbackData() {
  try {
    const [networksResponse, regionsResponse, ipResponse] = await Promise.all([
      apiGet("/api/resources/networks?page_size=1000"),
      apiGet("/api/resources/network-regions?page_size=1000"),
      apiGet("/api/resources/ip?page_size=1000")
    ]);

    const dashboardData = {
      networks: {
        networks: networksResponse.success ? (networksResponse.data.items || networksResponse.data).length : 0,
        regions: regionsResponse.success ? (regionsResponse.data.items || regionsResponse.data).length : 0
      },
      ips: {
        total: ipResponse.success ? (ipResponse.data.items || ipResponse.data).length : 0,
        active: ipResponse.success
          ? (ipResponse.data.items || ipResponse.data).filter(({ status }) => status === "active").length
          : 0
      },
      activity: {
        operations_24h: 0
      }
    };

    cache.set(CACHE_KEY_STATS, dashboardData, CACHE_TTL);
    return dashboardData;
  } catch (error) {
    console.error("加载备用数据失败:", error);
    return null;
  }
}

export function getStatsCache() {
  return cache.get(CACHE_KEY_STATS);
}

export function clearDashboardCache() {
  cache.delete(CACHE_KEY_STATS);
  cache.delete(CACHE_KEY_TOP_LISTS);
}
