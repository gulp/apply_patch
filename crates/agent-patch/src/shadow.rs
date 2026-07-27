//! Representative (or touched) verification shadow workspaces.

use crate::error::{ErrorCode, PublicError};
use crate::plan::{PatchPlan, PlannedChange};
use crate::store_layout::{agent_patch_dir, ensure_layout};
use serde::Serialize;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShadowMode {
    Tree,
    Touched,
}

impl ShadowMode {
    pub fn parse(s: &str) -> Result<Self, PublicError> {
        match s {
            "tree" => Ok(Self::Tree),
            "touched" => Ok(Self::Touched),
            other => Err(PublicError::new(
                ErrorCode::InputError,
                format!("Unknown --shadow-mode {other}; use tree|touched"),
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShadowOptions {
    pub mode: ShadowMode,
    pub include_caches: bool,
    pub max_files: u64,
    pub max_bytes: u64,
    pub max_wall: Duration,
}

impl Default for ShadowOptions {
    fn default() -> Self {
        Self {
            mode: ShadowMode::Tree,
            include_caches: false,
            max_files: 200_000,
            max_bytes: 20 * 1024 * 1024 * 1024,
            max_wall: Duration::from_secs(120),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ShadowReport {
    pub shadow_mode: ShadowMode,
    pub representative: bool,
    pub excludes: Vec<String>,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub shadow_root: String,
}

#[derive(Debug)]
pub struct ShadowWorkspace {
    pub real_root: PathBuf,
    pub shadow_root: PathBuf,
    pub report: ShadowReport,
}

impl Drop for ShadowWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.shadow_root);
    }
}

pub fn default_excludes(include_caches: bool) -> Vec<String> {
    let mut v = vec![".agent-patch".into()];
    if !include_caches {
        v.extend([
            ".git".into(),
            "target".into(),
            "node_modules".into(),
            ".venv".into(),
            "__pycache__".into(),
            ".tox".into(),
            "dist".into(),
            "build".into(),
            ".next".into(),
            ".cache".into(),
        ]);
    }
    v
}

pub fn materialize(
    real_root: &Path,
    plan: &PatchPlan,
    opts: &ShadowOptions,
) -> Result<ShadowWorkspace, PublicError> {
    ensure_layout(real_root)?;
    let id = format!(
        "shadow-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let shadow_root = agent_patch_dir(real_root).join("shadows").join(&id);
    fs::create_dir_all(&shadow_root).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot create shadow root: {e}"),
        )
    })?;

    let excludes = default_excludes(opts.include_caches);
    let started = Instant::now();
    let mut files_copied = 0u64;
    let mut bytes_copied = 0u64;

    match opts.mode {
        ShadowMode::Tree => {
            copy_tree(
                real_root,
                real_root,
                &shadow_root,
                &excludes,
                opts,
                started,
                &mut files_copied,
                &mut bytes_copied,
            )?;
        }
        ShadowMode::Touched => {
            // Parents only for planned paths; content filled by overlay.
            for entry in &plan.entries {
                let rel = PathBuf::from(entry.path().as_str());
                if let Some(parent) = rel.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(shadow_root.join(parent)).map_err(|e| {
                            PublicError::new(
                                ErrorCode::IoError,
                                format!("Cannot create touched parent: {e}"),
                            )
                        })?;
                    }
                }
            }
        }
    }

    overlay_plan(&shadow_root, plan, opts, started, &mut files_copied, &mut bytes_copied)?;

    let representative = opts.mode == ShadowMode::Tree;
    Ok(ShadowWorkspace {
        real_root: real_root.to_path_buf(),
        shadow_root: shadow_root.clone(),
        report: ShadowReport {
            shadow_mode: opts.mode,
            representative,
            excludes,
            files_copied,
            bytes_copied,
            shadow_root: shadow_root.display().to_string(),
        },
    })
}

fn overlay_plan(
    shadow_root: &Path,
    plan: &PatchPlan,
    opts: &ShadowOptions,
    started: Instant,
    files_copied: &mut u64,
    bytes_copied: &mut u64,
) -> Result<(), PublicError> {
    check_budgets(opts, started, *files_copied, *bytes_copied)?;
    for entry in &plan.entries {
        check_budgets(opts, started, *files_copied, *bytes_copied)?;
        let dest = shadow_root.join(entry.path().as_str());
        match entry {
            PlannedChange::Create(c) => {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        PublicError::new(ErrorCode::IoError, format!("shadow mkdir: {e}"))
                    })?;
                }
                write_bytes_no_hardlink(&dest, &c.bytes)?;
                *files_copied += 1;
                *bytes_copied += c.bytes.len() as u64;
            }
            PlannedChange::Modify(m) => {
                if let Some(parent) = dest.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        PublicError::new(ErrorCode::IoError, format!("shadow mkdir: {e}"))
                    })?;
                }
                write_bytes_no_hardlink(&dest, &m.after_bytes)?;
                *files_copied += 1;
                *bytes_copied += m.after_bytes.len() as u64;
            }
            PlannedChange::Remove(r) => {
                if dest.exists() {
                    fs::remove_file(&dest).map_err(|e| {
                        PublicError::new(
                            ErrorCode::IoError,
                            format!("Cannot remove {} in shadow: {e}", r.path),
                        )
                    })?;
                }
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn copy_tree(
    real_root: &Path,
    src_dir: &Path,
    shadow_root: &Path,
    excludes: &[String],
    opts: &ShadowOptions,
    started: Instant,
    files_copied: &mut u64,
    bytes_copied: &mut u64,
) -> Result<(), PublicError> {
    check_budgets(opts, started, *files_copied, *bytes_copied)?;
    let rd = fs::read_dir(src_dir).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot read {}: {e}", src_dir.display()),
        )
    })?;
    for ent in rd {
        let ent = ent.map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot read dir entry: {e}"))
        })?;
        let name = ent.file_name();
        let name_str = name.to_string_lossy();
        if excludes.iter().any(|e| e == name_str.as_ref()) {
            continue;
        }
        let src = ent.path();
        let rel = src.strip_prefix(real_root).map_err(|_| {
            PublicError::new(
                ErrorCode::InternalError,
                format!("Path {} escaped root during shadow copy", src.display()),
            )
        })?;
        let dest = shadow_root.join(rel);
        let meta = fs::symlink_metadata(&src).map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot stat {}: {e}", src.display()),
            )
        })?;
        let ft = meta.file_type();
        if ft.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| {
                PublicError::new(ErrorCode::IoError, format!("shadow mkdir: {e}"))
            })?;
            copy_tree(
                real_root,
                &src,
                shadow_root,
                excludes,
                opts,
                started,
                files_copied,
                bytes_copied,
            )?;
        } else if ft.is_symlink() {
            copy_symlink_safe(real_root, &src, &dest)?;
            *files_copied += 1;
        } else if ft.is_file() {
            let len = meta.len();
            check_budgets(opts, started, *files_copied + 1, *bytes_copied + len)?;
            copy_file_no_hardlink(&src, &dest)?;
            *files_copied += 1;
            *bytes_copied += len;
        } else {
            return Err(PublicError::new(
                ErrorCode::InputError,
                format!(
                    "Unsupported special file in shadow source: {}",
                    src.display()
                ),
            ));
        }
    }
    Ok(())
}

