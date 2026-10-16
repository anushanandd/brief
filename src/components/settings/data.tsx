import { useQuery, useQueryClient } from '@tanstack/react-query'
import { open, save } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import { toast } from 'sonner'

import { financeQueryKey } from '../../hooks/use-finance'
import { useFinance } from '../../hooks/use-finance'
import { useFindPlaces } from '../../hooks/use-find-places'
import {
  authenticateSensitiveAction,
  eraseFinanceState,
  exportFinanceBackup,
  exportSettingsFile,
  getFoundationModelStatus,
  inspectFinanceBackup,
  isTauri,
  readSettingsFile,
  restoreFinanceBackup,
} from '../../lib/api'
import { errorMessage } from '../../lib/errors'
import { exportLotsCsv, exportUpcomingCalendar } from '../../lib/exports'
import { countOf } from '../../lib/format'
import { getExternalLogosEnabled, saveExternalLogosEnabled } from '../../lib/logos'
import { getPrivacyPreferences, savePrivacyPreferences } from '../../lib/privacy-preferences'
import type { BackupInfo } from '../../lib/schema'
import {
  dataPreferenceKeys,
  exportSettingsPreferences,
  importSettingsPreferences,
  parseSettingsPreferences,
} from '../../lib/settings-preferences'
import {
  CalendarDays,
  Download,
  Fingerprint,
  MapPinned,
  RotateCcw,
  ShieldCheck,
  Trash2,
  Upload,
  X,
} from '../icons'
import {
  SettingsBlock,
  SettingsCard,
  SettingsResetButton,
  SettingsRow,
  SettingsSection,
  SettingsSwitch,
} from '../settings-ui'
import { Button, StatusDot } from '../ui'

/**
 * Runs a backup or file task, marking it busy until it ends and toasting `failure` with the
 * reason if it fails.
 */
async function runTask<T>(
  setBusy: (busy: T | null) => void,
  busy: T,
  failure: string,
  task: () => Promise<void>,
) {
  setBusy(busy)
  try {
    await task()
  } catch (error) {
    toast.error(failure, { description: errorMessage(error) })
  } finally {
    setBusy(null)
  }
}

export function DataSection() {
  return (
    <SettingsSection id="data" title="Backup & data">
      <SettingsCard>
        <FinancialBackup />
        <PreferencesFile />
        <Exports />
        <Places />
        <SettingsBlock id="local-storage" title="Local storage">
          <dl className="data-inventory">
            <div>
              <dt>
                <ShieldCheck size={15} aria-hidden="true" />
                Finance database
              </dt>
              <dd>Snapshot, caches, your edits, and sync history</dd>
            </div>
            <div>
              <dt>
                <Fingerprint size={15} aria-hidden="true" />
                macOS Keychain
              </dt>
              <dd>Credentials and access tokens</dd>
            </div>
            <div>
              <dt>
                <RotateCcw size={15} aria-hidden="true" />
                Disposable caches
              </dt>
              <dd>Prices, news, earnings, and places</dd>
            </div>
          </dl>
        </SettingsBlock>
      </SettingsCard>
      <SettingsCard>
        <EraseData />
      </SettingsCard>
    </SettingsSection>
  )
}

