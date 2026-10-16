import { useQuery, useQueryClient } from '@tanstack/react-query'
import { open as chooseFile } from '@tauri-apps/plugin-dialog'
import { useMemo, useRef, useState } from 'react'
import { toast } from 'sonner'

import { financeQueryKey } from '../hooks/use-finance'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import {
  getBankStatements,
  getStatementImports,
  importStatement,
  isTauri,
  openBankStatement,
  previewStatementImport,
} from '../lib/api'
import { errorMessage } from '../lib/errors'
import { formatShortDay } from '../lib/format'
import type { BankStatement, FinanceSnapshot, StatementPreview } from '../lib/schema'
import { statementCountsLabel, statementDateRange } from '../lib/statement-import'
import { Upload, X } from './icons'
import { SettingsLink } from './settings-dialog'
import { statementImportsQueryKey } from './statement-imports'
import { Button } from './ui'

const monthFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'long',
  year: 'numeric',
  timeZone: 'UTC',
})

/** `August 2026`. */
export const statementMonth = ({ year, month }: Pick<BankStatement, 'year' | 'month'>) =>
  monthFormatter.format(new Date(Date.UTC(year, month - 1, 1)))

const size = (bytes: number) =>
  bytes >= 1_000_000 ? `${(bytes / 1_000_000).toFixed(1)} MB` : `${Math.ceil(bytes / 1_000)} KB`

const open = (statement: BankStatement) =>
  void openBankStatement(statement.id).catch((error) =>
    toast.error('Could not open the statement', { description: errorMessage(error) }),
  )

/**
 * One account's statements: the bank's own PDFs (each opening in Preview) and the exports
 * imported into its history, with what each adds. A statement can be imported from here.
 */
export function AccountStatements({ accountId }: { accountId: string }) {
  const queryClient = useQueryClient()
  const bank = useQuery({ queryKey: ['bank-statements'], queryFn: getBankStatements })
  const imports = useQuery({
    queryKey: statementImportsQueryKey,
    queryFn: getStatementImports,
    enabled: isTauri(),
  })
  const detailsRef = useRef<HTMLDetailsElement>(null)
  const [preview, setPreview] = useState<StatementPreview>()
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string>()
  const saved = useMemo(
    () => (bank.data ?? []).filter((statement) => statement.accountId === accountId),
    [bank.data, accountId],
  )
  const imported = useMemo(
    () => (imports.data ?? []).filter((statement) => statement.accountId === accountId),
    [imports.data, accountId],
  )

  const choose = async () => {
    if (detailsRef.current) detailsRef.current.open = true
    setError(undefined)
    try {
      const path = await chooseFile({
        multiple: false,
        directory: false,
        filters: [{ name: 'Statement', extensions: ['csv', 'xlsx', 'txt'] }],
      })
      if (path) setPreview(await previewStatementImport(path))
    } catch (cause) {
      setError(errorMessage(cause))
    }
  }
  const fit = preview?.accounts.find((candidate) => candidate.accountId === accountId)
  const confirm = async () => {
    if (!preview) return
    setPending(true)
    setError(undefined)
    try {
      const snapshot: FinanceSnapshot = await importStatement(preview.draftId, accountId)
      queryClient.setQueryData(financeQueryKey, snapshot)
      await queryClient.invalidateQueries({ queryKey: statementImportsQueryKey })
      await queryClient.invalidateQueries({ queryKey: ['data-health'] })
      setPreview(undefined)
      toast.success('Statement imported')
    } catch (cause) {
      setError(errorMessage(cause))
    } finally {
      setPending(false)
    }
  }

  // The palette's action reads the latest `choose` without re-registering on every render.
  const chooseRef = useRef(choose)
  chooseRef.current = choose
  const actions = useMemo<AppAction[]>(
    () => [
      {
        id: 'account.statements.toggle',
        title: 'Show or hide statements',
        search: 'account statements pdf bank monthly imported',
        group: 'View',
        scope: 'page',
        run: () => {
          const details = detailsRef.current
          if (details) details.open = !details.open
        },
      },
      {
        id: 'account.statements.import',
        title: 'Import a statement into this account',
        search: 'import statement csv xlsx upload history',
        group: 'Actions',
        scope: 'page',
        disabled: !isTauri(),
        run: () => void chooseRef.current(),
      },
    ],
    [],
  )
  useRegisterActions(actions)

  const count = saved.length + imported.length
  return (
    <details ref={detailsRef} className="account-details-more account-statements">
      <summary data-command="account.statements.toggle">
        Statements {count ? <small>{count}</small> : null}
      </summary>
      <div
        className="account-statement-list"
        role="region"
        aria-label="Statements"
        tabIndex={0}
        data-keyboard-region
      >
        {imported.map((statement) => (
          <SettingsLink
            key={statement.id}
            target="statement-history"
            className="account-statement-row"
          >
            <strong>{statement.fileName}</strong>
            <small>
              {[
                statementDateRange(statement.firstDate, statement.lastDate),
                statement.check
                  ? statementCountsLabel(statement.counts, statement.accountId)
                  : 'Can no longer be read',
              ]
                .filter(Boolean)
                .join(' · ')}
            </small>
          </SettingsLink>
        ))}
        {saved.map((statement) => (
          <button
            type="button"
            key={statement.id}
            className="account-statement-row"
            data-keyboard-row
            data-keyboard-open
            onClick={() => open(statement)}
          >
            <strong>{statementMonth(statement)}</strong>
            <small>
              {statement.datePosted ? `Posted ${formatShortDay(statement.datePosted)} · ` : ''}
              {size(statement.bytes)}
            </small>
          </button>
        ))}
      </div>
      {preview ? (
        <div className="account-statement-import" role="group" aria-label="Import statement">
          <p>
            <strong>{preview.fileName}</strong>
            <small>
              {fit
                ? statementCountsLabel(fit.counts, accountId, fit.providerStart)
                : preview.importedAccountId
                  ? 'Already imported'
                  : 'This statement can’t go into this account'}
            </small>
          </p>
          <Button
            icon={X}
            size="compact"
            disabled={pending}
            data-command="account.statements.import"
            onClick={() => setPreview(undefined)}
          >
            Cancel
          </Button>
          <Button
            icon={Upload}
            size="compact"
            variant="positive"
            disabled={pending || !fit || preview.check.status === 'failed'}
            aria-busy={pending || undefined}
            data-command="account.statements.import"
            onClick={() => void confirm()}
          >
            {pending ? 'Importing…' : 'Import'}
          </Button>
        </div>
      ) : (
        <Button
          icon={Upload}
          size="compact"
          disabled={!isTauri()}
          data-command="account.statements.import"
          onClick={() => void choose()}
        >
          Import statement…
        </Button>
      )}
      {error ? (
        <p className="statement-preview-error" role="alert">
          {error}
        </p>
      ) : null}
    </details>
  )
}
