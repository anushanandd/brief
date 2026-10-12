import { useQuery, useQueryClient } from '@tanstack/react-query'
import { open, save } from '@tauri-apps/plugin-dialog'
import { useState } from 'react'
import { toast } from 'sonner'

import { financeQueryKey } from '../../hooks/use-finance'
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
import { getExternalLogosEnabled, saveExternalLogosEnabled } from '../../lib/logos'
import { getPrivacyPreferences, savePrivacyPreferences } from '../../lib/privacy-preferences'
import type { BackupInfo } from '../../lib/schema'
import {
  dataPreferenceKeys,
  exportSettingsPreferences,
  importSettingsPreferences,
  parseSettingsPreferences,
} from '../../lib/settings-preferences'
import { Download, Fingerprint, RotateCcw, ShieldCheck, Trash2, Upload, X } from '../icons'
import {
  SettingsBlock,
  SettingsCard,
  SettingsResetButton,
  SettingsRow,
  SettingsSection,
  SettingsSwitch,
} from '../settings-ui'
import { Button, StatusDot } from '../ui'

export function DataSection() {
  const queryClient = useQueryClient()
  const [backupBusy, setBackupBusy] = useState<'export' | 'inspect' | 'restore' | null>(null)
  const [settingsBusy, setSettingsBusy] = useState<'export' | 'import' | null>(null)
  const [pendingRestore, setPendingRestore] = useState<{ path: string; info: BackupInfo } | null>(
    null,
  )
  const [confirmErase, setConfirmErase] = useState(false)
  const [erasing, setErasing] = useState(false)

  const exportBackup = async () => {
    if (!isTauri()) return
    setBackupBusy('export')
    try {
      await authenticateSensitiveAction()
      const date = new Date().toISOString().slice(0, 10)
      const path = await save({
        defaultPath: `Brief ${date}.briefbackup`,
        filters: [{ name: 'Brief backup', extensions: ['briefbackup'] }],
      })
      if (!path) return
      await exportFinanceBackup(path)
      toast.success('Financial backup saved')
    } catch (error) {
      toast.error('Could not save backup', {
        description: errorMessage(error),
      })
    } finally {
      setBackupBusy(null)
    }
  }

  const chooseBackup = async () => {
    if (!isTauri()) return
    setBackupBusy('inspect')
    try {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Brief backup', extensions: ['briefbackup', 'sqlite3', 'db'] }],
      })
      if (!path) return
      setPendingRestore({ path, info: await inspectFinanceBackup(path) })
    } catch (error) {
      toast.error('Could not read backup', {
        description: errorMessage(error),
      })
    } finally {
      setBackupBusy(null)
    }
  }

  const restoreBackup = async () => {
    if (!pendingRestore) return
    setBackupBusy('restore')
    try {
      await authenticateSensitiveAction()
      const snapshot = await restoreFinanceBackup(pendingRestore.path)
      queryClient.setQueryData(financeQueryKey, snapshot)
      setPendingRestore(null)
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['transaction-annotations'] }),
        queryClient.invalidateQueries({ queryKey: ['provider-connections'] }),
        queryClient.invalidateQueries({ queryKey: ['data-health'] }),
        queryClient.invalidateQueries({ queryKey: ['portfolio-performance'] }),
        queryClient.invalidateQueries({ queryKey: ['market-snapshots'] }),
      ])
      toast.success('Backup restored')
    } catch (error) {
      toast.error('Could not restore backup', {
        description: errorMessage(error),
      })
    } finally {
      setBackupBusy(null)
    }
  }

  const exportPreferences = async () => {
    if (!isTauri()) return
    setSettingsBusy('export')
    try {
      const path = await save({
        defaultPath: 'Brief preferences.briefsettings',
        filters: [{ name: 'Brief preferences', extensions: ['briefsettings'] }],
      })
      if (!path) return
      await exportSettingsFile(path, JSON.stringify(exportSettingsPreferences(), null, 2))
      toast.success('Preferences exported')
    } catch (error) {
      toast.error('Could not export preferences', {
        description: errorMessage(error),
      })
    } finally {
      setSettingsBusy(null)
    }
  }

  const importPreferences = async () => {
    if (!isTauri()) return
    setSettingsBusy('import')
    try {
      const path = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Brief preferences', extensions: ['briefsettings', 'json'] }],
      })
      if (!path) return
      const file = parseSettingsPreferences(JSON.parse(await readSettingsFile(path)))
      importSettingsPreferences(file)
      window.location.reload()
    } catch (error) {
      toast.error('Could not import preferences', {
        description: errorMessage(error),
      })
    } finally {
      setSettingsBusy(null)
    }
  }

  const eraseData = async () => {
    setErasing(true)
    try {
      await authenticateSensitiveAction()
      const snapshot = await eraseFinanceState()
      queryClient.setQueryData(financeQueryKey, snapshot)
      queryClient.removeQueries({ queryKey: ['market-snapshots'] })
      queryClient.removeQueries({ queryKey: ['provider-connections'] })
      queryClient.removeQueries({ queryKey: ['sync-runs'] })
      setConfirmErase(false)
      toast.success('Data erased. Credentials kept.')
    } catch (error) {
      toast.error('Could not erase local data', {
        description: errorMessage(error),
      })
    } finally {
      setErasing(false)
    }
  }

  return (
    <SettingsSection id="data" title="Backup & data">
      <SettingsCard>
        <SettingsBlock
          id="financial-backup"
          title="Financial backup"
          description="All financial data. No credentials."
          action={
            <span className="settings-button-group">
              <Button
                icon={Download}
                size="compact"
                disabled={!isTauri() || backupBusy !== null}
                onClick={() => void exportBackup()}
              >
                {backupBusy === 'export' ? 'Saving…' : 'Save backup'}
              </Button>
              <Button
                icon={Upload}
                size="compact"
                disabled={!isTauri() || backupBusy !== null}
                onClick={() => void chooseBackup()}
              >
                {backupBusy === 'inspect' ? 'Validating…' : 'Restore…'}
              </Button>
            </span>
          }
        >
          <p className="settings-caution">Backups are unencrypted. Store them somewhere private.</p>
          {pendingRestore ? (
            <div className="backup-confirmation" role="group" aria-label="Confirm backup restore">
              <div>
                <strong>Replace current local financial data?</strong>
                <p>
                  Data through {new Date(pendingRestore.info.updatedAt).toLocaleString()} ·{' '}
                  {pendingRestore.info.accounts} accounts · {pendingRestore.info.transactions}{' '}
                  activities · {pendingRestore.info.holdings} holdings
                </p>
                <small>Validated. Credentials and preferences unchanged.</small>
              </div>
              <div className="settings-button-group">
                <Button
                  icon={X}
                  size="compact"
                  disabled={backupBusy === 'restore'}
                  onClick={() => setPendingRestore(null)}
                >
                  Cancel
                </Button>
                <Button
                  icon={Fingerprint}
                  size="compact"
                  variant="destructive"
                  disabled={backupBusy === 'restore'}
                  onClick={() => void restoreBackup()}
                >
                  {backupBusy === 'restore' ? 'Authenticating…' : 'Authenticate and restore'}
                </Button>
              </div>
            </div>
          ) : null}
        </SettingsBlock>
        <SettingsRow
          id="preferences-file"
          title="Preferences file"
          description="These settings only."
        >
          <span className="settings-button-group">
            <Button
              icon={Download}
              size="compact"
              disabled={!isTauri() || settingsBusy !== null}
              onClick={() => void exportPreferences()}
            >
              {settingsBusy === 'export' ? 'Exporting…' : 'Export'}
            </Button>
            <Button
              icon={Upload}
              size="compact"
              disabled={!isTauri() || settingsBusy !== null}
              onClick={() => void importPreferences()}
            >
              {settingsBusy === 'import' ? 'Importing…' : 'Import…'}
            </Button>
          </span>
        </SettingsRow>
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
              <dd>Prices, news, and earnings</dd>
            </div>
          </dl>
        </SettingsBlock>
      </SettingsCard>

      <SettingsCard>
        <SettingsBlock
          id="erase-data"
          title="Erase local financial data"
          description="Deletes local data and backups. Keeps credentials."
          action={
            confirmErase ? null : (
              <Button
                icon={Trash2}
                size="compact"
                variant="destructive"
                onClick={() => setConfirmErase(true)}
              >
                Erase…
              </Button>
            )
          }
        >
          {confirmErase ? (
            <div className="connection-confirmation" role="group" aria-label="Erase local data">
              <p>This can’t be undone without a backup.</p>
              <div>
                <Button
                  icon={X}
                  size="compact"
                  disabled={erasing}
                  onClick={() => setConfirmErase(false)}
                >
                  Cancel
                </Button>
                <Button
                  icon={Trash2}
                  size="compact"
                  variant="destructive"
                  disabled={!isTauri() || erasing}
                  onClick={() => void eraseData()}
                >
                  {erasing ? 'Erasing…' : 'Authenticate and erase'}
                </Button>
              </div>
            </div>
          ) : null}
        </SettingsBlock>
      </SettingsCard>
    </SettingsSection>
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
