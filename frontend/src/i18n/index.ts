import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import { en } from './en'
import { fr } from './fr'
import { toLanguage, type Language } from './language'

const resources: Record<Language, { translation: typeof fr }> = {
  fr: { translation: fr },
  en: { translation: en },
}

declare module 'i18next' {
  interface CustomTypeOptions {
    resources: { translation: typeof fr }
  }
}

// Screen readers and spell checkers rely on the document language.
i18n.on('languageChanged', (language) => {
  document.documentElement.lang = language
})

void i18n.use(initReactI18next).init({
  resources,
  lng: toLanguage(navigator.language) ?? 'fr',
  fallbackLng: 'fr',
  interpolation: { escapeValue: false },
})

/** Switches to the language chosen in the user's ITSM-NG preferences. */
export function applyUserLanguage(language: string | null | undefined): void {
  const target = toLanguage(language)
  if (target && target !== i18n.language) {
    void i18n.changeLanguage(target)
  }
}

export default i18n
