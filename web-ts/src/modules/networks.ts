import {
  apiGet,
} from "../utils/apiClient.js";

import {
  showToast,
  renderTable,
  formatDateTime,
  debounce,
  handleError,
  appendPaginationToTable,
  escapeHtml,
} from "../utils/ui.js";

import { openModal, closeModal } from "../utils/modal.js";
import { t } from "../utils/i18n.js";

import {
  loadNetworkTypeOptions,
} from "../utils/resources.js";

import { elementCache } from "../utils/helpers.js";
import { networkRegionManager, networkManager } from "../utils/managers.js";

const NETWORK_TYPE_PAGE_SIZE = 20;

export async function loadNetworkTypesData(page = 1): Promise<void> {
  try {
    const result = await networkRegionManager.list({
      page,
      pageSize: NETWORK_TYPE_PAGE_SIZE,
    });

    if (!result) return;

    const data = result.data as { items?: Record<string, unknown>[]; total?: number };
    const items = data.items || [];

    renderTable("#network-types-table", {
      data: items as unknown as Record<string, unknown>[],
      columns: [
        { field: "name", render: (v: unknown) => v as string },
        { field: "created_at", render: (v: unknown) => formatDateTime(v as string) },
        { field: "id", render: (v: unknown) => `
          <button class="btn btn-sm btn-edit" data-id="${v}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">删除</button>
        ` },
      ],
      emptyMessage: "暂无网络区域数据",
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#network-types-table", data, loadNetworkTypesData);
    }
  } catch (error) {
    console.error("加载网络区域数据失败:", error);
    renderTable("#network-types-table", {
      data: [],
      columns: [],
      emptyMessage: "加载失败，请刷新页面重试",
    });
  }
}

const NETWORK_PAGE_SIZE = 20;

export async function loadNetworksData(page = 1, searchTerm = ""): Promise<void> {
  try {
    const result = await networkManager.list({
      page,
      pageSize: NETWORK_PAGE_SIZE,
      search: searchTerm,
    });

    if (!result) return;

    const data = result.data as { items?: Record<string, unknown>[]; total?: number };
    const networks = data.items || [];
    const startIndex = (page - 1) * NETWORK_PAGE_SIZE;

    renderTable("#networks-table", {
      data: networks as unknown as Record<string, unknown>[],
      columns: [
        { field: "id", render: (_v: unknown, _row: unknown, index: number) => String(startIndex + index + 1), className: "index-column" },
        { field: "name", render: (v: unknown) => escapeHtml(v as string) },
        { field: "network_region", render: (v: unknown) => escapeHtml(v as string) },
        { field: "ipv4_cidr", render: (v: unknown) => escapeHtml(v as string) || "-" },
        { field: "ipv6_cidr", render: (v: unknown) => escapeHtml(v as string) || "-" },
        { field: "created_at", render: (v: unknown) => new Date(v as string).toLocaleString() },
        { field: "id", render: (v: unknown) => `
          <button class="btn btn-sm btn-secondary btn-usage" data-id="${v}">使用情况</button>
          <button class="btn btn-sm btn-edit" data-id="${v}">编辑</button>
          <button class="btn btn-sm btn-delete" data-id="${v}">删除</button>
        ` },
      ],
      emptyMessage: "没有找到匹配的网段数据",
    });

    if (data.total !== undefined) {
      appendPaginationToTable("#networks-table", data, (p: number) => loadNetworksData(p, searchTerm));
    }

    await loadNetworkTypeOptions();
  } catch (error) {
    handleError(error);
    renderTable("#networks-table", { data: [], columns: [], emptyMessage: "加载失败，请刷新页面重试" });
  }
}

export function initNetworksSearch(): void {
  const searchInput = document.getElementById("networks-search") as HTMLInputElement | null;
  const refreshBtn = document.getElementById("networks-refresh-btn") as HTMLButtonElement | null;

  if (searchInput) {
    const debouncedSearch = debounce(function (value: unknown) {
      loadNetworksData(1, value as string);
    }, 300);

    searchInput.addEventListener("input", function () {
      debouncedSearch(this.value);
    });
  }

  if (refreshBtn) {
    refreshBtn.addEventListener("click", function () {
      if (searchInput) {
        searchInput.value = "";
      }
      loadNetworksData();
    });
  }
}

export function calculateTotalIps(cidr: string | null | undefined): number {
  if (!cidr) return 0;

  try {
    const parts = cidr.split("/");
    if (parts.length !== 2) return 0;

    return Math.pow(2, 32 - parseInt(parts[1])) - 2;
  } catch (error) {
    console.error("计算总IP数量失败:", error);
    return 0;
  }
}

export function generateIpAddresses(cidr: string | null | undefined): string[] {
  if (!cidr) return [];

  try {
    const parts = cidr.split("/");
    if (parts.length !== 2) return [];

    const ip = parts[0];

    if (!/^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/.test(ip)) return [];

    const ipParts = ip.split(".").map(Number);
    const totalIps = calculateTotalIps(cidr);

    const ipAddresses: string[] = [];
    const maxDisplayIps = Math.min(totalIps, 256);

    for (let i = 1; i <= maxDisplayIps; i++) {
      const newIpParts = [...ipParts];
      let carry = i;

      for (let j = 3; j >= 0 && carry > 0; j--) {
        const sum = newIpParts[j] + carry;
        newIpParts[j] = sum % 256;
        carry = Math.floor(sum / 256);
      }

      ipAddresses.push(newIpParts.join("."));
    }

    return ipAddresses;
  } catch (error) {
    console.error("生成IP地址失败:", error);
    return [];
  }
}

interface IpRecord {
  ip_address: string;
  status: string;
  network_id: number;
  workstation_name?: string;
  cabinet_position_name?: string;
  switch_name?: string;
  mac_address?: string;
  hostname?: string;
}

interface NetworkData {
  id: number;
  name: string;
  network_region?: string;
  network_region_id?: number;
  ipv4_cidr?: string;
  ipv6_cidr?: string;
  ipv4_gateway?: string;
  ipv6_gateway?: string;
  ipv4_dns?: string | string[];
  ipv6_dns?: string | string[];
  description?: string;
}

export async function showNetworkUsage(id: string | number): Promise<void> {
  try {
    const networkResult = await apiGet(`/api/resources/networks/${id}`);
    if (!networkResult.success) {
      showToast("获取网段数据失败", "error");
      return;
    }

    const network = networkResult.data as NetworkData;
    const hasIPv4 = !!network.ipv4_cidr;
    const hasIPv6 = !!network.ipv6_cidr;

    const ipResult = await apiGet("/api/resources/ip?page_size=1000");
    if (!ipResult.success) {
      showToast("获取IP数据失败", "error");
      return;
    }

    const ipData = (ipResult.data as { data?: IpRecord[] }).data || ipResult.data as IpRecord[] || [];
    const allNetworkIps = Array.isArray(ipData) ? ipData.filter((ip: IpRecord) => ip.network_id === id) : [];
    const isIPv6 = (ip: IpRecord) => ip.ip_address.includes(":");
    const ipv4Ips = allNetworkIps.filter((ip: IpRecord) => !isIPv6(ip));
    const ipv6Ips = allNetworkIps.filter((ip: IpRecord) => isIPv6(ip));

    let tabsHtml = "";
    let contentHtml = "";

    if (hasIPv4 && hasIPv6) {
      tabsHtml = `
        <div class="usage-tabs">
          <button class="usage-tab-btn active" data-tab="ipv4">IPv4 信息</button>
          <button class="usage-tab-btn" data-tab="ipv6">IPv6 信息</button>
        </div>
      `;
      contentHtml = `
        <div class="usage-tab-content active" id="ipv4-content">
          ${buildIPv4Content(network, ipv4Ips)}
        </div>
        <div class="usage-tab-content" id="ipv6-content">
          ${buildIPv6Content(network, ipv6Ips)}
        </div>
      `;
    } else if (hasIPv4) {
      contentHtml = `
        <div class="usage-tab-content active">
          ${buildIPv4Content(network, ipv4Ips)}
        </div>
      `;
    } else if (hasIPv6) {
      contentHtml = `
        <div class="usage-tab-content active">
          ${buildIPv6Content(network, ipv6Ips)}
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

    const modalContainer = document.createElement("div");
    modalContainer.innerHTML = modalHtml;
    document.body.appendChild(modalContainer);

    const closeButtons = modalContainer.querySelectorAll('[data-dismiss="modal"]');
    closeButtons.forEach(button => {
      button.addEventListener("click", () => {
        modalContainer.remove();
      });
    });

    const tabButtons = modalContainer.querySelectorAll(".usage-tab-btn");
    tabButtons.forEach(btn => {
      btn.addEventListener("click", () => {
        tabButtons.forEach(b => b.classList.remove("active"));
        btn.classList.add("active");

        const tabId = (btn as HTMLElement).dataset.tab;
        const contents = modalContainer.querySelectorAll(".usage-tab-content");
        contents.forEach(content => {
          content.classList.remove("active");
          if ((content as HTMLElement).id === `${tabId}-content`) {
            content.classList.add("active");
          }
        });
      });
    });

    bindIPv4Events(modalContainer, id);
    bindIPv6Events(modalContainer, id);

  } catch (error) {
    console.error("获取网段使用情况失败:", error);
    showToast("操作失败，请重试", "error");
  }
}

function buildIPv4Content(network: NetworkData, networkIps: IpRecord[]): string {
  const cidr = network.ipv4_cidr;
  const totalIps = calculateTotalIps(cidr);
  const allIpAddresses = generateIpAddresses(cidr);

  const ipStatusMap = new Map<string, string>();
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
        <span class="stat-value">${Array.isArray(network.ipv4_dns) ? network.ipv4_dns.join(", ") : (network.ipv4_dns || "-")}</span>
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
              <span class="ip-label">${ip.split(".").pop()}</span>
            </div>
          `;
        }).join("")}
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
              <th>${t("ip.hostname")}</th>
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
            `).join("") : '<tr><td colspan="5" class="text-center">暂无IPv4地址记录</td></tr>'}
          </tbody>
        </table>
      </div>
    </div>
  `;
}

