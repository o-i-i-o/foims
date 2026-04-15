interface Switch {
    id: string;
    name: string;
    device_type?: string;
    ip_address: string;
    mac_address?: string;
    vendor?: string;
    model?: string;
    location?: string;
    snmp_enabled?: boolean;
    snmp_version?: string;
    parent_switch_id?: string;
    parent_switch_name?: string;
    description?: string;
    status?: string;
    ports_count?: number;
    active_ports_count?: number;
    created_at?: string;
}
export declare function loadSwitchesData(page?: number, searchTerm?: string): Promise<void>;
export declare function fetchSwitchById(id: string): Promise<Switch | null>;
export declare function deleteSwitch(id: string | number): Promise<boolean>;
export declare function submitSwitchForm(): Promise<boolean>;
export {};
//# sourceMappingURL=switchList.d.ts.map