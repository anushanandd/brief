//! Files the user picks: read within a size bound, and written only as new files that only this
//! user can read, so a mistaken choice never overwrites anything.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
};

const EXISTING: &str = "Choose a new filename; existing files are never overwritten";

/// Creates `path` as a new file readable only by this user.
pub fn create_private(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|_| EXISTING.to_string())
}

/// Writes `contents` to a new private file. A failed write removes the partial file and
/// reports `failure`.
pub fn write_private(path: &Path, contents: &[u8], failure: &str) -> Result<(), String> {
    let mut file = create_private(path)?;
    if file
        .write_all(contents)
        .and_then(|_| file.sync_all())
        .is_err()
    {
        let _ = fs::remove_file(path);
        return Err(failure.into());
    }
    Ok(())
}

#[derive(Debug, PartialEq)]
pub enum ReadFailure {
    Unreadable,
    /// Empty, larger than allowed, or not a regular file.
    OutOfBounds,
}

/// A regular file of 1 to `max` bytes. The size is checked before and while reading, so a file
/// that grows meanwhile is still refused.
pub fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>, ReadFailure> {
    let metadata = fs::metadata(path).map_err(|_| ReadFailure::Unreadable)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > max as u64 {
        return Err(ReadFailure::OutOfBounds);
    }
    let mut contents = Vec::new();
    File::open(path)
        .and_then(|file| file.take(max as u64 + 1).read_to_end(&mut contents))
        .map_err(|_| ReadFailure::Unreadable)?;
    if contents.is_empty() || contents.len() > max {
        return Err(ReadFailure::OutOfBounds);
    }
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_only_new_private_files_and_reads_within_bounds() {
        let directory = std::env::temp_dir().join(format!("brief-files-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("preferences.briefsettings");
        write_private(&path, b"{}", "failed").unwrap();
        assert_eq!(
            write_private(&path, b"[]", "failed"),
            Err(EXISTING.to_string())
        );
        assert_eq!(read_bounded(&path, 2).unwrap(), b"{}");
        assert_eq!(read_bounded(&path, 1), Err(ReadFailure::OutOfBounds));
        assert_eq!(
            read_bounded(&directory.join("missing"), 2),
            Err(ReadFailure::Unreadable)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
