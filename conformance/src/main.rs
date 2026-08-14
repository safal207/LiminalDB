use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use chrono::Utc;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about = "LiminalDB protocol conformance suite")]
struct Args {
    /// Directory for generated reports
    #[arg(long, default_value = "reports")]
    output_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct Scenario {
    name: &'static str,
    description: &'static str,
    status: ScenarioStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenarioStatus {
    Passed,
    Skipped,
    Failed,
}

impl Scenario {
    fn junit_fragment(&self, suite: &str) -> String {
        match self.status {
            ScenarioStatus::Passed => format!("    <testcase classname=\"{}\" name=\"{}\"/>\n", suite, self.name),
            ScenarioStatus::Skipped => format!("    <testcase classname=\"{}\" name=\"{}\"><skipped message=\"not executed in offline mode\"/></testcase>\n", suite, self.name),
            ScenarioStatus::Failed => format!("    <testcase classname=\"{}\" name=\"{}\"><failure message=\"scenario failed\"/></testcase>\n", suite, self.name),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    fs::create_dir_all(&args.output_dir)?;

    let scenarios = vec![
        Scenario {
            name: "auth_lql_view",
            description: "AUTH → LQL SELECT/SUBSCRIBE → VIEW stream",
            status: ScenarioStatus::Skipped,
        },
        Scenario {
            name: "intent_boost",
            description: "INTENT boost metric measurement",
            status: ScenarioStatus::Skipped,
        },
        Scenario {
            name: "dream_sync_awaken",
            description: "DREAM/SYNC/AWAKEN events validated",
            status: ScenarioStatus::Skipped,
        },
        Scenario {
            name: "seed_cycle",
            description: "SEED plant/harvest lifecycle",
            status: ScenarioStatus::Skipped,
        },
        Scenario {
            name: "noetic_single_node",
            description: "NOETIC propose→vote→commit",
            status: ScenarioStatus::Skipped,
        },
    ];

    write_junit(&args.output_dir, &scenarios)?;
    write_markdown(&args.output_dir, &scenarios)?;

    println!("Generated {} scenario stubs", scenarios.len());
    Ok(())
}

fn write_junit(dir: &PathBuf, scenarios: &[Scenario]) -> anyhow::Result<()> {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<testsuite name=\"liminaldb-conformance\" tests=\"{}\" timestamp=\"{}\">\n",
        scenarios.len(),
        Utc::now().to_rfc3339()
    ));
    for scenario in scenarios {
        xml.push_str(&scenario.junit_fragment("liminaldb"));
    }
    xml.push_str("</testsuite>\n");
    fs::write(dir.join("junit.xml"), xml)?;
    Ok(())
}

fn write_markdown(dir: &PathBuf, scenarios: &[Scenario]) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# LiminalDB Conformance Report\n\n");
    md.push_str("| Scenario | Status | Description |\n");
    md.push_str("| --- | --- | --- |\n");
    for scenario in scenarios {
        let status = match scenario.status {
            ScenarioStatus::Passed => "✅ Passed",
            ScenarioStatus::Skipped => "⚪ Skipped",
            ScenarioStatus::Failed => "❌ Failed",
        };
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            scenario.name, status, scenario.description
        ));
    }
    fs::write(dir.join("report.md"), md).context("failed to write markdown report")?;
    Ok(())
}
