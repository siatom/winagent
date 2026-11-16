import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import zhCN from "./locales/zh-CN.json";
import en from "./locales/en.json";

export type Language = "zh-CN" | "en";

export const normalizeLanguage = (language: unknown): Language => language === "en" ? "en" : "zh-CN";

void i18n.use(initReactI18next).init({
  resources: { "zh-CN": { translation: zhCN }, en: { translation: en } },
  lng: "zh-CN",
  fallbackLng: "zh-CN",
  supportedLngs: ["zh-CN", "en"],
  keySeparator: false,
  nsSeparator: false,
  interpolation: { escapeValue: false },
});

i18n.on("languageChanged", (language) => {
  document.documentElement.lang = normalizeLanguage(language);
});
document.documentElement.lang = normalizeLanguage(i18n.language);

export default i18n;
