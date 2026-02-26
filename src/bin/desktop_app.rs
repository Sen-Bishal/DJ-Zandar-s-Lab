#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
fn main() {
    use Amphoreus::ecs::init_global_ecs;
    use Amphoreus::engine::{AmphoreusEngine, WorldSeedConfig};
    use Amphoreus::observer::{ObserverRuntime, SharedObserverSnapshot};

    #[tauri::command]
    fn read_observer_snapshot(state: tauri::State<'_, SharedObserverSnapshot>) -> Amphoreus::observer::ObserverSnapshot {
        state.read()
    }

    #[tauri::command]
    fn read_global_state(state: tauri::State<'_, SharedObserverSnapshot>) -> Amphoreus::engine::GlobalState {
        state.read().state
    }

    #[tauri::command]
    fn read_entropy_series(state: tauri::State<'_, SharedObserverSnapshot>) -> Vec<f64> {
        state.read().entropy_samples
    }

    init_global_ecs(1_500_000);

    let mut engine = AmphoreusEngine::new(256 * 1024 * 1024);
    engine.seed_world(WorldSeedConfig {
        citizens: 20_000,
        titans: 500,
        chrysos_heirs: 128,
    });

    let runtime = ObserverRuntime::spawn(engine, 60, 600);
    let shared = runtime.shared_snapshot();

    tauri::Builder::default()
        .manage(shared)
        .invoke_handler(tauri::generate_handler![
            read_observer_snapshot,
            read_global_state,
            read_entropy_series
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Project AMPHOREUS desktop app");
}

#[cfg(any(not(feature = "desktop"), target_arch = "wasm32"))]
fn main() {
    eprintln!("desktop app requires: cargo run --features desktop --bin desktop_app");
}
