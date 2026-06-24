// src/lib/i18n.ts
// i18next 初始化：本地化检测 → 资源加载 → React 注入
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import zh from "@/locales/zh.json";
import en from "@/locales/en.json";

const STORAGE_KEY = "codex-box.lang";
const SUPPORTED = ["zh", "en"] as const;
type SupportedLng = (typeof SUPPORTED)[number];

/** 检测当前语言：localStorage > navigator > zh */
function detectLng(): SupportedLng {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    if (saved && (SUPPORTED as readonly string[]).includes(saved)) {
      return saved as SupportedLng;
    }
  } catch {
    /* localStorage 在隐私模式可能被禁；忽略 */
  }
  const nav = (navigator.language || "zh").toLowerCase();
  if (nav.startsWith("zh")) return "zh";
  if (nav.startsWith("en")) return "en";
  return "zh";
}

void i18n.use(initReactI18next).init({
  resources: {
    zh: { translation: zh },
    en: { translation: en },
  },
  lng: detectLng(),
  fallbackLng: "zh",
  interpolation: { escapeValue: false },
  returnNull: false,
  saveMissing: false,
});

/** 切换语言并写 localStorage */
export async function setLanguage(lng: SupportedLng): Promise<void> {
  await i18n.changeLanguage(lng);
  try {
    localStorage.setItem(STORAGE_KEY, lng);
  } catch {
    /* 忽略 */
  }
}

export const supportedLanguages = SUPPORTED;
export default i18n;
