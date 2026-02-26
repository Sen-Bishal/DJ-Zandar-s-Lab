/// `AmphoreusArena` is a deterministic bump allocator for simulation-frame data.
///
/// The arena is reset in O(1) by moving `offset` back to zero.
#[derive(Debug, Clone)]
pub struct AmphoreusArena {
    pub memory: Vec<u8>,
    pub offset: usize,
}

impl AmphoreusArena {
    /// Creates a new arena with a fixed contiguous capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            memory: vec![0_u8; capacity],
            offset: 0,
        }
    }

    /// O(1) world wipe: reset the allocation pointer.
    pub fn trigger_black_tide(&mut self) {
        self.offset = 0;
    }

    /// Returns the currently used byte region.
    pub fn used_bytes(&self) -> &[u8] {
        let used = self.offset.min(self.memory.len());
        &self.memory[..used]
    }

    /// Deterministic aligned byte allocation from the bump arena.
    ///
    /// Returns `None` if there is not enough capacity or alignment is invalid.
    pub fn alloc_bytes(&mut self, len: usize, align: usize) -> Option<&mut [u8]> {
        let align = align.max(1);
        if !align.is_power_of_two() {
            return None;
        }

        let aligned_offset = (self.offset + (align - 1)) & !(align - 1);
        let end = aligned_offset.checked_add(len)?;
        if end > self.memory.len() {
            return None;
        }

        self.offset = end;
        self.memory.get_mut(aligned_offset..end)
    }
}
