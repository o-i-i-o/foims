import { elementCache } from "../../utils/helpers.js";
import { getManager } from "../../utils/ipconfig.js";
import {
  positionData,
  networkRegion,
  updateNetworkRegion
} from "./switchState.js";

const PASSWORD_MASK = '••••••••';

function maskToNull(value) {
  if (!value || value === PASSWORD_MASK || value.trim() === '') {
    return null;
  }
  return value;
}

export const SWITCH_FORM_FIELDS = [
  'switch-id', 'switch-name', 'switch-model', 'switch-vendor',
  'switch-location', 'switch-description', 'switch-snmp-version',
  'switch-snmp-port', 'switch-snmp-community', 'switch-snmp-username',
  'switch-snmp-auth-protocol', 'switch-snmp-auth-password',
  'switch-snmp-priv-protocol', 'switch-snmp-priv-password'
];

export function getSwitchFormValues() {
  const manager = getManager('switch');
  const rawIps = manager ? manager.getIps() : [];

  let networkRegionId = null;

  const ips = rawIps.map(ip => {
    if (ip.network_region_id) networkRegionId = ip.network_region_id;
    return ip;
  });

  return {
    id: elementCache.getValue('switch-id'),
    name: elementCache.getValue('switch-name'),
    model: elementCache.getValue('switch-model') || null,
    vendor: elementCache.getValue('switch-vendor') || null,
    location: elementCache.getValue('switch-location') || null,
    description: elementCache.getValue('switch-description') || null,
    snmp_version: elementCache.getValue('switch-snmp-version'),
    snmp_port: parseInt(elementCache.getValue('switch-snmp-port')) || 161,
    snmp_community: maskToNull(elementCache.getValue('switch-snmp-community')),
    snmp_username: elementCache.getValue('switch-snmp-username') || null,
    snmp_auth_protocol: elementCache.getValue('switch-snmp-auth-protocol') || null,
    snmp_auth_password: maskToNull(elementCache.getValue('switch-snmp-auth-password')),
    snmp_priv_protocol: elementCache.getValue('switch-snmp-priv-protocol') || null,
    snmp_priv_password: maskToNull(elementCache.getValue('switch-snmp-priv-password')),
    ips: ips,
    network_region_id: networkRegionId || positionData.networkRegionId,
    position_id: positionData.positionId || null
  };
}

export async function setSwitchFormValues(sw) {
  elementCache.setValue('switch-id', sw.id || '');
  elementCache.setValue('switch-name', sw.name || '');
  elementCache.setValue('switch-model', sw.model || '');
  elementCache.setValue('switch-vendor', sw.vendor || '');
  elementCache.setValue('switch-location', sw.location || '');
  elementCache.setValue('switch-description', sw.description || '');
  elementCache.setValue('switch-snmp-version', sw.snmp_version || 'v2c');
  elementCache.setValue('switch-snmp-port', sw.snmp_port || 161);
  elementCache.setValue('switch-snmp-community', sw.snmp_community || '');
  elementCache.setValue('switch-snmp-username', sw.snmp_username || '');
  elementCache.setValue('switch-snmp-auth-protocol', sw.snmp_auth_protocol || '');
  elementCache.setValue('switch-snmp-auth-password', sw.snmp_auth_password || '');
  elementCache.setValue('switch-snmp-priv-protocol', sw.snmp_priv_protocol || '');
  elementCache.setValue('switch-snmp-priv-password', sw.snmp_priv_password || '');

  positionData.positionId = sw.position_id || null;

  if (sw.position && sw.position.cabinet_id) {
    const pos = sw.position;
    positionData.cabinetId = pos.cabinet_id;
    positionData.cabinetName = pos.cabinet_name || '';
    positionData.startU = pos.start_u;
    positionData.endU = pos.end_u;
    if (pos.network_region_id) {
      positionData.networkRegionId = pos.network_region_id;
    }
  } else if (sw.cabinet_id) {
    positionData.cabinetId = sw.cabinet_id;
    positionData.cabinetName = sw.cabinet_name || '';
    positionData.startU = sw.start_u;
    positionData.endU = sw.end_u;
  }

  if (sw.ips && sw.ips.length > 0) {
    const firstIp = sw.ips[0];
    if (firstIp.network_region_id) {
      positionData.networkRegionId = firstIp.network_region_id;
      updateNetworkRegion(firstIp.network_region_id, firstIp.network_region);
    }
  }

  const manager = getManager('switch');
  if (manager) {
    manager.setExcludeSwitchId(sw.id || null);
    await manager.loadIps(sw.ips || []);
  }
}

export async function resetSwitchForm() {
  SWITCH_FORM_FIELDS.forEach(field => elementCache.setValue(field, ''));
  Object.assign(positionData, {
    cabinetId: null,
    cabinetName: null,
    startU: null,
    endU: null,
    positionId: null,
    networkRegionId: null
  });
  updateNetworkRegion(null);

  const cabinetSelect = elementCache.get('switch-cabinet-select');
  const startUInput = elementCache.get('switch-start-u');
  const endUInput = elementCache.get('switch-end-u');

  if (cabinetSelect) {
    cabinetSelect.value = '';
  }
  if (startUInput) {
    startUInput.value = '';
  }
  if (endUInput) {
    endUInput.value = '';
  }

  const manager = getManager('switch');
  if (manager) {
    manager.setExcludeSwitchId(null);
    manager.clear();
    await manager.addIpRow();
  }
}
