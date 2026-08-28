import { apiGet, apiPost } from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  handleError,
  appendPaginationToTable,
  debounce,
  escapeHtml,
  createSortState,
  updateSortIcons,
  initSortEvents,
  initThSearchPopovers
} from "../utils/ui.js";

import { t } from "../utils/i18n.js";

import { getDeviceTypeName } from "../utils/formatter.js";

import { loadModal, openModal, closeModal } from "../utils/modalLoader.js";

import { fillSelect, fetchOptionItems } from "../utils/resources.js";

const IP_PAGE_SIZE = 100;
let currentPageSize = IP_PAGE_SIZE;

let currentFilters = {
  device_name: "",
  network: "",
  ip_address: ""
};

let currentPage = 1;

const ipTableState = createSortState("updated_at", "desc");

// ====== IP查询 ======

// 加载设备列表到拉取MAC模态框（仅列出配置了 SNMP 的设备）
async function loadDevicesForPullMac() {
  // 拉取走 resources.js 的共享下拉缓存（10s TTL，写操作成功后自动失效），
  // page_size=1000 全量拉取语义与原实现一致
  let result = null;
  try {
    result = await fetchOptionItems("/api/resources/devices?page_size=1000");
  } catch (error) {
    console.error("加载设备列表失败:", error);
  }

  // 请求异常时与原实现一致：置入失败短提示占位项（fillSelect 的失败路径
  // 固定用 common.load_failed，无法表达该专属文案，故此处单独处理）
  if (!result) {
    const select = document.getElementById("pull-mac-device-select");
    if (select) {
      select.innerHTML = `<option value="">${t("ip.load_failed_short")}</option>`;
    }
    return;
  }

  // success=false 时与原实现一致：仅保留占位项
  const devices = result.success ? result.data?.items ?? [] : null;
  if (devices === null) {
    await fillSelect("pull-mac-device-select", null, { placeholderKey: "ip.select_device" });
    return;
  }
  if (devices.length === 0) {
    await fillSelect("pull-mac-device-select", null, { placeholderKey: "ip.no_device_data" });
    return;
  }

  const snmpDevices = devices.filter((dev) => dev.snmp_community || dev.snmp_username);
  if (snmpDevices.length === 0) {
    await fillSelect("pull-mac-device-select", null, { placeholderKey: "ip.no_snmp_device" });
    return;
  }

  await fillSelect("pull-mac-device-select", null, {
    items: snmpDevices,
    placeholderKey: "ip.select_device",
    itemToLabel: (dev) => `${dev.name} (${dev.ip_address || "-"})`
  });
}

// 加载网段列表到拉取MAC模态框
async function loadNetworksForPullMac() {
  let result = null;
  try {
    result = await fetchOptionItems("/api/resources/networks?page_size=1000");
  } catch (error) {
    console.error("加载网段列表失败:", error);
  }

  if (!result) {
    const select = document.getElementById("pull-mac-network-select");
    if (select) {
      select.innerHTML = `<option value="">${t("ip.load_failed_short")}</option>`;
    }
    return;
  }

  const networks = result.success && result.data ? (result.data?.items ?? []) : null;
  if (networks === null) {
    // success=false 或无 data 时与原实现一致：仅保留占位项
    await fillSelect("pull-mac-network-select", null, { placeholderKey: "ip.select_network" });
    return;
  }
  if (networks.length === 0) {
    await fillSelect("pull-mac-network-select", null, { placeholderKey: "ip.no_network_data" });
    return;
  }

  await fillSelect("pull-mac-network-select", null, {
    items: networks,
    placeholderKey: "ip.select_network",
    itemToLabel: (network) => `${network.name} (${network.ipv4_cidr || network.ipv6_cidr || "-"})`
  });
}

// 打开拉取MAC模态框（MAC 地址表头“拉取”按钮入口）
async function openPullMacModal() {
  const modal = await loadModal("pull-mac-modal");
  if (!modal) {
    return;
  }

  await Promise.all([loadDevicesForPullMac(), loadNetworksForPullMac()]);

  const form = document.getElementById("pull-mac-form");
  form.onsubmit = async (e) => {
    e.preventDefault();
    await pullIpMacData();
  };

  openModal("pull-mac-modal");
}

