/**
 * vue-i18n 实例：复用旧版 web/static/i18n 的 zh/en JSON（迁移期单一来源）。
 *
 * 旧版自定义 t() 的占位符语法为 {{name}}，vue-i18n 为 {name}；消息中的
 * @ | $ 是 vue-i18n 消息格式的特殊字符（链接消息/复数/字面量），需转义。
 * 导入时统一归一化：{{name}} → {name}，其余特殊字符转义为字面量
 * （已核对现网语言包：花括号仅来自占位符，@ 仅 1 处、| 为 0 处）。
 */
import { createI18n } from "vue-i18n";
import zh from "./locales/zh.json";
import en from "./locales/en.json";

const SUPPORTED_LANGUAGES = new Set(["zh", "en"]);
const STORAGE_KEY = "language";

// 浏览器禁用存储（"阻止所有 Cookie"/存储策略）时 localStorage 访问抛
// SecurityError：读写均需防护，失败时回退浏览器语言/仅内存态
function readSavedLanguage() {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function persistLanguage(lang) {
  try {
    localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    console.warn("保存语言偏好失败（存储被禁用），仅保留在内存中");
  }
}

function detectInitialLanguage() {
  const saved = readSavedLanguage();
  if (saved && SUPPORTED_LANGUAGES.has(saved)) {
    return saved;
  }
  const browserLang = (navigator.language || "en").startsWith("zh") ? "zh" : "en";
  persistLanguage(browserLang);
  return browserLang;
}

/** 递归归一化消息文案：旧版占位符与特殊字符转 vue-i18n 语法 */
function normalizeMessages(obj) {
  if (typeof obj === "string") {
    return obj
      .replace(/\{\{(\w+)\}\}/g, "{$1}")
      // vue-i18n 字面量语法 {'@'}：转义后消息中的 @ | $ 按原样输出
      .replace(/[@|$]/g, "{'$&'}");
  }
  if (Array.isArray(obj)) {
    return obj.map(normalizeMessages);
  }
  if (obj && typeof obj === "object") {
    const out = {};
    for (const [key, value] of Object.entries(obj)) {
      out[key] = normalizeMessages(value);
    }
    return out;
  }
  return obj;
}

const initialLanguage = detectInitialLanguage();

export const i18n = createI18n({
  legacy: false,
  locale: initialLanguage,
  fallbackLocale: "zh",
  messages: {
    zh: normalizeMessages(zh),
    en: normalizeMessages(en)
  },
  missingWarn: false,
  fallbackWarn: false
});

/** 切换语言并持久化（与旧版行为一致：同步 <html lang>） */
export function changeLanguage(lang) {
  if (!SUPPORTED_LANGUAGES.has(lang)) {
    return;
  }
  i18n.global.locale.value = lang;
  persistLanguage(lang);
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
}
