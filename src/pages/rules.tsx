import { useQuery, useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent } from 'react'

import { ExternalLink } from '../components/external-link'
import { Check, RefreshCw, Search, X } from '../components/icons'
import { useShowObject } from '../components/object-details'
import { Button, Card, EmptyState, SectionHeading } from '../components/ui'
import { useFinanceEdits } from '../hooks/use-finance-edits'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { isTauri } from '../lib/api'
import type { ProgramTemplate } from '../lib/bindings/ProgramTemplate'
import type { ResearchPage } from '../lib/bindings/ResearchPage'
import { errorMessage } from '../lib/errors'
import {
  acknowledgeProgramSource,
  draftProgram,
  evidenceOf,
  explanationSpan,
  getResearch,
  recheckProgramSources,
  researchGap,
  sourceDue,
  tierLabels,
  type Explanation,
  type Program,
  type ResearchState,
} from '../lib/evidence'
import type { FinanceSnapshot } from '../lib/schema'

const researchKey = ['research'] as const

/**
 * Insights' Rules view: what published bank and employer rules explain in your records, each
 * conclusion with its tier and its proof — every condition, the records that met it, and the
 * rule's cited source. A conclusion with a condition no record shows stays a possibility until
 * you confirm it. Below, the rules Brief knows, and opt-in research for rules it doesn't.
 */
export function RulesView({ data }: { data: FinanceSnapshot }) {
  const evidence = evidenceOf(data)
  const client = useQueryClient()
  const research = useQuery({ queryKey: researchKey, queryFn: getResearch })
  useEffect(() => {
    if (!isTauri()) return undefined
    const stop = listen(
      'research-updated',
      () => void client.invalidateQueries({ queryKey: researchKey }),
    )
    return () => void stop.then((unlisten) => unlisten())
  }, [client])
  if (!data.insights) {
    return (
      <Card className="chapters-card">
        <EmptyState>Rules are unavailable for this snapshot.</EmptyState>
      </Card>
    )
  }
  return (
    <div className="trips-main">
      {evidence.alerts.length ? (
        <Card className="chapters-card">
          <SectionHeading title="Notes from rules" />
          <ul className="chapter-checklist">
            {evidence.alerts.map((alert) => (
              <li key={alert.id}>{alert.says}</li>
            ))}
          </ul>
        </Card>
      ) : null}
      <Card className="chapters-card">
        <SectionHeading
          title="Explained"
          detail="Conclusions published rules reach from your records, with the rule's source."
        />
        {evidence.explanations.length ? (
          <ul className="rules-list">
            {evidence.explanations.map((explanation) => (
              <ExplanationRow key={explanation.id} explanation={explanation} />
            ))}
          </ul>
        ) : (
          <EmptyState>No rule's conditions are all met by your records yet.</EmptyState>
        )}
      </Card>
      <ProgramsCard data={data} research={research.data} />
      <ResearchCard research={research.data} />
    </div>
  )
}

/** One conclusion and its Why chain; a possibility can be confirmed or dismissed. */
function ExplanationRow({ explanation }: { explanation: Explanation }) {
  const { updateSettings } = useFinanceEdits()
  const showObject = useShowObject()
  const [error, setError] = useState<string>()
  const span = explanationSpan(explanation)
  const review = async (status: 'confirmed' | 'dismissed') => {
    setError(undefined)
    try {
      await updateSettings((current) => ({
        ...current,
        explanationReviews: { ...current.explanationReviews, [explanation.id]: status },
      }))
    } catch (caught) {
      setError(errorMessage(caught))
    }
  }
  return (
    <li className="rules-row" data-keyboard-row>
      <div className="rules-row-head">
        <span className="chapters-row-copy">
          <strong>{explanation.says}</strong>
          <small>{[explanation.title, span].filter(Boolean).join(' · ')}</small>
        </span>
        <span
          className={`tier-label tier-${explanation.tier}`}
          title={tierLabels[explanation.tier].detail}
        >
          {tierLabels[explanation.tier].label}
        </span>
      </div>
      <details className="rules-why">
        <summary>Why</summary>
        <ol>
          {explanation.proof.steps.map((step) => (
            <li key={step.says} className={step.met ? 'is-met' : undefined}>
              <span aria-label={step.met ? 'Met' : 'Not met'}>{step.met ? '✓' : '–'}</span>
              <span>
                {step.says}
                {step.detail ? <small>{step.detail}</small> : null}
              </span>
              {step.evidence[0] ? (
                <button
                  type="button"
                  className="object-link"
                  onClick={() => showObject(step.evidence[0])}
                >
                  Record
                </button>
              ) : null}
            </li>
          ))}
          {explanation.proof.unchecked.map((says) => (
            <li key={says}>
              <span aria-label="No record shows this">?</span>
              <span>{says}</span>
            </li>
          ))}
        </ol>
        {explanation.proof.notes.length ? (
          <ul className="chapter-checklist">
            {explanation.proof.notes.map((note) => (
              <li key={note}>{note}</li>
            ))}
          </ul>
        ) : null}
        <p className="rules-source">
          {explanation.proof.name} ·{' '}
          <ExternalLink href={explanation.proof.source.url}>
            {explanation.proof.source.title}
          </ExternalLink>{' '}
          · read {explanation.proof.reviewed}
        </p>
      </details>
      {explanation.tier === 'suggested' ? (
        <div className="chapter-actions">
          <Button
            size="compact"
            variant="primary"
            icon={Check}
            onClick={() => void review('confirmed')}
          >
            Confirm
          </Button>
          <Button size="compact" icon={X} onClick={() => void review('dismissed')}>
            Dismiss
          </Button>
        </div>
      ) : null}
      {error ? (
        <p role="alert" className="transaction-edit-note">
          {error}
        </p>
      ) : null}
    </li>
  )
}

