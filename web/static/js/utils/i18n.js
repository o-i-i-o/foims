import { withVersion } from "./resourceLoader.js";

let i18nInstance = null;

/* 支持的语言清单：翻译文件按需加载（首屏只拉当前语言，切换时再取另一份），
   native_name 优先取语言文件内定义，未加载时用回退名 */
const SUPPORTED_LANGUAGES = ["zh", "en"];
const FALLBACK_NATIVE_NAMES = { zh: "中文", en: "English" };

function detectBrowserLanguage() {
  const browserLang = navigator.language || "en";
  return browserLang.startsWith("zh") ? "zh" : "en";
}

function getInitialLanguage() {
  const savedLang = localStorage.getItem("language");
  if (savedLang && SUPPORTED_LANGUAGES.includes(savedLang)) {
    return savedLang;
  }
  const browserLang = detectBrowserLanguage();
  localStorage.setItem("language", browserLang);
  return browserLang;
}

// 语言包位于 /static/i18n/（不在 /static/js/ 下）：生产 nginx 对 /static/js/
// 强制 no-cache（ES 模块需再验证），语言包走版本化 URL（?v=MODULE_VERSION）
// 才能落入 /static/ 的 immutable 长缓存桶 —— 版本号 bump 即失效，
// 重复访问零请求；未压缩体积 ~110KB，gzip 后线上 ~29KB
function fetchTranslations(lang) {
  // 非 2xx 响应的 body 通常不是合法 JSON：先校验状态再解析，
  // 失败抛出含 HTTP 状态的错误，由调用方 catch 兜底（保持原语言/回退实例）
  return fetch(withVersion(`/static/i18n/${lang}.json`)).then((r) => {
    if (!r.ok) {
      throw new Error(`HTTP ${r.status}`);
    }
    return r.json();
  });
}

export async function initI18n() {
  if (i18nInstance) {
    return i18nInstance;
  }

  const initialLanguage = getInitialLanguage();

  try {
    const translations = { [initialLanguage]: await fetchTranslations(initialLanguage) };

    i18nInstance = {
      language: initialLanguage,
      translations,

      /** 按需加载语言包；已加载或加载失败时原样返回 */
      async loadLanguage(lang) {
        if (this.translations[lang] || !SUPPORTED_LANGUAGES.includes(lang)) {
          return this.translations[lang] ?? null;
        }
        try {
          this.translations[lang] = await fetchTranslations(lang);
        } catch (error) {
          console.error(`加载语言包失败 [${lang}]:`, error);
        }
        return this.translations[lang] ?? null;
      },

      t(key, options = {}) {
        let defaultValue = null;
        let replaceOptions = options;

        if (typeof options === "string") {
          defaultValue = options;
          replaceOptions = {};
        }

        const value = key
          .split(".")
          .reduce(
            (obj, k) => (obj && typeof obj === "object" ? obj[k] : undefined),
            this.translations[this.language]
          );

        if (typeof value !== "string") {
          return defaultValue !== null ? defaultValue : key;
        }

        let result = value;
        for (const [k, v] of Object.entries(replaceOptions)) {
          result = result.replaceAll(`{{${k}}}`, v);
        }
        return result;
      },

      async changeLanguage(lang) {
        if (!SUPPORTED_LANGUAGES.includes(lang)) {
          return;
        }
        // 语言包未加载时先取回，取回失败则保持原语言
        if (!this.translations[lang]) {
          await this.loadLanguage(lang);
          if (!this.translations[lang]) {
            return;
          }
        }

        this.language = lang;
        localStorage.setItem("language", lang);
        document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
        this.updatePageTranslations();
        this.updateLanguageSelector();
        // Notify pages that render content dynamically (e.g. the init wizard's
        // status blocks) so they can re-render in the new language. Static
        // data-i18n elements are already handled by updatePageTranslations().
        window.dispatchEvent(new CustomEvent("languagechange", { detail: { language: lang } }));
      },

      getCurrentLanguage() {
        return this.language;
      },

      /** 可传入 root 元素限定扫描范围（如新打开的模态框），省去全文档遍历 */
      updatePageTranslations(root = document) {
        const apply = (attribute, property) => {
          root.querySelectorAll(`[${attribute}]`).forEach((el) => {
            const translation = this.t(el.getAttribute(attribute));
            if (translation !== el.getAttribute(attribute)) {
              if (property === "dataset") {
                el.dataset.tooltip = translation;
              } else if (property === "ariaLabel") {
                el.setAttribute("aria-label", translation);
              } else {
                el[property] = translation;
              }
            }
          });
        };

        apply("data-i18n", "textContent");
        apply("data-i18n-placeholder", "placeholder");
        apply("data-i18n-title", "title");
        apply("data-i18n-tooltip", "dataset");
        apply("data-i18n-aria-label", "ariaLabel");
        apply("data-i18n-value", "value");
      },

      updateLanguageSelector() {
        const selector = document.getElementById("language-selector");
        if (selector) {
          // 登录页/初始化页为 select 控件；main 页为按钮，展示当前语言名称
          if (selector.tagName === "SELECT") {
            selector.value = this.language;
          } else {
            const label = selector.querySelector(".lang-name");
            if (label) {
              label.textContent = this.getNativeName(this.language);
            }
          }
        }
      },

      getNativeName(code) {
        return (
          this.translations[code]?.language?.native_name || FALLBACK_NATIVE_NAMES[code] || code
        );
      }
    };

    document.documentElement.lang = initialLanguage === "zh" ? "zh-CN" : "en";

    i18nInstance.updatePageTranslations();
    i18nInstance.updateLanguageSelector();

    return i18nInstance;
  } catch (error) {
    console.error("Failed to initialize i18n:", error);

    i18nInstance = {
      language: initialLanguage,
      translations: {},
      t: (key) => key,
      async changeLanguage(lang) {
        this.language = lang;
        localStorage.setItem("language", lang);
      },
      getCurrentLanguage() {
        return this.language;
      },
      updatePageTranslations() {},
      updateLanguageSelector() {}
    };

    return i18nInstance;
  }
}

export function t(key, options = {}) {
  if (!i18nInstance) {
    return key;
  }
  return i18nInstance.t(key, options);
}

export function changeLanguage(lang) {
  if (!i18nInstance) {
    return;
  }
  return i18nInstance.changeLanguage(lang);
}

export function getCurrentLanguage() {
  if (!i18nInstance) {
    return localStorage.getItem("language") === "en" ? "en" : "zh";
  }
  return i18nInstance.getCurrentLanguage();
}

/**
 * 获取支持的语言列表（含各语言的本地名称）
 * native_name 取自已加载的语言文件，未加载的用内置回退名
 * @returns {Array<{code: string, nativeName: string}>}
 */
export function getSupportedLanguages() {
  const loaded = i18nInstance?.translations || {};
  return SUPPORTED_LANGUAGES.map((code) => ({
    code,
    nativeName: loaded[code]?.language?.native_name || FALLBACK_NATIVE_NAMES[code] || code
  }));
}

export function updatePageTranslations(root) {
  if (!i18nInstance) {
    return;
  }
  i18nInstance.updatePageTranslations(root);
}
