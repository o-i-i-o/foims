import { apiGet } from "../utils/apiClient.js";
import { escapeHtml } from "../utils/ui.js";
import { cache, safeAsync } from "../utils/helpers.js";
import {
  getStatusText,
  getRoomTypeName,
  getActionIcon,
  formatTime,
  getOperationTypeText,
  getResourceTypeText
} from "../utils/formatter.js";
import { t } from "../utils/i18n.js";
import { getHiddenSections } from "../utils/roleCapabilities.js";

const CACHE_KEY_STATS = "dashboard_stats";
const CACHE_KEY_TOP_LISTS = "dashboard_top_lists";
const CACHE_TTL = 60 * 1000;

// ===== 主机负载卡片（CPU/内存/温度页签轮播）状态 =====
const HOST_METRIC_KEYS = ["cpu", "memory", "temperature"];
const HOST_METRIC_ROTATE_MS = 10 * 1000;
let hostMetrics = { cpu: [], memory: [], temperature: [] };
let hostMetricActive = "cpu";
let hostMetricTimer = null;
// 手动切换页签后置 true：用户意图优先，自动轮播永久停止
let hostMetricManual = false;

export async function loadDashboardData(forceRefresh = false) {
  if (!forceRefresh) {
    const cached = cache.get(CACHE_KEY_STATS);
    if (cached) {
      updateDashboardUI(cached);
      await loadTopLists();
      initDashboardClickHandlers();
      return;
    }
  }

  const result = await safeAsync(() => apiGet("/api/system/dashboard-stats"), "加载仪表盘数据", {
    showToast: false
  });

  if (!result?.success || !result.data) {
    const fallbackData = await loadFallbackData();
    if (fallbackData) {
      updateDashboardUI(fallbackData);
      initDashboardClickHandlers();
    }
    return;
  }

  cache.set(CACHE_KEY_STATS, result.data, CACHE_TTL);
  updateDashboardUI(result.data);
  await loadTopLists();
  initDashboardClickHandlers();
}

