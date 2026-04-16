import { apiGet, apiPost } from "../../utils/apiClient.js";
import { escapeHtml } from "../../utils/ui.js";
import { elementCache } from "../../utils/helpers.js";
import { t } from "../../utils/i18n.js";
export async function viewArpTable(switchId) {
    const modal = document.createElement("div");
    modal.className = "modal active";
    modal.id = "arp-modal-" + Date.now();
    modal.innerHTML = `
    <div class="modal-content" style="max-width: 800px;">
      <div class="modal-header">
        <h3>${t("switch.mac_table") || "MAC表"} <span id="arp-switch-name"></span></h3>
        <span class="close arp-modal-close">&times;</span>
      </div>
      <div class="modal-body" style="max-height: 500px; overflow-y: auto;">
        <div id="arp-loading" style="text-align: center; padding: 40px;">
          <div class="spinner"></div>
          <p style="margin-top: 10px; color: #666;">${t("switch.loading_mac") || "正在加载MAC表..."}</p>
        </div>
        <div id="arp-content" style="display: none;">
          <div class="tab-container">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px;">
              <div class="tab-buttons" style="display: flex; gap: 5px;">
                <button class="tab-btn active" data-tab="ipv4">IPv4</button>
                <button class="tab-btn" data-tab="ipv6">IPv6</button>
              </div>
              <button class="btn btn-sm btn-primary" id="sync-mac-btn">
                <span>${t("switch.sync_from_snmp") || "从SNMP同步"}</span>
              </button>
            </div>
            <div class="tab-content active" id="ipv4-tab">
              <div style="margin-bottom: 10px;">
                <input type="text" id="ipv4-search" placeholder="${t("switch.search_ip_mac") || "搜索IP或MAC地址..."}" style="width: 100%; padding: 8px; border: 1px solid #ccc; border-radius: 4px;">
              </div>
              <div id="ipv4-table-container" style="max-height: 350px; overflow-y: auto;"></div>
            </div>
            <div class="tab-content" id="ipv6-tab">
              <div style="margin-bottom: 10px;">
                <input type="text" id="ipv6-search" placeholder="${t("switch.search_ip_mac") || "搜索IP或MAC地址..."}" style="width: 100%; padding: 8px; border: 1px solid #ccc; border-radius: 4px;">
              </div>
              <div id="ipv6-table-container" style="max-height: 350px; overflow-y: auto;"></div>
            </div>
          </div>
        </div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary arp-modal-close">${t("common.close") || "关闭"}</button>
      </div>
    </div>
  `;
    document.body.appendChild(modal);
    const closeButtons = modal.querySelectorAll(".arp-modal-close");
    closeButtons.forEach(btn => {
        btn.addEventListener("click", () => modal.remove());
    });
    modal.addEventListener("click", (e) => {
        if (e.target === modal)
            modal.remove();
    });
    let isFirstLoad = true;
    let ipv4SearchHandler = null;
    let ipv6SearchHandler = null;
    let ipv4Entries = [];
    let ipv6Entries = [];
    const loadArpData = async () => {
        const loadingEl = modal.querySelector("#arp-loading");
        const contentEl = modal.querySelector("#arp-content");
        const nameEl = modal.querySelector("#arp-switch-name");
        const syncBtn = modal.querySelector("#sync-mac-btn");
        if (loadingEl)
            loadingEl.style.display = "block";
        if (contentEl)
            contentEl.style.display = "none";
        if (syncBtn)
            syncBtn.disabled = true;
        try {
            const switchResult = await apiGet(`/api/switches/${switchId}`);
            if (!switchResult.success) {
                if (loadingEl)
                    loadingEl.innerHTML = `<p style="color: red;">${t("switch.fetch_info_failed") || "获取交换机信息失败"}</p>`;
                return;
            }
            const switchData = switchResult.data;
            if (nameEl)
                nameEl.textContent = `- ${switchData.name}`;
            const result = await apiGet(`/api/switches/${switchId}/macs`);
            if (result.success) {
                const entries = result.data || [];
                if (nameEl)
                    nameEl.textContent = `- ${switchData.name} (${entries.length}${t("switch.entries") || "条"})`;
                if (entries.length === 0) {
                    if (loadingEl) {
                        loadingEl.innerHTML = `
              <p style="color: #666; margin-bottom: 10px;">${t("switch.no_mac_data") || "暂无MAC数据，请点击\"从SNMP同步\"按钮获取"}</p>
              <button class="btn btn-primary" id="sync-mac-empty-btn">${t("switch.sync_from_snmp") || "从SNMP同步"}</button>
            `;
                    }
                    const syncEmptyBtn = modal.querySelector("#sync-mac-empty-btn");
                    syncEmptyBtn?.addEventListener("click", () => syncMacData());
                    return;
                }
                ipv4Entries = entries.filter(e => e.ip_address && e.ip_address.includes("."));
                ipv6Entries = entries.filter(e => e.ip_address && e.ip_address.includes(":"));
                const ipv4TabBtn = modal.querySelector('[data-tab="ipv4"]');
                const ipv6TabBtn = modal.querySelector('[data-tab="ipv6"]');
                if (ipv4TabBtn)
                    ipv4TabBtn.textContent = `IPv4 (${ipv4Entries.length})`;
                if (ipv6TabBtn)
                    ipv6TabBtn.textContent = `IPv6 (${ipv6Entries.length})`;
                const ipv4Container = modal.querySelector("#ipv4-table-container");
                const ipv6Container = modal.querySelector("#ipv6-table-container");
                if (ipv4Container)
                    ipv4Container.innerHTML = renderMacTable(ipv4Entries, "ipv4");
                if (ipv6Container)
                    ipv6Container.innerHTML = renderMacTable(ipv6Entries, "ipv6");
                if (ipv4Container)
                    bindCollapseEvents(ipv4Container);
                if (ipv6Container)
                    bindCollapseEvents(ipv6Container);
                if (isFirstLoad) {
                    const tabBtns = modal.querySelectorAll(".tab-btn");
                    const tabContents = modal.querySelectorAll(".tab-content");
                    tabBtns.forEach(btn => {
                        btn.addEventListener("click", () => {
                            tabBtns.forEach(b => b.classList.remove("active"));
                            btn.classList.add("active");
                            tabContents.forEach(c => c.classList.remove("active"));
                            const targetTab = modal.querySelector(`#${btn.dataset.tab}-tab`);
                            if (targetTab)
                                targetTab.classList.add("active");
                        });
                    });
                    const ipv4Search = modal.querySelector("#ipv4-search");
                    const ipv6Search = modal.querySelector("#ipv6-search");
                    ipv4SearchHandler = () => {
                        const filtered = filterEntries(ipv4Entries, ipv4Search?.value || "");
                        const container = modal.querySelector("#ipv4-table-container");
                        if (container) {
                            container.innerHTML = renderMacTable(filtered, "ipv4");
                            bindCollapseEvents(container);
                        }
                    };
                    ipv6SearchHandler = () => {
                        const filtered = filterEntries(ipv6Entries, ipv6Search?.value || "");
                        const container = modal.querySelector("#ipv6-table-container");
                        if (container) {
                            container.innerHTML = renderMacTable(filtered, "ipv6");
                            bindCollapseEvents(container);
                        }
                    };
                    ipv4Search?.addEventListener("input", ipv4SearchHandler);
                    ipv6Search?.addEventListener("input", ipv6SearchHandler);
                    syncBtn?.addEventListener("click", syncMacData);
                    isFirstLoad = false;
                }
                if (loadingEl)
                    loadingEl.style.display = "none";
                if (contentEl)
                    contentEl.style.display = "block";
            }
            else {
                if (loadingEl)
                    loadingEl.innerHTML = `<p style="color: red;">${t("switch.load_mac_failed") || "加载MAC表失败"}: ${result.message || t("common.unknown_error") || "未知错误"}</p>`;
            }
        }
        catch (err) {
            const error = err;
            if (loadingEl)
                loadingEl.innerHTML = `<p style="color: red;">${t("common.load_failed") || "加载失败"}: ${error.message}</p>`;
        }
        finally {
            if (syncBtn)
                syncBtn.disabled = false;
        }
    };
    const syncMacData = async () => {
        const loadingEl = modal.querySelector("#arp-loading");
        const contentEl = modal.querySelector("#arp-content");
        const syncBtn = modal.querySelector("#sync-mac-btn");
        if (loadingEl)
            loadingEl.style.display = "block";
        if (contentEl)
            contentEl.style.display = "none";
        if (syncBtn)
            syncBtn.disabled = true;
        if (loadingEl) {
            loadingEl.innerHTML = `
        <div class="spinner"></div>
        <p style="margin-top: 10px; color: #666;">${t("switch.syncing_mac") || "正在从SNMP同步MAC表..."}</p>
      `;
        }
        try {
            const result = await apiPost(`/api/switches/${switchId}/macs/sync`, {});
            if (result.success) {
                await loadArpData();
            }
            else {
                if (loadingEl)
                    loadingEl.innerHTML = `<p style="color: red;">${t("switch.sync_failed") || "同步失败"}: ${result.message || t("common.unknown_error") || "未知错误"}</p>`;
            }
        }
        catch (err) {
            const error = err;
            if (loadingEl)
                loadingEl.innerHTML = `<p style="color: red;">${t("switch.sync_failed") || "同步失败"}: ${error.message}</p>`;
        }
        finally {
            if (syncBtn)
                syncBtn.disabled = false;
        }
    };
    await loadArpData();
}
export function renderMacTable(entries, type) {
    if (entries.length === 0) {
        return `<p style="text-align: center; color: #666; padding: 20px;">${t("switch.no_ip_data") || "暂无"}${type === "ipv4" ? "IPv4" : "IPv6"}${t("switch.data") || "数据"}</p>`;
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
        <span>${escapeHtml(network)} (${items.length}${t("switch.entries") || "条"})</span>
        <span class="collapse-icon" style="transition: transform 0.2s; transform: ${iconRotate};">▼</span>
      </div>
      <div id="${groupId}" class="network-group-content" style="display: ${displayStyle};">
        <table style="width:100%; border-collapse: collapse;">
          <tr><th style="border:1px solid #ddd; padding:6px; text-align:left; background:#fafafa;">${t("switch.ip_address") || "IP地址"}</th><th style="border:1px solid #ddd; padding:6px; text-align:left; background:#fafafa;">${t("switch.mac_address") || "MAC地址"}</th></tr>`;
        items.forEach((entry) => {
            html += `<tr><td style="border:1px solid #ddd; padding:6px;">${escapeHtml(entry.ip_address)}</td><td style="border:1px solid #ddd; padding:6px;">${escapeHtml(entry.mac_address)}</td></tr>`;
        });
        html += `</table></div></div>`;
        groupIndex++;
    }
    return html;
}
export function bindCollapseEvents(container) {
    container.querySelectorAll(".network-group-header").forEach(header => {
        header.addEventListener("click", () => {
            const targetId = header.dataset.target;
            const content = targetId ? container.querySelector(`#${targetId}`) : null;
            const icon = header.querySelector(".collapse-icon");
            if (content && content.style.display === "none") {
                content.style.display = "block";
                if (icon)
                    icon.style.transform = "rotate(0deg)";
            }
            else if (content) {
                content.style.display = "none";
                if (icon)
                    icon.style.transform = "rotate(-90deg)";
            }
        });
    });
}
export function groupByNetwork(entries, type) {
    const groups = {};
    entries.forEach(entry => {
        let network;
        if (type === "ipv4") {
            const parts = entry.ip_address.split(".");
            network = parts.length >= 3 ? `${parts[0]}.${parts[1]}.${parts[2]}.0/24` : t("switch.unknown_network") || "未知网段";
        }
        else {
            const parts = entry.ip_address.split(":");
            if (parts.length >= 2) {
                if (entry.ip_address.startsWith("fe80")) {
                    network = "fe80::/10 (Link-Local)";
                }
                else if (entry.ip_address.startsWith("fd") || entry.ip_address.startsWith("fc")) {
                    network = `${parts[0]}::/16 (ULA)`;
                }
                else if (parts[0] === "240e" || parts[0] === "2409" || parts[0] === "2408") {
                    const prefix = parts.slice(0, 4).join(":");
                    network = `${prefix}::/64 (Public)`;
                }
                else {
                    const prefix = parts.slice(0, 4).join(":");
                    network = `${prefix}::/64`;
                }
            }
            else {
                network = t("switch.unknown_network") || "未知网段";
            }
        }
        if (!groups[network]) {
            groups[network] = [];
        }
        groups[network].push(entry);
    });
    const sortedGroups = {};
    Object.keys(groups).sort().forEach(key => {
        sortedGroups[key] = groups[key].sort((a, b) => {
            if (type === "ipv4") {
                const aParts = a.ip_address.split(".").map(Number);
                const bParts = b.ip_address.split(".").map(Number);
                for (let i = 0; i < 4; i++) {
                    if (aParts[i] !== bParts[i])
                        return aParts[i] - bParts[i];
                }
                return 0;
            }
            return a.ip_address.localeCompare(b.ip_address);
        });
    });
    return sortedGroups;
}
export function filterEntries(entries, searchTerm) {
    if (!searchTerm)
        return entries;
    const term = searchTerm.toLowerCase();
    return entries.filter(e => e.ip_address.toLowerCase().includes(term) ||
        e.mac_address.toLowerCase().includes(term));
}
export async function viewLldpNeighbors(switchId) {
    const modal = document.createElement("div");
    modal.className = "modal active";
    modal.id = "lldp-modal-" + Date.now();
    modal.innerHTML = `
    <div class="modal-content" style="max-width: 900px;">
      <div class="modal-header">
        <h3>${t("switch.lldp_neighbors") || "LLDP邻居信息"} <span id="lldp-switch-name"></span></h3>
        <span class="close lldp-modal-close">&times;</span>
      </div>
      <div class="modal-body" style="max-height: 500px; overflow-y: auto;">
        <div id="lldp-loading" style="text-align: center; padding: 40px;">
          <div class="spinner"></div>
          <p style="margin-top: 10px; color: #666;">${t("switch.loading_lldp") || "正在加载LLDP邻居信息..."}</p>
        </div>
        <div id="lldp-content" style="display: none;"></div>
      </div>
      <div class="modal-footer">
        <button class="btn btn-primary" id="sync-lldp-btn">${t("switch.sync_from_snmp") || "从SNMP同步"}</button>
        <button class="btn btn-secondary lldp-modal-close">${t("common.close") || "关闭"}</button>
      </div>
    </div>
  `;
    document.body.appendChild(modal);
    const closeButtons = modal.querySelectorAll(".lldp-modal-close");
    closeButtons.forEach(btn => {
        btn.addEventListener("click", () => modal.remove());
    });
    modal.addEventListener("click", (e) => {
        if (e.target === modal)
            modal.remove();
    });
    const loadLldpData = async () => {
        const loadingEl = modal.querySelector("#lldp-loading");
        const contentEl = modal.querySelector("#lldp-content");
        const nameEl = modal.querySelector("#lldp-switch-name");
        const syncBtn = modal.querySelector("#sync-lldp-btn");
        if (loadingEl)
            loadingEl.style.display = "block";
        if (contentEl)
            contentEl.style.display = "none";
        if (syncBtn)
            syncBtn.disabled = true;
        try {
            const switchResult = await apiGet(`/api/switches/${switchId}`);
            if (!switchResult.success) {
                if (loadingEl)
                    loadingEl.innerHTML = `<p style="color: red;">${t("switch.fetch_info_failed") || "获取交换机信息失败"}</p>`;
                return;
            }
            const switchData = switchResult.data;
            if (nameEl)
                nameEl.textContent = `- ${switchData.name}`;
            const result = await apiGet(`/api/switches/${switchId}/lldp-neighbors`);
            if (result.success) {
                const neighbors = result.data || [];
                if (nameEl)
                    nameEl.textContent = `- ${switchData.name} (${neighbors.length}${t("switch.entries") || "条"})`;
                if (neighbors.length === 0) {
                    if (loadingEl) {
                        loadingEl.innerHTML = `
              <p style="color: #666; margin-bottom: 10px;">${t("switch.no_lldp_data") || "暂无LLDP数据，请点击\"从SNMP同步\"按钮获取"}</p>
              <button class="btn btn-primary" id="sync-lldp-empty-btn">${t("switch.sync_from_snmp") || "从SNMP同步"}</button>
            `;
                    }
                    modal.querySelector("#sync-lldp-empty-btn")?.addEventListener("click", () => syncLldpData());
                    return;
                }
                if (contentEl)
                    contentEl.innerHTML = renderLldpTable(neighbors);
                if (loadingEl)
                    loadingEl.style.display = "none";
                if (contentEl)
                    contentEl.style.display = "block";
            }
            else {
                if (loadingEl)
                    loadingEl.innerHTML = `<p style="color: red;">${t("switch.load_lldp_failed") || "加载LLDP邻居失败"}: ${result.message || t("common.unknown_error") || "未知错误"}</p>`;
            }
        }
        catch (err) {
            const error = err;
            if (loadingEl)
                loadingEl.innerHTML = `<p style="color: red;">${t("common.load_failed") || "加载失败"}: ${error.message}</p>`;
        }
        finally {
            if (syncBtn)
                syncBtn.disabled = false;
        }
    };
    const syncLldpData = async () => {
        const loadingEl = modal.querySelector("#lldp-loading");
        const contentEl = modal.querySelector("#lldp-content");
        const syncBtn = modal.querySelector("#sync-lldp-btn");
        if (loadingEl)
            loadingEl.style.display = "block";
        if (contentEl)
            contentEl.style.display = "none";
        if (syncBtn)
            syncBtn.disabled = true;
        if (loadingEl) {
            loadingEl.innerHTML = `
        <div class="spinner"></div>
        <p style="margin-top: 10px; color: #666;">${t("switch.syncing_lldp") || "正在从SNMP同步LLDP信息..."}</p>
      `;
        }
        try {
            const result = await apiPost(`/api/switches/${switchId}/lldp/sync`, {});
            if (result.success) {
                await loadLldpData();
            }
            else {
                if (loadingEl)
                    loadingEl.innerHTML = `<p style="color: red;">${t("switch.sync_failed") || "同步失败"}: ${result.message || t("common.unknown_error") || "未知错误"}</p>`;
            }
        }
        catch (err) {
            const error = err;
            if (loadingEl)
                loadingEl.innerHTML = `<p style="color: red;">${t("switch.sync_failed") || "同步失败"}: ${error.message}</p>`;
        }
        finally {
            if (syncBtn)
                syncBtn.disabled = false;
        }
    };
    const syncBtn = modal.querySelector("#sync-lldp-btn");
    syncBtn?.addEventListener("click", syncLldpData);
    await loadLldpData();
}
export function renderLldpTable(neighbors) {
    const isMac = (str) => /^([0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}$/.test(str);
    return `
    <table class="table table-bordered" style="border-collapse: collapse; width: 100%; margin-top: 10px;">
      <thead>
        <tr style="background-color: var(--bg-secondary, #f0f2f5);">
          <th style="width: 60px; text-align: center; border: 1px solid var(--border-color, #ddd); padding: 12px 8px; font-weight: 600;">#</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("switch.local_port") || "本地端口"}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("switch.neighbor_device") || "邻居设备"}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("switch.neighbor_port") || "邻居端口"}</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">Chassis ID</th>
          <th style="border: 1px solid var(--border-color, #ddd); padding: 12px 10px; font-weight: 600;">${t("switch.system_desc") || "系统描述"}</th>
        </tr>
      </thead>
      <tbody>
        ${neighbors.map((n, i) => {
        let neighborPort = "-";
        if (n.neighbor_port_id && !isMac(n.neighbor_port_id)) {
            neighborPort = n.neighbor_port_id;
        }
        else if (n.neighbor_port_desc) {
            neighborPort = n.neighbor_port_desc;
        }
        else if (n.neighbor_port_id) {
            neighborPort = n.neighbor_port_id;
        }
        const sysName = n.neighbor_sys_name || n.remote_system_name || "-";
        const chassisId = n.neighbor_chassis_id || n.remote_chassis_id || "-";
        const sysDesc = n.neighbor_sys_desc || n.remote_system_description || "-";
        return `
          <tr style="background-color: ${i % 2 === 0 ? "var(--bg-primary, #fff)" : "var(--bg-tertiary, #fafbfc)"};">
            <td style="text-align: center; color: var(--text-muted, #888); border: 1px solid var(--border-color, #ddd); padding: 10px 8px;">${i + 1}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px; font-weight: 500;">${escapeHtml(n.local_port || "-")}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px;">${escapeHtml(sysName)}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px;">${escapeHtml(neighborPort)}</td>
            <td style="border: 1px solid var(--border-color, #ddd); padding: 10px; font-family: monospace; font-size: 0.9em;">${escapeHtml(chassisId)}</td>
            <td style="max-width: 250px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border: 1px solid var(--border-color, #ddd); padding: 10px; font-size: 0.9em; color: var(--text-secondary, #666);" title="${escapeHtml(sysDesc)}">${escapeHtml(sysDesc)}</td>
          </tr>
        `;
    }).join("")}
      </tbody>
    </table>
  `;
}
export async function loadSwitchesForLldp() {
    try {
        const result = await apiGet("/api/switches?page_size=1000");
        if (result.success) {
            const data = result.data;
            const switches = data?.items || (Array.isArray(data) ? data : []);
            const select = elementCache.get("lldp-switch-select");
            if (select) {
                select.innerHTML = `<option value="">${t("switch.select_switch") || "选择交换机..."}</option>` +
                    switches.map((sw) => `<option value="${sw.id}">${sw.name}</option>`).join("");
            }
        }
    }
    catch (error) {
        console.error("加载交换机列表失败:", error);
    }
}
//# sourceMappingURL=switchMacLldp.js.map