// MAC and LLDP table management
import { apiGet } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";

interface ArpEntry {
  ip_address: string;
  mac_address: string;
  interface?: string;
  vlan_id?: number;
}

interface LldpNeighbor {
  local_port: string;
  remote_system_name?: string;
  remote_port_id?: string;
  remote_chassis_id?: string;
  remote_system_description?: string;
}

interface MacEntry {
  mac_address: string;
  vlan_id?: number;
  port?: string;
  type?: string;
}

export async function viewArpTable(switchId: string): Promise<void> {
  try {
    const result = await apiGet(`/api/switches/${switchId}/arp-table`);
    if (result.success) {
      const entries = (result.data as { entries?: ArpEntry[] }).entries || [];
      renderMacTable(entries, "ARP表");
    } else {
      showToast("加载ARP表失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("加载ARP表失败:", error);
    showToast("加载ARP表失败", "error");
  }
}

export async function viewLldpNeighbors(switchId: string): Promise<void> {
  try {
    const result = await apiGet(`/api/switches/${switchId}/lldp-neighbors`);
    if (result.success) {
      const neighbors = (result.data as { neighbors?: LldpNeighbor[] }).neighbors || [];
      renderLldpTable(neighbors);
    } else {
      showToast("加载LLDP邻居失败: " + result.message, "error");
    }
  } catch (error) {
    console.error("加载LLDP邻居失败:", error);
    showToast("加载LLDP邻居失败", "error");
  }
}

export async function loadSwitchesForLldp(): Promise<void> {
  // TODO: Implement loading switches for LLDP
}

function renderMacTable(entries: ArpEntry[], title: string): void {
  // TODO: Implement MAC table rendering
  console.log("Render MAC table:", title, entries);
}

function renderLldpTable(neighbors: LldpNeighbor[]): void {
  // TODO: Implement LLDP table rendering
  console.log("Render LLDP table:", neighbors);
}

export function bindCollapseEvents(): void {
  // TODO: Implement collapse events
}

export function groupByNetwork(entries: MacEntry[]): Record<string, MacEntry[]> {
  const groups: Record<string, MacEntry[]> = {};
  entries.forEach((entry) => {
    const vlan = entry.vlan_id?.toString() || "default";
    if (!groups[vlan]) {
      groups[vlan] = [];
    }
    groups[vlan].push(entry);
  });
  return groups;
}

export function filterEntries(entries: MacEntry[], searchTerm: string): MacEntry[] {
  if (!searchTerm) return entries;
  const lowerTerm = searchTerm.toLowerCase();
  return entries.filter((entry) =>
    entry.mac_address.toLowerCase().includes(lowerTerm) ||
    (entry.port && entry.port.toLowerCase().includes(lowerTerm))
  );
}
