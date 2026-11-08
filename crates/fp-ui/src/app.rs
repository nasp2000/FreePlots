// SPDX-License-Identifier: MIT
//! The FreePlots workspace: classic menus, worksheet, Session and graph window.
//!
//! It is an egui application without a native window. `fp-app` wraps it in
//! `eframe` and performs file I/O; tests drive it with a plain `egui::Context`.

use crate::catalog::UiCatalog;
use egui::{Align, Color32, RichText};
use egui_extras::{Column as TableColumn, TableBuilder};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use fp_core::attribute::AttributeChartKind;
use fp_core::curvefit::CurveModel;
use fp_core::data::{Column, DType, DataFrame, DateFormat};
use fp_core::desirability::Goal;
use fp_core::gage::GageDesign;
use fp_core::hypothesis::Alternative;
use fp_core::io::{CsvOptions, CsvPreview};
use fp_core::plot::PlotSpec;
use fp_core::power::PowerProcedure;
use fp_core::proportion::ProportionMethod;
use fp_core::spc::{ControlChartKind, TestSet};
use fp_core::stepwise::{StepCriterion, StepMethod};
use fp_core::variance::{VarianceCenter, VarianceMethod};
use fp_core::workflow::{
    AnalysisOutput, AnalysisSpec, DistributionKind, PlotDisplay, PlotLayout, first_numeric_column,
    run_analysis,
};

mod chrome;
mod dialogs_data;
mod dialogs_doe;
mod dialogs_files;
mod dialogs_msa;
mod dialogs_quality;
mod dialogs_reliability;
mod dialogs_stat;
mod dialogs_tests;
mod editing;
mod graph_edit;
mod grid;
mod licences;
mod menu_bar;
mod session;
mod state;
mod tour;

/// File format of a worksheet request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// Delimited text worksheet.
    Csv,
    /// FreePlots JSON project (`.fpp`).
    Project,
    /// Standalone HTML export of the Session.
    Html,
    /// PDF export of the Session report.
    Pdf,
    /// Excel workbook (`.xlsx`) to import.
    Xlsx,
    /// SVG export of the current graph.
    Svg,
    /// PDF page with the current graph, for printing.
    GraphPdf,
    /// PNG raster of the current graph.
    Png,
    /// JPEG raster of the current graph.
    Jpeg,
}

/// A request to read a worksheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRequest {
    /// Source path.
    pub path: String,
    /// File format expected at the path.
    pub kind: FileKind,
    /// Worksheet of an Excel workbook (`None` = the first one).
    pub sheet: Option<String>,
    /// Explicit CSV options from the preview (`None` = auto-detect).
    pub csv_options: Option<CsvOptions>,
}

/// A request to show a native file browser.
///
/// The UI never touches the file system; the desktop runner opens the native
/// dialog and hands the chosen path back with
/// [`FreePlotsApp::set_browsed_path`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseRequest {
    /// Title of the native dialog.
    pub title: String,
    /// Filters, each written as `Name | *.extension`.
    pub filters: Vec<String>,
    /// `true` for a save dialog, `false` for an open dialog.
    pub save: bool,
}

/// A request to show a native folder browser.
///
/// The desktop runner opens the dialog and hands the chosen folder back with
/// [`FreePlotsApp::set_browsed_folder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRequest {
    /// Title of the native dialog.
    pub title: String,
}

/// A request to write the worksheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    /// Destination path.
    pub path: String,
    /// File format to write.
    pub kind: FileKind,
    /// Write `;` and decimal comma instead of `,` and decimal point (CSV only).
    pub decimal_comma: bool,
}

/// A request to summarise every CSV in a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRequest {
    /// Folder to scan.
    pub folder: String,
    /// Numeric column to summarise in each file.
    pub column: String,
}

const UNDO_LIMIT: usize = 100;
/// Largest Session text kept in memory (bytes). Well above any realistic
/// session, so the cap never removes text a user would miss.
const SESSION_LIMIT: usize = 16 << 20;

#[derive(Debug, Clone)]
struct CellEdit {
    column: usize,
    row: usize,
    before: Option<String>,
    after: Option<String>,
    before_rows: usize,
}

#[derive(Debug, Clone)]
enum EditAction {
    Cells(Vec<CellEdit>),
    InsertRow {
        row: usize,
    },
    RemoveRow {
        row: usize,
        cells: Vec<Option<String>>,
    },
    AddColumn {
        index: usize,
        name: String,
        dtype: DType,
    },
    RemoveColumn {
        index: usize,
        column: Column,
    },
    RenameColumn {
        index: usize,
        before: String,
        after: String,
    },
}

#[derive(Debug, Clone, Default)]
struct EditGroup {
    actions: Vec<EditAction>,
}

/// Horizontal alignment of a formatted column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellAlign {
    /// Left edge.
    Left,
    /// Centred.
    Center,
    /// Right edge.
    Right,
}

impl CellAlign {
    /// Name shown in the dialog.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
        }
    }
}

/// Display format of one column, applied by the worksheet only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnFormat {
    /// Decimal places of numeric cells.
    pub decimals: usize,
    /// Groups the integer part in threes.
    pub thousands: bool,
    /// Draws the text bolder.
    pub bold: bool,
    /// Horizontal alignment.
    pub align: CellAlign,
}

impl Default for ColumnFormat {
    fn default() -> Self {
        Self {
            decimals: 6,
            thousands: false,
            bold: false,
            align: CellAlign::Left,
        }
    }
}

/// Condition of a conditional-formatting rule.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ConditionalTest {
    /// Numeric value strictly above the limit.
    Greater(f64),
    /// Numeric value strictly below the limit.
    Less(f64),
    /// Numeric value between the two limits, both included.
    Between(f64, f64),
    /// Same number (when both parse) or same text, ignoring case.
    EqualTo(String),
    /// Text contains the fragment, ignoring case.
    Contains(String),
}

/// What a matching cell looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ConditionalStyle {
    /// Light red background.
    RedFill,
    /// Light yellow background.
    YellowFill,
    /// Light green background.
    GreenFill,
    /// Bold dark-red text.
    RedText,
    /// Bold text in the normal colour.
    Bold,
}

/// One conditional-formatting rule, applied to a named column.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ConditionalRule {
    /// Column name the rule applies to.
    pub column: String,
    /// What the cell must satisfy.
    pub test: ConditionalTest,
    /// How a matching cell is drawn.
    pub style: ConditionalStyle,
}

/// Condition chosen in the conditional-formatting dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalKind {
    /// Greater than.
    Greater,
    /// Less than.
    Less,
    /// Between two limits.
    Between,
    /// Equal to.
    EqualTo,
    /// Contains text.
    Contains,
}

impl ConditionalRule {
    /// `true` when the cell satisfies the rule's condition.
    #[must_use]
    pub fn matches(&self, value: Option<f64>, text: &str) -> bool {
        match &self.test {
            ConditionalTest::Greater(limit) => value.is_some_and(|value| value > *limit),
            ConditionalTest::Less(limit) => value.is_some_and(|value| value < *limit),
            ConditionalTest::Between(low, high) => {
                value.is_some_and(|value| value >= *low && value <= *high)
            }
            ConditionalTest::EqualTo(target) => {
                if let (Some(value), Ok(target)) = (value, target.trim().parse::<f64>()) {
                    (value - target).abs() <= 1e-12
                } else {
                    text.trim().eq_ignore_ascii_case(target.trim())
                }
            }
            ConditionalTest::Contains(fragment) => text
                .to_lowercase()
                .contains(&fragment.trim().to_lowercase()),
        }
    }
}

/// Actions of the worksheet context menus.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GridMenu {
    Cut,
    Copy,
    Paste,
    Clear,
    InsertRowAbove,
    InsertRowBelow,
    DeleteRow,
    AddColumn,
    AddTextColumn,
    DeleteColumn,
    RenameColumn,
}

/// One worksheet tab: its data and the editing state that belongs to it.
#[derive(Debug)]
struct SheetState {
    name: String,
    /// The frame of an **inactive** sheet; the active sheet lives in the app
    /// fields so every existing operation keeps working unchanged.
    data: Option<DataFrame>,
    source: String,
    undo: Vec<EditGroup>,
    redo: Vec<EditGroup>,
    selected_cell: Option<(usize, usize)>,
    column_widths: Vec<f32>,
}

/// User options kept between runs, stored as JSON in the data directory.
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Interface language code (`en` or `pt`).
    pub language: String,
    /// Dark theme.
    pub dark_mode: bool,
    /// Interface zoom (1.0 = 100 %).
    pub zoom: f32,
    /// Show the Session panel.
    pub show_session: bool,
    /// Draw the grid lines.
    pub show_grid_lines: bool,
    /// Open plots in their own native window.
    pub native_plot_windows: bool,
    /// `true` once the first-run tour has been shown.
    pub tour_seen: bool,
    /// Graph presentation edits (labels, reference lines, text style), kept
    /// between sessions so the user does not have to set them again.
    #[serde(default)]
    pub graph_overlay: fp_core::plot::GraphOverlay,
    /// Title of the graph the overlay content (annotations, reference lines,
    /// subtitle/footer, axis ranges) belongs to.
    #[serde(default)]
    pub graph_overlay_title: Option<String>,
    /// Conditional-formatting rules of the worksheet, kept between sessions.
    #[serde(default)]
    pub conditional_rules: Vec<ConditionalRule>,
    /// Last probability-plot choices, kept so the next session starts there.
    #[serde(default)]
    pub probability_distribution: Option<String>,
    /// Last probability-plot display choice.
    #[serde(default)]
    pub probability_display: Option<String>,
    /// Last multi-Y layout.
    #[serde(default)]
    pub probability_layout: Option<String>,
    /// Common X scale across probability panels.
    #[serde(default)]
    pub probability_same_x: bool,
    /// Common Y scale across probability panels.
    #[serde(default)]
    pub probability_same_y: bool,
    /// Last capability distribution.
    #[serde(default)]
    pub capability_distribution: Option<String>,
    /// Last capability confidence level.
    #[serde(default)]
    pub capability_confidence: Option<f64>,
    /// Last control-chart family.
    #[serde(default)]
    pub control_chart_kind: Option<String>,
    /// Last subgroup size.
    #[serde(default)]
    pub control_chart_subgroup: Option<usize>,
    /// Enabled special-cause tests (`"1 2 5"`).
    #[serde(default)]
    pub control_chart_tests: Option<String>,
    /// Eight sensitivities, comma-separated.
    #[serde(default)]
    pub control_chart_sensitivity: Option<String>,
    /// Last CUSUM reference and decision interval.
    #[serde(default)]
    pub control_cusum_k: Option<f64>,
    #[serde(default)]
    pub control_cusum_h: Option<f64>,
    /// Last EWMA smoothing and limit width.
    #[serde(default)]
    pub control_ewma_lambda: Option<f64>,
    #[serde(default)]
    pub control_ewma_limit: Option<f64>,
    /// Last regression confidence level (percent).
    #[serde(default)]
    pub regression_confidence: Option<f64>,
    /// Last one-way ANOVA confidence level (percent).
    #[serde(default)]
    pub anova_confidence: Option<f64>,
    /// Confidence levels of the remaining analysis dialogs, by key.
    #[serde(default)]
    pub analysis_confidences: std::collections::BTreeMap<String, f64>,
    /// Probability-plot confidence band switch.
    #[serde(default = "default_true")]
    pub probability_confidence_band: bool,
    /// Gage R&R design (`crossed` or `nested`).
    #[serde(default)]
    pub gage_design: Option<String>,
    /// ALT life and acceleration models, and the use stress.
    #[serde(default)]
    pub alt_life: Option<String>,
    #[serde(default)]
    pub alt_acceleration: Option<String>,
    #[serde(default)]
    pub alt_use_stress: Option<f64>,
    /// Step-stress life and acceleration models, and the use stress.
    #[serde(default)]
    pub step_life: Option<String>,
    #[serde(default)]
    pub step_acceleration: Option<String>,
    #[serde(default)]
    pub step_use_stress: Option<f64>,
    /// Acceptance-sampling plan and risk parameters.
    #[serde(default)]
    pub sampling_sample: Option<usize>,
    #[serde(default)]
    pub sampling_accept: Option<usize>,
    #[serde(default)]
    pub sampling_lot: Option<usize>,
    #[serde(default)]
    pub sampling_aql: Option<f64>,
    #[serde(default)]
    pub sampling_ltpd: Option<f64>,
    #[serde(default)]
    pub sampling_alpha: Option<f64>,
    #[serde(default)]
    pub sampling_beta: Option<f64>,
    /// DOE design options (factorial, Box–Behnken, Plackett–Burman, Taguchi).
    #[serde(default)]
    pub doe_factors: Option<String>,
    #[serde(default)]
    pub doe_runs: Option<usize>,
    #[serde(default)]
    pub doe_replicates: Option<usize>,
    #[serde(default)]
    pub doe_centers: Option<usize>,
    #[serde(default)]
    pub doe_block_size: Option<usize>,
    #[serde(default)]
    pub doe_randomize: Option<bool>,
    #[serde(default)]
    pub doe_seed: Option<u64>,
    #[serde(default)]
    pub taguchi_runs: Option<usize>,
    #[serde(default)]
    pub taguchi_design_factors: Option<usize>,
    /// PCA and matrix-plot options.
    #[serde(default)]
    pub pca_keep: Option<usize>,
    #[serde(default)]
    pub matrix_bins: Option<usize>,
    /// Time-series options.
    #[serde(default)]
    pub timeseries_kind: Option<String>,
    #[serde(default)]
    pub timeseries_window: Option<usize>,
    #[serde(default)]
    pub timeseries_period: Option<usize>,
    #[serde(default)]
    pub timeseries_model: Option<String>,
    #[serde(default)]
    pub timeseries_season: Option<usize>,
    #[serde(default)]
    pub timeseries_horizon: Option<usize>,
    /// Stepwise selection limits.
    #[serde(default)]
    pub step_max_steps: Option<usize>,
    #[serde(default)]
    pub step_alpha_enter: Option<f64>,
    #[serde(default)]
    pub step_alpha_remove: Option<f64>,
    /// Capability-study options.
    #[serde(default)]
    pub capability_subgroup: Option<usize>,
    #[serde(default)]
    pub capability_bootstrap: Option<usize>,
    #[serde(default)]
    pub capability_box_cox: Option<bool>,
    #[serde(default)]
    pub capability_require_stability: Option<bool>,
    /// Hypothesis-test options.
    #[serde(default)]
    pub t_alternative: Option<String>,
    #[serde(default)]
    pub t_pooled: Option<bool>,
    #[serde(default)]
    pub test2_method: Option<String>,
    #[serde(default)]
    pub test2_variance_method: Option<String>,
    #[serde(default)]
    pub test2_alternative: Option<String>,
    #[serde(default)]
    pub rank_alternative: Option<String>,
    /// Power and sample-size options.
    #[serde(default)]
    pub power_procedure: Option<String>,
    #[serde(default)]
    pub power_alpha: Option<f64>,
    #[serde(default)]
    pub power_target: Option<f64>,
    #[serde(default)]
    pub power_alternative: Option<String>,
    #[serde(default)]
    pub power_groups: Option<usize>,
    /// Curve-fit, scatter and response-surface options.
    #[serde(default)]
    pub curve_model: Option<String>,
    #[serde(default)]
    pub scatter_fit: Option<bool>,
    #[serde(default)]
    pub rsm_optimize: Option<bool>,
    #[serde(default)]
    pub rsm_contours: Option<bool>,
    #[serde(default)]
    pub rsm_steepest_ascent: Option<bool>,
    #[serde(default)]
    pub doe_main_effects: Option<bool>,
    /// Cluster, attribute-chart and quality options.
    #[serde(default)]
    pub kmeans_clusters: Option<usize>,
    #[serde(default)]
    pub kmeans_standardise: Option<bool>,
    #[serde(default)]
    pub kmeans_seed: Option<u64>,
    #[serde(default)]
    pub attribute_kind: Option<String>,
    #[serde(default)]
    pub attribute_size: Option<String>,
    #[serde(default)]
    pub t2_subgroup: Option<usize>,
    #[serde(default)]
    pub tolerance_proportion: Option<f64>,
    #[serde(default)]
    pub tolerance_free: Option<bool>,
    #[serde(default)]
    pub diagnostic_alpha: Option<f64>,
    /// Stepwise method and stopping criterion.
    #[serde(default)]
    pub step_method: Option<String>,
    #[serde(default)]
    pub step_criterion: Option<String>,
    /// Centre used by the variance-homogeneity test.
    #[serde(default)]
    pub variance_center: Option<String>,
    /// Two-way ANOVA interaction term.
    #[serde(default)]
    pub two_way_interaction: Option<bool>,
    /// Value counted as a success by the proportion tests.
    #[serde(default)]
    pub test2_success: Option<String>,
    /// Reference mean and margin of the equivalence tests.
    #[serde(default)]
    pub equivalence_null: Option<String>,
    #[serde(default)]
    pub equivalence_margin: Option<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: "en".to_owned(),
            dark_mode: false,
            zoom: 1.0,
            show_session: true,
            show_grid_lines: true,
            native_plot_windows: true,
            tour_seen: false,
            graph_overlay: fp_core::plot::GraphOverlay::default(),
            graph_overlay_title: None,
            conditional_rules: Vec::new(),
            probability_distribution: None,
            probability_display: None,
            probability_layout: None,
            probability_same_x: false,
            probability_same_y: false,
            capability_distribution: None,
            capability_confidence: None,
            control_chart_kind: None,
            control_chart_subgroup: None,
            control_chart_tests: None,
            control_chart_sensitivity: None,
            control_cusum_k: None,
            control_cusum_h: None,
            control_ewma_lambda: None,
            control_ewma_limit: None,
            regression_confidence: None,
            anova_confidence: None,
            analysis_confidences: std::collections::BTreeMap::new(),
            probability_confidence_band: true,
            gage_design: None,
            alt_life: None,
            alt_acceleration: None,
            alt_use_stress: None,
            step_life: None,
            step_acceleration: None,
            step_use_stress: None,
            sampling_sample: None,
            sampling_accept: None,
            sampling_lot: None,
            sampling_aql: None,
            sampling_ltpd: None,
            sampling_alpha: None,
            sampling_beta: None,
            doe_factors: None,
            doe_runs: None,
            doe_replicates: None,
            doe_centers: None,
            doe_block_size: None,
            doe_randomize: None,
            doe_seed: None,
            taguchi_runs: None,
            taguchi_design_factors: None,
            pca_keep: None,
            matrix_bins: None,
            timeseries_kind: None,
            timeseries_window: None,
            timeseries_period: None,
            timeseries_model: None,
            timeseries_season: None,
            timeseries_horizon: None,
            step_max_steps: None,
            step_alpha_enter: None,
            step_alpha_remove: None,
            capability_subgroup: None,
            capability_bootstrap: None,
            capability_box_cox: None,
            capability_require_stability: None,
            t_alternative: None,
            t_pooled: None,
            test2_method: None,
            test2_variance_method: None,
            test2_alternative: None,
            rank_alternative: None,
            power_procedure: None,
            power_alpha: None,
            power_target: None,
            power_alternative: None,
            power_groups: None,
            curve_model: None,
            scatter_fit: None,
            rsm_optimize: None,
            rsm_contours: None,
            rsm_steepest_ascent: None,
            doe_main_effects: None,
            kmeans_clusters: None,
            kmeans_standardise: None,
            kmeans_seed: None,
            attribute_kind: None,
            attribute_size: None,
            t2_subgroup: None,
            tolerance_proportion: None,
            tolerance_free: None,
            diagnostic_alpha: None,
            step_method: None,
            step_criterion: None,
            variance_center: None,
            two_way_interaction: None,
            test2_success: None,
            equivalence_null: None,
            equivalence_margin: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TTestKind {
    OneSample,
    TwoSample,
    Paired,
}

impl TTestKind {
    fn name(self) -> &'static str {
        match self {
            Self::OneSample => "1-Sample t",
            Self::TwoSample => "2-Sample t",
            Self::Paired => "Paired t",
        }
    }
}

/// Kind of equivalence test offered by the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquivalenceKind {
    /// One sample against a reference mean.
    OneSample,
    /// Two independent samples.
    TwoSample,
    /// Paired differences.
    Paired,
}

impl EquivalenceKind {
    fn name(self) -> &'static str {
        match self {
            Self::OneSample => "1-Sample Equivalence",
            Self::TwoSample => "2-Sample Equivalence",
            Self::Paired => "Paired Equivalence",
        }
    }
}

/// Kind of diagnostic test offered by the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticKind {
    /// Chi-square goodness-of-fit of observed against expected counts.
    #[default]
    ChiSquareGoodnessOfFit,
    /// Grubbs' test for a single outlier.
    Grubbs,
}

impl DiagnosticKind {
    fn name(self) -> &'static str {
        match self {
            Self::ChiSquareGoodnessOfFit => "Chi-Square Goodness-of-Fit",
            Self::Grubbs => "Grubbs Test",
        }
    }
}

