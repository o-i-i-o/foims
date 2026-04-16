interface ArpEntry {
    ip_address: string;
    mac_address: string;
    interface?: string;
    vlan_id?: number;
}
interface LldpNeighbor {
    local_port: string;
    neighbor_sys_name?: string;
    neighbor_port_id?: string;
    neighbor_port_desc?: string;
    neighbor_chassis_id?: string;
    neighbor_sys_desc?: string;
    remote_system_name?: string;
    remote_port_id?: string;
    remote_chassis_id?: string;
    remote_system_description?: string;
}
export declare function viewArpTable(switchId: string): Promise<void>;
export declare function renderMacTable(entries: ArpEntry[], type: "ipv4" | "ipv6"): string;
export declare function bindCollapseEvents(container: HTMLElement): void;
export declare function groupByNetwork(entries: ArpEntry[], type: "ipv4" | "ipv6"): Record<string, ArpEntry[]>;
export declare function filterEntries(entries: ArpEntry[], searchTerm: string): ArpEntry[];
export declare function viewLldpNeighbors(switchId: string): Promise<void>;
export declare function renderLldpTable(neighbors: LldpNeighbor[]): string;
export declare function loadSwitchesForLldp(): Promise<void>;
export {};
//# sourceMappingURL=switchMacLldp.d.ts.map