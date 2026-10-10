import type { QueryClient } from '@tanstack/react-query'
import {
  Outlet,
  createRootRouteWithContext,
  createRoute,
  createRouter,
  redirect,
} from '@tanstack/react-router'
import { isUnauthenticated } from './api/client'
import { sessionQuery } from './api/session'
import { Layout } from './components/Layout'
import { NotFound } from './components/States'
import { applyUserLanguage } from './i18n'
import { isSafeRedirect } from './lib/redirect'
import { parseComputerListSearch } from './lib/search'
import { ComputerPage } from './pages/ComputerPage'
import { ComputersPage } from './pages/ComputersPage'
import { LoginPage } from './pages/LoginPage'
import { PreferencesPage } from './pages/PreferencesPage'

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

const indexRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/',
  beforeLoad: () => {
    throw redirect({ to: '/assets/computers' })
  },
})

export const computersRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/assets/computers',
  validateSearch: parseComputerListSearch,
  component: ComputersPage,
})

export const computerRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/assets/computers/$computerId',
  params: {
    parse: ({ computerId }) => ({ computerId: Number(computerId) }),
    stringify: ({ computerId }) => ({ computerId: String(computerId) }),
  },
  component: ComputerPage,
})

const preferencesRoute = createRoute({
  getParentRoute: () => authenticatedRoute,
  path: '/preferences',
  component: PreferencesPage,
})

const routeTree = rootRoute.addChildren([
  loginRoute,
  authenticatedRoute.addChildren([indexRoute, computersRoute, computerRoute, preferencesRoute]),
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
}