/** Saving a backup of all financial data, and restoring one after validating it. */
function FinancialBackup() {
  const queryClient = useQueryClient()
  const [busy, setBusy] = useState<'export' | 'inspect' | 'restore' | null>(null)
  const [pendingRestore, setPendingRestore] = useState<{ path: string; info: BackupInfo } | null>(
    null,
  )
  const documents = useFinance().data?.documents?.length ?? 0
  // Seals the documents' key into the backup, so another Mac can open them.
  const [passphrase, setPassphrase] = useState('')
  const [restorePassphrase, setRestorePassphrase] = useState('')

  const exportBackup = () =>
    runTask(setBusy, 'export', 'Could not save backup', async () => {
      await authenticateSensitiveAction()
      const date = new Date().toISOString().slice(0, 10)
      const path = await save({
        defaultPath: `Brief ${date}.briefbackup`,
        filters: [{ name: 'Brief backup', extensions: ['briefbackup'] }],
      })
      if (!path) return
      await exportFinanceBackup(path, passphrase)
      setPassphrase('')
      toast.success('Financial backup saved')
    })

  const chooseBackup = () =>
    runTask(setBusy, 'inspect', 'Could not read backup', async () => {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Brief backup', extensions: ['briefbackup', 'sqlite3', 'db'] }],
      })
      if (!path) return
      setPendingRestore({ path, info: await inspectFinanceBackup(path) })
    })

  const restoreBackup = (path: string) =>
    runTask(setBusy, 'restore', 'Could not restore backup', async () => {
      await authenticateSensitiveAction()
      const snapshot = await restoreFinanceBackup(path, restorePassphrase)
      queryClient.setQueryData(financeQueryKey, snapshot)
      setPendingRestore(null)
      setRestorePassphrase('')
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['transaction-annotations'] }),
        queryClient.invalidateQueries({ queryKey: ['provider-connections'] }),
        queryClient.invalidateQueries({ queryKey: ['data-health'] }),
        queryClient.invalidateQueries({ queryKey: ['portfolio-performance'] }),
        queryClient.invalidateQueries({ queryKey: ['market-snapshots'] }),
      ])
      toast.success('Backup restored')
    })

  return (
    <SettingsBlock
      id="financial-backup"
      title="Financial backup"
      description="All financial data. No credentials."
      action={
        <span className="settings-button-group">
          <Button
            icon={Download}
            size="compact"
            disabled={!isTauri() || busy !== null}
            onClick={() => void exportBackup()}
          >
            {busy === 'export' ? 'Saving…' : 'Save backup'}
          </Button>
          <Button
            icon={Upload}
            size="compact"
            disabled={!isTauri() || busy !== null}
            onClick={() => void chooseBackup()}
          >
            {busy === 'inspect' ? 'Validating…' : 'Restore…'}
          </Button>
        </span>
      }
    >
      <p className="settings-caution">
        Figures in a backup are unencrypted, so store it somewhere private. Documents stay encrypted
        with this Mac’s key.
      </p>
      {documents ? (
        <label className="settings-inline-field">
          <span>Passphrase for documents on another Mac (optional, 8 or more characters)</span>
          <input
            type="password"
            className="settings-number-input"
            autoComplete="new-password"
            aria-label="Backup passphrase for documents"
            value={passphrase}
            onChange={(event) => setPassphrase(event.target.value)}
          />
        </label>
      ) : null}
      {pendingRestore ? (
        <div className="backup-confirmation" role="group" aria-label="Confirm backup restore">
          <div>
            <strong>Replace current local financial data?</strong>
            <p>
              Data through {new Date(pendingRestore.info.updatedAt).toLocaleString()} ·{' '}
              {pendingRestore.info.accounts} accounts · {pendingRestore.info.transactions}{' '}
              activities · {pendingRestore.info.holdings} holdings
              {pendingRestore.info.documents ? ` · ${pendingRestore.info.documents} documents` : ''}
            </p>
            <small>Validated. Credentials and preferences unchanged.</small>
            {pendingRestore.info.documentKey === 'passphrase' ? (
              <label className="settings-inline-field">
                <span>Its documents were saved on another Mac. Enter the backup’s passphrase:</span>
                <input
                  type="password"
                  className="settings-number-input"
                  aria-label="Backup passphrase"
                  value={restorePassphrase}
                  onChange={(event) => setRestorePassphrase(event.target.value)}
                />
              </label>
            ) : pendingRestore.info.documentKey === 'otherMac' ? (
              <small>
                Its documents were saved on another Mac without a passphrase: what Brief read from
                them comes back, but the originals won’t open here.
              </small>
            ) : null}
          </div>
          <div className="settings-button-group">
            <Button
              icon={X}
              size="compact"
              disabled={busy === 'restore'}
              onClick={() => setPendingRestore(null)}
            >
              Cancel
            </Button>
            <Button
              icon={Fingerprint}
              size="compact"
              variant="destructive"
              disabled={busy === 'restore'}
              onClick={() => void restoreBackup(pendingRestore.path)}
            >
              {busy === 'restore' ? 'Authenticating…' : 'Authenticate and restore'}
            </Button>
          </div>
        </div>
      ) : null}
    </SettingsBlock>
  )
}

