import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { RouterProvider } from '@tanstack/react-router'
import { ThemeProvider } from 'next-themes'
import { StrictMode, useEffect, useLayoutEffect } from 'react'
import { createRoot } from 'react-dom/client'
import { Toaster } from 'sonner'

import { loadSettingsPanel } from './components/settings-dialog'
import { TooltipProvider } from './components/ui/tooltip'
import { FinanceProvider } from './hooks/finance-provider'
import { LiveMarketProvider } from './hooks/live-market-provider'
import { useLiveFinance } from './hooks/use-live-finance'
import { ActionProvider } from './lib/action-system'
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

// The page waits, held at its first frame, until the window is on screen, then arrives once.
const root = document.documentElement
root.dataset.firstReveal = 'waiting'
function playFirstReveal() {
  if (root.dataset.firstReveal !== 'waiting') return
  requestAnimationFrame(() => {
    root.dataset.firstReveal = 'running'
    setTimeout(() => delete root.dataset.firstReveal, 1200)
  })
}
// Never leave the page held if startup stalls.
setTimeout(playFirstReveal, 4000)

function StartupReady() {
  const finance = useLiveFinance()
  useEffect(() => {
    if (finance.startupPending) return undefined
    let disposed = false
    let cancel = () => {}
    // Settings first: it opens over any page, so its code is the likeliest next need.
    const preloads: Array<() => Promise<unknown>> = [
      () => loadSettingsPanel(),
      ...navigation.map((page) => () => router.preloadRoute({ to: page.to })),
    ]
    let index = 0
    const schedule = () => {
      if (disposed) return
      if (index === preloads.length) return
      const run = () => {
        if (disposed) return
        void Promise.resolve()
          .then(preloads[index++])
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
    void revealMainWindow()
      .catch(() => undefined)
      .finally(playFirstReveal)
  }, [finance.startupPending])
  return null
}

// Light and System were retired; a saved or imported choice of either opens in Dark.
try {
  const savedTheme = localStorage.getItem('brief:theme')
  if (savedTheme === 'light' || savedTheme === 'system') localStorage.setItem('brief:theme', 'dark')
} catch {
  // Storage can be unavailable; the provider then falls back to Dark.
}

function App() {
  return (
    <ThemeProvider
      attribute="data-theme"
      defaultTheme="dark"
      enableSystem={false}
      storageKey="brief:theme"
      themes={['dark', 'custom']}
      disableTransitionOnChange
    >
      <QueryClientProvider client={queryClient}>
        <TooltipProvider delay={50} closeDelay={80}>
          <ActionProvider>
            <FinanceProvider>
              <LiveMarketProvider>
                <RouterProvider router={router} />
                <StartupReady />
              </LiveMarketProvider>
            </FinanceProvider>
          </ActionProvider>
          <Toaster
            theme="dark"
            position="bottom-right"
            toastOptions={{ className: 'brief-toast' }}
          />
        </TooltipProvider>
      </QueryClientProvider>
    </ThemeProvider>
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
    void revealMainWindow()
      .catch(() => undefined)
      .finally(playFirstReveal)
  })
