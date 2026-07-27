//! Match / risk / fuzzy policy options (contract-v2).

use crate::error::{ErrorCode, PublicError};
use crate::oracle::{MatchEvidence, MatchLevel};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FuzzyMode {
    #[default]
    Off,
    Rstrip,
    Strip,
}

impl FuzzyMode {
    pub fn parse(s: &str) -> Result<Self, PublicError> {
        match s {
            "off" => Ok(Self::Off),
            "rstrip" => Ok(Self::Rstrip),
            "strip" => Ok(Self::Strip),
            other => Err(PublicError::new(
                ErrorCode::InputError,
                format!("Unknown --fuzzy {other}; use off|rstrip|strip"),
            )),
        }
    }

    pub fn allows_rstrip(self) -> bool {
        matches!(self, Self::Rstrip | Self::Strip)
    }

    pub fn allows_strip(self) -> bool {
        matches!(self, Self::Strip)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiskMode {
    #[default]
    Off,
    Warn,
    Refuse,
}

impl RiskMode {
    pub fn parse(s: &str) -> Result<Self, PublicError> {
        match s {
            "off" => Ok(Self::Off),
            "warn" => Ok(Self::Warn),
            "refuse" => Ok(Self::Refuse),
            other => Err(PublicError::new(
                ErrorCode::InputError,
                format!("Unknown --risk {other}; use off|warn|refuse"),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MatchOptions {
    pub fuzzy: FuzzyMode,
    pub risk: RiskMode,
}

/// Fail closed when risk findings exceed the configured gate.
pub fn enforce_risk(evidence: &[MatchEvidence], mode: RiskMode) -> Result<(), PublicError> {
    if mode == RiskMode::Off {
        return Ok(());
    }
    let mut findings = Vec::new();
    for e in evidence {
        if e.nearby_twins > 0 {
            findings.push(format!(
                "{} hunk {}: {} nearby twin(s)",
                e.path,
                e.hunk_index + 1,
                e.nearby_twins
            ));
        }
        if matches!(
            e.accepted_level,
            MatchLevel::ContextReduced | MatchLevel::Rstrip | MatchLevel::Strip
        ) {
            findings.push(format!(
                "{} hunk {}: accepted via {:?}",
                e.path,
                e.hunk_index + 1,
                e.accepted_level
            ));
        }
    }
    if findings.is_empty() {
        return Ok(());
    }
    let detail = findings.join("; ");
    match mode {
        RiskMode::Off => Ok(()),
        RiskMode::Warn => Ok(()), // caller may surface via diagnostics later
        RiskMode::Refuse => Err(PublicError::new(
            ErrorCode::RiskRefused,
            format!("Match risk gate refused apply: {detail}"),
        )
        .with_hint("Regenerate a more unique hunk, or pass --risk=warn|off explicitly.")),
    }
}

pub fn normalize_line(line: &str, mode: FuzzyMode) -> String {
    match mode {
        FuzzyMode::Off => line.to_string(),
        FuzzyMode::Rstrip => line.trim_end().to_string(),
        FuzzyMode::Strip => line.trim().to_string(),
    }
}
