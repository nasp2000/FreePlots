// SPDX-License-Identifier: MIT
//! Heap budget for a million-cell worksheet.
//!
//! A dedicated test binary measures its own allocations with a counting
//! global allocator, so the number is portable and does not depend on reading
//! `/proc`. The plan's budget is 100 MB of resident memory; the heap peak is a
//! stricter proxy for the worksheet and the analysis buffers.

#![cfg(not(debug_assertions))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

struct Tracking;

static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Tracking {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live =
                LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed) + layout.size() as isize;
            if live > 0 {
                PEAK.fetch_max(live as usize, Ordering::Relaxed);
            }
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            let delta = new_size as isize - layout.size() as isize;
            let live = LIVE.fetch_add(delta, Ordering::Relaxed) + delta;
            if live > 0 {
                PEAK.fetch_max(live as usize, Ordering::Relaxed);
            }
        }
        new_pointer
    }
}

#[global_allocator]
static ALLOCATOR: Tracking = Tracking;

#[test]
fn one_million_cells_stay_inside_the_memory_budget() {
    use fp_core::data::{Column, DataFrame};

    let rows = 50_000usize;
    let columns = 20usize;
    let baseline = LIVE.load(Ordering::Relaxed).max(0) as usize;
    PEAK.store(baseline, Ordering::Relaxed);

    let values: Vec<Vec<f64>> = (0..columns)
        .map(|column| {
            (0..rows)
                .map(|row| row as f64 * 0.001 + column as f64)
                .collect()
        })
        .collect();
    let frame = DataFrame::new(
        values
            .into_iter()
            .enumerate()
            .map(|(index, values)| Column::new_f64(format!("C{}", index + 1), values))
            .collect(),
    )
    .expect("frame");
    assert_eq!(frame.n_rows() * frame.n_columns(), 1_000_000);

    let result = fp_core::workflow::descriptive_statistics(&frame, "C1").expect("descriptive");
    assert!(!result.title.is_empty());

    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    let budget = 100 * 1024 * 1024;
    println!("1M cells heap peak: {peak} bytes (budget {budget})");
    assert!(peak <= budget, "heap peak {peak} bytes exceeds {budget}");
}
