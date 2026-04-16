export type SupportedLanguage = "zh" | "en";

export interface I18nInstance {
  language: SupportedLanguage;
  translations: Record<SupportedLanguage, Record<string, unknown>>;
  t(key: string, options?: Record<string, string>): string;
  changeLanguage(lang: SupportedLanguage): void;
  getCurrentLanguage(): SupportedLanguage;
  updatePageTranslations(): void;
  updateLanguageSelector(): void;
}

export interface I18nFallbackInstance {
  language: SupportedLanguage;
  translations: Record<string, never>;
  t: (key: string) => string;
  changeLanguage(lang: SupportedLanguage): void;
  getCurrentLanguage(): SupportedLanguage;
  updatePageTranslations(): void;
  updateLanguageSelector(): void;
}
