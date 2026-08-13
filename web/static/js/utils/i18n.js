let i18nInstance = null;

function detectBrowserLanguage() {
  const browserLang = navigator.language || navigator.userLanguage || navigator.browserLanguage || 'en';
  if (browserLang.startsWith('zh')) {
    return 'zh';
  }
  return 'en';
}

function getInitialLanguage() {
  const savedLang = localStorage.getItem('language');
  if (savedLang && ['zh', 'en'].includes(savedLang)) {
    return savedLang;
  }
  const browserLang = detectBrowserLanguage();
  localStorage.setItem('language', browserLang);
  return browserLang;
}

export async function initI18n() {
  if (i18nInstance) {
    return i18nInstance;
  }

  const initialLanguage = getInitialLanguage();
  
  try {
    const [zhTranslations, enTranslations] = await Promise.all([
      // cache: 'no-store' guarantees the browser always loads the current
      // translation file. Without it, browsers (or proxies/CDNs) may serve a
      // stale cached JSON after new keys are added, leaving new data-i18n
      // elements untranslated.
      fetch('/static/js/i18n/zh.json', { cache: 'no-store' }).then(r => r.json()),
      fetch('/static/js/i18n/en.json', { cache: 'no-store' }).then(r => r.json())
    ]);

    i18nInstance = {
      language: initialLanguage,
      translations: {
        zh: zhTranslations,
        en: enTranslations
      },
      
      t(key, options = {}) {
        let defaultValue = null;
        let replaceOptions = options;
        
        if (typeof options === 'string') {
          defaultValue = options;
          replaceOptions = {};
        }
        
        const keys = key.split('.');
        let value = this.translations[this.language];
        
        for (const k of keys) {
          if (value && typeof value === 'object' && k in value) {
            value = value[k];
          } else {
            return defaultValue !== null ? defaultValue : key;
          }
        }
        
        if (typeof value !== 'string') {
          return defaultValue !== null ? defaultValue : key;
        }
        
        let result = value;
        for (const [k, v] of Object.entries(replaceOptions)) {
          result = result.replace(new RegExp(`\\{\\{${k}\\}\\}`, 'g'), v);
        }
        
        return result;
      },
      
      changeLanguage(lang) {
        this.language = lang;
        localStorage.setItem('language', lang);
        document.documentElement.lang = lang === 'zh' ? 'zh-CN' : 'en';
        this.updatePageTranslations();
        this.updateLanguageSelector();
        // Notify pages that render content dynamically (e.g. the init wizard's
        // status blocks) so they can re-render in the new language. Static
        // data-i18n elements are already handled by updatePageTranslations().
        window.dispatchEvent(new CustomEvent('languagechange', { detail: { language: lang } }));
      },
      
      getCurrentLanguage() {
        return this.language;
      },
      
      updatePageTranslations() {
        document.querySelectorAll('[data-i18n]').forEach(el => {
          const key = el.getAttribute('data-i18n');
          const translation = this.t(key);
          if (translation !== key) {
            el.textContent = translation;
          }
        });

        document.querySelectorAll('[data-i18n-placeholder]').forEach(el => {
          const key = el.getAttribute('data-i18n-placeholder');
          const translation = this.t(key);
          if (translation !== key) {
            el.placeholder = translation;
          }
        });

        document.querySelectorAll('[data-i18n-title]').forEach(el => {
          const key = el.getAttribute('data-i18n-title');
          const translation = this.t(key);
          if (translation !== key) {
            el.title = translation;
          }
        });

        document.querySelectorAll('[data-i18n-aria-label]').forEach(el => {
          const key = el.getAttribute('data-i18n-aria-label');
          const translation = this.t(key);
          if (translation !== key) {
            el.setAttribute('aria-label', translation);
          }
        });

        document.querySelectorAll('[data-i18n-value]').forEach(el => {
          const key = el.getAttribute('data-i18n-value');
          const translation = this.t(key);
          if (translation !== key) {
            el.value = translation;
          }
        });
      },
      
      updateLanguageSelector() {
        const selector = document.getElementById('language-selector');
        if (selector) {
          selector.value = this.language;
        }
      }
    };

    document.documentElement.lang = initialLanguage === 'zh' ? 'zh-CN' : 'en';
    
    i18nInstance.updatePageTranslations();
    i18nInstance.updateLanguageSelector();
    
    return i18nInstance;
  } catch (error) {
    console.error('Failed to initialize i18n:', error);
    
    i18nInstance = {
      language: initialLanguage,
      translations: {},
      t: (key) => key,
      changeLanguage(lang) {
        this.language = lang;
        localStorage.setItem('language', lang);
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
  i18nInstance.changeLanguage(lang);
}

export function updatePageTranslations() {
  if (!i18nInstance) {
    return;
  }
  i18nInstance.updatePageTranslations();
}

