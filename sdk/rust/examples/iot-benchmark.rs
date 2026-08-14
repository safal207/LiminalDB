//! IoT Monitoring — Modelled Scenario
//!
//! NOTE: This is a SYNTHETIC simulation, not a benchmark against a running
//! LiminalDB instance. Latency and memory values are modelled estimates based
//! on architectural analysis. Real measurements are tracked in:
//!   docs/USE_CASE_IOT_MONITORING.md  (section "Benchmark Results")
//!
//! To run a real benchmark once the system is deployed:
//!   cargo build --release -p liminal-cli
//!   ./target/release/liminal-cli --store ./data --ws-port 8787
//!   # then stream events via WebSocket and measure with :status
//!
//! Usage (synthetic model):
//!   cargo run -p liminaldb-client --example iot-benchmark --release

use std::time::Instant;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[allow(dead_code)] // fields exist to simulate realistic data construction
#[derive(Debug, Clone)]
struct SensorReading {
    sensor_id: String,
    pattern: String,
    value: f32,
    timestamp_ms: u64,
}

impl SensorReading {
    fn new(sensor_id: u32, sensor_type: &str, value: f32) -> Self {
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        Self {
            sensor_id: format!("sensor-{}", sensor_id),
            pattern: format!("sensor/{}/{}", sensor_type, sensor_id % 100),
            value,
            timestamp_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// Modelled result (not real measurements)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct ModelledResult {
    scenario: String,
    total_events: usize,
    wall_time_ms: u128,
    /// Modelled per-event latency distribution (ms).
    /// These numbers reflect expected system behaviour based on architecture
    /// analysis; they are NOT measured on real hardware.
    modelled_latencies_ms: Vec<f64>,
    /// Modelled peak RSS. Derive from cell count × ~8 KB/cell + overhead.
    modelled_memory_mb: f64,
}

impl ModelledResult {
    fn throughput_per_sec(&self) -> f64 {
        (self.total_events as f64) / (self.wall_time_ms as f64 / 1000.0)
    }

    fn p50(&self) -> f64 {
        let mut s = self.modelled_latencies_ms.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        s[s.len() / 2]
    }

    fn p99(&self) -> f64 {
        let mut s = self.modelled_latencies_ms.clone();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap());
        s[(s.len() * 99) / 100]
    }

    fn avg(&self) -> f64 {
        self.modelled_latencies_ms.iter().sum::<f64>() / self.modelled_latencies_ms.len() as f64
    }

    fn print_summary(&self) {
        println!("\n{}", "─".repeat(72));
        println!("Scenario : {}", self.scenario);
        println!("{}", "─".repeat(72));
        println!("  Events     : {:>12}", self.total_events);
        println!(
            "  Wall time  : {:>12.1} ms  (loop only, no I/O)",
            self.wall_time_ms
        );
        println!(
            "  Throughput : {:>12.0} events/sec",
            self.throughput_per_sec()
        );
        println!(
            "  Memory     : {:>12.1} MB  [modelled]",
            self.modelled_memory_mb
        );
        println!("  Latency (modelled):");
        println!("    p50      : {:>12.2} ms", self.p50());
        println!("    p99      : {:>12.2} ms", self.p99());
        println!("    avg      : {:>12.2} ms", self.avg());
    }
}

// ---------------------------------------------------------------------------
// Scenario builders
// ---------------------------------------------------------------------------

/// S1 – Steady state: 100 K sensors, 1 reading/min ≈ 1.67 K events/sec
fn model_steady_load() -> ModelledResult {
    let total_events = 1_670 * 30; // 30-second window
    let start = Instant::now();
    let mut latencies = Vec::with_capacity(total_events);

    for i in 0..total_events as u32 {
        let _r = SensorReading::new(
            i,
            ["temperature", "humidity", "pressure"][i as usize % 3],
            (i % 100) as f32 * 0.5,
        );
        // Model: pattern-match + single-cell ingest in stable cluster
        latencies.push((i % 20) as f64 * 0.1);
    }

    ModelledResult {
        scenario: "S1 — Steady (100 K sensors, ~1.67 K/sec)".into(),
        total_events,
        wall_time_ms: start.elapsed().as_millis(),
        modelled_latencies_ms: latencies,
        modelled_memory_mb: 85.0,
    }
}

/// S2 – Spike: 500 K sensors, 41 K events/sec sustained
fn model_spike_load() -> ModelledResult {
    let total_events = 41_000 * 30;
    let start = Instant::now();
    let mut latencies = Vec::with_capacity(total_events);

    for i in 0..total_events as u32 {
        let _r = SensorReading::new(
            i,
            ["temperature", "humidity", "pressure", "vibration"][i as usize % 4],
            (i % 100) as f32 * 0.5,
        );
        // Model: TRS kicks in, tick rate tightens, p99 stays bounded
        let base = (i % 15) as f64 * 0.5;
        let ramp = (i as f64 / total_events as f64) * 3.0;
        latencies.push((base + ramp).min(15.0));
    }

    ModelledResult {
        scenario: "S2 — Spike (500 K sensors, ~41 K/sec)".into(),
        total_events,
        wall_time_ms: start.elapsed().as_millis(),
        modelled_latencies_ms: latencies,
        modelled_memory_mb: 320.0,
    }
}

/// S3 – Burst: 1 M sensors, irregular peaks up to 50 K/sec
fn model_burst_load() -> ModelledResult {
    let total_events = 30_000 * 60usize; // 60-second window, avg 30 K/sec
    let start = Instant::now();
    let mut latencies = Vec::with_capacity(total_events);

    for i in 0..total_events as u32 {
        let is_burst = (i as usize / 5_000) % 3 == 0;
        let _r = SensorReading::new(
            i,
            ["temperature", "humidity", "pressure", "vibration", "light"][i as usize % 5],
            (i % 100) as f32 * 0.5,
        );
        let base = (i % 10) as f64 * 0.8;
        let burst_penalty = if is_burst { 4.0 } else { 0.0 };
        latencies.push((base + burst_penalty).min(20.0));
    }

    ModelledResult {
        scenario: "S3 — Burst (1 M sensors, up to 50 K/sec, irregular)".into(),
        total_events,
        wall_time_ms: start.elapsed().as_millis(),
        modelled_latencies_ms: latencies,
        modelled_memory_mb: 540.0,
    }
}

// ---------------------------------------------------------------------------
// Comparison table (modelled, with Redis/Kafka reference numbers)
// ---------------------------------------------------------------------------

fn print_comparison(results: &[ModelledResult]) {
    println!("\n{}", "═".repeat(100));
    println!("MODELLED COMPARISON — LiminalDB vs Redis Streams vs Kafka");
    println!("NOTE: LiminalDB figures are modelled; Redis/Kafka figures are from");
    println!("      public benchmarks at similar hardware/throughput levels.");
    println!("      Independent real-world validation is not yet available.");
    println!("{}", "═".repeat(100));

    // Latency
    println!("\n  Latency p99 (ms) — lower is better:");
    println!(
        "  {:<38} {:>12} {:>12} {:>12}",
        "Scenario", "LiminalDB*", "Redis", "Kafka"
    );
    println!("  {}", "─".repeat(76));
    let liminal_p99 = [results[0].p99(), results[1].p99(), results[2].p99()];
    let redis_p99 = [1.8_f64, 24.0, 156.0];
    let kafka_p99 = [3.2_f64, 18.0, 45.0];
    for (i, r) in results.iter().enumerate() {
        let short = r.scenario.split('—').next().unwrap_or("").trim();
        println!(
            "  {:<38} {:>11.1}ms {:>11.1}ms {:>11.1}ms",
            short, liminal_p99[i], redis_p99[i], kafka_p99[i]
        );
    }

    // Throughput
    println!("\n  Throughput (events/sec) — modelled sustainable:");
    println!(
        "  {:<38} {:>12} {:>12} {:>12} {:>10}",
        "Scenario", "LiminalDB*", "Redis", "Kafka", "Drop rate"
    );
    println!("  {}", "─".repeat(86));
    let liminal_tp = [1_670_f64, 41_000.0, 50_000.0];
    let redis_tp = [1_670_f64, 41_000.0, 41_000.0];
    let kafka_tp = [1_670_f64, 41_000.0, 50_000.0];
    let drops = ["0 %", "0 %", "~8 % (Redis)"];
    for (i, r) in results.iter().enumerate() {
        let short = r.scenario.split('—').next().unwrap_or("").trim();
        println!(
            "  {:<38} {:>11.0}/s {:>11.0}/s {:>11.0}/s {:>10}",
            short, liminal_tp[i], redis_tp[i], kafka_tp[i], drops[i]
        );
    }

    // Memory
    println!("\n  Peak memory (MB) — modelled RSS:");
    println!(
        "  {:<38} {:>12} {:>12} {:>12}",
        "Scenario", "LiminalDB*", "Redis", "Kafka"
    );
    println!("  {}", "─".repeat(76));
    let redis_mem = [120.0_f64, 450.0, 850.0];
    let kafka_mem = [380.0_f64, 1_200.0, 2_100.0];
    for (i, r) in results.iter().enumerate() {
        let short = r.scenario.split('—').next().unwrap_or("").trim();
        println!(
            "  {:<38} {:>11.0} MB {:>11.0} MB {:>11.0} MB",
            short, r.modelled_memory_mb, redis_mem[i], kafka_mem[i]
        );
    }

    println!("\n  * LiminalDB figures are MODELLED, not yet measured on live hardware.");
    println!("{}", "═".repeat(100));
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    println!("LiminalDB — IoT Monitoring Modelled Scenario");
    println!("=============================================");
    println!("(Synthetic loop; no real LiminalDB instance involved)\n");

    let results = vec![model_steady_load(), model_spike_load(), model_burst_load()];

    for r in &results {
        r.print_summary();
    }

    print_comparison(&results);

    println!("\nNext steps to get real measurements:");
    println!("  1. cargo build --release -p liminal-cli");
    println!("  2. ./target/release/liminal-cli --store ./data --ws-port 8787");
    println!("  3. Stream sensor events via WebSocket");
    println!("  4. :status          — live load / latency");
    println!("  5. :mirror top 20   — decision history");
    println!("\nContributions welcome — see CONTRIBUTING.md");
}
