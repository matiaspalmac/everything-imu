import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import { resources, type SupportedLocale } from "./resources";

const STORAGE_KEY = "everything-imu:locale";

function readSaved(): SupportedLocale {
  if (typeof window === "undefined") return "en";
  const v = window.localStorage.getItem(STORAGE_KEY);
  return v === "es" ? "es" : "en";
}

i18next.use(initReactI18next).init({
  resources,
  lng: readSaved(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
  returnNull: false,
});

export function setLocale(loc: SupportedLocale) {
  if (typeof window !== "undefined") {
    window.localStorage.setItem(STORAGE_KEY, loc);
  }
  void i18next.changeLanguage(loc);
}

export function currentLocale(): SupportedLocale {
  return (i18next.language as SupportedLocale) ?? "en";
}

export type { SupportedLocale };
