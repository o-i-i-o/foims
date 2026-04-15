import { createCrudManager } from "./crudFactory.js";
import type { CrudManager } from "../types/crud.js";
import type { NetworkRegion, Network, Room, Cabinet, Workstation, Switch } from "../types/resources.js";
import type { User } from "../types/session.js";

export function createNetworkRegionManager(): CrudManager<NetworkRegion> {
  return createCrudManager<NetworkRegion>({
    endpoint: "/api/resources/network-regions",
    entityName: "network_region",
    entityNameKey: "network.region",
    formId: "network-type-form",
    modalId: "network-type-modal",

    validateCallback: (data) => {
      if (!data.name || !String(data.name).trim()) {
        return "网络区域名称不能为空";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      description: data.description ? String(data.description).trim() : null,
    }),
  });
}

export function createNetworkManager(): CrudManager<Network> {
  return createCrudManager<Network>({
    endpoint: "/api/resources/networks",
    entityName: "network",
    entityNameKey: "network.network",
    formId: "network-form",
    modalId: "network-modal",

    validateCallback: (data) => {
      if (!data.name || !String(data.name).trim()) {
        return "网络名称不能为空";
      }
      if (!data.network_region_id) {
        return "请选择网络区域";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      network_region_id: data.network_region_id,
      ipv4_cidr: data.ipv4_cidr ? String(data.ipv4_cidr).trim() : null,
      ipv6_cidr: data.ipv6_cidr ? String(data.ipv6_cidr).trim() : null,
      ipv4_gateway: data.ipv4_gateway ? String(data.ipv4_gateway).trim() : null,
      ipv6_gateway: data.ipv6_gateway ? String(data.ipv6_gateway).trim() : null,
      ipv4_dns: data.ipv4_dns || null,
      ipv6_dns: data.ipv6_dns || null,
      description: data.description ? String(data.description).trim() : null,
    }),
  });
}

export function createRoomManager(): CrudManager<Room> {
  return createCrudManager<Room>({
    endpoint: "/api/resources/rooms",
    entityName: "room",
    entityNameKey: "room.room",
    formId: "room-form",
    modalId: "room-modal",

    validateCallback: (data) => {
      if (!data.name || !String(data.name).trim()) {
        return "房间名称不能为空";
      }
      if (!data.room_type) {
        return "请选择房间类型";
      }
      if (!data.network_ids || (data.network_ids as string[]).length === 0) {
        return "请至少选择一个网段";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      room_type: String(data.room_type ?? "").toUpperCase(),
      network_ids: data.network_ids || [],
      description: data.description ? String(data.description).trim() : null,
    }),
  });
}

export function createCabinetManager(): CrudManager<Cabinet> {
  return createCrudManager<Cabinet>({
    endpoint: "/api/resources/cabinets",
    entityName: "cabinet",
    entityNameKey: "cabinet.cabinet",
    formId: "cabinet-form",
    modalId: "cabinet-modal",

    validateCallback: (data) => {
      if (!data.name || !String(data.name).trim()) {
        return "机柜名称不能为空";
      }
      if (!data.room_id) {
        return "请选择所属机房";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      room_id: data.room_id,
      capacity: parseInt(String(data.capacity || data.total_units)) || 42,
      description: data.description ? String(data.description).trim() : null,
    }),
  });
}

export function createCabinetPositionManager(): CrudManager<{ id: number; name: string; cabinet_id: number; start_u: number; end_u: number; description?: string; ips?: unknown[] }> {
  return createCrudManager<{ id: number; name: string; cabinet_id: number; start_u: number; end_u: number; description?: string; ips?: unknown[] }>({
    endpoint: "/api/resources/positions",
    entityName: "cabinet_position",
    entityNameKey: "position.position",
    formId: "cabinet-position-form",
    modalId: "cabinet-position-modal",

    validateCallback: (data) => {
      if (!data.name || !String(data.name).trim()) {
        return "机位名称不能为空";
      }
      if (!data.cabinet_id) {
        return "请选择所属机柜";
      }
      const startU = parseInt(String(data.start_u));
      const endU = parseInt(String(data.end_u));
      if (isNaN(startU) || startU < 1) {
        return "起始U位必须是有效的正数";
      }
      if (isNaN(endU) || endU < 1) {
        return "结束U位必须是有效的正数";
      }
      if (endU < startU) {
        return "结束U位不能小于起始U位";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      cabinet_id: data.cabinet_id,
      start_u: parseInt(String(data.start_u)),
      end_u: parseInt(String(data.end_u)),
      ips: data.ips || [],
      description: data.description ? String(data.description).trim() : null,
    }),
  });
}

