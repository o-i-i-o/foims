interface MacEntry {
    mac_address: string;
    vlan_id?: number;
    port?: string;
    type?: string;
}
export declare function viewArpTable(switchId: string): Promise<void>;
export declare function viewLldpNeighbors(switchId: string): Promise<void>;
export declare function loadSwitchesForLldp(): Promise<void>;
export declare function bindCollapseEvents(): void;
export declare function groupByNetwork(entries: MacEntry[]): Record<string, MacEntry[]>;
export declare function filterEntries(entries: MacEntry[], searchTerm: string): MacEntry[];
export {};
//# sourceMappingURL=switchMacLldp.d.ts.map