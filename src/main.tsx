import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { StrictMode, useEffect, useLayoutEffect } from 'react'
import { createRoot } from 'react-dom/client'
import { Toaster } from 'sonner'

import { TooltipProvider } from './components/ui/tooltip'
import { FinanceProvider } from './hooks/finance-provider'
import { LiveMarketProvider } from './hooks/live-market-provider'
import { useLiveFinance } from './hooks/use-live-finance'
import { revealMainWindow } from './lib/api'
import { navigation } from './lib/navigation'
import { measureNavigation } from './lib/navigation-performance'
import { router } from './router'

import './styles.css'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { retry: 1, refetchOnWindowFocus: false },
  },
})

function StartupReady() {
  const finance = useLiveFinance()
  useEffect(() => {
    if (finance.startupPending) return undefined
    let disposed = false
    let cancel = () => {}
    let index = 0
    const schedule = () => {
      if (disposed || index === navigation.length) return
      const run = () => {
        if (disposed) return
        const page = navigation[index++]
        void router
          .preloadRoute({ to: page.to })
          .catch(() => undefined)
          .finally(schedule)
      }
      if ('requestIdleCallback' in window) {
        const id = window.requestIdleCallback(run, { timeout: 2000 })
        cancel = () => window.cancelIdleCallback(id)
      } else {
        const id = setTimeout(run, 200)
        cancel = () => clearTimeout(id)
      }
    }
    schedule()
    return () => {
      disposed = true
      cancel()
    }
  }, [finance.startupPending])
  useLayoutEffect(() => {
    if (finance.startupPending) return
    if (!performance.getEntriesByName('brief:local-interface-ready').length)
      performance.mark('brief:local-interface-ready')
    void revealMainWindow().catch(() => undefined)
  }, [finance.startupPending])
  return null
}

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delay={50} closeDelay={80}>
        <FinanceProvider>
          <LiveMarketProvider>
            <RouterProvider router={router} />
            <StartupReady />
          </LiveMarketProvider>
        </FinanceProvider>
        <Toaster theme="dark" position="bottom-right" toastOptions={{ className: 'brief-toast' }} />
      </TooltipProvider>
    </QueryClientProvider>
  )
}

performance.mark('brief:renderer-start')
measureNavigation(router)
void router
  .load()
  .then(() =>
    createRoot(document.getElementById('root')!).render(
      <StrictMode>
        <App />
      </StrictMode>,
    ),
  )
  .catch(() => {
    document.getElementById('root')!.textContent = 'Brief could not start. Please restart the app.'
    void revealMainWindow().catch(() => undefined)
  })
