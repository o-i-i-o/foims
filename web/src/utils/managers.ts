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
        return "common.required_field";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      description: data.description ? String(data.description).trim() : null,
    }),

    afterCreateCallback: () => {},
    afterUpdateCallback: () => {},
    afterDeleteCallback: () => {},
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
        return "common.required_field";
      }
      if (!data.network_type) {
        return "common.required_field";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      network_type: data.network_type,
      ipv4_cidr: data.ipv4_cidr ? String(data.ipv4_cidr).trim() : null,
      ipv6_cidr: data.ipv6_cidr ? String(data.ipv6_cidr).trim() : null,
      ipv4_gateway: data.ipv4_gateway ? String(data.ipv4_gateway).trim() : null,
      ipv6_gateway: data.ipv6_gateway ? String(data.ipv6_gateway).trim() : null,
      ipv4_dns: data.ipv4_dns ? String(data.ipv4_dns).trim() : null,
      ipv6_dns: data.ipv6_dns ? String(data.ipv6_dns).trim() : null,
      description: data.description ? String(data.description).trim() : null,
    }),

    afterCreateCallback: () => {},
    afterUpdateCallback: () => {},
    afterDeleteCallback: () => {},
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
        return "common.required_field";
      }
      if (!data.room_type) {
        return "common.required_field";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      room_type: String(data.room_type ?? "").toUpperCase(),
      description: data.description ? String(data.description).trim() : null,
    }),

    afterCreateCallback: () => {},
    afterUpdateCallback: () => {},
    afterDeleteCallback: () => {},
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
        return "common.required_field";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      room_id: data.room_id,
      total_units: parseInt(String(data.total_units)) || 42,
      description: data.description ? String(data.description).trim() : null,
    }),

    afterCreateCallback: () => {},
    afterUpdateCallback: () => {},
    afterDeleteCallback: () => {},
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
        return "common.required_field";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      name: String(data.name ?? "").trim(),
      room_id: data.room_id,
      description: data.description ? String(data.description).trim() : null,
    }),

    afterCreateCallback: () => {},
    afterUpdateCallback: () => {},
    afterDeleteCallback: () => {},
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
        return "common.required_field";
      }
      if (!data.ip_address || !String(data.ip_address).trim()) {
        return "common.required_field";
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
      description: data.description ? String(data.description).trim() : null,
    }),

    afterCreateCallback: () => {},
    afterUpdateCallback: () => {},
    afterDeleteCallback: () => {},
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
        return "common.required_field";
      }
      if (!data.email || !String(data.email).trim()) {
        return "common.required_field";
      }
      return null;
    },

    transformDataCallback: (data) => ({
      username: String(data.username ?? "").trim(),
      email: String(data.email ?? "").trim(),
      role: data.role || "user",
      status: data.status || "active",
    }),

    afterCreateCallback: () => {},
    afterUpdateCallback: () => {},
    afterDeleteCallback: () => {},
  });
}

export const managers: Record<string, CrudManager<unknown>> = {
  networkRegion: createNetworkRegionManager() as CrudManager<unknown>,
  network: createNetworkManager() as CrudManager<unknown>,
  room: createRoomManager() as CrudManager<unknown>,
  cabinet: createCabinetManager() as CrudManager<unknown>,
  workstation: createWorkstationManager() as CrudManager<unknown>,
  switch: createSwitchManager() as CrudManager<unknown>,
  user: createUserManager() as CrudManager<unknown>,
};
