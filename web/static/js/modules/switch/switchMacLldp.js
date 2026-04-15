// MAC and LLDP table management
import { apiGet } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";
export async function viewArpTable(switchId) {
    try {
        const result = await apiGet(`/api/switches/${switchId}/arp-table`);
        if (result.success) {
            const entries = result.data.entries || [];
            renderMacTable(entries, "ARP表");
        }
        else {
            showToast("加载ARP表失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("加载ARP表失败:", error);
        showToast("加载ARP表失败", "error");
    }
}
export async function viewLldpNeighbors(switchId) {
    try {
        const result = await apiGet(`/api/switches/${switchId}/lldp-neighbors`);
        if (result.success) {
            const neighbors = result.data.neighbors || [];
            renderLldpTable(neighbors);
        }
        else {
            showToast("加载LLDP邻居失败: " + result.message, "error");
        }
    }
    catch (error) {
        console.error("加载LLDP邻居失败:", error);
        showToast("加载LLDP邻居失败", "error");
    }
}
export async function loadSwitchesForLldp() {
    // TODO: Implement loading switches for LLDP
}
function renderMacTable(entries, title) {
    // TODO: Implement MAC table rendering
    console.log("Render MAC table:", title, entries);
}
function renderLldpTable(neighbors) {
    // TODO: Implement LLDP table rendering
    console.log("Render LLDP table:", neighbors);
}
export function bindCollapseEvents() {
    // TODO: Implement collapse events
}
export function groupByNetwork(entries) {
    const groups = {};
    entries.forEach((entry) => {
        const vlan = entry.vlan_id?.toString() || "default";
        if (!groups[vlan]) {
            groups[vlan] = [];
        }
        groups[vlan].push(entry);
    });
    return groups;
}
export function filterEntries(entries, searchTerm) {
    if (!searchTerm)
        return entries;
    const lowerTerm = searchTerm.toLowerCase();
    return entries.filter((entry) => entry.mac_address.toLowerCase().includes(lowerTerm) ||
        (entry.port && entry.port.toLowerCase().includes(lowerTerm)));
}
//# sourceMappingURL=switchMacLldp.js.map