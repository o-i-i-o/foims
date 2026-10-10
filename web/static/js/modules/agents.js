/**
 * 主机监控模块（admin/secadmin）
 * 主机列表 / 状态与关键字筛选 / 30s 自动刷新 / 行内管理操作 /
 * 详情弹窗（分组卡片 + 近 24h 迷你曲线）
 */
import { apiGet, apiRequest, apiDelete, apiPut } from "../utils/apiClient.js";
import { escapeHtml, renderTable, appendPaginationToTable, showToast } from "../utils/ui.js";
import { showConfirm } from "../utils/confirm.js";
import { t } from "../utils/i18n.js";
import { formatDateTime, formatTime } from "../utils/formatter.js";
import { createSeqGuard, nextFrame } from "../utils/helpers.js";
import { iconButton } from "../utils/icons.js";
import { openModal, closeModal } from "../utils/modalLoader.js";

// 自动刷新周期与每页条数
const REFRESH_INTERVAL_MS = 30_000;
const PAGE_SIZE = 20;
// 表格列数（与 main.html 表头一致，空态/错误行 colspan 用）
const COLUMN_COUNT = 12;

// 列表请求序号：筛选/翻页/自动刷新并发时丢弃过期响应，防止表格与筛选态错乱
const listSeq = createSeqGuard();
// 历史曲线请求序号：快速切换详情弹窗时丢弃过期曲线
const historySeq = createSeqGuard();

// 当前页码（自动刷新沿用，不重置用户所在页）
let currentPage = 1;

// 状态元数据：状态点样式类 + 名称 i18n 键
const STATUS_META = {
  pending: { dot: "agent-dot-pending", label: "agents.status_pending" },
  active: { dot: "agent-dot-active", label: "agents.status_active" },
  offline: { dot: "agent-dot-offline", label: "agents.status_offline" },
  disabled: { dot: "agent-dot-disabled", label: "agents.status_disabled" },
  revoked: { dot: "agent-dot-revoked", label: "agents.status_revoked" }
};

// 行操作 → PATCH 目标状态（pending 不可设置，仅禁用/启用/吊销三向切换）
const ACTION_STATUS = { disable: "disabled", enable: "active", revoke: "revoked" };

// 自动刷新定时器与 visibilitychange 绑定标志（模块级单例，防重复定时器）
let refreshTimer = null;
let visibilityBound = false;

// ==========================================
// 初始化与页面生命周期
// ==========================================

/**
 * 初始化主机监控页面（navigation 每次进入本页都会调用）：
 * 事件委托用 dataset 守卫只绑一次，自动刷新定时器幂等启动
 */
export function initAgentsPage() {
  const section = document.getElementById("agents");
  if (!section) {
    return;
  }

  if (!section.dataset.eventsBound) {
    section.dataset.eventsBound = "true";
    bindToolbarEvents(section);
    bindRowActions();
  }
  bindVisibilityChange();
  startAutoRefresh();
}

/** 工具栏事件：状态筛选/回车/查询按钮重置到第 1 页，刷新按钮维持当前页 */
function bindToolbarEvents(section) {
  section.querySelector("#agents-status-filter")?.addEventListener("change", () => {
    loadAgentsData(1);
  });
  section.querySelector("#agents-keyword")?.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      loadAgentsData(1);
    }
  });
  section.querySelector("#agents-query-btn")?.addEventListener("click", () => {
    loadAgentsData(1);
  });
  section.querySelector("#agents-refresh-btn")?.addEventListener("click", () => {
    loadAgentsData(currentPage);
  });
  section.querySelector("#agents-threshold-btn")?.addEventListener("click", () => {
    openAlertThresholdModal();
  });
}

