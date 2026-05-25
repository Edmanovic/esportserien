#![allow(unsafe_code)]

use crate::CryptoError;

/// Guard representing a best-effort operating system memory lock.
///
/// The lock is released on drop. Locking may fail on platforms with low
/// process limits, hardened sandboxing, or unsupported allocators.
pub struct MemoryLockGuard {
    ptr: *mut u8,
    len: usize,
}

impl MemoryLockGuard {
    pub(crate) fn new(ptr: *mut u8, len: usize) -> Result<Self, CryptoError> {
        if len == 0 {
            return Ok(Self { ptr, len });
        }

        lock_memory(ptr, len)?;
        Ok(Self { ptr, len })
    }
}

impl Drop for MemoryLockGuard {
    fn drop(&mut self) {
        if self.len != 0 {
            unlock_memory(self.ptr, self.len);
        }
    }
}

// SAFETY: MemoryLockGuard only calls mlock/VirtualLock/munlock/VirtualUnlock,
// which are thread-safe OS APIs. The pointer is to heap memory owned by the
// enclosing key type and is never dereferenced by the guard — it is only passed
// to the OS. Callers must protect concurrent access with a Mutex.
unsafe impl Send for MemoryLockGuard {}

/// Memory that can request an operating system lock.
pub trait MemoryLock {
    /// Attempts to lock the memory region until the returned guard is dropped.
    fn lock_memory(&mut self) -> Result<MemoryLockGuard, CryptoError>;
}

#[cfg(unix)]
fn lock_memory(ptr: *mut u8, len: usize) -> Result<(), CryptoError> {
    // SAFETY: `ptr` and `len` come from live Rust-owned buffers. The OS call
    // does not outlive the buffer and only marks pages resident.
    let result = unsafe { libc::mlock(ptr.cast(), len) };
    if result == 0 {
        Ok(())
    } else {
        Err(CryptoError::MemoryLockFailed)
    }
}

#[cfg(unix)]
fn unlock_memory(ptr: *mut u8, len: usize) {
    // SAFETY: The same live region locked by `mlock` is being unlocked during
    // guard drop. Failure during cleanup is intentionally ignored.
    unsafe {
        libc::munlock(ptr.cast(), len);
    }
}

#[cfg(windows)]
fn lock_memory(ptr: *mut u8, len: usize) -> Result<(), CryptoError> {
    // SAFETY: `ptr` and `len` come from live Rust-owned buffers. Windows pins
    // the corresponding pages for the current process.
    let ok = unsafe { windows_sys::Win32::System::Memory::VirtualLock(ptr.cast(), len) };
    if ok != 0 {
        Ok(())
    } else {
        Err(CryptoError::MemoryLockFailed)
    }
}

#[cfg(windows)]
fn unlock_memory(ptr: *mut u8, len: usize) {
    // SAFETY: The same live region locked by `VirtualLock` is being unlocked
    // during guard drop. Failure during cleanup is intentionally ignored.
    unsafe {
        windows_sys::Win32::System::Memory::VirtualUnlock(ptr.cast(), len);
    }
}

#[cfg(not(any(unix, windows)))]
fn lock_memory(_ptr: *mut u8, _len: usize) -> Result<(), CryptoError> {
    Err(CryptoError::MemoryLockFailed)
}

#[cfg(not(any(unix, windows)))]
fn unlock_memory(_ptr: *mut u8, _len: usize) {}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time check: MemoryLockGuard must be Send so it can live
    // inside Mutex<> across threads in Tauri state.
    #[test]
    fn memory_lock_guard_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<MemoryLockGuard>();
    }
}