/// Kind of time-series method offered by the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeSeriesKind {
    /// Centred moving average.
    #[default]
    MovingAverage,
    /// Classical decomposition.
    Decomposition,
    /// Exponential smoothing (level only, or Holt–Winters additive).
    ExponentialSmoothing,
}

impl TimeSeriesKind {
    fn name(self) -> &'static str {
        match self {
            Self::MovingAverage => "Moving Average",
            Self::Decomposition => "Decomposition",
            Self::ExponentialSmoothing => "Exponential Smoothing",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::MovingAverage => "moving-average",
            Self::Decomposition => "decomposition",
            Self::ExponentialSmoothing => "exponential-smoothing",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "moving-average" | "moving_average" | "ma" | "moving average" => {
                Some(Self::MovingAverage)
            }
            "decomposition" | "decompose" => Some(Self::Decomposition),
            "exponential-smoothing" | "exponential_smoothing" | "smoothing" | "holt-winters" => {
                Some(Self::ExponentialSmoothing)
            }
            _ => None,
        }
    }
}

/// Kind of one-way analysis offered by the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OneWayKind {
    /// One-way ANOVA of the response by the factor.
    #[default]
    Anova,
    /// Levene's test of equal variances.
    Levene,
    /// Bartlett's test of equal variances.
    Bartlett,
}

impl OneWayKind {
    fn name(self) -> &'static str {
        match self {
            Self::Anova => "One-Way ANOVA",
            Self::Levene => "Levene Test",
            Self::Bartlett => "Bartlett Test",
        }
    }
}

/// Kind of non-parametric rank test offered by the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankTestKind {
    /// Mann–Whitney test of two independent samples.
    MannWhitney,
    /// Wilcoxon signed-rank test of paired differences.
    Wilcoxon,
    /// Kruskal–Wallis test of a response by a factor.
    KruskalWallis,
    /// Friedman test of several columns measured on the same rows.
    Friedman,
    /// Mood's median test of a response by a factor.
    MoodsMedian,
    /// One-sample sign test of a median.
    SignTest,
}

impl RankTestKind {
    fn name(self) -> &'static str {
        match self {
            Self::MannWhitney => "Mann-Whitney Test",
            Self::Wilcoxon => "Wilcoxon Signed-Rank Test",
            Self::KruskalWallis => "Kruskal-Wallis Test",
            Self::Friedman => "Friedman Test",
            Self::MoodsMedian => "Mood's Median Test",
            Self::SignTest => "Sign Test",
        }
    }

    /// `true` when the test needs a response and a factor column.
    fn uses_factor(self) -> bool {
        matches!(self, Self::KruskalWallis | Self::MoodsMedian)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Test2Kind {
    OneProportion,
    TwoProportions,
    OneVariance,
    TwoVariances,
}

impl Test2Kind {
    fn name(self) -> &'static str {
        match self {
            Self::OneProportion => "1 Proportion",
            Self::TwoProportions => "2 Proportions",
            Self::OneVariance => "1 Variance",
            Self::TwoVariances => "2 Variances",
        }
    }

    fn needs_second(self) -> bool {
        matches!(self, Self::TwoProportions | Self::TwoVariances)
    }
}

/// One response row of the response optimizer.
#[derive(Debug, Clone)]
struct DesirabilityRow {
    column: String,
    goal: Goal,
    low: String,
    target: String,
    high: String,
    weight: String,
    importance: String,
}

/// classic statistical-desktop application state.
#[derive(Debug)]
pub struct FreePlotsApp {
    data: Option<DataFrame>,
    source: String,
    /// Worksheet tabs, in creation order.
    sheets: Vec<SheetState>,
    /// Index of the tab whose data is in `data`.
    active_sheet: usize,
    /// First worksheet tab shown when the strip is scrolled.
    tab_first: usize,
    /// In-app clipboard used by the cell context menu.
    clipboard: String,
    /// Set when the worksheet asks the runner for the system clipboard.
    paste_request: bool,
    /// Run analyses on a worker thread (the desktop runner switches this on).
    background_analyses: bool,
    /// Analysis currently running on the worker thread.
    pending_analysis: Option<Receiver<(String, Result<AnalysisOutput, String>)>>,
    /// Find/Replace dialog and its options.
    show_find_dialog: bool,
    find_text: String,
    replace_text: String,
    find_match_case: bool,
    find_whole_cell: bool,
    find_message: String,
    /// Widths of the worksheet columns, in pixels.
    column_widths: Vec<f32>,
    /// Per-column display formats.
    formats: std::collections::HashMap<String, ColumnFormat>,
    /// Conditional-formatting rules (persisted with the preferences).
    conditional_rules: Vec<ConditionalRule>,
    /// Points marked with Ctrl+click in the graph window, shared with the
    /// native viewport closure.
    brush_marks: std::sync::Arc<std::sync::Mutex<Vec<(usize, usize)>>>,
    /// Screen rectangle of the floating graph window, used to dock the graph
    /// editor to its right side.
    pub(super) graph_dialog_rect: Option<egui::Rect>,
    /// Dialog state of the conditional-formatting editor.
    show_conditional_dialog: bool,
    conditional_column: String,
    conditional_kind: ConditionalKind,
    conditional_value: String,
    conditional_value2: String,
    conditional_style: ConditionalStyle,
    /// Format Cells dialog and its options.
    show_format_dialog: bool,
    format_decimals: usize,
    format_thousands: bool,
    format_bold: bool,
    format_align: CellAlign,
    /// Pending "print the Session" request (the runner opens the PDF).
    print_request: Option<PathBuf>,
    /// Assistant dialog.
    show_assistant_dialog: bool,
    /// Dark theme.
    dark_mode: bool,
    /// Keyboard-shortcuts dialog.
    show_shortcuts: bool,
    /// Draw the worksheet grid lines.
    show_grid_lines: bool,
    /// Show the Session panel.
    show_session: bool,
    selected: Option<String>,
    selected_cell: Option<(usize, usize)>,
    selection_anchor: Option<(usize, usize)>,
    edit_target: Option<(usize, usize)>,
    edit_buffer: String,
    edit_dirty: bool,
    formula_focused: bool,
    inline_edit: Option<(usize, usize)>,
    inline_buffer: String,
    inline_request_focus: bool,
    column_summary: String,
    undo_stack: Vec<EditGroup>,
    redo_stack: Vec<EditGroup>,
    distribution: DistributionKind,
    show_probability_plot_dialog: bool,
    dialog_columns: Vec<String>,
    dialog_group: Option<String>,
    dialog_layout: PlotLayout,
    dialog_same_x: bool,
    dialog_same_y: bool,
    show_cap_dialog: bool,
    cap_column: String,
    cap_distribution: DistributionKind,
    cap_lsl: String,
    cap_usl: String,
    cap_target: String,
    cap_subgroup: usize,
    cap_confidence: f64,
    cap_bootstrap: usize,
    cap_box_cox: bool,
    cap_require_stability: bool,
    show_chart_dialog: bool,
    chart_column: String,
    chart_kind: ControlChartKind,
    chart_subgroup: usize,
    chart_subgroup_column: String,
    chart_phase_column: String,
    chart_tests: TestSet,
    chart_cusum_k: f64,
    chart_cusum_h: f64,
    chart_ewma_lambda: f64,
    chart_ewma_limit: f64,
    show_attr_dialog: bool,
    attr_column: String,
    attr_kind: AttributeChartKind,
    attr_sizes: Option<String>,
    attr_size: String,
    show_t_dialog: bool,
    show_equivalence_dialog: bool,
    rank_median: String,
    show_rank_dialog: bool,
    show_correlation_dialog: bool,
    language: fp_core::i18n::Lang,
    show_curve_dialog: bool,
    show_two_way_dialog: bool,
    show_crosstab_dialog: bool,
    show_g_chart_dialog: bool,
    show_partial_dialog: bool,
    show_box_cox_dialog: bool,
    box_cox_column: String,
    show_tolerance_dialog: bool,
    show_sort_dialog: bool,
    sort_keys: String,
    show_stack_dialog: bool,
    stack_columns: String,
    show_unstack_dialog: bool,
    unstack_value: String,
    unstack_group: String,
    show_subset_dialog: bool,
    show_date_diff_dialog: bool,
    date_diff_first: String,
    date_diff_second: String,
    date_diff_name: String,
    show_date_shift_dialog: bool,
    date_shift_column: String,
    date_shift_days: i64,
    date_shift_name: String,
    subset_column: String,
    subset_op: fp_core::reshape::SubsetOp,
    subset_value: String,
    show_agreement_dialog: bool,
    agreement_part: String,
    agreement_appraiser: String,
    agreement_rating: String,
    show_kmeans_dialog: bool,
    kmeans_columns: String,
    kmeans_clusters: usize,
    kmeans_standardise: bool,
    kmeans_seed: u64,
    show_taguchi_dialog: bool,
    taguchi_response: String,
    taguchi_factors: String,
    taguchi_quality: fp_core::taguchi::Quality,
    show_taguchi_design_dialog: bool,
    taguchi_runs: usize,
    taguchi_design_factors: usize,
    show_sampling_dialog: bool,
    sampling_plan_mode: bool,
    sampling_sample: usize,
    sampling_accept: usize,
    sampling_lot: usize,
    sampling_aql: f64,
    sampling_ltpd: f64,
    sampling_alpha: f64,
    sampling_beta: f64,
    show_t2_dialog: bool,
    t2_columns: String,
    t2_subgroup: usize,
    t2_confidence: f64,
    show_pca_dialog: bool,
    pca_columns: String,
    pca_keep: usize,
    show_timeseries_dialog: bool,
    timeseries_column: String,
    timeseries_kind: TimeSeriesKind,
    timeseries_window: usize,
    timeseries_period: usize,
    timeseries_model: fp_core::timeseries::DecompositionModel,
    timeseries_season: usize,
    timeseries_horizon: usize,
    show_alt_dialog: bool,
    alt_time: String,
    alt_stress: String,
    alt_mode: String,
    alt_horizon: String,
    show_step_stress_dialog: bool,
    ss_duration: String,
    ss_stress: String,
    ss_unit: String,
    ss_event: String,
    ss_use_stress: f64,
    ss_life: fp_core::alt::LifeModel,
    ss_acceleration: fp_core::alt::AccelerationModel,
    alt_use_stress: f64,
    alt_life: fp_core::alt::LifeModel,
    alt_acceleration: fp_core::alt::AccelerationModel,
    show_series_dialog: bool,
    series_column: String,
    series_time: String,
    show_stem_dialog: bool,
    stem_column: String,
    show_ecdf_dialog: bool,
    ecdf_column: String,
    ecdf_confidence: f64,
    show_dot_dialog: bool,
    dot_column: String,
    dot_factor: String,
    show_matrix_dialog: bool,
    matrix_columns: String,
    matrix_bins: usize,
    show_interval_dialog: bool,
    interval_response: String,
    interval_factor: String,
    interval_confidence: f64,
    show_sheet_dialog: bool,
    sheet_path: String,
    sheet_names: Vec<String>,
    sheet_choice: String,
    show_calc_dialog: bool,
    calc_name: String,
    calc_expression: String,
    show_pareto_dialog: bool,
    pareto_column: String,
    pareto_weight: String,
    show_ordinal_dialog: bool,
    ordinal_response: String,
    ordinal_predictors: String,
    show_poisson_dialog: bool,
    poisson_response: String,
    poisson_predictors: String,
    show_logistic_dialog: bool,
    logistic_response: String,
    logistic_predictors: String,
    show_linearity_dialog: bool,
    linearity_measurement: String,
    linearity_reference: String,
    tolerance_column: String,
    tolerance_proportion: f64,
    tolerance_confidence: f64,
    tolerance_free: bool,
    partial_x: String,
    partial_y: String,
    partial_control: String,
    g_chart_counts: String,
    g_chart_times: String,
    crosstab_row: String,
    crosstab_column: String,
    two_way_response: String,
    two_way_first: String,
    two_way_second: String,
    two_way_interaction: bool,
    curve_x: String,
    curve_y: String,
    curve_model: CurveModel,
    curve_confidence: f64,
    correlation_columns: String,
    show_diagnostic_dialog: bool,
    show_power_dialog: bool,
    power_procedure: PowerProcedure,
    power_effect: String,
    power_null: String,
    power_n: String,
    power_alpha: f64,
    power_target: f64,
    power_alternative: Alternative,
    power_groups: usize,
    diagnostic_kind: DiagnosticKind,
    diagnostic_column: String,
    diagnostic_expected: String,
    diagnostic_parameters: usize,
    diagnostic_alpha: f64,
    rank_kind: RankTestKind,
    rank_first: String,
    rank_second: String,
    rank_factor: String,
    rank_alternative: Alternative,
    rank_extra_columns: String,
    equivalence_kind: EquivalenceKind,
    equivalence_first: String,
    equivalence_second: String,
    equivalence_null: String,
    equivalence_margin: String,
    equivalence_confidence: f64,
    t_kind: TTestKind,
    t_first: String,
    t_second: String,
    t_null: String,
    t_alternative: Alternative,
    t_pooled: bool,
    t_confidence: f64,
    show_test2_dialog: bool,
    test2_kind: Test2Kind,
    test2_first: String,
    test2_second: String,
    test2_null: String,
    test2_success: String,
    test2_method: ProportionMethod,
    test2_variance_method: VarianceMethod,
    test2_alternative: Alternative,
    test2_confidence: f64,
    show_reg_dialog: bool,
    reg_response: String,
    reg_predictors: Vec<String>,
    reg_confidence: f64,
    show_step_dialog: bool,
    step_response: String,
    step_candidates: Vec<String>,
    step_method: StepMethod,
    step_criterion: StepCriterion,
    step_alpha_enter: f64,
    step_alpha_remove: f64,
    step_max_steps: usize,
    step_confidence: f64,
    show_anova_dialog: bool,
    anova_response: String,
    one_way_kind: OneWayKind,
    variance_center: VarianceCenter,
    anova_factor: String,
    anova_confidence: f64,
    show_km_dialog: bool,
    km_time: String,
    km_censor: Option<String>,
    km_censor_value: String,
    km_confidence: f64,
    show_weibull_dialog: bool,
    weibull_time: String,
    weibull_censor: Option<String>,
    weibull_censor_value: String,
    show_exp_dialog: bool,
    exp_time: String,
    exp_censor: Option<String>,
    exp_censor_value: String,
    show_log_dialog: bool,
    log_time: String,
    log_censor: Option<String>,
    log_censor_value: String,
    show_gage_dialog: bool,
    gage_measurement: String,
    gage_part: String,
    gage_operator: String,
    gage_design: GageDesign,
    show_type1_dialog: bool,
    type1_measurement: String,
    type1_reference: String,
    type1_lsl: String,
    type1_usl: String,
    show_doe_dialog: bool,
    doe_response: String,
    doe_factors: Vec<String>,
    doe_block: String,
    doe_main_effects: bool,
    show_rsm_dialog: bool,
    rsm_response: String,
    rsm_factors: Vec<String>,
    rsm_optimize: bool,
    rsm_contours: bool,
    rsm_steepest_ascent: bool,
    show_desirability_dialog: bool,
    desirability_factors: Vec<String>,
    desirability_responses: Vec<DesirabilityRow>,
    show_scatter_dialog: bool,
    scatter_x: String,
    scatter_y: String,
    scatter_fit: bool,
    show_design_dialog: bool,
    show_bb_dialog: bool,
    show_pb_dialog: bool,
    design_runs: usize,
    design_factors: String,
    design_replicates: usize,
    design_centers: usize,
    design_block_size: usize,
    design_randomize: bool,
    design_seed: u64,
    show_describe_dialog: bool,
    describe_column: String,
    describe_by: Option<String>,
    dialog_distribution: DistributionKind,
    dialog_display: PlotDisplay,
    dialog_ci: bool,
    dialog_ci_level: f64,
    session: String,
    plot: Option<PlotSpec>,
    show_plot: bool,
    show_open: bool,
    open_path: String,
    open_kind: FileKind,
    open_request: Option<OpenRequest>,
    preview_request: Option<String>,
    show_csv_options: bool,
    csv_path: String,
    csv_preview: Option<CsvPreview>,
    csv_delimiter: u8,
    csv_decimal_comma: bool,
    csv_header: bool,
    show_save: bool,
    show_overwrite: bool,
    /// Where the preferences are written, when the desktop resolved one.
    preferences_path: Option<PathBuf>,
    /// Last preferences known to be on disk, to avoid rewriting every frame.
    saved_preferences: Option<Preferences>,
    /// Last failure writing the preferences, shown in the Help dialog.
    preferences_error: Option<String>,
    /// Help → Assistant for agents (MCP).
    show_agents_dialog: bool,
    /// Help → Where the settings are stored.
    show_settings_info: bool,
    /// Help → Third-party licences.
    show_licences: bool,
    /// Help → Show the tour.
    show_tour: bool,
    /// Current tour step.
    tour_step: usize,
    /// `true` once the first-run tour has been shown.
    pub(super) tour_seen: bool,
    /// Presentation edits of the current graph.
    pub(super) graph_overlay: fp_core::plot::GraphOverlay,
    /// Title of the graph the overlay content belongs to, so annotations and
    /// reference lines never travel to an unrelated graph.
    pub(super) graph_overlay_title: Option<String>,
    /// Labels before the first edit, for "Reset graph edits".
    pub(super) graph_auto: Option<(String, String, String)>,
    pub(super) show_graph_labels: bool,
    pub(super) graph_label_title: String,
    pub(super) graph_label_subtitle: String,
    pub(super) graph_label_footer: String,
    pub(super) graph_label_x: String,
    pub(super) graph_label_y: String,
    pub(super) show_graph_text: bool,
    pub(super) graph_text_family: fp_core::plot::GraphFontFamily,
    pub(super) graph_text_size: f32,
    pub(super) show_graph_annotation: bool,
    pub(super) annotation_text: String,
    pub(super) annotation_x: f32,
    pub(super) annotation_y: f32,
    pub(super) annotation_size: f32,
    pub(super) annotation_color: fp_core::plot::AnnotationColor,
    pub(super) annotation_angle: f32,
    pub(super) show_notes: bool,
    pub(super) show_data_labels: bool,
    pub(super) data_label_mode: fp_core::plot::DataLabelMode,
    pub(super) show_percentiles: bool,
    pub(super) percentile_axis: fp_core::plot::ReferenceAxis,
    pub(super) percentile_values: String,
    pub(super) show_panel_layout: bool,
    pub(super) panel_columns_draft: usize,
    pub(super) show_axis_scale: bool,
    pub(super) show_data_display: bool,
    pub(super) axis_x_min: String,
    pub(super) axis_x_max: String,
    pub(super) axis_y_min: String,
    pub(super) axis_y_max: String,
    pub(super) show_reference_line: bool,
    pub(super) reference_axis: fp_core::plot::ReferenceAxis,
    pub(super) reference_value: String,
    pub(super) reference_label: String,
    /// Actions chosen in the graph's native window, picked up on the next frame.
    pub(super) plot_action_tx: std::sync::mpsc::Sender<graph_edit::GraphAction>,
    pub(super) plot_action_rx: std::sync::mpsc::Receiver<graph_edit::GraphAction>,
    /// Path and format waiting for the overwrite confirmation.
    overwrite_pending: Option<(String, FileKind, bool)>,
    save_path: String,
    save_kind: FileKind,
    save_decimal_comma: bool,
    save_request: Option<SaveRequest>,
    /// The project has changes that are not in a `.fpp` file.
    dirty: bool,
    /// Show the save/discard/cancel prompt before the window closes.
    close_prompt: bool,
    /// Close the window once the pending project save finishes.
    close_after_save: bool,
    /// The user already confirmed closing (discarded or saved).
    close_confirmed: bool,
    /// Ask the desktop runner to write the recovery snapshot.
    recovery_request: bool,
    /// Recovery file, next to the preferences.
    recovery_path: Option<PathBuf>,
    batch_request: Option<BatchRequest>,
    browse_request: Option<BrowseRequest>,
    folder_request: Option<FolderRequest>,
    show_batch_dialog: bool,
    show_samples_dialog: bool,
    batch_folder: String,
    batch_column: String,
    show_rename: bool,
    rename_text: String,
    show_sheet_rename: bool,
    sheet_rename_text: String,
    sheet_rename_target: usize,
    show_about: bool,
    status: String,
    error: Option<String>,
    light_visuals: bool,
    zoom: f32,
    date_format: DateFormat,
    native_plot_windows: bool,
    plot_viewport: Option<Arc<AtomicBool>>,
}

impl FreePlotsApp {
    /// Moves the live worksheet fields into the active tab.
    fn save_active_sheet(&mut self) {
        if let Some(sheet) = self.sheets.get_mut(self.active_sheet) {
            sheet.data = self.data.take();
            sheet.source = self.source.clone();
            sheet.undo = std::mem::take(&mut self.undo_stack);
            sheet.redo = std::mem::take(&mut self.redo_stack);
            sheet.selected_cell = self.selected_cell;
            sheet.column_widths = self.column_widths.clone();
        }
    }

