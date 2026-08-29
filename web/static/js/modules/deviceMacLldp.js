// ==================== MAC表 / LLDP 功能 ====================

import { apiGet, apiPost } from "../utils/apiClient.js";

import { escapeHtml, showToast } from "../utils/ui.js";
import { t } from "../utils/i18n.js";
import { iconButton } from "../utils/icons.js";
import { openModal, closeModal } from "../utils/modalLoader.js";

// IPv4/IPv6 标签页互斥切换：激活当前按钮与其对应内容面板
function activateTabPane(modal, btn) {
  // 同步 aria-selected（与 active 类一致，参考 login.js 的 Tab 切换写法）
  modal.querySelectorAll(".tab-btn").forEach((b) => {
    b.classList.remove("active");
    b.setAttribute("aria-selected", "false");
  });
  btn.classList.add("active");
  btn.setAttribute("aria-selected", "true");
  modal.querySelectorAll(".tab-content").forEach((c) => c.classList.remove("active"));
  const targetTab = modal.querySelector(`#${btn.dataset.tab}-tab`);
  if (targetTab) {
    targetTab.classList.add("active");
  }
}

// ==================== MAC 表函数 ====================

async function viewArpTable(deviceId) {
  const modal = await openModal("arp-modal");
  if (!modal) {
    showToast(t("common.load_failed"), "error");
    return;
  }

  modal.addEventListener("click", (e) => {
    if (e.target === modal) {
      closeModal("arp-modal");
    }
  });

  let isFirstLoad = true;
  let ipv4SearchHandler = null;
  let ipv6SearchHandler = null;

  const loadArpData = async () => {
    const loadingEl = modal.querySelector("#arp-loading");
    const contentEl = modal.querySelector("#arp-content");
    const nameEl = modal.querySelector("#arp-device-name");
    const syncBtn = modal.querySelector("#sync-mac-btn");

    loadingEl.classList.remove("hidden");
    contentEl.classList.add("hidden");
    syncBtn.disabled = true;

    try {
      const deviceResult = await apiGet(`/api/resources/devices/${deviceId}`);
      if (!deviceResult.success) {
        loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("device.load_failed")}</p>`;
        return;
      }

      const deviceData = deviceResult.data;
      nameEl.textContent = `- ${deviceData.name}`;

      const result = await apiGet(`/api/resources/devices/${deviceId}/macs`);

      if (result.success) {
        const entries = result.data || [];
        nameEl.textContent = `- ${deviceData.name} (${entries.length} ${t("common.records")})`;

        if (entries.length === 0) {
          loadingEl.innerHTML = `
            <p class="mac-lldp-text-muted">${t("device.no_mac_data")}</p>
            ${iconButton({ icon: "refresh", label: t("device.sync_from_snmp"), cls: "btn-primary", attrs: 'id="sync-mac-empty-btn"' })}
          `;
          modal.querySelector("#sync-mac-empty-btn").addEventListener("click", () => syncMacData());
          return;
        }

        const ipv4Entries = entries.filter((e) => e.ip_address && e.ip_address.includes("."));
        const ipv6Entries = entries.filter((e) => e.ip_address && e.ip_address.includes(":"));

        modal.querySelector('[data-tab="ipv4"]').textContent = `IPv4 (${ipv4Entries.length})`;
        modal.querySelector('[data-tab="ipv6"]').textContent = `IPv6 (${ipv6Entries.length})`;

        modal.querySelector("#ipv4-table-container").innerHTML = renderMacTable(
          ipv4Entries,
          "ipv4"
        );
        modal.querySelector("#ipv6-table-container").innerHTML = renderMacTable(
          ipv6Entries,
          "ipv6"
        );

        bindCollapseEvents(modal.querySelector("#ipv4-table-container"));
        bindCollapseEvents(modal.querySelector("#ipv6-table-container"));

        if (isFirstLoad) {
          const tabBtns = modal.querySelectorAll(".tab-btn");
          tabBtns.forEach((btn) => {
            btn.addEventListener("click", () => activateTabPane(modal, btn));
          });

          const ipv4Search = modal.querySelector("#ipv4-search");
          const ipv6Search = modal.querySelector("#ipv6-search");

          ipv4SearchHandler = () => {
            const filtered = filterEntries(ipv4Entries, ipv4Search.value);
            const container = modal.querySelector("#ipv4-table-container");
            container.innerHTML = renderMacTable(filtered, "ipv4");
            bindCollapseEvents(container);
          };

          ipv6SearchHandler = () => {
            const filtered = filterEntries(ipv6Entries, ipv6Search.value);
            const container = modal.querySelector("#ipv6-table-container");
            container.innerHTML = renderMacTable(filtered, "ipv6");
            bindCollapseEvents(container);
          };

          // MAC 表可达数千行，搜索按 300ms 防抖，避免每次按键全量重建两表
          const debounceInput = (handler) => {
            let timer = null;
            return () => {
              clearTimeout(timer);
              timer = setTimeout(handler, 300);
            };
          };

          ipv4Search.addEventListener("input", debounceInput(ipv4SearchHandler));
          ipv6Search.addEventListener("input", debounceInput(ipv6SearchHandler));

          syncBtn.addEventListener("click", syncMacData);

          isFirstLoad = false;
        }

        loadingEl.classList.add("hidden");
        contentEl.classList.remove("hidden");
      } else {
        loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("device.load_mac_failed")}: ${escapeHtml(result.message || "")}</p>`;
      }
    } catch (err) {
      loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("common.load_failed")}: ${escapeHtml(err.message)}</p>`;
    } finally {
      syncBtn.disabled = false;
    }
  };

  const syncMacData = async () => {
    const loadingEl = modal.querySelector("#arp-loading");
    const contentEl = modal.querySelector("#arp-content");
    const syncBtn = modal.querySelector("#sync-mac-btn");

    loadingEl.classList.remove("hidden");
    contentEl.classList.add("hidden");
    syncBtn.disabled = true;
    loadingEl.innerHTML = `
      <div class="spinner"></div>
      <p class="mac-lldp-text-syncing">${t("device.syncing_mac")}</p>
    `;

    try {
      const result = await apiPost(`/api/resources/devices/${deviceId}/macs/sync`, {});
      if (result.success) {
        await loadArpData();
      } else {
        loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("device.sync_failed")}: ${escapeHtml(result.message || "")}</p>`;
      }
    } catch (err) {
      loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("device.sync_failed")}: ${escapeHtml(err.message)}</p>`;
    } finally {
      syncBtn.disabled = false;
    }
  };

  await loadArpData();
}

function renderMacTable(entries, type) {
  if (entries.length === 0) {
    return `<p class="mac-table-empty">${type === "ipv4" ? "IPv4" : "IPv6"} ${t("common.no_data")}</p>`;
  }

  const groups = groupByNetwork(entries, type);

  let html = "";
  let groupIndex = 0;
  for (const [network, items] of Object.entries(groups)) {
    const groupId = `${type}-group-${groupIndex}`;
    // 初始折叠（内联 display:none），展开/折叠由点击处理切换同一内联样式
    html += `<div class="mac-network-group">
      <div class="network-group-header" data-target="${groupId}">
        <span>${escapeHtml(network)} (${items.length} ${t("common.records")})</span>
        <span class="collapse-icon mac-collapse-icon" style="transform: rotate(-90deg);">▼</span>
      </div>
      <div id="${groupId}" class="network-group-content" style="display: none;">
        <table class="mac-group-table">
          <tr><th>${t("device.ip_address")}</th><th>${t("device.mac_address")}</th></tr>`;
    items.forEach((entry) => {
      html += `<tr><td>${escapeHtml(entry.ip_address)}</td><td>${escapeHtml(entry.mac_address)}</td></tr>`;
    });
    html += "</table></div></div>";
    groupIndex++;
  }

  return html;
}

function bindCollapseEvents(container) {
  container.querySelectorAll(".network-group-header").forEach((header) => {
    header.addEventListener("click", () => {
      const targetId = header.dataset.target;
      const content = container.querySelector(`#${targetId}`);
      const icon = header.querySelector(".collapse-icon");
      if (!content || !icon) {
        return;
      }
      // 与初始态同一机制（内联 display）：初始 display:none 为折叠，
      // toggle hidden 类不会改变内联样式，明细永远不可见
      const collapsed = content.style.display === "none";
      content.style.display = collapsed ? "block" : "none";
      icon.style.transform = collapsed ? "rotate(0deg)" : "rotate(-90deg)";
    });
  });
}

