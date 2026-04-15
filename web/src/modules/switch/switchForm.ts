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

export function getSwitchFormValues(): SwitchFormData {
  return {
    name: elementCache.getValue("switch-name") as string,
    ip_address: elementCache.getValue("switch-ip") as string,
    vendor: elementCache.getValue("switch-vendor") as string,
    model: elementCache.getValue("switch-model") as string,
    description: elementCache.getValue("switch-description") as string,
    snmp_version: elementCache.getValue("switch-snmp-version") as string,
    snmp_community: elementCache.getValue("switch-snmp-community") as string | undefined,
    snmp_username: elementCache.getValue("switch-snmp-username") as string | undefined,
    snmp_auth_protocol: elementCache.getValue("switch-snmp-auth-protocol") as string | undefined,
    snmp_auth_password: elementCache.getValue("switch-snmp-auth-password") as string | undefined,
    snmp_priv_protocol: elementCache.getValue("switch-snmp-priv-protocol") as string | undefined,
    snmp_priv_password: elementCache.getValue("switch-snmp-priv-password") as string | undefined,
  };
}

export async function setSwitchFormValues(sw: Record<string, unknown>): Promise<void> {
  elementCache.setValue("switch-name", sw.name as string);
  elementCache.setValue("switch-ip", sw.ip_address as string);
  elementCache.setValue("switch-vendor", sw.vendor as string);
  elementCache.setValue("switch-model", sw.model as string);
  elementCache.setValue("switch-description", (sw.description as string) || "");
  
  const snmpConfig = sw.snmp_config as Record<string, unknown> | undefined;
  if (snmpConfig) {
    elementCache.setValue("switch-snmp-version", snmpConfig.version as string);
    elementCache.setValue("switch-snmp-community", (snmpConfig.community as string) || "");
    elementCache.setValue("switch-snmp-username", (snmpConfig.username as string) || "");
    elementCache.setValue("switch-snmp-auth-protocol", (snmpConfig.auth_protocol as string) || "");
    elementCache.setValue("switch-snmp-auth-password", (snmpConfig.auth_password as string) || "");
    elementCache.setValue("switch-snmp-priv-protocol", (snmpConfig.priv_protocol as string) || "");
    elementCache.setValue("switch-snmp-priv-password", (snmpConfig.priv_password as string) || "");
  }
}

export function resetSwitchForm(): void {
  const form = elementCache.get("switch-form") as HTMLFormElement | null;
  if (form) {
    form.reset();
  }
}