/** 行操作事件委托：详情/禁用/启用/吊销/删除按钮动态渲染，统一在 tbody 上监听 */
function bindRowActions() {
  const tbody = document.querySelector("#agents-table tbody");
  if (!tbody) {
    return;
  }
  tbody.addEventListener("click", (e) => {
    const btn = e.target.closest("button[data-agent-action]");
    if (!btn || btn.disabled) {
      return;
    }
    const id = btn.getAttribute("data-id");
    const action = btn.getAttribute("data-agent-action");
    if (action === "detail") {
      showAgentDetail(id);
      return;
    }
    if (action === "delete") {
      deleteAgent(id);
      return;
    }
    changeAgentStatus(id, action);
  });
}

/** 页面可见时每 30s 自动刷新；标签页隐藏或离开本页时清除定时器 */
function startAutoRefresh() {
  if (refreshTimer) {
    return;
  }
  refreshTimer = setInterval(() => {
    if (document.hidden) {
      return;
    }
    const section = document.getElementById("agents");
    if (!section?.classList.contains("active")) {
      stopAutoRefresh();
      return;
    }
    loadAgentsData(currentPage);
  }, REFRESH_INTERVAL_MS);
}

function stopAutoRefresh() {
  if (!refreshTimer) {
    return;
  }
  clearInterval(refreshTimer);
  refreshTimer = null;
}

/** 标签页隐藏时暂停轮询、恢复可见且停留本页时立即刷新（只绑定一次） */
function bindVisibilityChange() {
  if (visibilityBound) {
    return;
  }
  visibilityBound = true;
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      stopAutoRefresh();
      return;
    }
    const section = document.getElementById("agents");
    if (section?.classList.contains("active")) {
      loadAgentsData(currentPage);
      startAutoRefresh();
    }
  });
}

// ==========================================
// 列表加载与渲染
// ==========================================

/**
 * 加载主机列表：状态筛选 + 关键字 + 分页（page 传 1 重置页码；
 * 自动刷新/翻页沿用当前筛选态，避免覆盖用户输入）
 */
export async function loadAgentsData(page = currentPage) {
  currentPage = page;
  const requestSeq = listSeq.next();

  const params = new URLSearchParams();
  const status = document.getElementById("agents-status-filter")?.value || "";
  const keyword = document.getElementById("agents-keyword")?.value.trim() || "";
  if (status) {
    params.append("status", status);
  }
  if (keyword) {
    params.append("keyword", keyword);
  }
  params.append("page", page);
  params.append("page_size", PAGE_SIZE);

  try {
    const result = await apiGet(`/api/agents?${params.toString()}`);
    if (!listSeq.isCurrent(requestSeq)) {
      return; // 已有更新的请求，丢弃过期响应
    }
    const tbody = document.querySelector("#agents-table tbody");
    if (!tbody) {
      return;
    }
    if (!result.success) {
      tbody.innerHTML = `<tr class="empty-row"><td colspan="${COLUMN_COUNT}" class="text-center">${escapeHtml(result.message || t("common.load_failed"))}</td></tr>`;
      return;
    }
    const items = Array.isArray(result.data?.items) ? result.data.items : [];
    renderTable("#agents-table", items, renderAgentRow, t("agents.no_data"), COLUMN_COUNT);
    appendPaginationToTable(
      "#agents-table",
      {
        total: result.data?.total ?? 0,
        page: result.data?.page ?? page,
        page_size: result.data?.page_size ?? PAGE_SIZE,
        total_pages: result.data?.total_pages ?? 1
      },
      (p) => loadAgentsData(p)
    );
  } catch (error) {
    console.error("加载主机列表失败:", error);
    if (!listSeq.isCurrent(requestSeq)) {
      return;
    }
    const tbody = document.querySelector("#agents-table tbody");
    if (tbody) {
      tbody.innerHTML = `<tr class="empty-row"><td colspan="${COLUMN_COUNT}" class="text-center">${t("common.load_failed_retry")}</td></tr>`;
    }
  }
}

