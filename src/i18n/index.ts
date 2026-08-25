import en from "./en";
import zhCN from "./zh-CN";

export const localeOptions = [
  { locale: "en", name: "English" },
  { locale: "de", name: "Deutsch" },
  { locale: "fr", name: "Français" },
  { locale: "it", name: "Italiano" },
  { locale: "es", name: "Español" },
  { locale: "ja", name: "日本語" },
  { locale: "zh-CN", name: "简体中文" },
  { locale: "zh-TW", name: "繁體中文" },
] as const;

export type Locale = (typeof localeOptions)[number]["locale"];
export type MessageKey = keyof typeof zhCN.messages;
export type Messages = Record<MessageKey, string>;
type Values = Record<string, string | number>;

// English is intentionally bundled as the safety language. All normal display
// text is loaded from the external locale directory through Tauri.
const englishSafetyMessages: Messages = en.messages;
const externalCatalogs: Record<Locale, Partial<Messages>> = {
  "zh-CN": {},
  en: {},
  de: {},
  fr: {},
  it: {},
  es: {},
  ja: {},
  "zh-TW": {},
};
let activeLocale: Locale = "zh-CN";

export function normalizeLocale(value: string): Locale {
  return localeOptions.some((option) => option.locale === value) ? value as Locale : "en";
}

export function setActiveLocale(value: string) {
  activeLocale = normalizeLocale(value);
}

/** Replaces the current runtime catalog with entries read from one external file. */
export function applyExternalMessages(locale: string, messages: Record<string, string>) {
  const normalized = normalizeLocale(locale);
  const accepted: Partial<Messages> = {};
  for (const [key, value] of Object.entries(messages)) {
    if (key in zhCN.messages && typeof value === "string") {
      accepted[key as MessageKey] = value;
    }
  }
  externalCatalogs[normalized] = accepted;
}

export function t(key: MessageKey, values?: Values): string {
  const raw = externalCatalogs[activeLocale][key] ?? englishSafetyMessages[key];
  return values ? raw.replace(/\{(\w+)\}/g, (_, name) => String(values[name] ?? `{${name}}`)) : raw;
}

export function formatDateTime(value: number | null): string {
  return value ? new Intl.DateTimeFormat(activeLocale, { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hour12: false }).format(new Date(value)) : t("calendar.placeholder");
}

export function formatMonth(year: number, month: number): string {
  return new Intl.DateTimeFormat(activeLocale, { year: "numeric", month: "2-digit" }).format(new Date(year, month, 1));
}
