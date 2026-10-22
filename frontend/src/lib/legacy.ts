import { bridgeToken } from '../api/session'
import { legacyUrl } from './format'

/**
 * URL opening a page of the classic interface already logged in, through
 * the connection bridge; the plain URL when the bridge fails.
 */
export async function legacyBridgeUrl(path: string): Promise<string> {
  try {
    const token = await bridgeToken()
    const params = new URLSearchParams({ token, redirect: `/${path.replace(/^\/+/, '')}` })
    return legacyUrl(`front/nw_bridge.php?${params.toString()}`)
  } catch {
    return legacyUrl(path)
  }
}
