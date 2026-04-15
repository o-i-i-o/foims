// SNMP utilities for switch management
import { apiPost } from "../../utils/apiClient.js";
import { showToast } from "../../utils/ui.js";
import { elementCache } from "../../utils/helpers.js";

export function toggleSnmpConfig(): void {
  const snmpVersion = elementCache.getValue("switch-snmp-version");
  const v2Config = elementCache.get("switch-snmp-v2-config");
  const v3Config = elementCache.get("switch-snmp-v3-config");

  if (v2Config) {
    (v2Config as HTMLElement).style.display = snmpVersion === "v2c" ? "block" : "none";
  }
  if (v3Config) {
    (v3Config as HTMLElement).style.display = snmpVersion === "v3" ? "block" : "none";
  }
}

export async function testSnmpConnection(_switchId?: string): Promise<boolean> {
  try {
    const snmpVersion = elementCache.getValue("switch-snmp-version");
    const config: Record<string, unknown> = {
      ip_address: elementCache.getValue("switch-ip"),
      snmp_version: snmpVersion,
    };

    if (snmpVersion === "v2c") {
      config.snmp_community = elementCache.getValue("switch-snmp-community");
    } else if (snmpVersion === "v3") {
      config.snmp_username = elementCache.getValue("switch-snmp-username");
      config.snmp_auth_protocol = elementCache.getValue("switch-snmp-auth-protocol");
      config.snmp_auth_password = elementCache.getValue("switch-snmp-auth-password");
      config.snmp_priv_protocol = elementCache.getValue("switch-snmp-priv-protocol");
      config.snmp_priv_password = elementCache.getValue("switch-snmp-priv-password");
    }

    const result = await apiPost("/api/switches/test-snmp", config);
    if (result.success) {
      showToast("SNMP连接测试成功", "success");
      return true;
    } else {
      showToast("SNMP连接测试失败: " + result.message, "error");
      return false;
    }
  } catch (error) {
    console.error("SNMP连接测试失败:", error);
    showToast("SNMP连接测试失败", "error");
    return false;
  }
}

export async function getSwitchInfoFromSnmp(): Promise<Record<string, unknown> | null> {
  try {
    const result = await apiPost("/api/switches/discover", {
      ip_address: elementCache.getValue("switch-ip"),
      snmp_version: elementCache.getValue("switch-snmp-version"),
      snmp_community: elementCache.getValue("switch-snmp-community"),
    });

    if (result.success) {
      return result.data as Record<string, unknown>;
    }
    return null;
  } catch (error) {
    console.error("获取交换机信息失败:", error);
    return null;
  }
}

export async function syncPortsFromSnmp(switchId: string): Promise<boolean> {
  try {
    const result = await apiPost(`/api/switches/${switchId}/sync-ports`, {});
    if (result.success) {
      showToast("端口同步成功", "success");
      return true;
    } else {
      showToast("端口同步失败: " + result.message, "error");
      return false;
    }
  } catch (error) {
    console.error("端口同步失败:", error);
    showToast("端口同步失败", "error");
    return false;
  }
}
