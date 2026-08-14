use std::env;
use std::fs;
use std::path::PathBuf;

use liminal_store::{
    ProofPathAppendOutcome, ProofPathDurableInput, ProofPathDurableLedger,
};
use serde_json::json;

fn usage() -> ! {
    eprintln!(
        "usage:\n  proofpath_durable_ingestion ingest <root> <namespace> <logical_operation_id> <event_file> <admission_file> <source_receipt_ref> <valid_time_ms> <transaction_time_ms> <storage_admission_ref>\n  proofpath_durable_ingestion inspect <root> <namespace> <logical_operation_id> <event_output> <admission_output>"
    );
    std::process::exit(2);
}

fn parse_u64(value: &str, label: &str) -> Result<u64, Box<dyn std::error::Error>> {
    value
        .parse::<u64>()
        .map_err(|error| format!("invalid {label}: {error}").into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let Some(command) = args.get(1).map(String::as_str) else {
        usage();
    };

    match command {
        "ingest" => {
            if args.len() != 11 {
                usage();
            }
            let root = PathBuf::from(&args[2]);
            let namespace = &args[3];
            let logical_operation_id = &args[4];
            let event_bytes = fs::read(&args[5])?;
            let admission_bytes = fs::read(&args[6])?;
            let source_receipt_ref = args[7].clone();
            let valid_time_ms = parse_u64(&args[8], "valid_time_ms")?;
            let transaction_time_ms = parse_u64(&args[9], "transaction_time_ms")?;
            let storage_admission_ref = args[10].clone();

            let mut ledger = ProofPathDurableLedger::open(&root, namespace.clone())?;
            let outcome = ledger.append(ProofPathDurableInput {
                logical_operation_id: logical_operation_id.clone(),
                source_event_bytes: event_bytes,
                admission_report_bytes: admission_bytes,
                source_receipt_ref,
                valid_time_ms,
                transaction_time_ms,
                storage_admission_ref,
            })?;
            let status = match &outcome {
                ProofPathAppendOutcome::Inserted(_) => "INSERTED",
                ProofPathAppendOutcome::AlreadyPresent(_) => "ALREADY_PRESENT",
            };
            let record = outcome.record();
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "status": status,
                    "namespace": ledger.namespace(),
                    "event_count": ledger.event_count(),
                    "logical_operation_id": record.body.logical_operation_id,
                    "ingestion_key": record.body.ingestion_key,
                    "record_hash": record.record_hash,
                    "source_event_sha256": record.body.source_event_sha256,
                    "admission_report_sha256": record.body.admission_report_sha256,
                    "valid_time_ms": record.body.valid_time_ms,
                    "transaction_time_ms": record.body.transaction_time_ms,
                    "persistence_scope": record.body.persistence_scope,
                    "storage_write_authorized": record.body.storage_write_authorized,
                    "execution_authorized": record.body.execution_authorized,
                    "mutation_authorized": record.body.mutation_authorized,
                    "external_effects_authorized": record.body.external_effects_authorized,
                }))?
            );
        }
        "inspect" => {
            if args.len() != 8 {
                usage();
            }
            let root = PathBuf::from(&args[2]);
            let namespace = &args[3];
            let logical_operation_id = &args[4];
            let event_output = PathBuf::from(&args[5]);
            let admission_output = PathBuf::from(&args[6]);
            let summary_output = PathBuf::from(&args[7]);

            let ledger = ProofPathDurableLedger::open(&root, namespace.clone())?;
            let record = ledger
                .get(logical_operation_id)
                .ok_or_else(|| format!("logical operation not found: {logical_operation_id}"))?;
            if let Some(parent) = event_output.parent() {
                fs::create_dir_all(parent)?;
            }
            if let Some(parent) = admission_output.parent() {
                fs::create_dir_all(parent)?;
            }
            if let Some(parent) = summary_output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&event_output, &record.body.source_event_bytes)?;
            fs::write(&admission_output, &record.body.admission_report_bytes)?;
            fs::write(
                &summary_output,
                serde_json::to_vec_pretty(&json!({
                    "namespace": ledger.namespace(),
                    "event_count": ledger.event_count(),
                    "logical_operation_id": record.body.logical_operation_id,
                    "ingestion_key": record.body.ingestion_key,
                    "record_hash": record.record_hash,
                    "source_event_sha256": record.body.source_event_sha256,
                    "source_receipt_ref": record.body.source_receipt_ref,
                    "admission_report_sha256": record.body.admission_report_sha256,
                    "producer_capability_commit": record.body.producer_capability_commit,
                    "consumer_import_commit": record.body.consumer_import_commit,
                    "consumer_contract_blob_sha": record.body.consumer_contract_blob_sha,
                    "valid_time_ms": record.body.valid_time_ms,
                    "transaction_time_ms": record.body.transaction_time_ms,
                    "persistence_scope": record.body.persistence_scope,
                    "storage_write_authorized": record.body.storage_write_authorized,
                    "execution_authorized": record.body.execution_authorized,
                    "mutation_authorized": record.body.mutation_authorized,
                    "external_effects_authorized": record.body.external_effects_authorized,
                }))?,
            )?;
            println!("INSPECTED {}", record.record_hash);
        }
        _ => usage(),
    }
    Ok(())
}
