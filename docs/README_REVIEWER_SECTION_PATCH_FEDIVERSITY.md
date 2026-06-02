# Suggested README Patch for NGI Fediversity

Add this section near the top of the LiminalDB README, right after `# LiminalDB`.

```markdown
## NGI Fediversity reviewer path

LiminalDB was submitted to NGI Fediversity as an open-source federated event-sourced memory layer for local-first and federated cloud services.

Start here:

- [`docs/FEDIVERSITY_REVIEWER_PATH.md`](docs/FEDIVERSITY_REVIEWER_PATH.md)
- [`docs/FEDERATED_EVENT_SOURCING_ALIGNMENT.md`](docs/FEDERATED_EVENT_SOURCING_ALIGNMENT.md)
- [`docs/ACTIVITYPUB_MATRIX_INTEGRATION_PLAN.md`](docs/ACTIVITYPUB_MATRIX_INTEGRATION_PLAN.md)
- [`docs/BUDGET_AND_MILESTONES_FEDIVERSITY.md`](docs/BUDGET_AND_MILESTONES_FEDIVERSITY.md)

Reviewer quick commands:

```bash
cargo build --release -p liminal-cli
cargo test --workspace
```

Windows demo:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\demo-stack.ps1
```

Grant metadata:

```text
Application: 2026-08-00c
Fund: NGI Fediversity
Requested amount: EUR 50,000
Correct repository: https://github.com/safal207/LiminalDB
```
```
