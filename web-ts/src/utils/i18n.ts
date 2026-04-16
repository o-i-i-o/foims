import type { SupportedLanguage, I18nInstance, I18nFallbackInstance } from "../types/i18n.js";

let i18nInstance: I18nInstance | I18nFallbackInstance | null = null;

function detectBrowserLanguage(): SupportedLanguage {
  const browserLang = navigator.language || (navigator as unknown as Record<string, string>).userLanguage || (navigator as unknown as Record<string, string>).browserLanguage || "en";
  if (browserLang.startsWith("zh")) {
    return "zh";
  }
  return "en";
}

function getInitialLanguage(): SupportedLanguage {
  const savedLang = localStorage.getItem("language");
  if (savedLang && ["zh", "en"].includes(savedLang)) {
    return savedLang as SupportedLanguage;
  }
  const browserLang = detectBrowserLanguage();
  localStorage.setItem("language", browserLang);
  return browserLang;
}

export async function initI18n(): Promise<I18nInstance | I18nFallbackInstance> {
  if (i18nInstance) {
    return i18nInstance;
  }

  const initialLanguage = getInitialLanguage();

  try {
    const [zhTranslations, enTranslations] = await Promise.all([
      fetch("/static/js/i18n/zh.json").then(r => r.json()),
      fetch("/static/js/i18n/en.json").then(r => r.json()),
    ]);

    i18nInstance = {
      language: initialLanguage,
      translations: {
        zh: zhTranslations,
        en: enTranslations,
      },

      t(key: string, options: Record<string, string> = {}): string {
        const keys = key.split(".");
        let value: unknown = this.translations[this.language];

        for (const k of keys) {
          if (value && typeof value === "object" && k in (value as Record<string, unknown>)) {
            value = (value as Record<string, unknown>)[k];
          } else {
            return key;
          }
        }

        if (typeof value !== "string") {
          return key;
        }

        let result = value;
        for (const [k, v] of Object.entries(options)) {
          result = result.replace(new RegExp(`\\{\\{${k}\\}\\}`, "g"), v);
        }

        return result;
      },

      changeLanguage(lang: SupportedLanguage): void {
        this.language = lang;
        localStorage.setItem("language", lang);
        document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";
        this.updatePageTranslations();
        this.updateLanguageSelector();
      },

      getCurrentLanguage(): SupportedLanguage {
        return this.language;
      },

      updatePageTranslations(): void {
        document.querySelectorAll("[data-i18n]").forEach(el => {
          const key = el.getAttribute("data-i18n");
          if (key) {
            const translation = this.t(key);
            if (translation !== key) {
              el.textContent = translation;
            }
          }
        });

        document.querySelectorAll("[data-i18n-placeholder]").forEach(el => {
          const key = el.getAttribute("data-i18n-placeholder");
          if (key) {
            const translation = this.t(key);
            if (translation !== key) {
              (el as HTMLInputElement).placeholder = translation;
            }
          }
        });

        document.querySelectorAll("[data-i18n-title]").forEach(el => {
          const key = el.getAttribute("data-i18n-title");
          if (key) {
            const translation = this.t(key);
            if (translation !== key) {
              (el as HTMLElement).title = translation;
            }
          }
        });
      },

      updateLanguageSelector(): void {
        const selector = document.getElementById("language-selector") as HTMLSelectElement | null;
        if (selector) {
          selector.value = this.language;
        }
      },
    } satisfies I18nInstance;

    document.documentElement.lang = initialLanguage === "zh" ? "zh-CN" : "en";

    i18nInstance.updatePageTranslations();
    i18nInstance.updateLanguageSelector();

    return i18nInstance;
  } catch (error) {
    console.error("Failed to initialize i18n:", error);

    i18nInstance = {
      language: initialLanguage,
      translations: {} as Record<never, never>,
      t: (key: string) => key,
      changeLanguage(lang: SupportedLanguage): void {
        this.language = lang;
        localStorage.setItem("language", lang);
      },
      getCurrentLanguage(): SupportedLanguage {
        return this.language;
      },
      updatePageTranslations(): void {},
      updateLanguageSelector(): void {},
    } satisfies I18nFallbackInstance;

    return i18nInstance;
  }
}

export function t(key: string, options: Record<string, string> | string = {}): string {
  if (typeof options === "string") return i18nInstance ? i18nInstance.t(key) : key;
  if (!i18nInstance) {
    return key;
  }
  return i18nInstance.t(key, options);
}

export function changeLanguage(lang: SupportedLanguage): void {
  if (!i18nInstance) {
    return;
  }
  i18nInstance.changeLanguage(lang);
}

export function getCurrentLanguage(): SupportedLanguage {
  if (!i18nInstance) {
    return (localStorage.getItem("language") as SupportedLanguage) || detectBrowserLanguage();
  }
  return i18nInstance.getCurrentLanguage();
}

export function updatePageTranslations(): void {
  if (!i18nInstance) {
    return;
  }
  i18nInstance.updatePageTranslations();
}

export function getI18n(): I18nInstance | I18nFallbackInstance | null {
  return i18nInstance;
}

export function initLanguageSelector(): void {
  const selector = document.getElementById("language-selector") as HTMLSelectElement | null;
  if (!selector) return;

  if (i18nInstance) {
    selector.value = i18nInstance.language;
  }

  selector.addEventListener("change", (e: Event) => {
    const newLang = (e.target as HTMLSelectElement).value as SupportedLanguage;
    changeLanguage(newLang);
  });
}
