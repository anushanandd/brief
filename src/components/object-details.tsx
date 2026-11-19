import { useQuery } from '@tanstack/react-query'
import {
  createContext,
  useCallback,
  useContext,
  useId,
  useMemo,
  useState,
  type ReactNode,
} from 'react'

import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { tierLabels } from '../lib/objects'
import { getObject, objectKinds, type ObjectDetail } from '../lib/objects'
import { X } from './icons'
import { Button } from './ui'
import { Dialog } from './ui/dialog'
import { usePresence } from './ui/presence'

/**
 * The raw view (Details, ⌘I): everything stored about one object, as a readable tree. Curated
 * pages decide what is prominent; this guarantees nothing stored is unreachable. Amounts mask
 * like everywhere else when values are hidden.
 */
const ShowObjectContext = createContext<(id: string) => void>(() => undefined)

/** Opens the raw view of an object, by id. */
export const useShowObject = () => useContext(ShowObjectContext)

/** Registers ⌘I for the object a page shows, opening its raw view. */
export function useObjectDetailsAction(id: string | undefined) {
  const show = useShowObject()
  const actions = useMemo<AppAction[]>(
    () =>
      id
        ? [
            {
              id: 'object.details',
              title: 'Show details',
              search: 'details raw record stored fields sources',
              group: 'Here',
              scope: 'page',
              bindings: [{ sequence: ['Meta+i'], label: '⌘I' }],
              run: () => show(id),
            },
          ]
        : [],
    [id, show],
  )
  useRegisterActions(actions)
}

export function ObjectDetailsProvider({ children }: { children: ReactNode }) {
  const [shown, setShown] = useState<string | null>(null)
  const show = useCallback((id: string) => setShown(id), [])
  return (
    <ShowObjectContext.Provider value={show}>
      {children}
      <ObjectDetailsDialog id={shown} onShow={show} onClose={() => setShown(null)} />
    </ShowObjectContext.Provider>
  )
}

const MASK = '••••'

/** The raw view opens outside the page, so it follows screen privacy itself. */
function MaskedNumber({ value }: { value: number }) {
  return <span>{useAmountsHidden() ? MASK : String(value)}</span>
}

function Leaf({ value }: { value: unknown }) {
  if (typeof value === 'number') return <MaskedNumber value={value} />
  if (typeof value === 'boolean') return <span>{value ? 'yes' : 'no'}</span>
  return <span>{typeof value === 'string' ? value : JSON.stringify(value)}</span>
}

/** Whether a value holds anything: not null, blank, or only empty fields and lists. */
export const holds = (value: unknown): boolean =>
  Array.isArray(value)
    ? value.some(holds)
    : value && typeof value === 'object'
      ? Object.values(value).some(holds)
      : value !== null && value !== undefined && value !== ''

/**
 * A stored value as a tree: objects and lists open into their fields; the rest is a leaf. A
 * field that holds nothing is left out, so the tree shows only what was recorded.
 */
export function RawTree({ value }: { value: unknown }) {
  if (!holds(value)) return null
  if (Array.isArray(value)) {
    return (
      <ol className="raw-list">
        {value.flatMap((item, index) =>
          holds(item) ? (
            <li key={index}>
              <RawTree value={item} />
            </li>
          ) : (
            []
          ),
        )}
      </ol>
    )
  }
  if (value && typeof value === 'object') {
    const entries = Object.entries(value).filter(([, field]) => holds(field))
    return (
      <dl className="raw-tree">
        {entries.map(([key, field]) => (
          <div key={key}>
            <dt>{key}</dt>
            <dd>
              <RawTree value={field} />
            </dd>
          </div>
        ))}
      </dl>
    )
  }
  return <Leaf value={value} />
}

/** An object's facts, links, record, and each source's record of it. */
export function ObjectDetailsBody({
  detail,
  onShow,
}: {
  detail: ObjectDetail
  onShow: (id: string) => void
}) {
  const { object, facts, links, sources } = detail
  return (
    <div className="object-details-body">
      {facts.length ? (
        <section>
          <h3>Facts</h3>
          <ul className="object-facts">
            {facts.map((fact, index) => (
              <li key={index}>
                <span>{fact.predicate}</span>
                <RawTree value={fact.value} />
                <small>
                  <span
                    className={`tier-label tier-${fact.tier}`}
                    title={tierLabels[fact.tier].detail}
                  >
                    {tierLabels[fact.tier].label}
                  </span>{' '}
                  {[
                    fact.sources.join(', '),
                    fact.grade,
                    fact.from && fact.to ? `${fact.from} to ${fact.to}` : fact.from,
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </small>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {links.length ? (
        <section>
          <h3>Linked</h3>
          <ul className="object-links">
            {links.map((link) => {
              const other = link.from === object.id ? link.to : link.from
              return (
                <li key={`${link.from}|${link.relation}|${link.to}`}>
                  <span>{link.relation}</span>
                  <button type="button" className="object-link" onClick={() => onShow(other)}>
                    {other}
                  </button>
                </li>
              )
            })}
          </ul>
        </section>
      ) : null}
      <section>
        <h3>Record</h3>
        <RawTree value={object.record} />
      </section>
      {Object.entries(sources ?? {}).map(([source, record]) => (
        <section key={source}>
          <h3>As {source} sent it</h3>
          <RawTree value={record} />
        </section>
      ))}
    </div>
  )
}

function ObjectDetailsDialog({
  id,
  onShow,
  onClose,
}: {
  id: string | null
  onShow: (id: string) => void
  onClose: () => void
}) {
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null)
  const titleId = useId()
  const present = usePresence(Boolean(id), dialog)
  const query = useQuery({
    queryKey: ['object', id],
    queryFn: () => getObject(id ?? ''),
    enabled: Boolean(id),
    retry: false,
  })
  const detail = query.data
  const heading = detail
    ? `${objectKinds[detail.object.kind].label}: ${detail.object.title}`
    : 'Details'

  return (
    <Dialog
      ref={setDialog}
      open={Boolean(id)}
      onClose={onClose}
      className="shortcuts-dialog object-details-dialog"
      aria-labelledby={titleId}
    >
      {present ? (
        <div className="shortcuts-panel">
          <header className="shortcuts-header">
            <h2 id={titleId}>{heading}</h2>
            <Button size="icon" variant="ghost" aria-label="Close" onClick={onClose}>
              <X size={16} aria-hidden="true" />
            </Button>
          </header>
          <div className="shortcuts-body">
            <p className="object-details-id">{id}</p>
            {query.isLoading ? (
              <p role="status">Loading…</p>
            ) : detail ? (
              <ObjectDetailsBody detail={detail} onShow={onShow} />
            ) : (
              <p role="status">Nothing is stored under this id.</p>
            )}
          </div>
        </div>
      ) : null}
    </Dialog>
  )
}
