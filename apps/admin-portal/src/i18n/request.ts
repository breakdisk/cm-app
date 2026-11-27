import { cookies } from "next/headers";
import { getRequestConfig } from "next-intl/server";

export const locales = ["en", "ar"] as const;
export type Locale = (typeof locales)[number];
export const defaultLocale: Locale = "en";

export function isLocale(value: string | undefined): value is Locale {
  return value === "en" || value === "ar";
}

export default getRequestConfig(async () => {
  const cookieLocale = cookies().get("NEXT_LOCALE")?.value;
  const locale: Locale = isLocale(cookieLocale) ? cookieLocale : defaultLocale;
  return {
    locale,
    messages: (await import(`../../messages/${locale}.json`)).default,
  };
});
