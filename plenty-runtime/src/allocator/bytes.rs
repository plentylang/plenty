//! Provider-backed growable bytes shared by packed storage and structural rendering.
use super::{Allocator, Block};
use crate::memory::{self, AllocError};
use std::alloc::Layout;

pub(crate) struct SelectedBytes {
    allocator: Allocator,
    pub(crate) backing: Option<Block>,
    pub(crate) len: usize,
}

impl SelectedBytes {
    pub(crate) fn new(allocator: &Allocator) -> Self {
        Self {
            allocator: allocator.clone(),
            backing: None,
            len: 0,
        }
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        self.backing
            .as_ref()
            .map_or(&[], |b| &b.bytes()[..self.len])
    }

    pub(crate) fn extend_reserved(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            let end = self.len + bytes.len();
            self.backing.as_mut().unwrap().bytes_mut()[self.len..end].copy_from_slice(bytes);
            self.len = end;
        }
    }

    pub(crate) fn capacity(&self) -> usize {
        self.backing.as_ref().map_or(0, |block| block.bytes().len())
    }

    pub(crate) fn reserve(&mut self, additional: usize, amortized: bool) -> Result<(), AllocError> {
        let needed = self
            .len
            .checked_add(additional)
            .ok_or(AllocError::CapacityOverflow)?;
        memory::checked_capacity::<u8>(needed)?;
        let capacity = self.capacity();
        if needed <= capacity {
            return Ok(());
        }
        let target = if amortized {
            needed.max(capacity.saturating_mul(2)).max(8)
        } else {
            needed
        };
        let layout = Layout::array::<u8>(target).map_err(|_| AllocError::CapacityOverflow)?;
        let mut replacement = self.allocator.allocate(layout)?;
        if let Some(old) = &self.backing {
            replacement.bytes_mut()[..self.len].copy_from_slice(&old.bytes()[..self.len]);
        }
        self.backing = Some(replacement);
        Ok(())
    }
}
