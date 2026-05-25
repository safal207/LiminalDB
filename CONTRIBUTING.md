# Contributing to LiminalDB

Thank you for your interest in contributing to LiminalDB! This is an open-source project dedicated to building the next generation of self-adaptive databases.

## Code of Conduct

We are committed to providing a welcoming and inclusive environment. Please be respectful and constructive in all interactions.

**Core Values:**
- 🙏 Respectful collaboration
- 🎯 Clear communication
- 🌱 Continuous learning
- 🤝 Community over ego

## How to Contribute

### 1. **Report Issues**

Found a bug or have a feature request? Open an issue:

```bash
# Go to: https://github.com/safal207/LiminalBD/issues
# Click "New Issue"
# Provide:
# - Clear title
# - Steps to reproduce
# - Expected vs actual behavior
# - Environment (OS, Rust version)
```

### First-contributor lane

If you are new to the repository, start with:

- [`docs/FIRST_CONTRIBUTOR_ISSUES.md`](docs/FIRST_CONTRIBUTOR_ISSUES.md)

Each issue includes scope limits, direct context links, and acceptance
criteria so first PRs stay reviewable.

### 2. **Improve Documentation**

Documentation is crucial and often needs help:

- **README improvements** – Clarify confusing sections
- **API documentation** – Add rustdoc comments
- **Tutorials** – Write step-by-step guides
- **Examples** – Create real-world code samples
- **Blog posts** – Share insights and use cases

```bash
# Fork the repo
git clone https://github.com/YOUR_USERNAME/LiminalBD.git
cd LiminalBD

# Create feature branch
git checkout -b docs/improve-pattern-matching

# Make changes
# Commit with clear message
git commit -m "docs: clarify pattern matching semantics with examples"

# Push and open PR
git push origin docs/improve-pattern-matching
```

### 3. **Write Tests**

Help us achieve 80%+ test coverage.

> **Illustrative examples** — the snippets below show the intended testing
> patterns. Some API signatures may evolve as the codebase matures.
> Run `cargo test --all` to see the current passing test suite.

```rust
// liminal-core/src/tests/ — illustrative patterns

#[test]
fn test_cell_division_at_energy_threshold() {
    let mut cell = NodeCell::new(NodeId(1), "test/pattern");
    cell.energy = 0.85;
    
    assert!(cell.should_divide());
    
    let child = cell.clone_mutated(NodeId(2), 0.1);
    assert_eq!(child.energy, cell.energy / 2.0);
}

#[tokio::test]
async fn test_impulse_routing_with_affinity() {
    let mut field = ClusterField::new();
    
    // Setup cells with different affinities
    let cell_high = field.spawn_with_affinity("cpu/load", 0.9);
    let cell_low = field.spawn_with_affinity("cpu/load", 0.3);
    
    // Send impulse
    field.ingest_impulse(Impulse::query("cpu/load", 0.8));
    field.tick_all();
    
    // High affinity cell should have higher energy consumption
    let high_cell = field.get_cell(cell_high).unwrap();
    let low_cell = field.get_cell(cell_low).unwrap();
    
    assert!(high_cell.energy < low_cell.energy);
}
```

**Good test practices:**
- Test one behavior per function
- Use descriptive names: `test_x_when_y_then_z`
- Include setup, action, and assertion phases
- Mock external dependencies
- Aim for critical path coverage first

### 4. **Optimize Performance**

Help us meet (or beat) our performance targets:

| Target | Metric |
|--------|--------|
| <5ms p99 | Cell routing for 10K cells |
| >10K/sec | Impulse throughput |
| <100MB | Memory for 10K cells |
| <500ms | Snapshot write for 100MB state |

**How to optimize:**
```bash
# Synthetic benchmark harness
cargo run -p liminaldb-client --example iot-benchmark --release

# Benchmark status and evidence notes
cat docs/BENCHMARKS.md

# Profile with perf
cargo flamegraph --bin liminal-cli
perf report

# Check allocations
valgrind ./target/debug/liminal-cli

# Compare before/after
# Submit PR with benchmark results
```

### 5. **Build SDKs**

We have Rust and TypeScript SDKs. Help extend them:

**Wanted SDKs:**
- [ ] Python (popular for data science)
- [ ] Go (popular for infrastructure)
- [ ] Java (enterprise)
- [ ] C# (.NET)

**SDK Structure:**
```
sdk/python/
├── liminaldb/
│   ├── __init__.py
│   ├── client.py       # Main API
│   ├── protocol.py     # Serialization
│   └── types.py        # Data types
├── examples/
├── tests/
├── setup.py
└── README.md
```

### 6. **Implement Roadmap Items**