export function createWorkstationManager(): CrudManager<Workstation> {
  return createCrudManager<Workstation>({
    endpoint: "/api/resources/workstations",
    entityName: "workstation",
    entityNameKey: "workstation.workstation",
    formId: "workstation-form",
    modalId: "workstation-modal",

    validateCallback: (data) => {
      if (!data.name || !String(data.name).trim()) {
        return "工位名称不能为空";
      }
      if (!data.room_id) {
        return "请选择所属房间";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      room_id: data.room_id,
      manager: data.manager ? String(data.manager).trim() : null,
      ips: data.ips || [],
      description: data.description ? String(data.description).trim() : null,
    }),
  });
}

export function createSwitchManager(): CrudManager<Switch> {
  return createCrudManager<Switch>({
    endpoint: "/api/resources/switches",
    entityName: "switch",
    entityNameKey: "switch.switch",
    formId: "switch-form",
    modalId: "switch-modal",

    validateCallback: (data) => {
      if (!data.name || !String(data.name).trim()) {
        return "交换机名称不能为空";
      }
      if (!data.ip_address || !String(data.ip_address).trim()) {
        return "IP地址不能为空";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      ip_address: String(data.ip_address ?? "").trim(),
      snmp_version: data.snmp_version || "v2c",
      snmp_community: data.snmp_community ? String(data.snmp_community).trim() : "public",
      snmp_username: data.snmp_username ? String(data.snmp_username).trim() : null,
      snmp_auth_password: data.snmp_auth_password ? String(data.snmp_auth_password).trim() : null,
      snmp_priv_password: data.snmp_priv_password ? String(data.snmp_priv_password).trim() : null,
      network_region_id: data.network_region_id || null,
      description: data.description ? String(data.description).trim() : null,
    }),
  });
}

export function createUserManager(): CrudManager<User> {
  return createCrudManager<User>({
    endpoint: "/api/users",
    entityName: "user",
    entityNameKey: "user.user",
    formId: "user-form",
    modalId: "user-modal",

    validateCallback: (data) => {
      if (!data.username || !String(data.username).trim()) {
        return "用户名不能为空";
      }
      if (!data.email || !String(data.email).trim()) {
        return "邮箱不能为空";
      }
      if (data.password && data.password !== data.password_confirm) {
        return "两次输入的密码不一致";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      username: String(data.username ?? "").trim(),
      email: String(data.email ?? "").trim(),
      role: data.role || "user",
      status: data.status === "true" || data.status === true,
      password: data.password || undefined,
    }),
  });
}

export const managers: Record<string, CrudManager<unknown>> = {
  networkRegion: createNetworkRegionManager() as CrudManager<unknown>,
  network: createNetworkManager() as CrudManager<unknown>,
  room: createRoomManager() as CrudManager<unknown>,
  cabinet: createCabinetManager() as CrudManager<unknown>,
  cabinetPosition: createCabinetPositionManager() as CrudManager<unknown>,
  workstation: createWorkstationManager() as CrudManager<unknown>,
  switch: createSwitchManager() as CrudManager<unknown>,
  user: createUserManager() as CrudManager<unknown>,
};

export const roomManager = createRoomManager();
export const cabinetManager = createCabinetManager();
export const cabinetPositionManager = createCabinetPositionManager();
export const workstationManager = createWorkstationManager();
export const switchManager = createSwitchManager();
export const userManager = createUserManager();
export const networkRegionManager = createNetworkRegionManager();
export const networkManager = createNetworkManager();