    /// Loads a tab into the live worksheet fields.
    fn load_sheet(&mut self, index: usize) {
        if index >= self.sheets.len() {
            return;
        }
        self.active_sheet = index;
        let (data, source, undo, redo, cell, widths) = {
            let sheet = &mut self.sheets[index];
            (
                sheet.data.take(),
                sheet.source.clone(),
                std::mem::take(&mut sheet.undo),
                std::mem::take(&mut sheet.redo),
                sheet.selected_cell,
                sheet.column_widths.clone(),
            )
        };
        self.data = data;
        self.source = source;
        self.undo_stack = undo;
        self.redo_stack = redo;
        self.column_widths = widths;
        self.inline_edit = None;
        self.inline_request_focus = false;
        self.edit_dirty = false;
        self.plot = None;
        self.show_plot = false;
        let (column, row) = cell.unwrap_or((0, 0));
        self.select_cell(column, row);
    }

    /// Switches to another worksheet tab.
    pub fn switch_sheet(&mut self, index: usize) {
        if index >= self.sheets.len() || index == self.active_sheet {
            return;
        }
        self.save_active_sheet();
        self.load_sheet(index);
        self.tab_first = self.tab_first.min(index);
        self.status = format!(
            "{} — {} × {} columns",
            self.sheets[index].name,
            rows_label(self.data.as_ref().map_or(0, DataFrame::n_rows)),
            self.data.as_ref().map_or(0, DataFrame::n_columns)
        );
    }

    /// Adds a new eight-column worksheet and switches to it.
    pub fn add_sheet(&mut self) -> usize {
        self.save_active_sheet();
        let name = format!("Worksheet {}", self.sheets.len() + 1);
        self.sheets.push(Self::blank_sheet(name));
        let index = self.sheets.len() - 1;
        self.load_sheet(index);
        self.tab_first = self.tab_first.min(index);
        self.status = "New worksheet — 8 columns".to_owned();
        self.dirty = true;
        index
    }

    /// Closes a worksheet tab; the last tab cannot be closed.
    pub fn close_sheet(&mut self, index: usize) -> bool {
        if self.sheets.len() <= 1 {
            self.set_error("Keep at least one worksheet");
            return false;
        }
        if index >= self.sheets.len() {
            return false;
        }
        let name = self.sheets[index].name.clone();
        if index == self.active_sheet {
            self.data = None;
            self.undo_stack.clear();
            self.redo_stack.clear();
            self.sheets.remove(index);
            let next = index.min(self.sheets.len() - 1);
            self.load_sheet(next);
        } else {
            self.sheets.remove(index);
            if index < self.active_sheet {
                self.active_sheet -= 1;
            }
        }
        self.tab_first = self.tab_first.min(self.sheets.len().saturating_sub(1));
        self.status = format!("{name} closed");
        self.dirty = true;
        true
    }

    /// Opens the rename dialog for a worksheet tab.
    pub fn open_sheet_rename(&mut self, index: usize) {
        if let Some(sheet) = self.sheets.get(index) {
            self.sheet_rename_text = sheet.name.clone();
            self.sheet_rename_target = index;
            self.show_sheet_rename = true;
        }
    }

    /// Renames the worksheet tab being edited.
    pub fn commit_sheet_rename(&mut self) {
        let name = self.sheet_rename_text.trim().to_owned();
        if name.is_empty() {
            self.set_error("The worksheet name cannot be empty");
            return;
        }
        if let Some(sheet) = self.sheets.get_mut(self.sheet_rename_target) {
            sheet.name = name.clone();
            self.status = format!("Worksheet renamed to {name}");
            self.dirty = true;
        }
        self.show_sheet_rename = false;
    }

    /// Names of the worksheet tabs, in order.
    #[must_use]
    pub fn sheet_names(&self) -> Vec<String> {
        self.sheets.iter().map(|sheet| sheet.name.clone()).collect()
    }

    /// Index of the active worksheet tab.
    #[must_use]
    pub fn active_sheet(&self) -> usize {
        self.active_sheet
    }

    /// Number of worksheet tabs.
    #[must_use]
    pub fn worksheet_count(&self) -> usize {
        self.sheets.len()
    }

    /// First worksheet tab shown when the strip is scrolled.
    #[must_use]
    pub fn tab_first(&self) -> usize {
        self.tab_first
    }

    /// Replaces the active worksheet with an in-memory frame.
    ///
    /// A non-empty worksheet is kept and a new tab receives the frame, so
    /// opening a file never discards typed data.
    /// Every worksheet, in tab order, for the `.fpp` writer.
    ///
    /// The active sheet is still in the live fields, so it is filled from
    /// there while the others are cloned from their stored state.
    #[must_use]
    pub fn project_sheets(&self) -> Vec<(String, DataFrame)> {
        let mut sheets: Vec<(String, DataFrame)> = self
            .sheets
            .iter()
            .enumerate()
            .map(|(index, sheet)| {
                let data = if index == self.active_sheet {
                    self.data.clone()
                } else {
                    sheet.data.clone()
                };
                (
                    sheet.name.clone(),
                    data.unwrap_or_else(|| DataFrame::blank(8, 0).expect("blank sheet")),
                )
            })
            .collect();
        if sheets.is_empty()
            && let Some(data) = self.data.clone()
        {
            sheets.push(("Worksheet 1".to_owned(), data));
        }
        sheets
    }

    /// Replaces every worksheet with the sheets of a `.fpp` project.
    pub fn load_project_sheets(&mut self, sheets: Vec<(String, DataFrame)>, source: String) {
        if sheets.is_empty() {
            self.set_error("The project contains no worksheet");
            return;
        }
        let count = sheets.len();
        let (active_name, active_data) = (sheets[0].0.clone(), sheets[0].1.clone());
        self.sheets = sheets
            .into_iter()
            .map(|(name, data)| {
                let mut sheet = Self::blank_sheet(name);
                sheet.data = Some(data);
                sheet
            })
            .collect();
        self.active_sheet = 0;
        self.tab_first = 0;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.column_widths.clear();
        self.data = None;
        self.set_data(active_data, source);
        // `set_data` names the tab from the source; keep the stored name.
        if let Some(sheet) = self.sheets.get_mut(0) {
            sheet.name = active_name;
        }
        self.status = format!("Loaded {count} worksheet(s)");
    }

    pub fn set_data(&mut self, df: DataFrame, source: String) {
        let empty_active = self.data.as_ref().is_none_or(|frame| frame.n_rows() == 0);
        if !empty_active {
            self.save_active_sheet();
            let name = format!("Worksheet {}", self.sheets.len() + 1);
            self.sheets.push(Self::blank_sheet(name));
            self.active_sheet = self.sheets.len() - 1;
        }
        let rows = df.n_rows();
        let columns = df.n_columns();
        self.selected = first_numeric_column(&df).map(str::to_owned);
        self.reset_dialog_defaults();
        self.data = Some(df);
        self.status = format!("Loaded {source} — {} × {columns} columns", rows_label(rows));
        self.source = source;
        let source_path = self.source.split(" [").next().unwrap_or("Untitled");
        let lower = source_path.to_ascii_lowercase();
        self.save_kind = if lower.ends_with(".fpp") {
            FileKind::Project
        } else {
            FileKind::Csv
        };
        self.save_path = match self.save_kind {
            FileKind::Csv if lower.ends_with(".csv") => source_path.to_owned(),
            FileKind::Csv => "worksheet.csv".to_owned(),
            FileKind::Project if lower.ends_with(".fpp") => source_path.to_owned(),
            FileKind::Project => "worksheet.fpp".to_owned(),
            FileKind::Html => "session.html".to_owned(),
            FileKind::Pdf => "session.pdf".to_owned(),
            FileKind::Xlsx => "worksheet.xlsx".to_owned(),
            FileKind::Svg => "graph.svg".to_owned(),
            FileKind::GraphPdf => "graph.pdf".to_owned(),
            FileKind::Png => "graph.png".to_owned(),
            FileKind::Jpeg => "graph.jpg".to_owned(),
        };
        self.session.clear();
        self.plot = None;
        self.show_plot = false;
        self.error = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.dirty = !empty_active;
        self.close_prompt = false;
        self.close_after_save = false;
        self.close_confirmed = false;
        let column = self
            .selected
            .as_deref()
            .and_then(|name| self.data.as_ref()?.column_index(name).ok())
            .unwrap_or(0);
        self.select_cell(column, 0);
        if let Some(sheet) = self.sheets.get_mut(self.active_sheet) {
            sheet.data = None;
            sheet.source = self.source.clone();
            sheet.undo.clear();
            sheet.redo.clear();
            sheet.selected_cell = self.selected_cell;
        }
    }

    /// Creates a new eight-column worksheet tab.
    pub fn new_worksheet(&mut self) {
        self.add_sheet();
    }

