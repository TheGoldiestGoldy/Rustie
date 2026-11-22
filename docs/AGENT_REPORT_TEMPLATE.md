# Rustie Agent Report: <date> / <short task>

> **Status:** Implemented / Partial / Blocked. This report describes the actual result of this agent's work, with no inferred success.
> **Workspace:** `~/Projects/rust-daw`
> **Agent and date:** <identifier and UTC date>

## 1. What I implemented

Describe behavior and source changes. Distinguish live functionality, UI-only styling, tests and documentation.

| Area | Exact files | Change | Status |
| --- | --- | --- | --- |
| <area> | <paths> | <change> | Working / Partial / Not tested |

## 2. Current state and real screenshots

For UI work, show **captures from the running compiled app**, with exact local paths or verified URLs. For non-UI work, use observable logs/measurements. Do not present image-generation concepts as evidence of implementation.

## 3. Verification

| Check | Exact command or procedure | Observed result |
| --- | --- | --- |
| Rust format | `cargo fmt --all -- --check` | Passed / Failed / Not run |
| Rust tests | `cargo test --workspace` | X passed / Y failed |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | Passed / Failed / Not run |
| Release | `cargo build --release -p daw-ui` | Passed / Failed / Not run |
| App launch/UI | <what actually opened and clicked> | Verified / Not tested |
| Audio output | <device/backend, real heard sound if verified, latency measurements> | Verified / Not tested |
| Regression | <focused tests> | Verified / Not tested |

**Do not equate a nonzero DSP output meter or a running GUI with sound reaching the headphones.**

## 4. Known issues and limitations

List unresolved behaviors, measurable evidence, regressions, disabled controls, and anything not independently verified.

## 5. What remains: next priorities

P0 / P1 / P2 with reproducible steps and acceptance criteria. Avoid vague phrases such as “make it professional.”

## 6. Git state and deliverables

- Branch and HEAD: <checked values>
- Committed: Yes / No. Pushed: Yes / No.
- Changed/untracked files: <exact paths>
- New artifacts/screenshots: <exact verified paths or URLs>
- Secret/privacy checks: <what was performed>

## 7. Next-agent handoff

What the next agent must read, how to reproduce remaining issues, other-agent collision warnings, and what to attempt first. Update `AGENTS.md`, `README.md` and relevant docs where the observed state changed.

**Required:** Save this report under `docs/agent-reports/YYYY-MM-DD-short-task-report.md` (unique filename), then summarize the same information in the final chat response.