/** 渲染单行：状态点 / 主机名+来源徽标+备注 / 使用率迷你条 / 相对时间 / 行内操作 */
function renderAgentRow(agent) {
  const meta = STATUS_META[agent.status];
  const statusText = meta ? t(meta.label) : String(agent.status ?? "-");
  // SNMP 采集行显示来源徽标（非 Agent 上报，无令牌管理语义）
  const sourceBadge =
    agent.source === "snmp"
      ? `<span class="agent-source-badge">${escapeHtml(t("agents.source_snmp"))}</span>`
      : "";
  const labelBadge = agent.label
    ? `<span class="agent-host-label">${escapeHtml(agent.label)}</span>`
    : "";
  return `
    <td class="col-center">
      <span class="agent-status-wrap"><span class="agent-status-dot ${meta ? meta.dot : "agent-dot-pending"}" aria-hidden="true"></span>${escapeHtml(statusText)}</span>
    </td>
    <td>${escapeHtml(agent.hostname) || "-"}${sourceBadge}${labelBadge}</td>
    <td>${escapeHtml(agent.ip) || "-"}</td>
    <td>${escapeHtml(agent.os) || "-"}</td>
    <td class="col-center">${escapeHtml(agent.arch) || "-"}</td>
    <td>${renderUsageBar(agent.cpu_usage)}</td>
    <td>${renderUsageBar(agent.mem_usage_pct)}</td>
    <td>${renderUsageBar(agent.disk_usage_pct)}</td>
    <td class="col-center">${renderTemp(agent.max_temp)}</td>
    <td>${escapeHtml(agent.agent_version) || "-"}</td>
    <td>${agent.last_seen ? escapeHtml(formatTime(agent.last_seen)) : "-"}</td>
    <td class="col-center">${renderRowActions(agent)}</td>`;
}

/** 使用率迷你进度条：>85% 红、>60% 橙、其余绿；未上报显示 "-" */
function renderUsageBar(pct) {
  if (pct == null || !Number.isFinite(Number(pct))) {
    return `<span class="agent-usage-none">-</span>`;
  }
  const value = Math.min(100, Math.max(0, Number(pct)));
  const level = usageLevel(value);
  return `<span class="agent-usage">
    <span class="agent-usage-bar"><span class="agent-usage-fill agent-usage-${level}" style="width:${value.toFixed(1)}%"></span></span>
    <span class="agent-usage-num">${value.toFixed(1)}%</span>
  </span>`;
}

/** 使用率分档色：>85% 红、>60% 橙、其余绿 */
function usageLevel(value) {
  if (value > 85) {
    return "danger";
  }
  if (value > 60) {
    return "warning";
  }
  return "ok";
}

/** 最高温度单元格：未上报显示 "-" */
function renderTemp(temp) {
  if (temp == null || !Number.isFinite(Number(temp))) {
    return `<span class="agent-usage-none">-</span>`;
  }
  return `${Number(temp).toFixed(1)}℃`;
}

/** 行内操作按钮：按 status 动态显隐（吊销与删除需确认）；
 * SNMP 采集行无下发令牌，禁用/启用/吊销仅对 Agent 行有意义，予以隐藏 */
function renderRowActions(agent) {
  const isSnmp = agent.source === "snmp";
  const buttons = [
    iconButton({
      icon: "eye",
      label: t("agents.btn_detail"),
      cls: "agent-act-detail",
      attrs: `data-agent-action="detail" data-id="${agent.id}"`
    })
  ];
  if (!isSnmp && (agent.status === "active" || agent.status === "pending")) {
    buttons.push(
      iconButton({
        icon: "lock",
        label: t("agents.btn_disable"),
        cls: "agent-act-disable",
        attrs: `data-agent-action="disable" data-id="${agent.id}"`
      })
    );
  }
  if (!isSnmp && agent.status === "disabled") {
    buttons.push(
      iconButton({
        icon: "check",
        label: t("agents.btn_enable"),
        cls: "agent-act-enable",
        attrs: `data-agent-action="enable" data-id="${agent.id}"`
      })
    );
  }
  if (!isSnmp && agent.status !== "revoked") {
    buttons.push(
      iconButton({
        icon: "shield",
        label: t("agents.btn_revoke"),
        cls: "agent-act-revoke",
        attrs: `data-agent-action="revoke" data-id="${agent.id}"`
      })
    );
  }
  buttons.push(
    iconButton({
      icon: "trash",
      label: t("common.delete"),
      cls: "agent-act-delete",
      attrs: `data-agent-action="delete" data-id="${agent.id}"`
    })
  );
  return buttons.join("");
}

