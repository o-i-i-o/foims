export declare const SWITCH_FORM_FIELDS: string[];
interface SwitchFormData {
    name: string;
    ip_address: string;
    vendor: string;
    model: string;
    description: string;
    snmp_version: string;
    snmp_community?: string;
    snmp_username?: string;
    snmp_auth_protocol?: string;
    snmp_auth_password?: string;
    snmp_priv_protocol?: string;
    snmp_priv_password?: string;
}
export declare function getSwitchFormValues(): SwitchFormData;
export declare function setSwitchFormValues(sw: Record<string, unknown>): Promise<void>;
export declare function resetSwitchForm(): void;
export {};
//# sourceMappingURL=switchForm.d.ts.map