function updateDashboardUI(data) {
  // 页面仅展示以下统计卡；其余聚合字段（resources/users 等）供后续扩展使用
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

  // 仅请求页面实际渲染的榜单；devices 榜单无对应渲染区块，不请求（死数据）。
  // 主机监控分区不可见角色跳过 top-metrics 请求（该端点仅 admin/secadmin 可访问）
  const results = await Promise.allSettled([
    fetchTopData("/api/resources/networks?page_size=5", "networks"),
    fetchTopData("/api/resources/ip?page_size=5", "ips"),
    fetchTopData("/api/resources/rooms?page_size=5", "rooms"),
    fetchTopData("/api/resources/cabinets?page_size=5", "cabinets"),
    fetchTopData("/api/logs/operation?page_size=5", "logs"),
    getHiddenSections().includes("agents") ? Promise.resolve(null) : fetchHostMetricsData()
  ]);

  const topLists = {
    networks: results[0].status === "fulfilled" ? results[0].value : [],
    ips: results[1].status === "fulfilled" ? results[1].value : [],
    rooms: results[2].status === "fulfilled" ? results[2].value : [],
    cabinets: results[3].status === "fulfilled" ? results[3].value : [],
    logs: results[4].status === "fulfilled" ? results[4].value : [],
    host_metrics: results[5].status === "fulfilled" ? results[5].value : null
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

/** 主机负载榜单：响应为 {cpu,memory,temperature} 对象而非分页结构 */
async function fetchHostMetricsData() {
  const result = await apiGet("/api/agents/top-metrics?limit=5");
  if (result.success && result.data) {
    return result.data;
  }
  return null;
}

function renderTopLists(data) {
  renderTopNetworks(data.networks);
  renderTopIPs(data.ips);
  renderTopRooms(data.rooms);
  renderTopCabinets(data.cabinets);
  renderTopLogs(data.logs);
  renderHostMetrics(data.host_metrics);
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
      <span class="item-status status-${escapeHtml(ip.status || "inactive")}">${escapeHtml(getStatusText(ip.status))}</span>
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

/** 主机负载卡片：页签切换 + 自动轮播（悬停暂停；手动切换后停止轮播） */
function renderHostMetrics(data) {
  const card = document.querySelector(".host-metrics-card");
  if (!card) {
    return;
  }

  // 主机监控分区不可见角色直接隐藏卡片（与导航显隐矩阵一致）
  if (getHiddenSections().includes("agents")) {
    card.hidden = true;
    return;
  }
  card.hidden = false;

  hostMetrics = {
    cpu: data?.cpu ?? [],
    memory: data?.memory ?? [],
    temperature: data?.temperature ?? []
  };
  renderHostMetricList();

  // 交互监听只绑定一次；重渲染（缓存命中/强刷）不重复挂
  const tabs = card.querySelector(".host-metrics-tabs");
  if (tabs && tabs.dataset.init !== "true") {
    tabs.dataset.init = "true";
    tabs.addEventListener("click", (e) => {
      const tab = e.target.closest(".host-metrics-tab");
      if (!tab) {
        return;
      }
      hostMetricManual = true;
      stopHostMetricRotation();
      setHostMetricTab(tab.dataset.metric);
    });
    card.addEventListener("mouseenter", stopHostMetricRotation);
    card.addEventListener("mouseleave", () => {
      if (!hostMetricManual) {
        startHostMetricRotation();
      }
    });
  }
  if (!hostMetricManual) {
    startHostMetricRotation();
  }
}

/** 切换页签：更新页签态并重绘当前指标榜单 */
function setHostMetricTab(metric) {
  if (!HOST_METRIC_KEYS.includes(metric)) {
    return;
  }
  hostMetricActive = metric;
  for (const tab of document.querySelectorAll(".host-metrics-tab")) {
    const active = tab.dataset.metric === metric;
    tab.classList.toggle("active", active);
    tab.setAttribute("aria-selected", String(active));
  }
  renderHostMetricList();
}

function renderHostMetricList() {
  const container = document.getElementById("top-host-metrics-list");
  if (!container) {
    return;
  }

  const items = hostMetrics[hostMetricActive] ?? [];
  if (items.length === 0) {
    container.innerHTML = `<li class="empty-list-item">${t("dashboard.no_host_metrics_data")}</li>`;
    return;
  }

  container.innerHTML = items
    .map((item, index) => {
      const name = item.hostname || item.ip || "-";
      const meta = [item.label, item.ip].filter(Boolean).join(" · ") || "-";
      const value =
        hostMetricActive === "temperature"
          ? renderHostTemp(item.value)
          : renderHostUsage(item.value);
      return `
    <li>
      <div class="item-name">
        <span class="item-icon">${index + 1}</span>
        <div>
          <div>${escapeHtml(name)}</div>
          <div class="item-meta">${escapeHtml(meta)}</div>
        </div>
      </div>
      <span class="item-value">${value}</span>
    </li>
  `;
    })
    .join("");
}

/** 使用率迷你条（复用主机监控页 agent-usage 样式与分档：>85 红 >60 橙） */
function renderHostUsage(pct) {
  const value = Number(pct);
  if (!Number.isFinite(value)) {
    return `<span class="agent-usage-none">-</span>`;
  }
  const clamped = Math.min(100, Math.max(0, value));
  let level = "ok";
  if (clamped > 85) {
    level = "danger";
  } else if (clamped > 60) {
    level = "warning";
  }
  return `<span class="agent-usage">
    <span class="agent-usage-bar"><span class="agent-usage-fill agent-usage-${level}" style="width:${clamped.toFixed(1)}%"></span></span>
    <span class="agent-usage-num">${clamped.toFixed(1)}%</span>
  </span>`;
}

/** 温度榜值：保留一位小数（℃） */
function renderHostTemp(temp) {
  const value = Number(temp);
  if (!Number.isFinite(value)) {
    return `<span class="agent-usage-none">-</span>`;
  }
  return `${value.toFixed(1)}℃`;
}

function startHostMetricRotation() {
  stopHostMetricRotation();
  hostMetricTimer = setInterval(() => {
    const next =
      HOST_METRIC_KEYS[(HOST_METRIC_KEYS.indexOf(hostMetricActive) + 1) % HOST_METRIC_KEYS.length];
    setHostMetricTab(next);
  }, HOST_METRIC_ROTATE_MS);
}

function stopHostMetricRotation() {
  if (hostMetricTimer) {
    clearInterval(hostMetricTimer);
    hostMetricTimer = null;
  }
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