    /// Normalized selection rectangle `(first column, first row, last column, last row)`.
    #[must_use]
    pub fn selection_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let (column, row) = self.selected_cell?;
        let (anchor_column, anchor_row) = self.selection_anchor.unwrap_or((column, row));
        Some((
            column.min(anchor_column),
            row.min(anchor_row),
            column.max(anchor_column),
            row.max(anchor_row),
        ))
    }

    /// Cell currently being edited inline, if any.
    #[must_use]
    pub fn inline_edit(&self) -> Option<(usize, usize)> {
        self.inline_edit
    }

    /// Text of the inline editor.
    #[must_use]
    pub fn inline_buffer(&self) -> &str {
        &self.inline_buffer
    }

    /// Text currently in the formula bar.
    #[must_use]
    pub fn edit_buffer(&self) -> &str {
        &self.edit_buffer
    }

    /// Descriptive summary of the selected column.
    #[must_use]
    pub fn column_summary(&self) -> &str {
        &self.column_summary
    }

    /// Replaces the formula-bar text.
    pub fn set_edit_buffer(&mut self, text: impl Into<String>) {
        self.edit_buffer = text.into();
        self.edit_dirty = true;
    }

    fn cancel_inline_edit(&mut self) {
        self.inline_edit = None;
        self.inline_request_focus = false;
        self.edit_dirty = false;
        if let Some((column, row)) = self.selected_cell {
            self.select_cell(column, row);
        }
    }

    /// Appends a row and selects it.
    pub fn add_row(&mut self) {
        let Some(df) = self.data.as_mut() else {
            self.set_error("No worksheet open");
            return;
        };
        let had_columns = df.n_columns() > 0;
        match df.add_row() {
            Ok(row) => {
                self.error = None;
                let mut actions = vec![EditAction::InsertRow { row }];
                if !had_columns {
                    actions.insert(0, add_column_action(df, 0));
                }
                self.push_undo(EditGroup { actions });
                self.select_cell(self.selected_cell.map_or(0, |(c, _)| c), row);
                self.status = format!("Added row {}", row + 1);
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    /// Inserts a row above the selected row.
    pub fn insert_row(&mut self) {
        let Some((column, row)) = self.selected_cell else {
            self.add_row();
            return;
        };
        let Some(df) = self.data.as_mut() else {
            self.set_error("No worksheet open");
            return;
        };
        let had_columns = df.n_columns() > 0;
        let row = row.min(df.n_rows());
        match df.insert_row(row) {
            Ok(()) => {
                self.error = None;
                let mut actions = vec![EditAction::InsertRow { row }];
                if !had_columns {
                    actions.insert(0, add_column_action(df, 0));
                }
                self.push_undo(EditGroup { actions });
                self.select_cell(column, row);
                self.status = format!("Inserted row {}", row + 1);
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    /// Inserts an empty row below the selected one.
    pub fn insert_row_below(&mut self) {
        let Some((column, row)) = self.selected_cell else {
            self.add_row();
            return;
        };
        self.select_cell(column, row + 1);
        self.insert_row();
    }

    /// Copies the selection and clears it.
    pub fn cut_selection(&mut self) {
        if let Some(text) = self.selection_tsv() {
            self.clipboard = text;
        }
        self.clear_selection();
    }

    /// Deletes the selected row.
    pub fn delete_row(&mut self) {
        let Some((column, row)) = self.selected_cell else {
            self.set_error("Select a row first");
            return;
        };
        let Some(df) = self.data.as_mut() else {
            self.set_error("No worksheet open");
            return;
        };
        if row >= df.n_rows() {
            self.set_error("The selected row is already empty");
            return;
        }
        let cells: Vec<Option<String>> = (0..df.n_columns())
            .map(|column| cell_state(df, column, row))
            .collect();
        match df.remove_row(row) {
            Ok(()) => {
                self.error = None;
                let next_row = row.min(df.n_rows().saturating_sub(1));
                self.push_undo(EditGroup {
                    actions: vec![EditAction::RemoveRow { row, cells }],
                });
                self.select_cell(column, next_row);
                self.status = format!("Deleted row {}", row + 1);
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    /// Appends a numeric column and selects it.
    pub fn add_column(&mut self) {
        self.add_column_of_type(DType::Float);
    }

    /// Appends a text column and selects it.
    pub fn add_text_column(&mut self) {
        self.add_column_of_type(DType::Text);
    }

    /// Adds an empty date column.
    pub fn add_date_column(&mut self) {
        self.add_column_of_type(DType::Date);
    }

    fn add_column_of_type(&mut self, dtype: DType) {
        let Some(df) = self.data.as_mut() else {
            self.set_error("No worksheet open");
            return;
        };
        let index = df.add_column(dtype);
        let name = df.columns()[index].name().to_owned();
        self.error = None;
        self.push_undo(EditGroup {
            actions: vec![EditAction::AddColumn { index, name, dtype }],
        });
        self.select_cell(index, 0);
        self.status = format!("Added column C{}", index + 1);
    }

    /// Deletes the selected column.
    pub fn delete_column(&mut self) {
        let Some((column, _)) = self.selected_cell else {
            self.set_error("Select a column first");
            return;
        };
        let Some(df) = self.data.as_mut() else {
            self.set_error("No worksheet open");
            return;
        };
        let removed = df.columns()[column].clone();
        match df.remove_column(column) {
            Ok(()) => {
                self.error = None;
                let next_column = column.min(df.n_columns().saturating_sub(1));
                self.push_undo(EditGroup {
                    actions: vec![EditAction::RemoveColumn {
                        index: column,
                        column: removed,
                    }],
                });
                self.select_cell(next_column, 0);
                self.status = "Deleted column".to_owned();
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    /// Opens the rename-column dialog for the selected column.
    pub fn begin_rename_column(&mut self) {
        let Some((column, _)) = self.selected_cell else {
            self.set_error("Select a column first");
            return;
        };
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open");
            return;
        };
        self.rename_text = df
            .columns()
            .get(column)
            .map(|c| c.name().to_owned())
            .unwrap_or_else(|| format!("C{}", column + 1));
        self.show_rename = true;
    }

    /// Applies the rename dialog.
    pub fn commit_rename_column(&mut self) {
        let Some((column, _)) = self.selected_cell else {
            self.show_rename = false;
            self.show_sheet_rename = false;
            return;
        };
        let Some(df) = self.data.as_mut() else {
            self.show_rename = false;
            return;
        };
        let before = df
            .columns()
            .get(column)
            .map(|c| c.name().to_owned())
            .unwrap_or_default();
        let after = self.rename_text.trim().to_owned();
        match df.rename_column(column, &self.rename_text) {
            Ok(()) => {
                self.error = None;
                self.show_rename = false;
                self.status = format!("Renamed column to {}", self.rename_text.trim());
                self.push_undo(EditGroup {
                    actions: vec![EditAction::RenameColumn {
                        index: column,
                        before,
                        after,
                    }],
                });
                self.select_cell(column, self.selected_cell.map_or(0, |(_, r)| r));
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    fn selection_is_numeric(&self) -> bool {
        let Some(df) = self.data.as_ref() else {
            return false;
        };
        self.selected
            .as_deref()
            .and_then(|name| df.column(name).ok())
            .is_some_and(|column| {
                matches!(
                    column.dtype(),
                    fp_core::data::DType::Float | fp_core::data::DType::Integer
                )
            })
    }

    /// Selects the distribution used by the next probability plot.
    pub fn set_distribution(&mut self, distribution: DistributionKind) {
        self.distribution = distribution;
    }

    /// Reports a front-end error in the status bar.
    pub fn set_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.status = message.clone();
        self.error = Some(message);
        // A failed save must not be followed by the automatic close.
        self.close_after_save = false;
    }

    /// Takes a pending "open this path" request emitted by the File menu.
    ///
    /// The desktop runner performs the actual read (this crate never touches
    /// the file system) and calls [`Self::set_data`].
    pub fn take_open_request(&mut self) -> Option<OpenRequest> {
        self.open_request.take()
    }

    /// Sets the status line.
    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = message.into();
    }

    /// Takes the pending "paste from the system clipboard" request.
    pub fn take_paste_request(&mut self) -> bool {
        std::mem::take(&mut self.paste_request)
    }

    /// Pastes the in-app clipboard (fallback when the system one is empty).
    pub fn paste_internal(&mut self) {
        let text = self.clipboard.clone();
        if !text.is_empty() {
            self.paste_tsv(&text);
            self.status = "Pasted".to_owned();
        }
    }

    /// Switches the dark theme on or off.
    pub fn set_dark_mode(&mut self, dark: bool) {
        self.dark_mode = dark;
    }

    /// `true` when one of the graph edit dialogs is open.
    #[must_use]
    pub fn any_graph_editor_open(&self) -> bool {
        self.show_graph_labels
            || self.show_graph_text
            || self.show_graph_annotation
            || self.show_notes
            || self.show_data_labels
            || self.show_percentiles
            || self.show_panel_layout
            || self.show_axis_scale
            || self.show_data_display
            || self.show_reference_line
    }

    /// Dialog builder for a graph editor, pinned to the right of the floating
    /// graph window when it is visible.
    fn graph_editor_dialog(&self, title: impl Into<String>) -> DialogWindow<'static> {
        let mut dialog = dialog_window(title.into())
            .without_clear()
            .collapsible(false)
            .resizable(false)
            .default_width(376.0);
        if let Some(rect) = self.graph_dialog_rect {
            let width = 372.0;
            let x = (rect.right() - width - 6.0).max(rect.left());
            dialog = dialog.fixed_pos(egui::pos2(x, rect.top() + 4.0));
        }
        dialog
    }

    /// Chooses whether graphs open in their own window (off keeps them as a
    /// floating dialog inside the main window).
    pub fn set_native_plot_windows(&mut self, native: bool) {
        self.native_plot_windows = native;
    }

    /// Whether the dark theme is active.
    #[must_use]
    pub fn dark_mode(&self) -> bool {
        self.dark_mode
    }

    /// Takes the pending print request (a PDF path the runner opens).
    pub fn take_print_request(&mut self) -> Option<PathBuf> {
        self.print_request.take()
    }

    /// Takes the request for a CSV preview (the desktop runner reads the file).
    pub fn take_preview_request(&mut self) -> Option<String> {
        self.preview_request.take()
    }

    /// Adds a warning to the Session and to the status line.
    pub fn push_warning(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.append_session(&format!("Warning: {message}\n"));
        if !self.status.is_empty() {
            self.status = format!("{} — {message}", self.status);
        }
    }

    /// Receives the CSV preview and opens the options dialog.
    pub fn set_csv_preview(&mut self, path: String, preview: CsvPreview) {
        self.csv_path = path;
        self.csv_delimiter = preview.options.delimiter;
        self.csv_decimal_comma = preview.options.decimal_comma;
        self.csv_header = preview.options.has_header;
        self.csv_preview = Some(preview);
        self.show_csv_options = true;
    }

    /// Opens the "open file" dialog for a file kind.
    pub fn begin_open(&mut self, kind: FileKind) {
        self.open_kind = kind;
        self.show_open = true;
    }

    /// Asks the desktop runner to show the native file browser.
    ///
    /// The runner answers with [`Self::set_browsed_path`].
    pub fn request_browse(&mut self, kind: FileKind, save: bool) {
        self.browse_request = Some(browse_request(kind, save));
    }

    /// The path typed in the open dialog.
    pub fn open_path(&self) -> &str {
        &self.open_path
    }

    /// The path typed in the save dialog.
    pub fn save_path(&self) -> &str {
        &self.save_path
    }

    /// Asks the desktop runner for a native folder browser.
    pub fn request_folder_browse(&mut self, title: impl Into<String>) {
        self.folder_request = Some(FolderRequest {
            title: title.into(),
        });
    }

    /// Takes a pending "show a folder browser" request.
    pub fn take_folder_request(&mut self) -> Option<FolderRequest> {
        self.folder_request.take()
    }

    /// Fills the batch folder with the folder chosen in the browser.
    pub fn set_browsed_folder(&mut self, path: String) {
        self.batch_folder = path;
    }

    /// Takes a pending "show a file browser" request.
    ///
    /// The desktop runner opens the native dialog and calls
    /// [`Self::set_browsed_path`] with the result.
    pub fn take_browse_request(&mut self) -> Option<BrowseRequest> {
        self.browse_request.take()
    }

    /// Fills the open or the save path with the path chosen in the browser.
    pub fn set_browsed_path(&mut self, path: String, save: bool) {
        if save {
            self.save_path = path;
        } else {
            self.open_path = path;
        }
    }

    /// Takes a pending "save this worksheet" request.
    ///
    /// The desktop runner performs the write (this crate never touches the
    /// file system) and calls [`Self::mark_saved`].
    pub fn take_save_request(&mut self) -> Option<SaveRequest> {
        self.save_request.take()
    }

    fn begin_save(&mut self, kind: FileKind) {
        self.save_kind = kind;
        let source = self.source.split(" [").next().unwrap_or("worksheet");
        let file = source.rsplit(['/', '\\']).next().unwrap_or(source);
        let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
        let expected = match kind {
            FileKind::Csv => ".csv",
            FileKind::Project => ".fpp",
            FileKind::Html => ".html",
            FileKind::Pdf => ".pdf",
            FileKind::Xlsx => ".xlsx",
            FileKind::Svg => ".svg",
            FileKind::GraphPdf => ".pdf",
            FileKind::Png => ".png",
            FileKind::Jpeg => ".jpg",
        };
        if !self.save_path.to_ascii_lowercase().ends_with(expected) {
            self.save_path = format!("{stem}{expected}");
        }
        self.show_save = true;
    }

    /// Opens the Session PDF export dialog.
    pub fn begin_pdf_export(&mut self) {
        self.save_kind = FileKind::Pdf;
        self.save_path = "session.pdf".to_owned();
        self.show_save = true;
    }

    /// Records that the worksheet was written to `path`.
    ///
    /// Only a project save clears the unsaved-changes flag: CSV, HTML, PDF and
    /// graph exports are copies, not the document.
    pub fn mark_saved(&mut self, path: &str, kind: FileKind) {
        self.source = path.to_owned();
        self.status = format!("Saved {path}");
        self.error = None;
        if kind == FileKind::Project {
            self.dirty = false;
        }
    }

    /// `true` when the project has changes that are not in a `.fpp` file.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.dirty
    }

    /// Handles the window's close request.
    ///
    /// Returns `true` when the caller must cancel the close and show the
    /// prompt. A dirty project also asks the desktop runner for a recovery
    /// snapshot before the prompt appears.
    pub fn request_close(&mut self) -> bool {
        if self.close_after_save {
            return true;
        }
        if self.dirty && !self.close_confirmed {
            self.recovery_request = true;
            self.close_prompt = true;
            true
        } else {
            false
        }
    }

    /// `true` while the save/discard/cancel prompt is open.
    #[must_use]
    pub fn close_prompt_open(&self) -> bool {
        self.close_prompt
    }

    /// Takes the pending recovery-snapshot request.
    pub fn take_recovery_request(&mut self) -> bool {
        std::mem::take(&mut self.recovery_request)
    }

    /// Recovery file next to the preferences, when the desktop resolved one.
    #[must_use]
    pub fn recovery_path(&self) -> Option<&std::path::Path> {
        self.recovery_path.as_deref()
    }

    /// Saves the project and closes once the runner confirms the write.
    pub fn save_before_close(&mut self) {
        self.close_prompt = false;
        self.close_after_save = true;
        self.begin_save(FileKind::Project);
    }

    /// Closes without saving; the recovery snapshot is left on disk.
    pub fn discard_and_close(&mut self) {
        self.close_prompt = false;
        self.close_after_save = false;
        self.close_confirmed = true;
        self.dirty = false;
    }

    /// Keeps the window open.
    pub fn cancel_close(&mut self) {
        self.close_prompt = false;
        self.close_after_save = false;
        self.close_confirmed = false;
    }

    /// Last generated plot, if any.
    #[must_use]
    pub fn plot(&self) -> Option<&PlotSpec> {
        self.plot.as_ref()
    }

    /// Current worksheet.
    #[must_use]
    pub fn data(&self) -> Option<&DataFrame> {
        self.data.as_ref()
    }

    /// Status-bar text.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Source label shown in the toolbar (file name or `Untitled`).
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Runs the default probability plot on the selected column.
    pub fn run_probability_plot(&mut self) {
        if self.data.is_none() {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        }
        let Some(column) = self.selected.clone() else {
            self.set_error("Select a numeric column first");
            return;
        };
        let spec =
            AnalysisSpec::probability_plot(column.clone()).with_distribution(self.distribution);
        self.execute_spec(spec, format!("Probability plot: {column}"));
    }

    fn execute_spec(&mut self, spec: AnalysisSpec, label: String) {
        let _ = self.run_spec(spec, label);
    }

    /// Runs an analysis on the open worksheet and reports the outcome.
    ///
    /// # Errors
    ///
    /// Returns the message when there is no data or the engine rejects the
    /// analysis; the same message reaches the status line.
    pub fn run_spec(&mut self, spec: AnalysisSpec, label: String) -> Result<(), String> {
        let Some(df) = self.data.as_ref() else {
            let message = "No worksheet open: use File → Open CSV…".to_owned();
            self.set_error(&message);
            return Err(message);
        };
        if df.n_rows() == 0 {
            let message = "The worksheet is empty: type data or use File → Open CSV…".to_owned();
            self.set_error(&message);
            return Err(message);
        }
        if !self.background_analyses {
            return self.run_spec_blocking(spec, label);
        }
        if self.pending_analysis.is_some() {
            let message = "An analysis is already running; wait for it to finish".to_owned();
            self.set_error(&message);
            return Err(message);
        }
        let df = df.clone();
        let (sender, receiver) = channel();
        let worker_label = label.clone();
        // The engine is pure and Send, so the snapshot can move to the thread;
        // the interface keeps painting and polls the channel.
        let spawned = std::thread::Builder::new()
            .name("fp-analysis".to_owned())
            .spawn(move || {
                let outcome = run_analysis(&df, &spec).map_err(|error| error.to_string());
                let _ = sender.send((worker_label, outcome));
            });
        if let Err(error) = spawned {
            let message = format!("cannot start the analysis thread: {error}");
            self.set_error(&message);
            return Err(message);
        }
        self.status = format!("Running {label}…");
        self.error = None;
        self.pending_analysis = Some(receiver);
        Ok(())
    }

    /// Runs an analysis on the calling thread (bridge replies, tests, CLI).
    ///
    /// # Errors
    ///
    /// As [`Self::run_spec`].
    pub fn run_spec_blocking(&mut self, spec: AnalysisSpec, label: String) -> Result<(), String> {
        let Some(df) = self.data.as_ref() else {
            let message = "No worksheet open: use File → Open CSV…".to_owned();
            self.set_error(&message);
            return Err(message);
        };
        if df.n_rows() == 0 {
            let message = "The worksheet is empty: type data or use File → Open CSV…".to_owned();
            self.set_error(&message);
            return Err(message);
        }
        match run_analysis(df, &spec) {
            Ok(out) => {
                self.apply_output(out, label);
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                self.set_error(&message);
                Err(message)
            }
        }
    }

    /// Whether an analysis is running on the worker thread.
    #[must_use]
    pub fn analysis_running(&self) -> bool {
        self.pending_analysis.is_some()
    }

    /// Switches analyses to the worker thread.
    pub fn set_background_analyses(&mut self, enabled: bool) {
        self.background_analyses = enabled;
    }

    /// Adopts a new graph and drops the content edits of the previous one.
    ///
    /// Font size, colours, grid and the other presentation defaults stay (the
    /// user set them once); annotations, reference lines, subtitle/footer and
    /// manual axis ranges belong to one graph and must not leak into another.
    fn adopt_plot(&mut self, plot: Option<fp_core::plot::PlotSpec>) {
        if let Some(next) = &plot
            && self.graph_overlay_title.as_deref() != Some(next.title.as_str())
        {
            self.graph_overlay.subtitle = None;
            self.graph_overlay.footer = None;
            self.graph_overlay.reference_lines.clear();
            self.graph_overlay.annotations.clear();
            self.graph_overlay.x_range = None;
            self.graph_overlay.y_range = None;
            // Automatic label snapshot from the previous graph must not be
            // restored by "Reset graph edits" on this one.
            self.graph_auto = None;
            self.graph_overlay_title = Some(next.title.clone());
        }
        self.show_plot = plot.is_some();
        self.plot = plot;
    }

    /// Applies a finished analysis to the Session and the plot.
    fn apply_output(&mut self, output: AnalysisOutput, label: String) {
        for result in &output.results {
            self.append_result(result);
        }
        self.adopt_plot(output.plot);
        self.brush_marks.lock().map(|mut marks| marks.clear()).ok();
        self.status = label;
        self.error = None;
    }

    /// Collects a finished background analysis, if there is one.
    pub fn poll_analysis(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.pending_analysis else {
            return;
        };
        match receiver.try_recv() {
            Ok((label, outcome)) => {
                self.pending_analysis = None;
                match outcome {
                    Ok(output) => self.apply_output(output, label),
                    Err(message) => self.set_error(message),
                }
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(80));
            }
            Err(TryRecvError::Disconnected) => {
                self.pending_analysis = None;
                self.set_error("the analysis thread stopped unexpectedly");
            }
        }
    }

    /// Runs a registered plugin on the current worksheet.
    pub fn run_plugin(&mut self, id: &str) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let outcome = fp_core::plugin::registry()
            .lock()
            .map_err(|_| "the plugin registry is poisoned".to_owned())
            .and_then(|registry| {
                registry
                    .run(id, &df, &fp_core::plugin::PluginOptions::default())
                    .map_err(|error| error.to_string())
            });
        match outcome {
            Ok(out) => {
                for result in &out.results {
                    self.append_result(result);
                }
                self.adopt_plot(out.plot);
                self.status = format!("Plugin: {id}");
                self.error = None;
            }
            Err(error) => self.set_error(error),
        }
    }

    /// Runs a Shewhart control chart on one column.
    pub fn run_control_chart(
        &mut self,
        column: impl Into<String>,
        kind: ControlChartKind,
        subgroup: usize,
    ) {
        self.run_control_chart_with_tests(column, kind, subgroup, TestSet::all());
    }

    /// Runs a control chart with an explicit set of tests for special causes.
    pub fn run_control_chart_with_tests(
        &mut self,
        column: impl Into<String>,
        kind: ControlChartKind,
        subgroup: usize,
        tests: TestSet,
    ) {
        self.run_control_chart_with_options(
            column,
            kind,
            subgroup,
            tests,
            fp_core::spc::CusumOptions::default(),
            fp_core::spc::EwmaOptions::default(),
        );
    }

    /// Runs a control chart with explicit test and CUSUM options.
    pub fn run_control_chart_with_options(
        &mut self,
        column: impl Into<String>,
        kind: ControlChartKind,
        subgroup: usize,
        tests: TestSet,
        cusum: fp_core::spc::CusumOptions,
        ewma: fp_core::spc::EwmaOptions,
    ) {
        let column = column.into();
        let phases = matches!(
            kind,
            ControlChartKind::IndividualsMovingRange
                | ControlChartKind::XbarR
                | ControlChartKind::XbarS
        );
        let phase_column = (phases && !self.chart_phase_column.is_empty())
            .then(|| self.chart_phase_column.clone());
        let subgroup_column = (phase_column.is_none() && !self.chart_subgroup_column.is_empty())
            .then(|| self.chart_subgroup_column.clone());
        let subgroup = if subgroup_column.is_some() {
            0
        } else {
            subgroup
        };
        self.execute_spec(
            AnalysisSpec::control_chart(column.clone(), kind, subgroup)
                .with_control_chart_tests(tests)
                .with_cusum_options(cusum)
                .with_ewma_options(ewma)
                .with_subgroup_column(subgroup_column)
                .with_phase_column(phase_column),
            format!("{} chart: {column}", kind.name()),
        );
    }

    /// Runs a run chart with runs tests on one column.
    pub fn run_run_chart(&mut self, column: impl Into<String>) {
        let column = column.into();
        self.execute_spec(
            AnalysisSpec::run_chart(column.clone()),
            format!("Run chart: {column}"),
        );
    }

    /// Builds and runs the attribute chart selected in the dialog.
    pub fn run_attribute_chart(&mut self) {
        if self.attr_column.is_empty() {
            self.set_error("Select the defects column");
            return;
        }
        let constant = if self.attr_kind.needs_size() {
            match self.attr_size.trim() {
                "" => None,
                raw => match raw.parse::<usize>() {
                    Ok(value) if value > 0 => Some(value),
                    _ => {
                        self.set_error("Constant subgroup size must be a positive integer");
                        return;
                    }
                },
            }
        } else {
            None
        };
        if self.attr_kind.needs_size() && self.attr_sizes.is_none() && constant.is_none() {
            self.set_error("Provide a subgroup-size column or a constant size");
            return;
        }
        let spec = AnalysisSpec::attribute_chart(self.attr_kind, self.attr_column.clone())
            .with_attribute_size(self.attr_sizes.clone(), constant);
        let label = format!("{} chart: {}", self.attr_kind.name(), self.attr_column);
        self.execute_spec(spec, label);
        self.show_attr_dialog = false;
    }

    /// Builds and runs the tolerance interval of the dialog.
    pub fn run_tolerance_dialog(&mut self) {
        if self.tolerance_column.is_empty() {
            self.set_error("Select the column to analyse");
            return;
        }
        let spec = AnalysisSpec::tolerance_interval(self.tolerance_column.clone())
            .with_tolerance_proportion((self.tolerance_proportion / 100.0).clamp(0.01, 0.999))
            .with_tolerance_confidence((self.tolerance_confidence / 100.0).clamp(0.01, 0.999))
            .with_tolerance_free(self.tolerance_free);
        let label = format!("Tolerance Interval: {}", self.tolerance_column);
        self.execute_spec(spec, label);
        self.show_tolerance_dialog = false;
    }

    /// Replaces the worksheet with the result of a transformation and notes
    /// it in the Session.
    fn replace_worksheet(&mut self, df: DataFrame, note: &str) {
        let rows = df.n_rows();
        let columns = df.n_columns();
        self.set_data(df, note.to_owned());
        self.append_session(&format!("{note}: {rows} rows × {columns} columns.\n"));
        self.status = format!("{note} — {rows} rows × {columns} columns");
    }

    /// Transposes the worksheet.
    pub fn run_transpose(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::reshape::transpose(&df) {
            Ok(transposed) => self.replace_worksheet(transposed, "Transposed worksheet"),
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Offers the worksheets of a workbook when it has more than one.
    ///
    /// The runner calls this after reading the sheet names of an Excel file:
    /// with a single sheet it loads that sheet, otherwise the interface asks
    /// which one to use and emits a new [`OpenRequest`].
    pub fn set_workbook_sheets(&mut self, path: &str, sheets: Vec<String>) {
        if sheets.is_empty() {
            self.set_error("The workbook has no worksheets");
            return;
        }
        if sheets.len() == 1 {
            self.open_request = Some(OpenRequest {
                path: path.to_owned(),
                kind: FileKind::Xlsx,
                sheet: Some(sheets[0].clone()),
                csv_options: None,
            });
            return;
        }
        self.sheet_path = path.to_owned();
        self.sheet_choice = sheets[0].clone();
        self.sheet_names = sheets;
        self.show_sheet_dialog = true;
    }

    /// Runs a boxplot of one selected column.
    pub fn run_boxplot(&mut self, column: impl Into<String>) {
        let column = column.into();
        self.execute_spec(
            AnalysisSpec::boxplot(column.clone()),
            format!("Boxplot: {column}"),
        );
    }

    /// Runs an automatic-bin histogram of one selected column.
    pub fn run_histogram(&mut self, column: impl Into<String>) {
        let column = column.into();
        self.execute_spec(
            AnalysisSpec::histogram(column.clone()),
            format!("Histogram: {column}"),
        );
    }

    /// Opens the scatterplot dialog.
    pub fn open_scatter_dialog(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let numeric: Vec<String> = df
            .columns()
            .iter()
            .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
            .map(|column| column.name().to_owned())
            .collect();
        if numeric.len() < 2 {
            self.set_error("Scatterplot needs two numeric columns");
            return;
        }
        self.scatter_x = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.scatter_y = numeric
            .iter()
            .find(|name| **name != self.scatter_x)
            .cloned()
            .unwrap_or_default();
        self.scatter_fit = true;
        self.show_scatter_dialog = true;
    }

    /// Runs the scatterplot selected in the dialog.
    pub fn run_scatter_dialog(&mut self) {
        if self.scatter_x.is_empty() || self.scatter_y.is_empty() {
            self.set_error("Select the X and Y columns");
            return;
        }
        if self.scatter_x == self.scatter_y {
            self.set_error("The X and Y columns must be different");
            return;
        }
        let mut spec = AnalysisSpec::scatterplot(self.scatter_x.clone(), self.scatter_y.clone());
        if let AnalysisSpec::Scatterplot { fit, .. } = &mut spec {
            *fit = self.scatter_fit;
        }
        self.execute_spec(
            spec,
            format!("Scatterplot: {} vs {}", self.scatter_y, self.scatter_x),
        );
        self.show_scatter_dialog = false;
    }

    fn scatter_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_scatter_dialog {
            return;
        }
        let numeric: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_scatter_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Scatterplot"))
            .open(&mut open)
            .resizable(false)
            .default_width(410.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("X:");
                    egui::ComboBox::from_id_salt("scatter_x")
                        .selected_text(if self.scatter_x.is_empty() {
                            "—".to_owned()
                        } else {
                            self.scatter_x.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.scatter_x,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Y:");
                    egui::ComboBox::from_id_salt("scatter_y")
                        .selected_text(if self.scatter_y.is_empty() {
                            "—".to_owned()
                        } else {
                            self.scatter_y.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.scatter_y,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_checkbox(&mut self.scatter_fit, "Least-squares line");
                ui.small("Rows missing X or Y are removed pairwise.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_scatter_dialog = open && !run;
        if run {
            self.run_scatter_dialog();
        }
    }

    /// Appends descriptive statistics split by a group column.
    pub fn run_descriptive_statistics_by(&mut self, column: impl Into<String>, by: String) {
        let column = column.into();
        self.execute_spec(
            AnalysisSpec::descriptive_statistics_by(column.clone(), by.clone()),
            format!("Descriptive statistics: {column} by {by}"),
        );
    }

    /// Appends descriptive statistics for the selected column.
    pub fn run_descriptive_statistics(&mut self) {
        let Some(column) = self.selected.clone() else {
            self.set_error("Select a numeric column first");
            return;
        };
        self.execute_spec(
            AnalysisSpec::descriptive_statistics(column.clone()),
            format!("Descriptive statistics: {column}"),
        );
    }

    /// Runs the Anderson–Darling normality test on the selected column.
    pub fn run_normality_test(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let Some(column) = self.selected.clone() else {
            self.set_error("Select a numeric column first");
            return;
        };
        let spec = AnalysisSpec::normality_test(column.clone());
        match run_analysis(df, &spec) {
            Ok(out) => {
                for result in &out.results {
                    self.append_result(result);
                }
                self.status = format!("Normality test: {column}");
                self.error = None;
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    /// Runs the Ryan–Joiner normality test on the selected column.
    pub fn run_rj_test(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let Some(column) = self.selected.clone() else {
            self.set_error("Select a numeric column first");
            return;
        };
        let spec = AnalysisSpec::normality_test(column.clone())
            .with_normality_test(fp_core::workflow::NormalityTestKind::RyanJoiner);
        match run_analysis(df, &spec) {
            Ok(out) => {
                for result in &out.results {
                    self.append_result(result);
                }
                self.status = format!("Ryan–Joiner test: {column}");
                self.error = None;
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    /// Runs the Kolmogorov–Smirnov normality test on the selected column.
    /// Runs the D'Agostino–Pearson omnibus normality test.
    pub fn run_dagostino_test(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let Some(column) = self.selected.clone() else {
            self.set_error("Select a numeric column first");
            return;
        };
        let spec = AnalysisSpec::normality_test(column.clone())
            .with_normality_test(fp_core::workflow::NormalityTestKind::DAgostinoK2);
        match run_analysis(df, &spec) {
            Ok(out) => {
                for result in &out.results {
                    self.append_result(result);
                }
                self.status = format!("D'Agostino–Pearson test: {column}");
                self.error = None;
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    pub fn run_ks_test(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let Some(column) = self.selected.clone() else {
            self.set_error("Select a numeric column first");
            return;
        };
        let spec = AnalysisSpec::normality_test(column.clone())
            .with_normality_test(fp_core::workflow::NormalityTestKind::KolmogorovSmirnov);
        match run_analysis(df, &spec) {
            Ok(out) => {
                for result in &out.results {
                    self.append_result(result);
                }
                self.status = format!("Kolmogorov–Smirnov test: {column}");
                self.error = None;
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    /// Fits the four implemented distributions and appends the comparison table.
    pub fn run_distribution_comparison(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let Some(column) = self.selected.clone() else {
            self.set_error("Select a numeric column first");
            return;
        };
        let spec = AnalysisSpec::distribution_comparison(column.clone());
        match run_analysis(df, &spec) {
            Ok(out) => {
                for result in &out.results {
                    self.append_result(result);
                }
                self.status = format!("Distribution comparison: {column}");
                self.error = None;
            }
            Err(e) => self.set_error(e.to_string()),
        }
    }

    /// The catalogue of the interface for the current language.
    pub fn catalog(&self) -> UiCatalog {
        UiCatalog::new(self.language)
    }

    /// Current interface zoom factor (1.0 = 100 %).
    #[must_use]
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Current display format of the date columns.
    #[must_use]
    pub fn date_format(&self) -> DateFormat {
        self.date_format
    }

    fn display_cell(&self, df: &DataFrame, column: usize, row: usize) -> String {
        if let Some(source) = df.columns().get(column)
            && source.dtype() == DType::Date
        {
            return source.date(row).map_or_else(
                || "*".to_owned(),
                |days| fp_core::data::format_date_with(days, self.date_format),
            );
        }
        df.cell_text(column, row).unwrap_or_default()
    }

    /// Selects the language of the interface and of the reports.
    pub fn set_language(&mut self, language: fp_core::i18n::Lang) {
        self.language = language;
    }

    /// The language of the interface and of the reports.
    #[must_use]
    pub fn language(&self) -> fp_core::i18n::Lang {
        self.language
    }

    /// Current user options, in the form written to disk.
    #[must_use]
    pub fn preferences(&self) -> Preferences {
        Preferences {
            language: self.language.code().to_owned(),
            dark_mode: self.dark_mode,
            zoom: self.zoom,
            show_session: self.show_session,
            show_grid_lines: self.show_grid_lines,
            native_plot_windows: self.native_plot_windows,
            tour_seen: self.tour_seen,
            graph_overlay: self.graph_overlay.clone(),
            graph_overlay_title: self.graph_overlay_title.clone(),
            conditional_rules: self.conditional_rules.clone(),
            probability_distribution: Some(self.distribution.as_str().to_owned()),
            probability_display: Some(self.dialog_display.as_str().to_owned()),
            probability_layout: Some(self.dialog_layout.as_str().to_owned()),
            probability_same_x: self.dialog_same_x,
            probability_same_y: self.dialog_same_y,
            capability_distribution: Some(self.cap_distribution.as_str().to_owned()),
            capability_confidence: Some(self.cap_confidence),
            control_chart_kind: Some(
                match self.chart_kind {
                    fp_core::spc::ControlChartKind::IndividualsMovingRange => "imr",
                    fp_core::spc::ControlChartKind::XbarR => "xbar-r",
                    fp_core::spc::ControlChartKind::XbarS => "xbar-s",
                    fp_core::spc::ControlChartKind::Ewma => "ewma",
                    fp_core::spc::ControlChartKind::Cusum => "cusum",
                }
                .to_owned(),
            ),
            control_chart_subgroup: Some(self.chart_subgroup),
            control_chart_tests: Some(
                (1..=8u8)
                    .filter(|test| self.chart_tests.is_enabled(*test))
                    .map(|test| test.to_string())
                    .collect::<Vec<String>>()
                    .join(" "),
            ),
            control_chart_sensitivity: Some(
                (1..=8u8)
                    .map(|test| self.chart_tests.sensitivity(test).to_string())
                    .collect::<Vec<String>>()
                    .join(","),
            ),
            control_cusum_k: Some(self.chart_cusum_k),
            control_cusum_h: Some(self.chart_cusum_h),
            control_ewma_lambda: Some(self.chart_ewma_lambda),
            control_ewma_limit: Some(self.chart_ewma_limit),
            regression_confidence: Some(self.reg_confidence),
            anova_confidence: Some(self.anova_confidence),
            analysis_confidences: [
                ("probability", self.dialog_ci_level),
                ("t1", self.t_confidence),
                ("t2", self.test2_confidence),
                ("ecdf", self.ecdf_confidence),
                ("interval", self.interval_confidence),
                ("tolerance", self.tolerance_confidence),
                ("curve", self.curve_confidence),
                ("equivalence", self.equivalence_confidence),
                ("stepwise", self.step_confidence),
                ("kaplan", self.km_confidence),
                ("t2chart", self.t2_confidence),
            ]
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
            probability_confidence_band: self.dialog_ci,
            gage_design: Some(self.gage_design.name().to_owned()),
            alt_life: Some(
                match self.alt_life {
                    fp_core::alt::LifeModel::Weibull => "weibull",
                    fp_core::alt::LifeModel::Lognormal => "lognormal",
                    fp_core::alt::LifeModel::Exponential => "exponential",
                }
                .to_owned(),
            ),
            alt_acceleration: Some(
                match self.alt_acceleration {
                    fp_core::alt::AccelerationModel::Arrhenius => "arrhenius",
                    fp_core::alt::AccelerationModel::InversePower => "inverse-power",
                    fp_core::alt::AccelerationModel::Eyring => "eyring",
                }
                .to_owned(),
            ),
            alt_use_stress: Some(self.alt_use_stress),
            step_life: Some(
                match self.ss_life {
                    fp_core::alt::LifeModel::Weibull => "weibull",
                    fp_core::alt::LifeModel::Lognormal => "lognormal",
                    fp_core::alt::LifeModel::Exponential => "exponential",
                }
                .to_owned(),
            ),
            step_acceleration: Some(
                match self.ss_acceleration {
                    fp_core::alt::AccelerationModel::Arrhenius => "arrhenius",
                    fp_core::alt::AccelerationModel::InversePower => "inverse-power",
                    fp_core::alt::AccelerationModel::Eyring => "eyring",
                }
                .to_owned(),
            ),
            step_use_stress: Some(self.ss_use_stress),
            sampling_sample: Some(self.sampling_sample),
            sampling_accept: Some(self.sampling_accept),
            sampling_lot: Some(self.sampling_lot),
            sampling_aql: Some(self.sampling_aql),
            sampling_ltpd: Some(self.sampling_ltpd),
            sampling_alpha: Some(self.sampling_alpha),
            sampling_beta: Some(self.sampling_beta),
            doe_factors: Some(self.design_factors.clone()),
            doe_runs: Some(self.design_runs),
            doe_replicates: Some(self.design_replicates),
            doe_centers: Some(self.design_centers),
            doe_block_size: Some(self.design_block_size),
            doe_randomize: Some(self.design_randomize),
            doe_seed: Some(self.design_seed),
            taguchi_runs: Some(self.taguchi_runs),
            taguchi_design_factors: Some(self.taguchi_design_factors),
            pca_keep: Some(self.pca_keep),
            matrix_bins: Some(self.matrix_bins),
            timeseries_kind: Some(self.timeseries_kind.as_str().to_owned()),
            timeseries_window: Some(self.timeseries_window),
            timeseries_period: Some(self.timeseries_period),
            timeseries_model: Some(self.timeseries_model.name().to_owned()),
            timeseries_season: Some(self.timeseries_season),
            timeseries_horizon: Some(self.timeseries_horizon),
            step_max_steps: Some(self.step_max_steps),
            step_alpha_enter: Some(self.step_alpha_enter),
            step_alpha_remove: Some(self.step_alpha_remove),
            capability_subgroup: Some(self.cap_subgroup),
            capability_bootstrap: Some(self.cap_bootstrap),
            capability_box_cox: Some(self.cap_box_cox),
            capability_require_stability: Some(self.cap_require_stability),
            t_alternative: Some(self.t_alternative.as_str().to_owned()),
            t_pooled: Some(self.t_pooled),
            test2_method: Some(self.test2_method.as_str().to_owned()),
            test2_variance_method: Some(self.test2_variance_method.as_str().to_owned()),
            test2_alternative: Some(self.test2_alternative.as_str().to_owned()),
            rank_alternative: Some(self.rank_alternative.as_str().to_owned()),
            power_procedure: Some(self.power_procedure.as_str().to_owned()),
            power_alpha: Some(self.power_alpha),
            power_target: Some(self.power_target),
            power_alternative: Some(self.power_alternative.as_str().to_owned()),
            power_groups: Some(self.power_groups),
            curve_model: Some(match self.curve_model {
                CurveModel::Polynomial(degree) => format!("polynomial-{degree}"),
                CurveModel::Exponential => "exponential".to_owned(),
                CurveModel::Power => "power".to_owned(),
                CurveModel::Logistic => "logistic".to_owned(),
            }),
            scatter_fit: Some(self.scatter_fit),
            rsm_optimize: Some(self.rsm_optimize),
            rsm_contours: Some(self.rsm_contours),
            rsm_steepest_ascent: Some(self.rsm_steepest_ascent),
            doe_main_effects: Some(self.doe_main_effects),
            kmeans_clusters: Some(self.kmeans_clusters),
            kmeans_standardise: Some(self.kmeans_standardise),
            kmeans_seed: Some(self.kmeans_seed),
            attribute_kind: Some(
                match self.attr_kind {
                    AttributeChartKind::P => "p",
                    AttributeChartKind::Np => "np",
                    AttributeChartKind::C => "c",
                    AttributeChartKind::U => "u",
                    AttributeChartKind::LaneyP => "laney-p",
                    AttributeChartKind::LaneyNp => "laney-np",
                    AttributeChartKind::LaneyC => "laney-c",
                    AttributeChartKind::LaneyU => "laney-u",
                }
                .to_owned(),
            ),
            attribute_size: Some(self.attr_size.clone()),
            t2_subgroup: Some(self.t2_subgroup),
            tolerance_proportion: Some(self.tolerance_proportion),
            tolerance_free: Some(self.tolerance_free),
            diagnostic_alpha: Some(self.diagnostic_alpha),
            step_method: Some(
                match self.step_method {
                    StepMethod::Forward => "forward",
                    StepMethod::Backward => "backward",
                    StepMethod::Stepwise => "stepwise",
                }
                .to_owned(),
            ),
            step_criterion: Some(self.step_criterion.name().to_owned()),
            variance_center: Some(self.variance_center.as_str().to_owned()),
            two_way_interaction: Some(self.two_way_interaction),
            test2_success: Some(self.test2_success.clone()),
            equivalence_null: Some(self.equivalence_null.clone()),
            equivalence_margin: Some(self.equivalence_margin.clone()),
        }
    }

    /// Applies options read from disk, ignoring values out of range.
    pub fn apply_preferences(&mut self, preferences: &Preferences) {
        if let Some(language) = fp_core::i18n::Lang::parse(&preferences.language) {
            self.language = language;
        }
        self.dark_mode = preferences.dark_mode;
        if preferences.zoom.is_finite() {
            self.zoom = preferences.zoom.clamp(0.5, 2.5);
        }
        self.show_session = preferences.show_session;
        self.show_grid_lines = preferences.show_grid_lines;
        self.native_plot_windows = preferences.native_plot_windows;
        self.tour_seen = preferences.tour_seen;
        self.graph_overlay = preferences.graph_overlay.clone();
        self.graph_overlay_title = preferences.graph_overlay_title.clone();
        // A hand-edited file must not make the graph text unreadable.
        self.graph_overlay.font_scale = self.graph_overlay.font_scale.clamp(0.75, 1.5);
        self.conditional_rules = preferences.conditional_rules.clone();
        if let Some(code) = &preferences.probability_distribution {
            self.distribution = parse_distribution(code).unwrap_or(self.distribution);
            self.dialog_distribution = self.distribution;
        }
        if let Some(code) = &preferences.probability_display {
            self.dialog_display = match code.as_str() {
                "symbols" => fp_core::workflow::PlotDisplay::Symbols,
                "fit" => fp_core::workflow::PlotDisplay::Fit,
                _ => fp_core::workflow::PlotDisplay::Both,
            };
        }
        if let Some(code) = &preferences.probability_layout {
            self.dialog_layout = match code.as_str() {
                "overlay" => fp_core::workflow::PlotLayout::Overlay,
                _ => fp_core::workflow::PlotLayout::Separate,
            };
        }
        self.dialog_same_x = preferences.probability_same_x;
        self.dialog_same_y = preferences.probability_same_y;
        self.dialog_ci = preferences.probability_confidence_band;
        if let Some(level) = preferences.analysis_confidences.get("probability")
            && level.is_finite()
        {
            self.dialog_ci_level = *level;
        }
        if let Some(code) = &preferences.capability_distribution {
            self.cap_distribution = parse_distribution(code).unwrap_or(self.cap_distribution);
        }
        if let Some(confidence) = preferences.capability_confidence
            && confidence.is_finite()
            && (50.0..=99.99).contains(&confidence)
        {
            self.cap_confidence = confidence;
        }
        if let Some(code) = &preferences.gage_design
            && let Some(design) = fp_core::gage::GageDesign::parse(code)
        {
            self.gage_design = design;
        }
        if let Some(code) = &preferences.alt_life
            && let Some(model) = fp_core::alt::LifeModel::parse(code)
        {
            self.alt_life = model;
        }
        if let Some(code) = &preferences.alt_acceleration
            && let Some(model) = fp_core::alt::AccelerationModel::parse(code)
        {
            self.alt_acceleration = model;
        }
        if let Some(code) = &preferences.step_life
            && let Some(model) = fp_core::alt::LifeModel::parse(code)
        {
            self.ss_life = model;
        }
        if let Some(code) = &preferences.step_acceleration
            && let Some(model) = fp_core::alt::AccelerationModel::parse(code)
        {
            self.ss_acceleration = model;
        }
        if let Some(value) = preferences.alt_use_stress
            && value.is_finite()
        {
            self.alt_use_stress = value;
        }
        if let Some(value) = preferences.step_use_stress
            && value.is_finite()
        {
            self.ss_use_stress = value;
        }
        if let Some(value) = preferences.sampling_sample
            && (1..=5000).contains(&value)
        {
            self.sampling_sample = value;
        }
        if let Some(value) = preferences.sampling_accept
            && value <= 500
        {
            self.sampling_accept = value;
        }
        if let Some(value) = preferences.sampling_lot
            && value <= 10_000_000
        {
            self.sampling_lot = value;
        }
        for (slot, value) in [
            (&mut self.sampling_aql, preferences.sampling_aql),
            (&mut self.sampling_ltpd, preferences.sampling_ltpd),
            (&mut self.sampling_alpha, preferences.sampling_alpha),
            (&mut self.sampling_beta, preferences.sampling_beta),
        ] {
            if let Some(value) = value
                && value.is_finite()
            {
                *slot = value;
            }
        }
        if let Some(code) = &preferences.step_method
            && let Some(method) = fp_core::stepwise::StepMethod::parse(code)
        {
            self.step_method = method;
        }
        if let Some(code) = &preferences.step_criterion
            && let Some(criterion) = fp_core::stepwise::StepCriterion::parse(code)
        {
            self.step_criterion = criterion;
        }
        if let Some(code) = &preferences.variance_center
            && let Some(center) = fp_core::variance::VarianceCenter::parse(code)
        {
            self.variance_center = center;
        }
        if let Some(value) = preferences.two_way_interaction {
            self.two_way_interaction = value;
        }
        if let Some(value) = &preferences.test2_success {
            let trimmed = value.trim();
            if trimmed == "0" || trimmed == "1" {
                self.test2_success = trimmed.to_owned();
            }
        }
        for (slot, value) in [
            (&mut self.equivalence_null, &preferences.equivalence_null),
            (
                &mut self.equivalence_margin,
                &preferences.equivalence_margin,
            ),
        ] {
            if let Some(value) = value
                && value.trim().parse::<f64>().is_ok_and(f64::is_finite)
            {
                *slot = value.clone();
            }
        }
        if let Some(value) = preferences.capability_subgroup
            && (1..=100).contains(&value)
        {
            self.cap_subgroup = value;
        }
        if let Some(value) = preferences.capability_bootstrap
            && value <= 2000
        {
            self.cap_bootstrap = value;
        }
        if let Some(value) = preferences.capability_box_cox {
            self.cap_box_cox = value;
        }
        if let Some(value) = preferences.capability_require_stability {
            self.cap_require_stability = value;
        }
        if let Some(code) = &preferences.t_alternative
            && let Some(alternative) = fp_core::hypothesis::Alternative::parse(code)
        {
            self.t_alternative = alternative;
        }
        if let Some(value) = preferences.t_pooled {
            self.t_pooled = value;
        }
        if let Some(code) = &preferences.test2_method
            && let Some(method) = fp_core::proportion::ProportionMethod::parse(code)
        {
            self.test2_method = method;
        }
        if let Some(code) = &preferences.test2_variance_method
            && let Some(method) = fp_core::variance::VarianceMethod::parse(code)
        {
            self.test2_variance_method = method;
        }
        if let Some(code) = &preferences.test2_alternative
            && let Some(alternative) = fp_core::hypothesis::Alternative::parse(code)
        {
            self.test2_alternative = alternative;
        }
        if let Some(code) = &preferences.rank_alternative
            && let Some(alternative) = fp_core::hypothesis::Alternative::parse(code)
        {
            self.rank_alternative = alternative;
        }
        if let Some(code) = &preferences.power_procedure
            && let Some(procedure) = fp_core::power::PowerProcedure::parse(code)
        {
            self.power_procedure = procedure;
        }
        for (slot, value, low, high) in [
            (&mut self.power_alpha, preferences.power_alpha, 0.1, 20.0),
            (
                &mut self.diagnostic_alpha,
                preferences.diagnostic_alpha,
                0.1,
                50.0,
            ),
            (
                &mut self.tolerance_proportion,
                preferences.tolerance_proportion,
                50.0,
                99.9,
            ),
        ] {
            if let Some(value) = value
                && value.is_finite()
                && (low..=high).contains(&value)
            {
                *slot = value;
            }
        }
        if let Some(value) = preferences.power_target
            && value.is_finite()
            && (50.0..=99.9).contains(&value)
        {
            self.power_target = value;
        }
        if let Some(code) = &preferences.power_alternative
            && let Some(alternative) = fp_core::hypothesis::Alternative::parse(code)
        {
            self.power_alternative = alternative;
        }
        if let Some(value) = preferences.power_groups
            && (2..=20).contains(&value)
        {
            self.power_groups = value;
        }
        if let Some(code) = &preferences.curve_model
            && let Some(model) = fp_core::curvefit::CurveModel::parse(code)
        {
            self.curve_model = model;
        }
        for (slot, value) in [
            (&mut self.scatter_fit, preferences.scatter_fit),
            (&mut self.rsm_optimize, preferences.rsm_optimize),
            (&mut self.rsm_contours, preferences.rsm_contours),
            (
                &mut self.rsm_steepest_ascent,
                preferences.rsm_steepest_ascent,
            ),
            (&mut self.doe_main_effects, preferences.doe_main_effects),
            (&mut self.kmeans_standardise, preferences.kmeans_standardise),
            (&mut self.tolerance_free, preferences.tolerance_free),
        ] {
            if let Some(value) = value {
                *slot = value;
            }
        }
        if let Some(value) = preferences.kmeans_clusters
            && (2..=20).contains(&value)
        {
            self.kmeans_clusters = value;
        }
        if let Some(value) = preferences.kmeans_seed {
            self.kmeans_seed = value;
        }
        if let Some(code) = &preferences.attribute_kind
            && let Some(kind) = AttributeChartKind::parse(code)
        {
            self.attr_kind = kind;
        }
        if let Some(value) = &preferences.attribute_size
            && !value.trim().is_empty()
        {
            self.attr_size = value.clone();
        }
        if let Some(value) = preferences.t2_subgroup
            && (2..=25).contains(&value)
        {
            self.t2_subgroup = value;
        }
        if let Some(factors) = &preferences.doe_factors
            && !factors.trim().is_empty()
        {
            self.design_factors = factors.clone();
        }
        if let Some(value) = preferences.doe_runs
            && (8..=48).contains(&value)
        {
            self.design_runs = value;
        }
        if let Some(value) = preferences.doe_replicates
            && (1..=100).contains(&value)
        {
            self.design_replicates = value;
        }
        if let Some(value) = preferences.doe_centers
            && value <= 100
        {
            self.design_centers = value;
        }
        if let Some(value) = preferences.doe_block_size
            && value <= 100
        {
            self.design_block_size = value;
        }
        if let Some(value) = preferences.doe_randomize {
            self.design_randomize = value;
        }
        if let Some(value) = preferences.doe_seed {
            self.design_seed = value;
        }
        if let Some(value) = preferences.taguchi_runs
            && matches!(value, 8 | 9 | 12 | 16 | 18 | 27)
        {
            self.taguchi_runs = value;
        }
        if let Some(value) = preferences.taguchi_design_factors
            && (1..=15).contains(&value)
        {
            self.taguchi_design_factors = value;
        }
        if let Some(value) = preferences.pca_keep
            && (1..=10).contains(&value)
        {
            self.pca_keep = value;
        }
        if let Some(value) = preferences.matrix_bins
            && (3..=30).contains(&value)
        {
            self.matrix_bins = value;
        }
        if let Some(code) = &preferences.timeseries_kind
            && let Some(kind) = TimeSeriesKind::parse(code)
        {
            self.timeseries_kind = kind;
        }
        if let Some(value) = preferences.timeseries_window
            && (2..=100).contains(&value)
        {
            self.timeseries_window = value;
        }
        if let Some(value) = preferences.timeseries_period
            && (2..=365).contains(&value)
        {
            self.timeseries_period = value;
        }
        if let Some(code) = &preferences.timeseries_model
            && let Some(model) = fp_core::timeseries::DecompositionModel::parse(code)
        {
            self.timeseries_model = model;
        }
        if let Some(value) = preferences.timeseries_season
            && (1..=365).contains(&value)
        {
            self.timeseries_season = value;
        }
        if let Some(value) = preferences.timeseries_horizon
            && (1..=100).contains(&value)
        {
            self.timeseries_horizon = value;
        }
        if let Some(value) = preferences.step_max_steps
            && (1..=500).contains(&value)
        {
            self.step_max_steps = value;
        }
        for (slot, value) in [
            (&mut self.step_alpha_enter, preferences.step_alpha_enter),
            (&mut self.step_alpha_remove, preferences.step_alpha_remove),
        ] {
            if let Some(value) = value
                && value.is_finite()
                && (0.001..=0.5).contains(&value)
            {
                *slot = value;
            }
        }
        if let Some(code) = &preferences.control_chart_kind
            && let Some(kind) = fp_core::spc::ControlChartKind::parse(code)
        {
            self.chart_kind = kind;
        }
        if let Some(subgroup) = preferences.control_chart_subgroup
            && (1..=25).contains(&subgroup)
        {
            self.chart_subgroup = subgroup;
        }
        if let Some(list) = &preferences.control_chart_tests {
            if list.trim().is_empty() {
                self.chart_tests = fp_core::spc::TestSet::none();
            } else if let Ok(mut tests) = fp_core::spc::TestSet::parse_list(list) {
                if let Some(values) = &preferences.control_chart_sensitivity {
                    for (index, value) in values.split(',').enumerate() {
                        if let (Ok(test), Ok(k)) =
                            (u8::try_from(index + 1), value.trim().parse::<u8>())
                            && let Ok(updated) = tests.with_sensitivity(test, k)
                        {
                            tests = updated;
                        }
                    }
                }
                self.chart_tests = tests;
            }
        }
        for (slot, value) in [
            (&mut self.chart_cusum_k, preferences.control_cusum_k),
            (&mut self.chart_cusum_h, preferences.control_cusum_h),
            (&mut self.chart_ewma_lambda, preferences.control_ewma_lambda),
            (&mut self.chart_ewma_limit, preferences.control_ewma_limit),
            (&mut self.reg_confidence, preferences.regression_confidence),
            (&mut self.anova_confidence, preferences.anova_confidence),
            (
                &mut self.t_confidence,
                preferences.analysis_confidences.get("t1").copied(),
            ),
            (
                &mut self.test2_confidence,
                preferences.analysis_confidences.get("t2").copied(),
            ),
            (
                &mut self.ecdf_confidence,
                preferences.analysis_confidences.get("ecdf").copied(),
            ),
            (
                &mut self.interval_confidence,
                preferences.analysis_confidences.get("interval").copied(),
            ),
            (
                &mut self.tolerance_confidence,
                preferences.analysis_confidences.get("tolerance").copied(),
            ),
            (
                &mut self.curve_confidence,
                preferences.analysis_confidences.get("curve").copied(),
            ),
            (
                &mut self.equivalence_confidence,
                preferences.analysis_confidences.get("equivalence").copied(),
            ),
            (
                &mut self.step_confidence,
                preferences.analysis_confidences.get("stepwise").copied(),
            ),
            (
                &mut self.km_confidence,
                preferences.analysis_confidences.get("kaplan").copied(),
            ),
            (
                &mut self.t2_confidence,
                preferences.analysis_confidences.get("t2chart").copied(),
            ),
        ] {
            if let Some(value) = value
                && value.is_finite()
            {
                *slot = value;
            }
        }
    }

    /// Sets the file that keeps the preferences and loads it if it exists.
    ///
    /// A missing or malformed file is not an error: the defaults stay and the
    /// next save replaces it.
    pub fn set_preferences_path(&mut self, path: PathBuf) {
        let loaded = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Preferences>(&text).ok());
        if let Some(preferences) = &loaded {
            self.apply_preferences(preferences);
        }
        self.saved_preferences = Some(self.preferences());
        self.recovery_path = path
            .parent()
            .map(|directory| directory.join("recovery.fpp"));
        self.preferences_path = Some(path);
    }

    /// File that keeps the preferences, when the desktop resolved one.
    #[must_use]
    pub fn preferences_path(&self) -> Option<&std::path::Path> {
        self.preferences_path.as_deref()
    }

    /// Last failure writing the preferences, if any.
    #[must_use]
    pub fn preferences_error(&self) -> Option<&str> {
        self.preferences_error.as_deref()
    }

    /// Writes the preferences when they changed since the last save.
    fn persist_preferences(&mut self) {
        let Some(path) = self.preferences_path.clone() else {
            return;
        };
        let current = self.preferences();
        if self.saved_preferences.as_ref() == Some(&current) {
            return;
        }
        if let Some(parent) = path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            self.preferences_error = Some(format!("{}: {error}", parent.display()));
            return;
        }
        match serde_json::to_string_pretty(&current)
            .map_err(|error| error.to_string())
            .and_then(|text| std::fs::write(&path, text).map_err(|error| error.to_string()))
        {
            Ok(()) => {
                self.saved_preferences = Some(current);
                self.preferences_error = None;
            }
            Err(error) => {
                self.preferences_error = Some(format!("{}: {error}", path.display()));
            }
        }
    }

    /// Closes every open dialog (Escape, or the automation bridge).
    ///
    /// The native open/save requests are left alone: they belong to the
    /// desktop runner, not to an egui dialog.
    pub fn close_dialogs(&mut self) {
        self.show_about = false;
        self.show_assistant_dialog = false;
        self.show_shortcuts = false;
        self.show_agreement_dialog = false;
        self.show_alt_dialog = false;
        self.show_anova_dialog = false;
        self.show_attr_dialog = false;
        self.show_bb_dialog = false;
        self.show_box_cox_dialog = false;
        self.show_calc_dialog = false;
        self.show_cap_dialog = false;
        self.show_chart_dialog = false;
        self.show_csv_options = false;
        self.show_correlation_dialog = false;
        self.show_crosstab_dialog = false;
        self.show_curve_dialog = false;
        self.show_describe_dialog = false;
        self.show_date_diff_dialog = false;
        self.show_date_shift_dialog = false;
        self.show_design_dialog = false;
        self.show_desirability_dialog = false;
        self.show_diagnostic_dialog = false;
        self.show_batch_dialog = false;
        self.show_conditional_dialog = false;
        self.show_doe_dialog = false;
        self.show_dot_dialog = false;
        self.show_ecdf_dialog = false;
        self.show_equivalence_dialog = false;
        self.show_exp_dialog = false;
        self.show_g_chart_dialog = false;
        self.show_gage_dialog = false;
        self.show_graph_annotation = false;
        self.show_graph_labels = false;
        self.show_graph_text = false;
        self.show_reference_line = false;
        self.show_format_dialog = false;
        self.show_interval_dialog = false;
        self.show_km_dialog = false;
        self.show_kmeans_dialog = false;
        self.show_linearity_dialog = false;
        self.show_log_dialog = false;
        self.show_logistic_dialog = false;
        self.show_matrix_dialog = false;
        self.show_ordinal_dialog = false;
        self.show_pareto_dialog = false;
        self.show_partial_dialog = false;
        self.show_pb_dialog = false;
        self.show_pca_dialog = false;
        self.show_plot = false;
        self.show_poisson_dialog = false;
        self.show_power_dialog = false;
        self.show_probability_plot_dialog = false;
        self.show_rank_dialog = false;
        self.show_reg_dialog = false;
        self.show_rename = false;
        self.show_rsm_dialog = false;
        self.show_sampling_dialog = false;
        self.show_scatter_dialog = false;
        self.show_series_dialog = false;
        self.show_sheet_dialog = false;
        self.show_sort_dialog = false;
        self.show_stack_dialog = false;
        self.show_stem_dialog = false;
        self.show_step_dialog = false;
        self.show_subset_dialog = false;
        self.show_t_dialog = false;
        self.show_t2_dialog = false;
        self.show_test2_dialog = false;
        self.show_type1_dialog = false;
        self.show_taguchi_design_dialog = false;
        self.show_taguchi_dialog = false;
        self.show_timeseries_dialog = false;
        self.show_tolerance_dialog = false;
        self.show_two_way_dialog = false;
        self.show_step_stress_dialog = false;
        self.show_unstack_dialog = false;
        self.show_weibull_dialog = false;
    }

    /// Opens a file dropped on the window, choosing the reader by extension.
    pub fn open_dropped(&mut self, path: &str) -> bool {
        let lower = path.to_ascii_lowercase();
        let kind = if lower.ends_with(".csv") || lower.ends_with(".txt") {
            FileKind::Csv
        } else if lower.ends_with(".xlsx") || lower.ends_with(".xls") {
            FileKind::Xlsx
        } else if lower.ends_with(".fpp") {
            FileKind::Project
        } else {
            return false;
        };
        if kind == FileKind::Csv {
            self.preview_request = Some(path.to_owned());
        } else {
            self.open_request = Some(OpenRequest {
                path: path.to_owned(),
                kind,
                sheet: None,
                csv_options: None,
            });
        }
        self.status = format!("Opening {path}");
        true
    }

    /// Replaces every match in the active sheet as one undo group.
    pub fn replace_all(&mut self) {
        let cells = {
            let Some(df) = self.data.as_ref() else {
                return;
            };
            let mut cells = Vec::new();
            for row in 0..df.n_rows() {
                for column in 0..df.n_columns() {
                    let text = df.cell_text(column, row).unwrap_or_default();
                    if let Some(value) = replace_in(
                        &text,
                        &self.find_text,
                        &self.replace_text,
                        self.find_match_case,
                        self.find_whole_cell,
                    ) {
                        cells.push((column, row, value));
                    }
                }
            }
            cells
        };
        if cells.is_empty() {
            self.find_message = "No matches".to_owned();
            return;
        }
        let count = cells.len();
        if self.set_cells(&cells) {
            self.find_message = format!("{count} replaced");
            self.status = format!("{count} cells replaced");
        }
    }

    /// Copies the first row of the selection down over the rest (Ctrl+D).
    pub fn fill_down(&mut self) {
        let Some(df) = self.data.as_ref() else {
            return;
        };
        let Some((c0, r0, c1, r1)) = self.selection_bounds() else {
            return;
        };
        if r1 <= r0 {
            return;
        }
        let last_column = c1.min(df.n_columns().saturating_sub(1));
        let mut cells = Vec::new();
        for column in c0..=last_column {
            let value = cell_state(df, column, r0).unwrap_or_default();
            for row in r0 + 1..=r1 {
                cells.push((column, row, value.clone()));
            }
        }
        if self.set_cells(&cells) {
            self.status = "Filled down".to_owned();
        }
    }

    /// Copies the first column of the selection right over the rest (Ctrl+R).
    pub fn fill_right(&mut self) {
        let Some(df) = self.data.as_ref() else {
            return;
        };
        let Some((c0, r0, c1, r1)) = self.selection_bounds() else {
            return;
        };
        if c1 <= c0 {
            return;
        }
        let last_row = r1.min(df.n_rows().saturating_sub(1));
        let last_column = c1.min(df.n_columns().saturating_sub(1));
        let mut cells = Vec::new();
        for row in r0..=last_row {
            let value = cell_state(df, c0, row).unwrap_or_default();
            for column in c0 + 1..=last_column {
                cells.push((column, row, value.clone()));
            }
        }
        if self.set_cells(&cells) {
            self.status = "Filled right".to_owned();
        }
    }

    /// Applies the Format Cells options to the selected column.
    pub fn apply_column_format(&mut self) {
        let Some(name) = self.selected.clone() else {
            return;
        };
        self.formats.insert(
            name,
            ColumnFormat {
                decimals: self.format_decimals,
                thousands: self.format_thousands,
                bold: self.format_bold,
                align: self.format_align,
            },
        );
        self.status = "Column format applied".to_owned();
    }

    /// First conditional rule that matches a cell, if any.
    #[must_use]
    pub fn conditional_style(
        &self,
        df: &fp_core::data::DataFrame,
        column: usize,
        row: usize,
    ) -> Option<ConditionalStyle> {
        if self.conditional_rules.is_empty() {
            return None;
        }
        let name = df.columns().get(column)?.name();
        let text = df.cell_text(column, row).unwrap_or_default();
        let value = df
            .columns()
            .get(column)
            .and_then(|column| column.value(row));
        self.conditional_rules
            .iter()
            .find(|rule| rule.column == name && rule.matches(value, &text))
            .map(|rule| rule.style)
    }

    /// Removes the display format of the selected column.
    pub fn clear_column_format(&mut self) {
        if let Some(name) = self.selected.clone() {
            self.formats.remove(&name);
            self.status = "Column format cleared".to_owned();
        }
    }

    /// Writes `path`, asking first when it already exists.
    pub fn request_write(&mut self, path: String, kind: FileKind, decimal_comma: bool) {
        if std::path::Path::new(&path).exists() {
            self.overwrite_pending = Some((path, kind, decimal_comma));
            self.show_overwrite = true;
            return;
        }
        self.save_request = Some(SaveRequest {
            path,
            kind,
            decimal_comma,
        });
    }

    /// Emits the save request held by the overwrite dialog.
    pub fn confirm_overwrite(&mut self) {
        if let Some((path, kind, decimal_comma)) = self.overwrite_pending.take() {
            self.save_request = Some(SaveRequest {
                path,
                kind,
                decimal_comma,
            });
        }
        self.show_overwrite = false;
    }
}

fn add_column_action(df: &DataFrame, index: usize) -> EditAction {
    let column = &df.columns()[index];
    EditAction::AddColumn {
        index,
        name: column.name().to_owned(),
        dtype: column.dtype(),
    }
}

/// Restores the cells written so far and drops the rows a block added.
fn rollback_cells(df: &mut DataFrame, edits: &[CellEdit], before_rows: usize) {
    let group = EditAction::Cells(edits.to_vec());
    let _ = undo_action(df, &group);
    df.truncate_rows(before_rows);
}

fn undo_action(df: &mut DataFrame, action: &EditAction) -> Result<(), String> {
    match action {
        EditAction::Cells(edits) => {
            for edit in edits.iter().rev() {
                match &edit.before {
                    Some(text) => df
                        .set_cell(edit.column, edit.row, text)
                        .map_err(|e| e.to_string())?,
                    None => df
                        .set_missing(edit.column, edit.row)
                        .map_err(|e| e.to_string())?,
                }
            }
            let rows = edits
                .iter()
                .map(|edit| edit.before_rows)
                .min()
                .unwrap_or(df.n_rows());
            df.truncate_rows(rows);
        }
        EditAction::InsertRow { row } => {
            df.remove_row(*row).map_err(|e| e.to_string())?;
        }
        EditAction::RemoveRow { row, cells } => {
            df.insert_row(*row).map_err(|e| e.to_string())?;
            restore_row(df, *row, cells)?;
        }
        EditAction::AddColumn { index, .. } => {
            df.remove_column(*index).map_err(|e| e.to_string())?;
        }
        EditAction::RemoveColumn { index, column } => {
            df.insert_column(*index, column.clone())
                .map_err(|e| e.to_string())?;
        }
        EditAction::RenameColumn { index, before, .. } => {
            df.rename_column(*index, before)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn redo_action(df: &mut DataFrame, action: &EditAction) -> Result<(), String> {
    match action {
        EditAction::Cells(edits) => {
            for edit in edits {
                if edit.row >= df.n_rows() {
                    df.ensure_rows(edit.row + 1).map_err(|e| e.to_string())?;
                }
                match &edit.after {
                    Some(text) => df
                        .set_cell(edit.column, edit.row, text)
                        .map_err(|e| e.to_string())?,
                    None => df
                        .set_missing(edit.column, edit.row)
                        .map_err(|e| e.to_string())?,
                }
            }
        }
        EditAction::InsertRow { row } => {
            df.insert_row(*row).map_err(|e| e.to_string())?;
        }
        EditAction::RemoveRow { row, .. } => {
            df.remove_row(*row).map_err(|e| e.to_string())?;
        }
        EditAction::AddColumn { index, name, dtype } => {
            df.insert_empty_column(*index, name.clone(), *dtype)
                .map_err(|e| e.to_string())?;
        }
        EditAction::RemoveColumn { index, .. } => {
            df.remove_column(*index).map_err(|e| e.to_string())?;
        }
        EditAction::RenameColumn { index, after, .. } => {
            df.rename_column(*index, after).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn restore_row(df: &mut DataFrame, row: usize, cells: &[Option<String>]) -> Result<(), String> {
    for (column, value) in cells.iter().enumerate() {
        match value {
            Some(text) => df.set_cell(column, row, text).map_err(|e| e.to_string())?,
            None => df.set_missing(column, row).map_err(|e| e.to_string())?,
        }
    }
    Ok(())
}

fn group_target(group: &EditGroup) -> Option<(usize, usize)> {
    group.actions.iter().rev().find_map(|action| match action {
        EditAction::Cells(edits) => edits.last().map(|edit| (edit.column, edit.row)),
        EditAction::InsertRow { row } | EditAction::RemoveRow { row, .. } => Some((0, *row)),
        EditAction::AddColumn { index, .. }
        | EditAction::RemoveColumn { index, .. }
        | EditAction::RenameColumn { index, .. } => Some((*index, 0)),
    })
}

/// Applies a column format to a cell text (numbers only).
fn format_cell_text(text: String, format: Option<ColumnFormat>) -> String {
    let Some(format) = format else {
        return text;
    };
    let Ok(value) = text.parse::<f64>() else {
        return text;
    };
    let mut out = format!("{value:.*}", format.decimals);
    if format.thousands {
        out = group_thousands(&out);
    }
    out
}

/// Groups the integer part of a decimal string in threes.
fn group_thousands(text: &str) -> String {
    let (sign, rest) = match text.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", text),
    };
    let (integer, fraction) = match rest.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (rest, None),
    };
    let mut grouped = String::with_capacity(integer.len() + integer.len() / 3);
    for (index, digit) in integer.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            grouped.push(' ');
        }
        grouped.push(digit);
    }
    let integer: String = grouped.chars().rev().collect();
    match fraction {
        Some(fraction) => format!("{sign}{integer}.{fraction}"),
        None => format!("{sign}{integer}"),
    }
}

/// Whether a cell text matches the find options.
fn text_matches(text: &str, needle: &str, match_case: bool, whole_cell: bool) -> bool {
    if needle.is_empty() {
        return false;
    }
    if match_case {
        if whole_cell {
            text == needle
        } else {
            text.contains(needle)
        }
    } else {
        let text = text.to_lowercase();
        let needle = needle.to_lowercase();
        if whole_cell {
            text == needle
        } else {
            text.contains(&needle)
        }
    }
}

/// Replaces every match of `needle`; `None` when the text does not match.
fn replace_in(
    text: &str,
    needle: &str,
    replacement: &str,
    match_case: bool,
    whole_cell: bool,
) -> Option<String> {
    if !text_matches(text, needle, match_case, whole_cell) {
        return None;
    }
    if whole_cell {
        return Some(replacement.to_owned());
    }
    if match_case {
        return Some(text.replace(needle, replacement));
    }
    let lower = text.to_lowercase();
    let needle = needle.to_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while let Some(position) = lower[index..].find(&needle) {
        let start = index + position;
        out.push_str(&text[index..start]);
        out.push_str(replacement);
        index = start + needle.len();
    }
    out.push_str(&text[index..]);
    Some(out)
}

fn cell_state(df: &DataFrame, column: usize, row: usize) -> Option<String> {
    if column >= df.n_columns() || row >= df.n_rows() || df.cell_is_missing(column, row) {
        None
    } else {
        df.cell_text(column, row)
    }
}

fn fmt_f64(value: f64) -> String {
    if value != 0.0 && !(1e-4..1e6).contains(&value.abs()) {
        return format!("{value:.4e}");
    }
    let mut s = format!("{value:.6}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" {
        s = "0".to_owned();
    }
    s
}

/// Builds the native-dialog request for a file kind.
fn browse_request(kind: FileKind, save: bool) -> BrowseRequest {
    let (title, filters) = match (kind, save) {
        (FileKind::Csv, false) => (
            "Open CSV",
            vec![
                "CSV files (*.csv)|*.csv".to_owned(),
                "Text files (*.txt)|*.txt".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Csv, true) => (
            "Save CSV",
            vec![
                "CSV files (*.csv)|*.csv".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Project, false) => (
            "Open FreePlots project",
            vec![
                "FreePlots project (*.fpp)|*.fpp".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Project, true) => (
            "Save FreePlots project",
            vec![
                "FreePlots project (*.fpp)|*.fpp".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Html, false) => (
            "Open HTML",
            vec![
                "HTML files (*.html)|*.html;*.htm".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Html, true) => (
            "Export Session HTML",
            vec![
                "HTML files (*.html)|*.html".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Pdf, false) => (
            "Open PDF",
            vec![
                "PDF files (*.pdf)|*.pdf".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Xlsx, false) => (
            "Open Excel workbook",
            vec![
                "Excel workbooks (*.xlsx)|*.xlsx".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Xlsx, true) => (
            "Save Excel workbook",
            vec![
                "Excel workbooks (*.xlsx)|*.xlsx".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Pdf, true) => (
            "Export Session PDF",
            vec![
                "PDF files (*.pdf)|*.pdf".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Svg, true) => (
            "Save Graph",
            vec![
                "SVG files (*.svg)|*.svg".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Svg, false) => (
            "Open SVG",
            vec![
                "SVG files (*.svg)|*.svg".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::GraphPdf, true) => (
            "Print Graph",
            vec![
                "PDF files (*.pdf)|*.pdf".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::GraphPdf, false) => (
            "Open PDF",
            vec![
                "PDF files (*.pdf)|*.pdf".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Png, true) => (
            "Save Graph as PNG",
            vec![
                "PNG images (*.png)|*.png".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Png, false) => (
            "Open PNG",
            vec![
                "PNG images (*.png)|*.png".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Jpeg, true) => (
            "Save Graph as JPEG",
            vec![
                "JPEG images (*.jpg;*.jpeg)|*.jpg;*.jpeg".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
        (FileKind::Jpeg, false) => (
            "Open JPEG",
            vec![
                "JPEG images (*.jpg;*.jpeg)|*.jpg;*.jpeg".to_owned(),
                "All files (*.*)|*.*".to_owned(),
            ],
        ),
    };
    BrowseRequest {
        title: title.to_owned(),
        filters,
        save,
    }
}

/// A bar menu that explains itself on hover.
///
/// Submenus get the `▶` arrow from the bundled font instead of egui's `⏵`,
/// which DejaVu Sans does not carry (it rendered as a square).
fn help_menu_button<R>(
    ui: &mut egui::Ui,
    text: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::Response {
    let color = ui.visuals().text_color();
    if egui::containers::menu::is_in_menu(ui) {
        let button = egui::Button::new(egui::RichText::new(text).color(color)).right_text("▶");
        let (response, _) =
            egui::containers::menu::SubMenuButton::from_button(button).ui(ui, add_contents);
        response.on_hover_text(format!("Open the {text} menu"))
    } else {
        ui.menu_button(egui::RichText::new(text).color(color), add_contents)
            .response
            .on_hover_text(format!("Open the {text} menu"))
    }
}

/// Toolbar and menu icons painted by egui itself (no external assets).
#[derive(Clone, Copy)]
enum ToolIcon {
    Open,
    Save,
    New,
    Export,
    Undo,
    Redo,
    Plot,
    Clear,
    Pencil,
    Calculator,
    Grid,
    Plus,
    Minus,
    Sort,
    Swap,
    Layers,
    Filter,
    Box,
    Bars,
    Dots,
    Info,
    Globe,
    Plug,
    Sigma,
    Clock,
    Copy,
    Cut,
    Paste,
}

fn paint_icon(painter: &egui::Painter, c: egui::Pos2, icon: ToolIcon, stroke: egui::Stroke) {
    match icon {
        ToolIcon::Open => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 8.0, c.y - 4.0),
                    egui::pos2(c.x + 8.0, c.y + 6.0),
                ),
                1.5,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 8.0, c.y - 4.0),
                    egui::pos2(c.x - 2.0, c.y - 4.0),
                ],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - 2.0, c.y - 4.0), egui::pos2(c.x, c.y - 6.5)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x, c.y - 6.5), egui::pos2(c.x + 6.0, c.y - 6.5)],
                stroke,
            );
        }
        ToolIcon::Save => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 7.0, c.y - 7.0),
                    egui::pos2(c.x + 7.0, c.y + 7.0),
                ),
                1.5,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 3.0, c.y - 7.0),
                    egui::pos2(c.x + 4.0, c.y - 2.0),
                ),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 4.0, c.y + 2.0),
                    egui::pos2(c.x + 4.0, c.y + 2.0),
                ],
                stroke,
            );
        }
        ToolIcon::New => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 5.0, c.y - 7.0),
                    egui::pos2(c.x + 5.0, c.y + 7.0),
                ),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 1.0, c.y - 7.0),
                    egui::pos2(c.x + 5.0, c.y - 3.0),
                ],
                stroke,
            );
        }
        ToolIcon::Export => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 7.0, c.y - 7.0),
                    egui::pos2(c.x + 2.0, c.y + 7.0),
                ),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment([egui::pos2(c.x, c.y), egui::pos2(c.x + 8.0, c.y)], stroke);
            painter.line_segment(
                [egui::pos2(c.x + 4.0, c.y - 3.0), egui::pos2(c.x + 8.0, c.y)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x + 4.0, c.y + 3.0), egui::pos2(c.x + 8.0, c.y)],
                stroke,
            );
        }
        ToolIcon::Undo | ToolIcon::Redo => {
            let direction = if matches!(icon, ToolIcon::Undo) {
                -1.0
            } else {
                1.0
            };
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x + direction * 6.0, c.y + 5.0),
                    egui::pos2(c.x + direction * 1.0, c.y - 5.0),
                    egui::pos2(c.x - direction * 6.0, c.y + 5.0),
                ],
                stroke,
            ));
            painter.line_segment(
                [
                    egui::pos2(c.x - direction * 6.0, c.y + 5.0),
                    egui::pos2(c.x - direction * 6.0, c.y - 2.0),
                ],
                stroke,
            );
        }
        ToolIcon::Plot => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 7.0, c.y + 7.0),
                    egui::pos2(c.x + 7.0, c.y + 7.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 7.0, c.y + 7.0),
                    egui::pos2(c.x - 7.0, c.y - 7.0),
                ],
                stroke,
            );
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - 4.0, c.y + 4.0),
                    egui::pos2(c.x - 1.0, c.y - 1.0),
                    egui::pos2(c.x + 2.0, c.y + 1.0),
                    egui::pos2(c.x + 6.0, c.y - 6.0),
                ],
                stroke,
            ));
        }
        ToolIcon::Copy => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 7.0, c.y - 6.0),
                    egui::pos2(c.x + 2.0, c.y + 5.0),
                ),
                1.5,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 2.0, c.y - 2.0),
                    egui::pos2(c.x + 7.0, c.y + 6.0),
                ),
                1.5,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        ToolIcon::Cut => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 6.0, c.y - 7.0),
                    egui::pos2(c.x + 5.0, c.y + 5.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 6.0, c.y - 7.0),
                    egui::pos2(c.x - 5.0, c.y + 5.0),
                ],
                stroke,
            );
            painter.circle_stroke(egui::pos2(c.x - 5.5, c.y + 6.0), 2.4, stroke);
            painter.circle_stroke(egui::pos2(c.x + 5.5, c.y + 6.0), 2.4, stroke);
        }
        ToolIcon::Paste => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 6.0, c.y - 5.0),
                    egui::pos2(c.x + 6.0, c.y + 7.0),
                ),
                1.5,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 3.0, c.y - 8.0),
                    egui::pos2(c.x + 3.0, c.y - 4.0),
                ),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.0, c.y - 1.0),
                    egui::pos2(c.x + 3.0, c.y - 1.0),
                ],
                stroke,
            );
        }
        ToolIcon::Clear => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 5.0, c.y - 5.0),
                    egui::pos2(c.x + 5.0, c.y + 5.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 5.0, c.y - 5.0),
                    egui::pos2(c.x - 5.0, c.y + 5.0),
                ],
                stroke,
            );
        }
        ToolIcon::Pencil => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 6.0, c.y + 6.0),
                    egui::pos2(c.x + 5.0, c.y - 5.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 3.0, c.y - 7.0),
                    egui::pos2(c.x + 7.0, c.y - 3.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 6.0, c.y + 6.0),
                    egui::pos2(c.x - 2.0, c.y + 5.0),
                ],
                stroke,
            );
        }
        ToolIcon::Calculator => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 6.0, c.y - 7.0),
                    egui::pos2(c.x + 6.0, c.y + 7.0),
                ),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 4.0, c.y - 5.0),
                    egui::pos2(c.x + 4.0, c.y - 1.0),
                ),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            for row in 0..2 {
                for col in 0..2 {
                    let x = c.x - 3.0 + col as f32 * 4.0;
                    let y = c.y + 2.0 + row as f32 * 3.5;
                    painter.circle_filled(egui::pos2(x, y), 0.9, stroke.color);
                }
            }
        }
        ToolIcon::Grid => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 7.0, c.y - 7.0),
                    egui::pos2(c.x + 7.0, c.y + 7.0),
                ),
                1.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [egui::pos2(c.x, c.y - 7.0), egui::pos2(c.x, c.y + 7.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x - 7.0, c.y), egui::pos2(c.x + 7.0, c.y)],
                stroke,
            );
        }
        ToolIcon::Plus => {
            painter.line_segment(
                [egui::pos2(c.x - 6.0, c.y), egui::pos2(c.x + 6.0, c.y)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x, c.y - 6.0), egui::pos2(c.x, c.y + 6.0)],
                stroke,
            );
        }
        ToolIcon::Minus => {
            painter.line_segment(
                [egui::pos2(c.x - 6.0, c.y), egui::pos2(c.x + 6.0, c.y)],
                stroke,
            );
        }
        ToolIcon::Sort => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.0, c.y + 6.0),
                    egui::pos2(c.x - 3.0, c.y - 6.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 6.0, c.y - 3.0),
                    egui::pos2(c.x - 3.0, c.y - 6.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 3.0, c.y - 6.0),
                    egui::pos2(c.x + 3.0, c.y + 6.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 6.0, c.y + 3.0),
                    egui::pos2(c.x + 3.0, c.y + 6.0),
                ],
                stroke,
            );
        }
        ToolIcon::Swap => {
            painter.line_segment(
                [
                    egui::pos2(c.x - 6.0, c.y - 2.0),
                    egui::pos2(c.x + 6.0, c.y - 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 3.0, c.y - 5.0),
                    egui::pos2(c.x + 6.0, c.y - 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 6.0, c.y + 2.0),
                    egui::pos2(c.x - 6.0, c.y + 2.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.0, c.y + 5.0),
                    egui::pos2(c.x - 6.0, c.y + 2.0),
                ],
                stroke,
            );
        }
        ToolIcon::Layers => {
            for (index, dy) in [-4.0, 0.0, 4.0].iter().enumerate() {
                let y = c.y + *dy;
                let inset = index as f32 * 1.5;
                painter.line_segment(
                    [
                        egui::pos2(c.x - 7.0 + inset, y),
                        egui::pos2(c.x + 7.0 - inset, y),
                    ],
                    stroke,
                );
            }
        }
        ToolIcon::Filter => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x - 7.0, c.y - 6.0),
                    egui::pos2(c.x + 7.0, c.y - 6.0),
                    egui::pos2(c.x + 2.0, c.y),
                    egui::pos2(c.x + 2.0, c.y + 6.0),
                    egui::pos2(c.x - 2.0, c.y + 4.0),
                    egui::pos2(c.x - 2.0, c.y),
                    egui::pos2(c.x - 7.0, c.y - 6.0),
                ],
                stroke,
            ));
        }
        ToolIcon::Box => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 5.0, c.y - 3.0),
                    egui::pos2(c.x + 5.0, c.y + 3.0),
                ),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [egui::pos2(c.x, c.y - 7.0), egui::pos2(c.x, c.y - 3.0)],
                stroke,
            );
            painter.line_segment(
                [egui::pos2(c.x, c.y + 3.0), egui::pos2(c.x, c.y + 7.0)],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.0, c.y - 7.0),
                    egui::pos2(c.x + 3.0, c.y - 7.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.0, c.y + 7.0),
                    egui::pos2(c.x + 3.0, c.y + 7.0),
                ],
                stroke,
            );
        }
        ToolIcon::Bars => {
            for (index, height) in [4.0, 9.0, 6.0].iter().enumerate() {
                let x = c.x - 6.0 + index as f32 * 5.0;
                painter.rect_stroke(
                    egui::Rect::from_min_max(
                        egui::pos2(x, c.y + 7.0 - height),
                        egui::pos2(x + 3.5, c.y + 7.0),
                    ),
                    0.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        ToolIcon::Dots => {
            for (dx, dy) in [(-5.0, 3.0), (0.0, -4.0), (5.0, 2.0)] {
                painter.circle_filled(egui::pos2(c.x + dx, c.y + dy), 1.6, stroke.color);
            }
        }
        ToolIcon::Info => {
            painter.circle_stroke(c, 7.0, stroke);
            painter.text(
                c,
                egui::Align2::CENTER_CENTER,
                "i",
                egui::FontId::proportional(10.0),
                stroke.color,
            );
        }
        ToolIcon::Globe => {
            painter.circle_stroke(c, 7.0, stroke);
            painter.line_segment(
                [egui::pos2(c.x - 7.0, c.y), egui::pos2(c.x + 7.0, c.y)],
                stroke,
            );
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x, c.y - 7.0),
                    egui::pos2(c.x - 3.0, c.y),
                    egui::pos2(c.x, c.y + 7.0),
                    egui::pos2(c.x + 3.0, c.y),
                    egui::pos2(c.x, c.y - 7.0),
                ],
                stroke,
            ));
        }
        ToolIcon::Sigma => {
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(c.x + 6.0, c.y - 7.0),
                    egui::pos2(c.x - 6.0, c.y - 7.0),
                    egui::pos2(c.x + 2.0, c.y),
                    egui::pos2(c.x - 6.0, c.y + 7.0),
                    egui::pos2(c.x + 6.0, c.y + 7.0),
                ],
                stroke,
            ));
        }
        ToolIcon::Clock => {
            painter.circle_stroke(c, 7.0, stroke);
            painter.line_segment([egui::pos2(c.x, c.y), egui::pos2(c.x, c.y - 4.0)], stroke);
            painter.line_segment(
                [egui::pos2(c.x, c.y), egui::pos2(c.x + 3.5, c.y + 1.0)],
                stroke,
            );
        }
        // Two axes with arrow heads: loadings, components and coordinates.
        // Staircase: empirical CDF, survival curve, step functions.
        // Four cells: matrices, cross-tabulations, panel layouts.
        // Smooth S curve: fitted models and transformed distributions.
        ToolIcon::Plug => {
            painter.rect_stroke(
                egui::Rect::from_min_max(
                    egui::pos2(c.x - 5.0, c.y - 3.0),
                    egui::pos2(c.x + 5.0, c.y + 7.0),
                ),
                0.0,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x - 3.0, c.y - 3.0),
                    egui::pos2(c.x - 3.0, c.y - 7.0),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(c.x + 3.0, c.y - 3.0),
                    egui::pos2(c.x + 3.0, c.y - 7.0),
                ],
                stroke,
            );
        }
    }
}

