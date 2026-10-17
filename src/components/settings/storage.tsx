import { useQuery, useQueryClient } from '@tanstack/react-query'
import { open, save } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import { toast } from 'sonner'

import { useFinance } from '../../hooks/use-finance'
import { useFinanceEdits } from '../../hooks/use-finance-edits'
import { isTauri } from '../../lib/api'
import { errorMessage } from '../../lib/errors'
import { financeSettings } from '../../lib/finance-settings'
import { amountsHidden, countOf, formatUpdatedAt } from '../../lib/format'
import {
  exportDataFolder,
  getDataFolderStatus,
  getStoredFiles,
  getStoredRows,
  writeDataFolder,
} from '../../lib/storage-files'
import { Download, FolderPlus } from '../icons'
import { SettingsBlock, SettingsRow, SettingsSwitch } from '../settings-ui'
import { Button, StatusDot } from '../ui'

const PAGE = 100

function bytes(size: number) {
  if (size < 1024) return `${size} B`
  const units = ['KB', 'MB', 'GB']
  let value = size / 1024
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return `${value.toFixed(value < 10 ? 1 : 0)} ${units[unit]}`
}

/**
 * Every file in Brief's data folder, what it is for, and each database's tables, any of which
 * opens read-only. A file Brief doesn't know is flagged, so nothing sits there unaccounted
 * for. Credentials stay in Keychain and only their existence shows.
 */
export function StoredFiles() {
  const files = useQuery({ queryKey: ['stored-files'], queryFn: getStoredFiles })
  const [table, setTable] = useState<{ file: string; table: string } | null>(null)
  return (
    <SettingsBlock
      id="local-storage"
      title="Local storage"
      description="Every file Brief keeps on this Mac. Open a table to read its rows."
    >
      {!isTauri() ? (
        <p className="settings-copy">Files are listed in the app.</p>
      ) : files.isError ? (
        <p className="settings-copy">{errorMessage(files.error)}</p>
      ) : (
        <dl className="data-inventory stored-files">
          {(files.data ?? []).map((file) => (
            <div key={file.name}>
              <dt>
                {file.purpose ? null : (
                  <>
                    <StatusDot tone="warning" />
                    <span className="sr-only">Unknown file:</span>
                  </>
                )}
                {file.name}
              </dt>
              <dd>
                <span>
                  {file.purpose ?? 'Not a file Brief knows'}
                  {file.directory ? '' : ` · ${bytes(file.bytes)}`}
                  {file.modified ? ` · ${formatUpdatedAt(file.modified)}` : ''}
                </span>
                {file.tables.length ? (
                  <span className="stored-tables">
                    {file.tables.map(({ name, rows }) => (
                      <button
                        type="button"
                        key={name}
                        className="object-link"
                        aria-pressed={table?.file === file.name && table.table === name}
                        onClick={() => setTable({ file: file.name, table: name })}
                      >
                        {name} ({countOf(rows, 'row')})
                      </button>
                    ))}
                  </span>
                ) : null}
              </dd>
            </div>
          ))}
          <div>
            <dt>macOS Keychain</dt>
            <dd>Credentials and access tokens; their values never show</dd>
          </div>
        </dl>
      )}
      {table ? <TableRows {...table} onClose={() => setTable(null)} /> : null}
    </SettingsBlock>
  )
}

