import { apiGet } from "../utils/apiClient.js";
import { escapeHtml } from "../utils/ui.js";
import { cache, safeAsync } from "../utils/helpers.js";
import {
  getStatusText,
  getDeviceTypeName,
  getRoomTypeName,
  getActionIcon,
  formatTime,
  getOperationTypeText,
  getResourceTypeText
} from "../utils/formatter.js";
import { t } from "../utils/i18n.js";

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

  const result = await safeAsync(() => apiGet("/api/system/dashboard-stats"), "加载仪表盘数据", {
    showToast: false
  });

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
  // 页面仅展示以下统计卡；其余聚合字段（resources/users 等）供图表与后续扩展使用
  const elements = {
    "total-networks": data.networks?.networks || 0,
    "total-network-regions": data.networks?.regions || 0,
    "total-ip-addresses": data.ips?.total || 0,
    "active-devices": data.ips?.active || 0,
    "today-operations": data.activity?.operations_24h || 0
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
    fetchTopData("/api/resources/devices?page_size=5", "devices"),
    fetchTopData("/api/resources/cabinets?page_size=5", "cabinets"),
    fetchTopData("/api/logs/operation?page_size=5", "logs")
  ]);

  const topLists = {
    networks: results[0].status === "fulfilled" ? results[0].value : [],
    ips: results[1].status === "fulfilled" ? results[1].value : [],
    rooms: results[2].status === "fulfilled" ? results[2].value : [],
    devices: results[3].status === "fulfilled" ? results[3].value : [],
    cabinets: results[4].status === "fulfilled" ? results[4].value : [],
    logs: results[5].status === "fulfilled" ? results[5].value : []
  };

  cache.set(CACHE_KEY_TOP_LISTS, topLists, CACHE_TTL);
  renderTopLists(topLists);
}

async function fetchTopData(url, _type) {
  const result = await apiGet(url);
  // 各列表接口均为 paged_response，形状固定为 items
  if (result.success) {
    return result.data?.items ?? [];
  }
  return [];
}

function renderTopLists(data) {
  renderTopNetworks(data.networks);
  renderTopIPs(data.ips);
  renderTopRooms(data.rooms);
  renderTopCabinets(data.cabinets);
  renderTopLogs(data.logs);
}

function renderTopNetworks(items) {
  const container = document.getElementById("top-networks-list");
  if (!container) {
    return;
  }

  if (!items || items.length === 0) {
    container.innerHTML = `<li class="empty-list-item">${t("dashboard.no_network_data")}</li>`;
    return;
  }

  container.innerHTML = items
    .slice(0, 5)
    .map(
      (network) => `
    <li>
      <div class="item-name">
        <span class="item-icon">🌐</span>
        <div>
          <div>${escapeHtml(network.name || "-")}</div>
          <div class="item-meta">${escapeHtml(network.ipv4_cidr || network.ipv6_cidr || "-")}</div>
        </div>
      </div>
      <span class="item-value">${escapeHtml(network.network_region_name || "-")}</span>
    </li>
  `
    )
    .join("");
}

function renderTopIPs(items) {
  const container = document.getElementById("top-ips-list");
  if (!container) {
    return;
  }

  if (!items || items.length === 0) {
    container.innerHTML = `<li class="empty-list-item">${t("dashboard.no_ip_data")}</li>`;
    return;
  }

  container.innerHTML = items
    .slice(0, 5)
    .map(
      (ip) => `
    <li>
      <div class="item-name">
        <span class="item-icon">🔗</span>
        <div>
          <div>${escapeHtml(ip.ip_address || "-")}</div>
          <div class="item-meta">${escapeHtml(ip.hostname || ip.device_name || "-")}</div>
          <div class="item-meta">${escapeHtml([ip.network_region, ip.network_name].filter(Boolean).join(" / ") || "-")}</div>
        </div>
      </div>
      <span class="item-status status-${ip.status || "inactive"}">${getStatusText(ip.status)}</span>
    </li>
  `
    )
    .join("");
}

function renderTopRooms(items) {
  const container = document.getElementById("top-rooms-list");
  if (!container) {
    return;
  }

  if (!items || items.length === 0) {
    container.innerHTML = `<li class="empty-list-item">${t("dashboard.no_room_data")}</li>`;
    return;
  }

  container.innerHTML = items
    .slice(0, 5)
    .map(
      (room) => `
    <li>
      <div class="item-name">
        <span class="item-icon">🏠</span>
        <div>
          <div>${escapeHtml(room.name || "-")}</div>
          <div class="item-meta">${escapeHtml(getRoomTypeName(room.room_type))}</div>
        </div>
      </div>
      <span class="item-value">${room.workstation_count || 0} ${t("dashboard.unit_workstation")}</span>
    </li>
  `
    )
    .join("");
}

function renderTopCabinets(items) {
  const container = document.getElementById("top-cabinets-list");
  if (!container) {
    return;
  }

  if (!items || items.length === 0) {
    container.innerHTML = `<li class="empty-list-item">${t("dashboard.no_cabinet_data")}</li>`;
    return;
  }

  container.innerHTML = items
    .slice(0, 5)
    .map(
      (cabinet) => `
    <li>
      <div class="item-name">
        <span class="item-icon">🗄️</span>
        <div>
          <div>${escapeHtml(cabinet.name || "-")}</div>
          <div class="item-meta">${escapeHtml(cabinet.room_name || "-")}</div>
        </div>
      </div>
      <span class="item-value">${cabinet.position_count || 0} ${t("dashboard.unit_position")}</span>
    </li>
  `
    )
    .join("");
}

