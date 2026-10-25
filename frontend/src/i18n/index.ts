/**
 * vue-i18n 实例：语言包为随构建入库的 src/i18n/locales/ zh/en JSON（单一来源）。
 *
 * 语言包历史文案的占位符语法为 {{name}}，vue-i18n 为 {name}；消息中的
 * @ | $ 是 vue-i18n 消息格式的特殊字符（链接消息/复数/字面量），需转义。
 * 导入时统一归一化：{{name}} → {name}，其余特殊字符转义为字面量
 * （已核对现网语言包：花括号仅来自占位符，@ 仅 1 处、| 为 0 处）。
 */
import { createI18n } from "vue-i18n";
import zhJson from "./locales/zh.json";
import enJson from "./locales/en.json";

// 消息键为运行时数据（键位由 i18n JSON 维护），按宽化类型消费：
// 避免大型 JSON 字面量类型的深度实例化（TS2589），t() 保持 string 签名
const zh = zhJson as Record<string, unknown>;
const en = enJson as Record<string, unknown>;

const SUPPORTED_LANGUAGES = new Set(["zh", "en"]);
const STORAGE_KEY = "language";

/** 消息树（递归）：叶子为字符串，节点为嵌套对象 */
type MessageTree = { [key: string]: string | MessageTree };

// 浏览器禁用存储（"阻止所有 Cookie"/存储策略）时 localStorage 访问抛
// SecurityError：读写均需防护，失败时回退浏览器语言/仅内存态
function readSavedLanguage(): string | null {
  try {
    return localStorage.getItem(STORAGE_KEY);
  } catch {
    return null;
  }
}

function persistLanguage(lang: string): void {
  try {
    localStorage.setItem(STORAGE_KEY, lang);
  } catch {
    console.warn("保存语言偏好失败（存储被禁用），仅保留在内存中");
  }
}

function detectInitialLanguage(): string {
  const saved = readSavedLanguage();
  if (saved && SUPPORTED_LANGUAGES.has(saved)) {
    return saved;
  }
  const browserLang = (navigator.language || "en").startsWith("zh") ? "zh" : "en";
  persistLanguage(browserLang);
  return browserLang;
}

/** 递归归一化消息文案：历史占位符与特殊字符转 vue-i18n 语法 */
function normalizeLeaf(value: unknown): string | MessageTree {
  if (typeof value === "string") {
    return value
      .replace(/\{\{(\w+)\}\}/g, "{$1}")
      // vue-i18n 字面量语法 {'@'}：转义后消息中的 @ | $ 按原样输出
      .replace(/[@|$]/g, "{'$&'}");
  }
  if (value && typeof value === "object") {
    const out: MessageTree = {};
    for (const [key, item] of Object.entries(value)) {
      // 语言包无数组节点；保留运行时兼容（数组按项归一化后挂回对象树）
      out[key] = Array.isArray(item) ? (item.map(normalizeLeaf) as unknown as MessageTree) : normalizeLeaf(item);
    }
    return out;
  }
  // 字符串/对象之外的原语叶子现网语言包不存在，按原样透传
  return value as string;
}

const initialLanguage = detectInitialLanguage();

// 归一化结果按 Record<string, MessageTree> 消费：满足 createI18n 的消息
// schema 约束，t() 据此保持 string → string 的简单签名
const normalizedMessages: Record<string, MessageTree> = {
  // 语言包根节点恒为对象（zh/en JSON 顶层即分区表）
  zh: normalizeLeaf(zh) as MessageTree,
  en: normalizeLeaf(en) as MessageTree
};

export const i18n = createI18n({
  legacy: false,
  locale: initialLanguage,
  fallbackLocale: "zh",
  messages: normalizedMessages,
  missingWarn: false,
  fallbackWarn: false
});

/** 切换语言并持久化：内存态 + localStorage + 同步 <html lang> */
export function changeLanguage(lang: string): void {
  if (!SUPPORTED_LANGUAGES.has(lang)) {
    return;
  }
  i18n.global.locale.value = lang;
  persistLanguage(lang);
  document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
}
