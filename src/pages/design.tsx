import { Link } from '@tanstack/react-router'
import { useEffect, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { Ban, Check, Ellipsis, Save } from '../components/icons'
import {
  Button,
  Card,
  ChartRangeSelect,
  EmptyState,
  Metric,
  SectionHeading,
} from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'

const foundations = [
  ['Canvas', 'background'],
  ['Card', 'surface'],
  ['Overlay', 'popup-surface'],
  ['Primary', 'text-primary'],
  ['Secondary', 'text-secondary'],
  ['Muted', 'text-muted'],
  ['Accent', 'accent'],
  ['Positive', 'positive'],
  ['Negative', 'negative'],
  ['Warning', 'warning'],
] as const
const dataColors = ['Blue', 'Amber', 'Purple', 'Teal', 'Rose', 'Orange', 'Olive']
const typeRoles = [
  ['Metadata', 'meta'],
  ['Body', 'body'],
  ['Section heading', 'section'],
  ['Page heading', 'page'],
  ['$1,234.56', 'metric'],
  ['$12,345.67', 'hero'],
] as const
const detailTokens = [
  ...foundations.map(([, token]) => token),
  ...dataColors.map((_, i) => `data-${i + 1}`),
  ...typeRoles.map(([, role]) => `type-${role}`),
  'radius-control',
  'radius-card',
  'card-padding',
  'control-height',
  'row-height',
  'control-background',
  'control-border',
  'surface-hover',
  'accent-soft',
  'popup-filter',
  'shadow-popup',
]

export function DesignPage() {
  const [values, setValues] = useState<Record<string, string>>({})
  const [range, setRange] = useState('M')
  const [selected, setSelected] = useState(true)
  const [feedback, setFeedback] = useState('Ready')
  useEffect(() => {
    const style = getComputedStyle(document.documentElement)
    setValues(
      Object.fromEntries(
        detailTokens.map((token) => [token, style.getPropertyValue(`--${token}`).trim()]),
      ),
    )
  }, [])
  return (
    <div className="page settings-page design-page">
      <WorkspaceHeader
        title="Design"
        parent={{ label: 'Settings', to: '/settings' }}
        showSnapshot={false}
        showRefresh={false}
      />
      <div className="design-sections">
        <Card>
          <SectionHeading title="Midday study" />
          <div className="midday-study-links">
            <Link to="/settings/design/midday/home">Home prototype</Link>
            <Link to="/settings/design/midday/activity" search={{}}>
              Activity prototype
            </Link>
          </div>
        </Card>
        <Card>
          <SectionHeading title="Foundations" />
          <div className="design-swatches">
            {foundations.map(([label, token]) => (
              <div className="design-swatch" key={token}>
                <span
                  className="design-color"
                  style={{ background: `var(--${token})` }}
                  aria-hidden="true"
                />
                <span>{label}</span>
              </div>
            ))}
          </div>
          <div className="design-type-samples">
            {typeRoles.map(([label, role]) => (
              <div key={role}>
                <span className="muted">{role}</span>
                <span
                  style={{
                    fontSize: `var(--type-${role})`,
                    fontFamily: role === 'page' ? 'var(--font-editorial)' : undefined,
                  }}
                >
                  {label}
                </span>
              </div>
            ))}
          </div>
          <details className="design-details">
            <summary>Tokens and measurements</summary>
            <dl className="design-token-list">
              {detailTokens.map((token) => (
                <div key={token}>
                  <dt>
                    <code>--{token}</code>
                  </dt>
                  <dd>{values[token]}</dd>
                </div>
              ))}
            </dl>
            <p className="design-note">
              4px spacing base · 16px card gaps · 24px card padding · 8px controls · 16px cards ·
              24px icon hit areas. Lists keep 12px bottom padding.
            </p>
          </details>
        </Card>
        <Card>
          <SectionHeading title="Data colors" />
          <div className="design-swatches">
            {dataColors.map((name, i) => (
              <div className="design-swatch" key={name}>
                <span
                  className="design-color"
                  style={{ background: `var(--data-${i + 1})` }}
                  aria-hidden="true"
                />
                <span>{name}</span>
              </div>
            ))}
          </div>
          <div className="design-comparison">
            <div>
              <strong>Category</strong>
              <div className="design-example-bar" aria-label="Example category distribution">
                {[1, 2, 3].map((index) => (
                  <span
                    key={index}
                    style={{ background: `var(--data-${index})`, flex: 4 - index }}
                  />
                ))}
              </div>
              <p className="design-note">
                Color identifies a category. Labels and tooltips identify its meaning.
              </p>
            </div>
            <div>
              <strong>Performance</strong>
              <div className="design-controls">
                <span className="positive">+$120.00</span>
                <span className="negative">−$45.00</span>
                <span className="muted">— Unavailable</span>
              </div>
              <p className="design-note">
                Green and red communicate financial direction, always with a signed value.
              </p>
            </div>
          </div>
          <details className="design-details">
            <summary>Category mappings</summary>
            <p className="design-note">
              Assets: brokerage blue, retirement purple, cash teal, credit rose, loans orange, other
              olive. Asset tiles use darker shades for readable labels. Spending reuses the same
              palette. Analytics assigns distinct colors from its full category list before account
              and date filters; additional categories receive additional hues. Newly imported
              categories can change the assignment.
            </p>
          </details>
        </Card>
        <Card>
          <SectionHeading title="Components" />
          <div className="design-comparison">
            <div className="account-summary-metrics">
              <Metric label="Sample balance" value="$12,345.67" />
              <Metric label="Change" value="+$120.00" tone="positive" />
            </div>
            <ActivityList
              referenceIso="2026-09-20"
              activities={[
                {
                  id: 'design-credit',
                  kind: 'credit',
                  category: 'Credit',
                  title: 'Example reimbursement',
                  detail: 'Example card · Credit',
                  date: '2026-09-19',
                  amount: 25,
                },
              ]}
            />
          </div>
          <div className="design-controls">
            <Button
              icon={Check}
              variant="primary"
              onClick={() => setFeedback('Primary action preview')}
            >
              Primary
            </Button>
            <Button icon={Save} onClick={() => setFeedback('Secondary action preview')}>
              Secondary
            </Button>
            <Button
              icon={Ellipsis}
              variant="ghost"
              onClick={() => setFeedback('Quiet action preview')}
            >
              Quiet
            </Button>
            <label>
              Sample input <input placeholder="Type here" />
            </label>
          </div>
          <p className="design-note" role="status">
            {feedback}
          </p>
        </Card>
        <Card>
          <SectionHeading title="Interaction" />
          <div className="design-controls">
            <ChartRangeSelect
              label="Sample date range"
              options={['W', 'M', 'Q', 'A'].map((value) => ({
                value,
                label: value,
                accessibleLabel: { W: 'Week', M: 'Month', Q: 'Quarter', A: 'All time' }[value]!,
              }))}
              value={range}
              onValueChange={setRange}
            />
            <label>
              <input
                type="checkbox"
                checked={selected}
                onChange={(event) => setSelected(event.target.checked)}
              />{' '}
              Selected
            </label>
            <Button icon={Ban} disabled>
              Disabled
            </Button>
          </div>
          <div className="design-comparison">
            <EmptyState title="No sample activity" />
            <div className="design-loading" role="status" aria-label="Loading sample">
              <span className="muted">Loading</span>
              <div className="skeleton skeleton-label" />
              <div className="skeleton skeleton-value" />
            </div>
          </div>
          <details className="design-details">
            <summary>Focus and motion</summary>
            <p className="design-note">
              Tab through the controls to preview the accent focus outline. Tooltips open without
              delay. Hover and press feedback remain brief; reduced-motion preferences are
              respected. Preview controls do not save settings.
            </p>
          </details>
        </Card>
      </div>
    </div>
  )
}
