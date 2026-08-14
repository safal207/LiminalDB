use std::time::Duration;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sysinfo::{RefreshKind, System};
use tokio::sync::mpsc;
use tokio::time::sleep;

use liminal_core::types::{Impulse, ImpulseKind};

pub async fn start_host_sensors(tx: mpsc::Sender<Impulse>) {
    let mut system = System::new_with_specifics(RefreshKind::everything());
    let mut tick: f32 = 0.0;
    let mut rng = ChaCha8Rng::seed_from_u64(42);

    loop {
        system.refresh_cpu_usage();
        system.refresh_memory();

        let cpu_usage = system.global_cpu_usage() / 100.0;
        let total_memory = system.total_memory() as f32;
        let free_memory = if total_memory > 0.0 {
            system.available_memory() as f32 / total_memory
        } else {
            0.0
        };
        let temp_wave = (tick / 10.0).sin() * 0.25 + 0.5 + rng.gen_range(-0.05..0.05);

        let impulses = vec![
            Impulse {
                kind: ImpulseKind::Affect,
                pattern: "cpu/load".to_string(),
                strength: cpu_usage.clamp(0.0, 1.0),
                ttl_ms: 1_000,
                tags: vec!["host".into()],
            },
            Impulse {
                kind: ImpulseKind::Affect,
                pattern: "mem/free".to_string(),
                strength: free_memory.clamp(0.0, 1.0),
                ttl_ms: 1_000,
                tags: vec!["host".into()],
            },
            Impulse {
                kind: ImpulseKind::Affect,
                pattern: "temp/device".to_string(),
                strength: temp_wave.clamp(0.0, 1.0),
                ttl_ms: 1_000,
                tags: vec!["host".into()],
            },
        ];

        for impulse in impulses {
            if tx.send(impulse).await.is_err() {
                return;
            }
        }

        tick += 1.0;
        sleep(Duration::from_millis(500)).await;
    }
}