/** The rules Brief knows: each source, when it was last read, and whether it changed since. */
function ProgramsCard({ data, research }: { data: FinanceSnapshot; research?: ResearchState }) {
  const programs = evidenceOf(data).programs
  const client = useQueryClient()
  const [error, setError] = useState<string>()
  const [checking, setChecking] = useState(false)
  const checks = new Map((research?.sources ?? []).map((check) => [check.url, check]))
  const recheck = async () => {
    setChecking(true)
    setError(undefined)
    try {
      await recheckProgramSources(true)
    } catch (caught) {
      setError(errorMessage(caught))
    } finally {
      setChecking(false)
      await client.invalidateQueries({ queryKey: researchKey })
    }
  }
  const canRecheck = Boolean(research?.enabled && research.connected)
  const latest = useRef(recheck)
  latest.current = recheck
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        canRecheck
          ? [
              {
                id: 'rules.recheck',
                title: 'Read rule sources again',
                search: 'rules sources recheck research changed terms',
                group: 'Here',
                scope: 'page',
                run: () => void latest.current(),
              },
            ]
          : [],
      [canRecheck],
    ),
  )
  return (
    <Card className="chapters-card">
      <SectionHeading
        title="Rules"
        detail="Built-in and approved rules, each with the page its terms come from."
        action={
          canRecheck ? (
            <Button
              size="compact"
              icon={RefreshCw}
              data-command="rules.recheck"
              disabled={checking}
              onClick={() => void recheck()}
            >
              {checking ? 'Reading sources…' : 'Read sources again'}
            </Button>
          ) : undefined
        }
      />
      <ul className="chapter-facts">
        {programs.map((program) => {
          const check = checks.get(program.source.url)
          return (
            <li key={program.id} data-keyboard-row>
              <span className="chapters-row-copy">
                <strong>{program.name}</strong>
                <small>
                  {program.organization} ·{' '}
                  <ExternalLink href={program.source.url}>{program.source.title}</ExternalLink> ·
                  read {program.reviewed}
                  {program.approved ? ' · approved by you' : ''}
                </small>
              </span>
              {check?.changedAt ? (
                <Button
                  size="compact"
                  onClick={() =>
                    void acknowledgeProgramSource(check.url).then(() =>
                      client.invalidateQueries({ queryKey: researchKey }),
                    )
                  }
                >
                  Page changed · reviewed
                </Button>
              ) : program.stale ? (
                <span className="tier-label tier-suggested">Not read in a year</span>
              ) : null}
            </li>
          )
        })}
      </ul>
      {error ? (
        <p role="alert" className="transaction-edit-note">
          {error}
        </p>
      ) : null}
    </Card>
  )
}

/**
 * Opt-in research: what no rule covers, the public query that would look for one, and every
 * query sent. Pages that come back are shown, never acted on; a rule joins only when approved.
 */