/** These settings as a file, to move them to another Mac. */
function PreferencesFile() {
  const [busy, setBusy] = useState<'export' | 'import' | null>(null)

  const exportPreferences = () =>
    runTask(setBusy, 'export', 'Could not export preferences', async () => {
      const path = await save({
        defaultPath: 'Brief preferences.briefsettings',
        filters: [{ name: 'Brief preferences', extensions: ['briefsettings'] }],
      })
      if (!path) return
      await exportSettingsFile(path, JSON.stringify(exportSettingsPreferences(), null, 2))
      toast.success('Preferences exported')
    })

  const importPreferences = () =>
    runTask(setBusy, 'import', 'Could not import preferences', async () => {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Brief preferences', extensions: ['briefsettings', 'json'] }],
      })
      if (!path) return
      const file = parseSettingsPreferences(JSON.parse(await readSettingsFile(path)))
      importSettingsPreferences(file)
      window.location.reload()
    })

  return (
    <SettingsRow id="preferences-file" title="Preferences file" description="These settings only.">
      <span className="settings-button-group">
        <Button
          icon={Download}
          size="compact"
          disabled={!isTauri() || busy !== null}
          onClick={() => void exportPreferences()}
        >
          {busy === 'export' ? 'Exporting…' : 'Export'}
        </Button>
        <Button
          icon={Upload}
          size="compact"
          disabled={!isTauri() || busy !== null}
          onClick={() => void importPreferences()}
        >
          {busy === 'import' ? 'Importing…' : 'Import…'}
        </Button>
      </span>
    </SettingsRow>
  )
}

/** Files other apps read: upcoming dates for a calendar, and open tax lots for an accountant. */
function Exports() {
  const finance = useFinance()
  const [busy, setBusy] = useState<'calendar' | 'lots' | null>(null)
  const data = finance.data
  const hasLots = Boolean(data?.insights?.lots.some((position) => position.lots.length))
  return (
    <SettingsRow
      id="exports"
      title="Exports"
      description="Bills, statement closes, and dividends as a calendar, with no amounts. Open tax lots as CSV."
    >
      <span className="settings-button-group">
        <Button
          icon={CalendarDays}
          size="compact"
          data-command="finance.export-calendar"
          disabled={!data?.insights || busy !== null}
          onClick={() =>
            data &&
            void runTask(setBusy, 'calendar', 'Could not save the calendar', async () => {
              await exportUpcomingCalendar(data)
            })
          }
        >
          {busy === 'calendar' ? 'Saving…' : 'Calendar'}
        </Button>
        <Button
          icon={Download}
          size="compact"
          data-command="investments.export-lots"
          disabled={!hasLots || busy !== null}
          onClick={() =>
            data &&
            void runTask(setBusy, 'lots', 'Could not save the lots', async () => {
              await exportLotsCsv(data)
            })
          }
        >
          {busy === 'lots' ? 'Saving…' : 'Tax lots'}
        </Button>
      </span>
    </SettingsRow>
  )
}

/**
 * Matching purchases made in person to their business with Overture Maps. Only each city's
 * area is downloaded; merchant names never leave the Mac.
 */
function Places() {
  const data = useFinance().data
  const status = data?.places
  const places = useFindPlaces()
  const summary = places.running
    ? places.total
      ? `Finding places… ${places.done} of ${places.total} cities`
      : 'Finding places…'
    : status
      ? `${countOf(status.matched, 'purchase')} matched of ${status.inPerson} made in person` +
        (status.unsearched ? ` · ${countOf(status.unsearched, 'place')} to look up` : '')
      : 'Unavailable until a snapshot loads'
  return (
    <SettingsRow
      id="places"
      title="Places"
      description="Finds the business behind each purchase made in person, from Overture Maps. Downloads each city’s area, about 10 MB; merchant names stay on this Mac."
    >
      <span className="settings-button-group">
        <small aria-live="polite">{summary}</small>
        <Button
          icon={MapPinned}
          size="compact"
          data-command="places.find"
          disabled={!isTauri() || places.running || Boolean(data?.recovery) || !status?.unsearched}
          aria-busy={places.running || undefined}
          onClick={() =>
            void places
              .start()
              .catch((error: unknown) =>
                toast.error('Could not find places', { description: errorMessage(error) }),
              )
          }
        >
          {places.running ? 'Finding…' : 'Find places'}
        </Button>
      </span>
    </SettingsRow>
  )
}

