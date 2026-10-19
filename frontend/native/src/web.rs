//! Host memory exports for the browser build (issue 0088).
//!
//! A WebAssembly host can read and write the module's linear memory but
//! cannot allocate in it, so it borrows request buffers and out-parameters
//! through these two calls. They exist only on `wasm32-wasip1`; native hosts
//! use their own allocator, as the C ABI already assumes.
//!
//! Ownership: a block from [`domowik_web_alloc`] belongs to the host until it
//! passes the same pointer and length to [`domowik_web_free`]. It is never a
//! `domowik_buf` and must not be released with `domowik_buffer_free`; bridge
//! buffers are released by token as everywhere else. Blocks are 8-byte
//! aligned so a `uint64_t` out-parameter can live in one.

use std::alloc::Layout;

const ALIGN: usize = 8;

fn layout(len: u32) -> Option<Layout> {
    Layout::from_size_align(len.max(1) as usize, ALIGN).ok()
}

/// Allocates `len` bytes (at least one) for the host, or returns null.
#[no_mangle]
pub extern "C" fn domowik_web_alloc(len: u32) -> *mut u8 {
    match layout(len) {
        // SAFETY: the layout has a non-zero size.
        Some(layout) => unsafe { std::alloc::alloc(layout) },
        None => std::ptr::null_mut(),
    }
}

/// Releases a block from [`domowik_web_alloc`]. Null is ignored.
///
/// # Safety
///
/// `ptr` must come from [`domowik_web_alloc`] with this `len` and not have
/// been freed.
#[no_mangle]
pub unsafe extern "C" fn domowik_web_free(ptr: *mut u8, len: u32) {
    if let (false, Some(layout)) = (ptr.is_null(), layout(len)) {
        // SAFETY: the caller guarantees `ptr` was allocated with this layout.
        unsafe { std::alloc::dealloc(ptr, layout) };
    }
}
