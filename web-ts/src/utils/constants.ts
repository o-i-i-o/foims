export const APP_NAME = "IPMA";

export const API_BASE_URL = "/api";

export const DEFAULT_PAGE_SIZE = 20;
export const MAX_PAGE_SIZE = 100;

export const CACHE_TTL = 60 * 1000;
export const DEBOUNCE_DELAY = 300;
export const TOAST_DURATION = 3000;

export const HTTP_STATUS = {
  OK: 200,
  CREATED: 201,
  NO_CONTENT: 204,
  BAD_REQUEST: 400,
  UNAUTHORIZED: 401,
  FORBIDDEN: 403,
  NOT_FOUND: 404,
  CONFLICT: 409,
  INTERNAL_ERROR: 500,
} as const;

export const ERROR_CODES = {
  NETWORK_ERROR: "NETWORK_ERROR",
  API_ERROR: "API_ERROR",
  VALIDATION_ERROR: "VALIDATION_ERROR",
  AUTH_ERROR: "AUTH_ERROR",
  PERMISSION_ERROR: "PERMISSION_ERROR",
  NOT_FOUND_ERROR: "NOT_FOUND_ERROR",
  TIMEOUT_ERROR: "TIMEOUT_ERROR",
  RUNTIME_ERROR: "RUNTIME_ERROR",
  UNHANDLED_REJECTION: "UNHANDLED_REJECTION",
} as const;

export const ROUTES = {
  LOGIN: "/login",
  DASHBOARD: "/",
  ROOMS: "/rooms",
  CABINETS: "/cabinets",
  WORKSTATIONS: "/workstations",
  NETWORKS: "/networks",
  SWITCHES: "/switches",
  USERS: "/users",
  LOGS: "/logs",
  SETTINGS: "/settings",
} as const;

export const RESOURCE_TYPES = {
  ROOM: "room",
  CABINET: "cabinet",
  WORKSTATION: "workstation",
  NETWORK: "network",
  SWITCH: "switch",
  POSITION: "position",
} as const;

export const ROOM_TYPES = {
  OFFICE: "office",
  DATA_CENTER: "data_center",
} as const;

export const USER_ROLES = {
  ADMIN: "admin",
  USER: "user",
} as const;

export const SNMP_VERSIONS = {
  V2C: "v2c",
  V3: "v3",
} as const;

export const SORT_ORDERS = {
  ASC: "asc",
  DESC: "desc",
} as const;

export const TOAST_TYPES = {
  SUCCESS: "success",
  ERROR: "error",
  WARNING: "warning",
  INFO: "info",
} as const;

export const MODAL_SIZES = {
  SMALL: "small",
  MEDIUM: "medium",
  LARGE: "large",
} as const;

export const PAGINATION = {
  FIRST_PAGE: 1,
  DEFAULT_VISIBLE_PAGES: 5,
} as const;
