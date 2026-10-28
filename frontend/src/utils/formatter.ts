/**
 * 展示格式化工具（日志页使用项为主，
 * 其余格式化函数随需要补充）。
 */
import { i18n } from "../i18n/index";

// 日期格式化跟随界面语言（否则浏览器默认区域可能与界面语言不一致）
function locale(): string {
  return i18n.global.locale.value === "zh" ? "zh-CN" : "en-US";
}

export function formatDateTime(dateStr: string | null | undefined): string {
  if (!dateStr) {
    return "-";
  }
  return new Date(dateStr).toLocaleString(locale());
}

/** 仅日期（跟随界面语言），供"最早记录时间"、证书有效期等日期级展示使用 */
export function formatDate(dateStr: string | null | undefined): string {
  if (!dateStr) {
    return "-";
  }
  return new Date(dateStr).toLocaleDateString(locale());
}

/** 系统运行时长（秒 → X天X小时X分钟X秒，各时间单位文案走 i18n 动态键） */
export function formatUptime(totalSeconds: number): string {
  let seconds = totalSeconds;
  const days = Math.floor(seconds / (24 * 60 * 60));
  seconds %= 24 * 60 * 60;
  const hours = Math.floor(seconds / (60 * 60));
  seconds %= 60 * 60;
  const minutes = Math.floor(seconds / 60);
  seconds %= 60;

  let result = "";
  if (days > 0) {
    result += i18n.global.t("system.uptime_days", { count: days });
  }
  if (hours > 0) {
    result += i18n.global.t("system.uptime_hours", { count: hours });
  }
  if (minutes > 0) {
    result += i18n.global.t("system.uptime_minutes", { count: minutes });
  }
  result += i18n.global.t("system.uptime_seconds", { count: seconds });
  return result;
}

/** 下划线命名转空格首字母大写（翻译键缺失时的展示回退） */
function titleCase(value: string): string {
  return value
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

function translateEnumText(type: unknown, prefix: string): string {
  if (typeof type !== "string") {
    // 非字符串输入原样回显（模板插值按原文渲染）
    return type as string;
  }
  const key = `${prefix}.${type.toLowerCase()}`;
  return i18n.global.te(key) ? i18n.global.t(key) : titleCase(type);
}

/** 操作类型文本：语言包有对应键则翻译，否则 Title Case 回退 */
export function getOperationTypeText(type: unknown): string {
  return translateEnumText(type, "logs.operation_types");
}

/** 资源类型文本：同上 */
export function getResourceTypeText(type: unknown): string {
  return translateEnumText(type, "logs.resource_types");
}

/**
 * 登录日志 error_message 与通知 title：后端新数据存 i18n key（server.* 前缀），
 * 历史数据为中文/英文原文——key 形式翻译展示，原文原样展示。
 */
export function translateServerKey(value: unknown): string {
  if (typeof value === "string" && value.startsWith("server.")) {
    return i18n.global.t(value);
  }
  // 非字符串或历史原文原样回显
  return value as string;
}

/**
 * 通知 content：新格式为 JSON 字符串 {"key":"...","params":{...}}，
 * 历史数据为纯文本——解析成功且含 key 字段则按参数翻译，否则原样展示。
 */
export function translateNotificationContent(content: unknown): string {
  if (typeof content !== "string") {
    return content as string;
  }
  try {
    const parsed: unknown = JSON.parse(content);
    if (
      parsed !== null &&
      typeof parsed === "object" &&
      typeof (parsed as Record<string, unknown>).key === "string"
    ) {
      const record = parsed as { key: string; params?: Record<string, string> | null };
      return i18n.global.t(record.key, record.params || {});
    }
  } catch {
    // 历史纯文本内容，解析失败属预期，原样展示
  }
  return content;
}

/** IP 状态文本：语言包映射，未知值原样回显 */
export function getStatusText(status: string | null | undefined): string {
  const statusMap: Record<string, string> = {
    active: i18n.global.t("status.active"),
    inactive: i18n.global.t("status.inactive")
  };
  const mapped = typeof status === "string" ? statusMap[status] : undefined;
  return mapped || status || i18n.global.t("status.unknown");
}

/** 机房类型文本：语言包映射，未知值原样回显 */
export function getRoomTypeName(type: string | null | undefined): string {
  const typeNames: Record<string, string> = {
    OFFICE: i18n.global.t("room.type_office"),
    LOBBY: i18n.global.t("room.type_lobby"),
    RECEPTION: i18n.global.t("room.type_reception"),
    DATA_CENTER: i18n.global.t("room.type_datacenter"),
    TELECOM_CLOSET: i18n.global.t("room.type_telecom_closet"),
    OTHER: i18n.global.t("room.type_other")
  };
  const mapped = typeof type === "string" ? typeNames[type] : undefined;
  return mapped || type || "-";
}

/** 功率展示：指定单位输出（kW 保留至多 3 位小数）；空值显示 "-"（房间/机柜列表用） */
export function formatPowerValue(watts: number | null | undefined, unit: string): string {
  if (watts == null) {
    return "-";
  }
  if (unit === "kW") {
    return `${parseFloat((watts / 1000).toFixed(3))} kW`;
  }
  return `${watts} W`;
}

/** 操作类型图标：按 action 关键词匹配 emoji */
export function getActionIcon(action: string | null | undefined): string {
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

/** 相对时间：一分钟内刚刚，一小时内 N 分钟前，一天内 N 小时前，其余按日期展示 */
export function formatTime(timestamp: string | number | null | undefined): string {
  if (!timestamp) {
    return "-";
  }
  const date = new Date(timestamp);
  const diff = new Date().getTime() - date.getTime();

  if (diff < 60000) {
    return i18n.global.t("time.just_now");
  }
  if (diff < 3600000) {
    return i18n.global.t("time.minutes_ago", { count: Math.floor(diff / 60000) });
  }
  if (diff < 86400000) {
    return i18n.global.t("time.hours_ago", { count: Math.floor(diff / 3600000) });
  }
  return date.toLocaleDateString(locale());
}
