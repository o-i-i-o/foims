export interface NetworkRegion {
  id: number;
  name: string;
  description?: string;
  created_at?: string;
  updated_at?: string;
}

export interface Network {
  id: number;
  name: string;
  network_type?: string;
  network_region?: string;
  network_region_id?: number;
  ipv4_cidr?: string;
  ipv6_cidr?: string;
  ipv4_gateway?: string;
  ipv6_gateway?: string;
  ipv4_dns?: string;
  ipv6_dns?: string;
  description?: string;
  created_at?: string;
  updated_at?: string;
}

export interface Room {
  id: number;
  name: string;
  room_type: "OFFICE" | "DATA_CENTER";
  description?: string;
  networks?: Network[];
  created_at?: string;
  updated_at?: string;
}

export interface Workstation {
  id: number;
  name: string;
  room_id: number;
  room_name?: string;
  description?: string;
  ips?: IpAssignment[];
  created_at?: string;
  updated_at?: string;
}

export interface Cabinet {
  id: number;
  name: string;
  room_id: number;
  room_name?: string;
  total_units: number;
  description?: string;
  networks?: Network[];
  created_at?: string;
  updated_at?: string;
}

export interface CabinetPosition {
  id: number;
  name: string;
  cabinet_id: number;
  cabinet_name?: string;
  start_u: number;
  end_u: number;
  description?: string;
  ips?: IpAssignment[];
  created_at?: string;
  updated_at?: string;
}

export interface Switch {
  id: number;
  name: string;
  ip_address: string;
  snmp_version: "v2c" | "v3";
  snmp_community?: string;
  snmp_username?: string;
  snmp_auth_password?: string;
  snmp_priv_password?: string;
  network_region_id?: number;
  description?: string;
  ips?: IpAssignment[];
  created_at?: string;
  updated_at?: string;
}

export interface SwitchPort {
  id: number;
  switch_id: number;
  port_number: string;
  port_name?: string;
  status: "up" | "down";
  port_type?: string;
  speed?: string;
  description?: string;
  created_at?: string;
  updated_at?: string;
}

export interface IpAssignment {
  id?: number;
  network_id: string;
  ip_address: string;
  mac_address?: string | null;
  network_region_id?: string;
  device_type?: string;
  switch_id?: string;
  switch_port_id?: string;
  parent_switch_id?: string;
  parent_port_id?: string;
  port_id?: string;
}

export interface IpValidationResult {
  valid: boolean;
  errors?: string[];
  ips: IpAssignment[];
}

export interface CidrInfo {
  cidr: string | null;
  hasCidr: boolean;
  isV6: boolean;
}

export type ResourceType = "workstation" | "cabinet-position" | "switch";

export interface IpConfig {
  containerId: string;
  classPrefix: string;
  networksApi: (id?: string | number) => string | null;
  idSelector?: string;
  idName?: string;
  parentSwitchRequired: boolean;
  switchLabel: string;
  portLabel: string;
  excludeSwitchId?: number | string | null;
  loadNetworksByRegion?: boolean;
}
