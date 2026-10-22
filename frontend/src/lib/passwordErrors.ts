import { ApiError } from '../api/client'

export type PasswordErrorKey =
  | 'mismatch'
  | 'wrong_password'
  | 'same_password'
  | 'too_short'
  | 'missing_digit'
  | 'missing_lowercase'
  | 'missing_uppercase'
  | 'missing_symbol'
  | 'too_many_attempts'
  | 'unknown'

const RULES: readonly PasswordErrorKey[] = [
  'too_short',
  'missing_digit',
  'missing_lowercase',
  'missing_uppercase',
  'missing_symbol',
]
const CODES: readonly PasswordErrorKey[] = ['wrong_password', 'same_password', 'too_many_attempts']

/** Messages to show for a failed password change (`password.errors.*`). */
export function passwordErrorKeys(error: unknown): PasswordErrorKey[] {
  if (!(error instanceof ApiError)) {
    return ['unknown']
  }
  if (error.code === 'weak_password') {
    const rules = RULES.filter((rule) => error.details.includes(rule))
    return rules.length > 0 ? rules : ['unknown']
  }
  const code = CODES.find((known) => known === error.code)
  return [code ?? 'unknown']
}
