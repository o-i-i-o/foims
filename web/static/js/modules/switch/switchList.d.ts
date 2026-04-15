import type { SwitchFormData } from "./switchForm.js";
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
    snmp_community?: string;
    snmp_username?: string;
    parent_switch_id?: string;
    parent_switch_name?: string;
    description?: string;
    status?: string;
    ips?: Array<{
        ip_address: string;
        ip_type?: string;
    }>;
    ports_count?: number;
    active_ports_count?: number;
    created_at?: string;
    [key: string]: unknown;
}
export declare function loadSwitchesData(searchTerm?: string): Promise<void>;
export declare function fetchSwitchById(id: string): Promise<Switch | null>;
export declare function deleteSwitch(id: string | number): Promise<boolean>;
export declare function submitSwitchForm(formData: SwitchFormData): Promise<boolean>;
export type { Switch, SwitchFormData };
//# sourceMappingURL=switchList.d.ts.map