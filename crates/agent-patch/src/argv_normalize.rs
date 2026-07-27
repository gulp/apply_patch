//! Machine-mode argv peek + unique rewrite allowlist (see docs/design/robot-cli.md).

use crate::error::{ErrorCode, PublicError};
use serde::Serialize;
use std::path::Path;

const SUBCOMMANDS: &[&str] = &[
    "status",
    "doctor",
    "recover",
    "revert",
    "gc",
    "robot-docs",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CoachNote {
    pub rewrote_from: Vec<String>,
    pub canonical_argv: Vec<String>,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct NormalizedArgv {
    pub argv: Vec<String>,
    pub machine: bool,
    pub coach: Option<CoachNote>,
}

/// True when agents expect JSON / coaching (raw argv or env, before clap).
pub fn peek_machine_mode(argv: &[String]) -> bool {
    if robot_env_enabled() {
        return true;
    }
    argv.iter().any(|a| a == "--json" || a == "--robot")
}

fn robot_env_enabled() -> bool {
    match std::env::var("AGENT_PATCH_ROBOT") {
        Ok(v) => {
            let t = v.trim();
            !(t.is_empty()
                || t == "0"
                || t.eq_ignore_ascii_case("false")
                || t.eq_ignore_ascii_case("off"))
        }
        Err(_) => false,
    }
}

/// Allowlisted rewrites when in machine mode; identity otherwise.
pub fn normalize_argv(argv: Vec<String>) -> Result<NormalizedArgv, PublicError> {
    let machine = peek_machine_mode(&argv);
    if !machine {
        return Ok(NormalizedArgv {
            argv,
            machine: false,
            coach: None,
        });
    }

    let original = argv.clone();
    let mut argv = argv;
    let mut notes: Vec<&'static str> = Vec::new();

    // Alias --robot → --json before other rewrites so clap never sees --robot.
    ensure_json_flag(&mut argv);

    if apply_r3_undo(&mut argv) {
        notes.push("Alias: undo → revert.");
    }
    if apply_r2_subcommand_flags(&mut argv)? {
        notes.push("Moved --json/--root onto the subcommand.");
    }
    if apply_r1_verify_patch(&mut argv)? {
        notes.push("Moved patch path before -- for --verify.");
    }

    let coach = if notes.is_empty() {
        None
    } else {
        Some(CoachNote {
            rewrote_from: original,
            canonical_argv: argv.clone(),
            note: notes.join(" "),
        })
    };

    Ok(NormalizedArgv {
        argv,
        machine: true,
        coach,
    })
}

fn ensure_json_flag(argv: &mut Vec<String>) {
    if argv.iter().any(|a| a == "--json") {
        // Still strip --robot so clap never sees it on subcommands.
        argv.retain(|a| a != "--robot");
        return;
    }
    if argv.iter().any(|a| a == "--robot") || robot_env_enabled() {
        if argv.is_empty() {
            argv.push("--json".into());
        } else {
            argv.insert(1, "--json".into());
        }
        argv.retain(|a| a != "--robot");
    }
}

fn apply_r3_undo(argv: &mut [String]) -> bool {
    let mut i = 1;
    while i < argv.len() {
        let a = &argv[i];
        if a == "--" {
            break;
        }
        if a.starts_with('-') {
            if matches!(
                a.as_str(),
                "--root"
                    | "--verify-shell"
                    | "--verify-timeout"
                    | "--verify-output-limit"
                    | "--shadow-mode"
                    | "--fuzzy"
                    | "--risk"
                    | "--receipt"
                    | "--max-files"
                    | "--max-patch-bytes"
                    | "--max-file-bytes"
                    | "--transaction"
            ) {
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if a == "undo" {
            argv[i] = "revert".into();
            return true;
        }
        break;
    }
    false
}

fn apply_r2_subcommand_flags(argv: &mut Vec<String>) -> Result<bool, PublicError> {
    let sub_idx = match argv
        .iter()
        .skip(1)
        .position(|a| SUBCOMMANDS.contains(&a.as_str()))
    {
        Some(p) => p + 1,
        None => return Ok(false),
    };

    let mut movable: Vec<String> = Vec::new();
    let mut i = 1;
    let mut saw_junk = false;
    while i < sub_idx {
        let a = &argv[i];
        if a == "--json" || a == "--robot" || a == "--quiet" {
            movable.push(a.clone());
            i += 1;
            continue;
        }
        if a == "--root" {
            if i + 1 >= sub_idx {
                return Ok(false);
            }
            movable.push(a.clone());
            movable.push(argv[i + 1].clone());
            i += 2;
            continue;
        }
        saw_junk = true;
        break;
    }
    if saw_junk {
        if !movable.is_empty() {
            return Err(fail_closed_e6());
        }
        return Ok(false);
    }
    if movable.is_empty() {
        return Ok(false);
    }

    let mut remove_set: Vec<(usize, usize)> = Vec::new();
    let mut j = 1;
    while j < sub_idx {
        let a = &argv[j];
        if a == "--json" || a == "--robot" || a == "--quiet" {
            remove_set.push((j, 1));
            j += 1;
        } else if a == "--root" {
            remove_set.push((j, 2));
            j += 2;
        } else {
            j += 1;
        }
    }
    for (start, len) in remove_set.into_iter().rev() {
        for _ in 0..len {
            argv.remove(start);
        }
    }

    let sub_idx = argv
        .iter()
        .skip(1)
        .position(|a| SUBCOMMANDS.contains(&a.as_str()))
        .map(|p| p + 1)
        .expect("subcommand still present");

    let insert_at = sub_idx + 1;
    let mut to_insert = Vec::new();
    let mut k = 0;
    while k < movable.len() {
        let m = &movable[k];
        if m == "--root" {
            let val = &movable[k + 1];
            let exists = argv[sub_idx + 1..]
                .windows(2)
                .any(|w| w[0] == "--root");
            if !exists {
                to_insert.push(m.clone());
                to_insert.push(val.clone());
            }
            k += 2;
        } else if !argv[sub_idx + 1..].iter().any(|a| a == m) {
            to_insert.push(m.clone());
            k += 1;
        } else {
            k += 1;
        }
    }
    if to_insert.is_empty() {
        return Ok(false);
    }
    for (offset, tok) in to_insert.into_iter().enumerate() {
        argv.insert(insert_at + offset, tok);
    }
    Ok(true)
}

fn apply_r1_verify_patch(argv: &mut Vec<String>) -> Result<bool, PublicError> {
    let has_verify = argv.iter().any(|a| a == "--verify");
    if !has_verify {
        return Ok(false);
    }
    let dd = match argv.iter().position(|a| a == "--") {
        Some(i) => i,
        None => return Ok(false),
    };
    let after: Vec<String> = argv[dd + 1..].to_vec();
    if after.len() < 2 {
        if after.len() == 1 && looks_like_patch_path(Path::new(&after[0])) {
            return Err(fail_closed_e5());
        }
        return Ok(false);
    }
    let last = after.last().unwrap().clone();
    let last_path = Path::new(&last);
    if !is_patch_like_file(last_path) {
        if last.ends_with(".patch") {
            return Err(fail_closed_e5());
        }
        return Ok(false);
    }
    argv.pop();
    argv.insert(dd, last);
    Ok(true)
}

fn looks_like_patch_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("patch") || is_patch_like_file(path)
}

fn is_patch_like_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if path.extension().and_then(|e| e.to_str()) == Some("patch") {
        return true;
    }
    let mut buf = [0u8; 64];
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    use std::io::Read;
    let Ok(n) = f.read(&mut buf) else {
        return false;
    };
    let head = String::from_utf8_lossy(&buf[..n]);
    head.contains("*** Begin Patch")
}

fn fail_closed_e5() -> PublicError {
    PublicError::new(
        ErrorCode::InputError,
        "Could not uniquely move a patch path before `--` for --verify.",
    )
    .with_examples(vec![
        "agent-patch --json --verify change.patch -- true".into(),
        "agent-patch --json --verify -- true < change.patch".into(),
    ])
}

fn fail_closed_e6() -> PublicError {
    PublicError::new(
        ErrorCode::InputError,
        "Top-level flags before a subcommand could not be rewritten safely.",
    )
    .with_examples(vec![
        "agent-patch status --json --root .".into(),
        "agent-patch revert --json --root . receipt.json".into(),
    ])
}

/// Attach examples for known into_config / mode clash INPUT_ERRORs.
pub fn enrich_cli_input_error(mut err: PublicError) -> PublicError {
    if err.code != ErrorCode::InputError {
        return err;
    }
    if !err.examples.is_empty() {
        return err;
    }
    let msg = err.message.as_str();
    if msg.contains("mutually exclusive") {
        err.examples = vec![
            "agent-patch --json --check < change.patch".into(),
            "agent-patch --plan --json < change.patch".into(),
            "agent-patch --json --verify change.patch -- true".into(),
        ];
    } else if msg.contains("requires a command after") {
        err.examples = vec![
            "agent-patch --json --verify -- true < change.patch".into(),
            "agent-patch --json --verify change.patch -- true".into(),
        ];
    } else if msg.contains("cannot combine with argv after") {
        err.examples = vec![
            "agent-patch --json --verify-shell 'test -f src/lib.rs' < change.patch".into(),
            "agent-patch --json --verify change.patch -- true".into(),
        ];
    } else if msg.contains("Trailing argv after") {
        err.examples = vec![
            "agent-patch --json --verify change.patch -- true".into(),
            "agent-patch --json < change.patch".into(),
        ];
    }
    err
}

/// Map clap parse failures to INPUT_ERROR with examples / suggestions.
pub fn clap_error_to_public(err: clap::Error) -> PublicError {
    let msg = err.to_string();
    let mut suggestions = Vec::new();
    let mut examples = vec![
        "agent-patch --help".into(),
        "agent-patch robot-docs".into(),
    ];

    if msg.contains("--revert") || msg.contains("unexpected argument '--revert'") {
        suggestions.push("revert".into());
        examples = vec![
            "agent-patch revert --json receipt.json".into(),
            "agent-patch status --json".into(),
        ];
    } else if msg.contains("unexpected argument") {
        for cand in [
            "--json", "--root", "--verify", "--check", "--plan", "revert", "status",
        ] {
            suggestions.push(cand.to_string());
        }
        suggestions.truncate(3);
    }

    PublicError::new(
        ErrorCode::InputError,
        format!("Invalid CLI arguments: {}", first_line(&msg)),
    )
    .with_examples(examples)
    .with_suggestions(suggestions)
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s).trim()
}

pub fn robot_docs_guide() -> &'static str {
    r#"# agent-patch — agent guide

Machine mode: pass `--json` or `--robot` (or AGENT_PATCH_ROBOT=1). One JSON object on stdout.

Happy paths:
  scripts/agent-patch --json <<'PATCH'
  *** Begin Patch
  *** Update File: path/to/file
  @@
  -old
  +new
  *** End Patch
  PATCH

  scripts/agent-patch --plan --json < change.patch
  scripts/agent-patch --json --verify change.patch -- cargo test -q
  scripts/agent-patch status --json --root .
  scripts/agent-patch revert --json --root . receipt.json

Footguns:
  - Patch path BEFORE `--` under `--verify` (not after).
  - Put `--json` / `--root` on the subcommand, not before it.
  - Prefer `--verify -- PROG…` over `--verify-shell`.
  - Never invent flags like `--revert`; use the `revert` subcommand.

Canonical invoke: scripts/agent-patch (newer release/debug by mtime).
Design: docs/design/robot-cli.md
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    fn args(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn r2_moves_json_root_onto_status() {
        let n = normalize_argv(args(&[
            "agent-patch",
            "--json",
            "--root",
            "/tmp/r",
            "status",
        ]))
        .unwrap();
        assert_eq!(
            n.argv,
            args(&["agent-patch", "status", "--json", "--root", "/tmp/r"])
        );
        assert!(n.coach.unwrap().note.contains("subcommand"));
    }

    #[test]
    fn r3_undo_to_revert() {
        let n = normalize_argv(args(&["agent-patch", "--json", "undo", "r.json"])).unwrap();
        assert_eq!(n.argv[1], "revert");
        assert!(n.argv.iter().any(|a| a == "--json"));
        assert!(n.coach.unwrap().note.contains("undo"));
    }

    #[test]
    fn r1_moves_patch_before_dashdash() {
        let dir = tempdir().unwrap();
        let patch = dir.path().join("change.patch");
        let mut f = std::fs::File::create(&patch).unwrap();
        writeln!(f, "*** Begin Patch").unwrap();
        writeln!(f, "*** End Patch").unwrap();
        let p = patch.to_string_lossy().to_string();
        let n = normalize_argv(args(&[
            "agent-patch",
            "--json",
            "--verify",
            "--",
            "false",
            &p,
        ]))
        .unwrap();
        let dd = n.argv.iter().position(|a| a == "--").unwrap();
        assert_eq!(n.argv[dd - 1], p);
        assert_eq!(n.argv[dd + 1], "false");
        assert!(!n.argv[dd + 1..].iter().any(|a| a == &p));
    }

    #[test]
    fn human_mode_no_rewrite() {
        let n = normalize_argv(args(&["agent-patch", "--root", "/tmp", "status"])).unwrap();
        assert!(!n.machine);
        assert_eq!(n.argv, args(&["agent-patch", "--root", "/tmp", "status"]));
    }

    #[test]
    fn robot_injects_json() {
        let n = normalize_argv(args(&["agent-patch", "--robot", "status"])).unwrap();
        assert!(n.argv.iter().any(|a| a == "--json"));
    }
}
