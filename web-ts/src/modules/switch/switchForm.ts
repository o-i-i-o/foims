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

export interface SwitchFormData {
  id?: string;
  name: string;
  ips: Array<{ ip_address: string; ip_type?: string }>;
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
  position_id?: string | null;
  network_region_id?: string;
}

export function getSwitchFormValues(): SwitchFormData {
  const ipAddress = elementCache.getValue("switch-ip") as string;
  return {
    name: elementCache.getValue("switch-name") as string,
    ips: ipAddress ? [{ ip_address: ipAddress, ip_type: "management" }] : [],
    mac_address: (elementCache.getValue("switch-mac-address") as string) || undefined,
    vendor: (elementCache.getValue("switch-vendor") as string) || undefined,
    model: (elementCache.getValue("switch-model") as string) || undefined,
    description: (elementCache.getValue("switch-description") as string) || undefined,
    snmp_version: (elementCache.getValue("switch-snmp-version") as string) || undefined,
    snmp_community: (elementCache.getValue("switch-snmp-community") as string) || undefined,
    snmp_username: (elementCache.getValue("switch-snmp-username") as string) || undefined,
    snmp_auth_protocol: (elementCache.getValue("switch-snmp-auth-protocol") as string) || undefined,
    snmp_auth_password: (elementCache.getValue("switch-snmp-auth-password") as string) || undefined,
    snmp_priv_protocol: (elementCache.getValue("switch-snmp-priv-protocol") as string) || undefined,
    snmp_priv_password: (elementCache.getValue("switch-snmp-priv-password") as string) || undefined,
  };
}

export async function setSwitchFormValues(sw: Record<string, unknown>): Promise<void> {
  elementCache.setValue("switch-name", sw.name as string);

  if (sw.ips && Array.isArray(sw.ips)) {
    const ips = sw.ips as Array<{ ip_address: string; ip_type?: string }>;
    if (ips.length > 0) {
      elementCache.setValue("switch-ip", ips[0].ip_address);
    }
  } else if (sw.ip_address) {
    elementCache.setValue("switch-ip", sw.ip_address as string);
  }

  elementCache.setValue("switch-vendor", (sw.vendor as string) || "");
  elementCache.setValue("switch-model", (sw.model as string) || "");
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
