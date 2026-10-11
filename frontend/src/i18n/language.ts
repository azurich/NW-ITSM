export const LANGUAGES = ['fr', 'en'] as const

export type Language = (typeof LANGUAGES)[number]

/** `fr_FR` (ITSM-NG) or `fr-FR` (browser) to a supported language. */
export function toLanguage(value: string | null | undefined): Language | undefined {
  const short = value?.slice(0, 2).toLowerCase()
  return LANGUAGES.find((language) => language === short)
}