function ResearchCard({ research }: { research?: ResearchState }) {
  const client = useQueryClient()
  const [running, setRunning] = useState<string>()
  const [error, setError] = useState<string>()
  const [drafting, setDrafting] = useState<ResearchPage>()
  const stopDrafting = useCallback(() => setDrafting(undefined), [])
  // A month after the last read of a source, opening Rules reads it again; Rust reads only the
  // sources due, so once per visit is enough.
  const rechecked = useRef(false)
  useEffect(() => {
    if (rechecked.current || !research?.enabled || !research.connected) return
    const latest = research.log.find((entry) => entry.kind === 'recheck')
    if (!sourceDue(latest?.startedAt, new Date())) return
    rechecked.current = true
    void recheckProgramSources(false)
      .catch(() => undefined)
      .finally(() => client.invalidateQueries({ queryKey: researchKey }))
  }, [client, research?.connected, research?.enabled, research?.log])
  if (!research) return null
  const search = async (gapId: string) => {
    setRunning(gapId)
    setError(undefined)
    try {
      await researchGap(gapId)
    } catch (caught) {
      setError(errorMessage(caught))
    } finally {
      setRunning(undefined)
      await client.invalidateQueries({ queryKey: researchKey })
    }
  }
  return (
    <Card className="chapters-card">
      <SectionHeading
        title="Research"
        detail={
          !research.enabled
            ? 'Off. Turn on research in Settings › Connections to look up rules by public names.'
            : !research.connected
              ? 'Add an Exa API key in Settings › Connections.'
              : 'Each query names only public things and is logged before it runs.'
        }
      />
      {research.gaps.length ? (
        <section>
          <h3>No rule covers</h3>
          <ul className="chapter-facts">
            {research.gaps.map((gap) => (
              <li key={gap.id} data-keyboard-row>
                <span className="chapters-row-copy">
                  <strong>{gap.name}</strong>
                  <small>Query: {gap.query}</small>
                </span>
                <Button
                  size="compact"
                  icon={Search}
                  disabled={!research.enabled || !research.connected || running !== undefined}
                  onClick={() => void search(gap.id)}
                >
                  {running === gap.id ? 'Searching…' : 'Search'}
                </Button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {drafting ? <DraftSheet page={drafting} onDone={stopDrafting} /> : null}
      {research.log.length ? (
        <section>
          <h3>Research log</h3>
          <ul className="rules-log">
            {research.log.map((entry) => (
              <li key={entry.id} data-keyboard-row>
                <span className="chapters-row-copy">
                  <strong>{entry.query}</strong>
                  <small>
                    {entry.kind === 'recheck' ? 'Source read again' : 'Search'} ·{' '}
                    {entry.startedAt.slice(0, 16).replace('T', ' ')} ·{' '}
                    {entry.status === 'running'
                      ? 'running'
                      : entry.status === 'failed'
                        ? (entry.error ?? 'failed')
                        : `${entry.pages.length} pages`}
                  </small>
                </span>
                {entry.kind === 'search' && entry.pages.length ? (
                  <ul className="rules-pages">
                    {entry.pages.map((page) => (
                      <li key={page.url}>
                        <ExternalLink href={page.url}>{page.title || page.url}</ExternalLink>
                        <Button size="compact" variant="ghost" onClick={() => setDrafting(page)}>
                          Draft a rule from this page
                        </Button>
                      </li>
                    ))}
                  </ul>
                ) : null}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {error ? (
        <p role="alert" className="transaction-edit-note">
          {error}
        </p>
      ) : null}
    </Card>
  )
}

const templates: Record<ProgramTemplate, string> = {
  bonusOffsetsFee: 'A deposit that repays a card’s annual fee',
  employeeDepositWaiver: 'A monthly fee waived for employees with enough deposits',
}
const templateIds: ProgramTemplate[] = ['bonusOffsetsFee', 'employeeDepositWaiver']

/**
 * Drafting a rule beside the page it comes from. Rust builds and checks the rule; it joins
 * settings only when you approve it, and is matched by the plain words you type.
 */
function DraftSheet({ page, onDone }: { page: ResearchPage; onDone: () => void }) {
  const { updateSettings } = useFinanceEdits()
  const [template, setTemplate] = useState<ProgramTemplate>('employeeDepositWaiver')
  const [fields, setFields] = useState<Record<string, string>>({
    name: '',
    organization: '',
    summary: '',
    effective: '',
    accountName: '',
    institution: '',
    cardName: '',
    bonusWords: '',
    amount: '',
    fee: '',
    unobservable: '',
  })
  const [draft, setDraft] = useState<Program>()
  const [error, setError] = useState<string>()
  const field = (key: string, label: string, type = 'text') => (
    <label key={key}>
      <span>{label}</span>
      <input
        type={type}
        value={fields[key]}
        onChange={(event) => {
          setDraft(undefined)
          setFields({ ...fields, [key]: event.target.value })
        }}
      />
    </label>
  )
  const check = async (event: FormEvent) => {
    event.preventDefault()
    setError(undefined)
    try {
      setDraft(
        await draftProgram({
          template,
          name: fields.name,
          organization: fields.organization,
          summary: fields.summary,
          sourceUrl: page.url,
          sourceTitle: page.title || page.url,
          effective: fields.effective,
          accountName: fields.accountName,
          institution: fields.institution,
          amount: Number(fields.amount),
          ...(template === 'bonusOffsetsFee'
            ? { cardName: fields.cardName, bonusWords: fields.bonusWords }
            : {
                fee: Number(fields.fee),
                ...(fields.unobservable ? { unobservable: fields.unobservable } : {}),
              }),
        }),
      )
    } catch (caught) {
      setError(errorMessage(caught))
    }
  }
  const approve = async () => {
    if (!draft) return
    try {
      await updateSettings((current) => ({
        ...current,
        programs: [...(current.programs ?? []).filter((known) => known.id !== draft.id), draft],
      }))
      onDone()
    } catch (caught) {
      setError(errorMessage(caught))
    }
  }
  const form = useRef<HTMLFormElement>(null)
  const latest = useRef(approve)
  latest.current = approve
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'rules.draft.check',
          title: 'Check the drafted rule',
          search: 'rule draft check research',
          group: 'Here',
          scope: 'page',
          run: () => form.current?.requestSubmit(),
        },
        {
          id: 'rules.draft.approve',
          title: 'Approve the drafted rule',
          search: 'rule draft approve save research',
          group: 'Here',
          scope: 'page',
          disabled: !draft,
          run: () => void latest.current(),
        },
        {
          id: 'rules.draft.cancel',
          title: 'Cancel the draft',
          search: 'rule draft cancel',
          group: 'Here',
          scope: 'page',
          run: onDone,
        },
      ],
      [draft, onDone],
    ),
  )
  return (
    <section className="rules-draft">
      <h3>Draft a rule</h3>
      <div className="rules-draft-grid">
        <blockquote className="rules-page-text">
          <ExternalLink href={page.url}>{page.title || page.url}</ExternalLink>
          <p>{page.text || 'Exa returned no text for this page.'}</p>
        </blockquote>
        <form ref={form} className="chapter-edit" onSubmit={(event) => void check(event)}>
          <label>
            <span>Kind</span>
            <select
              value={template}
              onChange={(event) => {
                const chosen = templateIds.find((id) => id === event.target.value)
                if (chosen) setTemplate(chosen)
              }}
            >
              {templateIds.map((id) => (
                <option key={id} value={id}>
                  {templates[id]}
                </option>
              ))}
            </select>
          </label>
          {field('name', 'Rule name')}
          {field(
            'organization',
            template === 'employeeDepositWaiver' ? 'Employer' : 'Organization',
          )}
          {field('summary', 'What the page says')}
          {field('effective', 'Terms begin', 'date')}
          {field('accountName', 'Account name')}
          {field('institution', 'Institution')}
          {template === 'bonusOffsetsFee' ? (
            <>
              {field('cardName', 'Card’s official name')}
              {field('bonusWords', 'Words the bonus posts under')}
              {field('amount', 'Bonus amount', 'number')}
            </>
          ) : (
            <>
              {field('amount', 'Monthly deposits needed', 'number')}
              {field('fee', 'Monthly fee waived', 'number')}
              {field('unobservable', 'A condition no record shows (optional)')}
            </>
          )}
          <div className="chapter-actions">
            <Button type="submit" size="compact" data-command="rules.draft.check">
              Check
            </Button>
            <Button
              type="button"
              size="compact"
              variant="primary"
              icon={Check}
              data-command="rules.draft.approve"
              disabled={!draft}
              onClick={() => void approve()}
            >
              Approve rule
            </Button>
            <Button type="button" size="compact" data-command="rules.draft.cancel" onClick={onDone}>
              Cancel
            </Button>
          </div>
          {draft ? (
            <p className="transaction-edit-note">
              Checked: {draft.conditions.length} conditions, citing {draft.source.url}.
            </p>
          ) : null}
          {error ? (
            <p role="alert" className="transaction-edit-note">
              {error}
            </p>
          ) : null}
        </form>
      </div>
    </section>
  )
}