function groupByNetwork(entries, type) {
  const groups = {};

  entries.forEach((entry) => {
    let network;
    if (type === "ipv4") {
      const parts = entry.ip_address.split(".");
      network =
        parts.length >= 3
          ? `${parts[0]}.${parts[1]}.${parts[2]}.0/24`
          : t("device.unknown_network");
    } else {
      const parts = entry.ip_address.split(":");
      if (parts.length >= 2) {
        if (entry.ip_address.startsWith("fe80")) {
          network = `fe80::/10 (${t("device.link_local")})`;
        } else if (entry.ip_address.startsWith("fd") || entry.ip_address.startsWith("fc")) {
          network = `${parts[0]}::/16 (ULA)`;
        } else if (parts[0] === "240e" || parts[0] === "2409" || parts[0] === "2408") {
          const prefix = parts.slice(0, 4).join(":");
          network = `${prefix}::/64 (${t("device.public_network")})`;
        } else {
          const prefix = parts.slice(0, 4).join(":");
          network = `${prefix}::/64`;
        }
      } else {
        network = t("device.unknown_network");
      }
    }

    if (!groups[network]) {
      groups[network] = [];
    }
    groups[network].push(entry);
  });

  const sortedGroups = {};
  Object.keys(groups)
    .sort()
    .forEach((key) => {
      sortedGroups[key] = groups[key].sort((a, b) => {
        if (type === "ipv4") {
          const aParts = a.ip_address.split(".").map(Number);
          const bParts = b.ip_address.split(".").map(Number);
          for (let i = 0; i < 4; i++) {
            if (aParts[i] !== bParts[i]) {
              return aParts[i] - bParts[i];
            }
          }
          return 0;
        }
        return a.ip_address.localeCompare(b.ip_address);
      });
    });

  return sortedGroups;
}

