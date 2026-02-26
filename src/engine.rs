use std::fs;
use std::mem::{align_of, size_of};

use bincode::config::standard;
use bincode::serde::encode_to_vec;
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::arena::AmphoreusArena;
use crate::ecs::{
    Coreflame, Entity, GoldenBlood, MemoryLog, Path, with_global_ecs, with_global_ecs_mut,
};
use crate::equation::{DestructionNode, evaluate_destruction_ast};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GlobalState {
    pub cycle_count: u64,
    pub destruction_entropy: f64,
    pub time_concept_active: bool,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            cycle_count: 0,
            destruction_entropy: 0.0,
            time_concept_active: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationResult {
    TickAdvanced,
    TimeBypassed,
    BlackTideTriggered,
}

#[derive(Debug, Clone, Copy)]
pub struct WorldSeedConfig {
    pub citizens: u32,
    pub titans: u32,
    pub chrysos_heirs: u32,
}

impl Default for WorldSeedConfig {
    fn default() -> Self {
        Self {
            citizens: 12_000,
            titans: 320,
            chrysos_heirs: 64,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlameChaseHandles {
    pub phainon: Option<Entity>,
    pub cyrene: Option<Entity>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SpawnEntitySpec {
    pub coreflame: Option<Coreflame>,
    pub memory_log: Option<MemoryLog>,
    pub golden_blood: Option<GoldenBlood>,
}

pub struct AmphoreusEngine {
    pub arena: AmphoreusArena,
    pub state: GlobalState,
    pub flame_chase: FlameChaseHandles,
    pub world_seed: WorldSeedConfig,
    persistent_phainon_memory: MemoryLog,
}

#[derive(Serialize)]
struct ArenaSnapshot<'a> {
    offset: usize,
    memory: &'a [u8],
}

impl AmphoreusEngine {
    pub fn new(arena_capacity: usize) -> Self {
        Self {
            arena: AmphoreusArena::new(arena_capacity),
            state: GlobalState::default(),
            flame_chase: FlameChaseHandles::default(),
            world_seed: WorldSeedConfig::default(),
            persistent_phainon_memory: MemoryLog::default(),
        }
    }

    /// Allocates entity storage in the arena, creates an entity, and writes component columns.
    pub fn spawn_entity(&mut self, spec: SpawnEntitySpec) -> Option<Entity> {
        let allocation_bytes = size_of::<Entity>()
            + spec
                .coreflame
                .map(|_| size_of::<Coreflame>())
                .unwrap_or_default()
            + spec
                .memory_log
                .map(|_| size_of::<MemoryLog>())
                .unwrap_or_default()
            + spec
                .golden_blood
                .map(|_| size_of::<GoldenBlood>())
                .unwrap_or_default();

        let allocation_bytes = allocation_bytes.max(1);
        self.arena
            .alloc_bytes(allocation_bytes, align_of::<u64>())
            .and_then(|_| {
                with_global_ecs_mut(|ecs| {
                    let entity = ecs.spawn();
                    if let Some(coreflame) = spec.coreflame {
                        ecs.coreflames.insert(entity, coreflame);
                    }
                    if let Some(memory_log) = spec.memory_log {
                        ecs.memory_logs.insert(entity, memory_log);
                    }
                    if let Some(golden_blood) = spec.golden_blood {
                        ecs.golden_blood.insert(entity, golden_blood);
                    }
                    entity
                })
            })
    }

    pub fn seed_world(&mut self, seed: WorldSeedConfig) {
        self.world_seed = seed;
        self.arena.trigger_black_tide();
        let _ = with_global_ecs_mut(|ecs| ecs.clear_for_black_tide());
        self.flame_chase = FlameChaseHandles::default();

        self.seed_population_groups();
        self.seed_flame_chase_variables();
        self.apply_cyrene_time_exploit();
    }

    fn seed_population_groups(&mut self) {
        for idx in 0..self.world_seed.citizens {
            let power = (0.28 + ((idx % 97) as f64 * 0.004)).clamp(0.0, 1.0);
            let corruption = ((idx % 37) as f64 * 0.008).clamp(0.0, 0.45);
            let _ = self.spawn_entity(SpawnEntitySpec {
                coreflame: Some(Coreflame {
                    power_level: power,
                    alignment: Path::Erudition,
                }),
                memory_log: Some(MemoryLog {
                    retained_cycles: 0,
                    trauma_index: 0.05,
                }),
                golden_blood: Some(GoldenBlood {
                    corruption_level: corruption,
                }),
            });
        }

        for idx in 0..self.world_seed.titans {
            let power = (1.2 + ((idx % 13) as f64 * 0.07)).clamp(0.0, 3.0);
            let _ = self.spawn_entity(SpawnEntitySpec {
                coreflame: Some(Coreflame {
                    power_level: power,
                    alignment: Path::Destruction,
                }),
                memory_log: Some(MemoryLog {
                    retained_cycles: 2,
                    trauma_index: 0.65,
                }),
                golden_blood: Some(GoldenBlood {
                    corruption_level: 0.72,
                }),
            });
        }

        for idx in 0..self.world_seed.chrysos_heirs {
            let power = (0.9 + ((idx % 11) as f64 * 0.05)).clamp(0.0, 2.0);
            let trauma = (0.2 + ((idx % 7) as f64 * 0.1)).clamp(0.0, 0.95);
            let _ = self.spawn_entity(SpawnEntitySpec {
                coreflame: Some(Coreflame {
                    power_level: power,
                    alignment: Path::Remembrance,
                }),
                memory_log: Some(MemoryLog {
                    retained_cycles: 1,
                    trauma_index: trauma,
                }),
                golden_blood: Some(GoldenBlood {
                    corruption_level: 0.48,
                }),
            });
        }
    }

    /// Spawns Phainon and Cyrene, preserving Phainon's memory across black tides.
    fn seed_flame_chase_variables(&mut self) {
        let phainon = self.spawn_entity(SpawnEntitySpec {
            coreflame: Some(Coreflame {
                power_level: 1.65,
                alignment: Path::Remembrance,
            }),
            memory_log: Some(self.persistent_phainon_memory),
            golden_blood: Some(GoldenBlood {
                corruption_level: 0.52,
            }),
        });

        let cyrene = self.spawn_entity(SpawnEntitySpec {
            coreflame: Some(Coreflame {
                power_level: 1.35,
                alignment: Path::Remembrance,
            }),
            memory_log: Some(MemoryLog {
                retained_cycles: 0,
                trauma_index: 0.92,
            }),
            golden_blood: Some(GoldenBlood {
                corruption_level: 0.33,
            }),
        });

        self.flame_chase = FlameChaseHandles { phainon, cyrene };
    }

    fn apply_cyrene_time_exploit(&mut self) {
        let cyrene = self.flame_chase.cyrene;
        let exploit_active = cyrene
            .and_then(|entity| {
                with_global_ecs(|ecs| {
                    let coreflame = ecs.coreflames.get(entity)?;
                    let memory = ecs.memory_logs.get(entity)?;
                    Some(
                        coreflame.alignment == Path::Remembrance
                            && memory.trauma_index >= 0.85
                            && coreflame.power_level >= 1.0,
                    )
                })
                .flatten()
            })
            .unwrap_or(false);

        self.state.time_concept_active = !exploit_active;
    }

    fn advance_phainon_memory(&mut self) {
        self.persistent_phainon_memory.retained_cycles = self
            .persistent_phainon_memory
            .retained_cycles
            .saturating_add(1);
        self.persistent_phainon_memory.trauma_index = (self.persistent_phainon_memory.trauma_index
            + (self.state.destruction_entropy * 0.02))
            .clamp(0.0, 1.0);

        if let Some(phainon) = self.flame_chase.phainon {
            let memory = self.persistent_phainon_memory;
            let _ = with_global_ecs_mut(|ecs| {
                if let Some(memory_log) = ecs.memory_logs.get_mut(phainon) {
                    *memory_log = memory;
                }
            });
        }
    }

    fn capture_phainon_memory(&mut self) {
        if let Some(phainon) = self.flame_chase.phainon {
            if let Some(memory_log) =
                with_global_ecs(|ecs| ecs.memory_logs.get(phainon).copied()).flatten()
            {
                self.persistent_phainon_memory = memory_log;
            }
        }
    }

    fn reseed_after_black_tide(&mut self) {
        self.flame_chase = FlameChaseHandles::default();
        self.seed_population_groups();
        self.seed_flame_chase_variables();
        self.apply_cyrene_time_exploit();
    }

    pub fn tick(&mut self) -> SimulationResult {
        self.apply_cyrene_time_exploit();
        let time_bypassed = !self.state.time_concept_active;

        let nodes = self.build_destruction_nodes();
        self.state.destruction_entropy = evaluate_destruction_ast(&nodes);

        self.advance_phainon_memory();
        self.apply_golden_blood_corruption();

        if self.state.destruction_entropy >= 1.0 {
            self.capture_phainon_memory();
            self.snapshot_to_eternal_page("amphoreus_autosave.page");
            self.arena.trigger_black_tide();
            let _ = with_global_ecs_mut(|ecs| ecs.clear_for_black_tide());
            self.state.cycle_count = self.state.cycle_count.saturating_add(1);
            self.reseed_after_black_tide();
            return SimulationResult::BlackTideTriggered;
        }

        if time_bypassed {
            return SimulationResult::TimeBypassed;
        }

        self.state.cycle_count = self.state.cycle_count.saturating_add(1);
        SimulationResult::TickAdvanced
    }

    /// Serializes the used byte-state of the arena to a `.page` file.
    pub fn snapshot_to_eternal_page(&self, file_path: &str) {
        let snapshot = ArenaSnapshot {
            offset: self.arena.offset,
            memory: self.arena.used_bytes(),
        };

        match encode_to_vec(&snapshot, standard()) {
            Ok(bytes) => {
                if let Err(err) = fs::write(file_path, bytes) {
                    eprintln!("failed to write eternal page `{file_path}`: {err}");
                }
            }
            Err(err) => {
                eprintln!("failed to serialize eternal page `{file_path}`: {err}");
            }
        }
    }

    fn build_destruction_nodes(&self) -> Vec<DestructionNode> {
        let entity_count = with_global_ecs(|ecs| ecs.entity_count() as u32).unwrap_or(0);
        let average_corruption = with_global_ecs(|ecs| ecs.average_corruption()).unwrap_or(0.0);
        let memory_multiplier = 1.0 + self.persistent_phainon_memory.trauma_index * 0.25;

        vec![
            DestructionNode::EntityCount(entity_count),
            DestructionNode::ConflictEvent(average_corruption),
            DestructionNode::EntropyMultiplier((1.0 + average_corruption * 0.35) * memory_multiplier),
        ]
    }

    pub fn apply_golden_blood_corruption(&mut self) {
        let local_entropy = self.state.destruction_entropy;

        let _ = with_global_ecs_mut(|ecs| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let entity_span = ecs.entity_span();
                let mut corruption_lookup = vec![0.0_f64; entity_span];

                let (golden_entities, golden_data) = ecs.golden_blood.dense_pairs_mut();
                let updates: Vec<(Entity, f64)> = golden_entities
                    .par_iter()
                    .copied()
                    .zip(golden_data.par_iter_mut())
                    .filter_map(|(entity, blood)| {
                        if blood.corruption_level < 0.6 {
                            return None;
                        }

                        blood.corruption_level =
                            (blood.corruption_level + (local_entropy * 0.05)).clamp(0.0, 1.0);
                        Some((entity, blood.corruption_level))
                    })
                    .collect();

                for (entity, corruption_level) in updates {
                    let index = entity as usize;
                    if index < corruption_lookup.len() {
                        corruption_lookup[index] = corruption_level;
                    }
                }

                let (coreflame_entities, coreflame_data) = ecs.coreflames.dense_pairs_mut();
                coreflame_entities
                    .par_iter()
                    .copied()
                    .zip(coreflame_data.par_iter_mut())
                    .for_each(|(entity, coreflame)| {
                        let corruption_level =
                            corruption_lookup.get(entity as usize).copied().unwrap_or(0.0);
                        if corruption_level <= 0.0 {
                            return;
                        }

                        coreflame.power_level =
                            (coreflame.power_level * (1.0 - corruption_level * 0.03)).max(0.0);
                        coreflame.alignment = Path::Destruction;
                    });
            }

            #[cfg(target_arch = "wasm32")]
            {
                let (coreflames, golden_blood) = (&mut ecs.coreflames, &mut ecs.golden_blood);
                for (entity, blood) in golden_blood.iter_mut() {
                    if blood.corruption_level < 0.6 {
                        continue;
                    }

                    blood.corruption_level =
                        (blood.corruption_level + (local_entropy * 0.05)).clamp(0.0, 1.0);

                    if let Some(coreflame) = coreflames.get_mut(entity) {
                        coreflame.power_level =
                            (coreflame.power_level * (1.0 - blood.corruption_level * 0.03))
                                .max(0.0);
                        coreflame.alignment = Path::Destruction;
                    }
                }
            }
        });
    }
}
