import { elementCache } from "../../utils/helpers.js";
export const SWITCH_FORM_FIELDS = [
    "switch-name",
    "switch-ip",
    "switch-vendor",
    "switch-model",
    "switch-description",
    "switch-snmp-version",
    "switch-snmp-community",
    "switch-snmp-username",
    "switch-snmp-auth-protocol",
    "switch-snmp-auth-password",
    "switch-snmp-priv-protocol",
    "switch-snmp-priv-password",
];
export function getSwitchFormValues() {
    const ipAddress = elementCache.getValue("switch-ip");
    return {
        name: elementCache.getValue("switch-name"),
        ips: ipAddress ? [{ ip_address: ipAddress, ip_type: "management" }] : [],
        mac_address: elementCache.getValue("switch-mac-address") || undefined,
        vendor: elementCache.getValue("switch-vendor") || undefined,
        model: elementCache.getValue("switch-model") || undefined,
        description: elementCache.getValue("switch-description") || undefined,
        snmp_version: elementCache.getValue("switch-snmp-version") || undefined,
        snmp_community: elementCache.getValue("switch-snmp-community") || undefined,
        snmp_username: elementCache.getValue("switch-snmp-username") || undefined,
        snmp_auth_protocol: elementCache.getValue("switch-snmp-auth-protocol") || undefined,
        snmp_auth_password: elementCache.getValue("switch-snmp-auth-password") || undefined,
        snmp_priv_protocol: elementCache.getValue("switch-snmp-priv-protocol") || undefined,
        snmp_priv_password: elementCache.getValue("switch-snmp-priv-password") || undefined,
    };
}
export async function setSwitchFormValues(sw) {
    elementCache.setValue("switch-name", sw.name);
    if (sw.ips && Array.isArray(sw.ips)) {
        const ips = sw.ips;
        if (ips.length > 0) {
            elementCache.setValue("switch-ip", ips[0].ip_address);
        }
    }
    else if (sw.ip_address) {
        elementCache.setValue("switch-ip", sw.ip_address);
    }
    elementCache.setValue("switch-vendor", sw.vendor || "");
    elementCache.setValue("switch-model", sw.model || "");
    elementCache.setValue("switch-description", sw.description || "");
    const snmpConfig = sw.snmp_config;
    if (snmpConfig) {
        elementCache.setValue("switch-snmp-version", snmpConfig.version);
        elementCache.setValue("switch-snmp-community", snmpConfig.community || "");
        elementCache.setValue("switch-snmp-username", snmpConfig.username || "");
        elementCache.setValue("switch-snmp-auth-protocol", snmpConfig.auth_protocol || "");
        elementCache.setValue("switch-snmp-auth-password", snmpConfig.auth_password || "");
        elementCache.setValue("switch-snmp-priv-protocol", snmpConfig.priv_protocol || "");
        elementCache.setValue("switch-snmp-priv-password", snmpConfig.priv_password || "");
    }
}
export function resetSwitchForm() {
    const form = elementCache.get("switch-form");
    if (form) {
        form.reset();
    }
}
//# sourceMappingURL=switchForm.js.map