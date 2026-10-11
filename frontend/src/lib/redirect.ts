/** Only paths inside the application are followed after login: never an open redirect. */
export function isSafeRedirect(value: unknown): value is string {
  return typeof value === 'string' && value.startsWith('/') && !value.startsWith('//') && !value.includes('\\')
}
