// SPDX-License-Identifier: MIT
//! FreePlots interface: classic statistical-desktop worksheet, Session and plot widgets.
//!
//! This crate is deliberately **pure egui**: it never opens a file, reads the
//! environment or talks to a window system. The desktop runner in `fp-app`
//! performs I/O and passes an in-memory [`fp_core::data::DataFrame`] in.
//!
//! The layout mirrors the product's mental model: classic menus, a worksheet
//! with `C1..Cn`, a Session window on the right, and graph windows opened by an
//! analysis. All numbers come from `fp-core`; no statistic is computed here.

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod app;
pub mod catalog;
pub mod plot_view;

pub use app::{FileKind, FreePlotsApp, OpenRequest, SaveRequest};