/** Erasing local financial data and backups after authenticating; credentials stay. */
function EraseData() {
  const queryClient = useQueryClient()
  const [confirming, setConfirming] = useState(false)
  const [erasing, setErasing] = useState<'erase' | null>(null)

  const eraseData = () =>
    runTask(setErasing, 'erase', 'Could not erase local data', async () => {
      await authenticateSensitiveAction()
      const snapshot = await eraseFinanceState()
      queryClient.setQueryData(financeQueryKey, snapshot)
      queryClient.removeQueries({ queryKey: ['market-snapshots'] })
      queryClient.removeQueries({ queryKey: ['provider-connections'] })
      queryClient.removeQueries({ queryKey: ['sync-runs'] })
      setConfirming(false)
      toast.success('Data erased. Credentials kept.')
    })

  return (
    <SettingsBlock
      id="erase-data"
      title="Erase local financial data"
      description="Deletes local data and backups. Keeps credentials."
      action={
        confirming ? null : (
          <Button
            icon={Trash2}
            size="compact"
            variant="destructive"
            onClick={() => setConfirming(true)}
          >
            Erase…
          </Button>
        )
      }
    >
      {confirming ? (
        <div className="connection-confirmation" role="group" aria-label="Erase local data">
          <p>This can’t be undone without a backup.</p>
          <div>
            <Button
              icon={X}
              size="compact"
              disabled={erasing !== null}
              onClick={() => setConfirming(false)}
            >
              Cancel
            </Button>
            <Button
              icon={Trash2}
              size="compact"
              variant="destructive"
              disabled={!isTauri() || erasing !== null}
              onClick={() => void eraseData()}
            >
              {erasing ? 'Erasing…' : 'Authenticate and erase'}
            </Button>
          </div>
        </div>
      ) : null}
    </SettingsBlock>
  )
}

export function PrivacySection() {
  const foundationModel = useQuery({
    queryKey: ['foundation-model-status'],
    queryFn: getFoundationModelStatus,
    staleTime: Number.POSITIVE_INFINITY,
  })
  const [externalLogosEnabled, setExternalLogosEnabled] = useState(getExternalLogosEnabled)
  const [privacy, setPrivacy] = useState(getPrivacyPreferences)
  const updatePrivacy = (next: typeof privacy) => {
    savePrivacyPreferences(next)
    setPrivacy(next)
  }
  const model = foundationModel.data
  return (
    <SettingsSection
      id="privacy"
      title="Privacy"
      action={<SettingsResetButton section="privacy" keys={dataPreferenceKeys} />}
    >
      <SettingsCard>
        <SettingsRow
          id="external-logos"
          title="Logos"
          description="Sends tickers and domains to Logo.dev and Plaid."
        >
          <SettingsSwitch
            labelledBy="external-logos-title"
            describedBy="external-logos-description"
            checked={externalLogosEnabled}
            onCheckedChange={(checked) => {
              saveExternalLogosEnabled(checked)
              setExternalLogosEnabled(checked)
            }}
          />
        </SettingsRow>
        <SettingsRow id="hide-on-launch" title="Hide values when Brief opens">
          <SettingsSwitch
            labelledBy="hide-on-launch-title"

            checked={privacy.hideOnLaunch}
            onCheckedChange={(hideOnLaunch) => updatePrivacy({ ...privacy, hideOnLaunch })}
          />
        </SettingsRow>
        <SettingsRow
          id="hide-on-blur"
          title="Hide values when the window loses focus"
          description="Reveal with ⌘H."
        >
          <SettingsSwitch
            labelledBy="hide-on-blur-title"
            describedBy="hide-on-blur-description"
            checked={privacy.hideOnBlur}
            onCheckedChange={(hideOnBlur) => updatePrivacy({ ...privacy, hideOnBlur })}
          />
        </SettingsRow>
        <SettingsRow
          id="apple-intelligence"
          title="Apple Intelligence"
          description="On-device answers. Read-only."
        >
          <span className="provider-card-status">
            <StatusDot tone={model?.state === 'available' ? 'positive' : 'neutral'} />
            {model?.state === 'available'
              ? 'Available'
              : // The row already names Apple Intelligence; don't repeat it in the status.
                (model?.message?.replace(/^Apple Intelligence requires/, 'Requires') ??
                (foundationModel.isLoading ? 'Checking…' : 'Unavailable'))}
          </span>
        </SettingsRow>
      </SettingsCard>
    </SettingsSection>
  )
}
