use std::thread;
use std::time::Duration;

use Amphoreus::ecs::init_global_ecs;
use Amphoreus::engine::{AmphoreusEngine, WorldSeedConfig};
use Amphoreus::observer::ObserverRuntime;

fn main() {
    init_global_ecs(1_500_000);

    let mut engine = AmphoreusEngine::new(256 * 1024 * 1024);
    engine.seed_world(WorldSeedConfig {
        citizens: 20_000,
        titans: 500,
        chrysos_heirs: 128,
    });

    let runtime = ObserverRuntime::spawn(engine, 60, 360);
    let shared = runtime.shared_snapshot();

    for _ in 0..6 {
        thread::sleep(Duration::from_millis(500));
        let snapshot = shared.read();
        println!(
            "cycle={} entropy={:.6} time_active={} samples={}",
            snapshot.state.cycle_count,
            snapshot.state.destruction_entropy,
            snapshot.state.time_concept_active,
            snapshot.entropy_samples.len()
        );
    }
}
