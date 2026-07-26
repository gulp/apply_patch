//! Filesystem adapter.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tempfile::NamedTempFile;

#[derive(Debug, Clone)]
pub struct FsMetadata {
    pub len: u64,
    pub mode: u32,
    pub is_file: bool,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub modified: Option<SystemTime>,
}

pub trait FileSystem: Send + Sync {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn metadata(&self, path: &Path) -> io::Result<FsMetadata>;
    fn symlink_metadata(&self, path: &Path) -> io::Result<FsMetadata>;
    fn create_temp_near(&self, path: &Path) -> io::Result<TempHandle>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn set_permissions(&self, path: &Path, mode: u32) -> io::Result<()>;
    fn sync_file(&self, path: &Path) -> io::Result<()>;
    fn write_temp(&self, temp: &mut TempHandle, bytes: &[u8]) -> io::Result<()>;
    fn persist_temp(&self, temp: TempHandle, dest: &Path) -> io::Result<()>;
}

#[derive(Debug)]
pub struct TempHandle {
    pub path: PathBuf,
    inner: Option<NamedTempFile>,
}

pub struct RealFileSystem {
    pub fsync: bool,
}

impl Default for RealFileSystem {
    fn default() -> Self {
        Self { fsync: true }
    }
}

fn to_meta(meta: fs::Metadata, is_symlink: bool) -> FsMetadata {
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode()
    };
    #[cfg(not(unix))]
    let mode = 0o644;

    FsMetadata {
        len: meta.len(),
        mode,
        is_file: meta.is_file(),
        is_dir: meta.is_dir(),
        is_symlink,
        modified: meta.modified().ok(),
    }
}

impl FileSystem for RealFileSystem {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn metadata(&self, path: &Path) -> io::Result<FsMetadata> {
        let meta = fs::metadata(path)?;
        Ok(to_meta(meta, false))
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<FsMetadata> {
        let meta = fs::symlink_metadata(path)?;
        let is_symlink = meta.file_type().is_symlink();
        Ok(to_meta(meta, is_symlink))
    }

    fn create_temp_near(&self, path: &Path) -> io::Result<TempHandle> {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let temp = NamedTempFile::new_in(parent)?;
        let path = temp.path().to_path_buf();
        Ok(TempHandle {
            path,
            inner: Some(temp),
        })
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn set_permissions(&self, path: &Path, mode: u32) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode))
        }
        #[cfg(not(unix))]
        {
            let _ = (path, mode);
            Ok(())
        }
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        if !self.fsync {
            return Ok(());
        }
        let f = File::open(path)?;
        f.sync_all()
    }

    fn write_temp(&self, temp: &mut TempHandle, bytes: &[u8]) -> io::Result<()> {
        let file = temp
            .inner
            .as_mut()
            .ok_or_else(|| io::Error::other("temp handle closed"))?;
        file.write_all(bytes)?;
        file.flush()?;
        if self.fsync {
            file.as_file().sync_all()?;
        }
        Ok(())
    }

    fn persist_temp(&self, mut temp: TempHandle, dest: &Path) -> io::Result<()> {
        let named = temp
            .inner
            .take()
            .ok_or_else(|| io::Error::other("temp handle closed"))?;
        named.persist(dest).map_err(|e| e.error)?;
        Ok(())
    }
}

/// Counting wrapper for check-mode assertions in tests.
pub struct CountingFs<'a> {
    pub inner: &'a dyn FileSystem,
    pub writes: std::sync::atomic::AtomicUsize,
}

impl<'a> CountingFs<'a> {
    pub fn new(inner: &'a dyn FileSystem) -> Self {
        Self {
            inner,
            writes: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl FileSystem for CountingFs<'_> {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.inner.read(path)
    }
    fn metadata(&self, path: &Path) -> io::Result<FsMetadata> {
        self.inner.metadata(path)
    }
    fn symlink_metadata(&self, path: &Path) -> io::Result<FsMetadata> {
        self.inner.symlink_metadata(path)
    }
    fn create_temp_near(&self, path: &Path) -> io::Result<TempHandle> {
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.create_temp_near(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.rename(from, to)
    }
    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.remove_file(path)
    }
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.create_dir_all(path)
    }
    fn set_permissions(&self, path: &Path, mode: u32) -> io::Result<()> {
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.set_permissions(path, mode)
    }
    fn sync_file(&self, path: &Path) -> io::Result<()> {
        self.inner.sync_file(path)
    }
    fn write_temp(&self, temp: &mut TempHandle, bytes: &[u8]) -> io::Result<()> {
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.write_temp(temp, bytes)
    }
    fn persist_temp(&self, temp: TempHandle, dest: &Path) -> io::Result<()> {
        self.writes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.inner.persist_temp(temp, dest)
    }
}

#[allow(dead_code)]
fn _open_opts() -> OpenOptions {
    OpenOptions::new()
}