// ==========================================
// 行内管理操作
// ==========================================

/** 修改主机状态（禁用/启用/吊销）：吊销先确认——token 立即失效不可恢复 */
async function changeAgentStatus(id, action) {
  const status = ACTION_STATUS[action];
  if (!status) {
    return;
  }
  if (action === "revoke") {
    const confirmed = await showConfirm(t("agents.confirm_revoke"), {
      title: t("agents.btn_revoke"),
      confirmText: t("agents.btn_revoke"),
      danger: true
    });
    if (!confirmed) {
      return;
    }
  }
  try {
    const result = await apiRequest(`/api/agents/${id}`, {
      method: "PATCH",
      body: JSON.stringify({ status })
    });
    if (result.success) {
      showToast(t("agents.status_updated"), "success");
      loadAgentsData(currentPage);
      return;
    }
    showToast(result.message || t("common.operation_failed"), "error");
  } catch (error) {
    console.error("更新主机状态失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  }
}

/** 删除主机：确认后 DELETE（后端级联删除历史数据） */
async function deleteAgent(id) {
  const confirmed = await showConfirm(t("agents.confirm_delete"), {
    title: t("common.delete"),
    confirmText: t("common.delete"),
    danger: true
  });
  if (!confirmed) {
    return;
  }
  try {
    const result = await apiDelete(`/api/agents/${id}`);
    if (result.success) {
      showToast(t("common.delete_success"), "success");
      loadAgentsData(currentPage);
      return;
    }
    showToast(result.message || t("common.delete_failed"), "error");
  } catch (error) {
    console.error("删除主机失败:", error);
    showToast(t("common.operation_failed_retry"), "error");
  }
}

// ==========================================
// 告警阈值配置弹窗
// ==========================================

/** 阈值输入框定义：id 与取值范围（百分比 1-100 / 温度 1-200） */
const ALERT_THRESHOLD_FIELDS = [
  { id: "agent-alert-cpu", key: "cpu_pct", min: 1, max: 100 },
  { id: "agent-alert-mem", key: "mem_pct", min: 1, max: 100 },
  { id: "agent-alert-disk", key: "disk_pct", min: 1, max: 100 },
  { id: "agent-alert-temp", key: "temp_c", min: 1, max: 200 }
];

/** 打开告警阈值弹窗：拉取当前配置填充表单（closeModal 会移除 DOM，每次重绑提交事件） */
async function openAlertThresholdModal() {
  const modal = await openModal("agent-alert-threshold-modal");
  if (!modal) {
    return;
  }
  modal
    .querySelector("#agent-alert-threshold-form")
    ?.addEventListener("submit", saveAlertThresholds);
  try {
    const result = await apiGet("/api/agents/alert-thresholds");
    if (!result.success) {
      showToast(result.message || t("common.load_failed"), "error");
      return;
    }
    const config = result.data || {};
    const enabled = modal.querySelector("#agent-alert-enabled");
    if (enabled) {
      enabled.checked = !!config.enabled;
    }
    for (const field of ALERT_THRESHOLD_FIELDS) {
      const input = modal.querySelector(`#${field.id}`);
      if (input) {
        input.value = config[field.key] ?? "";
      }
    }
  } catch (error) {
    showToast(`${t("common.load_failed")}: ${error.message}`, "error");
  }
}

/** 保存告警阈值：前端范围校验后 PUT，服务端保存并立即评估一轮 */
async function saveAlertThresholds(e) {
  e.preventDefault();
  const modal = document.getElementById("agent-alert-threshold-modal");
  if (!modal || modal.dataset.saving === "true") {
    return;
  }
  const payload = {
    enabled: !!modal.querySelector("#agent-alert-enabled")?.checked,
    cpu_pct: null,
    mem_pct: null,
    disk_pct: null,
    temp_c: null
  };
  for (const field of ALERT_THRESHOLD_FIELDS) {
    const input = modal.querySelector(`#${field.id}`);
    if (!input) {
      continue;
    }
    const raw = String(input.value).trim();
    if (raw === "") {
      continue;
    }
    const value = Number(raw);
    if (!Number.isInteger(value) || value < field.min || value > field.max) {
      showToast(t("agents.alert_invalid_range", { min: field.min, max: field.max }), "error");
      input.focus();
      return;
    }
    payload[field.key] = value;
  }
  modal.dataset.saving = "true";
  try {
    const result = await apiPut("/api/agents/alert-thresholds", payload);
    if (!result.success) {
      showToast(`${t("agents.alert_save_failed")}: ${result.message}`, "error");
      return;
    }
    showToast(t("agents.alert_save_success"), "success");
    closeModal("agent-alert-threshold-modal");
  } catch (error) {
    showToast(`${t("agents.alert_save_failed")}: ${error.message}`, "error");
  } finally {
    delete modal.dataset.saving;
  }
}

// ==========================================
// 详情弹窗
// ==========================================

/** 打开详情弹窗：先取详情（失败提示并中止），填充分组卡片后再绘制历史曲线 */
async function showAgentDetail(id) {
  const result = await apiGet(`/api/agents/${id}`);
  if (!result.success || !result.data) {
    showToast(result.message || t("common.load_failed"), "error");
    return;
  }
  const modal = await openModal("agent-detail-modal");
  if (!modal) {
    return;
  }
  fillAgentDetail(modal, result.data);
  // modal 已激活显示后再绘制曲线：隐藏容器宽度为 0 时布局测量不可靠
  nextFrame(() => loadAgentHistory(id));
}

/** 弹窗字段填充小工具：字段 id 统一为 agent-detail-{field} */
function setDetailText(modal, field, text) {
  const el = modal.querySelector(`#agent-detail-${field}`);
  if (el) {
    el.textContent = text;
  }
}

/** 填充详情弹窗各分组卡片（原始数据人性化：字节/速率/运行时长） */
function fillAgentDetail(modal, agent) {
  const raw = agent.raw_metrics || {};

  // 标题栏主机名 + 头部概要（状态徽章 / IP）
  const titleHost = modal.querySelector("#agent-detail-title-host");
  if (titleHost) {
    titleHost.textContent = agent.hostname ? ` · ${agent.hostname}` : "";
  }
  setDetailText(modal, "ip", agent.ip || "-");
  const meta = STATUS_META[agent.status];
  const badge = modal.querySelector("#agent-detail-status-badge");
  if (badge) {
    badge.textContent = meta ? t(meta.label) : String(agent.status ?? "-");
    badge.className = meta
      ? `status-badge agent-detail-badge agent-badge-${agent.status}`
      : "status-badge agent-detail-badge";
  }

  // 系统信息
  setDetailText(modal, "os", raw.system?.os || agent.os || "-");
  setDetailText(modal, "kernel", raw.system?.kernel || "-");
  setDetailText(modal, "arch", raw.system?.arch || agent.arch || "-");
  setDetailText(modal, "uptime", formatUptime(raw.system?.uptime_secs ?? agent.uptime_secs));
  setDetailText(modal, "version", agent.agent_version || "-");
  setDetailText(modal, "machine-id", raw.machine_id || "-");
  setDetailText(modal, "collected-at", raw.collected_at ? formatDateTime(raw.collected_at) : "-");

  // CPU
  const cpu = raw.cpu || {};
  setDetailText(modal, "cpu", cpu.usage_pct == null ? "-" : `${Number(cpu.usage_pct).toFixed(1)}%`);
  setDetailText(modal, "cpu-cores", cpu.cores ?? "-");
  setDetailText(modal, "cpu-load", formatLoads(cpu));

  // 内存
  const mem = raw.memory || {};
  const memBar = modal.querySelector("#agent-detail-mem-bar");
  if (memBar) {
    memBar.innerHTML = renderUsageBar(agent.mem_usage_pct);
  }
  setDetailText(modal, "mem-total", formatBytes(mem.total));
  setDetailText(modal, "mem-used", formatBytes(mem.used));
  setDetailText(modal, "mem-swap", formatSwap(mem));

  // 磁盘 / 网络 / 温度风扇
  fillDetailRows(modal, "disks", raw.disks, renderDiskRow);
  fillDetailRows(modal, "nets", raw.nets, renderNetRow);
  fillDetailRows(modal, "sensors", raw.sensors, renderSensorRow);
}

/** 通用行填充：空数组渲染空态提示 */
function fillDetailRows(modal, field, rows, renderRow) {
  const container = modal.querySelector(`#agent-detail-${field}`);
  if (!container) {
    return;
  }
  const list = Array.isArray(rows) ? rows : [];
  container.innerHTML = list.length
    ? list.map(renderRow).join("")
    : `<p class="agent-detail-empty">${t("common.no_data")}</p>`;
}

/** 磁盘挂载点行：设备 / 挂载 / 容量条 / 读写 IOPS / 利用率 */
function renderDiskRow(disk) {
  const total = Number(disk.total) || 0;
  const used = Number(disk.used) || 0;
  const pct = total > 0 ? Math.min(100, (used / total) * 100) : 0;
  const level = usageLevel(pct);
  let util = "-";
  if (disk.util_pct != null) {
    util = `${Number(disk.util_pct).toFixed(1)}%`;
  }
  return `<div class="agent-disk-row">
    <span class="agent-disk-name" title="${escapeHtml(disk.mount || "")}">${escapeHtml(disk.device || "-")}</span>
    <span class="agent-disk-mount">${escapeHtml(disk.mount || "-")}</span>
    <span class="agent-bar"><span class="agent-bar-fill agent-usage-${level}" style="width:${pct.toFixed(1)}%"></span></span>
    <span class="agent-disk-cap">${formatBytes(used)} / ${formatBytes(total)}</span>
    <span class="agent-disk-iops">${t("agents.label_iops")}: ${formatIops(disk)}</span>
    <span class="agent-disk-util">${t("agents.label_util")}: ${escapeHtml(util)}</span>
  </div>`;
}

/** 读写 IOPS 数值：未上报返回 "-" */
function formatIops(disk) {
  if (disk.read_iops == null && disk.write_iops == null) {
    return "-";
  }
  const read = disk.read_iops == null ? "-" : Number(disk.read_iops).toFixed(1);
  const write = disk.write_iops == null ? "-" : Number(disk.write_iops).toFixed(1);
  return `${read} / ${write}`;
}

/** 网卡行：名称 / 收发速率（人性化 Mbps）/ 错误数 */
function renderNetRow(net) {
  return `<div class="agent-net-row">
    <span class="agent-net-iface">${escapeHtml(net.iface || "-")}</span>
    <span class="agent-net-rate">↓ ${formatBps(net.rx_bps)} · ↑ ${formatBps(net.tx_bps)}</span>
    <span class="agent-net-errors">${t("agents.label_errors")}: ${net.errors ?? 0}</span>
  </div>`;
}

/** 温度/风扇行：标签 / 类型（i18n 名）/ 值（℃ 或 RPM） */
function renderSensorRow(sensor) {
  const kind = String(sensor.kind || "").toLowerCase();
  let kindLabel = kind || t("common.unknown");
  if (kind === "temp") {
    kindLabel = t("agents.sensor_temp");
  } else if (kind === "fan") {
    kindLabel = t("agents.sensor_fan");
  }
  let value;
  if (sensor.value == null) {
    value = "-";
  } else if (kind === "temp") {
    value = `${Number(sensor.value).toFixed(1)} ℃`;
  } else if (kind === "fan") {
    value = `${Math.round(Number(sensor.value))} RPM`;
  } else {
    value = String(sensor.value);
  }
  return `<div class="agent-sensor-row">
    <span class="agent-sensor-label">${escapeHtml(sensor.label || "-")}</span>
    <span class="agent-sensor-kind">${escapeHtml(kindLabel)}</span>
    <span class="agent-sensor-value">${escapeHtml(value)}</span>
  </div>`;
}

// ==========================================
// 近 24h 迷你曲线
// ==========================================

/** 拉取 24h 历史并渲染迷你曲线卡（CPU/内存/磁盘/温度/网络） */
async function loadAgentHistory(id) {
  const requestSeq = historySeq.next();
  try {
    const result = await apiGet(`/api/agents/${id}/history?hours=24`);
    if (!historySeq.isCurrent(requestSeq)) {
      return; // 已打开另一台主机的详情，丢弃过期响应
    }
    if (!result.success) {
      showToast(t("common.load_failed"), "error");
      renderAllCharts([]);
      return;
    }
    const items = Array.isArray(result.data?.items) ? result.data.items : [];
    renderAllCharts(items);
  } catch (error) {
    console.error("加载主机历史数据失败:", error);
    if (historySeq.isCurrent(requestSeq)) {
      renderAllCharts([]);
    }
  }
}

/** 按指标拆分序列并渲染五张曲线卡（cpu/mem/disk/temp/rx/tx 均可能为 null） */
function renderAllCharts(items) {
  renderSparkline("agent-chart-cpu", items.map((item) => item?.cpu), "%", "agent-chart-line-cpu");
  renderSparkline(
    "agent-chart-memory",
    items.map((item) => item?.mem),
    "%",
    "agent-chart-line-memory"
  );
  renderSparkline(
    "agent-chart-disk",
    items.map((item) => item?.disk),
    "%",
    "agent-chart-line-disk"
  );
  renderSparkline(
    "agent-chart-temp",
    items.map((item) => item?.temp),
    "℃",
    "agent-chart-line-temp"
  );
  // 网络卡双线：下行 rx / 上行 tx（bit/s，与列表行 formatBps 口径一致）
  renderSparklineSeries(
    "agent-chart-net",
    [
      { values: items.map((item) => item?.rx_bps), lineClass: "agent-chart-line-rx" },
      { values: items.map((item) => item?.tx_bps), lineClass: "agent-chart-line-tx" }
    ],
    formatBps,
    t("agents.chart_net")
  );
}

/**
 * 迷你曲线（多序列）：固定 viewBox（0 0 320 96）自绘 polyline，
 * 每序列独立配色（lineClass）。max/min 标注由 formatValue 人性化；
 * null/缺失值视为断点（分段 polyline）；全空时渲染"暂无历史数据"。
 */
function renderSparklineSeries(containerId, series, formatValue, ariaLabel) {
  const container = document.getElementById(containerId);
  if (!container) {
    return;
  }

  const width = 320;
  const height = 96;
  const pad = 6;
  const nums = series
    .flatMap((s) => s.values)
    .filter((v) => v != null && Number.isFinite(Number(v)))
    .map(Number);
  if (!nums.length) {
    container.innerHTML = `<p class="agent-spark-empty">${t("agents.no_history")}</p>`;
    return;
  }

  const max = Math.max(...nums);
  const min = Math.min(...nums);
  const span = max - min || 1;
  const len = Math.max(...series.map((s) => s.values.length));
  const step = (width - pad * 2) / Math.max(len - 1, 1);
  const toY = (v) => height - pad - ((v - min) / span) * (height - pad * 2);

  // 连续非空点拼接 polyline：null 视为断点；孤立单点记录为圆点，避免序列仅 1 点时曲线空白
  const groups = series.map((s) => {
    const segments = [];
    const singles = [];
    let current = [];
    s.values.forEach((v, i) => {
      if (v == null || !Number.isFinite(Number(v))) {
        if (current.length > 1) {
          segments.push(current.join(" "));
        } else if (current.length === 1) {
          singles.push(current[0]);
        }
        current = [];
        return;
      }
      current.push(`${(pad + i * step).toFixed(1)},${toY(Number(v)).toFixed(1)}`);
    });
    if (current.length > 1) {
      segments.push(current.join(" "));
    } else if (current.length === 1) {
      singles.push(current[0]);
    }
    return { lineClass: s.lineClass, segments, singles };
  });

  const groupsSvg = groups
    .map((g) => {
      const lines = g.segments.map((points) => `<polyline points="${points}"/>`).join("");
      const dots = g.singles
        .map((point) => {
          const [cx, cy] = point.split(",");
          return `<circle cx="${cx}" cy="${cy}" r="3"/>`;
        })
        .join("");
      return `<g class="agent-spark-line ${g.lineClass}">${lines}${dots}</g>`;
    })
    .join("");
  container.innerHTML = `<svg class="agent-spark" viewBox="0 0 ${width} ${height}" preserveAspectRatio="none" role="img" aria-label="${escapeHtml(ariaLabel || "")}">
      <line class="agent-spark-gridline" x1="${pad}" y1="${(height / 3).toFixed(1)}" x2="${width - pad}" y2="${(height / 3).toFixed(1)}"></line>
      <line class="agent-spark-gridline" x1="${pad}" y1="${((height / 3) * 2).toFixed(1)}" x2="${width - pad}" y2="${((height / 3) * 2).toFixed(1)}"></line>
      ${groupsSvg}
    </svg>
    <div class="agent-spark-range"><span>${formatValue(max)}</span><span>${formatValue(min)}</span></div>`;
}

/** 单序列迷你曲线（四张百分比/温度卡复用，单位后缀标注） */
function renderSparkline(containerId, values, unit, lineClass) {
  renderSparklineSeries(
    containerId,
    [{ values, lineClass }],
    (v) => formatChartNum(v) + unit,
    unit
  );
}

/** 曲线 min/max 标注数值：整数去小数，其余保留一位 */
function formatChartNum(value) {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

// ==========================================
// 数值人性化工具
// ==========================================

/** 运行时长：>1 天 "x天y小时"、>1 小时 "x小时y分"、其余 "x分钟" */
function formatUptime(secs) {
  const total = Number(secs);
  if (!Number.isFinite(total) || total <= 0) {
    return "-";
  }
  const d = Math.floor(total / 86400);
  const h = Math.floor((total % 86400) / 3600);
  const m = Math.floor((total % 3600) / 60);
  if (d > 0) {
    return t("agents.uptime_dh", { d, h });
  }
  if (h > 0) {
    return t("agents.uptime_hm", { h, m });
  }
  return t("agents.uptime_m", { m });
}

/** 字节容量：自动选 B/KB/MB/GB/TB 单位 */
function formatBytes(bytes) {
  const value = Number(bytes);
  if (!Number.isFinite(value) || value <= 0) {
    return "-";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = value;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  return `${unitIndex === 0 ? size : size.toFixed(1)} ${units[unitIndex]}`;
}

/** 网络速率：bps → bps/Kbps/Mbps/Gbps */
function formatBps(bps) {
  const value = Number(bps);
  if (!Number.isFinite(value)) {
    return "-";
  }
  if (value >= 1e9) {
    return `${(value / 1e9).toFixed(2)} Gbps`;
  }
  if (value >= 1e6) {
    return `${(value / 1e6).toFixed(2)} Mbps`;
  }
  if (value >= 1e3) {
    return `${(value / 1e3).toFixed(1)} Kbps`;
  }
  return `${Math.round(value)} bps`;
}

/** CPU 负载：1/5/15 分钟负载拼接展示 */
function formatLoads(cpu) {
  const loads = [cpu.load1, cpu.load5, cpu.load15];
  if (loads.every((v) => v == null)) {
    return "-";
  }
  return loads.map((v) => (v == null ? "-" : Number(v).toFixed(2))).join(" / ");
}

/** 交换分区：已用 / 总量（均未上报时 "-"） */
function formatSwap(memory) {
  if (memory.swap_total == null && memory.swap_used == null) {
    return "-";
  }
  return `${formatBytes(memory.swap_used ?? 0)} / ${formatBytes(memory.swap_total ?? 0)}`;
}