function renderTopLogs(items) {
  const container = document.getElementById("top-logs-list");
  if (!container) {
    return;
  }

  if (!items || items.length === 0) {
    container.innerHTML = `<li class="empty-list-item">${t("dashboard.no_log_data")}</li>`;
    return;
  }

  container.innerHTML = items
    .slice(0, 5)
    .map(
      (log) => `
    <li>
      <div class="item-name">
        <span class="item-icon">${getActionIcon(log.action)}</span>
        <div>
          <div>${escapeHtml(getOperationTypeText(log.action))}</div>
          <div class="item-meta">${escapeHtml(log.username || "-")} · ${formatTime(log.created_at)}</div>
        </div>
      </div>
      <span class="item-value">${escapeHtml(getResourceTypeText(log.resource_type))}</span>
    </li>
  `
    )
    .join("");
}

function initDashboardClickHandlers() {
  const dashboard = document.getElementById("dashboard");
  if (!dashboard) {
    return;
  }

  if (dashboard.dataset.clickInitialized === "true") {
    return;
  }
  dashboard.dataset.clickInitialized = "true";

  dashboard.addEventListener("click", (e) => {
    const card = e.target.closest(".stat-card.clickable, .dashboard-card");
    if (!card) {
      return;
    }

    // 如果点击的是链接或按钮，不进行卡片级别的跳转
    if (e.target.closest("a") || e.target.closest("button")) {
      return;
    }

    e.preventDefault();

    const nav = card.getAttribute("data-nav");
    const tab = card.getAttribute("data-tab");

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
  renderDeviceTypeChart(data.devices?.by_type || {});
  renderIpStatusChart(data.ips?.by_status || {});
}

function renderDeviceTypeChart(deviceTypes) {
  const container = document.getElementById("device-type-chart");
  if (!container) {
    return;
  }

  const total = Object.values(deviceTypes).reduce((a, b) => a + b, 0);
  if (total === 0) {
    container.innerHTML = `<div class="chart-empty">${t("common.no_data")}</div>`;
    return;
  }

  const colors = ["var(--chart-color-1)", "var(--chart-color-2)", "var(--chart-color-3)"];
  let html = '<div class="chart-bars">';

  let index = 0;
  for (const [type, count] of Object.entries(deviceTypes)) {
    const percentage = ((count / total) * 100).toFixed(1);
    const name = getDeviceTypeName(type);
    html += `
      <div class="chart-bar-item">
        <div class="chart-bar-label">${escapeHtml(name)}</div>
        <div class="chart-bar-container">
          <div class="chart-bar-fill" style="width: ${percentage}%; background-color: ${colors[index % colors.length]}"></div>
        </div>
        <div class="chart-bar-value">${count} (${percentage}%)</div>
      </div>
    `;
    index++;
  }

  html += "</div>";
  container.innerHTML = html;
}

function renderIpStatusChart(statusData) {
  const container = document.getElementById("ip-status-chart");
  if (!container) {
    return;
  }

  const total = Object.values(statusData).reduce((a, b) => a + b, 0);
  if (total === 0) {
    container.innerHTML = `<div class="chart-empty">${t("common.no_data")}</div>`;
    return;
  }

  const statusNames = {
    active: t("status.active"),
    inactive: t("status.inactive"),
    reserved: t("status.reserved")
  };

  const colors = {
    active: "var(--status-active)",
    inactive: "var(--status-inactive)",
    reserved: "var(--status-reserved)"
  };

  let html = '<div class="chart-pie">';
  html += '<div class="chart-pie-legend">';

  for (const [status, count] of Object.entries(statusData)) {
    const percentage = ((count / total) * 100).toFixed(1);
    const name = statusNames[status] || escapeHtml(status);
    const color = colors[status] || "var(--status-fallback)";

    html += `
      <div class="chart-legend-item">
        <span class="chart-legend-color" style="background-color: ${color}"></span>
        <span class="chart-legend-label">${name}: ${count} (${percentage}%)</span>
      </div>
    `;
  }

  html += "</div></div>";
  container.innerHTML = html;
}

async function loadFallbackData() {
  try {
    const [networksResponse, regionsResponse, ipResponse] = await Promise.all([
      apiGet("/api/resources/networks?page_size=1"),
      apiGet("/api/resources/network-regions?page_size=1"),
      apiGet("/api/resources/ip?page_size=1")
    ]);

    const getCount = (response) => {
      if (!response.success) {
        return 0;
      }
      const data = response.data;
      if (data.total !== undefined) {
        return data.total;
      }
      if (Array.isArray(data)) {
        return data.length;
      }
      if (data.items) {
        return data.items.length;
      }
      if (data.data && Array.isArray(data.data)) {
        return data.data.length;
      }
      return 0;
    };

    const dashboardData = {
      networks: {
        networks: getCount(networksResponse),
        regions: getCount(regionsResponse)
      },
      ips: {
        total: getCount(ipResponse),
        active: 0
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
