use agent_patch::app::{default_root, run};
use agent_patch::argv_normalize::{
    clap_error_to_public, enrich_cli_input_error, normalize_argv, robot_docs_guide, CoachNote,
};
use agent_patch::cli::{Cli, Command};
use agent_patch::diagnostics::{
    emit_error_human, emit_error_json_with_coach, json_with_coach,
};
use agent_patch::doctor::doctor;
use agent_patch::gc::gc;
use agent_patch::recover::recover;
use agent_patch::revert::revert;
use agent_patch::status::status;
use clap::Parser;
use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().collect();
    let normalized = match normalize_argv(raw) {
        Ok(n) => n,
        Err(err) => {
            return emit_err(err, true, None);
        }
    };
    // normalize_argv fail paths always come from machine mode allowlist.
    let machine = normalized.machine;
    let coach = normalized.coach.clone();

    let cli = match Cli::try_parse_from(&normalized.argv) {
        Ok(c) => c,
        Err(e) => {
            if matches!(
                e.kind(),
                clap::error::ErrorKind::DisplayHelp
                    | clap::error::ErrorKind::DisplayVersion
                    | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
            ) {
                let _ = e.print();
                return ExitCode::from(e.exit_code() as u8);
            }
            if machine {
                return emit_err(clap_error_to_public(e), true, coach.as_ref());
            }
            let _ = e.print();
            return ExitCode::from(e.exit_code() as u8);
        }
    };

    match cli.command {
        Some(Command::RobotDocs { json }) => {
            let guide = robot_docs_guide();
            if json || machine {
                let body = serde_json::json!({
                    "ok": true,
                    "guide": guide,
                });
                let out = json_with_coach(&body, coach.as_ref());
                let _ = writeln!(io::stdout().lock(), "{out}");
            } else {
                let _ = writeln!(io::stdout().lock(), "{guide}");
            }
            ExitCode::from(0)
        }
        Some(Command::Status { json, root }) => {
            let json = json || machine;
            let root = root.unwrap_or_else(default_root);
            match status(&root) {
                Ok(report) => {
                    let code = if report.ok { 0 } else { 1 };
                    let out = if json {
                        json_with_coach(&report, coach.as_ref())
                    } else {
                        format_status_human(&report)
                    };
                    let _ = writeln!(io::stdout().lock(), "{out}");
                    ExitCode::from(code)
                }
                Err(err) => emit_err(err, json, coach.as_ref()),
            }
        }
        Some(Command::Doctor { json, root }) => {
            let json = json || machine;
            let root = root.unwrap_or_else(default_root);
            match doctor(&root) {
                Ok(report) => {
                    let code = if report.ok { 0 } else { 1 };
                    let out = if json {
                        json_with_coach(&report, coach.as_ref())
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
                Err(err) => emit_err(err, json, coach.as_ref()),
            }
        }
        Some(Command::Recover {
            transaction,
            json,
            root,
        }) => {
            let json = json || machine;
            let root = root.unwrap_or_else(default_root);
            match recover(&root, transaction.as_deref()) {
                Ok(result) => {
                    let out = if json {
                        json_with_coach(&result, coach.as_ref())
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
                Err(err) => emit_err(err, json, coach.as_ref()),
            }
        }
        Some(Command::Revert {
            receipt,
            json,
            root,
        }) => {
            let json = json || machine;
            let root = root.unwrap_or_else(default_root);
            match revert(&root, &receipt) {
                Ok(result) => {
                    let out = if json {
                        json_with_coach(&result, coach.as_ref())
                    } else {
                        format!(
                            "revert ok: {} → new tx {}",
                            result.reverted_transaction_id, result.new_transaction_id
                        )
                    };
                    let _ = writeln!(io::stdout().lock(), "{out}");
                    ExitCode::from(0)
                }
                Err(err) => emit_err(err, json, coach.as_ref()),
            }
        }
        Some(Command::Gc {
            dry_run,
            json,
            root,
        }) => {
            let json = json || machine;
            let root = root.unwrap_or_else(default_root);
            match gc(&root, dry_run) {
                Ok(result) => {
                    let out = if json {
                        json_with_coach(&result, coach.as_ref())
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
                Err(err) => emit_err(err, json, coach.as_ref()),
            }
        }
        None => {
            let mut config = match cli.into_config() {
                Ok(c) => c,
                Err(err) => {
                    let err = enrich_cli_input_error(err);
                    return emit_err(err, machine, coach.as_ref());
                }
            };
            config.coach = coach;
            if machine {
                config.json = true;
            }
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

fn emit_err(
    err: agent_patch::error::PublicError,
    json: bool,
    coach: Option<&CoachNote>,
) -> ExitCode {
    if json {
        let _ = writeln!(
            io::stdout().lock(),
            "{}",
            emit_error_json_with_coach(&err, coach)
        );
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
