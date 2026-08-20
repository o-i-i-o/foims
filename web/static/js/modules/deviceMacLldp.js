// ==================== MAC表 / LLDP 功能 ====================

import { apiGet, apiPost } from "../utils/apiClient.js";

import { elementCache } from "../utils/helpers.js";
import { escapeHtml, showToast } from "../utils/ui.js";
import { t, updatePageTranslations } from "../utils/i18n.js";
import { fetchModalHtml } from "../utils/modalLoader.js";


// 从独立模板文件创建一次性模态框（ARP / LLDP），创建后由调用方自行关闭
async function createModalFromTemplate(modalId) {
  const html = await fetchModalHtml(modalId);
  if (!html) {
    return null;
  }
  const wrap = document.createElement("div");
  wrap.innerHTML = html;
  const modal = wrap.firstElementChild;
  document.body.appendChild(modal);
  modal.classList.add("active");
  updatePageTranslations();
  return modal;
}

// ==================== MAC 表函数 ====================

async function viewArpTable(deviceId) {
  const modal = await createModalFromTemplate("arp-modal");
  if (!modal) {
    showToast(t("common.load_failed"), "error");
    return;
  }

  const closeButtons = modal.querySelectorAll(".arp-modal-close");
  closeButtons.forEach((btn) => {
    btn.addEventListener("click", () => modal.remove());
  });

  modal.addEventListener("click", (e) => {
    if (e.target === modal) modal.remove();
  });

  let isFirstLoad = true;
  let ipv4SearchHandler = null;
  let ipv6SearchHandler = null;

  const loadArpData = async () => {
    const loadingEl = modal.querySelector("#arp-loading");
    const contentEl = modal.querySelector("#arp-content");
    const nameEl = modal.querySelector("#arp-device-name");
    const syncBtn = modal.querySelector("#sync-mac-btn");

    loadingEl.style.display = "block";
    contentEl.style.display = "none";
    syncBtn.disabled = true;

    try {
      const deviceResult = await apiGet(`/api/resources/devices/${deviceId}`);
      if (!deviceResult.success) {
        loadingEl.innerHTML = `<p style="color: red;">${t("device.load_failed")}</p>`;
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
            <p style="color: #666; margin-bottom: 10px;">${t("device.no_mac_data")}</p>
            <button class="btn btn-primary" id="sync-mac-empty-btn">${t("device.sync_from_snmp")}</button>
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
          const tabContents = modal.querySelectorAll(".tab-content");
          tabBtns.forEach((btn) => {
            btn.addEventListener("click", () => {
              tabBtns.forEach((b) => b.classList.remove("active"));
              btn.classList.add("active");
              tabContents.forEach((c) => c.classList.remove("active"));
              const targetTab = modal.querySelector("#" + btn.dataset.tab + "-tab");
              if (targetTab) targetTab.classList.add("active");
            });
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

          ipv4Search.addEventListener("input", ipv4SearchHandler);
          ipv6Search.addEventListener("input", ipv6SearchHandler);

          syncBtn.addEventListener("click", syncMacData);

          isFirstLoad = false;
        }

        loadingEl.style.display = "none";
        contentEl.style.display = "block";
      } else {
        loadingEl.innerHTML = `<p style="color: red;">${t("device.load_mac_failed")}: ${escapeHtml(result.message || "")}</p>`;
      }
    } catch (err) {
      loadingEl.innerHTML = `<p style="color: red;">${t("common.load_failed")}: ${escapeHtml(err.message)}</p>`;
    } finally {
      syncBtn.disabled = false;
    }
  };

  const syncMacData = async () => {
    const loadingEl = modal.querySelector("#arp-loading");
    const contentEl = modal.querySelector("#arp-content");
    const syncBtn = modal.querySelector("#sync-mac-btn");

    loadingEl.style.display = "block";
    contentEl.style.display = "none";
    syncBtn.disabled = true;
    loadingEl.innerHTML = `
      <div class="spinner"></div>
      <p style="margin-top: 10px; color: #666;">${t("device.syncing_mac")}</p>
    `;

    try {
      const result = await apiPost(`/api/resources/devices/${deviceId}/macs/sync`, {});
      if (result.success) {
        await loadArpData();
      } else {
        loadingEl.innerHTML = `<p style="color: red;">${t("device.sync_failed")}: ${escapeHtml(result.message || "")}</p>`;
      }
    } catch (err) {
      loadingEl.innerHTML = `<p style="color: red;">${t("device.sync_failed")}: ${escapeHtml(err.message)}</p>`;
    } finally {
      syncBtn.disabled = false;
    }
  };

  await loadArpData();
}

function renderMacTable(entries, type) {
  if (entries.length === 0) {
    return `<p style="text-align: center; color: #666; padding: 20px;">${type === "ipv4" ? "IPv4" : "IPv6"} ${t("common.no_data")}</p>`;
  }

  const groups = groupByNetwork(entries, type);
  const defaultCollapsed = true;

  let html = "";
  let groupIndex = 0;
  for (const [network, items] of Object.entries(groups)) {
    const groupId = `${type}-group-${groupIndex}`;
    const displayStyle = defaultCollapsed ? "none" : "block";
    const iconRotate = defaultCollapsed ? "rotate(-90deg)" : "rotate(0deg)";

    html += `<div style="margin-bottom: 10px;">
      <div class="network-group-header" data-target="${groupId}" style="background: #f5f5f5; padding: 8px 12px; font-weight: bold; border-left: 3px solid #4CAF50; cursor: pointer; display: flex; justify-content: space-between; align-items: center; user-select: none;">
        <span>${escapeHtml(network)} (${items.length} ${t("common.records")})</span>
        <span class="collapse-icon" style="transition: transform 0.2s; transform: ${iconRotate};">▼</span>
      </div>
      <div id="${groupId}" class="network-group-content" style="display: ${displayStyle};">
        <table style="width:100%; border-collapse: collapse;">
          <tr><th style="border:1px solid #ddd; padding:6px; text-align:left; background:#fafafa;">${t("device.ip_address")}</th><th style="border:1px solid #ddd; padding:6px; text-align:left; background:#fafafa;">${t("device.mac_address")}</th></tr>`;
    items.forEach((entry) => {
      html += `<tr><td style="border:1px solid #ddd; padding:6px;">${escapeHtml(entry.ip_address)}</td><td style="border:1px solid #ddd; padding:6px;">${escapeHtml(entry.mac_address)}</td></tr>`;
    });
    html += `</table></div></div>`;
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

      if (content.style.display === "none") {
        content.style.display = "block";
        icon.style.transform = "rotate(0deg)";
      } else {
        content.style.display = "none";
        icon.style.transform = "rotate(-90deg)";
      }
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
            if (aParts[i] !== bParts[i]) return aParts[i] - bParts[i];
          }
          return 0;
        }
        return a.ip_address.localeCompare(b.ip_address);
      });
    });

  return sortedGroups;
}