fn copy_symlink_safe(real_root: &Path, src: &Path, dest: &Path) -> Result<(), PublicError> {
    let target = fs::read_link(src).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot read symlink {}: {e}", src.display()),
        )
    })?;
    // Lexical target must resolve under real_root when absolute or joined.
    let resolved = if target.is_absolute() {
        target.clone()
    } else {
        src.parent().unwrap_or(src).join(&target)
    };
    let resolved = resolved.canonicalize().unwrap_or(resolved);
    if !resolved.starts_with(real_root) {
        return Err(PublicError::new(
            ErrorCode::InputError,
            format!(
                "Symlink {} escapes root; refusing shadow materialization",
                src.display()
            ),
        ));
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("shadow mkdir: {e}"))
        })?;
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, dest).map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot create symlink {}: {e}", dest.display()),
            )
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = target;
        return Err(PublicError::new(
            ErrorCode::InputError,
            "Symlink shadow copy is only supported on Unix.",
        ));
    }
    Ok(())
}

fn copy_file_no_hardlink(src: &Path, dest: &Path) -> Result<(), PublicError> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("shadow mkdir: {e}"))
        })?;
    }
    // Prefer copy (never hard link). Attempt clonefile/reflink via std::fs::copy
    // which on Linux may use copy_file_range; still a distinct inode.
    let mut in_f = File::open(src).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot open {}: {e}", src.display()),
        )
    })?;
    let mut out_f = File::create(dest).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot create {}: {e}", dest.display()),
        )
    })?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = in_f.read(&mut buf).map_err(|e| {
            PublicError::new(ErrorCode::IoError, format!("Cannot read {}: {e}", src.display()))
        })?;
        if n == 0 {
            break;
        }
        out_f.write_all(&buf[..n]).map_err(|e| {
            PublicError::new(
                ErrorCode::IoError,
                format!("Cannot write {}: {e}", dest.display()),
            )
        })?;
    }
    out_f.sync_all().map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot fsync {}: {e}", dest.display()),
        )
    })?;
    // Preserve mode best-effort
    if let Ok(meta) = fs::metadata(src) {
        let _ = fs::set_permissions(dest, meta.permissions());
    }
    Ok(())
}

