import {
  apiGet,
  apiPost,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  handleError,
} from "../utils/ui.js";

import { getDeviceTypeName } from "../utils/formatter.js";

export async function loadSwitchesForPullMac(): Promise<void> {
  try {
    const result = await apiGet("/api/switches");
    const select = document.getElementById("pull-mac-switch-select") as HTMLSelectElement | null;
    if (!select) return;

    select.innerHTML = '<option value="">-- 选择交换机 --</option>';

    if (result.success && result.data) {
      const switches = Array.isArray(result.data) ? result.data : ((result.data as { items?: unknown[] }).items || []);

      if (switches.length === 0) {
        select.innerHTML = '<option value="">暂无交换机数据</option>';
        return;
      }

      let hasSnmpSwitch = false;
      switches.forEach((sw) => {
        const s = sw as Record<string, unknown>;
        if (s.snmp_community || s.snmp_username) {
          hasSnmpSwitch = true;
          const option = document.createElement("option");
          option.value = String(s.id);
          option.textContent = `${s.name} (${s.ip_address})`;
          select.appendChild(option);
        }
      });

      if (!hasSnmpSwitch) {
        select.innerHTML = '<option value="">暂无配置SNMP的交换机</option>';
      }
    }
  } catch (error) {
    console.error("加载交换机列表失败:", error);
    const select = document.getElementById("pull-mac-switch-select") as HTMLSelectElement | null;
    if (select) {
      select.innerHTML = '<option value="">加载失败</option>';
    }
  }
}

export async function loadNetworksForPullMac(): Promise<void> {
  try {
    const result = await apiGet("/api/resources/networks?page_size=1000");
    const select = document.getElementById("pull-mac-network-select") as HTMLSelectElement | null;
    if (!select) return;

    select.innerHTML = '<option value="">-- 选择网段 --</option>';

    if (result.success && result.data) {
      const data = result.data as { items?: Record<string, unknown>[]; data?: Record<string, unknown>[] } | Record<string, unknown>[];
      const networks = Array.isArray(data) ? data : (data.items || (data as { data?: Record<string, unknown>[] }).data || []);

      if (networks.length === 0) {
        select.innerHTML = '<option value="">暂无网段数据</option>';
        return;
      }

      networks.forEach((network) => {
        const n = network as Record<string, unknown>;
        const option = document.createElement("option");
        option.value = String(n.id);
        option.textContent = `${n.name} (${n.ipv4_cidr || n.ipv6_cidr || "-"})`;
        select.appendChild(option);
      });
    }
  } catch (error) {
    console.error("加载网段列表失败:", error);
    const select = document.getElementById("pull-mac-network-select") as HTMLSelectElement | null;
    if (select) {
      select.innerHTML = '<option value="">加载失败</option>';
    }
  }
}

export async function pullIpMacData(): Promise<void> {
  const switchSelect = document.getElementById("pull-mac-switch-select") as HTMLSelectElement | null;
  const switchId = switchSelect ? switchSelect.value : "";

  if (!switchId) {
    showToast("请先选择一个交换机", "warning");
    return;
  }

  const networkSelect = document.getElementById("pull-mac-network-select") as HTMLSelectElement | null;
  const networkId = networkSelect ? networkSelect.value : "";

  if (!networkId) {
    showToast("请先选择一个网段", "warning");
    return;
  }

  const btn = document.getElementById("pull-ip-btn") as HTMLButtonElement | null;
  if (!btn) return;
  const originalText = btn.textContent;

  try {
    btn.innerHTML = '<span class="loading"></span> 拉取中...';
    btn.disabled = true;

    const result = await apiPost("/api/resources/ip/pull", { switch_id: switchId, network_id: networkId });

    if (result.success) {
      showToast(result.message || "MAC数据拉取成功", "success");
      loadIpMacData();
    } else {
      showToast(`MAC数据拉取失败: ${result.message}`, "error");
    }
  } catch (error) {
    handleError(error, "拉取MAC数据失败");
  } finally {
    btn.innerHTML = originalText || "";
    btn.disabled = false;
  }
}

