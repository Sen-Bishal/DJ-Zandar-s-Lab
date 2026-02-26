use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::engine::{AmphoreusEngine, GlobalState};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObserverSnapshot {
    pub state: GlobalState,
    pub entropy_samples: Vec<f64>,
}

#[derive(Clone)]
pub struct SharedObserverSnapshot {
    inner: Arc<RwLock<ObserverSnapshot>>,
}

impl SharedObserverSnapshot {
    pub fn new(initial: ObserverSnapshot) -> Self {
        Self {
            inner: Arc::new(RwLock::new(initial)),
        }
    }

    pub fn read(&self) -> ObserverSnapshot {
        self.inner.read().clone()
    }

    fn update(&self, next: ObserverSnapshot) {
        *self.inner.write() = next;
    }
}

pub struct ObserverRuntime {
    shared: SharedObserverSnapshot,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl ObserverRuntime {
    /// Runs simulation with a fixed timestep loop on a dedicated thread.
    pub fn spawn(mut engine: AmphoreusEngine, tick_hz: u64, max_samples: usize) -> Self {
        let tick_hz = tick_hz.max(1);
        let max_samples = max_samples.max(16);
        let fixed_dt_nanos = (1_000_000_000_u64 / tick_hz).max(1);
        let fixed_dt = Duration::from_nanos(fixed_dt_nanos);
        let idle_sleep = Duration::from_millis(1);
        let max_catch_up_steps = 8_u32;

        let shared = SharedObserverSnapshot::new(ObserverSnapshot {
            state: engine.state,
            entropy_samples: Vec::with_capacity(max_samples),
        });
        let shared_for_thread = shared.clone();

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_for_thread = Arc::clone(&shutdown);

        let handle = thread::Builder::new()
            .name("amphoreus-engine-thread".to_owned())
            .spawn(move || {
                let mut entropy_history = VecDeque::with_capacity(max_samples);
                let mut previous_frame = Instant::now();
                let mut accumulator = Duration::ZERO;

                while !shutdown_for_thread.load(Ordering::Relaxed) {
                    let now = Instant::now();
                    let frame_time = now.saturating_duration_since(previous_frame);
                    previous_frame = now;

                    // Clamp to prevent runaway catch-up after long stalls.
                    let clamped_frame = frame_time.min(fixed_dt.saturating_mul(max_catch_up_steps));
                    accumulator = accumulator.saturating_add(clamped_frame);

                    let mut steps = 0_u32;
                    while accumulator >= fixed_dt && steps < max_catch_up_steps {
                        let _ = engine.tick();
                        accumulator = accumulator.saturating_sub(fixed_dt);
                        steps += 1;

                        entropy_history.push_back(engine.state.destruction_entropy);
                        if entropy_history.len() > max_samples {
                            let _ = entropy_history.pop_front();
                        }
                    }

                    if steps > 0 {
                        shared_for_thread.update(ObserverSnapshot {
                            state: engine.state,
                            entropy_samples: entropy_history.iter().copied().collect(),
                        });
                    } else {
                        thread::sleep(idle_sleep);
                    }
                }
            })
            .expect("failed to spawn amphoreus engine thread");

        Self {
            shared,
            shutdown,
            handle: Some(handle),
        }
    }

    pub fn shared_snapshot(&self) -> SharedObserverSnapshot {
        self.shared.clone()
    }
}

impl Drop for ObserverRuntime {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
#[tauri::command]
pub fn read_observer_snapshot(state: tauri::State<'_, SharedObserverSnapshot>) -> ObserverSnapshot {
    state.read()
}

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
#[tauri::command]
pub fn read_global_state(state: tauri::State<'_, SharedObserverSnapshot>) -> GlobalState {
    state.read().state
}

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
#[tauri::command]
pub fn read_entropy_series(state: tauri::State<'_, SharedObserverSnapshot>) -> Vec<f64> {
    state.read().entropy_samples
}

#[cfg(all(feature = "desktop", not(target_arch = "wasm32")))]
pub fn wire_tauri_observer(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        read_observer_snapshot,
        read_global_state,
        read_entropy_series
    ])
}

#[cfg(all(feature = "web-ui", target_arch = "wasm32"))]
pub mod yew_frontend {
    use std::cell::Cell;
    use std::rc::Rc;

    use gloo_timers::future::TimeoutFuture;
    use js_sys::Error;
    use wasm_bindgen::JsValue;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::spawn_local;
    use yew::prelude::*;

    use crate::engine::GlobalState;
    use crate::observer::ObserverSnapshot;