Check [docs/ROADMAP.md](docs/ROADMAP.md) for the
full prioritised backlog. Current focus areas:

- **Architecture:** Refactor `cluster_field.rs` into bounded contexts
- **Type safety:** Newtype IDs (`NodeId`, `EpochId`, `SeedId`)
- **Observability:** OpenTelemetry tracing, decision logging, Grafana dashboards
- **Distribution:** Multi-node cluster, Raft consensus, horizontal auto-scaling

## Development Setup

### Prerequisites

```bash
# Rust 1.75+ (check current MSRV in Cargo.toml)
rustc --version

# Cargo
cargo --version

# Optional: for profiling
cargo install flamegraph

# Optional: for docs generation
cargo doc --open
```

### Build & Test

```bash
# Build all crates
cargo build --all

# Run all tests
cargo test --all

# Run specific tests
cargo test -p liminal-core pattern_matching

# Run tests with output
cargo test -- --nocapture

# Check code quality
cargo clippy --all

# Format code
cargo fmt

# Generate documentation
cargo doc --open
```

### Development Workflow

```bash
# 1. Create feature branch
git checkout -b feat/improve-pattern-matching

# 2. Make changes, write tests
# 3. Run test suite
cargo test --all

# 4. Check formatting and linting
cargo fmt --all
cargo clippy --all

# 5. Commit with clear message and DCO sign-off
git commit -s -m "feat: add fuzzy pattern matching with affinity weighting

- Implement Levenshtein distance for pattern similarity
- Weight matches by cell affinity values
- Add 50+ test cases covering edge cases
- Performance: <1ms for 10K cell lookups

Fixes #123"

# 6. Push and open PR
git push origin feat/improve-pattern-matching
```

By contributing, you agree to sign off commits as described in `DCO.md`.

## PR Guidelines

### Before Submitting

- [ ] Tests written and passing
- [ ] Code formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Documentation updated
- [ ] Commit messages are clear
- [ ] PR description explains what and why

### PR Template

```markdown
## Description
Clear explanation of what this PR does and why.

## Related Issues
Fixes #123
Related to #456

## Changes
- Bullet point summary
- Of key changes
- For reviewer

## Testing
How to test these changes:
```
cargo test -p liminal-core my_new_feature
```

## Performance Impact
- Latency: <1ms overhead
- Memory: +2MB for 10K cells
- Throughput: +5% improvement

## Screenshots / Traces
(if applicable)
```

### What Makes a Good PR?

✅ **Good:**
- Focused scope — one area of concern per PR where possible
- Clear commit history (logical, reviewable units)
- Tests for new behaviour
- Documentation updated to match
- Performance benchmarks included for performance-affecting changes

❌ **Avoid:**
- Mixing unrelated features in a single PR (e.g. new feature + unrelated refactor)
- Burying a breaking change in a large diff without calling it out
- No explanation of *why* in the PR description

> **Note:** Some PRs are necessarily broad — for example, a documentation
> foundation PR or a licence change touches many files by nature.
> In those cases, a clear PR description that explains the unified purpose
> is more important than minimising file count.

## Areas We Need Help

### High Priority (funding-friendly!)

1. **Benchmarking & Performance**
   - Compare vs Redis, Kafka, ClickHouse
   - Real-world use case validation
   - Performance regression testing

2. **Documentation**
   - Architecture deep-dives
   - Tutorial series
   - API reference
   - Philosophy guide

3. **Community**
   - Blog posts about use cases
   - Twitter/LinkedIn demos
   - Conference talks
   - Open-source discussions

### Medium Priority

4. **Testing**
   - Property-based testing with `proptest`
   - Chaos engineering
   - Load testing
   - Failure scenario testing

5. **Observability**
   - Tracing integration
   - Metrics export
   - Grafana dashboards
   - Alert templates

6. **Distributed Systems**
   - Multi-node cluster
   - Consensus protocols
   - Cross-region replication
   - Failure recovery

## Getting Unstuck

**Questions?**
- 📖 Read the [Architecture Analysis](docs/ARCHITECTURE_ANALYSIS.md)
- 💬 Open a discussion: https://github.com/safal207/LiminalBD/discussions
- 📧 Email: safal0645@gmail.com

**Want to pair program?**
- We welcome pair programming sessions
- Great for learning the codebase
- Contact us to schedule

## Recognition

Contributors are recognized in:
- [CONTRIBUTORS.md](CONTRIBUTORS.md) (coming soon)
- GitHub profile (automatic)
- Release notes
- Monthly community calls

## License

By contributing, you agree that your contributions will be licensed under the Apache-2.0 License.

---

**May your contributions help many.** 🙏
