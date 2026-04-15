// Switch form utilities
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
    return {
        name: elementCache.getValue("switch-name"),
        ip_address: elementCache.getValue("switch-ip"),
        vendor: elementCache.getValue("switch-vendor"),
        model: elementCache.getValue("switch-model"),
        description: elementCache.getValue("switch-description"),
        snmp_version: elementCache.getValue("switch-snmp-version"),
        snmp_community: elementCache.getValue("switch-snmp-community"),
        snmp_username: elementCache.getValue("switch-snmp-username"),
        snmp_auth_protocol: elementCache.getValue("switch-snmp-auth-protocol"),
        snmp_auth_password: elementCache.getValue("switch-snmp-auth-password"),
        snmp_priv_protocol: elementCache.getValue("switch-snmp-priv-protocol"),
        snmp_priv_password: elementCache.getValue("switch-snmp-priv-password"),
    };
}
export async function setSwitchFormValues(sw) {
    elementCache.setValue("switch-name", sw.name);
    elementCache.setValue("switch-ip", sw.ip_address);
    elementCache.setValue("switch-vendor", sw.vendor);
    elementCache.setValue("switch-model", sw.model);
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