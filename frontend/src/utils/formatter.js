/**
 * 展示格式化工具（移植自旧版 web/static/js/utils/formatter.js 中日志页使用项，
 * 其余格式化函数随对应页面迁移时补充）。
 */
import { i18n } from "../i18n/index.js";

// 日期格式化跟随界面语言（否则浏览器默认区域可能与界面语言不一致）
function locale() {
  return i18n.global.locale.value === "zh" ? "zh-CN" : "en-US";
}

export function formatDateTime(dateStr) {
  if (!dateStr) {
    return "-";
  }
  return new Date(dateStr).toLocaleString(locale());
}

/** 下划线命名转空格首字母大写（翻译键缺失时的展示回退） */
function titleCase(value) {
  return value
    .split("_")
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
    .join(" ");
}

function translateEnumText(type, prefix) {
  if (typeof type !== "string") {
    return type;
  }
  const key = `${prefix}.${type.toLowerCase()}`;
  return i18n.global.te(key) ? i18n.global.t(key) : titleCase(type);
}

/** 操作类型文本：语言包有对应键则翻译，否则 Title Case 回退 */
export function getOperationTypeText(type) {
  return translateEnumText(type, "logs.operation_types");
}

/** 资源类型文本：同上 */
export function getResourceTypeText(type) {
  return translateEnumText(type, "logs.resource_types");
}

/**
 * 登录日志 error_message 与通知 title：后端新数据存 i18n key（server.* 前缀），
 * 历史数据为中文/英文原文——key 形式翻译展示，原文原样展示。
 */
export function translateServerKey(value) {
  if (typeof value === "string" && value.startsWith("server.")) {
    return i18n.global.t(value);
  }
  return value;
}

/**
 * 通知 content：新格式为 JSON 字符串 {"key":"...","params":{...}}，
 * 历史数据为纯文本——解析成功且含 key 字段则按参数翻译，否则原样展示。
 */
export function translateNotificationContent(content) {
  if (typeof content !== "string") {
    return content;
  }
  try {
    const parsed = JSON.parse(content);
    if (parsed && typeof parsed === "object" && typeof parsed.key === "string") {
      return i18n.global.t(parsed.key, parsed.params || {});
    }
  } catch {
    // 历史纯文本内容，解析失败属预期，原样展示
  }
  return content;
}