fn tool_icon(ui: &mut egui::Ui, icon: ToolIcon, tooltip: &str, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
    let visuals = ui.style().interact(&response);
    if enabled && (response.hovered() || response.is_pointer_button_down_on()) {
        ui.painter()
            .rect_filled(rect.shrink(1.0), 3.0, visuals.weak_bg_fill);
    }
    let color = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    paint_icon(
        ui.painter(),
        rect.center(),
        icon,
        egui::Stroke::new(1.4, color),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tooltip));
    response.on_hover_text(tooltip)
}

/// A menu row with an icon, a label and an optional shortcut.
/// A dialog window: light title bar with the FreePlots-blue title and close cross.
///
/// The title (and its close button) is drawn before the body closure, so the
/// foreground strokes are switched to white for the title and restored to the
/// theme colour at the start of the body.
struct DialogWindow<'a> {
    inner: egui::Window<'a>,
    clear: bool,
    /// The dialog is pinned (docked) instead of centred.
    fixed: bool,
}

impl<'a> DialogWindow<'a> {
    /// Hides the Clear button (the dialog has no options to reset).
    fn without_clear(mut self) -> Self {
        self.clear = false;
        self
    }

    fn collapsible(mut self, value: bool) -> Self {
        self.inner = self.inner.collapsible(value);
        self
    }

