import { Link } from '@tanstack/react-router'

import { settingsNavigation } from '../lib/navigation'
import type { useOrderedNavigation } from '../lib/navigation-preferences'

/**
 * The pages and Settings. Links carry their command ids; ⌘1 to ⌘9 open the pages in this order,
 * which holding ⌘ reveals beside each (`CommandKeyHints`).
 */
export function Sidebar({
  pages,
  reportHome,
  settingsOpen,
  onOpenSettings,
}: {
  pages: ReturnType<typeof useOrderedNavigation>
  /** The page a report belongs to, highlighted while the report is open. */
  reportHome: string | undefined
  settingsOpen: boolean
  onOpenSettings: () => void
}) {
  const SettingsIcon = settingsNavigation.icon
  return (
    <aside className="sidebar" aria-label="Primary navigation">
      <div className="sidebar-header">
        <div className="brand">
          <span className="brand-name">Brief</span>
        </div>
      </div>

      <div className="sidebar-content">
        <nav className="sidebar-navigation" aria-label="Pages">
          <section className="sidebar-group">
            <h2 className="sidebar-group-label">Workspace</h2>
            <ul className="sidebar-menu">
              {pages.map(({ to, label, icon: Icon }, index) => (
                <li className="sidebar-menu-item" key={to}>
                  <Link
                    to={to}
                    className={`sidebar-menu-button${reportHome === to ? ' active' : ''}`}
                    activeProps={{ className: 'sidebar-menu-button active' }}
                    activeOptions={{ exact: to === '/' }}
                    aria-label={label}
                    aria-keyshortcuts={index < 9 ? `Meta+${index + 1}` : undefined}
                    data-command={`navigation.open.${to}`}
                  >
                    <Icon size={16} aria-hidden="true" />
                    <span>{label}</span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>
        </nav>
      </div>

      <div className="sidebar-footer">
        <nav aria-label="Application settings">
          <ul className="sidebar-menu">
            <li className="sidebar-menu-item">
              <a
                href="#settings"
                className={`sidebar-menu-button${settingsOpen ? ' active' : ''}`}
                aria-keyshortcuts="Meta+,"
                data-command="settings.open"
                onClick={(event) => {
                  event.preventDefault()
                  onOpenSettings()
                }}
              >
                <SettingsIcon size={16} aria-hidden="true" />
                <span>{settingsNavigation.label}</span>
              </a>
            </li>
          </ul>
        </nav>
      </div>
    </aside>
  )
}
