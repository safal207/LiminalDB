# Budget and Milestones for NGI Fediversity

## Grant request

```text
Requested amount: EUR 50,000
Fund: NGI Fediversity
Project: LiminalDB: Federated Event-Sourced Memory Layer
```

## Budget summary

| Area | Amount |
|---|---:|
| Rust runtime and event model | EUR 18,000 |
| Replication and adapter design | EUR 10,000 |
| Testing and reproducible demos | EUR 7,000 |
| Documentation and specifications | EUR 6,000 |
| Community and interoperability work | EUR 5,000 |
| CI, hosting, and test infrastructure | EUR 4,000 |
| **Total** | **EUR 50,000** |

## Milestone 1 — Event envelope and local replay model

Duration: 1 month

Budget: EUR 8,000

Deliverables:

- stable event envelope draft;
- Mirror Timeline replay model;
- local validation notes;
- reviewer quickstart update.

Acceptance checks:

- event format is documented;
- local events can be inspected;
- replay assumptions are clear;
- non-goals are explicit.

## Milestone 2 — Local-first persistence and audit path

Duration: 2 months

Budget: EUR 10,000

Deliverables:

- improved persistence path;
- audit and replay documentation;
- pruning and compaction design;
- deterministic test fixtures.

Acceptance checks:

- local runtime can persist event history;
- events can be replayed or inspected;
- pruning risks are documented;
- tests cover core lifecycle flows.

## Milestone 3 — Federated replication design

Duration: 2 months

Budget: EUR 11,000

Deliverables:

- mock node-to-node replication design;
- duplicate detection;
- remote event validation path;
- conflict-handling notes;
- rejection records.

Acceptance checks:

- mock node A can emit an event;
- mock node B can receive, validate, and record the event;
- duplicate remote events are not blindly applied;
- rejected events leave an auditable record.

## Milestone 4 — Protocol adapter notes

Duration: 1 month

Budget: EUR 7,000

Deliverables:

- ActivityPub mapping note;
- Matrix mapping note;
- adapter interface draft;
- privacy and payload-minimization note.

Acceptance checks:

- ActivityPub concepts map to LiminalDB event fields;
- Matrix concepts map to LiminalDB event fields;
- adapter responsibilities are separated from core runtime responsibilities;
- privacy boundaries are documented.

## Milestone 5 — Developer and reviewer experience

Duration: 1 month

Budget: EUR 8,000

Deliverables:

- reviewer path;
- demo scripts;
- public examples;
- README reviewer section;
- issue templates and contribution notes.

Acceptance checks:

- reviewer can clone and run validation commands;
- docs explain the Fediversity fit;
- examples are reproducible;
- limitations are clear.

## Milestone 6 — Community feedback and final report

Duration: 1 month

Budget: EUR 6,000

Deliverables:

- feedback round with federated-web developers;
- final grant report;
- roadmap update;
- interoperability notes.

Acceptance checks:

- feedback is captured in issues or docs;
- final report explains what was built;
- next steps are realistic and scoped.

## Total timeline

```text
Month 1: event envelope and replay model
Months 2-3: local-first persistence and audit path
Months 4-5: federated replication design
Month 6: protocol adapter notes
Month 7: developer and reviewer experience
Month 8: community feedback and final report
```

## Public outputs

The grant will produce source code, event envelope docs, replay docs, audit docs, mock replication demos, protocol mapping notes, privacy notes, reviewer docs, and a final report.

## Success definition

The project is successful if an independent reviewer can clone the repository, run validation commands, inspect local event history, understand the replication model, and see how ActivityPub and Matrix adapters can connect.