function buildIPv6Content(network: NetworkData, networkIps: IpRecord[]): string {
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
          <span class="info-value ipv6-address">${Array.isArray(network.ipv6_dns) ? network.ipv6_dns.join(", ") : (network.ipv6_dns || "-")}</span>
        </div>
        <div class="ipv6-info-item">
          <span class="info-label">连接状态</span>
          <span class="info-value">
            <span class="connection-status ${totalAssigned > 0 ? "status-enabled" : "status-disabled"}">
              ${totalAssigned > 0 ? "● 已启用" : "○ 未使用"}
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
              <th>${t("ip.hostname")}</th>
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
            `).join("") : '<tr><td colspan="5" class="text-center">暂无IPv6地址记录</td></tr>'}
          </tbody>
        </table>
      </div>
    </div>
  `;
}

function bindIPv4Events(modalContainer: HTMLElement, networkId: string | number): void {
  const filterSelect = modalContainer.querySelector("#ip-status-filter") as HTMLSelectElement | null;
  const ipGrid = modalContainer.querySelector("#ip-grid");
  const ipListBody = modalContainer.querySelector("#ipv4-list-body");

  if (filterSelect && ipGrid) {
    filterSelect.addEventListener("change", () => {
      const filterValue = filterSelect.value;
      const ipBlocks = ipGrid.querySelectorAll(".ip-block");

      ipBlocks.forEach(block => {
        const status = (block as HTMLElement).dataset.status;
        const isUsed = status !== "unused";

        if (filterValue === "all") {
          (block as HTMLElement).style.display = "block";
        } else if (filterValue === "used" && isUsed) {
          (block as HTMLElement).style.display = "block";
        } else if (filterValue === "unused" && !isUsed) {
          (block as HTMLElement).style.display = "block";
        } else {
          (block as HTMLElement).style.display = "none";
        }
      });
    });
  }

  const refreshButton = modalContainer.querySelector("#refresh-ipv4-usage") as HTMLButtonElement | null;
  if (refreshButton) {
    refreshButton.addEventListener("click", async () => {
      refreshButton.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span> 刷新中...';
      refreshButton.disabled = true;

      try {
        const refreshIpResult = await apiGet("/api/resources/ip?page_size=1000");
        if (refreshIpResult.success) {
          const allIps = (refreshIpResult.data as { data?: IpRecord[] }).data || refreshIpResult.data as IpRecord[] || [];
          const isIPv6 = (ip: IpRecord) => ip.ip_address.includes(":");
          const refreshedNetworkIps = (allIps as IpRecord[]).filter((ip: IpRecord) => ip.network_id === networkId && !isIPv6(ip));

          const newIpStatusMap = new Map<string, string>();
          refreshedNetworkIps.forEach(ip => {
            newIpStatusMap.set(ip.ip_address, ip.status || "inactive");
          });

          if (ipGrid) {
            const ipBlocks = ipGrid.querySelectorAll(".ip-block");
            ipBlocks.forEach(block => {
              const ip = (block as HTMLElement).dataset.ip!;
              const isUsed = newIpStatusMap.has(ip);
              const status = newIpStatusMap.get(ip) || "unused";
              const statusClass = isUsed ? (status === "active" ? "ip-used-active" : "ip-used-inactive") : "ip-unused";
              const tooltipText = `${ip} (${isUsed ? status === "active" ? "活跃" : "非活跃" : "未使用"})`;

              block.className = `ip-block ${statusClass}`;
              (block as HTMLElement).dataset.status = isUsed ? status : "unused";
              block.setAttribute("title", tooltipText);
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
            `).join("") : '<tr><td colspan="5" class="text-center">暂无IPv4地址记录</td></tr>';
          }

          showToast("IPv4使用情况已更新", "success");
        }
      } catch (error) {
        console.error("刷新IPv4使用情况失败:", error);
        showToast("刷新失败，请重试", "error");
      } finally {
        refreshButton.innerHTML = "刷新";
        refreshButton.disabled = false;
      }
    });
  }
}

function bindIPv6Events(modalContainer: HTMLElement, networkId: string | number): void {
  const refreshButton = modalContainer.querySelector("#refresh-ipv6-usage") as HTMLButtonElement | null;
  const ipListBody = modalContainer.querySelector("#ipv6-list-body");

  if (refreshButton) {
    refreshButton.addEventListener("click", async () => {
      refreshButton.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span> 刷新中...';
      refreshButton.disabled = true;

      try {
        const refreshIpResult = await apiGet("/api/resources/ip?page_size=1000");
        if (refreshIpResult.success) {
          const allIps = (refreshIpResult.data as { data?: IpRecord[] }).data || refreshIpResult.data as IpRecord[] || [];
          const isIPv6 = (ip: IpRecord) => ip.ip_address.includes(":");
          const refreshedNetworkIps = (allIps as IpRecord[]).filter((ip: IpRecord) => ip.network_id === networkId && isIPv6(ip));

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
            `).join("") : '<tr><td colspan="5" class="text-center">暂无IPv6地址记录</td></tr>';
          }

          const activeIps = refreshedNetworkIps.filter(ip => ip.status === "active").length;
          const inactiveIps = refreshedNetworkIps.filter(ip => ip.status !== "active").length;

          const statCards = modalContainer.querySelectorAll(".ipv6-stat-card");
          if (statCards.length >= 3) {
            statCards[0].querySelector(".stat-number")!.textContent = String(refreshedNetworkIps.length);
            statCards[1].querySelector(".stat-number")!.textContent = String(activeIps);
            statCards[2].querySelector(".stat-number")!.textContent = String(inactiveIps);
          }

          showToast("IPv6使用情况已更新", "success");
        }
      } catch (error) {
        console.error("刷新IPv6使用情况失败:", error);
        showToast("刷新失败，请重试", "error");
      } finally {
        refreshButton.innerHTML = "刷新";
        refreshButton.disabled = false;
      }
    });
  }
}

export async function editNetwork(id: string | number): Promise<void> {
  try {
    const network = await networkManager.get(id);
    if (network) {
      openNetworkModal(network as unknown as Record<string, unknown>);
    }
  } catch (error) {
    handleError(error);
  }
}

export async function deleteNetwork(id: string | number): Promise<void> {
  const result = await networkManager.delete(id, { confirmMessage: "确定要删除这个网络吗？" });
  if (result.success) {
    await loadNetworksData();
  }
}

export async function editNetworkType(id: string | number): Promise<void> {
  try {
    const networkType = await networkRegionManager.get(id);
    if (networkType) {
      openNetworkTypeModal(networkType as unknown as Record<string, unknown>);
    }
  } catch (error) {
    handleError(error);
  }
}

export async function deleteNetworkType(id: string | number): Promise<void> {
  const result = await networkRegionManager.delete(id, { confirmMessage: "确定要删除这个网络区域吗？" });
  if (result.success) {
    await loadNetworkTypesData();
    await loadNetworkTypeOptions();
  }
}

export async function submitNetworkTypeForm(): Promise<boolean | void> {
  const form = document.getElementById("network-type-form") as HTMLFormElement | null;
  if (!form) return;

  const formData = new FormData(form);
  const id = formData.get("network-type-id") as string;
  const name = formData.get("name") as string;
  const description = formData.get("description") as string;

  if (!name || !name.trim()) {
    showToast("网络区域名称不能为空", "warning");
    return;
  }

  const networkTypeData = {
    name: name.trim(),
    description: description?.trim() || null,
  };

  try {
    let result;
    if (id) {
      result = await networkRegionManager.update(id, networkTypeData);
    } else {
      result = await networkRegionManager.create(networkTypeData);
    }

    if (result.success) {
      closeModal("network-type-modal");
      await loadNetworkTypesData();
      await loadNetworkTypeOptions();
      return true;
    }
  } catch (error) {
    handleError(error, "保存网络区域数据失败");
  }
}

export async function submitNetworkForm(): Promise<boolean | void> {
  const form = document.getElementById("network-form") as HTMLFormElement | null;
  if (!form) return;

  const formData = new FormData(form);
  const id = formData.get("network-id") as string;
  const name = formData.get("name") as string;
  const networkRegionId = formData.get("network_type") as string;
  const ipv4Cidr = formData.get("ipv4_cidr") as string;
  const ipv6Cidr = formData.get("ipv6_cidr") as string;
  const ipv4Gateway = formData.get("ipv4_gateway") as string;
  const ipv6Gateway = formData.get("ipv6_gateway") as string;
  const ipv4DnsStr = formData.get("ipv4_dns") as string;
  const ipv6DnsStr = formData.get("ipv6_dns") as string;
  const description = formData.get("description") as string;

  if (!ipv4Cidr && !ipv6Cidr) {
    showToast("至少需要提供一个有效的IPv4或IPv6 CIDR", "warning");
    return;
  }

  const parseDnsList = (dnsStr: string): string[] | null => {
    if (!dnsStr || !dnsStr.trim()) return null;
    const dnsList = dnsStr.split(/[,\s]+/).map(dns => dns.trim()).filter(dns => dns.length > 0);
    if (dnsList.length > 5) {
      showToast("DNS服务器数量不能超过5个", "warning");
      return null;
    }
    return dnsList;
  };

  const ipv4Dns = parseDnsList(ipv4DnsStr);
  const ipv6Dns = parseDnsList(ipv6DnsStr);

  if (ipv4Dns === null && ipv4DnsStr && ipv4DnsStr.trim()) return;
  if (ipv6Dns === null && ipv6DnsStr && ipv6DnsStr.trim()) return;

  const networkData = {
    name: name.trim(),
    network_region_id: networkRegionId,
    ipv4_cidr: ipv4Cidr.trim() || null,
    ipv6_cidr: ipv6Cidr.trim() || null,
    ipv4_gateway: ipv4Gateway.trim() || null,
    ipv6_gateway: ipv6Gateway.trim() || null,
    ipv4_dns: ipv4Dns,
    ipv6_dns: ipv6Dns,
    description: description.trim() || null,
  };

  try {
    let result;
    if (id) {
      result = await networkManager.update(id, networkData);
    } else {
      result = await networkManager.create(networkData);
    }

    if (result.success) {
      closeModal("network-modal");
      await loadNetworksData();
      return true;
    }
  } catch (error) {
    handleError(error, "保存网络数据失败");
  }
}

export function openNetworkTypeModal(networkType: Record<string, unknown> | null = null): void {
  openModal("network-type-modal");

  const title = elementCache.get("network-type-modal-title");
  const form = elementCache.get("network-type-form") as HTMLFormElement | null;

  if (networkType) {
    if (title) title.textContent = "编辑网络区域";
    elementCache.setValue("network-type-id", String(networkType.id));
    elementCache.setValue("network-type-name", networkType.name as string);
    elementCache.setValue("network-type-description", (networkType.description as string) || "");
  } else {
    if (title) title.textContent = "添加网络区域";
    form?.reset();
    elementCache.setValue("network-type-id", "");
  }
}

export async function openNetworkModal(network: Record<string, unknown> | null = null): Promise<void> {
  openModal("network-modal");

  const title = elementCache.get("network-modal-title");
  const form = elementCache.get("network-form") as HTMLFormElement | null;

  await loadNetworkTypeOptions();

  if (network) {
    if (title) title.textContent = "编辑网络";
    elementCache.setValue("network-id", String(network.id));
    elementCache.setValue("network-name", network.name as string);
    elementCache.setValue("network-type", String(network.network_region_id));
    elementCache.setValue("network-ipv4-cidr", (network.ipv4_cidr as string) || "");
    elementCache.setValue("network-ipv6-cidr", (network.ipv6_cidr as string) || "");
    elementCache.setValue("network-ipv4-gateway", (network.ipv4_gateway as string) || "");
    elementCache.setValue("network-ipv6-gateway", (network.ipv6_gateway as string) || "");
    elementCache.setValue("network-ipv4-dns", Array.isArray(network.ipv4_dns) ? (network.ipv4_dns as string[]).join(", ") : (network.ipv4_dns as string) || "");
    elementCache.setValue("network-ipv6-dns", Array.isArray(network.ipv6_dns) ? (network.ipv6_dns as string[]).join(", ") : (network.ipv6_dns as string) || "");
    elementCache.setValue("network-description", (network.description as string) || "");
  } else {
    if (title) title.textContent = "添加网络";
    form?.reset();
    elementCache.setValue("network-id", "");
  }
}
