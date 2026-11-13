// SPDX-License-Identifier: MIT
//! FreePlots — statistical engine.
//!
//! This crate knows nothing about the GUI, the operating system, or the network.
//! That neutrality is a rule checked in CI (see `PLAN.md`, section 5.1):
//! porting to Linux/macOS/arm64 should be a recompile, not a rewrite.
//!
//! Permanent rules:
//! - `#![forbid(unsafe_code)]`.
//! - No function returns `NaN`/`inf` without signalling it in the return type.
//! - Errors with context, never `unwrap()` on user data.

#![forbid(unsafe_code)]
#![deny(missing_debug_implementations)]

pub mod agreement;
pub mod alt;
pub mod anova;
pub mod attrcap;
pub mod attribute;
pub mod batch;
pub mod boxcox;
pub mod boxplot;
pub mod calc;
pub mod cluster;
pub mod contingency;
pub mod correlation;
pub mod curvefit;
pub mod data;
pub mod dates;
pub mod desirability;
pub mod dists;
pub mod doe;
pub mod dotplot;
pub mod ecdf;
pub mod equivalence;
pub mod error;
pub mod estimate;
pub mod formula;
pub mod gage;
pub mod gage_type1;
pub mod gof;
pub mod histogram;
pub mod hypothesis;
pub mod i18n;
pub mod inflate;
pub mod intervalplot;
pub mod io;
pub mod johnson;
pub mod linearity;
pub mod logistic;
pub mod matrixplot;
pub mod mvchart;
pub mod nonparametric;
pub mod optimize;
pub mod ordinal;
pub mod pareto;
pub mod pca;
pub mod pdf;
pub mod plot;
pub mod plugin;
pub mod poisson;
pub mod power;
pub mod proportion;
pub mod rare;
pub mod regression;
pub mod reliability;
pub mod report;
pub mod reshape;
pub mod rsm;
pub mod runchart;
pub mod sampling;
pub mod scatter;
pub mod seriesplot;
pub mod sixpack;
pub mod spc;
pub mod special;
pub mod stemleaf;
pub mod stepwise;
pub mod taguchi;
pub mod timeseries;
pub mod tolerance;
pub mod transform;
pub mod twoway;
pub mod variance;
pub mod workflow;
pub mod xlsx;
pub mod xlsx_read;
pub mod zip;

pub use data::{Column, ColumnSummary, DataFrame, Descriptives, MissingPolicy, Sample};
pub use dists::{Distribution, Normal};
pub use error::{Error, Result};

/// Engine version, used in reproducible reports.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Schema version of the `.fpp` project file.
pub const PROJECT_SCHEMA_VERSION: u32 = 2;
