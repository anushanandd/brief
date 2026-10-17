import { invoke } from '@tauri-apps/api/core'

import { isTauri } from './api'
import type { DataFolderStatus } from './bindings/DataFolderStatus'
import type { StoredFile } from './bindings/StoredFile'
import type { StoredRows } from './bindings/StoredRows'

/**
 * What Brief keeps on disk, for Settings › Backup & data: every file in its data folder with
 * each database's tables, a read-only page of any table, and the readable folder it writes.
 * Only the app has files; the preview lists none.
 */
export type { DataFolderStatus, StoredFile, StoredRows }

export async function getStoredFiles(): Promise<StoredFile[]> {
  if (!isTauri()) return []
  return invoke<StoredFile[]>('get_stored_files')
}

/** Up to 100 rows of one table, from `offset`. */
export async function getStoredRows(
  file: string,
  table: string,
  offset: number,
): Promise<StoredRows> {
  return invoke<StoredRows>('get_stored_rows', { file, table, offset })
}

export async function getDataFolderStatus(): Promise<DataFolderStatus> {
  if (!isTauri()) return { synced: false }
  return invoke<DataFolderStatus>('get_data_folder_status')
}

/** Writes the readable folder now, as after it is turned on or moved. */
export async function writeDataFolder(): Promise<DataFolderStatus> {
  return invoke<DataFolderStatus>('write_data_folder')
}

/** Zips the readable folder to `path`, a new file. */
export async function exportDataFolder(path: string): Promise<void> {
  await invoke('export_data_folder', { path })
}