    fn resizable(mut self, value: bool) -> Self {
        self.inner = self.inner.resizable(value);
        self
    }

    fn default_width(mut self, value: f32) -> Self {
        self.inner = self.inner.default_width(value);
        self
    }

    fn default_pos(mut self, value: impl Into<egui::Pos2>) -> Self {
        self.inner = self.inner.default_pos(value);
        self
    }

    /// Pins the dialog at a screen position (the graph editor docks itself to
    /// the right of the graph).
    fn fixed_pos(mut self, value: egui::Pos2) -> Self {
        self.fixed = true;
        self.inner = self
            .inner
            .fixed_pos(value)
            .pivot(egui::Align2::LEFT_TOP)
            .movable(false);
        self
    }

    fn default_size(mut self, value: impl Into<egui::Vec2>) -> Self {
        self.inner = self.inner.default_size(value);
        self
    }

    fn id(mut self, id: egui::Id) -> Self {
        self.inner = self.inner.id(id);
        self
    }

    fn open(mut self, open: &'a mut bool) -> Self {
        self.inner = self.inner.open(open);
        self
    }

    fn show<R>(
        mut self,
        ctx: &egui::Context,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        // Open in the middle of the window, then let the user drag it. A
        // pinned dialog keeps the position it was given.
        if !self.fixed {
            self.inner = self
                .inner
                .default_pos(ctx.content_rect().center())
                .pivot(egui::Align2::CENTER_CENTER);
        }
        let theme = ctx.theme();
        let previous = ctx.global_style();
        let dark = (
            previous.visuals.widgets.inactive.fg_stroke,
            previous.visuals.widgets.hovered.fg_stroke,
            previous.visuals.widgets.active.fg_stroke,
        );
        ctx.global_style_mut(|style| {
            style.spacing.combo_height = 160.0;
            let title = egui::Stroke::new(
                1.4,
                previous.visuals.override_text_color.unwrap_or_default(),
            );
            style.visuals.widgets.inactive.fg_stroke = title;
            style.visuals.widgets.hovered.fg_stroke = title;
            style.visuals.widgets.active.fg_stroke = title;
        });
        let clear = self.clear;
        let response = self.inner.show(ctx, |ui| {
            // The title is already painted; the body keeps the theme strokes.
            // A shorter popup keeps long column lists scrollable inside the
            // dialog instead of being clipped by its bottom edge.
            ui.spacing_mut().combo_height = 160.0;
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.visuals_mut().widgets.inactive.fg_stroke = dark.0;
            ui.visuals_mut().widgets.hovered.fg_stroke = dark.1;
            ui.visuals_mut().widgets.active.fg_stroke = dark.2;
            if clear {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Clear")
                            .on_hover_text(
                                "Reset the transient options; saved preferences are kept",
                            )
                            .clicked()
                        {
                            ctx.data_mut(|data| {
                                data.insert_temp(egui::Id::new("fp_clear_dialog"), true);
                            });
                        }
                    });
                });
            }
            add_contents(ui)
        });
        ctx.set_style_of(theme, previous);
        response
    }
}

