import { useQuery, useQueryClient } from '@tanstack/react-query'
import { open } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import { toast } from 'sonner'

import { financeQueryKey } from '../hooks/use-finance'
import { accountDisplayName, type AccountDisplayNames } from '../lib/account-names'
import {
  getStatementImports,
  importStatement,
  isTauri,
  previewStatementImport,
  removeStatementImport,
} from '../lib/api'
import { errorMessage } from '../lib/errors'
import { countOf, formatCurrency } from '../lib/format'
import type { FinanceSnapshot, StatementPreview } from '../lib/schema'
import {
  statementCheckLabel,
  statementCountsLabel,
  statementDateRange,
} from '../lib/statement-import'
import { FilterSelect } from './filter-select'
import { Trash2, Upload, X } from './icons'
import { SettingsBlock } from './settings-ui'
import { Button } from './ui'

export const statementImportsQueryKey = ['statement-imports'] as const

/** Imports statement files that fill account history from before a provider's first activity. */
export function StatementImports({
  data,
  accountNames,
}: {
  data: FinanceSnapshot
  accountNames: AccountDisplayNames
}) {
  const queryClient = useQueryClient()
  const imports = useQuery({
    queryKey: [...statementImportsQueryKey, data.revision],
    queryFn: getStatementImports,
    enabled: isTauri(),
  })
  const [choosing, setChoosing] = useState(false)
  const [preview, setPreview] = useState<StatementPreview | null>(null)
  const [accountId, setAccountId] = useState('')
  const [importing, setImporting] = useState(false)
  const [importError, setImportError] = useState<string>()
  const [confirmRemove, setConfirmRemove] = useState<string>()
  const [removing, setRemoving] = useState(false)
  const [removeError, setRemoveError] = useState<string>()

  const nameFor = (id: string) => {
    const account = data.accounts.find((candidate) => candidate.id === id)
    return account
      ? accountDisplayName(account.id, account.name, accountNames)
      : 'Unavailable account'
  }
  const committed = async (snapshot: FinanceSnapshot) => {
    queryClient.setQueryData(financeQueryKey, snapshot)
    await queryClient.invalidateQueries({ queryKey: statementImportsQueryKey })
    await queryClient.invalidateQueries({ queryKey: ['data-health'] })
  }

  const choose = async () => {
    setChoosing(true)
    try {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Statement', extensions: ['csv', 'xlsx', 'txt'] }],
      })
      if (!path) return
      const next = await previewStatementImport(path)
      setPreview(next)
      setAccountId(next.importedAccountId ?? next.suggestedAccountId ?? '')
      setImportError(undefined)
    } catch (error) {
      toast.error('Could not read statement', { description: errorMessage(error) })
    } finally {
      setChoosing(false)
    }
  }

  const confirmImport = async () => {
    if (!preview || !accountId) return
    setImporting(true)
    setImportError(undefined)
    try {
      await committed(await importStatement(preview.draftId, accountId))
      setPreview(null)
      toast.success('Statement imported')
    } catch (error) {
      setImportError(errorMessage(error))
    } finally {
      setImporting(false)
    }
  }

  const remove = async (id: string) => {
    setRemoving(true)
    setRemoveError(undefined)
    try {
      await committed(await removeStatementImport(id))
      setConfirmRemove(undefined)
      toast.success('Statement removed')
    } catch (error) {
      setRemoveError(errorMessage(error))
    } finally {
      setRemoving(false)
    }
  }

  const fit = preview?.accounts.find((candidate) => candidate.accountId === accountId)
  const blocked = preview?.check.status === 'failed'
  const alreadyImported = Boolean(preview?.importedAccountId)
  const saved = imports.data ?? []

  return (
    <SettingsBlock
      id="statement-history"
      title="Statement history"
      description="Fills history from before Plaid or SnapTrade."
      action={
        <Button
          icon={Upload}
          size="compact"
          disabled={!isTauri() || choosing || importing}
          aria-busy={choosing}
          onClick={() => void choose()}
        >
          {choosing ? 'Reading…' : 'Import…'}
        </Button>
      }
    >
      {preview ? (
        <div className="statement-preview" role="group" aria-label="Import statement">
          <div className="statement-preview-copy">
            <strong>{preview.fileName}</strong>
            <p>
              {[
                preview.label,
                statementDateRange(preview.firstDate, preview.lastDate),
                countOf(preview.rows, 'row'),
              ]
                .filter(Boolean)
                .join(' · ')}
            </p>
            <p>
              In <span className="sensitive-value">{formatCurrency(preview.moneyIn)}</span> · Out{' '}
              <span className="sensitive-value">{formatCurrency(preview.moneyOut)}</span>
            </p>
            <p className={`statement-check statement-check-${preview.check.status}`}>
              {statementCheckLabel(preview.check)}
              {preview.datesAssumed ? ' · Dates read month first' : ''}
            </p>
          </div>
          {preview.accounts.length ? (
            <FilterSelect
              label="Account for this statement"
              value={accountId}
              disabled={alreadyImported || importing}
              options={[
                ...(accountId ? [] : [{ value: '', label: 'Choose an account' }]),
                ...preview.accounts.map((candidate) => ({
                  value: candidate.accountId,
                  label: nameFor(candidate.accountId),
                })),
              ]}
              onValueChange={setAccountId}
            />
          ) : null}
          <small className="statement-preview-detail">
            {alreadyImported
              ? `Already imported to ${nameFor(preview.importedAccountId ?? '')}`
              : !preview.accounts.length
                ? 'Statements with trades need an investment account'
                : fit
                  ? statementCountsLabel(fit.counts, fit.accountId, fit.providerStart)
                  : 'Choose the account this statement is from'}
          </small>
          {importError ? (
            <p className="statement-preview-error" role="alert">
              {importError}
            </p>
          ) : null}
          <div className="settings-button-group">
            <Button
              icon={X}
              size="compact"
              disabled={importing}
              onClick={() => {
                setPreview(null)
                setImportError(undefined)
              }}
            >
              Cancel
            </Button>
            <Button
              icon={Upload}
              size="compact"
              variant="positive"
              disabled={importing || blocked || alreadyImported || !fit}
              aria-busy={importing}
              onClick={() => void confirmImport()}
            >
              {importing ? 'Importing…' : 'Import'}
            </Button>
          </div>
        </div>
      ) : null}
      {saved.length ? (
        <ul className="settings-list">
          {saved.map((statement) => (
            <li className="settings-list-row statement-row" key={statement.id}>
              <span className="settings-list-copy">
                <strong>{statement.fileName}</strong>
                <small>
                  {[
                    nameFor(statement.accountId),
                    statementDateRange(statement.firstDate, statement.lastDate),
                    statement.check
                      ? statementCountsLabel(statement.counts, statement.accountId)
                      : 'Can no longer be read',
                  ]
                    .filter(Boolean)
                    .join(' · ')}
                </small>
              </span>
              {confirmRemove === statement.id ? (
                <div className="connection-confirmation" role="group" aria-label="Remove statement">
                  <p>Its activity leaves your history. You can import it again.</p>
                  {removeError ? <p role="alert">{removeError}</p> : null}
                  <div>
                    <Button
                      icon={X}
                      size="compact"
                      disabled={removing}
                      onClick={() => {
                        setConfirmRemove(undefined)
                        setRemoveError(undefined)
                      }}
                    >
                      Cancel
                    </Button>
                    <Button
                      icon={Trash2}
                      size="compact"
                      variant="destructive"
                      disabled={removing}
                      aria-busy={removing}
                      onClick={() => void remove(statement.id)}
                    >
                      {removing ? 'Removing…' : 'Remove'}
                    </Button>
                  </div>
                </div>
              ) : (
                <Button
                  icon={Trash2}
                  size="compact"
                  disabled={removing || importing}
                  aria-label={`Remove ${statement.fileName}`}
                  onClick={() => {
                    setConfirmRemove(statement.id)
                    setRemoveError(undefined)
                  }}
                >
                  Remove…
                </Button>
              )}
            </li>
          ))}
        </ul>
      ) : imports.isError ? (
        <p className="statement-preview-error" role="alert">
          Couldn’t load imported statements
        </p>
      ) : null}
    </SettingsBlock>
  )
}
