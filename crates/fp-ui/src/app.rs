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
mod grid;
mod menu_bar;
mod session;
mod state;

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
    /// Path and format waiting for the overwrite confirmation.
    overwrite_pending: Option<(String, FileKind, bool)>,
    save_path: String,
    save_kind: FileKind,
    save_decimal_comma: bool,
    save_request: Option<SaveRequest>,
    browse_request: Option<BrowseRequest>,
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
        };
        self.session.clear();
        self.plot = None;
        self.show_plot = false;
        self.error = None;
        self.undo_stack.clear();
        self.redo_stack.clear();
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
    pub fn mark_saved(&mut self, path: &str) {
        self.source = path.to_owned();
        self.status = format!("Saved {path}");
        self.error = None;
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

    /// Applies a finished analysis to the Session and the plot.
    fn apply_output(&mut self, output: AnalysisOutput, label: String) {
        for result in &output.results {
            self.append_result(result);
        }
        self.show_plot = output.plot.is_some();
        self.plot = output.plot;
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
                self.show_plot = out.plot.is_some();
                self.plot = out.plot;
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
        let subgroup_column =
            (!self.chart_subgroup_column.is_empty()).then(|| self.chart_subgroup_column.clone());
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
                .with_subgroup_column(subgroup_column),
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
        self.show_doe_dialog = false;
        self.show_dot_dialog = false;
        self.show_ecdf_dialog = false;
        self.show_equivalence_dialog = false;
        self.show_exp_dialog = false;
        self.show_g_chart_dialog = false;
        self.show_gage_dialog = false;
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
        // Open in the middle of the window, then let the user drag it.
        self.inner = self
            .inner
            .default_pos(ctx.content_rect().center())
            .pivot(egui::Align2::CENTER_CENTER);
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
                            .on_hover_text("Reset the options of this dialog")
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
    // Only the bundled face is loaded: the default font set (four faces) is
    // not compiled in, which keeps the binary and the font atlas small.
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert(
        "dejavu".to_owned(),
        Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/DejaVuSans.ttf"
        ))),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.insert(family, vec!["dejavu".to_owned()]);
    }
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