/// "1 row" / "3 rows" — the count with the right plural.
fn rows_label(rows: usize) -> String {
    if rows == 1 {
        "1 row".to_owned()
    } else {
        format!("{rows} rows")
    }
}

/// Numerator of the About formula.
const ABOUT_NUMERATOR: &str = "Σ(F² + R² + E²)";
/// Denominator of the About formula.
const ABOUT_DENOMINATOR: &str = "√(O² + T² + S²)";
/// Factor of the About formula: the remaining letters of FreePlots.
const ABOUT_FACTOR: &str = "× (P + L + E)";

/// Applies the light or dark visual theme.
fn apply_theme(ctx: &egui::Context, light: bool) {
    install_fonts(ctx);
    let mut visuals = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    if light {
        visuals.panel_fill = Color32::from_rgb(244, 241, 234);
        visuals.window_fill = Color32::from_rgb(252, 250, 246);
        visuals.extreme_bg_color = Color32::from_rgb(233, 228, 218);
        visuals.text_edit_bg_color = Some(Color32::from_rgb(255, 254, 251));
        visuals.faint_bg_color = Color32::from_rgb(238, 234, 225);
        visuals.window_stroke = egui::Stroke::new(1.0, Color32::from_rgb(208, 201, 189));
        visuals.hyperlink_color = Color32::from_rgb(15, 128, 116);
        visuals.selection = egui::style::Selection {
            bg_fill: Color32::from_rgb(15, 143, 128),
            stroke: egui::Stroke::new(1.0, Color32::BLACK),
        };
        visuals.override_text_color = Some(Color32::BLACK);
        visuals.weak_text_color = Some(Color32::BLACK);
        visuals.widgets.noninteractive.bg_fill = visuals.panel_fill;
        visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(218, 212, 201));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(255, 253, 248);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(250, 247, 240);
        visuals.widgets.inactive.bg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(214, 207, 195));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(229, 245, 241);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(223, 242, 237);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(15, 143, 128));
        visuals.widgets.active.bg_fill = Color32::from_rgb(15, 143, 128);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(15, 143, 128);
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(15, 143, 128));
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(34, 39, 43));
        visuals.widgets.open.bg_fill = Color32::from_rgb(246, 243, 236);
        visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(200, 193, 181));
    } else {
        visuals.panel_fill = Color32::from_rgb(20, 23, 27);
        visuals.window_fill = Color32::from_rgb(29, 34, 41);
        visuals.extreme_bg_color = Color32::from_rgb(13, 16, 19);
        visuals.text_edit_bg_color = Some(Color32::from_rgb(16, 19, 23));
        visuals.faint_bg_color = Color32::from_rgb(35, 41, 49);
        visuals.code_bg_color = Color32::from_rgb(16, 19, 23);
        visuals.window_stroke = egui::Stroke::new(1.0, Color32::from_rgb(58, 68, 78));
        visuals.hyperlink_color = Color32::from_rgb(47, 182, 166);
        visuals.selection = egui::style::Selection {
            bg_fill: Color32::from_rgb(47, 182, 166),
            stroke: egui::Stroke::new(1.0, Color32::WHITE),
        };
        visuals.warn_fg_color = Color32::from_rgb(240, 176, 72);
        visuals.error_fg_color = Color32::from_rgb(240, 104, 104);
        visuals.override_text_color = Some(Color32::WHITE);
        visuals.weak_text_color = Some(Color32::WHITE);
        visuals.widgets.noninteractive.bg_fill = visuals.panel_fill;
        visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0, Color32::from_rgb(44, 52, 61));
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(30, 35, 42);
        visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(34, 40, 47);
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(52, 61, 71));
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 48, 56);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(46, 55, 64);
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(47, 182, 166));
        visuals.widgets.active.bg_fill = Color32::from_rgb(47, 182, 166);
        visuals.widgets.active.weak_bg_fill = Color32::from_rgb(47, 182, 166);
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(47, 182, 166));
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(230, 236, 239));
        visuals.widgets.open.bg_fill = Color32::from_rgb(37, 44, 52);
        visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, Color32::from_rgb(70, 82, 93));
    }
    visuals.window_corner_radius = egui::CornerRadius::same(3);
    visuals.menu_corner_radius = egui::CornerRadius::same(3);
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.striped = true;
    visuals.slider_trailing_fill = true;
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(2);
    }
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 16,
        spread: 0,
        color: Color32::from_black_alpha(if light { 40 } else { 90 }),
    };
    ctx.set_visuals(visuals);
    // The text scale is egui's default (Body 14, Small 10, Heading 18), the
    // same one the companion drawing program uses; the OS supplies the DPI.
}