function filterEntries(entries, searchTerm) {
  if (!searchTerm) {
    return entries;
  }
  const term = searchTerm.toLowerCase();
  return entries.filter(
    (e) => e.ip_address.toLowerCase().includes(term) || e.mac_address.toLowerCase().includes(term)
  );
}

// ==================== LLDP 函数 ====================

async function viewLldpNeighbors(deviceId) {
  const modal = await openModal("lldp-modal");
  if (!modal) {
    showToast(t("common.load_failed"), "error");
    return;
  }

  modal.addEventListener("click", (e) => {
    if (e.target === modal) {
      closeModal("lldp-modal");
    }
  });

  const loadLldpData = async () => {
    const loadingEl = modal.querySelector("#lldp-loading");
    const contentEl = modal.querySelector("#lldp-content");
    const nameEl = modal.querySelector("#lldp-device-name");
    const syncBtn = modal.querySelector("#sync-lldp-btn");

    loadingEl.classList.remove("hidden");
    contentEl.classList.add("hidden");
    syncBtn.disabled = true;

    try {
      const deviceResult = await apiGet(`/api/resources/devices/${deviceId}`);
      if (!deviceResult.success) {
        loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("device.load_failed")}</p>`;
        return;
      }

      const deviceData = deviceResult.data;
      nameEl.textContent = `- ${deviceData.name}`;

      const result = await apiGet(`/api/resources/devices/${deviceId}/lldp-neighbors`);

      if (result.success) {
        const neighbors = result.data || [];
        nameEl.textContent = `- ${deviceData.name} (${neighbors.length} ${t("common.records")})`;

        if (neighbors.length === 0) {
          loadingEl.innerHTML = `
            <p class="mac-lldp-text-muted">${t("device.no_lldp_data")}</p>
            ${iconButton({ icon: "refresh", label: t("device.sync_from_snmp"), cls: "btn-primary", attrs: 'id="sync-lldp-empty-btn"' })}
          `;
          modal
            .querySelector("#sync-lldp-empty-btn")
            .addEventListener("click", () => syncLldpData());
          return;
        }

        contentEl.innerHTML = renderLldpTable(neighbors);
        loadingEl.classList.add("hidden");
        contentEl.classList.remove("hidden");
      } else {
        loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("device.load_lldp_failed")}: ${escapeHtml(result.message || "")}</p>`;
      }
    } catch (err) {
      loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("common.load_failed")}: ${escapeHtml(err.message)}</p>`;
    } finally {
      syncBtn.disabled = false;
    }
  };

  const syncLldpData = async () => {
    const loadingEl = modal.querySelector("#lldp-loading");
    const contentEl = modal.querySelector("#lldp-content");
    const syncBtn = modal.querySelector("#sync-lldp-btn");

    loadingEl.classList.remove("hidden");
    contentEl.classList.add("hidden");
    syncBtn.disabled = true;
    loadingEl.innerHTML = `
      <div class="spinner"></div>
      <p class="mac-lldp-text-syncing">${t("device.syncing_lldp")}</p>
    `;

    try {
      const result = await apiPost(`/api/resources/devices/${deviceId}/lldp/sync`, {});
      if (result.success) {
        await loadLldpData();
      } else {
        loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("device.sync_failed")}: ${escapeHtml(result.message || "")}</p>`;
      }
    } catch (err) {
      loadingEl.innerHTML = `<p class="mac-lldp-text-error">${t("device.sync_failed")}: ${escapeHtml(err.message)}</p>`;
    } finally {
      syncBtn.disabled = false;
    }
  };

  const syncBtn = modal.querySelector("#sync-lldp-btn");
  syncBtn.addEventListener("click", syncLldpData);

  await loadLldpData();
}