/** One table's rows, a page at a time, as stored. Hidden while values are hidden. */
function TableRows({ file, table, onClose }: { file: string; table: string; onClose: () => void }) {
  const [offset, setOffset] = useState(0)
  const rows = useQuery({
    queryKey: ['stored-rows', file, table, offset],
    queryFn: () => getStoredRows(file, table, offset),
  })
  const total = rows.data?.total ?? 0
  return (
    <section className="stored-rows" aria-label={`${file} › ${table}`}>
      <header>
        <strong>
          {file} › {table}
        </strong>
        <span className="settings-button-group">
          <Button size="compact" disabled={offset === 0} onClick={() => setOffset(offset - PAGE)}>
            Previous
          </Button>
          <Button
            size="compact"
            disabled={offset + PAGE >= total}
            onClick={() => setOffset(offset + PAGE)}
          >
            Next
          </Button>
          <Button size="compact" variant="ghost" onClick={onClose}>
            Close
          </Button>
        </span>
      </header>
      {amountsHidden() ? (
        <p className="settings-copy">Rows are hidden while values are hidden.</p>
      ) : rows.isError ? (
        <p className="settings-copy">{errorMessage(rows.error)}</p>
      ) : rows.data ? (
        <>
          <p className="settings-copy">
            {total ? `Rows ${offset + 1}–${Math.min(offset + PAGE, total)} of ${total}` : 'No rows'}
          </p>
          <div className="stored-rows-scroll">
            <table>
              <thead>
                <tr>
                  {rows.data.columns.map((column) => (
                    <th key={column}>{column}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {rows.data.rows.map((row, index) => (
                  <tr key={offset + index}>
                    {row.map((cell, column) => (
                      <td key={column} className={cell === null ? 'raw-null' : undefined}>
                        {cell ?? 'null'}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      ) : (
        <p className="settings-copy">Loading…</p>
      )}
    </section>
  )
}

async function exportZip() {
  const path = await save({ defaultPath: 'Brief Data.zip', title: 'Export Brief Data' })
  if (!path) return
  try {
    await exportDataFolder(path)
    toast.success('Exported Brief Data')
  } catch (error) {
    toast.error('Could not export Brief Data', { description: errorMessage(error) })
  }
}

/**
 * The readable folder: a plain CSV and Markdown copy of everything Brief knows, written after
 * changes and never read back. Off until a folder is chosen; Brief warns when macOS may sync
 * that folder to the cloud.
 */
export function DataFolder() {
  const finance = useFinance()
  const { updateSettings } = useFinanceEdits()
  const queryClient = useQueryClient()
  const folder = financeSettings(finance.data).dataFolder
  const status = useQuery({
    queryKey: ['data-folder', folder?.path, folder?.masked],
    queryFn: getDataFolderStatus,
    enabled: isTauri(),
  })
  const [busy, setBusy] = useState(false)
  const unavailable = !isTauri() || Boolean(finance.data?.recovery)

  const change = async (next: typeof folder, failure: string) => {
    setBusy(true)
    try {
      await updateSettings((current) => ({ ...current, dataFolder: next }))
      queryClient.setQueryData(['data-folder', next?.path, next?.masked], await writeDataFolder())
    } catch (error) {
      toast.error(failure, { description: errorMessage(error) })
    } finally {
      setBusy(false)
    }
  }
  const choose = async () => {
    const path = await open({ directory: true, title: 'Choose where Brief Data goes' })
    if (typeof path === 'string')
      await change({ path, masked: folder?.masked ?? false }, 'Could not use that folder')
  }

  return (
    <>
      <SettingsRow
        id="data-folder"
        title="Readable folder"
        description={
          folder ? (
            <>
              {status.data?.folder ?? folder.path}
              {status.data?.writtenAt ? ` · Written ${formatUpdatedAt(status.data.writtenAt)}` : ''}
              {status.data?.error ? ` · ${status.data.error}` : ''}
              {status.data?.synced
                ? ' · macOS may sync this folder to the cloud, where these files would be copied too'
                : ''}
            </>
          ) : (
            'A plain CSV and Markdown copy of everything Brief knows, written after changes and never read back. Off.'
          )
        }
      >
        <span className="settings-button-group">
          <Button
            icon={FolderPlus}
            size="compact"
            disabled={unavailable || busy}
            onClick={() => void choose()}
          >
            {folder ? 'Move…' : 'Choose folder…'}
          </Button>
          {folder ? (
            <>
              <Button
                icon={Download}
                size="compact"
                disabled={unavailable || busy || !status.data?.writtenAt}
                onClick={() => void exportZip()}
              >
                Export as zip
              </Button>
              <Button
                size="compact"
                variant="ghost"
                disabled={unavailable || busy}
                onClick={() => void change(undefined, 'Could not turn off the folder')}
              >
                Turn off
              </Button>
            </>
          ) : null}
        </span>
      </SettingsRow>
      <SettingsRow
        id="data-folder-masked"
        title="Mask amounts in the folder"
        description="Every amount is written as •••. Choose a folder first."
      >
        <SettingsSwitch
          labelledBy="data-folder-masked-title"
          describedBy="data-folder-masked-description"
          checked={folder?.masked ?? false}
          disabled={unavailable || busy || !folder}
          onCheckedChange={(masked) =>
            folder && void change({ ...folder, masked }, 'Could not change the folder')
          }
        />
      </SettingsRow>
    </>
  )
}