    #[wasm_bindgen(inline_js = r#"
    export async function invoke_tauri(command) {
      if (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke) {
        return await window.__TAURI__.core.invoke(command);
      }
      throw new Error("Tauri bridge unavailable");
    }
    "#)]
    extern "C" {
        #[wasm_bindgen(catch, js_name = invoke_tauri)]
        async fn invoke_tauri(command: &str) -> Result<JsValue, JsValue>;
    }

    async fn fetch_global_state() -> Result<GlobalState, JsValue> {
        let value = invoke_tauri("read_global_state").await?;
        serde_wasm_bindgen::from_value(value)
            .map_err(|err| Error::new(&format!("global_state decode failed: {err}")).into())
    }

    async fn fetch_entropy_series() -> Result<Vec<f64>, JsValue> {
        let value = invoke_tauri("read_entropy_series").await?;
        serde_wasm_bindgen::from_value(value)
            .map_err(|err| Error::new(&format!("entropy decode failed: {err}")).into())
    }

    #[derive(Properties, PartialEq)]
    pub struct DashboardProps {
        #[prop_or(75)]
        pub poll_ms: u32,
        #[prop_or(240)]
        pub max_points: usize,
    }

    #[function_component(Dashboard)]
    pub fn dashboard(props: &DashboardProps) -> Html {
        let snapshot = use_state_eq(|| ObserverSnapshot {
            state: GlobalState::default(),
            entropy_samples: Vec::new(),
        });
        let in_flight = use_mut_ref(|| false);

        {
            let snapshot = snapshot.clone();
            let poll_ms = props.poll_ms.max(16);
            let max_points = props.max_points.max(16);
            let in_flight = in_flight.clone();
            use_effect_with((poll_ms, max_points), move |_| {
                let running = Rc::new(Cell::new(true));
                let running_task = Rc::clone(&running);

                spawn_local(async move {
                    while running_task.get() {
                        TimeoutFuture::new(poll_ms).await;
                        if !running_task.get() {
                            break;
                        }

                        if *in_flight.borrow() {
                            continue;
                        }

                        *in_flight.borrow_mut() = true;

                        if let (Ok(state), Ok(mut entropy_samples)) =
                            (fetch_global_state().await, fetch_entropy_series().await)
                        {
                            if entropy_samples.len() > max_points {
                                let start = entropy_samples.len() - max_points;
                                entropy_samples = entropy_samples[start..].to_vec();
                            }

                            let next = ObserverSnapshot {
                                state,
                                entropy_samples,
                            };

                            if *snapshot != next {
                                snapshot.set(next);
                            }
                        }

                        *in_flight.borrow_mut() = false;
                    }
                });

                move || running.set(false)
            });
        }

        html! {
            <section class="amphoreus-dashboard" style="font-family: 'IBM Plex Sans', sans-serif; padding: 20px; background: linear-gradient(120deg, #f4f7e8 0%, #f8f0dd 100%); color: #2f3b2f; border-radius: 12px;">
                <h1 style="margin-top: 0;">{ "Project AMPHOREUS Observer" }</h1>
                <p>{ format!("Cycle Count: {}", snapshot.state.cycle_count) }</p>
                <p>{ format!("Destruction Entropy: {:.6}", snapshot.state.destruction_entropy) }</p>
                <p>{ format!("Time Concept Active: {}", snapshot.state.time_concept_active) }</p>
                <EntropyChart samples={snapshot.entropy_samples.clone()} />
            </section>
        }
    }

    #[derive(Properties, PartialEq)]
    pub struct EntropyChartProps {
        pub samples: Vec<f64>,
        #[prop_or(680)]
        pub width: u32,
        #[prop_or(220)]
        pub height: u32,
    }

    #[function_component(EntropyChart)]
    pub fn entropy_chart(props: &EntropyChartProps) -> Html {
        let width = props.width.max(100) as f64;
        let height = props.height.max(80) as f64;
        let samples = &props.samples;
        let count = samples.len().max(2);

        let points = samples
            .iter()
            .enumerate()
            .map(|(idx, sample)| {
                let x = if count <= 1 {
                    0.0
                } else {
                    (idx as f64 / (count - 1) as f64) * width
                };
                let y = height - (sample.clamp(0.0, 1.0) * height);
                format!("{x:.2},{y:.2}")
            })
            .collect::<Vec<String>>()
            .join(" ");

        let target_line_y = height * 0.02;

        html! {
            <svg width={props.width.to_string()} height={props.height.to_string()} viewBox={format!("0 0 {width} {height}")} style="display: block; margin-top: 12px; background: #fffef7; border: 1px solid #d8d2bf; border-radius: 10px;">
                <line x1="0" y1={target_line_y.to_string()} x2={width.to_string()} y2={target_line_y.to_string()} stroke="#b94a48" stroke-dasharray="4 4" />
                <polyline points={points} fill="none" stroke="#0d5c63" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
        }
    }
}
