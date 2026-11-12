import { QueryClient } from '@tanstack/react-query'

/** The app's one query cache: pages read it, and route loaders fill it before pages open. */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false },
  },
})
