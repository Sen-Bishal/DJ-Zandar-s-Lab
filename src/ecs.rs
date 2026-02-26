use std::sync::OnceLock;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

pub type Entity = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Path {
    Erudition,
    Destruction,
    Remembrance,
    #[default]
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coreflame {
    pub power_level: f64,
    pub alignment: Path,
}

impl Default for Coreflame {
    fn default() -> Self {
        Self {
            power_level: 0.0,
            alignment: Path::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MemoryLog {
    pub retained_cycles: u64,
    pub trauma_index: f64,
}

impl Default for MemoryLog {
    fn default() -> Self {
        Self {
            retained_cycles: 0,
            trauma_index: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GoldenBlood {
    pub corruption_level: f64,
}

impl Default for GoldenBlood {
    fn default() -> Self {
        Self {
            corruption_level: 0.0,
        }
    }
}

/// Dense/sparse component storage for cache-friendly iteration and O(1) access.
#[derive(Debug, Default)]
pub struct ComponentStore<T> {
    dense_entities: Vec<Entity>,
    dense_data: Vec<T>,
    sparse: Vec<u32>,
}

impl<T> ComponentStore<T> {
    pub fn with_capacity(entity_capacity: usize, component_capacity: usize) -> Self {
        Self {
            dense_entities: Vec::with_capacity(component_capacity),
            dense_data: Vec::with_capacity(component_capacity),
            sparse: vec![0; entity_capacity],
        }
    }

    fn ensure_sparse_capacity(&mut self, entity: Entity) {
        let index = entity as usize;
        if index >= self.sparse.len() {
            self.sparse.resize(index + 1, 0);
        }
    }

    pub fn insert(&mut self, entity: Entity, value: T) {
        self.ensure_sparse_capacity(entity);
        let sparse_index = entity as usize;
        let slot = self.sparse[sparse_index];

        if slot == 0 {
            let dense_index = self.dense_data.len();
            self.dense_entities.push(entity);
            self.dense_data.push(value);
            self.sparse[sparse_index] = (dense_index as u32) + 1;
            return;
        }

        let dense_index = (slot - 1) as usize;
        self.dense_data[dense_index] = value;
    }

    pub fn get(&self, entity: Entity) -> Option<&T> {
        let sparse_index = entity as usize;
        let slot = *self.sparse.get(sparse_index)?;
        if slot == 0 {
            return None;
        }

        self.dense_data.get((slot - 1) as usize)
    }

    pub fn get_mut(&mut self, entity: Entity) -> Option<&mut T> {
        let sparse_index = entity as usize;
        let slot = *self.sparse.get(sparse_index)?;
        if slot == 0 {
            return None;
        }

        self.dense_data.get_mut((slot - 1) as usize)
    }

    pub fn remove(&mut self, entity: Entity) -> Option<T> {
        let sparse_index = entity as usize;
        let slot = *self.sparse.get(sparse_index)?;
        if slot == 0 {
            return None;
        }

        let dense_index = (slot - 1) as usize;
        let last_index = self.dense_data.len().saturating_sub(1);
        let removed_entity = self.dense_entities[dense_index];
        let removed = self.dense_data.swap_remove(dense_index);
        self.dense_entities.swap_remove(dense_index);

        if dense_index != last_index {
            let moved_entity = self.dense_entities[dense_index];
            self.sparse[moved_entity as usize] = (dense_index as u32) + 1;
        }

        self.sparse[removed_entity as usize] = 0;
        Some(removed)
    }

    pub fn clear(&mut self) {
        self.dense_entities.clear();
        self.dense_data.clear();
        self.sparse.fill(0);
    }

    pub fn len(&self) -> usize {
        self.dense_data.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Entity, &T)> {
        self.dense_entities
            .iter()
            .copied()
            .zip(self.dense_data.iter())
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Entity, &mut T)> {
        self.dense_entities
            .iter()
            .copied()
            .zip(self.dense_data.iter_mut())
    }

    pub fn dense_entities(&self) -> &[Entity] {
        &self.dense_entities
    }

    pub fn dense_data(&self) -> &[T] {
        &self.dense_data
    }

    pub fn dense_data_mut(&mut self) -> &mut [T] {
        &mut self.dense_data
    }

    pub fn dense_pairs_mut(&mut self) -> (&[Entity], &mut [T]) {
        (&self.dense_entities, &mut self.dense_data)
    }
}

/// Core world storage using dense per-component arrays.
#[derive(Debug)]
pub struct SoaEcs {
    next_entity: Entity,
    alive_count: usize,
    alive: Vec<bool>,
    pub coreflames: ComponentStore<Coreflame>,
    pub memory_logs: ComponentStore<MemoryLog>,
    pub golden_blood: ComponentStore<GoldenBlood>,
}

impl SoaEcs {
    pub fn with_capacity(entity_capacity: usize) -> Self {
        Self {
            next_entity: 0,
            alive_count: 0,
            alive: vec![false; entity_capacity],
            coreflames: ComponentStore::with_capacity(entity_capacity, entity_capacity / 4),
            memory_logs: ComponentStore::with_capacity(entity_capacity, entity_capacity / 8),
            golden_blood: ComponentStore::with_capacity(entity_capacity, entity_capacity / 4),
        }
    }

    pub fn spawn(&mut self) -> Entity {
        let entity = self.next_entity;
        self.next_entity = self
            .next_entity
            .checked_add(1)
            .expect("entity id overflowed u32");

        let index = entity as usize;
        if index >= self.alive.len() {
            self.alive.resize(index + 1, false);
        }

        self.alive[index] = true;
        self.alive_count += 1;
        entity
    }

    pub fn despawn(&mut self, entity: Entity) -> bool {
        if !self.is_alive(entity) {
            return false;
        }

        self.alive[entity as usize] = false;
        self.alive_count = self.alive_count.saturating_sub(1);
        self.coreflames.remove(entity);
        self.memory_logs.remove(entity);
        self.golden_blood.remove(entity);
        true
    }

    pub fn is_alive(&self, entity: Entity) -> bool {
        self.alive.get(entity as usize).copied().unwrap_or(false)
    }

    pub fn entity_count(&self) -> usize {
        self.alive_count
    }

    pub fn entity_span(&self) -> usize {
        self.alive.len()
    }

    pub fn average_corruption(&self) -> f64 {
        let count = self.golden_blood.len();
        if count == 0 {
            return 0.0;
        }

        let total: f64 = self
            .golden_blood
            .iter()
            .map(|(_, blood)| blood.corruption_level)
            .sum();
        total / count as f64
    }

    pub fn clear_for_black_tide(&mut self) {
        self.next_entity = 0;
        self.alive_count = 0;
        self.alive.fill(false);
        self.coreflames.clear();
        self.memory_logs.clear();
        self.golden_blood.clear();
    }
}

static GLOBAL_ECS: OnceLock<RwLock<SoaEcs>> = OnceLock::new();

pub fn init_global_ecs(entity_capacity: usize) {
    let _ = GLOBAL_ECS.set(RwLock::new(SoaEcs::with_capacity(entity_capacity)));
}

pub fn with_global_ecs<R>(f: impl FnOnce(&SoaEcs) -> R) -> Option<R> {
    let lock = GLOBAL_ECS.get()?;
    let guard = lock.read();
    Some(f(&guard))
}

pub fn with_global_ecs_mut<R>(f: impl FnOnce(&mut SoaEcs) -> R) -> Option<R> {
    let lock = GLOBAL_ECS.get()?;
    let mut guard = lock.write();
    Some(f(&mut guard))
}
