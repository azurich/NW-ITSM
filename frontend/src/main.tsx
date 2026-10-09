import { QueryCache, QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { ApiError, isUnauthenticated } from './api/client'
import './i18n'
import { createAppRouter } from './router'
import './styles.css'

const queryClient = new QueryClient({
  queryCache: new QueryCache({
    // The session expired or was revoked: back to the login page.
    onError: (error) => {
      if (isUnauthenticated(error) && router.state.location.pathname !== '/login') {
        queryClient.clear()
        void router.navigate({ to: '/login', search: { redirect: router.state.location.href } })
      }
    },
  }),
  defaultOptions: {
    queries: {
      // Client errors (403, 404...) will not change by retrying.
      retry: (failureCount, error) =>
        !(error instanceof ApiError && error.status < 500) && failureCount < 2,
    },
  },
})

const router = createAppRouter(queryClient)

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </StrictMode>,
)
