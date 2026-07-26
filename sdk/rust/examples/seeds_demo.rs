use liminaldb_client::{ClientOptions, LiminalClient};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut opts = ClientOptions::default();
    opts.url = "wss://demo.liminaldb.dev/ws".parse()?;
    let client = LiminalClient::connect(opts).await?;

    client.auth().await;
    let seed_id = client
        .seed_plant(json!({
            "name": "demo-seed",
            "parameters": {"boost": 0.2}
        }))
        .await;
    println!("Planted seed {}", seed_id);

    client.seed_garden().await;
    client.seed_abort(&seed_id, Some("cleanup"));

    Ok(())
}
