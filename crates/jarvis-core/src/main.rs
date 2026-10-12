//! J.A.R.V.I.S trusted core — PR-1 skeleton.
//!
//! Nothing in this binary implements assistant behavior yet. The durable core,
//! policy, tools and memory arrive in PR-2 through PR-5 as specified in
//! docs/phase-0/pull-requests.md. The process exists so toolchain pins,
//! dependency graph and CI checks are real and verifiable.

mod limits;

fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!(
        target: "jarvis_core",
        max_turns = limits::MAX_MODEL_TURNS,
        provider_deadline_ms = limits::PROVIDER_ATTEMPT_DEADLINE_MS,
        "core skeleton: implementation begins in PR-2 (durable core)"
    );
}
