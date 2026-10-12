//! Default slice limits from docs/phase-0/README.md ("Default constraints").
//!
//! These are starting design limits to verify, not measured performance. The
//! values are defined once here so the runtime and tests cannot drift from the
//! documented constraints. The full set is consumed by the policy, tools and
//! memory modules from PR-2 onward; until then the module is intentionally
//! referenced only by its unit tests.
#![allow(dead_code)]

/// Maximum model turns per task.
pub const MAX_MODEL_TURNS: u32 = 4;

/// Deadline for a single provider attempt, in milliseconds.
pub const PROVIDER_ATTEMPT_DEADLINE_MS: u64 = 30_000;

/// Maximum tool output returned to the model, in bytes.
pub const MAX_TOOL_OUTPUT_BYTES: usize = 64 * 1024;

/// Lifetime of a one-use approval grant, in seconds.
pub const GRANT_TTL_SECONDS: u64 = 60;

/// Default retention windows from docs/phase-0/security.md.
pub const PAYLOAD_RETENTION_DAYS: u32 = 30;
pub const CANDIDATE_RETENTION_HOURS: u32 = 24;
pub const AUDIT_METADATA_RETENTION_DAYS: u32 = 90;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_documented_constraints() {
        assert_eq!(MAX_MODEL_TURNS, 4, "README default constraints");
        assert_eq!(PROVIDER_ATTEMPT_DEADLINE_MS, 30_000);
        assert_eq!(MAX_TOOL_OUTPUT_BYTES, 65_536, "64 KiB tool output");
        assert_eq!(GRANT_TTL_SECONDS, 60, "60-second one-use grants");
        assert_eq!(
            (
                PAYLOAD_RETENTION_DAYS,
                CANDIDATE_RETENTION_HOURS,
                AUDIT_METADATA_RETENTION_DAYS
            ),
            (30, 24, 90)
        );
    }
}
