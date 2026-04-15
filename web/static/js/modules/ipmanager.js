import { apiGet, apiPost, } from "../utils/apiClient.js";
import { showToast, renderTable, formatDateTime, handleError, } from "../utils/ui.js";
import { getDeviceTypeName } from "../utils/formatter.js";
export async function loadSwitchesForPullMac() {
    try {
        const result = await apiGet("/api/switches");
        const select = document.getElementById("pull-mac-switch-select");
        if (!select)
            return;
        select.innerHTML = '<option value="">-- 选择交换机 --</option>';
        if (result.success && result.data) {
            const switches = Array.isArray(result.data) ? result.data : (result.data.items || []);
            if (switches.length === 0) {
                select.innerHTML = '<option value="">暂无交换机数据</option>';
                return;
            }
            let hasSnmpSwitch = false;
            switches.forEach((sw) => {
                const s = sw;
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
    }
    catch (error) {
        console.error("加载交换机列表失败:", error);
        const select = document.getElementById("pull-mac-switch-select");
        if (select) {
            select.innerHTML = '<option value="">加载失败</option>';
        }
    }
}
export async function loadNetworksForPullMac() {
    try {
        const result = await apiGet("/api/resources/networks?page_size=1000");
        const select = document.getElementById("pull-mac-network-select");
        if (!select)
            return;
        select.innerHTML = '<option value="">-- 选择网段 --</option>';
        if (result.success && result.data) {
            const data = result.data;
            const networks = Array.isArray(data) ? data : (data.items || data.data || []);
            if (networks.length === 0) {
                select.innerHTML = '<option value="">暂无网段数据</option>';
                return;
            }
            networks.forEach((network) => {
                const n = network;
                const option = document.createElement("option");
                option.value = String(n.id);
                option.textContent = `${n.name} (${n.ipv4_cidr || n.ipv6_cidr || "-"})`;
                select.appendChild(option);
            });
        }
    }
    catch (error) {
        console.error("加载网段列表失败:", error);
        const select = document.getElementById("pull-mac-network-select");
        if (select) {
            select.innerHTML = '<option value="">加载失败</option>';
        }
    }
}
export async function pullIpMacData() {
    const switchSelect = document.getElementById("pull-mac-switch-select");
    const switchId = switchSelect ? switchSelect.value : "";
    if (!switchId) {
        showToast("请先选择一个交换机", "warning");
        return;
    }
    const networkSelect = document.getElementById("pull-mac-network-select");
    const networkId = networkSelect ? networkSelect.value : "";
    if (!networkId) {
        showToast("请先选择一个网段", "warning");
        return;
    }
    const btn = document.getElementById("pull-ip-btn");
    if (!btn)
        return;
    const originalText = btn.textContent;
    try {
        btn.innerHTML = '<span class="loading"></span> 拉取中...';
        btn.disabled = true;
        const result = await apiPost("/api/resources/ip/pull", { switch_id: switchId, network_id: networkId });
        if (result.success) {
            showToast(result.message || "MAC数据拉取成功", "success");
            loadIpMacData();
        }
        else {
            showToast(`MAC数据拉取失败: ${result.message}`, "error");
        }
    }
    catch (error) {
        handleError(error, "拉取MAC数据失败");
    }
    finally {
        btn.innerHTML = originalText || "";
        btn.disabled = false;
    }
}
export async function loadIpMacData(searchParams = {}) {
    try {
        const { search = "", device_type = "", status = "", page = 1, page_size = 100 } = searchParams;
        const params = new URLSearchParams();
        if (search)
            params.append("search", search);
        if (device_type)
            params.append("device_type", device_type);
        if (status)
            params.append("status", status);
        params.append("page", String(page));
        params.append("page_size", String(page_size));
        const result = await apiGet(`/api/resources/ip?${params.toString()}`);
        if (result.success && result.data) {
            const data = result.data;
            const pageNum = data.page || 1;
            const startIndex = (pageNum - 1) * page_size;
            renderTable("#ip-table", {
                data: (data.data || []),
                columns: [
                    { field: "id", render: (_v, _row, index) => String(startIndex + index + 1), className: "index-column" },
                    { field: "device_name", render: (v) => v || "-" },
                    { field: "device_type", render: (v) => getDeviceTypeName(v) },
                    { field: "network_name", render: (v, row) => `${v || "未知"} (${row.network_region || "未知"})` },
                    { field: "ip_address", render: (v) => v },
                    { field: "mac_address", render: (v) => v || "-" },
                    { field: "hostname", render: (v) => v || "-" },
                    { field: "status", render: (v) => `<span class="status-badge ${v === "active" ? "status-active" : "status-inactive"}">${v}</span>` },
                    { field: "last_seen", render: (v) => formatDateTime(v) },
                    { field: "created_at", render: (v) => formatDateTime(v) },
                ],
                emptyMessage: "暂无IP数据",
            });
            return { total: data.total, page: data.page, total_pages: data.total_pages };
        }
        return null;
    }
    catch (error) {
        console.error("加载IP数据失败:", error);
        renderTable("#ip-table", {
            data: [],
            columns: [],
            emptyMessage: "服务器连接失败，请检查网络或联系管理员",
        });
        return null;
    }
}
export const initIpMacFunctions = () => {
    const ipSection = document.getElementById("ip");
    if (!ipSection)
        return;
    if (ipSection.dataset.initialized === "true")
        return;
    ipSection.dataset.initialized = "true";
    ipSection.addEventListener("click", (e) => {
        const target = e.target;
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
        if (e.target.id === "ip-search-input" && e.key === "Enter") {
            handleSearch();
        }
    });
};
function handleSearch() {
    const searchInput = document.getElementById("ip-search-input");
    const deviceTypeSelect = document.getElementById("ip-device-type-filter");
    const statusSelect = document.getElementById("ip-status-filter");
    loadIpMacData({
        search: searchInput?.value || "",
        device_type: deviceTypeSelect?.value || "",
        status: statusSelect?.value || "",
    });
}
//# sourceMappingURL=ipmanager.js.map