fn write_bytes_no_hardlink(dest: &Path, bytes: &[u8]) -> Result<(), PublicError> {
    let mut f = File::create(dest).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot create {}: {e}", dest.display()),
        )
    })?;
    f.write_all(bytes).map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot write {}: {e}", dest.display()),
        )
    })?;
    f.sync_all().map_err(|e| {
        PublicError::new(
            ErrorCode::IoError,
            format!("Cannot fsync {}: {e}", dest.display()),
        )
    })?;
    Ok(())
}

fn check_budgets(
    opts: &ShadowOptions,
    started: Instant,
    files: u64,
    bytes: u64,
) -> Result<(), PublicError> {
    if started.elapsed() > opts.max_wall {
        return Err(PublicError::new(
            ErrorCode::ShadowLimitExceeded,
            format!(
                "Shadow wall time exceeded {}s",
                opts.max_wall.as_secs()
            ),
        ));
    }
    if files > opts.max_files {
        return Err(PublicError::new(
            ErrorCode::ShadowLimitExceeded,
            format!("Shadow file count exceeded {}", opts.max_files),
        ));
    }
    if bytes > opts.max_bytes {
        return Err(PublicError::new(
            ErrorCode::ShadowLimitExceeded,
            format!("Shadow byte budget exceeded {}", opts.max_bytes),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path_policy::CanonicalRoot;
    use crate::plan::{PlanSummary, PlannedCreate};
    use crate::error::ContentFingerprint;
    use crate::path_policy::parse_repo_path;
    use std::collections::BTreeMap;

    fn tiny_plan(root: &Path, path: &str, bytes: &[u8]) -> PatchPlan {
        let root = CanonicalRoot::resolve(root).unwrap();
        let rp = parse_repo_path(path).unwrap();
        let abs = root.path.join(path);
        PatchPlan {
            root,
            entries: vec![PlannedChange::Create(PlannedCreate {
                path: rp,
                abs_path: abs,
                bytes: bytes.to_vec(),
                after_hash: ContentFingerprint::blake3(bytes),
                operation_index: 0,
                lines_added: 1,
            })],
            base_fingerprints: BTreeMap::new(),
            summary: PlanSummary::default(),
            match_evidence: vec![],
            plan_digest: "blake3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                .into(),
        }
    }

    #[test]
    fn tree_shadow_excludes_git_and_overlays_plan() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("keep.txt"), "keep\n").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/config"), "secret\n").unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/x"), "bin\n").unwrap();

        let plan = tiny_plan(root, "new.txt", b"fresh\n");
        let shadow = materialize(root, &plan, &ShadowOptions::default()).unwrap();
        assert!(shadow.report.representative);
        assert!(shadow.shadow_root.join("keep.txt").is_file());
        assert!(!shadow.shadow_root.join(".git").exists());
        assert!(!shadow.shadow_root.join("target").exists());
        assert_eq!(
            fs::read_to_string(shadow.shadow_root.join("new.txt")).unwrap(),
            "fresh\n"
        );
        // Mutating shadow must not affect real root
        fs::write(shadow.shadow_root.join("keep.txt"), "mutated\n").unwrap();
        assert_eq!(fs::read_to_string(root.join("keep.txt")).unwrap(), "keep\n");
    }

    #[test]
    fn touched_is_non_representative() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("other.txt"), "x\n").unwrap();
        let plan = tiny_plan(root, "only.txt", b"y\n");
        let opts = ShadowOptions {
            mode: ShadowMode::Touched,
            ..ShadowOptions::default()
        };
        let shadow = materialize(root, &plan, &opts).unwrap();
        assert!(!shadow.report.representative);
        assert!(shadow.shadow_root.join("only.txt").is_file());
        assert!(!shadow.shadow_root.join("other.txt").exists());
    }

    #[test]
    fn file_budget_fails_closed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for i in 0..5 {
            fs::write(root.join(format!("f{i}.txt")), b"x").unwrap();
        }
        let plan = tiny_plan(root, "z.txt", b"z\n");
        let opts = ShadowOptions {
            max_files: 2,
            ..ShadowOptions::default()
        };
        let err = materialize(root, &plan, &opts).unwrap_err();
        assert_eq!(err.code, ErrorCode::ShadowLimitExceeded);
    }
}