function filterEntries(entries, searchTerm) {
  if (!searchTerm) return entries;
  const term = searchTerm.toLowerCase();
  return entries.filter(
    (e) => e.ip_address.toLowerCase().includes(term) || e.mac_address.toLowerCase().includes(term)
  );
}

// ==================== LLDP 函数 ====================

async function viewLldpNeighbors(deviceId) {
  const modal = await createModalFromTemplate("lldp-modal");
  if (!modal) {
    showToast(t("common.load_failed"), "error");
    return;
  }

  const closeButtons = modal.querySelectorAll(".lldp-modal-close");
  closeButtons.forEach((btn) => {
    btn.addEventListener("click", () => modal.remove());
  });

  modal.addEventListener("click", (e) => {
    if (e.target === modal) modal.remove();
  });

  const loadLldpData = async () => {
    const loadingEl = modal.querySelector("#lldp-loading");
    const contentEl = modal.querySelector("#lldp-content");
    const nameEl = modal.querySelector("#lldp-device-name");
    const syncBtn = modal.querySelector("#sync-lldp-btn");

    loadingEl.style.display = "block";
    contentEl.style.display = "none";
    syncBtn.disabled = true;

    try {
      const deviceResult = await apiGet(`/api/resources/devices/${deviceId}`);
      if (!deviceResult.success) {
        loadingEl.innerHTML = `<p style="color: red;">${t("device.load_failed")}</p>`;
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
            <p style="color: #666; margin-bottom: 10px;">${t("device.no_lldp_data")}</p>
            <button class="btn btn-primary" id="sync-lldp-empty-btn">${t("device.sync_from_snmp")}</button>
          `;
          modal
            .querySelector("#sync-lldp-empty-btn")
            .addEventListener("click", () => syncLldpData());
          return;
        }

        contentEl.innerHTML = renderLldpTable(neighbors);
        loadingEl.style.display = "none";
        contentEl.style.display = "block";
      } else {
        loadingEl.innerHTML = `<p style="color: red;">${t("device.load_lldp_failed")}: ${escapeHtml(result.message || "")}</p>`;
      }
    } catch (err) {
      loadingEl.innerHTML = `<p style="color: red;">${t("common.load_failed")}: ${escapeHtml(err.message)}</p>`;
    } finally {
      syncBtn.disabled = false;
    }
  };

  const syncLldpData = async () => {
    const loadingEl = modal.querySelector("#lldp-loading");
    const contentEl = modal.querySelector("#lldp-content");
    const syncBtn = modal.querySelector("#sync-lldp-btn");

    loadingEl.style.display = "block";
    contentEl.style.display = "none";
    syncBtn.disabled = true;
    loadingEl.innerHTML = `
      <div class="spinner"></div>
      <p style="margin-top: 10px; color: #666;">${t("device.syncing_lldp")}</p>
    `;

    try {
      const result = await apiPost(`/api/resources/devices/${deviceId}/lldp/sync`, {});
      if (result.success) {
        await loadLldpData();
      } else {
        loadingEl.innerHTML = `<p style="color: red;">${t("device.sync_failed")}: ${escapeHtml(result.message || "")}</p>`;
      }
    } catch (err) {
      loadingEl.innerHTML = `<p style="color: red;">${t("device.sync_failed")}: ${escapeHtml(err.message)}</p>`;
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
    <table class="table table-bordered" style="border-collapse: collapse; width: 100%; margin-top: 10px;">
      <thead>
        <tr style="background-color: var(--bg-secondary, #f0f2f5);">
          <th style="width: 60px; text-align: center; border: 1px solid var(--border-color, #ddd); padding: 12px 8px; font-weight: 600;">${t("common.index")}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("device.local_port")}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("device.neighbor_device")}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("device.neighbor_port")}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">Chassis ID</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("device.system_description")}</th>
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
          <tr style="background-color: ${i % 2 === 0 ? "var(--bg-primary, #fff)" : "var(--bg-tertiary, #fafbfc)"};">
            <td style="text-align: center; color: var(--text-muted, #888); border: 1px solid var(--border-color, #ddd); padding: 10px 8px;">${i + 1}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px; font-weight: 500;">${escapeHtml(n.local_port || "-")}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px;">${escapeHtml(n.neighbor_sys_name || "-")}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px;">${escapeHtml(neighborPort)}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px; font-family: monospace; font-size: 0.9em;">${escapeHtml(n.neighbor_chassis_id || "-")}</td>
            <td style="max-width: 250px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border: 1px solid var(--border-color, #ddd); padding: 10px; font-size: 0.9em; color: var(--text-secondary, #666);" title="${escapeHtml(n.neighbor_sys_desc || "")}">${escapeHtml(n.neighbor_sys_desc || "-")}</td>
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
