//! Durable atomic file store for external safety state: the lifecycle journal and
//! the deletion ledger live here, outside the SQLite database, so a managed
//! restore cannot forget them. Contents are opaque identifiers only.
//!
//! Durability contract: write a same-directory temporary file, flush and sync it,
//! rename it over the target, then sync the directory where the platform allows
//! it. A failure at any step leaves the previous contents in place and reports
//! [`JournalError::Durability`], which stops the lifecycle mutation before a
//! secret is written or a deletion is acknowledged.
//!
//! Platform constraint: `sync_all` and directory synchronization are best-effort
//! guarantees only on local filesystems. Network shares, some removable media,
//! and Windows directory handles may accept the call without flushing to stable
//! storage. Phase 1 asserts the call sequence, not the media behaviour.

use quotavista_core::journal::{DurableStore, JournalError};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

/// Injected failure points for recovery fixtures. Each maps to one bit so a test
/// can arm several boundaries at once.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    /// Reading an existing entry fails. Absence may not be inferred from this.
    Read = 1,
    /// Creating or writing the temporary file fails.
    TempWrite = 2,
    /// The flush/sync step fails, so durability is not established.
    Sync = 4,
    /// The atomic rename fails after a successful flush.
    Rename = 8,
}

impl Failure {
    fn bit(self) -> u8 {
        self as u8
    }
}

/// Longest accepted entry name; names never contain path separators.
const MAX_NAME_BYTES: usize = 64;

/// Filesystem-backed [`DurableStore`] rooted at one directory.
#[derive(Debug)]
pub struct FileStore {
    root: PathBuf,
    failures: AtomicU8,
}

impl FileStore {
    /// Create the root directory if needed. A directory that cannot be created
    /// is a durability failure, so callers stop before mutating safety state.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, JournalError> {
        let root = root.into();
        fs::create_dir_all(&root).map_err(|_| JournalError::Durability)?;
        Ok(Self {
            root,
            failures: AtomicU8::new(0),
        })
    }

    /// Directory holding every entry.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Arm an injected failure. Test fixtures only.
    pub fn fail(&self, failure: Failure) {
        self.failures.fetch_or(failure.bit(), Ordering::Relaxed);
    }

    /// Clear every injected failure. Test fixtures only.
    pub fn clear(&self) {
        self.failures.store(0, Ordering::Relaxed);
    }

    fn failing(&self, failure: Failure) -> bool {
        self.failures.load(Ordering::Relaxed) & failure.bit() != 0
    }

    /// Resolve an entry name inside the root. Names are restricted so an entry can
    /// never address a path outside it.
    fn path(&self, name: &str) -> Result<PathBuf, JournalError> {
        let safe = !name.is_empty()
            && name.len() <= MAX_NAME_BYTES
            && !name.starts_with('.')
            && !name.contains("..")
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.');
        if safe {
            Ok(self.root.join(name))
        } else {
            Err(JournalError::Validation)
        }
    }
}

impl DurableStore for FileStore {
    fn read(&self, name: &str) -> Result<Option<String>, JournalError> {
        let path = self.path(name)?;
        if self.failing(Failure::Read) {
            return Err(JournalError::RecoveryRequired);
        }
        match fs::read_to_string(&path) {
            Ok(contents) => Ok(Some(contents)),
            // A missing entry means no state was ever written. Any other error is
            // a visible recovery condition: unreadable is never treated as absent.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(JournalError::RecoveryRequired),
        }
    }

    fn replace(&self, name: &str, contents: &str) -> Result<(), JournalError> {
        let path = self.path(name)?;
        let directory = path.parent().ok_or(JournalError::Validation)?.to_path_buf();
        // Same directory, so the rename cannot cross a filesystem boundary.
        let temporary = directory.join(format!(".{name}.tmp"));
        if self.failing(Failure::TempWrite) {
            return Err(JournalError::Durability);
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .map_err(|_| JournalError::Durability)?;
        file.write_all(contents.as_bytes())
            .and_then(|()| file.flush())
            .map_err(|_| JournalError::Durability)?;
        if self.failing(Failure::Sync) {
            return Err(JournalError::Durability);
        }
        file.sync_all().map_err(|_| JournalError::Durability)?;
        if self.failing(Failure::Rename) {
            // A crash here leaves the previous contents authoritative and a stray
            // temporary file that the next attempt truncates.
            return Err(JournalError::Durability);
        }
        fs::rename(&temporary, &path).map_err(|_| JournalError::Durability)?;
        sync_directory(&directory);
        Ok(())
    }
}

/// Sync a directory so the rename itself is durable. Unsupported on some
/// platforms, where the call is allowed to fail without failing the write.
fn sync_directory(directory: &Path) {
    if let Ok(handle) = File::open(directory) {
        let _ = handle.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_is_atomic_and_previous_contents_survive_a_failed_rename() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = FileStore::new(directory.path().join("safety")).expect("store");
        store.replace("journal", "first").expect("first write");
        assert_eq!(store.read("journal").expect("read"), Some("first".into()));

        store.fail(Failure::Rename);
        assert_eq!(
            store.replace("journal", "second"),
            Err(JournalError::Durability)
        );
        store.clear();
        assert_eq!(
            store.read("journal").expect("read"),
            Some("first".into()),
            "a failed rename must not expose partial contents"
        );

        store.replace("journal", "second").expect("retry");
        assert_eq!(store.read("journal").expect("read"), Some("second".into()));
    }

    #[test]
    fn durability_failures_stop_the_write_and_reads_never_infer_absence() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = FileStore::new(directory.path()).expect("store");
        assert_eq!(store.read("missing").expect("read"), None);

        for failure in [Failure::TempWrite, Failure::Sync, Failure::Rename] {
            store.fail(failure);
            assert_eq!(
                store.replace("ledger", "opaque").err(),
                Some(JournalError::Durability),
                "{failure:?} must report a durability failure"
            );
            store.clear();
        }
        assert_eq!(store.read("ledger").expect("read"), None);

        store.replace("ledger", "opaque").expect("write");
        store.fail(Failure::Read);
        assert_eq!(
            store.read("ledger").err(),
            Some(JournalError::RecoveryRequired),
            "an unreadable entry is a recovery condition, not an absent one"
        );
    }

    #[test]
    fn entry_names_cannot_leave_the_root_directory() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = FileStore::new(directory.path()).expect("store");
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "a/b",
            ".hidden",
            &"n".repeat(65),
            "name with space",
        ] {
            assert_eq!(
                store.read(name).err(),
                Some(JournalError::Validation),
                "{name:?} must be rejected"
            );
            assert_eq!(
                store.replace(name, "x").err(),
                Some(JournalError::Validation)
            );
        }
        assert!(store.read("journal-v1").expect("read").is_none());
    }
}
