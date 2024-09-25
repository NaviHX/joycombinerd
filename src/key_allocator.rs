use anyhow::Result as Anyhow;
use bit_set::BitSet;

pub struct KeyAllocator {
    bitmap: BitSet,
}

impl KeyAllocator {
    pub fn new(capacity: usize) -> Self {
        Self {
            bitmap: BitSet::with_capacity(capacity),
        }
    }

    pub fn allocate(&mut self) -> Anyhow<usize> {
        for i in 0..self.bitmap.len() {
            if !self.bitmap.contains(i) {
                self.bitmap.insert(i);
                return Ok(i);
            }
        }

        Err(anyhow::anyhow!("Failed to allocate a new key"))
    }

    pub fn release(&mut self, key: usize) {
        self.bitmap.remove(key);
    }
}