function renderLldpTable(neighbors) {
  return `
    <table class="table table-bordered lldp-table">
      <thead>
        <tr>
          <th class="lldp-col-index">${t("common.index")}</th>
          <th>${t("device.local_port")}</th>
          <th>${t("device.neighbor_device")}</th>
          <th>${t("device.neighbor_port")}</th>
          <th>Chassis ID</th>
          <th>${t("device.system_description")}</th>
        </tr>
      </thead>
      <tbody>
        ${neighbors
          .map((n, i) => {
            const isMac = (str) => /^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/.test(str);
            let neighborPort = "-";
            if (n.neighbor_port_id && !isMac(n.neighbor_port_id)) {
              neighborPort = n.neighbor_port_id;
            } else if (n.neighbor_port_desc) {
              neighborPort = n.neighbor_port_desc;
            } else if (n.neighbor_port_id) {
              neighborPort = n.neighbor_port_id;
            }
            return `
          <tr>
            <td class="lldp-index">${i + 1}</td>
            <td class="lldp-port">${escapeHtml(n.local_port || "-")}</td>
            <td>${escapeHtml(n.neighbor_sys_name || "-")}</td>
            <td>${escapeHtml(neighborPort)}</td>
            <td class="lldp-chassis">${escapeHtml(n.neighbor_chassis_id || "-")}</td>
            <td class="lldp-desc" title="${escapeHtml(n.neighbor_sys_desc || "")}">${escapeHtml(n.neighbor_sys_desc || "-")}</td>
          </tr>
        `;
          })
          .join("")}
      </tbody>
    </table>
  `;
}

// ==================== 导出 ====================

export { viewArpTable, viewLldpNeighbors };
