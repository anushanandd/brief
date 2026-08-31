import { Dialog } from '@base-ui/react/dialog'
import { useQueryClient } from '@tanstack/react-query'
import { useNavigate } from '@tanstack/react-router'
import { Command } from 'cmdk'
import {
  ChartNoAxesCombined,
  Database,
  LayoutDashboard,
  Moon,
  RefreshCw,
  Settings,
  ShoppingBag,
  Sun,
  WalletCards,
} from 'lucide-react'
import { useTheme } from 'next-themes'
import { toast } from 'sonner'

import { financeQueryKey } from '../hooks/use-finance'
import { refreshFinanceSnapshot } from '../lib/api'
import { useUiStore } from '../store/ui'

const destinations = [
  { label: 'Overview', to: '/', icon: LayoutDashboard },
  { label: 'Analytics', to: '/analytics', icon: ChartNoAxesCombined },
  { label: 'Holdings', to: '/holdings', icon: WalletCards },
  { label: 'Spending', to: '/spending', icon: ShoppingBag },
  { label: 'Settings', to: '/settings', icon: Settings },
] as const

export function CommandMenu() {
  const open = useUiStore((state) => state.commandOpen)
  const setOpen = useUiStore((state) => state.setCommandOpen)
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { setTheme } = useTheme()

  const run = (action: () => void | Promise<void>) => {
    setOpen(false)
    void action()
  }

  const refresh = async () => {
    const promise = refreshFinanceSnapshot()
    toast.promise(promise, {
      loading: 'Refreshing your snapshot…',
      success: 'Snapshot refreshed',
      error: 'Refresh failed',
    })
    queryClient.setQueryData(financeQueryKey, await promise)
  }

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Backdrop className="command-backdrop" />
        <Dialog.Viewport className="command-viewport">
          <Dialog.Popup className="command-popup">
            <Dialog.Title className="sr-only">Quick actions</Dialog.Title>
            <Command label="Quick actions">
              <div className="command-input-wrap">
                <Command.Input placeholder="Go somewhere or run an action…" autoFocus />
                <span>esc</span>
              </div>
              <Command.List>
                <Command.Empty>No matching action.</Command.Empty>
                <Command.Group heading="Navigate">
                  {destinations.map(({ label, to, icon: Icon }, index) => (
                    <Command.Item
                      key={to}
                      value={`navigate ${label}`}
                      onSelect={() => run(() => navigate({ to }))}
                    >
                      <Icon size={17} aria-hidden="true" />
                      <span>{label}</span>
                      <kbd>⌘{index + 1}</kbd>
                    </Command.Item>
                  ))}
                </Command.Group>
                <Command.Separator />
                <Command.Group heading="Actions">
                  <Command.Item value="refresh data" onSelect={() => run(refresh)}>
                    <RefreshCw size={17} aria-hidden="true" />
                    <span>Refresh snapshot</span>
                  </Command.Item>
                  <Command.Item value="light theme" onSelect={() => run(() => setTheme('light'))}>
                    <Sun size={17} aria-hidden="true" />
                    <span>Use light appearance</span>
                  </Command.Item>
                  <Command.Item value="dark theme" onSelect={() => run(() => setTheme('dark'))}>
                    <Moon size={17} aria-hidden="true" />
                    <span>Use dark appearance</span>
                  </Command.Item>
                  <Command.Item
                    value="data sources"
                    onSelect={() => run(() => navigate({ to: '/settings' }))}
                  >
                    <Database size={17} aria-hidden="true" />
                    <span>Review data sources</span>
                  </Command.Item>
                </Command.Group>
              </Command.List>
            </Command>
          </Dialog.Popup>
        </Dialog.Viewport>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
