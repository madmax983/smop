//! `hash::sha256_file` reads a big file in constant memory.
//!
//! A counting allocator records the peak heap use. The test runs without
//! the test harness (`harness = false`), so no other thread allocates at the
//! same time.
#![allow(unsafe_code, clippy::unwrap_used)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;

static CURRENT: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: each call goes to `System` with the same arguments. The counters
// do not change what is allocated.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract for `alloc` is passed on.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let now = CURRENT.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(now, Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract for `dealloc` is passed on.
        unsafe { System.dealloc(ptr, layout) };
        CURRENT.fetch_sub(layout.size(), Ordering::SeqCst);
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

const MIB: usize = 1024 * 1024;

/// SHA-256 of `i % 251` for `i` in `0..10 MiB` (from Python's `hashlib`).
const DIGEST_10_MIB: &str = "44f9296993796e201208c6c245b9515d36b62c87d0be4459ff347bfa054cd527";

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.bin");
    let mut file = std::fs::File::create(&path).unwrap();
    let mut chunk = vec![0u8; MIB];
    for n in 0..10 {
        for (i, b) in chunk.iter_mut().enumerate() {
            *b = u8::try_from((n * MIB + i) % 251).unwrap();
        }
        file.write_all(&chunk).unwrap();
    }
    drop((file, chunk));
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 10 * MIB as u64);

    let base = CURRENT.load(Ordering::SeqCst);
    PEAK.store(base, Ordering::SeqCst);
    let digest = smop::hash::sha256_file(&path).unwrap();
    let extra = PEAK.load(Ordering::SeqCst) - base;

    assert_eq!(digest, DIGEST_10_MIB);
    assert!(
        extra < 256 * 1024,
        "peak heap use {extra} bytes for a 10 MiB file"
    );
    println!("hash_memory: ok, peak {extra} bytes for a 10 MiB file");
}
