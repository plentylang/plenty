//! Checked formatting sink. Formatting is completed before exposing any bytes.
use crate::allocator::{Allocator, bytes::SelectedBytes};
use crate::memory::{self, AllocError};

enum Bytes {
    Ordinary(Vec<u8>),
    Selected(SelectedBytes),
}
impl Default for Bytes {
    fn default() -> Self {
        Self::Ordinary(Vec::new())
    }
}

#[derive(Default)]
pub(crate) struct Buffer {
    bytes: Bytes,
    error: Option<AllocError>,
    /// Where a failed `display` or `debug` method's `Result` goes. Only
    /// hooked field, `print` and `str.repr` operations pass one, so `main`'s
    /// raw error report never runs user methods.
    hooks: Option<*mut u128>,
    hook_failed: bool,
    pub(crate) field_allocator: Option<*const u128>,
}
impl Buffer {
    /// A buffer whose rendering calls the types' formatting adapters.
    pub fn hooked(cell: *mut u128) -> Self {
        Self {
            hooks: (!cell.is_null()).then_some(cell),
            ..Self::default()
        }
    }
    /// The allocator reference remains borrowed through all selected callbacks.
    pub fn selected(allocator: &Allocator, reference: *const u128, cell: *mut u128) -> Self {
        Self {
            bytes: Bytes::Selected(SelectedBytes::new(allocator)),
            field_allocator: Some(reference),
            ..Self::hooked(cell)
        }
    }
    /// Large borrowed inline hook receivers need an aligned temporary slot.
    pub fn allocator(&self) -> Option<&Allocator> {
        match &self.bytes {
            Bytes::Selected(bytes) => Some(&bytes.allocator),
            Bytes::Ordinary(_) => None,
        }
    }
    pub fn hooks(&self) -> bool {
        self.hooks.is_some()
    }
    pub fn failed(&self) -> bool {
        self.error.is_some() || self.hook_failed
    }
    /// Whether a formatting method failed; its error is in the cell.
    pub fn hook_failed(&self) -> bool {
        self.hook_failed
    }
    /// Stop formatting after a method returned `result`, an `Err`.
    pub fn fail_hook(&mut self, result: u128) {
        if let Some(cell) = self.hooks {
            // SAFETY: the caller's cell outlives the rendering.
            unsafe { cell.write(result) };
        }
        self.hook_failed = true;
    }
    pub fn fail_alloc(&mut self, error: AllocError) {
        self.error.get_or_insert(error);
    }
    pub fn push(&mut self, byte: u8) {
        self.extend_from_slice(&[byte]);
    }
    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        if self.failed() {
            return;
        }
        let result = match &mut self.bytes {
            Bytes::Ordinary(out) => memory::try_reserve_amortized(out, bytes.len())
                .map(|()| out.extend_from_slice(bytes)),
            Bytes::Selected(out) => out
                .reserve(bytes.len(), true)
                .map(|()| out.extend_reserved(bytes)),
        };
        self.error = result.err();
    }
    pub fn finish(&self) -> Result<&[u8], AllocError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(match &self.bytes {
            Bytes::Ordinary(out) => out,
            Bytes::Selected(out) => out.as_slice(),
        })
    }
}
impl std::io::Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        // Keep the original allocation error rather than allocating an I/O
        // diagnostic. Callers must finish() before using the buffered output.
        self.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
