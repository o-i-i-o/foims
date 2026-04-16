export declare const SWITCH_FORM_FIELDS: string[];
export interface SwitchFormData {
    id?: string;
    name: string;
    ips: Array<{
        ip_address: string;
        ip_type?: string;
    }>;
    mac_address?: string;
    vendor?: string;
    model?: string;
    location?: string;
    snmp_enabled?: boolean;
    snmp_version?: string;
    snmp_community?: string;
    snmp_username?: string;
    snmp_auth_protocol?: string;
    snmp_auth_password?: string;
    snmp_priv_protocol?: string;
    snmp_priv_password?: string;
    parent_switch_id?: string;
    description?: string;
    cabinet_id?: string;
    start_u?: number;
    end_u?: number;
    network_region_id?: string;
}
export declare function getSwitchFormValues(): SwitchFormData;
export declare function setSwitchFormValues(sw: Record<string, unknown>): Promise<void>;
export declare function resetSwitchForm(): void;
//# sourceMappingURL=switchForm.d.ts.map