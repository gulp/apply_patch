use agent_patch::app::{default_root, run};
use agent_patch::cli::{Cli, Command};
use agent_patch::diagnostics::{emit_error_human, emit_error_json};
use agent_patch::doctor::doctor;
use agent_patch::gc::gc;
use agent_patch::recover::recover;
use agent_patch::revert::revert;
use agent_patch::status::status;
use clap::Parser;
use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Status { json, root }) => {
            let root = root.unwrap_or_else(default_root);
            match status(&root) {
                Ok(report) => {
                    let code = if report.ok { 0 } else { 1 };
                    let out = if json {
                        serde_json::to_string_pretty(&report)
                            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialize"}"#.to_string())
                    } else {
                        format_status_human(&report)
                    };
                    let _ = writeln!(io::stdout().lock(), "{out}");
                    ExitCode::from(code)
                }
                Err(err) => emit_err(err, json),
            }
        }
        Some(Command::Doctor { json, root }) => {
            let root = root.unwrap_or_else(default_root);
            match doctor(&root) {
                Ok(report) => {
                    let code = if report.ok { 0 } else { 1 };
                    let out = if json {
                        serde_json::to_string_pretty(&report)
                            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialize"}"#.to_string())
                    } else {
                        let mut lines = vec![format!(
                            "agent-patch doctor: {} ({})",
                            if report.ok { "ok" } else { "needs attention" },
                            report.selected_binary
                        )];
                        for c in &report.checks {
                            lines.push(format!("  [{}] {}: {}", c.level, c.name, c.message));
                        }
                        lines.join("\n")
                    };
                    let _ = writeln!(io::stdout().lock(), "{out}");
                    ExitCode::from(code)
                }
                Err(err) => emit_err(err, json),
            }
        }
        Some(Command::Recover {
            transaction,
            json,
            root,
        }) => {
            let root = root.unwrap_or_else(default_root);
            match recover(&root, transaction.as_deref()) {
                Ok(result) => {
                    let out = if json {
                        serde_json::to_string_pretty(&result)
                            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialize"}"#.to_string())
                    } else if result.recovered.is_empty() {
                        "No incomplete transactions.".to_string()
                    } else {
                        result
                            .recovered
                            .iter()
                            .map(|r| {
                                format!(
                                    "recovered {} → {} ({})",
                                    r.transaction_id, r.outcome, r.state
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    let _ = writeln!(io::stdout().lock(), "{out}");
                    ExitCode::from(0)
                }
                Err(err) => emit_err(err, json),
            }
        }
        Some(Command::Revert {
            receipt,
            json,
            root,
        }) => {
            let root = root.unwrap_or_else(default_root);
            match revert(&root, &receipt) {
                Ok(result) => {
                    let out = if json {
                        serde_json::to_string_pretty(&result)
                            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialize"}"#.to_string())
                    } else {
                        format!(
                            "revert ok: {} → new tx {}",
                            result.reverted_transaction_id, result.new_transaction_id
                        )
                    };
                    let _ = writeln!(io::stdout().lock(), "{out}");
                    ExitCode::from(0)
                }
                Err(err) => emit_err(err, json),
            }
        }
        Some(Command::Gc {
            dry_run,
            json,
            root,
        }) => {
            let root = root.unwrap_or_else(default_root);
            match gc(&root, dry_run) {
                Ok(result) => {
                    let out = if json {
                        serde_json::to_string_pretty(&result)
                            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialize"}"#.to_string())
                    } else if dry_run {
                        format!(
                            "gc dry-run: {} unreferenced object(s)",
                            result.unreferenced.len()
                        )
                    } else {
                        format!("gc deleted {} object(s)", result.deleted.len())
                    };
                    let _ = writeln!(io::stdout().lock(), "{out}");
                    ExitCode::from(0)
                }
                Err(err) => emit_err(err, json),
            }
        }
        None => {
            let config = match cli.into_config() {
                Ok(c) => c,
                Err(err) => {
                    let mut err_out = io::stderr().lock();
                    let _ = writeln!(err_out, "error[{}]: {}", err.code.as_str(), err.message);
                    return ExitCode::from(err.exit_code());
                }
            };
            let output = run(config);

            if !output.stdout.is_empty() {
                let mut out = io::stdout().lock();
                let _ = writeln!(out, "{}", output.stdout);
            }
            if !output.stderr.is_empty() {
                let mut err = io::stderr().lock();
                let _ = writeln!(err, "{}", output.stderr);
            }

            ExitCode::from(output.exit_code)
        }
    }
}

fn emit_err(err: agent_patch::error::PublicError, json: bool) -> ExitCode {
    if json {
        let _ = writeln!(io::stdout().lock(), "{}", emit_error_json(&err));
    } else {
        let _ = writeln!(io::stderr().lock(), "{}", emit_error_human(&err));
    }
    ExitCode::from(err.exit_code())
}

fn format_status_human(report: &agent_patch::status::StatusReport) -> String {
    let mut lines = vec![format!(
        "agent-patch status: {}",
        if report.ok { "ok" } else { "needs attention" }
    )];
    for c in &report.checks {
        lines.push(format!("  [{}] {}: {}", c.level, c.name, c.message));
    }
    if !report.incomplete_transactions.is_empty() {
        lines.push(format!(
            "  incomplete: {}",
            report.incomplete_transactions.join(", ")
        ));
    }
    lines.join("\n")
}