// 拉取IP MAC数据（目标设备与网段取自拉取MAC模态框）
async function pullIpMacData() {
  const deviceSelect = document.getElementById("pull-mac-device-select");
  const deviceId = deviceSelect ? deviceSelect.value : "";

  if (!deviceId) {
    showToast(t("ip.select_device_first"), "warning");
    return;
  }

  const networkSelect = document.getElementById("pull-mac-network-select");
  const networkId = networkSelect ? networkSelect.value : "";

  if (!networkId) {
    showToast(t("ip.select_network_first"), "warning");
    return;
  }

  const btn = document.getElementById("pull-ip-btn");
  const originalText = btn.textContent;

  try {
    btn.innerHTML = `<span class="loading"></span> ${t("ip.pulling")}`;
    btn.disabled = true;

    const result = await apiPost("/api/resources/ip/pull", {
      device_id: deviceId,
      network_id: networkId
    });

    if (result.success) {
      showToast(result.message || t("ip.pull_mac_success"), "success");
      closeModal("pull-mac-modal");
      loadIpMacData();
    } else {
      showToast(`${t("ip.pull_mac_failed")}: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, t("ip.pull_mac_failed"));
  } finally {
    btn.innerHTML = originalText;
    btn.disabled = false;
  }
}

export async function loadIpMacData(
  filters = currentFilters,
  page = currentPage,
  sortBy = null,
  sortOrder = null
) {
  currentFilters = filters;
  currentPage = page;
  if (sortBy) {
    ipTableState.setSort(sortBy, sortOrder);
  }

  try {
    const { device_name = "", device_type = "", network = "", ip_address = "" } = filters;

    const params = new URLSearchParams();
    if (device_name) {
      params.append("device_name", device_name);
    }
    if (device_type) {
      params.append("device_type", device_type);
    }
    if (network) {
      params.append("network", network);
    }
    if (ip_address) {
      params.append("ip_address", ip_address);
    }
    params.append("page", page);
    params.append("page_size", currentPageSize);
    params.append("sort_by", ipTableState.sortBy);
    params.append("sort_order", ipTableState.sortOrder);

    const result = await apiGet(`/api/resources/ip?${params.toString()}`);

    if (result.success && result.data) {
      const { items, total, page: currentPage, total_pages } = result.data;
      const pageNum = currentPage || 1;
      const startIndex = (pageNum - 1) * currentPageSize;

      renderTable("#ip-table", {
        data: items || [],
        columns: [
          {
            field: "id",
            render: (v, row, index) => startIndex + index + 1,
            className: "index-column"
          },
          {
            field: "location",
            render: (v, row) => {
              return escapeHtml(
                row.workstation_name ||
                  row.cabinet_name ||
                  row.port_device_name ||
                  row.device_name ||
                  "-"
              );
            }
          },
          { field: "device_name", render: (v) => escapeHtml(v || "-") },
          {
            field: "device_type",
            render: (v) => escapeHtml(getDeviceTypeName(v)),
            className: "col-center"
          },
          {
            field: "network_name",
            render: (v, row) =>
              `${escapeHtml(v || t("common.unknown"))} (${escapeHtml(row.network_region || t("common.unknown"))})`
          },
          { field: "ip_address", render: (v) => escapeHtml(v) },
          { field: "mac_address", render: (v) => escapeHtml(v || "-") },
          { field: "hostname", render: (v) => escapeHtml(v || "-") },
          {
            field: "status",
            render: (v) =>
              `<span class="status-badge ${v === "active" ? "status-active" : "status-inactive"}">${escapeHtml(v)}</span>`,
            className: "col-center"
          },
          { field: "last_seen", render: (v) => formatDateTime(v), className: "col-center" },
          { field: "created_at", render: (v) => formatDateTime(v), className: "col-center" }
        ],
        emptyMessage: t("ip.no_ip_data")
      });

      if (total !== undefined) {
        appendPaginationToTable(
          "#ip-table",
          { total, page: pageNum, total_pages, page_size: currentPageSize },
          (p) => loadIpMacData(filters, p),
          {
            pageSize: currentPageSize,
            onPageSizeChange: (size) => {
              currentPageSize = size;
              loadIpMacData(filters, 1);
            }
          }
        );
      }

      updateSortIcons("ip-table", ipTableState);

      return { total, page: currentPage, total_pages };
    }

    return null;
  } catch (error) {
    console.error("加载IP数据失败:", error);
    renderTable("#ip-table", {
      data: [],
      columns: [],
      emptyMessage: t("ip.server_connection_failed")
    });
    return null;
  }
}

export const initIpMacFunctions = () => {
  const ipSection = document.getElementById("ip");
  if (!ipSection) {
    return;
  }

  if (ipSection.dataset.initialized === "true") {
    return;
  }
  ipSection.dataset.initialized = "true";

  initIpFilters();
  initThSearchPopovers("#ip-table");

  initSortEvents("ip-table", ipTableState, (page, sortBy, sortOrder) =>
    loadIpMacData(currentFilters, page, sortBy, sortOrder)
  );

  const pullMacBtn = document.getElementById("open-pull-mac-modal");
  if (pullMacBtn) {
    pullMacBtn.addEventListener("click", openPullMacModal);
  }
};

export function initIpFilters() {
  const filterIds = [
    "ip-device-name-filter",
    "ip-device-type-filter",
    "ip-network-filter",
    "ip-address-filter"
  ];

  const debouncedFilter = debounce(applyIpFilters, 300);

  filterIds.forEach((filterId) => {
    const filterElement = document.getElementById(filterId);
    if (filterElement) {
      filterElement.addEventListener("input", debouncedFilter);
    }
  });
}

// 表头搜索弹层逻辑已通用化至 utils/ui.js 的 initThSearchPopovers(selector)

function applyIpFilters() {
  const filters = {
    device_name: document.getElementById("ip-device-name-filter")?.value || "",
    device_type: document.getElementById("ip-device-type-filter")?.value || "",
    network: document.getElementById("ip-network-filter")?.value || "",
    ip_address: document.getElementById("ip-address-filter")?.value || ""
  };

  currentPage = 1;
  loadIpMacData(filters, 1);
}