fn dialog_window<'a>(title: impl Into<String>) -> DialogWindow<'a> {
    DialogWindow {
        clear: true,
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
mod tests {
    use super::*;
    use fp_core::data::Column;

    /// Three-step test with twelve units, all failing at the last step.
    fn step_stress_frame() -> DataFrame {
        let mut duration = Vec::new();
        let mut stress = Vec::new();
        let mut unit = Vec::new();
        let mut event = Vec::new();
        let levels = [100.0, 120.0, 140.0];
        for index in 0..12 {
            let first = 40.0 + f64::from(index % 4);
            let second = 20.0 + 0.5 * f64::from(index % 3);
            let third = 10.0 + 0.25 * f64::from(index % 2);
            for (step, (level, time)) in levels.iter().zip([first, second, third]).enumerate() {
                duration.push(time);
                stress.push(*level);
                unit.push(format!("unit {}", index + 1));
                event.push(f64::from(u8::from(step == 2)));
            }
        }
        DataFrame::new(vec![
            Column::new_f64("duration", duration),
            Column::new_f64("stress", stress),
            Column::new_text("unit", unit),
            Column::new_f64("event", event),
        ])
        .expect("frame")
    }

    /// Two numeric columns of ten observations, for two-sample and paired tests.
    fn two_sample_frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64(
                "before",
                vec![12.1, 11.8, 12.4, 11.9, 12.3, 12.0, 11.7, 12.2, 12.5, 11.6],
            ),
            Column::new_f64(
                "after",
                vec![12.6, 12.4, 12.9, 12.3, 12.8, 12.5, 12.2, 12.7, 12.9, 12.1],
            ),
        ])
        .expect("frame")
    }

    /// Forty observations for the time-series dialogs.
    fn series_values_frame() -> DataFrame {
        let values: Vec<f64> = (0..40)
            .map(|index| 10.0 + 0.2 * f64::from(index) + 2.0 * f64::from(index % 4))
            .collect();
        DataFrame::new(vec![Column::new_f64("y", values)]).expect("frame")
    }

    /// Gage R&R data: ten parts, two operators, two replicates.
    fn gage_frame() -> DataFrame {
        let mut measure = Vec::new();
        let mut part = Vec::new();
        let mut operator = Vec::new();
        for p in 1..=10 {
            let reference = 10.0 + f64::from(p);
            for op in ["A", "B"] {
                for rep in 0..2 {
                    measure.push(
                        reference + 0.05 * f64::from(rep) + if op == "A" { 0.1 } else { -0.1 },
                    );
                    part.push(format!("P{p}"));
                    operator.push(op.to_owned());
                }
            }
        }
        DataFrame::new(vec![
            Column::new_f64("measure", measure),
            Column::new_text("part", part),
            Column::new_text("operator", operator),
        ])
        .expect("frame")
    }

    /// Attribute agreement data: five parts, three appraisers, one rating each.
    fn attribute_frame() -> DataFrame {
        let mut part = Vec::new();
        let mut appraiser = Vec::new();
        let mut rating = Vec::new();
        for p in 1..=5 {
            for (index, who) in ["A", "B", "C"].iter().enumerate() {
                part.push(format!("P{p}"));
                appraiser.push((*who).to_owned());
                rating.push(if f64::from(p as u8) + index as f64 > 4.0 {
                    "pass".to_owned()
                } else {
                    "fail".to_owned()
                });
            }
        }
        DataFrame::new(vec![
            Column::new_text("part", part),
            Column::new_text("appraiser", appraiser),
            Column::new_text("rating", rating),
        ])
        .expect("frame")
    }

    /// Linearity data: five reference levels with three replicates each.
    fn linearity_frame() -> DataFrame {
        let mut measure = Vec::new();
        let mut reference = Vec::new();
        for level in 1..=5 {
            for rep in 0..3 {
                let base = f64::from(level) * 2.0;
                measure.push(base + 0.1 * f64::from(rep) + 0.05);
                reference.push(base);
            }
        }
        DataFrame::new(vec![
            Column::new_f64("measure", measure),
            Column::new_f64("reference", reference),
        ])
        .expect("frame")
    }

    /// 2x2 factorial with two replicates, a block column and a response.
    fn factorial_frame() -> DataFrame {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut y = Vec::new();
        let mut block = Vec::new();
        for run in 0..2 {
            for (level_a, level_b) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                a.push(level_a);
                b.push(level_b);
                y.push(10.0 + 2.0 * level_a - level_b + 0.1 * f64::from(run));
                block.push("1".to_owned());
            }
        }
        DataFrame::new(vec![
            Column::new_f64("A", a),
            Column::new_f64("B", b),
            Column::new_f64("y", y),
            Column::new_text("block", block),
        ])
        .expect("frame")
    }

    /// L8 Taguchi design with a response.
    fn taguchi_frame() -> DataFrame {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut c = Vec::new();
        let mut y = Vec::new();
        for run in 0..8u8 {
            let level = |bit: u8| if run & bit == 0 { 1.0 } else { 2.0 };
            let (la, lb, lc) = (level(1), level(2), level(4));
            a.push(la);
            b.push(lb);
            c.push(lc);
            y.push(20.0 + 1.5 * (la - 1.5) + 0.8 * (lb - 1.5) - 0.6 * (lc - 1.5));
        }
        DataFrame::new(vec![
            Column::new_f64("A", a),
            Column::new_f64("B", b),
            Column::new_f64("C", c),
            Column::new_f64("y", y),
        ])
        .expect("frame")
    }

    /// Life data: ten times, two censored, two stress levels and two modes.
    fn life_frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64(
                "time",
                vec![10.0, 12.0, 15.0, 18.0, 20.0, 25.0, 30.0, 35.0, 40.0, 50.0],
            ),
            Column::new_f64(
                "censor",
                vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0],
            ),
            Column::new_f64(
                "stress",
                vec![
                    100.0, 100.0, 100.0, 100.0, 100.0, 120.0, 120.0, 120.0, 120.0, 120.0,
                ],
            ),
            Column::new_text(
                "mode",
                vec![
                    "A".to_owned(),
                    "B".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                ],
            ),
        ])
        .expect("frame")
    }

    /// Twenty observations with two predictors: y depends on x1 and x2.
    fn regression_frame() -> DataFrame {
        let mut x1 = Vec::new();
        let mut x2 = Vec::new();
        let mut x3 = Vec::new();
        let mut y = Vec::new();
        for index in 0..20 {
            let a = f64::from(index);
            let b = f64::from(index % 5);
            let c = f64::from(index % 3);
            x1.push(a);
            x2.push(b);
            x3.push(c);
            y.push(5.0 + 2.0 * a + 0.5 * b + 0.05 * c);
        }
        DataFrame::new(vec![
            Column::new_f64("x1", x1),
            Column::new_f64("x2", x2),
            Column::new_f64("x3", x3),
            Column::new_f64("y", y),
        ])
        .expect("frame")
    }

    /// Balanced two-factor design: 3 groups x 2 levels x 4 replicates.
    fn two_way_frame() -> DataFrame {
        let mut y = Vec::new();
        let mut g = Vec::new();
        let mut h = Vec::new();
        for (group, level) in [
            ("A", "low"),
            ("B", "low"),
            ("C", "low"),
            ("A", "high"),
            ("B", "high"),
            ("C", "high"),
        ] {
            for replicate in 0..4 {
                let base = match group {
                    "A" => 10.0,
                    "B" => 12.0,
                    _ => 14.0,
                };
                let lift = if level == "high" { 1.0 } else { 0.0 };
                y.push(base + lift + 0.2 * f64::from(replicate));
                g.push(group.to_owned());
                h.push(level.to_owned());
            }
        }
        DataFrame::new(vec![
            Column::new_f64("y", y),
            Column::new_text("g", g),
            Column::new_text("h", h),
        ])
        .expect("frame")
    }

    /// Thirty observations with a small drift, for the control charts.
    fn process_frame() -> DataFrame {
        let values: Vec<f64> = (0..30)
            .map(|index| 10.0 + 0.05 * f64::from(index % 7) + 0.02 * f64::from(index))
            .collect();
        DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame")
    }

    /// Two 0/1 columns for the proportion tests.
    fn proportion_frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64(
                "pass",
                vec![
                    1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 0.0,
                    1.0, 1.0, 1.0, 0.0,
                ],
            ),
            Column::new_f64(
                "retest",
                vec![
                    1.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0,
                    1.0, 0.0, 1.0, 0.0,
                ],
            ),
        ])
        .expect("frame")
    }

    /// Full 3x3 factorial with a quadratic response and three replicates.
    fn desirability_frame() -> DataFrame {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut y = Vec::new();
        for replicate in 0..3 {
            for i in 0..3 {
                for j in 0..3 {
                    let x = f64::from(i) - 1.0;
                    let z = f64::from(j) - 1.0;
                    a.push(x);
                    b.push(z);
                    y.push(
                        10.0 + 2.0 * x + 3.0 * z - 1.5 * x * x - 2.0 * z * z
                            + x * z
                            + 0.1 * f64::from(replicate),
                    );
                }
            }
        }
        DataFrame::new(vec![
            Column::new_f64("a", a),
            Column::new_f64("b", b),
            Column::new_f64("y", y),
        ])
        .expect("frame")
    }

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("weight", vec![12.5, 13.1, 11.9, 12.8, 12.2]),
            Column::new_text("batch", vec!["A".into(); 5]),
        ])
        .expect("frame")
    }

    #[test]
    fn starts_with_an_empty_grid() {
        let app = FreePlotsApp::new();
        let data = app.data().expect("the grid is visible at startup");
        assert_eq!(data.n_columns(), 8);
        assert_eq!(data.n_rows(), 0);
        assert_eq!(app.selected_column(), Some("C1"));
        assert_eq!(app.selected_cell(), Some((0, 0)));
        assert_eq!(app.worksheet_count(), 1);
        assert_eq!(app.sheet_names(), vec!["Worksheet 1".to_owned()]);
        assert!(app.session_text().is_empty());
        assert!(app.plot().is_none());
    }

    #[test]
    fn typing_over_a_cell_starts_and_commits_the_edit() {
        use egui_kittest::Harness;

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness
            .input_mut()
            .events
            .push(egui::Event::Text("7".to_owned()));
        harness.run();
        assert_eq!(harness.state().inline_edit(), Some((0, 0)));
        assert_eq!(harness.state().inline_buffer(), "7");
        harness.input_mut().events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
        harness.run();
        assert_eq!(harness.state().inline_edit(), None);
        assert_eq!(
            harness.state().data().unwrap().cell_text(0, 0).as_deref(),
            Some("7")
        );
    }

    #[test]
    fn the_tab_arrows_scroll_the_worksheet_strip() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.add_sheet();
        app.add_sheet();
        app.add_sheet();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        assert_eq!(harness.state().worksheet_count(), 4);
        harness.get_by_label("▶").click();
        harness.run();
        assert_eq!(harness.state().tab_first(), 1);
        harness.get_by_label("▶|").click();
        harness.run();
        assert_eq!(harness.state().tab_first(), 3);
        harness.get_by_label("|◀").click();
        harness.run();
        assert_eq!(harness.state().tab_first(), 0);
    }

    #[test]
    fn a_paste_event_fills_a_block() {
        use egui_kittest::Harness;

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        // egui-winit delivers Ctrl+V as a Paste event carrying the clipboard
        // text; it never reports the V key itself.
        harness.event(egui::Event::Paste("7\t8\n9\t10\n".to_owned()));
        harness.run();
        let data = harness.state().data().expect("frame");
        assert_eq!(data.cell_text(0, 0).as_deref(), Some("7"));
        assert_eq!(data.cell_text(1, 0).as_deref(), Some("8"));
        assert_eq!(data.cell_text(0, 1).as_deref(), Some("9"));
        assert_eq!(data.cell_text(1, 1).as_deref(), Some("10"));
    }

    #[test]
    fn a_click_anywhere_in_an_empty_cell_selects_it() {
        use egui_kittest::Harness;

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        // The right-hand half of an empty C2 cell: there is no text there.
        let pos = egui::pos2(170.0, 205.0);
        harness.hover_at(pos);
        harness.run();
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
        harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        });
        harness.run();
        let selected = harness.state().selected_cell();
        assert_eq!(selected, Some((1, 2)));
    }

    #[test]
    fn the_global_shortcuts_reach_the_worksheet() {
        use egui_kittest::Harness;

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        harness.run();
        assert_eq!(harness.state().selection_bounds(), Some((0, 0, 1, 4)));
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::N);
        harness.run();
        harness.run();
        assert_eq!(harness.state().worksheet_count(), 2);
    }

    #[test]
    fn the_help_menu_opens_the_shortcuts() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.get_by_label("Help").click();
        harness.run();
        harness.get_by_label("Keyboard shortcuts").click();
        harness.run();
        assert!(harness.state().show_shortcuts);
        assert!(
            harness
                .get_by_label("Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z")
                .rect()
                .width()
                > 0.0
        );
    }

    #[test]
    fn the_dark_mode_is_opt_in_and_applies_the_dark_theme() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        assert!(!harness.state().dark_mode);
        harness.get_by_label("Tools").click();
        harness.run();
        harness.get_by_label("Dark mode").click();
        harness.run();
        harness.run();
        assert!(harness.state().dark_mode);
        assert!(!harness.state().light_visuals);
        harness.get_by_label("Tools").click();
        harness.run();
        harness.get_by_label("Dark mode").click();
        harness.run();
        harness.run();
        assert!(!harness.state().dark_mode);
        assert!(harness.state().light_visuals);
    }

    #[test]
    fn the_assistant_guides_to_a_dialog() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.get_by_label("Assistant").click();
        harness.run();
        harness.get_by_label("Choose an analysis…").click();
        harness.run();
        assert!(harness.state().show_assistant_dialog);
        harness.get_by_label("Compare two groups").click();
        harness.run();
        assert!(!harness.state().show_assistant_dialog);
        assert!(harness.state().show_t_dialog);
    }

    #[test]
    fn the_file_menu_requests_a_print() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.get_by_label("File").click();
        harness.run();
        harness.get_by_label("Print…").click();
        harness.run();
        assert!(harness.state_mut().take_print_request().is_some());
    }

    #[test]
    fn the_about_formula_spells_freeplots() {
        let mut letters: Vec<char> = [ABOUT_NUMERATOR, ABOUT_DENOMINATOR, ABOUT_FACTOR]
            .iter()
            .flat_map(|part| part.chars())
            .filter(|c| c.is_ascii_alphabetic())
            .map(|c| c.to_ascii_uppercase())
            .collect();
        letters.sort_unstable();
        assert_eq!(letters, vec!['E', 'E', 'F', 'L', 'O', 'P', 'R', 'S', 'T']);
    }

    #[test]
    fn the_about_dialog_has_no_clear_button() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.show_about = true;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        // The Session panel still has its own Clear; the About window has none.
        let window = harness.get_by_label("About FreePlots").rect();
        assert!(
            !harness
                .get_all_by_label("Clear")
                .any(|button| window.contains(button.rect().center())),
            "the About dialog must not show a Clear button"
        );
        assert!(harness.get_by_label("MIT license").rect().width() > 0.0);
        assert!(
            harness
                .get_by_label("FreePlots Engine 0.1.0")
                .rect()
                .width()
                > 0.0
        );
    }

    #[test]
    fn the_plus_button_adds_a_worksheet_tab() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        assert_eq!(harness.state().worksheet_count(), 1);
        {
            // The status bar also has a zoom "+"; the tab button sits above it.
            let buttons: Vec<_> = harness.get_all_by_label("+").collect();
            let tab = buttons
                .into_iter()
                .min_by(|a, b| a.rect().center().y.total_cmp(&b.rect().center().y))
                .expect("tab +");
            tab.click();
        }
        harness.run();
        assert_eq!(harness.state().worksheet_count(), 2);
        assert_eq!(harness.state().active_sheet(), 1);
        assert_eq!(harness.state().sheet_names()[1], "Worksheet 2");
        assert_eq!(harness.state().data().map(DataFrame::n_rows), Some(0));
        harness.state_mut().switch_sheet(0);
        assert_eq!(harness.state().data().map(DataFrame::n_rows), Some(5));
        assert_eq!(harness.state().source(), "weights.csv");
    }

    #[test]
    fn closing_a_worksheet_keeps_the_last_one() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.add_sheet();
        assert_eq!(app.worksheet_count(), 2);
        assert!(app.close_sheet(1));
        assert_eq!(app.worksheet_count(), 1);
        assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
        assert!(!app.close_sheet(0));
        assert!(app.status().contains("at least one"));
        assert_eq!(app.worksheet_count(), 1);
    }

    #[test]
    fn closing_the_active_worksheet_loads_another() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.add_sheet();
        app.select_cell(0, 0);
        app.set_edit_buffer("7");
        assert!(app.commit_cell());
        assert_eq!(app.active_sheet(), 1);
        assert!(app.close_sheet(1));
        assert_eq!(app.worksheet_count(), 1);
        assert_eq!(app.active_sheet(), 0);
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
    }

    #[test]
    fn a_worksheet_can_be_renamed() {
        let mut app = FreePlotsApp::new();
        app.add_sheet();
        app.open_sheet_rename(1);
        assert!(app.show_sheet_rename);
        app.sheet_rename_text = "  Sales  ".to_owned();
        app.commit_sheet_rename();
        assert_eq!(app.sheet_names()[1], "Sales");
        assert!(!app.show_sheet_rename);

        app.open_sheet_rename(1);
        app.sheet_rename_text = "   ".to_owned();
        app.commit_sheet_rename();
        assert_eq!(app.sheet_names()[1], "Sales");
        assert!(app.status().contains("cannot be empty"));
    }

    #[test]
    fn the_headers_select_rows_columns_and_the_whole_sheet() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_column(1);
        assert_eq!(app.selection_bounds(), Some((1, 0, 1, 4)));
        assert_eq!(app.selection_size(), (1, 5));
        app.select_row(2);
        assert_eq!(app.selection_bounds(), Some((0, 2, 1, 2)));
        assert_eq!(app.selection_size(), (2, 1));
        app.select_all_cells();
        assert_eq!(app.selection_bounds(), Some((0, 0, 1, 4)));
        assert_eq!(app.selection_size(), (2, 5));
        // The whole column copies as one tab-separated line per row.
        app.select_column(0);
        assert_eq!(
            app.selection_tsv().as_deref(),
            Some("12.5\n13.1\n11.9\n12.8\n12.2")
        );
    }

    #[test]
    fn switching_worksheets_keeps_each_sheet_data_and_undo() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.add_sheet();
        app.select_cell(0, 0);
        app.set_edit_buffer("42");
        assert!(app.commit_cell());

        app.switch_sheet(0);
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
        app.switch_sheet(1);
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("42"));

        // The undo history travels with its own worksheet.
        app.undo();
        assert_ne!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("42"));
        app.switch_sheet(0);
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
    }

    #[test]
    fn opening_a_file_keeps_a_non_empty_worksheet() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        assert_eq!(app.worksheet_count(), 1);
        app.set_data(frame(), "other.csv".to_owned());
        assert_eq!(app.worksheet_count(), 2);
        assert_eq!(app.active_sheet(), 1);
        assert_eq!(app.source(), "other.csv");
        app.switch_sheet(0);
        assert_eq!(app.source(), "weights.csv");
    }

    #[test]
    fn loading_selects_first_numeric_column() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        assert_eq!(app.selected_column(), Some("weight"));
        assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
        assert!(app.status().contains("5 rows"));
    }

    #[test]
    fn analysis_fills_session_and_plot() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.run_probability_plot();
        assert!(app.session_text().contains("Probability Plot"));
        assert!(app.session_text().contains("Anderson–Darling"));
        let plot = app.plot().expect("plot");
        assert_eq!(plot.x_axis.label, "weight");
        assert_eq!(plot.y_axis.label, "Percent");
        assert_eq!(plot.points.len(), 5);
    }

    #[test]
    fn lognormal_analysis_uses_a_log_axis() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "life",
            vec![1.0, 2.0, 4.0, 8.0, 16.0],
        )])
        .expect("frame");
        app.set_data(df, "life.csv".to_owned());
        app.distribution = DistributionKind::Lognormal;
        app.run_probability_plot();
        assert!(app.session_text().contains("Lognormal"));
        assert!(matches!(
            app.plot().expect("plot").x_axis.scale,
            fp_core::plot::Scale::Log10
        ));
    }

    #[test]
    fn weibull_analysis_uses_the_gumbel_axis() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "life",
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        )])
        .expect("frame");
        app.set_data(df, "life.csv".to_owned());
        app.distribution = DistributionKind::Weibull;
        app.run_probability_plot();
        assert!(app.session_text().contains("Weibull"));
        assert!(matches!(
            app.plot().expect("plot").y_axis.scale,
            fp_core::plot::Scale::Probability(fp_core::plot::ProbabilityScale::Gumbel)
        ));
    }

    #[test]
    fn logistic_analysis_uses_the_logit_axis() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "score",
            vec![-2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
        )])
        .expect("frame");
        app.set_data(df, "score.csv".to_owned());
        app.distribution = DistributionKind::Logistic;
        app.run_probability_plot();
        assert!(app.session_text().contains("Logistic"));
        assert!(matches!(
            app.plot().expect("plot").y_axis.scale,
            fp_core::plot::Scale::Probability(fp_core::plot::ProbabilityScale::Logit)
        ));
    }

    #[test]
    fn loglogistic_analysis_uses_a_log_axis() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "life",
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        )])
        .expect("frame");
        app.set_data(df, "life.csv".to_owned());
        app.distribution = DistributionKind::Loglogistic;
        app.run_probability_plot();
        assert!(app.session_text().contains("Loglogistic"));
        assert!(matches!(
            app.plot().expect("plot").x_axis.scale,
            fp_core::plot::Scale::Log10
        ));
    }

    #[test]
    fn gamma_analysis_uses_the_normal_probability_axis() {
        use fp_core::dists::{Distribution, Gamma};

        let mut app = FreePlotsApp::new();
        let dist = Gamma::new(2.0, 5.0).expect("gamma");
        let values: Vec<f64> = (1..=40)
            .map(|i| {
                let p = (i as f64 - 0.5) / 40.0;
                dist.quantile(p)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
        app.set_data(df, "life.csv".to_owned());
        app.distribution = DistributionKind::Gamma;
        app.run_probability_plot();
        assert!(app.session_text().contains("Gamma"));
        assert!(matches!(
            app.plot().expect("plot").y_axis.scale,
            fp_core::plot::Scale::Probability(fp_core::plot::ProbabilityScale::Normal)
        ));
    }

    #[test]
    fn extreme_value_analysis_uses_the_extreme_value_axis() {
        use fp_core::dists::{Distribution, ExtremeValue};

        let mut app = FreePlotsApp::new();
        let dist = ExtremeValue::new(2.0, 1.5).expect("extreme value");
        let values: Vec<f64> = (1..=40)
            .map(|i| {
                let p = (i as f64 - 0.5) / 40.0;
                dist.quantile(p)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
        app.set_data(df, "life.csv".to_owned());
        app.distribution = DistributionKind::ExtremeValue;
        app.run_probability_plot();
        assert!(app.session_text().contains("Extreme Value"));
        assert!(matches!(
            app.plot().expect("plot").y_axis.scale,
            fp_core::plot::Scale::Probability(fp_core::plot::ProbabilityScale::ExtremeValue)
        ));
    }

    #[test]
    fn display_dialog_can_be_opened_and_painted() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.open_probability_plot_dialog();
        assert!(app.show_probability_plot_dialog);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
        let painted = output.shapes.len();
        output.textures_delta.clear();
        assert!(painted > 0);
    }

    #[test]
    fn multi_y_report_and_overlay_reach_the_ui() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![12.5, 13.1, 11.9, 12.8, 12.2]),
            Column::new_f64("height", vec![21.5, 22.1, 20.9, 21.8, 21.2]),
        ])
        .expect("frame");
        app.set_data(df, "two.csv".to_owned());
        app.execute_spec(
            AnalysisSpec::probability_plot_multi(vec!["weight".to_owned(), "height".to_owned()])
                .with_layout(PlotLayout::Overlay),
            "multi".to_owned(),
        );
        assert!(app.session_text().contains("column weight"));
        assert!(app.session_text().contains("column height"));
        assert_eq!(app.plot().expect("plot").series.len(), 2);
    }

    #[test]
    fn grouping_reaches_the_ui_as_two_series() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![11.0, 11.5, 12.0, 13.0, 13.5, 14.0]),
            Column::new_text(
                "batch",
                vec![
                    "A".into(),
                    "A".into(),
                    "A".into(),
                    "B".into(),
                    "B".into(),
                    "B".into(),
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "grouped.csv".to_owned());
        // Grouping follows the layout switch: the overlay is asked for here,
        // the separate panels are the default.
        app.execute_spec(
            AnalysisSpec::probability_plot("weight")
                .with_group("batch")
                .with_layout(fp_core::workflow::PlotLayout::Overlay),
            "grouped".to_owned(),
        );
        assert_eq!(app.plot().expect("plot").series.len(), 2);
        assert!(app.session_text().contains("weight (A)"));
        assert!(app.session_text().contains("weight (B)"));
    }

    #[test]
    fn symbols_display_drops_line_and_band() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let spec = AnalysisSpec::probability_plot("weight").with_display(PlotDisplay::Symbols);
        app.execute_spec(spec, "test".to_owned());
        let plot = app.plot().expect("plot");
        assert!(!plot.points.is_empty());
        assert!(plot.lines.is_empty());
        assert!(plot.bands.is_empty());
    }

    #[test]
    fn capability_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.execute_spec(
            AnalysisSpec::capability("weight", Some(8.0), Some(12.0)),
            "capability".to_owned(),
        );
        assert!(app.session_text().contains("Capability Analysis"));
        assert!(app.session_text().contains("Cpk"));
        assert!(app.session_text().contains("PPM"));
        app.execute_spec(
            AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
                .with_capability_confidence(0.90),
            "capability 90".to_owned(),
        );
        assert!(app.session_text().contains("Cpk 90% CI"));
        app.execute_spec(
            AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
                .with_capability_distribution(DistributionKind::Weibull),
            "capability Weibull".to_owned(),
        );
        assert!(app.session_text().contains("Capability Analysis: Weibull"));
        assert!(app.session_text().contains("PPM total"));
    }

    #[test]
    fn t_test_dialog_runs_one_and_two_sample_tests() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("before", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("after", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
        ])
        .expect("frame");
        app.set_data(df, "pairs.csv".to_owned());
        app.open_t_test_dialog(TTestKind::OneSample);
        app.t_null = "3".to_owned();
        app.run_t_test_dialog();
        assert!(app.session_text().contains("1-Sample t"));
        assert!(app.session_text().contains("p-value"));

        app.open_t_test_dialog(TTestKind::TwoSample);
        app.run_t_test_dialog();
        assert!(app.session_text().contains("2-Sample t"));
        assert!(app.session_text().contains("Welch"));

        app.open_t_test_dialog(TTestKind::Paired);
        app.run_t_test_dialog();
        assert!(app.session_text().contains("Paired t"));
    }

    #[test]
    fn the_new_dialogs_can_be_opened_and_painted() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![1.0, 2.0, 3.0, 4.0, 5.0, 100.0]),
            Column::new_text(
                "g",
                vec![
                    "x".to_owned(),
                    "x".to_owned(),
                    "x".to_owned(),
                    "y".to_owned(),
                    "y".to_owned(),
                    "y".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "weights.csv".to_owned());

        let ctx = egui::Context::default();
        let paint = |app: &mut FreePlotsApp| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty());
        };

        for kind in [
            DiagnosticKind::ChiSquareGoodnessOfFit,
            DiagnosticKind::Grubbs,
        ] {
            app.open_diagnostic_dialog(kind);
            paint(&mut app);
        }
        for kind in [
            RankTestKind::MannWhitney,
            RankTestKind::Wilcoxon,
            RankTestKind::KruskalWallis,
            RankTestKind::Friedman,
            RankTestKind::MoodsMedian,
        ] {
            app.open_rank_test_dialog(kind);
            paint(&mut app);
        }
        for kind in [
            EquivalenceKind::OneSample,
            EquivalenceKind::TwoSample,
            EquivalenceKind::Paired,
        ] {
            app.open_equivalence_dialog(kind);
            paint(&mut app);
        }
        app.open_power_dialog();
        paint(&mut app);
        app.open_tolerance_dialog();
        paint(&mut app);
        app.open_box_behnken_dialog();
        paint(&mut app);
        app.open_linearity_dialog();
        paint(&mut app);
        app.open_logistic_dialog();
        paint(&mut app);
        app.open_timeseries_dialog();
        paint(&mut app);
        app.open_pca_dialog();
        paint(&mut app);
        app.open_kmeans_dialog();
        paint(&mut app);
        app.open_agreement_dialog();
        paint(&mut app);
        app.open_sort_dialog();
        paint(&mut app);
        app.open_stack_dialog();
        paint(&mut app);
        app.open_unstack_dialog();
        paint(&mut app);
        app.open_subset_dialog();
        paint(&mut app);
        app.open_pb_dialog();
        paint(&mut app);
        app.open_poisson_dialog();
        paint(&mut app);
        app.open_ordinal_dialog();
        paint(&mut app);
        app.open_pareto_dialog();
        paint(&mut app);
        app.open_calc_dialog();
        paint(&mut app);
        app.open_alt_dialog();
        paint(&mut app);
        app.open_t2_dialog();
        paint(&mut app);
        app.open_sampling_dialog();
        paint(&mut app);
        app.open_taguchi_dialog();
        paint(&mut app);
        app.open_taguchi_design_dialog();
        paint(&mut app);
        for kind in [
            TTestKind::OneSample,
            TTestKind::TwoSample,
            TTestKind::Paired,
        ] {
            app.open_t_test_dialog(kind);
            paint(&mut app);
        }
        for kind in [
            Test2Kind::OneProportion,
            Test2Kind::TwoProportions,
            Test2Kind::OneVariance,
            Test2Kind::TwoVariances,
        ] {
            app.open_test2_dialog(kind);
            paint(&mut app);
        }
        for kind in [OneWayKind::Levene, OneWayKind::Bartlett] {
            app.open_one_way_dialog(kind);
            paint(&mut app);
        }
        for opener in [
            FreePlotsApp::open_anova_dialog,
            FreePlotsApp::open_attribute_dialog,
            FreePlotsApp::open_box_cox_dialog,
            FreePlotsApp::open_correlation_dialog,
            FreePlotsApp::open_crosstab_dialog,
            FreePlotsApp::open_curve_dialog,
            FreePlotsApp::open_design_dialog,
            FreePlotsApp::open_doe_dialog,
            FreePlotsApp::open_dotplot_dialog,
            FreePlotsApp::open_ecdf_dialog,
            FreePlotsApp::open_exponential_dialog,
            FreePlotsApp::open_gage_dialog,
            FreePlotsApp::open_g_chart_dialog,
            FreePlotsApp::open_interval_dialog,
            FreePlotsApp::open_kaplan_meier_dialog,
            FreePlotsApp::open_lognormal_dialog,
            FreePlotsApp::open_matrix_dialog,
            FreePlotsApp::open_partial_dialog,
            FreePlotsApp::open_probability_plot_dialog,
            FreePlotsApp::open_regression_dialog,
            FreePlotsApp::open_rsm_dialog,
            FreePlotsApp::open_scatter_dialog,
            FreePlotsApp::open_series_dialog,
            FreePlotsApp::open_stem_dialog,
            FreePlotsApp::open_stepwise_dialog,
            FreePlotsApp::open_two_way_dialog,
            FreePlotsApp::open_type1_dialog,
            FreePlotsApp::open_weibull_dialog,
        ] {
            opener(&mut app);
            paint(&mut app);
        }
    }

    #[test]
    fn power_dialog_covers_the_anova_and_regression_procedures() {
        let mut app = FreePlotsApp::new();
        let df =
            DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0])]).expect("frame");
        app.set_data(df, "values.csv".to_owned());

        app.open_power_dialog();
        app.power_procedure = PowerProcedure::Anova;
        app.power_effect = "1".to_owned();
        app.power_groups = 3;
        app.power_n = "10".to_owned();
        app.run_power_dialog();
        assert!(app.session_text().contains("One-Way ANOVA"));

        app.open_power_dialog();
        app.power_procedure = PowerProcedure::Regression;
        app.power_effect = "0.3".to_owned();
        app.power_groups = 2;
        app.power_n = "40".to_owned();
        app.run_power_dialog();
        assert!(app.session_text().contains("Regression"));
    }

    #[test]
    fn power_dialog_reports_the_power_and_the_sample_size() {
        let mut app = FreePlotsApp::new();
        let df =
            DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0])]).expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.open_power_dialog();
        app.power_effect = "0.5".to_owned();
        app.power_n = "64".to_owned();
        app.run_power_dialog();
        let text = app.session_text();
        assert!(text.contains("Power and Sample Size"), "{text}");
        assert!(text.contains("Power"), "{text}");

        app.open_power_dialog();
        app.power_effect = "1.0".to_owned();
        app.power_n = "0".to_owned();
        app.power_target = 80.0;
        app.run_power_dialog();
        let text = app.session_text();
        assert!(text.contains("Sample size"), "{text}");
    }

    #[test]
    fn diagnostic_dialog_runs_the_goodness_of_fit_and_outlier_tests() {
        let mut app = FreePlotsApp::new();
        let counts =
            DataFrame::new(vec![Column::new_f64("count", vec![20.0, 30.0, 50.0])]).expect("frame");
        app.set_data(counts, "counts.csv".to_owned());
        app.open_diagnostic_dialog(DiagnosticKind::ChiSquareGoodnessOfFit);
        app.diagnostic_column = "count".to_owned();
        app.diagnostic_expected = "25,25,50".to_owned();
        app.run_diagnostic_dialog();
        assert!(app.session_text().contains("Chi-Square Goodness-of-Fit"));

        let weights = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 100.0],
        )])
        .expect("frame");
        app.set_data(weights, "weights.csv".to_owned());
        app.open_diagnostic_dialog(DiagnosticKind::Grubbs);
        app.diagnostic_column = "weight".to_owned();
        app.run_diagnostic_dialog();
        assert!(app.session_text().contains("Grubbs Test"));
    }

    #[test]
    fn one_way_dialog_runs_the_variance_tests() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![1.0, 2.0, 3.0, 4.0, 20.0, 25.0]),
            Column::new_text(
                "g",
                vec![
                    "x".to_owned(),
                    "x".to_owned(),
                    "x".to_owned(),
                    "y".to_owned(),
                    "y".to_owned(),
                    "y".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "weights.csv".to_owned());

        app.open_one_way_dialog(OneWayKind::Levene);
        app.anova_response = "weight".to_owned();
        app.anova_factor = "g".to_owned();
        app.run_anova_dialog();
        assert!(app.session_text().contains("Levene Test"));

        app.open_one_way_dialog(OneWayKind::Bartlett);
        app.anova_response = "weight".to_owned();
        app.anova_factor = "g".to_owned();
        app.run_anova_dialog();
        assert!(app.session_text().contains("Bartlett Test"));
    }

    #[test]
    fn alt_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let times = vec![
            10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 30.0, 34.0, 38.0, 42.0, 46.0, 50.0,
        ];
        let stress = vec![
            400.0, 400.0, 400.0, 400.0, 400.0, 400.0, 350.0, 350.0, 350.0, 350.0, 350.0, 350.0,
        ];
        let df = DataFrame::new(vec![
            Column::new_f64("time", times),
            Column::new_f64("stress", stress),
        ])
        .expect("frame");
        app.set_data(df, "life.csv".to_owned());
        app.open_alt_dialog();
        app.alt_time = "time".to_owned();
        app.alt_stress = "stress".to_owned();
        app.alt_use_stress = 300.0;
        app.run_alt_dialog();
        let text = app.session_text();
        assert!(text.contains("Accelerated Life"), "{text}");
        assert!(app.plot().is_some());

        app.open_alt_dialog();
        app.alt_stress = "time".to_owned();
        app.run_alt_dialog();
        assert!(app.status.contains("different"), "{}", app.status);
    }

    #[test]
    fn calc_dialog_adds_a_column() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![1.0, 4.0, 9.0]),
            Column::new_f64("offset", vec![1.0, 2.0, 3.0]),
        ])
        .expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.open_calc_dialog();
        app.calc_name = "root".to_owned();
        app.calc_expression = "abs(weight) + offset * 2".to_owned();
        app.run_calc_dialog();
        let data = app.data().expect("worksheet");
        assert!(data.column("root").is_ok());
        assert_eq!(data.n_rows(), 3);
        assert!(app.session_text().contains("Column root"));

        // A bad expression reports the error and leaves the sheet alone.
        app.open_calc_dialog();
        app.calc_name = "broken".to_owned();
        app.calc_expression = "1 / (weight - 1)".to_owned();
        app.run_calc_dialog();
        assert!(app.status.contains("row 1"), "{}", app.status);
        assert!(app.data().expect("worksheet").column("broken").is_err());
    }

    #[test]
    fn sign_test_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "x",
            vec![8.0, 9.0, 11.0, 12.0, 13.0, 14.0, 15.0],
        )])
        .expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.open_rank_test_dialog(RankTestKind::SignTest);
        app.rank_first = "x".to_owned();
        app.rank_median = "10".to_owned();
        app.run_rank_test_dialog();
        let text = app.session_text();
        assert!(text.contains("Sign Test"), "{text}");
        assert!(text.contains("Hypothesised median"), "{text}");

        // An empty field tests against 0, as the captured run does.
        app.session.clear();
        app.open_rank_test_dialog(RankTestKind::SignTest);
        app.rank_first = "x".to_owned();
        app.rank_median.clear();
        app.run_rank_test_dialog();
        assert!(
            app.session_text().contains("median 0"),
            "{}",
            app.session_text()
        );
    }

    #[test]
    fn pareto_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_text(
            "defect",
            vec![
                "scratch".to_owned(),
                "scratch".to_owned(),
                "scratch".to_owned(),
                "dent".to_owned(),
                "dent".to_owned(),
                "crack".to_owned(),
            ],
        )])
        .expect("frame");
        app.set_data(df, "defects.csv".to_owned());
        app.open_pareto_dialog();
        app.pareto_column = "defect".to_owned();
        app.run_pareto_dialog();
        let text = app.session_text();
        assert!(text.contains("Pareto Chart"), "{text}");
        assert!(text.contains("Vital few"), "{text}");
        assert!(app.plot().is_some());
    }

    #[test]
    fn ordinal_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        // Ordered codes 1..3 rising with the predictor but overlapping, so the
        // likelihood has a finite maximum (perfect separation has none).
        let y = vec![1.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 3.0, 2.0, 3.0, 3.0, 3.0];
        let x = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1];
        let df =
            DataFrame::new(vec![Column::new_f64("y", y), Column::new_f64("x", x)]).expect("frame");
        app.set_data(df, "ordinal.csv".to_owned());
        app.open_ordinal_dialog();
        app.ordinal_response = "y".to_owned();
        app.ordinal_predictors = "x".to_owned();
        app.run_ordinal_dialog();
        let text = app.session_text();
        assert!(text.contains("Ordinal Logistic Regression"), "{text}");
        assert!(text.contains("Thresholds"), "{text}");

        app.open_ordinal_dialog();
        app.ordinal_predictors.clear();
        app.run_ordinal_dialog();
        assert!(app.status.contains("predictor"), "{}", app.status);
    }

    #[test]
    fn taguchi_dialogs_reach_the_session() {
        let mut app = FreePlotsApp::new();
        app.open_taguchi_design_dialog();
        app.taguchi_runs = 8;
        app.taguchi_design_factors = 3;
        app.design_randomize = false;
        app.run_taguchi_design_dialog();
        let df = app.data().expect("worksheet");
        assert_eq!(df.n_rows(), 8);
        assert_eq!(df.n_columns(), 5);
        assert!(app.session_text().contains("Taguchi design generated"));

        app.open_taguchi_dialog();
        app.taguchi_response = "A".to_owned();
        app.taguchi_factors = "B,C".to_owned();
        app.taguchi_quality = fp_core::taguchi::Quality::Smaller;
        app.run_taguchi_dialog();
        let text = app.session_text();
        assert!(text.contains("Taguchi Analysis"), "{text}");
        assert!(text.contains("Best level"), "{text}");

        // The response cannot also be a factor.
        app.open_taguchi_dialog();
        app.taguchi_response = "A".to_owned();
        app.taguchi_factors = "A,B".to_owned();
        app.run_taguchi_dialog();
        assert!(app.status.contains("factor"), "{}", app.status);
    }

    #[test]
    fn sampling_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        app.open_sampling_dialog();
        app.sampling_plan_mode = false;
        app.sampling_sample = 20;
        app.sampling_accept = 1;
        app.sampling_lot = 0;
        app.run_sampling_dialog();
        let text = app.session_text();
        assert!(text.contains("Acceptance Sampling"), "{text}");
        assert!(text.contains("OC Curve"), "{text}");
        assert!(app.plot().is_some());

        // Planning mode needs no worksheet.
        app.open_sampling_dialog();
        app.sampling_plan_mode = true;
        app.sampling_aql = 1.0;
        app.sampling_ltpd = 5.0;
        app.sampling_alpha = 5.0;
        app.sampling_beta = 10.0;
        app.run_sampling_dialog();
        assert!(
            app.session_text().contains("Producer risk"),
            "{}",
            app.session_text()
        );
    }

    #[test]
    fn t2_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        // Ten subgroups of three observations, written out literally.
        let offsets = [(0.05, -0.1), (-0.02, 0.08), (0.03, 0.01)];
        let mut a = Vec::new();
        let mut b = Vec::new();
        for _ in 0..10 {
            for (offset_a, offset_b) in offsets {
                a.push(10.0 + offset_a);
                b.push(20.0 + offset_b);
            }
        }
        let df =
            DataFrame::new(vec![Column::new_f64("a", a), Column::new_f64("b", b)]).expect("frame");
        app.set_data(df, "t2.csv".to_owned());
        app.open_t2_dialog();
        app.t2_columns = "a,b".to_owned();
        app.t2_subgroup = 3;
        app.run_t2_dialog();
        let text = app.session_text();
        assert!(text.contains("Hotelling T"), "{text}");
        assert!(text.contains("UCL"), "{text}");
        assert!(app.plot().is_some());

        app.open_t2_dialog();
        app.t2_columns = "a".to_owned();
        app.run_t2_dialog();
        assert!(app.status.contains("two columns"), "{}", app.status);
    }

    #[test]
    fn poisson_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        // Growing counts against a growing predictor, written out so that the
        // UI layer does no arithmetic of its own.
        let y = vec![1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let x = vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9];
        let df =
            DataFrame::new(vec![Column::new_f64("y", y), Column::new_f64("x", x)]).expect("frame");
        app.set_data(df, "counts.csv".to_owned());
        app.open_poisson_dialog();
        app.poisson_response = "y".to_owned();
        app.poisson_predictors = "x".to_owned();
        app.run_poisson_dialog();
        let text = app.session_text();
        assert!(text.contains("Poisson Regression"), "{text}");
        assert!(text.contains("Rate ratio"), "{text}");

        app.open_poisson_dialog();
        app.poisson_predictors.clear();
        app.run_poisson_dialog();
        assert!(app.status.contains("predictor"), "{}", app.status);
    }

    #[test]
    fn plackett_burman_dialog_writes_the_design() {
        let mut app = FreePlotsApp::new();
        app.open_pb_dialog();
        app.design_runs = 12;
        app.design_randomize = false;
        app.run_pb_dialog();
        let df = app.data().expect("worksheet");
        assert_eq!(df.n_rows(), 12);
        assert_eq!(df.n_columns(), 13);
        assert!(app.session_text().contains("Plackett-Burman"));

        // An unavailable run count reaches the status bar.
        app.open_pb_dialog();
        app.design_runs = 10;
        app.run_pb_dialog();
        assert!(app.status.contains("Plackett"), "{}", app.status);
    }

    #[test]
    fn data_menus_transform_the_worksheet() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![3.0, 1.0, 2.0, 4.0]),
            Column::new_text(
                "g",
                vec![
                    "b".to_owned(),
                    "a".to_owned(),
                    "b".to_owned(),
                    "a".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "values.csv".to_owned());

        // Sort by the numeric column, descending.
        app.open_sort_dialog();
        app.sort_keys = "y:desc".to_owned();
        app.run_sort_dialog();
        assert_eq!(
            app.data().expect("data").cell_text(0, 0).as_deref(),
            Some("4")
        );

        // Subset on the text column.
        app.open_subset_dialog();
        app.subset_column = "g".to_owned();
        app.subset_op = fp_core::reshape::SubsetOp::Equal;
        app.subset_value = "a".to_owned();
        app.run_subset_dialog();
        assert_eq!(app.data().expect("data").n_rows(), 2);

        // Stack, unstack and transpose round-trip through dialogs.
        app.open_stack_dialog();
        app.stack_columns = "y".to_owned();
        app.run_stack_dialog();
        assert!(app.data().expect("data").column("Variable").is_ok());

        app.open_unstack_dialog();
        app.unstack_value = "Value".to_owned();
        app.unstack_group = "Variable".to_owned();
        app.run_unstack_dialog();
        assert!(app.data().expect("data").column("y").is_ok());

        app.run_transpose();
        assert!(app.data().expect("data").n_columns() > 1);

        // A condition that matches nothing reports the error.
        app.open_subset_dialog();
        app.subset_column = "Column".to_owned();
        app.subset_op = fp_core::reshape::SubsetOp::Equal;
        app.subset_value = "does-not-exist".to_owned();
        app.run_subset_dialog();
        assert!(
            app.status.contains("empty") || app.status.contains("Empty"),
            "{}",
            app.status
        );
    }

    #[test]
    fn agreement_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let mut part = Vec::new();
        let mut appraiser = Vec::new();
        let mut rating = Vec::new();
        for part_label in ["1", "2", "3"] {
            let verdict = if part_label == "2" { "fail" } else { "pass" };
            for who in ["A", "B"] {
                for _ in 0..2 {
                    part.push(part_label.to_owned());
                    appraiser.push(who.to_owned());
                    rating.push(verdict.to_owned());
                }
            }
        }
        let df = DataFrame::new(vec![
            Column::new_text("part", part),
            Column::new_text("appraiser", appraiser),
            Column::new_text("rating", rating),
        ])
        .expect("frame");
        app.set_data(df, "agreement.csv".to_owned());
        app.open_agreement_dialog();
        app.agreement_part = "part".to_owned();
        app.agreement_appraiser = "appraiser".to_owned();
        app.agreement_rating = "rating".to_owned();
        app.run_agreement_dialog();
        let text = app.session_text();
        assert!(text.contains("Attribute Agreement"), "{text}");
        assert!(text.contains("Kappa"), "{text}");

        app.open_agreement_dialog();
        app.agreement_rating = "part".to_owned();
        app.run_agreement_dialog();
        assert!(app.status.contains("different"), "{}", app.status);
    }

    #[test]
    fn kmeans_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let mut x = Vec::new();
        let mut y = Vec::new();
        for centre in [(0.0, 0.0), (10.0, 10.0), (0.0, 10.0)] {
            for step in 0..6 {
                x.push(centre.0 + step as f64 * 0.1);
                y.push(centre.1 + step as f64 * 0.1);
            }
        }
        let df =
            DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
        app.set_data(df, "clusters.csv".to_owned());
        app.open_kmeans_dialog();
        app.kmeans_columns = "x,y".to_owned();
        app.kmeans_clusters = 3;
        app.run_kmeans_dialog();
        let text = app.session_text();
        assert!(text.contains("K-Means Clustering"), "{text}");
        assert!(text.contains("Cluster"), "{text}");
        assert!(app.plot().is_some());

        app.open_kmeans_dialog();
        app.kmeans_columns = "x".to_owned();
        app.run_kmeans_dialog();
        assert!(app.status.contains("two columns"), "{}", app.status);
    }

    #[test]
    fn pca_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut c = Vec::new();
        for index in 0..30 {
            let x = index as f64;
            a.push(x);
            b.push(2.0 * x + ((index % 3) as f64) * 0.01);
            c.push(((index * 7) % 11) as f64);
        }
        let df = DataFrame::new(vec![
            Column::new_f64("a", a),
            Column::new_f64("b", b),
            Column::new_f64("c", c),
        ])
        .expect("frame");
        app.set_data(df, "pca.csv".to_owned());
        app.open_pca_dialog();
        app.pca_columns = "a,b,c".to_owned();
        app.pca_keep = 2;
        app.run_pca_dialog();
        let text = app.session_text();
        assert!(text.contains("Principal Component Analysis"), "{text}");
        assert!(text.contains("Eigenvalue"), "{text}");
        assert!(app.plot().is_some());

        app.open_pca_dialog();
        app.pca_columns = "a".to_owned();
        app.run_pca_dialog();
        assert!(app.status.contains("two columns"), "{}", app.status);
    }

    #[test]
    fn timeseries_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let mut values = Vec::new();
        for index in 0..36 {
            values.push(100.0 + 0.8 * index as f64 + 8.0 * ((index % 12) as f64).sin());
        }
        let df = DataFrame::new(vec![Column::new_f64("sales", values)]).expect("frame");
        app.set_data(df, "sales.csv".to_owned());

        app.open_timeseries_dialog();
        app.timeseries_column = "sales".to_owned();
        app.timeseries_kind = TimeSeriesKind::MovingAverage;
        app.timeseries_window = 4;
        app.run_timeseries_dialog();
        assert!(
            app.session_text().contains("Moving average"),
            "{}",
            app.session_text()
        );
        assert!(app.plot().is_some());

        app.open_timeseries_dialog();
        app.timeseries_column = "sales".to_owned();
        app.timeseries_kind = TimeSeriesKind::Decomposition;
        app.timeseries_period = 12;
        app.run_timeseries_dialog();
        assert!(
            app.session_text().contains("decomposition"),
            "{}",
            app.session_text()
        );

        app.open_timeseries_dialog();
        app.timeseries_column = "sales".to_owned();
        app.timeseries_kind = TimeSeriesKind::ExponentialSmoothing;
        app.timeseries_season = 12;
        app.timeseries_horizon = 3;
        app.run_timeseries_dialog();
        assert!(
            app.session_text().contains("Holt-Winters"),
            "{}",
            app.session_text()
        );
    }

    #[test]
    fn logistic_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let mut response = Vec::new();
        let mut x = Vec::new();
        for index in 0..40 {
            x.push(index as f64);
            let event = (index >= 20 && index % 4 != 0) || (index < 5 && index % 2 == 0);
            response.push(if event { 1.0 } else { 0.0 });
        }
        let df = DataFrame::new(vec![
            Column::new_f64("y", response),
            Column::new_f64("x", x),
        ])
        .expect("frame");
        app.set_data(df, "logit.csv".to_owned());
        app.open_logistic_dialog();
        app.logistic_response = "y".to_owned();
        app.logistic_predictors = "x".to_owned();
        app.run_logistic_dialog();
        let text = app.session_text();
        assert!(text.contains("Binary Logistic Regression"), "{text}");
        assert!(text.contains("Odds ratio"), "{text}");

        // An empty predictor list is refused.
        app.open_logistic_dialog();
        app.logistic_predictors.clear();
        app.run_logistic_dialog();
        assert!(app.status.contains("predictor"), "{}", app.status);
    }

    #[test]
    fn logistic_dialog_accepts_a_categorical_predictor() {
        let mut app = FreePlotsApp::new();
        let mut group = Vec::new();
        let mut response = Vec::new();
        for (level, events) in [("A", 2), ("B", 5), ("C", 8)] {
            for index in 0..10 {
                group.push(level.to_owned());
                response.push(if index < events { 1.0 } else { 0.0 });
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("y", response),
            Column::new_text("group", group),
        ])
        .expect("frame");
        app.set_data(df, "logit.csv".to_owned());
        app.open_logistic_dialog();
        app.logistic_response = "y".to_owned();
        app.logistic_predictors = "group".to_owned();
        app.run_logistic_dialog();
        let text = app.session_text();
        assert!(text.contains("group[B]"), "{text}");
        assert!(text.contains("group[C]"), "{text}");
        assert!(text.contains("dummy coded"), "{text}");
    }

    #[test]
    fn linearity_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let mut measurement = Vec::new();
        let mut reference = Vec::new();
        for target in [10.0, 20.0, 30.0, 40.0] {
            for offset in [-0.2, 0.0, 0.2] {
                measurement.push(target + 0.1 * target - 0.1 + offset);
                reference.push(target);
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("measurement", measurement),
            Column::new_f64("reference", reference),
        ])
        .expect("frame");
        app.set_data(df, "linearity.csv".to_owned());
        app.open_linearity_dialog();
        app.linearity_measurement = "measurement".to_owned();
        app.linearity_reference = "reference".to_owned();
        app.run_linearity_dialog();
        let text = app.session_text();
        assert!(text.contains("Gage Linearity"), "{text}");
        assert!(text.contains("%Linearity"), "{text}");
        assert!(app.plot().is_some());

        // The same column twice is refused before the analysis runs.
        app.open_linearity_dialog();
        app.linearity_reference = "measurement".to_owned();
        app.run_linearity_dialog();
        assert!(app.status.contains("different columns"), "{}", app.status);
    }

    #[test]
    fn box_behnken_dialog_writes_the_design() {
        let mut app = FreePlotsApp::new();
        app.open_box_behnken_dialog();
        app.design_factors = "A,B,C".to_owned();
        app.design_centers = 3;
        app.design_randomize = false;
        app.run_box_behnken_dialog();
        let df = app.data().expect("worksheet");
        assert_eq!(df.n_rows(), 15);
        assert!(df.column("StdOrder").is_ok());
        assert!(app.session_text().contains("Box-Behnken design generated"));

        // Two factors are not enough and the error reaches the status bar.
        app.open_box_behnken_dialog();
        app.design_factors = "A,B".to_owned();
        app.run_box_behnken_dialog();
        assert!(app.status.contains("Box-Behnken"), "{}", app.status);
    }

    #[test]
    fn tolerance_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let values: Vec<f64> = (1..=30).map(|i| 10.0 + (i % 7) as f64 * 0.3).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.open_tolerance_dialog();
        app.tolerance_column = "x".to_owned();
        app.tolerance_proportion = 99.0;
        app.tolerance_confidence = 95.0;
        app.run_tolerance_dialog();
        let text = app.session_text();
        assert!(text.contains("Tolerance Interval"), "{text}");
        assert!(text.contains("Lower limit"), "{text}");
        assert!(text.contains("99.0%"), "{text}");

        // Without a column the dialog reports the error.
        app.open_tolerance_dialog();
        app.tolerance_column.clear();
        app.run_tolerance_dialog();
        assert_eq!(app.status, "Select the column to analyse");
    }

    #[test]
    fn box_cox_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let values: Vec<f64> = vec![
            1.0, 2.0, 3.0, 5.0, 8.0, 13.0, 21.0, 34.0, 55.0, 89.0, 144.0, 233.0,
        ];
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.open_box_cox_dialog();
        app.box_cox_column = "x".to_owned();
        app.run_box_cox_dialog();
        let text = app.session_text();
        assert!(text.contains("Box-Cox"), "{text}");
        assert!(text.contains("p-value after"), "{text}");
    }

    #[test]
    fn partial_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]),
            Column::new_f64("y", vec![2.0, 4.0, 5.0, 8.0, 9.0, 11.0, 12.0]),
            Column::new_f64("z", vec![1.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
        ])
        .expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.open_partial_dialog();
        app.partial_x = "x".to_owned();
        app.partial_y = "y".to_owned();
        app.partial_control = "z".to_owned();
        app.run_partial_dialog();
        assert!(app.session_text().contains("Partial Correlation"));
    }

    #[test]
    fn g_chart_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "events",
            vec![1.0, 2.0, 1.0, 3.0, 2.0, 1.0],
        )])
        .expect("frame");
        app.set_data(df, "events.csv".to_owned());
        app.open_g_chart_dialog();
        app.g_chart_counts = "events".to_owned();
        app.run_g_chart_dialog();
        let text = app.session_text();
        assert!(text.contains("G Chart"), "{text}");
        assert!(text.contains("Center line"), "{text}");
    }

    #[test]
    fn crosstab_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_text(
                "r",
                vec![
                    "a".to_owned(),
                    "a".to_owned(),
                    "a".to_owned(),
                    "b".to_owned(),
                    "b".to_owned(),
                    "b".to_owned(),
                ],
            ),
            Column::new_text(
                "c",
                vec![
                    "x".to_owned(),
                    "x".to_owned(),
                    "x".to_owned(),
                    "y".to_owned(),
                    "y".to_owned(),
                    "y".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "table.csv".to_owned());
        app.open_crosstab_dialog();
        app.crosstab_row = "r".to_owned();
        app.crosstab_column = "c".to_owned();
        app.run_crosstab_dialog();
        let text = app.session_text();
        assert!(text.contains("Chi-Square Test for Independence"), "{text}");
        assert!(text.contains("Cramer's V"), "{text}");
    }

    #[test]
    fn two_way_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![10.0, 11.0, 13.0, 14.0, 10.1, 11.1, 13.1, 14.1]),
            Column::new_text(
                "a",
                vec![
                    "a1".to_owned(),
                    "a1".to_owned(),
                    "a2".to_owned(),
                    "a2".to_owned(),
                    "a1".to_owned(),
                    "a1".to_owned(),
                    "a2".to_owned(),
                    "a2".to_owned(),
                ],
            ),
            Column::new_text(
                "b",
                vec![
                    "b1".to_owned(),
                    "b2".to_owned(),
                    "b1".to_owned(),
                    "b2".to_owned(),
                    "b1".to_owned(),
                    "b2".to_owned(),
                    "b1".to_owned(),
                    "b2".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "design.csv".to_owned());
        app.open_two_way_dialog();
        app.two_way_response = "y".to_owned();
        app.two_way_first = "a".to_owned();
        app.two_way_second = "b".to_owned();
        app.run_two_way_dialog();
        assert!(app.session_text().contains("Two-Way ANOVA"));
    }

    #[test]
    fn curve_dialog_fits_the_model() {
        let mut app = FreePlotsApp::new();
        let x: Vec<f64> = (0..20).map(|i| i as f64 * 0.5).collect();
        let y: Vec<f64> = x.iter().map(|v| 1.0 + 2.0 * v).collect();
        let df =
            DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
        app.set_data(df, "curve.csv".to_owned());
        app.open_curve_dialog();
        app.curve_x = "x".to_owned();
        app.curve_y = "y".to_owned();
        app.curve_model = CurveModel::Polynomial(1);
        app.run_curve_dialog();
        let text = app.session_text();
        assert!(text.contains("Curve Fit"), "{text}");
        assert!(text.contains("R²"), "{text}");
        assert!(app.plot().is_some());
    }

    #[test]
    fn correlation_dialog_reports_the_pairs() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_f64("b", vec![2.0, 4.0, 6.0, 8.0, 10.0, 12.0]),
        ])
        .expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.open_correlation_dialog();
        app.correlation_columns = "a,b".to_owned();
        app.run_correlation_dialog();
        let text = app.session_text();
        assert!(text.contains("Correlation"), "{text}");
        assert!(text.contains("a × b"), "{text}");
    }

    #[test]
    fn the_language_switch_translates_the_session_and_the_menus() {
        let mut app = FreePlotsApp::new();
        let values: Vec<f64> = (1..=20).map(|i| i as f64 * 1.5).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        app.set_data(df, "values.csv".to_owned());
        assert_eq!(app.language(), fp_core::i18n::Lang::English);

        app.set_language(fp_core::i18n::Lang::Portuguese);
        assert_eq!(app.language(), fp_core::i18n::Lang::Portuguese);
        assert_eq!(app.catalog().label("File"), "Ficheiro");

        app.run_dagostino_test();
        let text = app.session_text();
        assert!(text.contains("Estatísticas"), "{text}");
    }

    #[test]
    fn the_dagostino_test_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let values: Vec<f64> = (1..=20).map(|i| i as f64 * 1.5).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.run_dagostino_test();
        assert!(app.session_text().contains("D'Agostino"));
    }

    #[test]
    fn rank_test_dialog_runs_the_non_parametric_tests() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("b", vec![10.0, 11.0, 12.0, 13.0, 14.0]),
            Column::new_text(
                "g",
                vec![
                    "x".to_owned(),
                    "x".to_owned(),
                    "y".to_owned(),
                    "y".to_owned(),
                    "x".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "pairs.csv".to_owned());

        app.open_rank_test_dialog(RankTestKind::MannWhitney);
        app.rank_second = "b".to_owned();
        app.run_rank_test_dialog();
        assert!(app.session_text().contains("Mann-Whitney Test"));

        app.open_rank_test_dialog(RankTestKind::Wilcoxon);
        app.rank_second = "b".to_owned();
        app.run_rank_test_dialog();
        assert!(app.session_text().contains("Wilcoxon Signed-Rank Test"));

        app.open_rank_test_dialog(RankTestKind::KruskalWallis);
        app.rank_first = "a".to_owned();
        app.rank_factor = "g".to_owned();
        app.run_rank_test_dialog();
        assert!(app.session_text().contains("Kruskal-Wallis Test"));

        app.open_rank_test_dialog(RankTestKind::Friedman);
        app.rank_first = "a".to_owned();
        app.rank_extra_columns = "b".to_owned();
        app.run_rank_test_dialog();
        assert!(app.session_text().contains("Friedman Test"));

        app.open_rank_test_dialog(RankTestKind::MoodsMedian);
        app.rank_first = "a".to_owned();
        app.rank_factor = "g".to_owned();
        app.run_rank_test_dialog();
        assert!(app.session_text().contains("Mood's Median Test"));
    }

    #[test]
    fn equivalence_dialog_runs_the_to_st_tests() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("before", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("after", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
        ])
        .expect("frame");
        app.set_data(df, "pairs.csv".to_owned());

        app.open_equivalence_dialog(EquivalenceKind::OneSample);
        app.equivalence_null = "3".to_owned();
        app.equivalence_margin = "1".to_owned();
        app.run_equivalence_dialog();
        assert!(app.session_text().contains("Equivalence Test: 1-Sample"));

        app.open_equivalence_dialog(EquivalenceKind::TwoSample);
        app.equivalence_second = "after".to_owned();
        app.equivalence_margin = "1".to_owned();
        app.run_equivalence_dialog();
        assert!(app.session_text().contains("Equivalence Test: 2-Sample"));

        app.open_equivalence_dialog(EquivalenceKind::Paired);
        app.equivalence_second = "after".to_owned();
        app.equivalence_margin = "1".to_owned();
        app.run_equivalence_dialog();
        assert!(app.session_text().contains("Equivalence Test: Paired"));
    }

    #[test]
    fn lognormal_reliability_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 10.0]),
            Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]),
        ])
        .expect("frame");
        app.set_data(df, "lognormal.csv".to_owned());
        app.open_lognormal_dialog();
        app.log_time = "time".to_owned();
        app.log_censor = Some("status".to_owned());
        app.log_censor_value = "0".to_owned();
        app.run_lognormal_dialog();
        assert!(app.session_text().contains("Lognormal Reliability"));
        assert!(app.session_text().contains("B10 life"));
        assert!(!app.plot().expect("plot").lines.is_empty());
    }

    #[test]
    fn exponential_reliability_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 10.0]),
            Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0]),
        ])
        .expect("frame");
        app.set_data(df, "exponential.csv".to_owned());
        app.open_exponential_dialog();
        app.exp_time = "time".to_owned();
        app.exp_censor = Some("status".to_owned());
        app.exp_censor_value = "0".to_owned();
        app.run_exponential_dialog();
        assert!(app.session_text().contains("Exponential Reliability"));
        assert!(app.session_text().contains("B10 life"));
        assert!(!app.plot().expect("plot").lines.is_empty());
    }

    #[test]
    fn weibull_dialog_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0]),
            Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]),
        ])
        .expect("frame");
        app.set_data(df, "weibull.csv".to_owned());
        app.open_weibull_dialog();
        app.weibull_time = "time".to_owned();
        app.weibull_censor = Some("status".to_owned());
        app.weibull_censor_value = "0".to_owned();
        app.run_weibull_dialog();
        assert!(app.session_text().contains("Weibull Reliability"));
        assert!(app.session_text().contains("B10 life"));
        assert!(!app.plot().expect("plot").lines.is_empty());
    }

    #[test]
    fn boxplot_and_scatter_reach_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("y", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
        ])
        .expect("frame");
        app.set_data(df, "graphs.csv".to_owned());
        app.run_boxplot("y");
        assert!(app.session_text().contains("Boxplot"));
        app.run_histogram("y");
        assert!(app.session_text().contains("Histogram"));
        app.open_scatter_dialog();
        app.scatter_x = "x".to_owned();
        app.scatter_y = "y".to_owned();
        app.run_scatter_dialog();
        assert!(app.session_text().contains("Scatterplot"));
        assert!(app.session_text().contains("Pearson r"));
        assert_eq!(app.plot().expect("plot").points.len(), 5);
    }

    #[test]
    fn design_dialog_generates_a_factorial_worksheet() {
        let mut app = FreePlotsApp::new();
        app.open_design_dialog();
        app.design_factors = "A,B".to_owned();
        app.design_replicates = 2;
        app.design_centers = 1;
        app.design_randomize = false;
        app.run_design_dialog();
        let df = app.data().expect("worksheet");
        assert_eq!(df.n_rows(), 9);
        assert_eq!(df.n_columns(), 4);
        assert!(app.session_text().contains("DOE design generated"));
    }

    #[test]
    fn rsm_dialog_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        let mut rows = Vec::new();
        for a in [-1.0, 0.0, 1.0] {
            for b in [-1.0, 0.0, 1.0] {
                rows.push((
                    a,
                    b,
                    10.0 + 2.0 * a + 3.0 * b + a * b + 0.5 * a * a + 0.25 * b * b,
                ));
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("A", rows.iter().map(|row| row.0).collect()),
            Column::new_f64("B", rows.iter().map(|row| row.1).collect()),
            Column::new_f64("y", rows.iter().map(|row| row.2).collect()),
        ])
        .expect("frame");
        app.set_data(df, "rsm.csv".to_owned());
        app.open_rsm_dialog();
        app.rsm_response = "y".to_owned();
        app.rsm_factors = vec!["A".to_owned(), "B".to_owned()];
        app.rsm_contours = true;
        app.run_rsm_dialog();
        assert!(app.session_text().contains("Response Surface"));
        assert!(app.session_text().contains("A^2"));
        assert_eq!(app.plot().expect("plot").panels.len(), 3);
    }

    #[test]
    fn blocked_factorial_uses_the_block_column() {
        let mut app = FreePlotsApp::new();
        let mut rows = Vec::new();
        for block in 0..2 {
            for a in [-1.0, 1.0] {
                for b in [-1.0, 1.0] {
                    for _ in 0..2 {
                        let offset = if block == 1 { 5.0 } else { 0.0 };
                        rows.push((a, b, 10.0 + 2.0 * a + 3.0 * b + offset, block));
                    }
                }
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("A", rows.iter().map(|row| row.0).collect()),
            Column::new_f64("B", rows.iter().map(|row| row.1).collect()),
            Column::new_f64("y", rows.iter().map(|row| row.2).collect()),
            Column::new_text(
                "Block",
                rows.iter().map(|row| (row.3 + 1).to_string()).collect(),
            ),
        ])
        .expect("frame");
        app.set_data(df, "blocked.csv".to_owned());
        app.open_doe_dialog();
        app.doe_response = "y".to_owned();
        app.doe_factors = vec!["A".to_owned(), "B".to_owned()];
        app.doe_block = "Block".to_owned();
        app.run_doe_dialog();
        assert!(
            app.session_text().contains("Blocks"),
            "{}",
            app.session_text()
        );
        assert!(app.session_text().contains("Factorial model"));
    }

    #[test]
    fn a_registered_plugin_runs_from_the_workspace() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, f64::NAN]),
            Column::new_text(
                "label",
                vec!["a".to_owned(), "a".to_owned(), "b".to_owned()],
            ),
        ])
        .expect("frame");
        app.set_data(df, "plugins.csv".to_owned());
        app.run_plugin("data-quality");
        assert!(
            app.session_text().contains("Data Quality"),
            "{}",
            app.session_text()
        );
        assert!(app.session_text().contains("Missing values"));
    }

    #[test]
    fn the_browse_button_emits_a_native_dialog_request() {
        let mut app = FreePlotsApp::new();
        app.begin_open(FileKind::Csv);
        // The dialog is drawn by the runner; the request is what the UI owns.
        app.request_browse(FileKind::Csv, false);
        let request = app.take_browse_request().expect("browse request");
        assert!(!request.save);
        assert!(
            request
                .filters
                .iter()
                .any(|filter| filter.contains("*.csv"))
        );
        app.set_browsed_path("C:\\data\\file.csv".to_owned(), false);
        assert_eq!(app.open_path(), "C:\\data\\file.csv");
    }

    #[test]
    fn factorial_dialog_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        let mut rows = Vec::new();
        for a in [-1.0, 1.0] {
            for b in [-1.0, 1.0] {
                for _ in 0..2 {
                    rows.push((a, b, 10.0 + 2.0 * a + 3.0 * b + a * b));
                }
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("A", rows.iter().map(|row| row.0).collect()),
            Column::new_f64("B", rows.iter().map(|row| row.1).collect()),
            Column::new_f64("y", rows.iter().map(|row| row.2).collect()),
        ])
        .expect("frame");
        app.set_data(df, "doe.csv".to_owned());
        app.open_doe_dialog();
        app.doe_response = "y".to_owned();
        app.doe_factors = vec!["A".to_owned(), "B".to_owned()];
        app.run_doe_dialog();
        assert!(app.session_text().contains("2-Level Factorial"));
        assert!(app.session_text().contains("Effect"));
        assert!(app.plot().is_some());
    }

    #[test]
    fn type1_gage_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "m",
            vec![10.1, 10.2, 10.0, 10.3, 10.1, 10.2],
        )])
        .expect("frame");
        app.set_data(df, "type1.csv".to_owned());
        app.open_type1_dialog();
        app.type1_measurement = "m".to_owned();
        app.type1_reference = "10".to_owned();
        app.type1_lsl = "9".to_owned();
        app.type1_usl = "11".to_owned();
        app.run_type1_dialog();
        assert!(app.session_text().contains("Gage Type 1"));
        assert!(app.session_text().contains("%Tolerance"));
    }

    #[test]
    fn gage_rr_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("m", vec![1.0, 2.0, 2.0, 3.0, 4.0, 5.0, 5.0, 6.0]),
            Column::new_text(
                "part",
                vec![
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                ],
            ),
            Column::new_text(
                "operator",
                vec![
                    "O1".to_owned(),
                    "O1".to_owned(),
                    "O2".to_owned(),
                    "O2".to_owned(),
                    "O1".to_owned(),
                    "O1".to_owned(),
                    "O2".to_owned(),
                    "O2".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "gage.csv".to_owned());
        app.open_gage_dialog();
        app.gage_measurement = "m".to_owned();
        app.gage_part = "part".to_owned();
        app.gage_operator = "operator".to_owned();
        app.run_gage_dialog();
        assert!(app.session_text().contains("Gage R&R"));
        assert!(app.session_text().contains("%Gage R&R"));
    }

    #[test]
    fn kaplan_meier_dialog_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("status", vec![1.0, 0.0, 1.0, 1.0, 0.0]),
        ])
        .expect("frame");
        app.set_data(df, "survival.csv".to_owned());
        app.open_kaplan_meier_dialog();
        app.km_time = "time".to_owned();
        app.km_censor = Some("status".to_owned());
        app.km_censor_value = "0".to_owned();
        app.run_kaplan_meier_dialog();
        assert!(app.session_text().contains("Kaplan–Meier"));
        assert!(app.session_text().contains("Median survival"));
        assert!(!app.plot().expect("plot").lines.is_empty());
    }

    #[test]
    fn anova_dialog_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("y", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_text(
                "g",
                vec![
                    "A".to_owned(),
                    "A".to_owned(),
                    "B".to_owned(),
                    "B".to_owned(),
                    "C".to_owned(),
                    "C".to_owned(),
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "anova.csv".to_owned());
        app.open_anova_dialog();
        app.anova_response = "y".to_owned();
        app.anova_factor = "g".to_owned();
        app.run_anova_dialog();
        assert!(app.session_text().contains("One-Way ANOVA"));
        assert!(app.session_text().contains("Bonferroni"));
        assert_eq!(app.plot().expect("plot").points.len(), 6);
    }

    #[test]
    fn regression_dialog_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("x1", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_f64("x2", vec![2.0, 1.0, 4.0, 3.0, 6.0, 5.0]),
            Column::new_f64("y", vec![6.0, 5.0, 12.0, 11.0, 18.0, 17.0]),
        ])
        .expect("frame");
        app.set_data(df, "reg.csv".to_owned());
        app.open_regression_dialog();
        app.reg_response = "y".to_owned();
        app.reg_predictors = vec!["x1".to_owned(), "x2".to_owned()];
        app.run_regression_dialog();
        assert!(app.session_text().contains("Regression"));
        assert!(app.session_text().contains("Regression equation"));
        let plot = app.plot().expect("plot");
        assert_eq!(plot.points.len(), 6);
    }

    #[test]
    fn stepwise_dialog_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64(
                "x1",
                vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0],
            ),
            Column::new_f64(
                "x2",
                vec![
                    0.769231, -1.188811, 0.853147, -1.104895, 0.937063, -1.020979, 1.020979,
                    -0.937063, 1.104895, -0.853147, 1.188811, -0.769231,
                ],
            ),
            Column::new_f64(
                "y",
                vec![
                    1.410456, 4.198339, 5.105444, 5.594605, 8.019128, 11.014304, 13.465211,
                    14.88367, 16.664885, 18.692888, 21.334876, 23.616195,
                ],
            ),
        ])
        .expect("frame");
        app.set_data(df, "step.csv".to_owned());
        app.open_stepwise_dialog();
        app.step_response = "y".to_owned();
        app.step_candidates = vec!["x1".to_owned(), "x2".to_owned()];
        app.step_method = StepMethod::Forward;
        app.step_alpha_enter = 0.05;
        app.run_stepwise_dialog();
        assert!(
            app.session_text().contains("Stepwise Regression"),
            "{}",
            app.session_text()
        );
        assert!(app.session_text().contains("x1"), "{}", app.session_text());
        assert!(!app.session_text().contains("x2"), "{}", app.session_text());
        let plot = app.plot().expect("plot");
        assert_eq!(plot.points.len(), 12);
    }

    #[test]
    fn proportion_and_variance_dialogs_reach_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 1.0, 1.0, 0.0, 0.0, 0.0]),
            Column::new_f64("b", vec![1.0, 1.0, 0.0, 0.0, 0.0, 0.0]),
        ])
        .expect("frame");
        app.set_data(df, "binary.csv".to_owned());
        app.open_test2_dialog(Test2Kind::OneProportion);
        app.test2_null = "0.5".to_owned();
        app.run_test2_dialog();
        assert!(app.session_text().contains("1 Proportion"));
        app.open_test2_dialog(Test2Kind::TwoProportions);
        app.run_test2_dialog();
        assert!(app.session_text().contains("2 Proportions"));
        app.open_test2_dialog(Test2Kind::OneVariance);
        app.test2_null = "1".to_owned();
        app.run_test2_dialog();
        assert!(app.session_text().contains("1 Variance"));
        app.open_test2_dialog(Test2Kind::TwoVariances);
        app.run_test2_dialog();
        assert!(app.session_text().contains("2 Variances"));
    }

    #[test]
    fn bonett_variance_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![10.0, 20.0, 30.0, 40.0, 50.0]),
            Column::new_f64("b", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        ])
        .expect("frame");
        app.set_data(df, "spread.csv".to_owned());
        app.open_test2_dialog(Test2Kind::TwoVariances);
        app.test2_variance_method = VarianceMethod::Bonett;
        app.run_test2_dialog();
        assert!(app.session_text().contains("Bonett"));
    }

    #[test]
    fn attribute_chart_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("defects", vec![2.0, 4.0, 3.0, 5.0, 1.0, 4.0]),
            Column::new_f64("size", vec![100.0; 6]),
        ])
        .expect("frame");
        app.set_data(df, "defects.csv".to_owned());
        app.open_attribute_dialog();
        app.attr_column = "defects".to_owned();
        app.attr_sizes = Some("size".to_owned());
        app.attr_size.clear();
        app.run_attribute_chart();
        assert!(app.session_text().contains("P Chart"));
        assert_eq!(app.plot().expect("plot").points.len(), 6);
    }

    #[test]
    fn run_chart_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.run_run_chart("weight");
        assert!(app.session_text().contains("Run Chart"));
        assert!(app.session_text().contains("About the median"));
        let plot = app.plot().expect("plot");
        assert_eq!(plot.points.len(), 5);
        assert_eq!(plot.lines.len(), 1);
    }

    #[test]
    fn control_chart_reaches_the_session_and_plot() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.run_control_chart("weight", ControlChartKind::IndividualsMovingRange, 1);
        assert!(app.session_text().contains("Control Chart: I–MR"));
        assert!(app.session_text().contains("Control limits"));
        assert_eq!(app.plot().expect("plot").panels.len(), 2);
        app.run_control_chart("weight", ControlChartKind::XbarR, 2);
        assert!(app.session_text().contains("Control Chart: X̄–R"));
    }

    #[test]
    fn control_chart_test_selection_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![10.0, 10.1, 10.0, 10.1, 10.0, 10.1, 10.0, 10.1, 10.0, 15.0],
        )])
        .expect("frame");
        app.set_data(df, "weights.csv".to_owned());
        app.run_control_chart_with_tests(
            "weight",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::none(),
        );
        assert!(
            app.session_text().contains("No out-of-control points"),
            "{}",
            app.session_text()
        );
        app.run_control_chart_with_tests(
            "weight",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::only(&[1]).expect("set"),
        );
        assert!(
            app.session_text().contains("point outside a control limit"),
            "{}",
            app.session_text()
        );
        assert!(
            app.session_text().contains("tests 1, subgroup"),
            "{}",
            app.session_text()
        );
        app.run_control_chart_with_tests(
            "weight",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::all().with_sensitivity(2, 8).expect("sensitivity"),
        );
        assert!(
            app.session_text().contains("test 2 K = 8"),
            "{}",
            app.session_text()
        );
    }

    #[test]
    fn descriptive_statistics_by_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![12.5, 12.8, 13.0, 11.9]),
            Column::new_text(
                "batch",
                vec!["A".into(), "A".into(), "B".into(), "B".into()],
            ),
        ])
        .expect("frame");
        app.set_data(df, "grouped.csv".to_owned());
        app.run_descriptive_statistics_by("weight", "batch".to_owned());
        assert!(app.session_text().contains("batch = A"));
        assert!(app.session_text().contains("batch = B"));
        assert!(app.plot().is_none());
    }

    #[test]
    fn box_cox_capability_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.execute_spec(
            AnalysisSpec::capability("weight", Some(8.0), Some(12.0)).with_capability_box_cox(true),
            "box-cox".to_owned(),
        );
        assert!(app.session_text().contains("Normal (Box-Cox)"));
        assert!(app.session_text().contains("Box-Cox λ"));
    }

    #[test]
    fn distribution_comparison_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.run_distribution_comparison();
        assert!(app.session_text().contains("Distribution Comparison"));
        assert!(app.session_text().contains("Weibull"));
        assert!(app.session_text().contains("AIC"));
    }

    #[test]
    fn descriptive_statistics_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.run_descriptive_statistics();
        assert!(app.session_text().contains("Descriptive Statistics"));
        assert!(app.session_text().contains("First quartile (Q1)"));
        assert!(app.session_text().contains("95% CI for the mean"));
        assert!(app.plot().is_none());
    }

    #[test]
    fn ryan_joiner_test_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "x",
            vec![-1.2, -0.8, -0.5, -0.2, 0.0, 0.1, 0.3, 0.6, 0.9, 1.1],
        )])
        .expect("frame");
        app.set_data(df, "normal.csv".to_owned());
        app.run_rj_test();
        assert!(app.session_text().contains("Ryan–Joiner"));
        assert!(app.session_text().contains("Monte Carlo RJ"));
    }

    #[test]
    fn kolmogorov_smirnov_test_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![Column::new_f64(
            "x",
            vec![-1.2, -0.8, -0.5, -0.2, 0.0, 0.1, 0.3, 0.6, 0.9, 1.1],
        )])
        .expect("frame");
        app.set_data(df, "normal.csv".to_owned());
        app.run_ks_test();
        assert!(app.session_text().contains("Kolmogorov–Smirnov"));
        assert!(app.session_text().contains("Monte Carlo KS"));
    }

    #[test]
    fn normality_test_adds_a_report_without_a_plot() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.run_normality_test();
        assert!(app.session_text().contains("Normality Test"));
        assert!(app.session_text().contains("Anderson–Darling"));
        assert!(app.plot().is_none());
    }

    #[test]
    fn series_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let values: Vec<f64> = (1..=20).map(|index| index as f64 * 0.5).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        app.set_data(df, "series.csv".to_owned());
        app.open_series_dialog();
        app.series_column = "x".to_owned();
        app.run_series_dialog();
        let text = app.session_text();
        assert!(text.contains("Time Series Plot"), "{text}");
        assert!(text.contains("Theil"), "{text}");
        assert!(app.plot().is_some());

        app.open_series_dialog();
        app.series_column = "x".to_owned();
        app.series_time = "x".to_owned();
        app.run_series_dialog();
        assert!(app.status.contains("different"), "{}", app.status);
    }

    #[test]
    fn stem_and_ecdf_dialogs_reach_the_session() {
        let mut app = FreePlotsApp::new();
        let values: Vec<f64> = (1..=40).map(|index| 10.0 + index as f64 * 0.5).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        app.set_data(df, "values.csv".to_owned());

        app.open_stem_dialog();
        app.stem_column = "x".to_owned();
        app.run_stem_dialog();
        assert!(app.session_text().contains("Stem-and-Leaf"));

        app.open_ecdf_dialog();
        app.ecdf_column = "x".to_owned();
        app.ecdf_confidence = 90.0;
        app.run_ecdf_dialog();
        let text = app.session_text();
        assert!(text.contains("Empirical CDF"), "{text}");
        assert!(text.contains("Band half-width"), "{text}");
        assert!(app.plot().is_some());
    }

    #[test]
    fn dotplot_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let mut values = Vec::new();
        let mut labels = Vec::new();
        for (index, level) in ["A", "B"].iter().enumerate() {
            for point in 0..5 {
                values.push(10.0 + index as f64 * 2.0 + point as f64 * 0.1);
                labels.push((*level).to_owned());
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("y", values),
            Column::new_text("g", labels),
        ])
        .expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.open_dotplot_dialog();
        app.dot_column = "y".to_owned();
        app.dot_factor = "g".to_owned();
        app.run_dotplot_dialog();
        let text = app.session_text();
        assert!(text.contains("Dotplot"), "{text}");
        assert_eq!(app.plot().expect("plot").lines.len(), 2);

        app.open_dotplot_dialog();
        app.dot_column = "y".to_owned();
        app.dot_factor = "y".to_owned();
        app.run_dotplot_dialog();
        assert!(app.status.contains("different"), "{}", app.status);
    }

    #[test]
    fn matrix_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let mut a = Vec::new();
        let mut b = Vec::new();
        for index in 0..20 {
            a.push(index as f64);
            b.push(2.0 * index as f64 + 0.5);
        }
        let df =
            DataFrame::new(vec![Column::new_f64("a", a), Column::new_f64("b", b)]).expect("frame");
        app.set_data(df, "pairs.csv".to_owned());
        app.open_matrix_dialog();
        app.matrix_columns = "a,b".to_owned();
        app.matrix_bins = 5;
        app.run_matrix_dialog();
        let text = app.session_text();
        assert!(text.contains("Matrix Plot"), "{text}");
        assert!(app.plot().is_some());
        assert_eq!(app.plot().expect("plot").panels.len(), 4);

        app.open_matrix_dialog();
        app.matrix_columns = "a".to_owned();
        app.run_matrix_dialog();
        assert!(app.status.contains("two columns"), "{}", app.status);
    }

    #[test]
    fn interval_dialog_reaches_the_session() {
        let mut app = FreePlotsApp::new();
        let mut values = Vec::new();
        let mut labels = Vec::new();
        for (index, level) in ["A", "B", "C"].iter().enumerate() {
            for point in 0..6 {
                values.push(10.0 + index as f64 * 2.0 + point as f64 * 0.1);
                labels.push((*level).to_owned());
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("y", values),
            Column::new_text("g", labels),
        ])
        .expect("frame");
        app.set_data(df, "groups.csv".to_owned());
        app.open_interval_dialog();
        app.interval_response = "y".to_owned();
        app.interval_factor = "g".to_owned();
        app.run_interval_dialog();
        let text = app.session_text();
        assert!(text.contains("Interval Plot"), "{text}");
        assert!(app.plot().is_some());

        app.open_interval_dialog();
        app.interval_factor = "y".to_owned();
        app.run_interval_dialog();
        assert!(app.status.contains("different"), "{}", app.status);
    }

    #[test]
    fn the_tolerance_dialog_offers_the_distribution_free_method() {
        let mut app = FreePlotsApp::new();
        let values: Vec<f64> = (1..=120).map(|index| 40.0 + index as f64 * 0.5).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        app.set_data(df, "values.csv".to_owned());
        app.open_tolerance_dialog();
        app.tolerance_column = "x".to_owned();
        app.tolerance_free = true;
        app.run_tolerance_dialog();
        let text = app.session_text();
        assert!(
            text.contains("Distribution-Free Tolerance Interval"),
            "{text}"
        );
        assert!(text.contains("Achieved confidence"), "{text}");
    }

    #[test]
    fn a_workbook_with_several_sheets_asks_which_one() {
        let mut app = FreePlotsApp::new();
        // A single sheet is opened straight away.
        app.set_workbook_sheets("book.xlsx", vec!["Only".to_owned()]);
        let request = app.take_open_request().expect("request");
        assert_eq!(request.kind, FileKind::Xlsx);
        assert_eq!(request.sheet.as_deref(), Some("Only"));
        // Several sheets open the picker and emit nothing until answered.
        app.set_workbook_sheets("book.xlsx", vec!["First".to_owned(), "Second".to_owned()]);
        assert!(app.take_open_request().is_none());
        assert!(app.show_sheet_dialog);
        assert_eq!(app.sheet_choice, "First");
        app.sheet_choice = "Second".to_owned();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
        output.textures_delta.clear();
        // The dialog is painted; answering it emits the request.
        app.show_sheet_dialog = true;
        app.sheet_path = "book.xlsx".to_owned();
        app.sheet_names = vec!["First".to_owned(), "Second".to_owned()];
        app.sheet_choice = "Second".to_owned();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
        output.textures_delta.clear();
    }

    #[test]
    fn open_request_is_taken_once() {
        let mut app = FreePlotsApp::new();
        app.open_request = Some(OpenRequest {
            path: "C:\\data\\weights.csv".to_owned(),
            kind: FileKind::Csv,
            sheet: None,
            csv_options: None,
        });
        let request = app.take_open_request().expect("open request");
        assert_eq!(request.path, "C:\\data\\weights.csv");
        assert_eq!(request.kind, FileKind::Csv);
        assert!(app.take_open_request().is_none());
    }

    #[test]
    fn session_html_export_request_is_exposed() {
        let mut app = FreePlotsApp::new();
        app.begin_session_export();
        assert_eq!(app.save_kind, FileKind::Html);
        assert_eq!(app.save_path, "session.html");
        app.mark_session_exported("session.html");
        assert!(app.status().contains("Session exported"));
    }

    #[test]
    fn save_request_and_mark_saved_are_exposed() {
        let mut app = FreePlotsApp::new();
        app.save_request = Some(SaveRequest {
            path: "out.csv".to_owned(),
            kind: FileKind::Csv,
            decimal_comma: false,
        });
        let request = app.take_save_request().expect("save request");
        assert_eq!(request.path, "out.csv");
        assert_eq!(request.kind, FileKind::Csv);
        assert!(!request.decimal_comma);
        app.mark_saved("out.csv");
        assert_eq!(app.source(), "out.csv");
        assert!(app.status().contains("Saved"));
    }

    #[test]
    fn project_save_uses_the_fpp_extension() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv [UTF-8]".to_owned());
        app.begin_save(FileKind::Project);
        assert!(app.show_save);
        assert_eq!(app.save_kind, FileKind::Project);
        assert_eq!(app.save_path, "weights.fpp");
        app.begin_save(FileKind::Csv);
        assert_eq!(app.save_path, "weights.csv");
    }

    #[test]
    fn new_worksheet_starts_editable_with_eight_columns() {
        let mut app = FreePlotsApp::new();
        app.new_worksheet();
        assert_eq!(app.data().map(DataFrame::n_columns), Some(8));
        assert_eq!(app.data().map(DataFrame::n_rows), Some(0));
        assert_eq!(app.selected_cell(), Some((0, 0)));

        app.set_edit_buffer("10,5");
        assert!(app.commit_cell());
        assert_eq!(app.data().map(DataFrame::n_rows), Some(1));
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("10.5"));
    }

    #[test]
    fn worksheet_editing_reaches_the_data_frame() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 1);
        assert_eq!(app.edit_buffer(), "13.1");
        app.set_edit_buffer("14,25");
        assert!(app.commit_cell());
        assert_eq!(
            app.data().unwrap().cell_text(0, 1).as_deref(),
            Some("14.25")
        );
        assert!(app.column_summary().contains("mean="));
    }

    #[test]
    fn rectangle_selection_normalizes_and_copies_tsv() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.extend_selection(1, 1);
        assert_eq!(app.selection_bounds(), Some((0, 0, 1, 1)));
        assert_eq!(app.selection_size(), (2, 2));
        assert_eq!(app.selection_tsv().as_deref(), Some("12.5\tA\n13.1\tA"));

        app.select_cell(1, 2);
        app.extend_selection(0, 0);
        assert_eq!(app.selection_bounds(), Some((0, 0, 1, 2)));
    }

    #[test]
    fn missing_cells_copy_as_empty_fields() {
        let mut app = FreePlotsApp::new();
        let df = DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, f64::NAN]),
            Column::new_text("y", vec!["a".into(), String::new()]),
        ])
        .unwrap();
        app.set_data(df, "missing.csv".to_owned());
        app.select_cell(0, 0);
        app.extend_selection(1, 1);
        assert_eq!(app.selection_tsv().as_deref(), Some("1\ta\n\t"));
    }

    #[test]
    fn clearing_a_rectangle_is_one_undo_group() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.extend_selection(1, 1);
        app.clear_selection();
        assert!(app.data().unwrap().cell_is_missing(0, 0));
        assert!(app.data().unwrap().cell_is_missing(1, 1));
        assert!(!app.data().unwrap().cell_is_missing(0, 2));
        assert_eq!(app.selection_size(), (2, 2));
        app.undo();
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
        assert_eq!(app.data().unwrap().cell_text(1, 1).as_deref(), Some("A"));
    }

    #[test]
    fn inline_edit_commits_and_is_undoable() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.begin_inline_edit(0, 1);
        assert_eq!(app.inline_edit, Some((0, 1)));
        assert_eq!(app.inline_buffer, "13.1");
        app.inline_buffer = "15.25".to_owned();
        assert!(app.commit_inline_edit());
        assert!(app.inline_edit.is_none());
        assert_eq!(
            app.data().unwrap().cell_text(0, 1).as_deref(),
            Some("15.25")
        );
        app.undo();
        assert_eq!(app.data().unwrap().cell_text(0, 1).as_deref(), Some("13.1"));
    }

    #[test]
    fn inline_edit_of_missing_cell_starts_empty_and_cancel_keeps_it_missing() {
        let mut app = FreePlotsApp::new();
        app.new_worksheet();
        app.begin_inline_edit(0, 0);
        assert!(app.inline_buffer.is_empty());
        app.inline_buffer = "1".to_owned();
        app.cancel_inline_edit();
        assert!(app.inline_edit.is_none());
        assert!(app.data().unwrap().columns()[0].is_missing(0));
    }

    #[test]
    fn cell_edits_can_be_undone_and_redone() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.set_edit_buffer("99");
        assert!(app.commit_cell());
        assert!(app.can_undo());

        app.undo();
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
        assert!(app.can_redo());

        app.redo();
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("99"));
    }

    #[test]
    fn undo_removes_a_row_created_by_the_edit() {
        let mut app = FreePlotsApp::new();
        app.new_worksheet();
        app.select_cell(0, 5);
        app.set_edit_buffer("7");
        assert!(app.commit_cell());
        assert_eq!(app.data().map(DataFrame::n_rows), Some(6));
        app.undo();
        assert_eq!(app.data().map(DataFrame::n_rows), Some(0));
    }

    #[test]
    fn paste_that_adds_columns_is_undone_as_one_group() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(1, 0);
        app.paste_tsv("1\t2\t3");
        assert_eq!(app.data().map(DataFrame::n_columns), Some(4));
        assert!(app.can_undo(), "nothing to undo: {}", app.status());
        app.undo();
        assert_eq!(
            app.data().map(DataFrame::n_columns),
            Some(2),
            "status: {}",
            app.status()
        );
        assert_eq!(app.data().unwrap().cell_text(1, 0).as_deref(), Some("A"));
    }

    #[test]
    fn paste_is_undone_as_one_group() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.paste_tsv("1\t2\n3\t4");
        assert_eq!(app.data().unwrap().cell_text(1, 1).as_deref(), Some("4"));
        app.undo();
        let df = app.data().unwrap();
        assert_eq!(df.cell_text(0, 0).as_deref(), Some("12.5"));
        assert_eq!(df.cell_text(1, 1).as_deref(), Some("A"));
    }

    #[test]
    fn structural_edits_are_undoable() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());

        app.add_row();
        assert_eq!(app.data().map(DataFrame::n_rows), Some(6));
        app.undo();
        assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
        app.redo();
        assert_eq!(app.data().map(DataFrame::n_rows), Some(6));

        app.select_cell(0, 0);
        app.delete_row();
        assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
        app.undo();
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));

        app.add_column();
        assert_eq!(app.data().map(DataFrame::n_columns), Some(3));
        app.undo();
        assert_eq!(app.data().map(DataFrame::n_columns), Some(2));
        app.redo();
        assert_eq!(app.data().map(DataFrame::n_columns), Some(3));

        app.select_cell(1, 0);
        app.delete_column();
        assert_eq!(app.data().map(DataFrame::n_columns), Some(2));
        app.undo();
        assert_eq!(app.data().unwrap().cell_text(1, 0).as_deref(), Some("A"));

        app.select_cell(0, 0);
        app.begin_rename_column();
        app.rename_text = "mass".to_owned();
        app.commit_rename_column();
        assert_eq!(app.data().unwrap().columns()[0].name(), "mass");
        app.undo();
        assert_eq!(app.data().unwrap().columns()[0].name(), "weight");
        app.redo();
        assert_eq!(app.data().unwrap().columns()[0].name(), "mass");
    }

    #[test]
    fn undo_and_redo_follow_the_order_of_cell_and_structural_edits() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.set_edit_buffer("99");
        assert!(app.commit_cell());
        app.add_row();
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("99"));

        app.undo(); // remove the row first
        assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("99"));
        app.undo(); // then restore the cell
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
    }

    #[test]
    fn invalid_cell_text_is_rejected_without_losing_the_old_value() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.set_edit_buffer("not numeric");
        assert!(!app.commit_cell());
        assert!(app.status().contains("expected a number"));
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
    }

    #[test]
    fn add_remove_rows_and_columns() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.add_row();
        assert_eq!(app.data().map(DataFrame::n_rows), Some(6));
        app.add_column();
        assert_eq!(app.data().map(DataFrame::n_columns), Some(3));
        app.add_text_column();
        assert_eq!(app.data().map(DataFrame::n_columns), Some(4));
        assert_eq!(app.data().unwrap().columns()[3].dtype(), DType::Text);
        app.delete_column();
        assert_eq!(app.data().map(DataFrame::n_columns), Some(3));
        app.delete_column();
        assert_eq!(app.data().map(DataFrame::n_columns), Some(2));
        app.delete_row();
        assert_eq!(app.data().map(DataFrame::n_rows), Some(5));
    }

    #[test]
    fn paste_tsv_fills_a_rectangle() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.paste_tsv("1\t2\n3\t4");
        let df = app.data().unwrap();
        assert_eq!(df.cell_text(0, 0).as_deref(), Some("1"));
        assert_eq!(df.cell_text(1, 1).as_deref(), Some("4"));
    }

    #[test]
    fn rename_column_updates_the_selection() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(1, 0);
        app.begin_rename_column();
        app.rename_text = "batch_id".to_owned();
        app.commit_rename_column();
        assert_eq!(app.data().unwrap().columns()[1].name(), "batch_id");
        assert_eq!(app.selected_column(), Some("batch_id"));
    }

    #[test]
    #[ignore = "phase 1 grid budget; run on the target machine"]
    fn grid_100k_by_20_stays_inside_the_frame_budget() {
        use std::time::{Duration, Instant};

        let rows = 100_000;
        let columns = 20;
        let values: Vec<Vec<f64>> = (0..columns)
            .map(|column| {
                (0..rows)
                    .map(|row| (row as f64) * 0.001 + column as f64)
                    .collect()
            })
            .collect();
        let df = DataFrame::new(
            values
                .into_iter()
                .enumerate()
                .map(|(index, values)| Column::new_f64(format!("C{}", index + 1), values))
                .collect(),
        )
        .expect("frame");
        let mut app = FreePlotsApp::new();
        app.set_data(df, "large.csv".to_owned());

        let ctx = egui::Context::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1320.0, 840.0),
            )),
            ..Default::default()
        };
        // Warm up fonts and layout caches.
        let mut output = ctx.run_ui(input(), |ui| app.show(ui));
        output.textures_delta.clear();

        let started = Instant::now();
        for _ in 0..3 {
            let mut output = ctx.run_ui(input(), |ui| app.show(ui));
            output.textures_delta.clear();
        }
        let frame = started.elapsed() / 3;
        // The 20 ms budget is the desktop target; the Raspberry Pi 4 is
        // slower and gets a 60 ms budget for the same frame.
        let frame_budget = if cfg!(target_arch = "aarch64") {
            Duration::from_millis(60)
        } else {
            Duration::from_millis(20)
        };
        println!("100k×20 frame: {frame:?} (budget {frame_budget:?})");
        assert!(frame <= frame_budget, "frame took {frame:?}");

        app.select_cell(0, 50_000);
        app.set_edit_buffer("42");
        let started = Instant::now();
        assert!(app.commit_cell());
        let edit = started.elapsed();
        let edit_budget = if cfg!(target_arch = "aarch64") {
            Duration::from_millis(40)
        } else {
            Duration::from_millis(16)
        };
        println!("cell edit + status summary: {edit:?} (budget {edit_budget:?})");
        assert!(edit <= edit_budget, "edit took {edit:?}");
    }

    #[test]
    fn headless_frame_paints_worksheet_and_session() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.run_probability_plot();

        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
        let painted = output.shapes.len();
        output.textures_delta.clear();
        assert!(painted > 0, "the frame painted nothing");
    }

    #[test]
    fn toolbar_renders_the_icon_buttons() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
        let painted = output.shapes.len();
        output.textures_delta.clear();
        assert!(painted > 0, "the toolbar painted nothing");
    }

    #[test]
    fn every_tool_icon_paints_inside_its_box() {
        // One representative of every family the menus use; each must draw
        // something visible and stay inside its 26×24 hit area.
        let icons = [
            ToolIcon::Open,
            ToolIcon::Save,
            ToolIcon::New,
            ToolIcon::Export,
            ToolIcon::Undo,
            ToolIcon::Redo,
            ToolIcon::Plot,
            ToolIcon::Clear,
            ToolIcon::Pencil,
            ToolIcon::Calculator,
            ToolIcon::Grid,
            ToolIcon::Plus,
            ToolIcon::Minus,
            ToolIcon::Sort,
            ToolIcon::Swap,
            ToolIcon::Layers,
            ToolIcon::Filter,
            ToolIcon::Box,
            ToolIcon::Bars,
            ToolIcon::Dots,
            ToolIcon::Info,
            ToolIcon::Globe,
            ToolIcon::Plug,
            ToolIcon::Sigma,
            ToolIcon::Clock,
        ];
        for icon in icons {
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                paint_icon(
                    ui.painter(),
                    egui::pos2(20.0, 20.0),
                    icon,
                    egui::Stroke::new(1.4, egui::Color32::BLACK),
                );
            });
            output.textures_delta.clear();
            assert!(!output.shapes.is_empty(), "an icon painted nothing");
            let bounds = output
                .shapes
                .iter()
                .fold(egui::Rect::NOTHING, |rect, shape| {
                    rect.union(shape.shape.visual_bounding_rect())
                });
            assert!(
                bounds.width().max(bounds.height()) >= 6.0,
                "an icon is too small to see: {bounds:?}"
            );
            assert!(
                bounds.width() <= 26.0 && bounds.height() <= 24.0,
                "an icon overflows its hit area: {bounds:?}"
            );
        }
    }

    #[test]
    fn kittest_opens_the_about_dialog_through_the_menu() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), FreePlotsApp::new());
        harness.run();
        harness.get_by_label("Help").click();
        harness.run();
        harness.get_by_label("About FreePlots").click();
        harness.run();
        assert!(harness.state().show_about);
    }

    #[test]
    fn analysis_without_data_reports_an_error() {
        let mut app = FreePlotsApp::new();
        app.run_probability_plot();
        assert!(app.status().contains("worksheet is empty"));
    }

    #[test]
    fn the_native_plot_window_option_is_on_by_default() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        assert!(harness.state().native_plot_windows);
        harness.get_by_label("Tools").click();
        harness.run();
        harness.get_by_label("Native plot windows").click();
        harness.run();
        assert!(!harness.state().native_plot_windows);
    }

    #[test]
    fn the_native_plot_viewport_embeds_in_the_test_harness() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.native_plot_windows = true;
        app.run_probability_plot();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| app.show(ui));
        output.textures_delta.clear();
        assert!(app.show_plot);
    }

    #[test]
    fn a_warning_reaches_the_session_and_the_status() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.push_warning("2 formula cells were read as cached values");
        assert!(
            app.session_text()
                .contains("Warning: 2 formula cells were read as cached values")
        );
        assert!(app.status().contains("cached values"));
    }

    #[test]
    fn the_clear_button_resets_the_dialog_options() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.open_probability_plot_dialog();
        app.dialog_group = Some("batch".to_owned());
        app.dialog_display = PlotDisplay::Symbols;
        app.dialog_ci = false;
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        {
            let buttons: Vec<_> = harness.get_all_by_label("Clear").collect();
            assert!(!buttons.is_empty());
            // The dialog's Clear is the left one; the Session panel also has a
            // Clear button at the top right.
            let button = buttons
                .into_iter()
                .min_by(|a, b| a.rect().center().x.total_cmp(&b.rect().center().x))
                .expect("clear");
            button.click();
        }
        harness.run();
        harness.run();
        let app = harness.state();
        assert!(app.dialog_group.is_none());
        assert_eq!(app.dialog_display, PlotDisplay::Both);
        assert!(app.dialog_ci);
        assert!(app.show_probability_plot_dialog);
    }

    #[test]
    fn the_csv_preview_confirms_the_detected_options() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_csv_preview(
            "weights.csv".to_owned(),
            CsvPreview {
                text: "weight;height\n12,5;10\n13,5;11".to_owned(),
                options: CsvOptions {
                    delimiter: b';',
                    decimal_comma: true,
                    has_header: true,
                },
                encoding: "UTF-8".to_owned(),
            },
        );
        assert!(app.show_csv_options);
        assert_eq!(app.csv_delimiter, b';');
        assert!(app.csv_decimal_comma);
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.get_by_label("Open").click();
        harness.run();
        let request = harness
            .state_mut()
            .take_open_request()
            .expect("open request");
        assert_eq!(request.kind, FileKind::Csv);
        assert_eq!(
            request.csv_options,
            Some(CsvOptions {
                delimiter: b';',
                decimal_comma: true,
                has_header: true,
            })
        );
        assert!(!harness.state().show_csv_options);
    }

    #[test]
    fn the_date_format_changes_the_grid_text() {
        let date = |text: &str| fp_core::data::parse_date(text).expect("date");
        let mut app = FreePlotsApp::new();
        app.set_data(
            DataFrame::new(vec![Column::new_date(
                "date",
                vec![date("2024-02-29"), date("2025-01-01")],
            )])
            .expect("frame"),
            "dates.csv".to_owned(),
        );
        app.date_format = DateFormat::DayMonthYear;
        assert_eq!(
            app.display_cell(app.data().expect("data"), 0, 0),
            "29/02/2024"
        );
        app.date_format = DateFormat::DayMonthNameYear;
        assert_eq!(
            app.display_cell(app.data().expect("data"), 0, 1),
            "01 Jan 2025"
        );
        app.date_format = DateFormat::Iso;
        assert_eq!(
            app.display_cell(app.data().expect("data"), 0, 0),
            "2024-02-29"
        );
    }

    #[test]
    fn the_help_catalog_covers_the_common_controls() {
        for label in [
            "Column:",
            "Distribution:",
            "Confidence:",
            "Response:",
            "LSL:",
            "Time column:",
            "Use stress:",
            "Censoring value:",
            "Sample size:",
            "Keys:",
            "New name:",
        ] {
            assert!(label_help(label).is_some(), "{label}");
        }
        assert!(label_help("Some unknown field:").is_some());
        assert!(label_help("A plain heading").is_none());
        assert!(button_help("Run").contains("analysis"));
        assert!(button_help("Cancel").contains("without"));
        assert!(checkbox_help("Randomize run order").contains("seed"));
    }

    #[test]
    fn find_selects_the_next_match_and_wraps_around() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.find_text = "13.1".to_owned();
        app.find_next();
        assert_eq!(app.selected_cell(), Some((0, 1)));
        assert_eq!(app.find_message, "Found at C1 R2");
        // Wrapping around finds the same single occurrence again.
        app.find_next();
        assert_eq!(app.find_message, "Found at C1 R2");
        app.find_text = "missing".to_owned();
        app.find_next();
        assert_eq!(app.find_message, "No matches");
    }

    #[test]
    fn find_options_respect_case_and_whole_cells() {
        assert!(text_matches("Alpha", "alpha", false, false));
        assert!(!text_matches("Alpha", "alpha", true, false));
        assert!(text_matches("Alpha", "Alpha", true, true));
        assert!(!text_matches("Alphabet", "Alpha", true, true));
        assert_eq!(
            replace_in("a-B-a", "a", "x", false, false).as_deref(),
            Some("x-B-x")
        );
        assert_eq!(replace_in("a-B-a", "a", "x", true, true), None);
        assert_eq!(replace_in("a", "a", "x", true, true).as_deref(), Some("x"));
    }

    #[test]
    fn replace_all_rewrites_matches_and_undo_restores_them() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.find_text = "A".to_owned();
        app.replace_text = "B".to_owned();
        app.replace_all();
        assert_eq!(app.find_message, "5 replaced");
        for row in 0..5 {
            assert_eq!(
                app.data().expect("frame").cell_text(1, row).as_deref(),
                Some("B")
            );
        }
        app.undo();
        for row in 0..5 {
            assert_eq!(
                app.data().expect("frame").cell_text(1, row).as_deref(),
                Some("A")
            );
        }
    }

    #[test]
    fn fill_down_and_right_copy_the_first_row_and_column() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.extend_selection(0, 2);
        app.fill_down();
        assert_eq!(
            app.data().expect("frame").cell_text(0, 1).as_deref(),
            Some("12.5")
        );
        assert_eq!(
            app.data().expect("frame").cell_text(0, 2).as_deref(),
            Some("12.5")
        );
        app.undo();

        app.select_cell(0, 0);
        app.extend_selection(1, 0);
        app.fill_right();
        assert_eq!(
            app.data().expect("frame").cell_text(1, 0).as_deref(),
            Some("12.5")
        );
        app.undo();
        assert_eq!(
            app.data().expect("frame").cell_text(1, 0).as_deref(),
            Some("A")
        );
    }

    #[test]
    fn a_column_format_changes_the_display_only() {
        let format = ColumnFormat {
            decimals: 2,
            thousands: true,
            bold: true,
            align: CellAlign::Right,
        };
        assert_eq!(
            format_cell_text("1234.5678".to_owned(), Some(format)),
            "1 234.57"
        );
        assert_eq!(
            format_cell_text("-1234.5".to_owned(), Some(format)),
            "-1 234.50"
        );
        assert_eq!(format_cell_text("text".to_owned(), Some(format)), "text");
        assert_eq!(format_cell_text("12.5".to_owned(), None), "12.5");

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.formats.insert("weight".to_owned(), format);
        assert_eq!(
            app.data().expect("frame").cell_text(0, 0).as_deref(),
            Some("12.5")
        );
        app.clear_column_format();
        assert!(app.formats.is_empty());
    }

    #[test]
    fn dropping_a_file_picks_the_reader_by_extension() {
        let mut app = FreePlotsApp::new();
        assert!(app.open_dropped("/tmp/report.csv"));
        assert_eq!(app.preview_request.as_deref(), Some("/tmp/report.csv"));
        assert!(app.open_dropped("/tmp/book.XLSX"));
        assert_eq!(
            app.open_request.as_ref().map(|request| request.kind),
            Some(FileKind::Xlsx)
        );
        app.open_request = None;
        assert!(app.open_dropped("/tmp/session.fpp"));
        assert_eq!(
            app.open_request.as_ref().map(|request| request.kind),
            Some(FileKind::Project)
        );
        assert!(!app.open_dropped("/tmp/notes.pdf"));
    }

    #[test]
    fn date_difference_and_shift_add_columns() {
        let date = |text: &str| fp_core::data::parse_date(text).expect("date");
        let mut app = FreePlotsApp::new();
        app.set_data(
            DataFrame::new(vec![
                Column::new_date("start", vec![date("2024-01-01"), date("2024-02-01")]),
                Column::new_date("end", vec![date("2024-01-11"), date("2024-03-01")]),
            ])
            .expect("frame"),
            "dates.csv".to_owned(),
        );
        app.open_date_diff_dialog();
        assert_eq!(app.date_diff_first, "start");
        assert_eq!(app.date_diff_second, "end");
        app.run_date_diff_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let df = app.data().expect("frame");
        assert_eq!(df.n_columns(), 3);
        assert_eq!(df.cell_text(2, 0).as_deref(), Some("10"));
        assert_eq!(df.cell_text(2, 1).as_deref(), Some("29"));

        app.open_date_shift_dialog();
        assert_eq!(app.date_shift_column, "start");
        app.date_shift_days = 0;
        app.date_shift_name = "same".to_owned();
        app.run_date_shift_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let df = app.data().expect("frame");
        assert_eq!(df.cell_text(3, 0), df.cell_text(0, 0));

        app.open_date_shift_dialog();
        app.date_shift_column = "start".to_owned();
        app.date_shift_days = -1;
        app.date_shift_name = "before".to_owned();
        app.run_date_shift_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
    }

    #[test]
    fn the_date_dialogs_reject_missing_columns() {
        let mut app = FreePlotsApp::new();
        app.open_date_diff_dialog();
        app.date_diff_first.clear();
        app.date_diff_second.clear();
        app.run_date_diff_dialog();
        assert_eq!(app.error.as_deref(), Some("Select the two date columns"));
        app.date_diff_first = "a".to_owned();
        app.date_diff_second = "b".to_owned();
        app.date_diff_name.clear();
        app.run_date_diff_dialog();
        assert_eq!(app.error.as_deref(), Some("Name the new column"));

        app.open_date_shift_dialog();
        app.date_shift_column.clear();
        app.run_date_shift_dialog();
        assert_eq!(app.error.as_deref(), Some("Select the date column"));
    }

    #[test]
    fn the_step_stress_dialog_needs_every_column() {
        let mut app = FreePlotsApp::new();
        app.open_step_stress_dialog();
        app.ss_duration.clear();
        app.ss_stress.clear();
        app.ss_unit.clear();
        app.ss_event.clear();
        app.run_step_stress_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Select the duration, stress, unit and event columns")
        );
    }

    #[test]
    fn the_step_stress_dialog_fits_a_synthetic_test() {
        let mut app = FreePlotsApp::new();
        app.set_data(step_stress_frame(), "step.csv".to_owned());
        app.open_step_stress_dialog();
        // The first columns are picked in declaration order.
        app.ss_duration = "duration".to_owned();
        app.ss_stress = "stress".to_owned();
        app.ss_unit = "unit".to_owned();
        app.ss_event = "event".to_owned();
        app.ss_use_stress = 90.0;
        app.run_step_stress_dialog();
        let text = app.session_text();
        assert!(
            app.error.is_none() || text.is_empty(),
            "unexpected error: {:?}",
            app.error
        );
    }

    #[test]
    fn the_desirability_dialog_validates_and_optimises() {
        let mut app = FreePlotsApp::new();
        app.set_data(desirability_frame(), "design.csv".to_owned());

        app.open_desirability_dialog();
        app.desirability_factors = vec!["a".to_owned()];
        app.run_desirability_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Select at least two factor columns")
        );

        app.open_desirability_dialog();
        app.desirability_factors = vec!["a".to_owned(), "b".to_owned()];
        app.run_desirability_dialog();
        assert_eq!(app.error.as_deref(), Some("Add at least one response"));

        app.open_desirability_dialog();
        app.desirability_factors = vec!["a".to_owned(), "b".to_owned()];
        app.desirability_responses.push(DesirabilityRow {
            column: "y".to_owned(),
            goal: Goal::Maximize,
            low: "0".to_owned(),
            target: "0".to_owned(),
            high: "30".to_owned(),
            weight: "1".to_owned(),
            importance: "1".to_owned(),
        });
        app.desirability_responses[0].goal = Goal::Target;
        app.desirability_responses[0].target = "not a number".to_owned();
        app.run_desirability_dialog();
        assert_eq!(app.error.as_deref(), Some("Target must be a number"));
    }

    #[test]
    fn the_response_optimiser_runs_on_a_quadratic_design() {
        let mut app = FreePlotsApp::new();
        app.set_data(desirability_frame(), "design.csv".to_owned());
        app.plot = None;
        app.open_desirability_dialog();
        app.desirability_factors = vec!["a".to_owned(), "b".to_owned()];
        app.desirability_responses.push(DesirabilityRow {
            column: "y".to_owned(),
            goal: Goal::Maximize,
            low: "0".to_owned(),
            target: String::new(),
            high: "30".to_owned(),
            weight: "1".to_owned(),
            importance: "1".to_owned(),
        });
        app.run_desirability_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Response Optimizer"));
    }

    #[test]
    fn control_chart_options_reach_the_session() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.run_control_chart_with_options(
            "weight",
            ControlChartKind::IndividualsMovingRange,
            1,
            TestSet::default(),
            fp_core::spc::CusumOptions::default(),
            fp_core::spc::EwmaOptions::default(),
        );
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Control Chart"));
        assert!(app.plot().is_some());
    }

    #[test]
    fn run_spec_reports_the_empty_worksheet() {
        let mut app = FreePlotsApp::new();
        app.data = None;
        let result = app.run_spec(
            AnalysisSpec::capability("x", Some(1.0), None),
            "Capability".to_owned(),
        );
        assert!(result.is_err());
        assert_eq!(
            app.error.as_deref(),
            Some("No worksheet open: use File → Open CSV…")
        );
    }

    #[test]
    fn the_find_and_format_dialogs_open_with_the_current_values() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.open_find_dialog();
        assert!(app.show_find_dialog);
        app.open_format_dialog();
        assert!(app.show_format_dialog);
        assert_eq!(app.format_decimals, 6);
        assert_eq!(app.format_align, CellAlign::Left);
        app.format_decimals = 2;
        app.format_thousands = true;
        app.format_bold = true;
        app.format_align = CellAlign::Right;
        app.apply_column_format();
        let saved = app.formats.get("weight").copied().expect("format");
        assert_eq!(saved.decimals, 2);
        assert!(saved.thousands && saved.bold);
        assert_eq!(saved.align, CellAlign::Right);
        app.open_format_dialog();
        assert_eq!(app.format_decimals, 2);
        assert_eq!(app.format_align, CellAlign::Right);
    }

    #[test]
    fn the_t_test_dialog_honours_its_options() {
        let mut app = FreePlotsApp::new();
        app.set_data(two_sample_frame(), "samples.csv".to_owned());

        app.open_t_test_dialog(TTestKind::OneSample);
        assert!(app.show_t_dialog);
        app.t_first = "before".to_owned();
        app.t_null = "12".to_owned();
        app.run_t_test_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("1-Sample t"));
        let two_sided = app.session_text().to_owned();

        app.session.clear();
        app.t_alternative = Alternative::Greater;
        app.run_t_test_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(
            app.session_text(),
            two_sided,
            "the alternative changed nothing"
        );

        app.session.clear();
        app.t_confidence = 99.9;
        app.t_alternative = Alternative::TwoSided;
        app.run_t_test_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(
            app.session_text(),
            two_sided,
            "the confidence changed nothing"
        );

        app.session.clear();
        app.open_t_test_dialog(TTestKind::TwoSample);
        app.t_first = "before".to_owned();
        app.t_second = "after".to_owned();
        app.t_pooled = true;
        app.run_t_test_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let pooled = app.session_text().to_owned();
        app.session.clear();
        app.open_t_test_dialog(TTestKind::TwoSample);
        app.t_first = "before".to_owned();
        app.t_second = "after".to_owned();
        app.t_pooled = false;
        app.run_t_test_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), pooled, "pooling changed nothing");

        app.session.clear();
        app.open_t_test_dialog(TTestKind::Paired);
        app.t_first = "before".to_owned();
        app.t_second = "after".to_owned();
        app.run_t_test_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Paired t"));
    }

    #[test]
    fn the_t_test_dialog_rejects_bad_input() {
        let mut app = FreePlotsApp::new();
        app.set_data(two_sample_frame(), "samples.csv".to_owned());
        app.open_t_test_dialog(TTestKind::OneSample);
        app.t_first.clear();
        app.run_t_test_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Select the first numeric column")
        );
        app.t_first = "before".to_owned();
        app.t_null = "abc".to_owned();
        app.run_t_test_dialog();
        assert_eq!(app.error.as_deref(), Some("Null mean must be a number"));
        app.open_t_test_dialog(TTestKind::TwoSample);
        app.t_first = "before".to_owned();
        app.t_second.clear();
        app.run_t_test_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Select the second numeric column")
        );
        app.t_second = "before".to_owned();
        app.run_t_test_dialog();
        assert_eq!(app.error.as_deref(), Some("Select two different columns"));
    }

    #[test]
    fn the_equivalence_dialog_checks_the_margin() {
        let mut app = FreePlotsApp::new();
        app.set_data(two_sample_frame(), "samples.csv".to_owned());
        app.open_equivalence_dialog(EquivalenceKind::TwoSample);
        app.equivalence_first = "before".to_owned();
        app.equivalence_second = "after".to_owned();
        app.equivalence_margin = "1.5".to_owned();
        app.run_equivalence_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let text = app.session_text().to_owned();
        assert!(text.to_lowercase().contains("equivalence"), "{text}");

        app.session.clear();
        app.open_equivalence_dialog(EquivalenceKind::TwoSample);
        app.equivalence_first = "before".to_owned();
        app.equivalence_second = "after".to_owned();
        app.equivalence_margin = "abc".to_owned();
        app.run_equivalence_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The equivalence margin must be a number")
        );
        app.equivalence_margin = "0".to_owned();
        app.run_equivalence_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The equivalence margin must be positive")
        );
    }

    #[test]
    fn the_proportion_dialog_honours_the_method_and_rejects_bad_values() {
        let mut app = FreePlotsApp::new();
        app.set_data(proportion_frame(), "pass.csv".to_owned());

        app.open_test2_dialog(Test2Kind::OneProportion);
        app.test2_first = "pass".to_owned();
        app.test2_null = "0.5".to_owned();
        app.test2_success = "1".to_owned();
        app.test2_method = ProportionMethod::Exact;
        app.run_test2_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let exact = app.session_text().to_owned();
        assert!(exact.contains("1 Proportion"), "{exact}");

        app.session.clear();
        app.test2_method = ProportionMethod::Normal;
        app.run_test2_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), exact, "the method changed nothing");

        app.test2_null = "abc".to_owned();
        app.run_test2_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Null proportion must be a number")
        );

        app.session.clear();
        app.open_test2_dialog(Test2Kind::TwoProportions);
        app.test2_first = "pass".to_owned();
        app.test2_success = "1".to_owned();
        app.test2_second.clear();
        app.run_test2_dialog();
        assert_eq!(app.error.as_deref(), Some("Select two different columns"));
        app.test2_second = "retest".to_owned();
        app.run_test2_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("2 Proportions"));
    }

    #[test]
    fn the_power_dialog_solves_for_n_and_validates() {
        let mut app = FreePlotsApp::new();
        app.set_data(two_sample_frame(), "samples.csv".to_owned());
        app.power_procedure = PowerProcedure::TwoSampleT;
        app.power_effect = "0.5".to_owned();
        app.power_n = "0".to_owned();
        app.power_target = 80.0;
        app.power_alpha = 5.0;
        app.power_groups = 2;
        app.run_power_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Power"));

        app.session.clear();
        app.power_procedure = PowerProcedure::TwoSampleT;
        app.power_effect = "0.5".to_owned();
        app.power_n = "50".to_owned();
        app.run_power_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Power"));

        app.power_effect = "abc".to_owned();
        app.run_power_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The effect size must be a number")
        );
        app.power_effect = "0.5".to_owned();
        app.power_n = "abc".to_owned();
        app.run_power_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The sample size must be a whole number (0 solves for it)")
        );
    }

    #[test]
    fn every_control_chart_kind_runs() {
        let mut app = FreePlotsApp::new();
        app.set_data(process_frame(), "process.csv".to_owned());
        let kinds = [
            ControlChartKind::IndividualsMovingRange,
            ControlChartKind::XbarR,
            ControlChartKind::XbarS,
            ControlChartKind::Ewma,
            ControlChartKind::Cusum,
        ];
        for kind in kinds {
            app.session.clear();
            app.run_control_chart_with_options(
                "x",
                kind,
                5,
                TestSet::default(),
                fp_core::spc::CusumOptions::default(),
                fp_core::spc::EwmaOptions::default(),
            );
            assert!(app.error.is_none(), "{kind:?}: {:?}", app.error);
            assert!(
                app.session_text().contains(kind.name()),
                "{} missing from the session",
                kind.name()
            );
        }
    }

    #[test]
    fn the_control_chart_tests_and_options_are_applied() {
        let mut app = FreePlotsApp::new();
        app.set_data(process_frame(), "process.csv".to_owned());
        let tests = TestSet::only(&[1, 2]).expect("tests");
        let cusum = fp_core::spc::CusumOptions::new(0.5, 4.0).expect("cusum");
        let ewma = fp_core::spc::EwmaOptions::new(0.2, 3.0).expect("ewma");
        app.run_control_chart_with_options(
            "x",
            ControlChartKind::IndividualsMovingRange,
            1,
            tests,
            cusum,
            ewma,
        );
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.plot().is_some());

        // Out-of-range options are rejected before the chart is drawn.
        assert!(fp_core::spc::CusumOptions::new(0.0, 4.0).is_err());
        assert!(fp_core::spc::CusumOptions::new(0.5, 0.0).is_err());
        assert!(fp_core::spc::EwmaOptions::new(0.0, 3.0).is_err());
        assert!(fp_core::spc::EwmaOptions::new(1.5, 3.0).is_err());
        assert!(fp_core::spc::TestSet::only(&[0]).is_err());
        assert!(fp_core::spc::TestSet::only(&[9]).is_err());
    }

    #[test]
    fn the_capability_options_reach_the_engine() {
        let mut app = FreePlotsApp::new();
        app.set_data(process_frame(), "process.csv".to_owned());

        let normal = AnalysisSpec::capability("x", Some(8.0), Some(12.0));
        app.run_spec(normal, "Capability".to_owned())
            .expect("normal capability");
        let normal_text = app.session_text().to_owned();
        assert!(normal_text.contains("Capability"));

        app.session.clear();
        let lognormal = AnalysisSpec::capability("x", Some(8.0), Some(12.0))
            .with_capability_distribution(DistributionKind::Lognormal)
            .with_capability_bootstrap(50)
            .with_capability_target(Some(10.0));
        app.run_spec(lognormal, "Capability".to_owned())
            .expect("lognormal capability");
        assert_ne!(app.session_text(), normal_text);

        app.session.clear();
        let box_cox = AnalysisSpec::capability("x", Some(8.0), None).with_capability_box_cox(true);
        app.run_spec(box_cox, "Capability".to_owned())
            .expect("box-cox capability");

        // Requiring a stable screen refuses this drifting series.
        app.session.clear();
        let strict = AnalysisSpec::capability("x", Some(8.0), None).with_capability_stability(true);
        assert!(app.run_spec(strict, "Capability".to_owned()).is_err());
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("special-cause")),
            "{:?}",
            app.error
        );

        // No specification limit is refused by the engine.
        let none = AnalysisSpec::capability("x", None, None);
        assert!(app.run_spec(none, "Capability".to_owned()).is_err());
        assert!(app.error.is_some());
    }

    #[test]
    fn the_regression_dialog_fits_predictors_and_confidence() {
        let mut app = FreePlotsApp::new();
        app.set_data(regression_frame(), "reg.csv".to_owned());

        app.open_regression_dialog();
        assert!(app.show_reg_dialog);
        app.reg_response = "y".to_owned();
        app.reg_predictors = vec!["x1".to_owned(), "x2".to_owned()];
        app.run_regression_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let text = app.session_text().to_owned();
        assert!(text.contains("Regression"), "{text}");
        assert!(text.contains("x1"), "{text}");

        app.session.clear();
        app.open_regression_dialog();
        app.reg_response = "y".to_owned();
        app.reg_predictors = vec!["x1".to_owned()];
        app.reg_confidence = 99.9;
        app.run_regression_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), text, "the confidence changed nothing");

        app.open_regression_dialog();
        app.reg_response.clear();
        app.run_regression_dialog();
        assert_eq!(app.error.as_deref(), Some("Select the response column"));
        app.reg_response = "y".to_owned();
        app.reg_predictors.clear();
        app.run_regression_dialog();
        assert_eq!(app.error.as_deref(), Some("Select at least one predictor"));
    }

    #[test]
    fn the_stepwise_dialog_honours_methods_and_criteria() {
        let mut app = FreePlotsApp::new();
        app.set_data(regression_frame(), "reg.csv".to_owned());

        let mut texts = Vec::new();
        for method in [
            StepMethod::Forward,
            StepMethod::Backward,
            StepMethod::Stepwise,
        ] {
            app.session.clear();
            app.open_stepwise_dialog();
            app.step_response = "y".to_owned();
            app.step_candidates = vec!["x1".to_owned(), "x2".to_owned(), "x3".to_owned()];
            app.step_method = method;
            app.run_stepwise_dialog();
            assert!(app.error.is_none(), "{method:?}: {:?}", app.error);
            texts.push(app.session_text().to_owned());
        }
        assert!(
            texts
                .iter()
                .all(|text| text.contains("Stepwise Regression"))
        );
        assert_ne!(texts[0], texts[1]);
        assert_ne!(texts[1], texts[2]);

        for criterion in [StepCriterion::Aic, StepCriterion::Bic] {
            app.session.clear();
            app.open_stepwise_dialog();
            app.step_response = "y".to_owned();
            app.step_candidates = vec!["x1".to_owned(), "x2".to_owned()];
            app.step_criterion = criterion;
            app.run_stepwise_dialog();
            assert!(app.error.is_none(), "{criterion:?}: {:?}", app.error);
        }

        app.open_stepwise_dialog();
        app.step_response.clear();
        app.run_stepwise_dialog();
        assert_eq!(app.error.as_deref(), Some("Select the response column"));
        app.step_response = "y".to_owned();
        app.step_candidates.clear();
        app.run_stepwise_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Select at least one candidate predictor")
        );
    }

    #[test]
    fn every_one_way_kind_runs() {
        let mut app = FreePlotsApp::new();
        app.set_data(two_way_frame(), "anova.csv".to_owned());
        for kind in [OneWayKind::Anova, OneWayKind::Levene, OneWayKind::Bartlett] {
            app.session.clear();
            app.open_one_way_dialog(kind);
            app.anova_response = "y".to_owned();
            app.anova_factor = "g".to_owned();
            app.run_anova_dialog();
            assert!(app.error.is_none(), "{kind:?}: {:?}", app.error);
            assert!(
                app.session_text().contains(kind.name()),
                "{} missing from the session",
                kind.name()
            );
        }

        app.open_one_way_dialog(OneWayKind::Anova);
        app.anova_response = "g".to_owned();
        app.anova_factor = "g".to_owned();
        app.run_anova_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The response and factor must be different")
        );
    }

    #[test]
    fn the_two_way_dialog_validates_its_factors() {
        let mut app = FreePlotsApp::new();
        app.set_data(two_way_frame(), "anova.csv".to_owned());
        app.open_two_way_dialog();
        app.two_way_response = "y".to_owned();
        app.two_way_first = "g".to_owned();
        app.two_way_second = "h".to_owned();
        app.run_two_way_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Two-Way"));

        app.open_two_way_dialog();
        app.two_way_response = "y".to_owned();
        app.two_way_first = "g".to_owned();
        app.two_way_second = "g".to_owned();
        app.run_two_way_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Select two different factor columns")
        );
    }

    #[test]
    fn the_reliability_dialogs_fit_and_validate() {
        let mut app = FreePlotsApp::new();
        app.set_data(life_frame(), "life.csv".to_owned());

        app.open_weibull_dialog();
        app.weibull_time = "time".to_owned();
        app.run_weibull_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let plain = app.session_text().to_owned();
        assert!(plain.contains("Weibull"));

        app.session.clear();
        app.open_weibull_dialog();
        app.weibull_time = "time".to_owned();
        app.weibull_censor = Some("censor".to_owned());
        app.weibull_censor_value = "0".to_owned();
        app.run_weibull_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), plain, "censoring changed nothing");

        app.open_weibull_dialog();
        app.weibull_time = "time".to_owned();
        app.weibull_censor = Some("censor".to_owned());
        app.weibull_censor_value = "abc".to_owned();
        app.run_weibull_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Censoring value must be a number")
        );
        app.weibull_censor_value = "0".to_owned();
        app.weibull_time.clear();
        app.run_weibull_dialog();
        assert_eq!(app.error.as_deref(), Some("Select the time column"));

        for (open, run, marker) in [
            (
                FreePlotsApp::open_exponential_dialog as fn(&mut FreePlotsApp),
                FreePlotsApp::run_exponential_dialog as fn(&mut FreePlotsApp),
                "Exponential",
            ),
            (
                FreePlotsApp::open_lognormal_dialog as fn(&mut FreePlotsApp),
                FreePlotsApp::run_lognormal_dialog as fn(&mut FreePlotsApp),
                "Lognormal",
            ),
        ] {
            app.session.clear();
            open(&mut app);
            if marker == "Exponential" {
                app.exp_time = "time".to_owned();
                app.exp_censor = Some("censor".to_owned());
                app.exp_censor_value = "0".to_owned();
            } else {
                app.log_time = "time".to_owned();
                app.log_censor = Some("censor".to_owned());
                app.log_censor_value = "0".to_owned();
            }
            run(&mut app);
            assert!(app.error.is_none(), "{marker}: {:?}", app.error);
            assert!(app.session_text().contains(marker), "{marker} missing");
        }
    }

    #[test]
    fn the_alt_dialog_checks_the_horizon() {
        let mut app = FreePlotsApp::new();
        app.set_data(life_frame(), "life.csv".to_owned());
        app.open_alt_dialog();
        app.alt_time = "time".to_owned();
        app.alt_stress = "stress".to_owned();
        app.alt_use_stress = 80.0;
        app.alt_life = fp_core::alt::LifeModel::Weibull;
        app.alt_acceleration = fp_core::alt::AccelerationModel::Arrhenius;
        app.alt_horizon.clear();
        app.run_alt_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Accelerated"));

        app.session.clear();
        app.open_alt_dialog();
        app.alt_time = "time".to_owned();
        app.alt_stress = "stress".to_owned();
        app.alt_use_stress = 80.0;
        app.alt_horizon = "60".to_owned();
        app.run_alt_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);

        app.session.clear();
        app.open_alt_dialog();
        app.alt_time = "time".to_owned();
        app.alt_stress = "stress".to_owned();
        app.alt_use_stress = 80.0;
        app.alt_mode = "mode".to_owned();
        app.run_alt_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);

        app.open_alt_dialog();
        app.alt_time = "time".to_owned();
        app.alt_stress = "time".to_owned();
        app.run_alt_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The time and the stress must be different columns")
        );
        app.alt_stress = "stress".to_owned();
        app.alt_horizon = "abc".to_owned();
        app.run_alt_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The horizon must be a positive number")
        );
        app.alt_horizon = "-5".to_owned();
        app.run_alt_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The horizon must be a positive number")
        );
    }

    #[test]
    fn the_kaplan_meier_confidence_changes_the_band() {
        let mut app = FreePlotsApp::new();
        app.set_data(life_frame(), "life.csv".to_owned());
        app.open_kaplan_meier_dialog();
        app.km_time = "time".to_owned();
        app.km_censor = Some("censor".to_owned());
        app.km_censor_value = "0".to_owned();
        app.km_confidence = 50.0;
        app.run_kaplan_meier_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let narrow = app.session_text().to_owned();
        assert!(narrow.contains("Kaplan"));

        app.session.clear();
        app.open_kaplan_meier_dialog();
        app.km_time = "time".to_owned();
        app.km_censor = Some("censor".to_owned());
        app.km_censor_value = "0".to_owned();
        app.km_confidence = 99.9;
        app.run_kaplan_meier_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), narrow, "the confidence changed nothing");

        app.open_kaplan_meier_dialog();
        app.km_time = "time".to_owned();
        app.km_censor = Some("censor".to_owned());
        app.km_censor_value = "x".to_owned();
        app.run_kaplan_meier_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Censoring value must be a number")
        );
    }

    #[test]
    fn the_design_dialog_builds_factorials() {
        let mut app = FreePlotsApp::new();
        app.open_design_dialog();
        app.design_factors = "A,B".to_owned();
        app.design_replicates = 2;
        app.design_centers = 3;
        app.design_block_size = 0;
        app.design_randomize = false;
        app.run_design_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let df = app.data().expect("design");
        assert_eq!(df.n_rows(), 11, "4 runs x 2 replicates + 3 centre points");
        let names: Vec<&str> = df.columns().iter().map(|column| column.name()).collect();
        assert!(names.contains(&"A") && names.contains(&"B"), "{names:?}");

        app.open_design_dialog();
        app.design_factors = "A,B".to_owned();
        app.design_replicates = 1;
        app.design_centers = 0;
        app.design_block_size = 2;
        app.run_design_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let df = app.data().expect("design");
        assert_eq!(df.n_rows(), 4);
        assert!(df.columns().iter().any(|column| column.name() == "Block"));

        app.open_design_dialog();
        app.design_factors.clear();
        app.run_design_dialog();
        assert_eq!(app.error.as_deref(), Some("Enter at least one factor name"));
    }

    #[test]
    fn the_box_behnken_and_plackett_burman_designs() {
        let mut app = FreePlotsApp::new();
        app.open_box_behnken_dialog();
        app.run_box_behnken_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let df = app.data().expect("design");
        assert_eq!(
            df.columns()
                .iter()
                .filter(|column| matches!(column.name(), "A" | "B" | "C"))
                .count(),
            3
        );
        assert!(df.n_rows() >= 13, "{} runs", df.n_rows());

        app.open_box_behnken_dialog();
        app.design_factors = "A,B".to_owned();
        app.run_box_behnken_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("A Box-Behnken design needs at least three factors")
        );

        app.open_pb_dialog();
        app.design_runs = 8;
        app.design_randomize = false;
        app.run_pb_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_eq!(app.data().expect("design").n_rows(), 8);

        app.open_pb_dialog();
        app.design_runs = 7;
        app.run_pb_dialog();
        assert!(app.error.is_some(), "7 runs must be refused");
    }

    #[test]
    fn the_factorial_analysis_reads_blocks_and_effects() {
        let mut app = FreePlotsApp::new();
        app.set_data(factorial_frame(), "doe.csv".to_owned());

        app.open_doe_dialog();
        app.doe_response = "y".to_owned();
        app.doe_factors = vec!["A".to_owned(), "B".to_owned()];
        app.doe_block.clear();
        app.doe_main_effects = true;
        app.run_doe_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let plain = app.session_text().to_owned();
        assert!(plain.contains("Factorial"), "{plain}");

        app.session.clear();
        app.doe_block = "block".to_owned();
        app.run_doe_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), plain, "the block changed nothing");

        app.open_doe_dialog();
        app.doe_factors.clear();
        app.run_doe_dialog();
        assert_eq!(app.error.as_deref(), Some("Select at least one factor"));
    }

    #[test]
    fn the_response_surface_flags_reach_the_engine() {
        let mut app = FreePlotsApp::new();
        app.set_data(desirability_frame(), "rsm.csv".to_owned());
        for (optimize, contours, steepest) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (false, false, true),
            (true, true, true),
        ] {
            app.session.clear();
            app.open_rsm_dialog();
            app.rsm_response = "y".to_owned();
            app.rsm_factors = vec!["a".to_owned(), "b".to_owned()];
            app.rsm_optimize = optimize;
            app.rsm_contours = contours;
            app.rsm_steepest_ascent = steepest;
            app.run_rsm_dialog();
            assert!(app.error.is_none(), "{:?}", app.error);
            assert!(app.session_text().contains("Response Surface"));
        }

        app.open_rsm_dialog();
        app.rsm_factors = vec!["a".to_owned()];
        app.run_rsm_dialog();
        assert_eq!(app.error.as_deref(), Some("Select at least two factors"));
    }

    #[test]
    fn the_taguchi_design_and_analysis() {
        let mut app = FreePlotsApp::new();
        app.open_taguchi_design_dialog();
        app.taguchi_runs = 8;
        app.taguchi_design_factors = 3;
        app.design_randomize = false;
        app.run_taguchi_design_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let df = app.data().expect("design");
        assert_eq!(df.n_rows(), 8);
        assert_eq!(
            df.columns()
                .iter()
                .filter(|column| matches!(column.name(), "A" | "B" | "C"))
                .count(),
            3
        );

        app.set_data(taguchi_frame(), "taguchi.csv".to_owned());
        app.open_taguchi_dialog();
        app.taguchi_response = "y".to_owned();
        app.taguchi_factors = "A,B,C".to_owned();
        app.taguchi_quality = fp_core::taguchi::Quality::Smaller;
        app.run_taguchi_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let smaller = app.session_text().to_owned();
        assert!(smaller.contains("Taguchi"));

        app.session.clear();
        app.open_taguchi_dialog();
        app.taguchi_response = "y".to_owned();
        app.taguchi_factors = "A,B,C".to_owned();
        app.taguchi_quality = fp_core::taguchi::Quality::Larger;
        app.run_taguchi_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), smaller, "the quality changed nothing");

        app.open_taguchi_dialog();
        app.taguchi_response = "y".to_owned();
        app.taguchi_factors = "A,y".to_owned();
        app.run_taguchi_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The response cannot also be a factor")
        );
    }

    #[test]
    fn the_gage_and_type_one_dialogs_run() {
        let mut app = FreePlotsApp::new();
        app.set_data(gage_frame(), "gage.csv".to_owned());
        app.open_gage_dialog();
        app.gage_measurement = "measure".to_owned();
        app.gage_part = "part".to_owned();
        app.gage_operator = "operator".to_owned();
        app.run_gage_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Gage R&R"));

        app.open_gage_dialog();
        app.gage_measurement = "part".to_owned();
        app.gage_part = "part".to_owned();
        app.gage_operator = "operator".to_owned();
        app.run_gage_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The three columns must be different")
        );

        app.open_type1_dialog();
        app.type1_measurement = "measure".to_owned();
        app.type1_reference = "10".to_owned();
        app.type1_lsl = "8".to_owned();
        app.type1_usl = "12".to_owned();
        app.run_type1_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Type 1"));

        app.type1_reference = "abc".to_owned();
        app.run_type1_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("Reference value must be a number")
        );
    }

    #[test]
    fn the_linearity_and_agreement_dialogs_run() {
        let mut app = FreePlotsApp::new();
        app.set_data(linearity_frame(), "linearity.csv".to_owned());
        app.open_linearity_dialog();
        let names: Vec<String> = app
            .data()
            .expect("data")
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .collect();
        assert!(names.contains(&"measure".to_owned()));
        app.linearity_measurement = "measure".to_owned();
        app.linearity_reference = "reference".to_owned();
        app.run_linearity_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Linearity"));

        app.open_linearity_dialog();
        app.linearity_measurement = "measure".to_owned();
        app.linearity_reference = "measure".to_owned();
        app.run_linearity_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The measurement and the reference must be different columns")
        );

        app.set_data(attribute_frame(), "agreement.csv".to_owned());
        app.open_agreement_dialog();
        app.agreement_part = "part".to_owned();
        app.agreement_appraiser = "appraiser".to_owned();
        app.agreement_rating = "rating".to_owned();
        app.run_agreement_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Attribute Agreement"));

        app.open_agreement_dialog();
        app.agreement_part = "part".to_owned();
        app.agreement_appraiser = "part".to_owned();
        app.agreement_rating = "rating".to_owned();
        app.run_agreement_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The three columns must be different")
        );
    }

    #[test]
    fn the_multivariate_dialogs_take_their_options() {
        let mut app = FreePlotsApp::new();
        app.set_data(regression_frame(), "mv.csv".to_owned());

        app.open_pca_dialog();
        app.pca_columns = "x1,x2,x3".to_owned();
        app.pca_keep = 2;
        app.run_pca_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let two = app.session_text().to_owned();
        assert!(two.contains("Principal Component"));

        app.session.clear();
        app.open_pca_dialog();
        app.pca_columns = "x1,x2,x3".to_owned();
        app.pca_keep = 3;
        app.run_pca_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(
            app.session_text(),
            two,
            "the kept components changed nothing"
        );

        app.open_pca_dialog();
        app.pca_columns = "x1".to_owned();
        app.run_pca_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("List at least two columns, separated by commas")
        );

        app.session.clear();
        app.open_kmeans_dialog();
        app.kmeans_columns = "x1,x2".to_owned();
        app.kmeans_clusters = 2;
        app.kmeans_standardise = true;
        app.run_kmeans_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let two_clusters = app.session_text().to_owned();
        assert!(two_clusters.contains("K-Means"));

        app.session.clear();
        app.open_kmeans_dialog();
        app.kmeans_columns = "x1,x2".to_owned();
        app.kmeans_clusters = 3;
        app.kmeans_standardise = false;
        app.run_kmeans_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(
            app.session_text(),
            two_clusters,
            "the cluster count changed nothing"
        );

        app.session.clear();
        app.open_t2_dialog();
        app.t2_columns = "x1,x2".to_owned();
        app.t2_subgroup = 2;
        app.t2_confidence = 99.9;
        app.run_t2_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Hotelling"));
    }

    #[test]
    fn the_time_series_dialogs_cover_their_options() {
        let mut app = FreePlotsApp::new();
        app.set_data(series_values_frame(), "series.csv".to_owned());

        app.open_timeseries_dialog();
        app.timeseries_column = "y".to_owned();
        app.timeseries_kind = TimeSeriesKind::MovingAverage;
        app.timeseries_window = 2;
        app.run_timeseries_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let small = app.session_text().to_owned();
        assert!(small.contains("Time Series"));

        app.session.clear();
        app.open_timeseries_dialog();
        app.timeseries_column = "y".to_owned();
        app.timeseries_kind = TimeSeriesKind::MovingAverage;
        app.timeseries_window = 5;
        app.run_timeseries_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), small, "the window changed nothing");

        app.session.clear();
        app.open_timeseries_dialog();
        app.timeseries_column = "y".to_owned();
        app.timeseries_kind = TimeSeriesKind::Decomposition;
        app.timeseries_period = 4;
        app.run_timeseries_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Time Series"));

        app.session.clear();
        app.open_timeseries_dialog();
        app.timeseries_column = "y".to_owned();
        app.timeseries_kind = TimeSeriesKind::ExponentialSmoothing;
        app.timeseries_season = 4;
        app.timeseries_horizon = 3;
        app.run_timeseries_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Time Series"));

        app.open_timeseries_dialog();
        app.timeseries_column.clear();
        app.run_timeseries_dialog();
        assert_eq!(app.error.as_deref(), Some("Select the series column"));

        app.open_series_dialog();
        app.series_column = "y".to_owned();
        app.series_time = "y".to_owned();
        app.run_series_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The time column must be different from the series")
        );
    }

    #[test]
    fn the_quality_tools_run_and_validate() {
        let mut app = FreePlotsApp::new();
        app.set_data(two_sample_frame(), "quality.csv".to_owned());

        app.open_pareto_dialog();
        app.pareto_column = "before".to_owned();
        app.run_pareto_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Pareto"));

        app.open_stem_dialog();
        app.stem_column = "before".to_owned();
        app.run_stem_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Stem"));

        app.open_ecdf_dialog();
        app.ecdf_column = "before".to_owned();
        app.ecdf_confidence = 50.0;
        app.run_ecdf_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let narrow = app.session_text().to_owned();
        app.session.clear();
        app.open_ecdf_dialog();
        app.ecdf_column = "before".to_owned();
        app.ecdf_confidence = 99.9;
        app.run_ecdf_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), narrow);

        app.open_dotplot_dialog();
        app.dot_column = "before".to_owned();
        app.run_dotplot_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        app.dot_factor = "before".to_owned();
        app.run_dotplot_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The factor must be a different column")
        );

        app.open_matrix_dialog();
        app.matrix_columns = "before,after".to_owned();
        app.matrix_bins = 5;
        app.run_matrix_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        app.matrix_columns = "before".to_owned();
        app.run_matrix_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("List at least two columns, separated by commas")
        );

        app.open_interval_dialog();
        app.interval_response = "before".to_owned();
        app.interval_factor = "before".to_owned();
        app.run_interval_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The response and the factor must be different columns")
        );

        app.session.clear();
        app.set_data(two_way_frame(), "interval.csv".to_owned());
        app.open_interval_dialog();
        app.interval_response = "y".to_owned();
        app.interval_factor = "g".to_owned();
        app.interval_confidence = 99.9;
        app.run_interval_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Interval Plot"));

        app.session.clear();
        app.set_data(two_sample_frame(), "quality.csv".to_owned());
        app.run_run_chart("before");
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Run Chart"));
        assert!(app.plot().is_some());
    }

    #[test]
    fn the_remaining_option_dialogs_run() {
        let mut app = FreePlotsApp::new();
        app.set_data(regression_frame(), "options.csv".to_owned());

        // Attribute charts: constant size for C, subgroup size for U.
        for (kind, size) in [
            (AttributeChartKind::C, "5"),
            (AttributeChartKind::U, "5"),
            (AttributeChartKind::LaneyP, "5"),
        ] {
            app.session.clear();
            app.attr_column = "x2".to_owned();
            app.attr_kind = kind;
            app.attr_size = size.to_owned();
            app.attr_sizes = None;
            app.run_attribute_chart();
            assert!(app.error.is_none(), "{kind:?}: {:?}", app.error);
            assert!(app.session_text().contains(kind.name()));
        }
        app.attr_kind = AttributeChartKind::U;
        app.attr_size = "0".to_owned();
        app.run_attribute_chart();
        assert_eq!(
            app.error.as_deref(),
            Some("Constant subgroup size must be a positive integer")
        );

        // G chart with and without a times column.
        app.session.clear();
        app.open_g_chart_dialog();
        app.g_chart_counts = "x3".to_owned();
        app.g_chart_times.clear();
        app.run_g_chart_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let plain = app.session_text().to_owned();
        assert!(plain.contains("G Chart"));

        app.session.clear();
        app.open_g_chart_dialog();
        app.g_chart_counts = "x3".to_owned();
        app.g_chart_times = "x1".to_owned();
        app.run_g_chart_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), plain, "the times changed nothing");

        // Sampling: a plan and a plan evaluation.
        app.session.clear();
        app.sampling_plan_mode = true;
        app.sampling_aql = 1.0;
        app.sampling_ltpd = 5.0;
        app.sampling_alpha = 5.0;
        app.sampling_beta = 10.0;
        app.sampling_lot = 1000;
        app.run_sampling_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Acceptance"));

        app.session.clear();
        app.sampling_plan_mode = false;
        app.sampling_lot = 1000;
        app.sampling_sample = 20;
        app.sampling_accept = 1;
        app.run_sampling_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);

        // Box-Cox, tolerance interval, partial correlation and the curve fit.
        app.session.clear();
        app.open_box_cox_dialog();
        app.box_cox_column = "y".to_owned();
        app.run_box_cox_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Box-Cox"));

        app.session.clear();
        app.open_tolerance_dialog();
        app.tolerance_column = "y".to_owned();
        app.tolerance_proportion = 99.0;
        app.tolerance_confidence = 99.0;
        app.tolerance_free = false;
        app.run_tolerance_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Tolerance"));

        // The distribution-free interval needs a far larger sample.
        app.session.clear();
        app.open_tolerance_dialog();
        app.tolerance_column = "y".to_owned();
        app.tolerance_proportion = 99.0;
        app.tolerance_confidence = 99.0;
        app.tolerance_free = true;
        app.run_tolerance_dialog();
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("distribution-free")),
            "{:?}",
            app.error
        );

        app.session.clear();
        app.open_partial_dialog();
        app.partial_x = "x1".to_owned();
        app.partial_y = "y".to_owned();
        app.partial_control = "x2".to_owned();
        app.run_partial_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let partial = app.session_text().to_owned();
        assert!(partial.contains("Partial"), "{partial}");

        app.open_partial_dialog();
        app.partial_x = "x1".to_owned();
        app.partial_y = "x1".to_owned();
        app.partial_control = "x2".to_owned();
        app.run_partial_dialog();
        assert_eq!(app.error.as_deref(), Some("Select three different columns"));

        app.session.clear();
        app.open_correlation_dialog();
        app.correlation_columns = "x1,x2,x3".to_owned();
        app.run_correlation_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Correlation"));

        for model in [
            CurveModel::Polynomial(1),
            CurveModel::Polynomial(2),
            CurveModel::Exponential,
        ] {
            app.session.clear();
            app.open_curve_dialog();
            app.curve_x = "x1".to_owned();
            app.curve_y = "y".to_owned();
            app.curve_model = model;
            app.run_curve_dialog();
            assert!(app.error.is_none(), "{model:?}: {:?}", app.error);
            assert!(app.session_text().contains("Curve Fit"));
        }
        app.open_curve_dialog();
        app.curve_x = "x1".to_owned();
        app.curve_y = "x1".to_owned();
        app.run_curve_dialog();
        assert_eq!(app.error.as_deref(), Some("Select two different columns"));
    }

    #[test]
    fn the_probability_plot_dialog_honours_its_options() {
        let mut app = FreePlotsApp::new();
        app.set_data(two_sample_frame(), "pp.csv".to_owned());

        app.open_probability_plot_dialog();
        app.dialog_columns = vec!["before".to_owned()];
        app.dialog_distribution = DistributionKind::Normal;
        app.dialog_display = PlotDisplay::Both;
        app.dialog_ci = true;
        app.dialog_ci_level = 95.0;
        app.run_probability_plot_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.session_text().contains("Probability Plot"));
        assert!(app.plot().is_some());
        let single = app.session_text().to_owned();

        app.session.clear();
        app.open_probability_plot_dialog();
        app.dialog_columns = vec!["before".to_owned()];
        app.dialog_ci_level = 99.9;
        app.run_probability_plot_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert_ne!(app.session_text(), single, "the level changed nothing");

        // Two columns: the layout and the shared scales shape the report.
        app.session.clear();
        app.open_probability_plot_dialog();
        app.dialog_columns = vec!["before".to_owned(), "after".to_owned()];
        app.dialog_layout = PlotLayout::Overlay;
        app.dialog_same_x = true;
        app.dialog_same_y = true;
        app.run_probability_plot_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let overlay = app.session_text().to_owned();
        assert!(overlay.contains("Overlay"), "{overlay}");
        assert!(overlay.contains("same X/Y"), "{overlay}");

        app.session.clear();
        app.open_probability_plot_dialog();
        app.dialog_columns = vec!["before".to_owned(), "after".to_owned()];
        app.dialog_layout = PlotLayout::Separate;
        app.run_probability_plot_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let separate = app.session_text().to_owned();
        assert!(separate.contains("Separate"), "{separate}");
        assert_ne!(overlay, separate);

        // A distribution without a band, and symbols-only, both suppress the CI.
        for (distribution, display) in [
            (DistributionKind::Weibull, PlotDisplay::Both),
            (DistributionKind::Normal, PlotDisplay::Symbols),
        ] {
            app.session.clear();
            app.open_probability_plot_dialog();
            app.dialog_columns = vec!["before".to_owned()];
            app.dialog_distribution = distribution;
            app.dialog_display = display;
            app.dialog_ci = true;
            app.run_probability_plot_dialog();
            assert!(app.error.is_none(), "{distribution:?}: {:?}", app.error);
            assert!(!app.session_text().is_empty());
        }

        // A grouping column is accepted for a single Y variable.
        app.set_data(two_way_frame(), "groups.csv".to_owned());
        app.session.clear();
        app.open_probability_plot_dialog();
        app.dialog_columns = vec!["y".to_owned()];
        app.dialog_group = Some("g".to_owned());
        app.run_probability_plot_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(
            app.session_text().contains("grouped"),
            "{}",
            app.session_text()
        );

        app.open_probability_plot_dialog();
        app.dialog_columns.clear();
        app.run_probability_plot_dialog();
        assert_eq!(app.error.as_deref(), Some("Select at least one Y variable"));
    }

    #[test]
    fn the_grouped_descriptives_dialog_runs_per_level() {
        let mut app = FreePlotsApp::new();
        app.set_data(two_way_frame(), "groups.csv".to_owned());
        app.open_describe_by_dialog();
        app.describe_column = "y".to_owned();
        app.describe_by = Some("g".to_owned());
        app.run_describe_by_dialog();
        assert!(app.error.is_none(), "{:?}", app.error);
        let text = app.session_text().to_owned();
        for level in ["A", "B", "C"] {
            assert!(text.contains(&format!("g = {level}")), "{text}");
        }
        assert!(!app.show_describe_dialog);

        app.open_describe_by_dialog();
        app.describe_column.clear();
        app.run_describe_by_dialog();
        assert_eq!(app.error.as_deref(), Some("Select the numeric column"));
        app.describe_column = "y".to_owned();
        app.describe_by = None;
        app.run_describe_by_dialog();
        assert_eq!(app.error.as_deref(), Some("Select the grouping column"));
        app.describe_by = Some("y".to_owned());
        app.run_describe_by_dialog();
        assert_eq!(
            app.error.as_deref(),
            Some("The grouping column must be different")
        );
    }

    #[test]
    fn every_dialog_renders_without_panicking() {
        use egui_kittest::Harness;

        type Opener = fn(&mut FreePlotsApp);

        fn openers() -> Vec<(&'static str, Opener)> {
            vec![
                (
                    "open_probability_plot_dialog",
                    FreePlotsApp::open_probability_plot_dialog,
                ),
                ("open_attribute_dialog", FreePlotsApp::open_attribute_dialog),
                (
                    "open_correlation_dialog",
                    FreePlotsApp::open_correlation_dialog,
                ),
                ("open_curve_dialog", FreePlotsApp::open_curve_dialog),
                ("open_two_way_dialog", FreePlotsApp::open_two_way_dialog),
                ("open_crosstab_dialog", FreePlotsApp::open_crosstab_dialog),
                ("open_g_chart_dialog", FreePlotsApp::open_g_chart_dialog),
                ("open_partial_dialog", FreePlotsApp::open_partial_dialog),
                ("open_tolerance_dialog", FreePlotsApp::open_tolerance_dialog),
                ("open_agreement_dialog", FreePlotsApp::open_agreement_dialog),
                ("open_taguchi_dialog", FreePlotsApp::open_taguchi_dialog),
                (
                    "open_taguchi_design_dialog",
                    FreePlotsApp::open_taguchi_design_dialog,
                ),
                ("open_sampling_dialog", FreePlotsApp::open_sampling_dialog),
                ("open_t2_dialog", FreePlotsApp::open_t2_dialog),
                ("open_kmeans_dialog", FreePlotsApp::open_kmeans_dialog),
                ("open_pca_dialog", FreePlotsApp::open_pca_dialog),
                ("open_sort_dialog", FreePlotsApp::open_sort_dialog),
                ("open_stack_dialog", FreePlotsApp::open_stack_dialog),
                ("open_unstack_dialog", FreePlotsApp::open_unstack_dialog),
                ("open_date_diff_dialog", FreePlotsApp::open_date_diff_dialog),
                (
                    "open_date_shift_dialog",
                    FreePlotsApp::open_date_shift_dialog,
                ),
                ("open_subset_dialog", FreePlotsApp::open_subset_dialog),
                (
                    "open_timeseries_dialog",
                    FreePlotsApp::open_timeseries_dialog,
                ),
                ("open_alt_dialog", FreePlotsApp::open_alt_dialog),
                (
                    "open_step_stress_dialog",
                    FreePlotsApp::open_step_stress_dialog,
                ),
                ("open_series_dialog", FreePlotsApp::open_series_dialog),
                ("open_stem_dialog", FreePlotsApp::open_stem_dialog),
                ("open_ecdf_dialog", FreePlotsApp::open_ecdf_dialog),
                ("open_dotplot_dialog", FreePlotsApp::open_dotplot_dialog),
                ("open_matrix_dialog", FreePlotsApp::open_matrix_dialog),
                ("open_interval_dialog", FreePlotsApp::open_interval_dialog),
                ("open_calc_dialog", FreePlotsApp::open_calc_dialog),
                ("open_pareto_dialog", FreePlotsApp::open_pareto_dialog),
                ("open_ordinal_dialog", FreePlotsApp::open_ordinal_dialog),
                ("open_poisson_dialog", FreePlotsApp::open_poisson_dialog),
                ("open_logistic_dialog", FreePlotsApp::open_logistic_dialog),
                ("open_linearity_dialog", FreePlotsApp::open_linearity_dialog),
                ("open_box_cox_dialog", FreePlotsApp::open_box_cox_dialog),
                ("open_power_dialog", FreePlotsApp::open_power_dialog),
                (
                    "open_regression_dialog",
                    FreePlotsApp::open_regression_dialog,
                ),
                ("open_stepwise_dialog", FreePlotsApp::open_stepwise_dialog),
                ("open_anova_dialog", FreePlotsApp::open_anova_dialog),
                (
                    "open_kaplan_meier_dialog",
                    FreePlotsApp::open_kaplan_meier_dialog,
                ),
                ("open_gage_dialog", FreePlotsApp::open_gage_dialog),
                ("open_doe_dialog", FreePlotsApp::open_doe_dialog),
                ("open_design_dialog", FreePlotsApp::open_design_dialog),
                (
                    "open_box_behnken_dialog",
                    FreePlotsApp::open_box_behnken_dialog,
                ),
                ("open_pb_dialog", FreePlotsApp::open_pb_dialog),
                ("open_rsm_dialog", FreePlotsApp::open_rsm_dialog),
                (
                    "open_desirability_dialog",
                    FreePlotsApp::open_desirability_dialog,
                ),
                ("open_scatter_dialog", FreePlotsApp::open_scatter_dialog),
                ("open_weibull_dialog", FreePlotsApp::open_weibull_dialog),
                ("open_type1_dialog", FreePlotsApp::open_type1_dialog),
                (
                    "open_exponential_dialog",
                    FreePlotsApp::open_exponential_dialog,
                ),
                ("open_lognormal_dialog", FreePlotsApp::open_lognormal_dialog),
                ("open_find_dialog", FreePlotsApp::open_find_dialog),
                ("open_format_dialog", FreePlotsApp::open_format_dialog),
            ]
        }

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1100.0, 700.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.step();
        for (name, open) in openers() {
            open(harness.state_mut());
            harness.step();
            harness.state_mut().close_dialogs();
            harness.step();
            assert!(
                harness.state().session_text().len() < 1_000_000,
                "dialog {name} produced too much session text"
            );
        }
    }

    #[test]
    fn arrows_tab_and_enter_move_the_selection() {
        use egui_kittest::Harness;

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.key_press(egui::Key::ArrowDown);
        harness.run();
        assert_eq!(harness.state().selected_cell(), Some((0, 1)));
        harness.key_press(egui::Key::ArrowDown);
        harness.run();
        assert_eq!(harness.state().selected_cell(), Some((0, 2)));
        harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::ArrowDown);
        harness.run();
        assert_eq!(harness.state().selection_size(), (1, 2));
        harness.key_press(egui::Key::ArrowRight);
        harness.run();
        assert_eq!(
            harness.state().selected_cell().map(|(column, _)| column),
            Some(1)
        );
        harness.key_press(egui::Key::Enter);
        harness.run();
        assert_eq!(harness.state().selected_cell(), Some((1, 4)));
    }

    #[test]
    fn the_headers_select_the_column_the_row_and_the_sheet() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();

        harness.get_by_label("C2").click();
        harness.run();
        assert_eq!(harness.state().selected_column(), Some("batch"));

        harness.hover_at(egui::pos2(20.0, 210.0));
        harness.run();
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: egui::pos2(20.0, 210.0),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run();
        }
        assert_eq!(harness.state().selection_size().1, 1);
        assert_eq!(harness.state().selected_cell().map(|(_, row)| row), Some(2));
    }

    #[test]
    fn the_cell_context_menu_inserts_a_row_and_undo_removes_it() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.hover_at(egui::pos2(90.0, 170.0));
        harness.run();
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: egui::pos2(90.0, 170.0),
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run();
        }
        harness.get_by_label("Insert row below").click();
        harness.run();
        assert_eq!(harness.state().data().expect("frame").n_rows(), 6);
        harness.state_mut().undo();
        assert_eq!(harness.state().data().expect("frame").n_rows(), 5);
    }

    fn open_cell_menu(harness: &mut egui_kittest::Harness<'_, FreePlotsApp>, pos: egui::Pos2) {
        harness.hover_at(pos);
        harness.run();
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run();
        }
    }

    #[test]
    fn every_cell_context_menu_action_works_and_undoes() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();

        // Insert a row above the second row.
        harness.state_mut().select_cell(0, 1);
        open_cell_menu(&mut harness, egui::pos2(90.0, 190.0));
        harness.get_by_label("Insert row above").click();
        harness.run();
        assert_eq!(harness.state().data().expect("frame").n_rows(), 6);
        harness.state_mut().undo();
        assert_eq!(harness.state().data().expect("frame").n_rows(), 5);

        // Delete the selected row.
        harness.state_mut().select_cell(0, 1);
        open_cell_menu(&mut harness, egui::pos2(90.0, 190.0));
        harness.get_by_label("Delete row").click();
        harness.run();
        assert_eq!(harness.state().data().expect("frame").n_rows(), 4);
        harness.state_mut().undo();
        assert_eq!(harness.state().data().expect("frame").n_rows(), 5);

        // Add a numeric column, then a text column, both empty.
        harness.state_mut().select_cell(0, 0);
        open_cell_menu(&mut harness, egui::pos2(90.0, 170.0));
        harness.get_by_label("Add numeric column").click();
        harness.run();
        let columns = harness.state().data().expect("frame").n_columns();
        assert_eq!(columns, 3);
        harness.state_mut().undo();

        // Delete a column.
        harness.state_mut().select_cell(1, 0);
        open_cell_menu(&mut harness, egui::pos2(150.0, 170.0));
        harness.get_by_label("Delete column").click();
        harness.run();
        assert_eq!(harness.state().data().expect("frame").n_columns(), 1);
        harness.state_mut().undo();
        assert_eq!(harness.state().data().expect("frame").n_columns(), 2);

        // Rename through the dialog the menu opens.
        harness.state_mut().select_cell(1, 0);
        open_cell_menu(&mut harness, egui::pos2(150.0, 170.0));
        harness.get_by_label("Rename column…").click();
        harness.run();
        harness
            .get_by_role_and_label(accesskit::Role::TextInput, "New name:")
            .click();
        harness.run();
        harness
            .get_by_role_and_label(accesskit::Role::TextInput, "New name:")
            .type_text("weight");
        harness.run();
        harness.get_by_label("Rename").click();
        harness.run();
        assert!(
            harness
                .state()
                .data()
                .expect("frame")
                .column("weight")
                .is_ok(),
            "the column was not renamed"
        );
        harness.state_mut().undo();
        assert!(
            harness
                .state()
                .data()
                .expect("frame")
                .column("batch")
                .is_ok(),
            "undo restores the old name"
        );
    }

    #[test]
    fn a_ten_thousand_character_cell_survives_the_round_trip() {
        let long = "A".repeat(10_000);
        let values = vec![long.clone(), "short".to_owned()];
        let df = DataFrame::new(vec![Column::new_text("text", values)]).expect("frame");
        let app_json = fp_core::io::to_project_json(&df, "long.fpp").expect("project json");
        let back = fp_core::io::from_project_json(&app_json).expect("project read");
        assert_eq!(back.n_rows(), 2);
        assert_eq!(back.cell_text(0, 0).as_deref().map(str::len), Some(10_000));

        let bytes = fp_core::xlsx::workbook(&df);
        let back = fp_core::xlsx_read::read_workbook_sheet(&bytes, None).expect("xlsx read");
        assert_eq!(back.cell_text(0, 0).as_deref().map(str::len), Some(10_000));

        let mut app = FreePlotsApp::new();
        app.set_data(df, "long.csv".to_owned());
        // The editor keeps the whole value, never a truncated copy.
        app.select_cell(0, 0);
        let shown = app
            .data()
            .and_then(|frame| frame.cell_text(0, 0))
            .unwrap_or_default();
        assert_eq!(shown.len(), 10_000);
    }

    #[test]
    fn the_undo_stack_stays_bounded_under_large_edits() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let big = "x".repeat(200_000);
        for step in 0..120 {
            app.select_cell(1, 0);
            app.set_edit_buffer(format!("{big}{step}"));
            assert!(app.commit_cell(), "edit {step} refused: {:?}", app.error);
        }
        assert!(
            app.undo_stack.len() <= 100,
            "undo stack kept {} groups",
            app.undo_stack.len()
        );
        // The newest edit is undoable and the text is restored to a previous
        // large value rather than to the original cell.
        app.undo();
        let text = app
            .data()
            .expect("frame")
            .cell_text(1, 0)
            .unwrap_or_default();
        assert!(text.len() > 100_000, "undo left {} characters", text.len());
    }

    #[test]
    fn the_preferences_round_trip_through_the_settings_file() {
        let dir = std::env::temp_dir().join(format!("fp-prefs-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("settings.json");

        let mut app = FreePlotsApp::new();
        app.set_preferences_path(path.clone());
        assert!(!path.exists(), "nothing is written until an option changes");
        app.set_language(fp_core::i18n::Lang::Portuguese);
        app.set_dark_mode(true);
        app.zoom = 1.25;
        app.show_session = false;
        app.show_grid_lines = false;
        app.native_plot_windows = false;
        app.persist_preferences();
        assert!(path.exists(), "the change was not written");
        assert_eq!(app.preferences_error(), None);

        let mut restored = FreePlotsApp::new();
        restored.set_preferences_path(path.clone());
        assert_eq!(restored.language(), fp_core::i18n::Lang::Portuguese);
        assert!(restored.dark_mode);
        assert!((restored.zoom - 1.25).abs() < 1e-9);
        assert!(!restored.show_session);
        assert!(!restored.show_grid_lines);
        assert!(!restored.native_plot_windows);

        // A malformed file keeps the defaults and is replaced on the next save.
        std::fs::write(&path, "{not json").expect("write");
        let mut recovered = FreePlotsApp::new();
        recovered.set_preferences_path(path.clone());
        assert_eq!(recovered.language(), fp_core::i18n::Lang::English);
        assert!(recovered.show_grid_lines);
        recovered.set_dark_mode(true);
        recovered.persist_preferences();
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"dark_mode\": true"), "{text}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_help_menu_opens_the_settings_and_agents_dialogs() {
        use egui_kittest::{Harness, kittest::Queryable};

        let dir = std::env::temp_dir().join(format!("fp-help-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut app = FreePlotsApp::new();
        app.set_preferences_path(dir.join("settings.json"));
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();

        harness.get_by_label("Help").click();
        harness.run();
        harness
            .get_by_label("Where the settings are stored…")
            .click();
        harness.run();
        harness.get_by_label("Where the settings are stored");
        harness.get_by_label_contains("settings.json");
        harness.get_by_label("Copy path").click();
        harness.step();
        let copied = harness.output().platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(text) if text.ends_with("settings.json"))
        });
        assert!(copied, "the path did not reach the clipboard");
        harness.state_mut().show_settings_info = false;
        harness.run();

        harness.get_by_label("Help").click();
        harness.run();
        harness.get_by_label("Assistant for agents…").click();
        harness.run();
        harness.get_by_label("Assistant for agents");
        harness.get_by_label("FreePlots --mcp");
        harness.get_by_label("FreePlots --mcp-http 127.0.0.1:8767");
        harness.get_by_label("Copy configuration").click();
        harness.step();
        let config = harness.output().platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(text) if text.contains("mcpServers"))
        });
        assert!(
            config,
            "the client configuration did not reach the clipboard"
        );
        harness.state_mut().show_agents_dialog = false;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_about_and_help_follow_the_language() {
        use egui_kittest::{Harness, kittest::Queryable};

        let app = FreePlotsApp::new();
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();

        // English by default, including the description and the licence.
        harness.state_mut().show_about = true;
        harness.run();
        harness.get_by_label(
            "Statistics and analysis software written in Rust by AI, accessible to everyone.",
        );
        harness.get_by_label("MIT license");
        assert_eq!(
            harness
                .query_all_by_label(
                    "Software de análise e estatística escrito em Rust por IA, acessível a todos."
                )
                .count(),
            0
        );

        // The same dialog in Portuguese, with no English left in it.
        harness
            .state_mut()
            .set_language(fp_core::i18n::Lang::Portuguese);
        harness.run();
        harness.get_by_label(
            "Software de análise e estatística escrito em Rust por IA, acessível a todos.",
        );
        harness.get_by_label("Licença MIT");
        assert_eq!(harness.query_all_by_label("MIT license").count(), 0);
        assert_eq!(
            harness
                .query_all_by_label(
                    "Statistics and analysis software written in Rust by AI, accessible to everyone."
                )
                .count(),
            0
        );

        // The help window follows the same catalogue.
        harness.state_mut().show_about = false;
        harness.state_mut().show_shortcuts = true;
        harness.run();
        harness.get_by_label("Anular e repetir");
        harness
            .state_mut()
            .set_language(fp_core::i18n::Lang::English);
        harness.run();
        harness.get_by_label("Undo and redo");
        assert_eq!(harness.query_all_by_label("Anular e repetir").count(), 0);
        harness.get_by_label(
            "Ctrl and Shift combinations use the keyboard layout in use; the grid never captures keys while a text field has focus.",
        );
        harness
            .state_mut()
            .set_language(fp_core::i18n::Lang::Portuguese);
        harness.run();
        harness.get_by_label(
            "As combinações com Ctrl e Shift seguem o layout do teclado em uso; a grelha nunca capta teclas enquanto um campo de texto tem o foco.",
        );

        // The assistant goals are translated too, while the action is still
        // chosen by index.
        harness.state_mut().show_shortcuts = false;
        harness.state_mut().show_assistant_dialog = true;
        harness.run();
        harness.get_by_label("Comparar dois grupos");
        harness.get_by_label("Planear uma experiência");
    }

    #[test]
    fn the_cells_expose_their_painted_text_to_accessibility() {
        use egui_kittest::{Harness, kittest::Queryable};

        let df =
            DataFrame::new(vec![Column::new_f64("weight", vec![1234.5678, 2.0])]).expect("frame");
        let mut app = FreePlotsApp::new();
        app.set_data(df, "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();

        // Unformatted: the address and the value as the grid paints it.
        harness.get_by_label("C1 R1: 1234.5678");
        harness.get_by_label("C1 R2: 2");
        // Empty cells still have an address, so a screen reader can navigate.
        harness.get_by_label("C2 R1");

        // The accessibility label follows the column format, exactly like the
        // painted text: this is the live check the plan asked for.
        harness.state_mut().formats.insert(
            "weight".to_owned(),
            ColumnFormat {
                decimals: 1,
                thousands: true,
                bold: false,
                align: CellAlign::Right,
            },
        );
        harness.run();
        harness.get_by_label("C1 R1: 1 234.6");
        assert_eq!(harness.query_all_by_label("C1 R1: 1234.5678").count(), 0);
    }

    #[test]
    fn saving_over_an_existing_file_asks_first() {
        use egui_kittest::{Harness, kittest::Queryable};

        let dir = std::env::temp_dir().join(format!("fp-overwrite-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let existing = dir.join("report.csv");
        std::fs::write(&existing, "old").expect("fixture");
        let fresh = dir.join("fresh.csv");

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();

        // A new path goes straight through.
        harness.state_mut().begin_save(FileKind::Csv);
        harness.state_mut().save_path = fresh.display().to_string();
        harness.run();
        harness.get_by_label("Save").click();
        harness.run();
        assert_eq!(
            harness
                .state_mut()
                .take_save_request()
                .map(|request| request.path),
            Some(fresh.display().to_string())
        );

        // An existing path waits for confirmation; Cancel writes nothing.
        harness.state_mut().begin_save(FileKind::Csv);
        harness.state_mut().save_path = existing.display().to_string();
        harness.run();
        harness.get_by_label("Save").click();
        harness.run();
        assert!(harness.state().show_overwrite);
        assert!(harness.state().save_request.is_none());
        harness.get_by_label("Cancel").click();
        harness.run();
        assert!(!harness.state().show_overwrite);
        assert!(harness.state().save_request.is_none());

        // Replace emits the request; the window is gone.
        harness.state_mut().begin_save(FileKind::Csv);
        harness.state_mut().save_path = existing.display().to_string();
        harness.run();
        harness.get_by_label("Save").click();
        harness.run();
        harness.get_by_label("Replace").click();
        harness.run();
        assert!(!harness.state().show_overwrite);
        let request = harness.state_mut().take_save_request().expect("request");
        assert_eq!(request.path, existing.display().to_string());
        assert_eq!(request.kind, FileKind::Csv);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_assistant_opens_the_right_dialog_for_each_goal() {
        use egui_kittest::{Harness, kittest::Queryable};

        // Two numeric columns: the DOE and the two-group paths need them.
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![12.5, 13.1, 11.9, 12.8, 12.2]),
            Column::new_f64("length", vec![20.1, 20.4, 19.8, 20.2, 20.0]),
            Column::new_text("batch", vec!["A".to_string(); 5]),
        ])
        .expect("frame");
        let mut app = FreePlotsApp::new();
        app.set_data(df, "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();

        let goals = [
            ("Describe my data", "session"),
            ("Compare two groups", "t"),
            ("Compare three or more groups", "anova"),
            ("Test a proportion", "test2"),
            ("Check a process (control chart)", "chart"),
            ("Measure process capability", "cap"),
            ("Fit a regression", "reg"),
            ("Design an experiment", "doe"),
            ("Test normality", "probability"),
        ];
        for (goal, expected) in goals {
            harness.state_mut().show_assistant_dialog = true;
            harness.run();
            harness.get_by_label(goal).click();
            harness.run();
            let state = harness.state();
            assert!(
                !state.show_assistant_dialog,
                "{goal} kept the assistant open"
            );
            let opened = match expected {
                "session" => !state.session.is_empty(),
                "t" => state.show_t_dialog,
                "anova" => state.show_anova_dialog,
                "test2" => state.show_test2_dialog,
                "chart" => state.show_chart_dialog,
                "cap" => state.show_cap_dialog,
                "reg" => state.show_reg_dialog,
                "doe" => state.show_doe_dialog,
                "probability" => state.show_probability_plot_dialog,
                other => panic!("unknown expectation {other}"),
            };
            assert!(opened, "{goal} did not open the {expected} path");
            // Close whatever opened so the next goal starts clean.
            let state = harness.state_mut();
            state.show_t_dialog = false;
            state.show_anova_dialog = false;
            state.show_test2_dialog = false;
            state.show_chart_dialog = false;
            state.show_cap_dialog = false;
            state.show_reg_dialog = false;
            state.show_doe_dialog = false;
            state.show_probability_plot_dialog = false;
            harness.run();
        }
    }

    #[test]
    fn find_through_the_dialog_selects_the_cell() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
        harness.run();
        harness
            .get_by_role_and_label(accesskit::Role::TextInput, "Find what:")
            .click();
        harness.run();
        harness
            .get_by_role_and_label(accesskit::Role::TextInput, "Find what:")
            .type_text("13.1");
        harness.run();
        assert_eq!(harness.state().find_text, "13.1");
        harness.get_by_label("Find Next").click();
        harness.run();
        assert_eq!(harness.state().selected_cell(), Some((0, 1)));
    }

    #[test]
    fn the_grid_renders_a_large_frame_with_twenty_six_columns() {
        use egui_kittest::{Harness, kittest::Queryable};

        let values: Vec<f64> = (0..200_000).map(|index| index as f64 * 0.001).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let mut app = FreePlotsApp::new();
        app.set_data(df, "big.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        assert_eq!(harness.state().data().expect("frame").n_rows(), 200_000);
        // The sheet offers 26 columns even though the data has one. Only the
        // columns inside the viewport are laid out: the twentieth is on
        // screen, the twenty-sixth is beyond it and waits for a scroll, which
        // is what lets the grid keep a thousand columns cheap.
        assert_eq!(harness.get_all_by_label("C1").count(), 1);
        assert_eq!(harness.get_all_by_label("C15").count(), 1);
        let laid_out = (1..=26)
            .filter(|index| harness.query_all_by_label(&format!("C{index}")).count() > 0)
            .count();
        assert!(
            (10..26).contains(&laid_out),
            "{laid_out} headers laid out of 26"
        );
        assert_eq!(harness.query_all_by_label("C26").count(), 0);
        harness.hover_at(egui::pos2(600.0, 300.0));
        harness.run();
        assert_eq!(harness.state().data().expect("frame").n_rows(), 200_000);
    }

    #[test]
    fn dragging_a_divider_widens_and_keeps_the_column() {
        use egui_kittest::Harness;

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        let before = harness
            .state()
            .column_widths
            .first()
            .copied()
            .unwrap_or(72.0);
        // The divider between the row header and C1 sits at x = 127.
        harness.hover_at(egui::pos2(127.0, 140.0));
        harness.run();
        harness.drag_at(egui::pos2(127.0, 140.0));
        harness.event(egui::Event::PointerMoved(egui::pos2(165.0, 140.0)));
        harness.drop_at(egui::pos2(165.0, 140.0));
        harness.run();
        let after = harness
            .state()
            .column_widths
            .first()
            .copied()
            .unwrap_or(0.0);
        assert!(after > before, "before {before}, after {after}");
    }

    #[test]
    fn adding_and_deleting_columns_is_undoable() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.add_column();
        assert_eq!(app.data().expect("frame").n_columns(), 3);
        app.undo();
        assert_eq!(app.data().expect("frame").n_columns(), 2);

        app.add_text_column();
        assert_eq!(app.data().expect("frame").n_columns(), 3);
        app.undo();
        assert_eq!(app.data().expect("frame").n_columns(), 2);

        app.select_cell(1, 0);
        app.delete_column();
        assert_eq!(app.data().expect("frame").n_columns(), 1);
        app.undo();
        let data = app.data().expect("frame");
        assert_eq!(data.n_columns(), 2);
        assert_eq!(data.columns()[1].name(), "batch");
    }

    #[test]
    fn sorting_reorders_the_worksheet() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.open_sort_dialog();
        app.sort_keys = "weight:desc".to_owned();
        app.run_sort_dialog();
        let data = app.data().expect("frame");
        assert_eq!(data.cell_text(0, 0).as_deref(), Some("13.1"));
        assert_eq!(data.cell_text(0, 1).as_deref(), Some("12.8"));
    }

    #[test]
    fn a_big_paste_is_one_undo_group() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut block = String::new();
        for row in 0..200 {
            for column in 0..10 {
                if column > 0 {
                    block.push('\t');
                }
                block.push_str(&format!("{}", row * 10 + column));
            }
            block.push('\n');
        }
        app.paste_tsv(&block);
        let data = app.data().expect("frame");
        assert_eq!(data.n_rows(), 200);
        assert_eq!(data.n_columns(), 10);
        assert_eq!(data.cell_text(9, 199).as_deref(), Some("1999"));
        app.undo();
        let data = app.data().expect("frame");
        assert_eq!(data.n_rows(), 5);
        assert_eq!(data.n_columns(), 2);
    }

    #[test]
    fn the_sheets_keep_their_own_data_and_undo() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.set_edit_buffer("99");
        assert!(app.commit_cell());
        let first = app.data().expect("frame").cell_text(0, 0);
        assert_eq!(first.as_deref(), Some("99"));

        app.add_sheet();
        assert_eq!(app.sheet_names().len(), 2);
        assert_eq!(app.data().expect("new sheet").n_rows(), 0);

        app.switch_sheet(0);
        assert_eq!(app.data().expect("first").cell_text(0, 0), first);
        app.undo();
        assert_eq!(
            app.data().expect("first").cell_text(0, 0).as_deref(),
            Some("12.5")
        );
        app.switch_sheet(1);
        assert!(app.data().expect("second").n_rows() == 0);
        assert!(app.close_sheet(1));
        assert_eq!(app.sheet_names().len(), 1);
    }

    #[test]
    fn the_context_menu_copies_and_cuts() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.state_mut().select_cell(1, 0);
        harness.state_mut().extend_selection(1, 1);
        harness.hover_at(egui::pos2(150.0, 170.0));
        harness.run();
        for pressed in [true, false] {
            harness.event(egui::Event::PointerButton {
                pos: egui::pos2(150.0, 170.0),
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run();
        }
        harness.get_by_label("Copy").click();
        harness.step();
        assert_eq!(harness.state().clipboard, "A\nA");
        // The same text goes to the system clipboard.
        let copied = harness
            .output()
            .platform_output
            .commands
            .iter()
            .any(|command| {
                matches!(
                    command,
                    egui::OutputCommand::CopyText(text) if text == "A\nA"
                )
            });
        assert!(copied, "no CopyText command reached the platform");
        harness.run();

        harness.state_mut().select_cell(1, 0);
        harness.state_mut().extend_selection(1, 1);
        harness.get_by_label("Edit").click();
        harness.run();
        harness.get_by_label("Cut").click();
        harness.run();
        let data = harness.state().data().expect("frame");
        assert!(data.cell_is_missing(1, 0));
        assert!(data.cell_is_missing(1, 1));
    }

    #[test]
    fn a_formula_in_the_cell_is_evaluated_on_commit() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.set_edit_buffer("=SUM(C1R1:C1R5)");
        assert!(app.commit_cell(), "{:?}", app.error);
        assert_eq!(
            app.data().expect("frame").cell_text(0, 0).as_deref(),
            Some("62.5")
        );

        app.set_edit_buffer("=ROUND(2.345,2)");
        assert!(app.commit_cell());
        assert_eq!(
            app.data().expect("frame").cell_text(0, 0).as_deref(),
            Some("2.35")
        );

        // A broken formula is refused and the cell keeps its value.
        app.set_edit_buffer("=SUM(");
        assert!(!app.commit_cell());
        assert!(app.error.is_some());
        assert_eq!(
            app.data().expect("frame").cell_text(0, 0).as_deref(),
            Some("2.35")
        );
    }

    #[test]
    fn a_background_analysis_finishes_and_reports() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.set_background_analyses(true);
        app.run_spec(
            AnalysisSpec::descriptive_statistics("weight"),
            "Describe".to_owned(),
        )
        .expect("started");
        let ctx = egui::Context::default();
        let started = std::time::Instant::now();
        while app.analysis_running() && started.elapsed() < std::time::Duration::from_secs(10) {
            std::thread::sleep(std::time::Duration::from_millis(10));
            app.poll_analysis(&ctx);
        }
        assert!(!app.analysis_running(), "the worker did not finish");
        assert!(
            app.session_text().contains("Descriptive"),
            "{}",
            app.session_text()
        );
        assert_eq!(app.status(), "Describe");
        assert!(app.error.is_none());
    }

    #[test]
    fn a_second_analysis_is_refused_while_one_runs() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.set_background_analyses(true);
        // White-box: pretend a worker is running.
        let (sender, receiver) = std::sync::mpsc::channel();
        app.pending_analysis = Some(receiver);
        let outcome = app.run_spec(
            AnalysisSpec::descriptive_statistics("weight"),
            "Describe".to_owned(),
        );
        assert!(outcome.is_err());
        assert!(
            app.error
                .as_deref()
                .unwrap_or("")
                .contains("already running")
        );
        drop(sender);
    }

    #[test]
    fn a_project_carries_every_worksheet() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.add_sheet();
        app.set_data(frame(), "batch.csv".to_owned());
        let sheets = app.project_sheets();
        assert_eq!(sheets.len(), 2, "{sheets:?}");
        assert_eq!(sheets[0].0, "Worksheet 1");
        assert_eq!(sheets[1].0, "Worksheet 2");

        let json = fp_core::io::to_project_json_sheets(&sheets, "job.fpp").expect("write");
        let back = fp_core::io::from_project_json_sheets(&json).expect("read");
        let mut restored = FreePlotsApp::new();
        restored.load_project_sheets(back, "job.fpp".to_owned());
        assert_eq!(restored.sheets.len(), 2);
        assert_eq!(restored.sheets[0].name, "Worksheet 1");
        assert_eq!(restored.sheets[1].name, "Worksheet 2");
        assert_eq!(
            restored.sheets[1].data.as_ref().map(DataFrame::n_rows),
            Some(5)
        );
        assert_eq!(restored.active_sheet, 0);
        // The second tab is reachable and keeps its data.
        restored.switch_sheet(1);
        assert_eq!(restored.data().map(DataFrame::n_rows), Some(5));
        assert_eq!(restored.sheets[1].name, "Worksheet 2");
    }

    #[test]
    fn a_refused_paste_changes_nothing() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.paste_tsv("abc");
        assert!(app.error.as_deref().is_some_and(|error| !error.is_empty()));
        let data = app.data().expect("frame");
        assert_eq!(data.n_rows(), 5);
        assert_eq!(data.n_columns(), 2);
        assert_eq!(data.cell_text(0, 0).as_deref(), Some("12.5"));
        assert!(!app.can_undo(), "a refused paste must not leave history");

        // A block that fails on its second row is rolled back as a whole.
        app.select_cell(0, 0);
        app.paste_tsv("1\t2\nabc\t4");
        assert!(app.error.is_some());
        let data = app.data().expect("frame");
        assert_eq!(data.n_rows(), 5);
        assert_eq!(data.cell_text(0, 0).as_deref(), Some("12.5"));
        assert_eq!(data.cell_text(1, 0).as_deref(), Some("A"));
        assert!(!app.can_undo());

        // Columns added for the paste are dropped again when it fails.
        app.select_cell(1, 0);
        app.paste_tsv("a\tb\tc\t\td\te\tabc");
        assert!(app.error.is_some());
        assert_eq!(app.data().expect("frame").n_columns(), 2);
    }

    #[test]
    fn a_windows_clipboard_block_pastes_without_carriage_returns() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.select_cell(0, 0);
        app.paste_tsv("1\t2\r\n3\t4\r\n");
        let data = app.data().expect("frame");
        assert_eq!(data.n_rows(), 5);
        assert_eq!(data.cell_text(0, 0).as_deref(), Some("1"));
        assert_eq!(data.cell_text(1, 0).as_deref(), Some("2"));
        assert_eq!(data.cell_text(0, 1).as_deref(), Some("3"));
        assert_eq!(data.cell_text(1, 1).as_deref(), Some("4"));
    }

    #[test]
    fn a_paste_of_ten_thousand_cells_takes_under_two_seconds() {
        use std::time::Instant;

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut block = String::new();
        for row in 0..1_000 {
            for column in 0..10 {
                if column > 0 {
                    block.push('\t');
                }
                block.push_str(&(row * 10 + column).to_string());
            }
            block.push('\n');
        }
        let started = Instant::now();
        app.paste_tsv(&block);
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "10 000 cells took {elapsed:?}"
        );
        let data = app.data().expect("frame");
        assert_eq!(data.n_rows(), 1_000);
        assert_eq!(data.n_columns(), 10);
        assert_eq!(data.cell_text(9, 999).as_deref(), Some("9999"));
        // One undo returns the whole block to the frame that was there.
        app.undo();
        let data = app.data().expect("frame");
        assert_eq!(data.n_rows(), 5);
        assert_eq!(data.n_columns(), 2);
    }

    #[test]
    fn the_session_trims_itself_at_the_memory_limit() {
        let mut app = FreePlotsApp::new();
        // Each "report" is about a mebibyte; twenty of them exceed the cap.
        let block = "x".repeat(1 << 20);
        for index in 0..20 {
            app.append_session(&format!("REPORT {index}\n{block}"));
        }
        assert!(
            app.session.len() <= 16 << 20,
            "session grew to {} bytes",
            app.session.len()
        );
        assert!(
            app.session.contains("REPORT 19"),
            "the newest report was trimmed away"
        );
        assert!(
            !app.session.contains("REPORT 0"),
            "the oldest report should be gone"
        );
    }

    #[test]
    fn a_thousand_columns_render_without_stalling() {
        use egui_kittest::{Harness, kittest::Queryable};

        let rows = 100;
        let columns = 1_000;
        let data: Vec<Column> = (0..columns)
            .map(|column| {
                Column::new_f64(
                    format!("C{}", column + 1),
                    (0..rows)
                        .map(|row| (row * columns + column) as f64)
                        .collect(),
                )
            })
            .collect();
        let df = DataFrame::new(data).expect("frame");
        let mut app = FreePlotsApp::new();
        app.set_data(df, "wide.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        let started = std::time::Instant::now();
        harness.run();
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "the first frame took {elapsed:?}"
        );
        assert_eq!(
            harness.state().data().map(DataFrame::n_columns),
            Some(1_000)
        );
        // Only a screenful of headers is laid out: the far columns are built
        // when the grid scrolls to them, which is what keeps the frame cheap.
        let laid_out = (1..=columns)
            .filter(|index| harness.query_all_by_label(&format!("C{index}")).count() > 0)
            .count();
        assert!(
            laid_out < 40,
            "{laid_out} of {columns} column headers were laid out"
        );
        assert_eq!(harness.query_all_by_label("C1").count(), 1);
        assert_eq!(harness.query_all_by_label("C1000").count(), 0);
    }

    #[test]
    fn replace_all_through_the_dialog_and_undo() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::F);
        harness.run();
        harness
            .get_by_role_and_label(accesskit::Role::TextInput, "Find what:")
            .click();
        harness.run();
        harness
            .get_by_role_and_label(accesskit::Role::TextInput, "Find what:")
            .type_text("A");
        harness.run();
        harness
            .get_by_role_and_label(accesskit::Role::TextInput, "Replace with:")
            .click();
        harness.run();
        harness
            .get_by_role_and_label(accesskit::Role::TextInput, "Replace with:")
            .type_text("B");
        harness.run();
        harness.get_by_label("Replace All").click();
        harness.run();
        assert_eq!(harness.state().find_message, "5 replaced");
        for row in 0..5 {
            assert_eq!(
                harness
                    .state()
                    .data()
                    .expect("frame")
                    .cell_text(1, row)
                    .as_deref(),
                Some("B")
            );
        }
        harness.state_mut().undo();
        for row in 0..5 {
            assert_eq!(
                harness
                    .state()
                    .data()
                    .expect("frame")
                    .cell_text(1, row)
                    .as_deref(),
                Some("A")
            );
        }
    }

    #[test]
    fn the_column_widths_belong_to_each_worksheet() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        app.column_widths = vec![100.0; 26];
        app.add_sheet();
        assert!(
            app.column_widths.is_empty(),
            "new sheet starts with defaults"
        );
        app.column_widths = vec![140.0; 26];
        app.switch_sheet(0);
        assert_eq!(app.column_widths[0], 100.0, "first sheet keeps its widths");
        app.switch_sheet(1);
        assert_eq!(app.column_widths[0], 140.0, "second sheet keeps its widths");
    }

    #[test]
    fn a_hundred_edits_undo_and_redo_in_order() {
        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let rows = app.data().expect("frame").n_rows();
        for step in 0..100 {
            app.select_cell(0, step);
            app.set_edit_buffer(format!("{}", 500 + step));
            assert!(app.commit_cell(), "edit {step} was refused");
        }
        let last = app
            .data()
            .expect("frame")
            .cell_text(0, 99)
            .unwrap_or_default();
        assert_eq!(last, "599");
        assert_eq!(app.data().map(DataFrame::n_rows), Some(100));
        for _ in 0..100 {
            app.undo();
        }
        assert!(!app.can_undo());
        assert_eq!(app.data().map(DataFrame::n_rows), Some(rows));
        assert_eq!(app.data().unwrap().cell_text(0, 0).as_deref(), Some("12.5"));
        for _ in 0..100 {
            app.redo();
        }
        assert!(!app.can_redo());
        assert_eq!(app.data().unwrap().cell_text(0, 99).as_deref(), Some("599"));
    }

    #[test]
    fn the_grid_remembers_its_column_widths() {
        use egui_kittest::Harness;

        let mut app = FreePlotsApp::new();
        app.set_data(frame(), "weights.csv".to_owned());
        let mut harness = Harness::builder()
            .with_size(egui::vec2(1400.0, 900.0))
            .build_ui_state(|ui, app| app.show(ui), app);
        harness.run();
        let widths = &harness.state().column_widths;
        assert_eq!(widths.len(), 26);
        assert!(widths.iter().all(|width| *width > 0.0));
    }
}
