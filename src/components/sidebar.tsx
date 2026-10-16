import { Link } from '@tanstack/react-router'

import { analyticsNavigation, settingsNavigation } from '../lib/navigation'
import type { useOrderedNavigation } from '../lib/navigation-preferences'
import { ChevronDown } from './icons'

/** The pages, the reports disclosure, and Settings. Links carry their command ids. */
export function Sidebar({
  pages,
  reportHome,
  reportsOpen,
  onToggleReports,
  settingsOpen,
  onOpenSettings,
}: {
  pages: ReturnType<typeof useOrderedNavigation>
  /** The page a report belongs to, highlighted while the report is open. */
  reportHome: string | undefined
  reportsOpen: boolean
  onToggleReports: () => void
  settingsOpen: boolean
  onOpenSettings: () => void
}) {
  const SettingsIcon = settingsNavigation.icon
  return (
    <aside className="sidebar" aria-label="Primary navigation">
      <div className="sidebar-header">
        <Link to="/" className="brand" aria-label="Brief home" data-command="navigation.open./">
          <span className="brand-name">Brief</span>
        </Link>
      </div>

      <div className="sidebar-content">
        <nav className="sidebar-navigation" aria-label="Pages">
          <section className="sidebar-group">
            <h2 className="sidebar-group-label">Workspace</h2>
            <ul className="sidebar-menu">
              {pages.map(({ to, label, icon: Icon }) => (
                <li className="sidebar-menu-item" key={to}>
                  <Link
                    to={to}
                    className={`sidebar-menu-button${reportHome === to ? ' active' : ''}`}
                    activeProps={{ className: 'sidebar-menu-button active' }}
                    activeOptions={{ exact: to === '/' }}
                    aria-label={label}
                    data-command={`navigation.open.${to}`}
                  >
                    <Icon size={16} aria-hidden="true" />
                    <span>{label}</span>
                  </Link>
                </li>
              ))}
            </ul>
          </section>

          <section className="sidebar-group">
            <h2 className="sidebar-group-label">
              <button
                type="button"
                className="sidebar-group-toggle"
                aria-expanded={reportsOpen}
                aria-controls="sidebar-analytics-menu"
                data-command="navigation.reports.toggle"
                onClick={onToggleReports}
              >
                Reports
                <ChevronDown size={12} aria-hidden="true" />
              </button>
            </h2>
            <ul className="sidebar-menu" id="sidebar-analytics-menu" hidden={!reportsOpen}>
              {analyticsNavigation.map((chart) => {
                const Icon = chart.icon
                return (
                  <li className="sidebar-menu-item" key={chart.search.chart}>
                    <Link
                      to={chart.to}
                      search={chart.search}
                      className="sidebar-menu-button"
                      data-command={`navigation.report.${chart.search.chart}`}
                      // One highlight: the report's destination above carries it.
                      activeProps={{}}
                    >
                      <Icon size={16} aria-hidden="true" />
                      <span>{chart.label}</span>
                    </Link>
                  </li>
                )
              })}
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