interface IpSearchParams {
  search?: string;
  device_type?: string;
  status?: string;
  page?: number;
  page_size?: number;
}

interface IpDataRecord {
  id: number;
  device_name?: string;
  device_type?: string;
  network_name?: string;
  network_region?: string;
  ip_address: string;
  mac_address?: string;
  hostname?: string;
  status: string;
  last_seen?: string;
  created_at?: string;
}

export async function loadIpMacData(searchParams: IpSearchParams = {}): Promise<{ total: number; page: number; total_pages: number } | null> {
  try {
    const { search = "", device_type = "", status = "", page = 1, page_size = 100 } = searchParams;

    const params = new URLSearchParams();
    if (search) params.append("search", search);
    if (device_type) params.append("device_type", device_type);
    if (status) params.append("status", status);
    params.append("page", String(page));
    params.append("page_size", String(page_size));

    const result = await apiGet(`/api/resources/ip?${params.toString()}`);

    if (result.success && result.data) {
      const data = result.data as { data: IpDataRecord[]; total: number; page: number; total_pages: number };
      const pageNum = data.page || 1;
      const startIndex = (pageNum - 1) * page_size;

      renderTable("#ip-table", {
        data: (data.data || []) as unknown as Record<string, unknown>[],
        columns: [
          { field: "id", render: (_v: unknown, _row: unknown, index: number) => String(startIndex + index + 1), className: "index-column" },
          { field: "device_name", render: (v: unknown) => (v as string) || "-" },
          { field: "device_type", render: (v: unknown) => getDeviceTypeName(v as string) },
          { field: "network_name", render: (v: unknown, row: unknown) => `${v || "未知"} (${(row as Record<string, unknown>).network_region || "未知"})` },
          { field: "ip_address", render: (v: unknown) => v as string },
          { field: "mac_address", render: (v: unknown) => (v as string) || "-" },
          { field: "hostname", render: (v: unknown) => (v as string) || "-" },
          { field: "status", render: (v: unknown) => `<span class="status-badge ${v === "active" ? "status-active" : "status-inactive"}">${v}</span>` },
          { field: "last_seen", render: (v: unknown) => formatDateTime(v as string) },
          { field: "created_at", render: (v: unknown) => formatDateTime(v as string) },
        ],
        emptyMessage: "暂无IP数据",
      });

      return { total: data.total, page: data.page, total_pages: data.total_pages };
    }

    return null;
  } catch (error) {
    console.error("加载IP数据失败:", error);
    renderTable("#ip-table", {
      data: [],
      columns: [],
      emptyMessage: "服务器连接失败，请检查网络或联系管理员",
    });
    return null;
  }
}

export const initIpMacFunctions = (): void => {
  const ipSection = document.getElementById("ip");
  if (!ipSection) return;

  if (ipSection.dataset.initialized === "true") return;
  ipSection.dataset.initialized = "true";

  ipSection.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    const id = target.id || target.dataset?.action;

    switch (id) {
      case "ip-refresh-btn":
      case "refresh":
        handleSearch();
        break;
      case "ip-search-btn":
      case "search":
        handleSearch();
        break;
      case "pull-ip-btn":
        pullIpMacData();
        break;
    }
  });

  ipSection.addEventListener("keypress", (e) => {
    if ((e.target as HTMLElement).id === "ip-search-input" && e.key === "Enter") {
      handleSearch();
    }
  });
};

function handleSearch(): void {
  const searchInput = document.getElementById("ip-search-input") as HTMLInputElement | null;
  const deviceTypeSelect = document.getElementById("ip-device-type-filter") as HTMLSelectElement | null;
  const statusSelect = document.getElementById("ip-status-filter") as HTMLSelectElement | null;

  loadIpMacData({
    search: searchInput?.value || "",
    device_type: deviceTypeSelect?.value || "",
    status: statusSelect?.value || "",
  });
}
