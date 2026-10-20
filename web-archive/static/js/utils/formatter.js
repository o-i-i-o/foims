import { getCurrentLanguage, t } from "./i18n.js";

// 日期格式化跟随界面语言（否则浏览器默认区域可能与界面语言不一致）
function locale() {
  return getCurrentLanguage() === "zh" ? "zh-CN" : "en-US";
}

export function formatDateTime(dateStr) {
  if (!dateStr) {
    return "-";
  }
  const date = new Date(dateStr);
  return date.toLocaleString(locale());
}

/** 仅日期（跟随界面语言），供"最早记录时间"等日期级展示使用 */
export function formatDate(dateStr) {
  if (!dateStr) {
    return "-";
  }
  const date = new Date(dateStr);
  return date.toLocaleDateString(locale());
}

export function getStatusText(status) {
  const statusMap = {
    active: t("status.active"),
    inactive: t("status.inactive"),
    reserved: t("status.reserved")
  };
  return statusMap[status] || status || t("status.unknown");
}

export function getDeviceTypeName(type) {
  // 覆盖设备表全部 9 种 device_type（device-modal.html 下拉一致）；
  // workstation / cabinet_position 为额外兜底键，设备表约束中并无此二类
  const typeNames = {
    workstation: t("device.workstation"),
    cabinet_position: t("device.cabinet_position"),
    switch: t("device_type.switch"),
    desktop: t("device_type.desktop"),
    laptop: t("device_type.laptop"),
    printer: t("device_type.printer"),
    server: t("device_type.server"),
    network_device: t("device_type.network_device"),
    camera: t("device_type.camera"),
    phone: t("device_type.phone"),
    other: t("device_type.other")
  };
  return typeNames[type] || type || "-";
}

export function getRoomTypeName(type) {
  const typeNames = {
    OFFICE: t("room.type_office"),
    LOBBY: t("room.type_lobby"),
    RECEPTION: t("room.type_reception"),
    DATA_CENTER: t("room.type_datacenter"),
    TELECOM_CLOSET: t("room.type_telecom_closet"),
    OTHER: t("room.type_other")
  };
  return typeNames[type] || type || "-";
}

export function getActionIcon(action) {
  if (!action) {
    return "📋";
  }
  const actionLower = action.toLowerCase();
  if (actionLower.includes("create") || actionLower.includes("add")) {
    return "➕";
  }
  if (actionLower.includes("update") || actionLower.includes("edit")) {
    return "✏️";
  }
  if (actionLower.includes("delete") || actionLower.includes("remove")) {
    return "🗑️";
  }
  if (actionLower.includes("login")) {
    return "🔐";
  }
  if (actionLower.includes("logout")) {
    return "🚪";
  }
  return "📋";
}

export function formatTime(timestamp) {
  if (!timestamp) {
    return "-";
  }
  const date = new Date(timestamp);
  const now = new Date();
  const diff = now - date;

  if (diff < 60000) {
    return t("time.just_now");
  }
  if (diff < 3600000) {
    return t("time.minutes_ago", { count: Math.floor(diff / 60000) });
  }
  if (diff < 86400000) {
    return t("time.hours_ago", { count: Math.floor(diff / 3600000) });
  }
  return date.toLocaleDateString(locale());
}

export function getOperationTypeText(action) {
  if (!action) {
    return "-";
  }
  return t(`logs.operation_types.${action}`, action);
}

export function getResourceTypeText(resourceType) {
  if (!resourceType) {
    return "-";
  }
  return t(`logs.resource_types.${resourceType}`, resourceType);
}

/**
 * 功率展示：按单位（W/kW）格式化瓦值，未设置返回 "-"。
 * 千瓦最多保留三位小数并去掉尾零（1500 -> 1.5 kW）。
 */
export function formatPowerValue(watts, unit) {
  if (watts == null) {
    return "-";
  }
  if (unit === "kW") {
    return `${parseFloat((watts / 1000).toFixed(3))} kW`;
  }
  return `${watts} W`;
}
