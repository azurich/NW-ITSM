import type { QueryClient } from '@tanstack/react-query'
import {
  Outlet,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  notFound,
  redirect,
} from '@tanstack/react-router'
import { ASSET_TYPE_LIST, ASSET_TYPES, isAssetType, type AssetType, type DuplicateNotice } from './api/assets'
import { isUnauthenticated } from './api/client'
import { READ, hasRight, sessionQuery } from './api/session'
import { Layout } from './components/Layout'
import { NotFound } from './components/States'
import { applyUserLanguage } from './i18n'
import { isSafeRedirect } from './lib/redirect'
import { parseAssetCreateSearch, parseAssetListSearch } from './lib/search'
import { AssetCreatePage, AssetEditPage, TemplateCreatePage, TemplateEditPage } from './pages/AssetEditPage'
import { AssetPage } from './pages/AssetPage'
import { AssetsPage } from './pages/AssetsPage'
import { LoginPage } from './pages/LoginPage'
import { PreferencesPage } from './pages/PreferencesPage'
import { TemplatesPage } from './pages/TemplatesPage'

export interface RouterContext {
  queryClient: QueryClient
}

const rootRoute = createRootRouteWithContext<RouterContext>()({
  component: Outlet,
  notFoundComponent: NotFound,
})

export const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login',
  validateSearch: (search: Record<string, unknown>): { redirect?: string; bridge?: 'failed' } => ({
    redirect: isSafeRedirect(search.redirect) ? search.redirect : undefined,
    // Set by the API when opening from the classic interface failed.
    bridge: search.bridge === 'failed' ? 'failed' : undefined,
  }),
  component: LoginPage,
})

/** Every page below requires a session. */
const authenticatedRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'authenticated',
  beforeLoad: async ({ context, location }) => {
    try {
      const session = await context.queryClient.ensureQueryData(sessionQuery)
      applyUserLanguage(session.user.language)
    } catch (error) {
      if (isUnauthenticated(error)) {
        throw redirect({ to: '/login', search: { redirect: location.href } })
      }
      throw error
    }
  },
  component: Layout,
})

/** Home: the first asset list the profile can read. */
const indexRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/',
  beforeLoad: async ({ context }) => {
    const session = await context.queryClient.ensureQueryData(sessionQuery)
    const kind = ASSET_TYPE_LIST.find((candidate) => hasRight(session, ASSET_TYPES[candidate].right, READ))
    throw kind ? redirect({ to: '/assets/$kind', params: { kind } }) : redirect({ to: '/preferences' })
  },
})

/** Pages of one asset type; unknown types are not found. */
const assetTypeRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/assets/$kind',
  params: {
    parse: ({ kind }) => ({ kind: kind as AssetType }),
    stringify: ({ kind }) => ({ kind }),
  },
  beforeLoad: ({ params }) => {
    if (!isAssetType(params.kind)) {
      throw notFound()
    }
  },
})

const assetListRoute = createRoute({
  getParentRoute: () => assetTypeRoute,
  path: '/',
  validateSearch: parseAssetListSearch,
  component: AssetsPage,
})

const assetCreateRoute = createRoute({
  getParentRoute: () => assetTypeRoute,
  path: '/new',
  validateSearch: parseAssetCreateSearch,
  component: AssetCreatePage,
})

const idParams = {
  parse: ({ id }: { id: string }) => ({ id: Number(id) }),
  stringify: ({ id }: { id: number }) => ({ id: String(id) }),
}

const assetRoute = createRoute({
  getParentRoute: () => assetTypeRoute,
  path: '/$id',
  params: idParams,
  component: AssetPage,
})

const assetEditRoute = createRoute({
  getParentRoute: () => assetTypeRoute,
  path: '/$id/edit',
  params: idParams,
  component: AssetEditPage,
})

const templatesRoute = createRoute({
  getParentRoute: () => assetTypeRoute,
  path: '/templates',
})

const templateListRoute = createRoute({
  getParentRoute: () => templatesRoute,
  path: '/',
  component: TemplatesPage,
})

const templateCreateRoute = createRoute({
  getParentRoute: () => templatesRoute,
  path: '/new',
  component: TemplateCreatePage,
})

const templateEditRoute = createRoute({
  getParentRoute: () => templatesRoute,
  path: '/$id/edit',
  params: idParams,
  component: TemplateEditPage,
})

const preferencesRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/preferences',
  component: PreferencesPage,
})

const routeTree = rootRoute.addChildren([
  loginRoute,
  authenticatedRoute.addChildren([
    indexRoute,
    assetTypeRoute.addChildren([
      assetListRoute,
      assetCreateRoute,
      assetRoute,
      assetEditRoute,
      templatesRoute.addChildren([templateListRoute, templateCreateRoute, templateEditRoute]),
    ]),
    preferencesRoute,
  ]),
])

export function createAppRouter(queryClient: QueryClient) {
  return createRouter({
    routeTree,
    basepath: '/app',
    context: { queryClient },
    defaultPreload: 'intent',
    scrollRestoration: true,
  })
}

declare module '@tanstack/react-router' {
  interface Register {
    router: ReturnType<typeof createAppRouter>
  }
  interface HistoryState {
    /** Items with the same values as the asset just written. */
    duplicates?: DuplicateNotice[]
  }
}