/// Installs the bundled UI font (DejaVu Sans, the same face used by the
/// companion drawing program).
fn install_fonts(ctx: &egui::Context) {
    use std::sync::Arc;
    // Only the bundled faces are loaded: the default font set (four faces) is
    // not compiled in, which keeps the binary and the font atlas small. The
    // serif and mono faces are offered to the graph text dialog.
    let mut fonts = egui::FontDefinitions::empty();
    for (name, bytes) in [
        ("dejavu", &include_bytes!("../assets/DejaVuSans.ttf")[..]),
        (
            "dejavu-serif",
            &include_bytes!("../assets/DejaVuSerif.ttf")[..],
        ),
        (
            "dejavu-mono",
            &include_bytes!("../assets/DejaVuSansMono.ttf")[..],
        ),
    ] {
        fonts.font_data.insert(
            name.to_owned(),
            Arc::new(egui::FontData::from_static(bytes)),
        );
    }
    fonts
        .families
        .insert(egui::FontFamily::Proportional, vec!["dejavu".to_owned()]);
    fonts.families.insert(
        egui::FontFamily::Name("serif".into()),
        vec!["dejavu-serif".to_owned()],
    );
    fonts
        .families
        .insert(egui::FontFamily::Monospace, vec!["dejavu-mono".to_owned()]);
    ctx.set_fonts(fonts);
}

/// Paints the About formula:
/// `FreePlots = Σ(F² + R² + E²) / √(O² + T² + S²) × (P + L + E)`.
///
/// The letters of the formula spell `FREEPLOTS` (nine letters, two E).
fn about_formula(ui: &mut egui::Ui) {
    let color = ui.visuals().text_color();
    let big = egui::FontId::proportional(22.0);
    let measure = |text: &str| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), big.clone(), color)
            .size()
            .x
    };
    let left_width = measure("FreePlots =");
    let right_width = measure(ABOUT_FACTOR);
    let fraction_width = measure(ABOUT_NUMERATOR)
        .max(measure(ABOUT_DENOMINATOR))
        .max(200.0);
    let total = left_width + 8.0 + fraction_width + 8.0 + right_width;
    ui.allocate_ui_with_layout(
        egui::vec2(total, 96.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.label(RichText::new("FreePlots =").font(big.clone()).color(color));
            ui.add_space(8.0);
            ui.allocate_ui_with_layout(
                egui::vec2(fraction_width, 96.0),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new(ABOUT_NUMERATOR)
                            .font(big.clone())
                            .color(color),
                    );
                    let (rect, _) = ui
                        .allocate_exact_size(egui::vec2(fraction_width, 4.0), egui::Sense::hover());
                    ui.painter().hline(
                        rect.x_range(),
                        rect.center().y,
                        egui::Stroke::new(1.6, color),
                    );
                    ui.label(
                        RichText::new(ABOUT_DENOMINATOR)
                            .font(big.clone())
                            .color(color),
                    );
                },
            );
            ui.add_space(8.0);
            ui.label(RichText::new(ABOUT_FACTOR).font(big).color(color));
        },
    );
}

/// Parses a stable distribution code from the settings file.
fn parse_distribution(code: &str) -> Option<fp_core::workflow::DistributionKind> {
    use fp_core::workflow::DistributionKind;
    match code {
        "normal" => Some(DistributionKind::Normal),
        "lognormal" => Some(DistributionKind::Lognormal),
        "weibull" => Some(DistributionKind::Weibull),
        "exponential" => Some(DistributionKind::Exponential),
        "logistic" => Some(DistributionKind::Logistic),
        "loglogistic" => Some(DistributionKind::Loglogistic),
        "gamma" => Some(DistributionKind::Gamma),
        "extremevalue" => Some(DistributionKind::ExtremeValue),
        "weibull3" => Some(DistributionKind::Weibull3),
        "lognormal3" => Some(DistributionKind::Lognormal3),
        _ => None,
    }
}

fn dialog_window<'a>(title: impl Into<String>) -> DialogWindow<'a> {
    DialogWindow {
        clear: true,
        fixed: false,
        inner: egui::Window::new(egui::RichText::new(title.into()).heading())
            .title_frame(
                egui::Frame::window(&egui::Style::default())
                    .fill(egui::Color32::TRANSPARENT)
                    .stroke(egui::Stroke::NONE),
            )
            .pivot(egui::Align2::CENTER_CENTER),
    }
}

/// Convenience methods that attach the help text to a control.
trait HelpUi {
    fn help_label(&mut self, text: impl AsRef<str>) -> egui::Response;
    fn help_button(&mut self, text: impl AsRef<str>) -> egui::Response;
    fn help_checkbox(&mut self, checked: &mut bool, text: impl AsRef<str>) -> egui::Response;
}

/// Carries the tooltip of the label over to the widget the label describes.
trait HelpLink {
    fn hover_from(self, label: &egui::Response) -> egui::Response;
}

impl HelpLink for egui::Response {
    fn hover_from(self, label: &egui::Response) -> egui::Response {
        match self
            .ctx
            .data(|data| data.get_temp::<String>(label.id.with("fp_help")))
        {
            Some(help) => self.on_hover_text(help),
            None => self,
        }
    }
}

impl HelpUi for egui::Ui {
    fn help_label(&mut self, text: impl AsRef<str>) -> egui::Response {
        add_help_label(self, text)
    }
    fn help_button(&mut self, text: impl AsRef<str>) -> egui::Response {
        add_help_button(self, text)
    }
    fn help_checkbox(&mut self, checked: &mut bool, text: impl AsRef<str>) -> egui::Response {
        add_help_checkbox(self, checked, text)
    }
}

/// Help text for a form label (the text before the colon).
fn label_help(text: &str) -> Option<String> {
    let trimmed = text.trim().trim_end_matches(':').trim();
    let help = match trimmed {
        "Column" => "Column used by the analysis",
        "First column" => "First column of the comparison",
        "Second column" => "Second column of the comparison",
        "Response" => "Column analysed as the response",
        "Predictors" => "Comma-separated predictor columns",
        "Factor" => "Column that defines the groups",
        "Factor A" => "First factor of the model",
        "Factor B" => "Second factor of the model",
        "Row" => "Column that defines the rows of the table",
        "Distribution" => "Distribution family used for the fit",
        "Confidence" => "Confidence level in percent",
        "Alternative" => "Two-sided or one-sided alternative hypothesis",
        "Method" => "Method used by the test or the chart",
        "Null proportion" => "Proportion under the null hypothesis",
        "Null variance" => "Variance under the null hypothesis",
        "Null mean" => "Mean under the null hypothesis",
        "Reference mean" => "Reference mean of the equivalence test",
        "Margin" => "Equivalence margin",
        "Reference" => "Known reference value",
        "Success value (0/1)" => "Value counted as a success",
        "LSL" => "Lower specification limit",
        "USL" => "Upper specification limit",
        "Target" => "Target value of the capability study",
        "Subgroup size" => "Number of consecutive observations per subgroup",
        "Subgroup" => "Subgroup size of the multivariate chart",
        "Subgroup column" => "Optional column with the subgroup labels",
        "Time" => "Time column",
        "Time column" => "Time column of the reliability model",
        "Time (optional)" => "Optional time column used for the horizontal axis",
        "Stress" => "Stress column",
        "Use stress" => "Stress at which the life is extrapolated",
        "Mode column" => "Optional failure-mode column for competing risks",
        "Horizon" => "Time at which the incidence is reported (empty = last failure)",
        "Duration" => "Duration of one step-stress step",
        "Unit" => "Unit identifier of the step-stress test",
        "Event" => "Event indicator column (1 = failure)",
        "Censoring column" => "Optional column with the censoring indicator",
        "Censoring value" => "Value that marks a right-censored observation",
        "Sample size" => "Number of units sampled",
        "Acceptance" => "Acceptance number of the sampling plan",
        "Lot size" => "Lot size (0 = large lot)",
        "AQL %" => "Acceptable quality level in percent",
        "LTPD %" => "Lot tolerance percent defective",
        "Producer risk %" => "Producer risk in percent",
        "Consumer risk %" => "Consumer risk in percent",
        "Window" => "Number of observations in the moving average",
        "Period" => "Seasonal period of the decomposition",
        "Season" => "Seasonal period of the smoothing model",
        "Forecast periods" => "Number of periods forecast",
        "Replicates" => "Number of replicates of every design point",
        "Center points" => "Number of centre points",
        "Block size" => "Size of each block (0 = no blocks)",
        "Runs" => "Number of runs of the design",
        "Seed" => "Random seed used for the run order",
        "Factors" => "Comma-separated factor names or columns",
        "λ" => "EWMA smoothing constant",
        "L" => "EWMA limit width in sigma units",
        "k" => "CUSUM reference value in sigma units",
        "h" => "CUSUM decision interval in sigma units",
        "Level" => "Confidence level of the band",
        "Display" => "What the probability plot shows",
        "Layout" => "Separate panels or one overlay",
        "Group" => "Optional grouping column",
        "Columns" => "Comma-separated columns",
        "Bins on the diagonal" => "Number of histogram bins on the diagonal",
        "Entry alpha" => "Alpha needed to enter a term",
        "Removal alpha" => "Alpha needed to remove a term",
        "Max steps" => "Maximum number of selection steps",
        "Score components" => "Number of components written as score columns",
        "Clusters" => "Number of clusters",
        "Bootstrap reps" => "Bootstrap replications (0 = automatic)",
        "Keys" => "Comma-separated sort keys; append :desc for descending",
        "Value" => "Value used by the condition",
        "New name" => "New name of the selected column",
        "Find what" => "Text to look for in the worksheet",
        "Replace with" => "Text written in place of the matches",
        "Decimal places" => "Digits shown after the decimal point",
        "Alignment" => "Horizontal alignment of the column",
        _ => "",
    };
    if !help.is_empty() {
        return Some(help.to_owned());
    }
    text.trim_end()
        .ends_with(':')
        .then(|| format!("Set the {}", trimmed.to_lowercase()))
}

fn button_help(text: &str) -> String {
    match text {
        "Run" => "Run the analysis with the current options".to_owned(),
        "OK" => "Apply the options and run".to_owned(),
        "Cancel" => "Close without running".to_owned(),
        "Generate" => "Generate the design and replace the worksheet".to_owned(),
        "Find Next" => "Select the next cell that matches".to_owned(),
        "Replace" => "Replace this cell, then find the next match".to_owned(),
        "Replace All" => "Replace every match in this worksheet".to_owned(),
        "Apply" => "Apply the format to the selected column".to_owned(),
        "Remove format" => "Show the column with the default format".to_owned(),
        "Close" => "Close the dialog".to_owned(),
        "Rename" => "Rename the selected column".to_owned(),
        "Clear" => "Clear the Session output".to_owned(),
        "English" => "Switch the interface and the report to English".to_owned(),
        "Português" => "Switch the interface and the report to Portuguese".to_owned(),
        other if other.starts_with("Remove ") => "Remove this item from the list".to_owned(),
        other => format!("Click {other}"),
    }
}

fn checkbox_help(text: &str) -> String {
    match text {
        "Randomize run order" => "Shuffle the run order with the seed".to_owned(),
        "Least-squares line" => "Draw the fitted least-squares line".to_owned(),
        "Confidence band" => "Draw the confidence band around the fitted line".to_owned(),
        "Same X scale" => "Use one X scale across the panels".to_owned(),
        "Same Y scale" => "Use one Y scale across the panels".to_owned(),
        "Box-Cox transformation" => {
            "Estimate a Box-Cox transform before the capability fit".to_owned()
        }
        "Require a stable I–MR screen" => {
            "Refuse the analysis when the I–MR screen signals".to_owned()
        }
        "Stationary point and eigenvalues" => {
            "Report the stationary point and its eigenvalues".to_owned()
        }
        "Contour plot (two factors)" => "Draw the two-factor contour plot".to_owned(),
        "Steepest ascent path" => "Follow the gradient from the design centre".to_owned(),
        "Standardise (z scores)" => "Standardise the columns before clustering".to_owned(),
        "Match case" => "Distinguish upper and lower case".to_owned(),
        "Match the whole cell" => "Accept only cells equal to the text".to_owned(),
        "Thousands separator" => "Group the integer part in threes".to_owned(),
        "Bold" => "Draw the column in a heavier face".to_owned(),
        "Left" => "Align the column to the left".to_owned(),
        "Center" => "Centre the column".to_owned(),
        "Right" => "Align the column to the right".to_owned(),
        "Assume equal variances (pooled)" => "Use the pooled-variance two-sample t test".to_owned(),
        other => other.to_owned(),
    }
}

fn add_help_label(ui: &mut egui::Ui, text: impl AsRef<str>) -> egui::Response {
    let text = text.as_ref();
    let response = ui.label(text);
    match label_help(text) {
        Some(help) => {
            // The field next to the label picks the same help up through
            // `labelled_by`, so both the label and the input explain
            // themselves.
            ui.ctx()
                .data_mut(|data| data.insert_temp(response.id.with("fp_help"), help.clone()));
            response.on_hover_text(help)
        }
        None => response,
    }
}

fn add_help_button(ui: &mut egui::Ui, text: impl AsRef<str>) -> egui::Response {
    let text = text.as_ref();
    ui.add(egui::Button::new(text))
        .on_hover_text(button_help(text))
}

fn add_help_checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    text: impl AsRef<str>,
) -> egui::Response {
    let text = text.as_ref();
    ui.add(egui::Checkbox::new(checked, text))
        .on_hover_text(checkbox_help(text))
}

/// Small painted check button for the formula bar.
fn apply_icon_button(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(22.0, 18.0), egui::Sense::click());
    let color = ui.visuals().widgets.inactive.fg_stroke.color;
    let stroke = egui::Stroke::new(1.6, color);
    let c = rect.center();
    ui.painter().line_segment(
        [egui::pos2(c.x - 4.0, c.y), egui::pos2(c.x - 1.0, c.y + 3.0)],
        stroke,
    );
    ui.painter().line_segment(
        [
            egui::pos2(c.x - 1.0, c.y + 3.0),
            egui::pos2(c.x + 4.5, c.y - 3.5),
        ],
        stroke,
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Apply to the selected cell")
    });
    response.on_hover_text("Apply to the selected cell")
}

/// Kind of view toggle in the status bar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewIcon {
    Grid,
    Panel,
}

/// Painted view toggle (the geometric glyphs are missing from the font).
fn view_icon_button(
    ui: &mut egui::Ui,
    icon: ViewIcon,
    active: bool,
    tooltip: &str,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 20.0), egui::Sense::click());
    if active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            3.0,
            if active {
                ui.visuals().selection.bg_fill
            } else {
                ui.visuals().widgets.hovered.weak_bg_fill
            },
        );
    }
    let color = ui.visuals().text_color();
    let stroke = egui::Stroke::new(1.3, color);
    let frame = rect.shrink2(egui::vec2(6.0, 4.5));
    ui.painter()
        .rect_stroke(frame, 1.0, stroke, egui::StrokeKind::Inside);
    match icon {
        ViewIcon::Grid => {
            let center = frame.center();
            ui.painter().line_segment(
                [
                    egui::pos2(center.x, frame.top()),
                    egui::pos2(center.x, frame.bottom()),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    egui::pos2(frame.left(), center.y),
                    egui::pos2(frame.right(), center.y),
                ],
                stroke,
            );
        }
        ViewIcon::Panel => {
            let x = frame.left() + frame.width() * 0.62;
            ui.painter().line_segment(
                [egui::pos2(x, frame.top()), egui::pos2(x, frame.bottom())],
                stroke,
            );
        }
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tooltip));
    response.on_hover_text(tooltip)
}

/// A menu row with a label and an optional shortcut, without an icon.
fn menu_text_item(
    ui: &mut egui::Ui,
    label: &str,
    shortcut: Option<&str>,
    enabled: bool,
) -> egui::Response {
    let width = ui.available_width().max(190.0);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 22.0), sense);
    let visuals = ui.style().interact(&response);
    if enabled && response.hovered() {
        ui.painter().rect_filled(
            rect.shrink2(egui::vec2(2.0, 1.0)),
            3.0,
            visuals.weak_bg_fill,
        );
    }
    let color = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    ui.painter().text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        color,
    );
    if let Some(shortcut) = shortcut {
        ui.painter().text(
            egui::pos2(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            shortcut,
            egui::FontId::proportional(11.0),
            ui.visuals().weak_text_color(),
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response.on_hover_text(label)
}

fn menu_icon_item(
    ui: &mut egui::Ui,
    icon: ToolIcon,
    label: &str,
    shortcut: Option<&str>,
    enabled: bool,
) -> egui::Response {
    let width = ui.available_width().max(190.0);
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 22.0), sense);
    let visuals = ui.style().interact(&response);
    if enabled && response.hovered() {
        ui.painter().rect_filled(
            rect.shrink2(egui::vec2(2.0, 1.0)),
            3.0,
            visuals.weak_bg_fill,
        );
    }
    // Menu icons use the text colour, so the menus stay monochrome.
    let color = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    paint_icon(
        ui.painter(),
        egui::pos2(rect.left() + 13.0, rect.center().y),
        icon,
        egui::Stroke::new(1.4, color),
    );
    ui.painter().text(
        egui::pos2(rect.left() + 26.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.0),
        color,
    );
    if let Some(shortcut) = shortcut {
        ui.painter().text(
            egui::pos2(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            shortcut,
            egui::FontId::proportional(11.0),
            ui.visuals().weak_text_color(),
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response.on_hover_text(label)
}

#[cfg(test)]
mod tests;
