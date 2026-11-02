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

impl Default for FreePlotsApp {
    fn default() -> Self {
        Self::new()
    }
}

impl FreePlotsApp {
    /// Empty workspace.
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: Some(DataFrame::blank(8, 0).expect("blank worksheet")),
            source: "Untitled".to_owned(),
            sheets: vec![SheetState {
                name: "Worksheet 1".to_owned(),
                data: None,
                source: "Untitled".to_owned(),
                undo: Vec::new(),
                redo: Vec::new(),
                selected_cell: Some((0, 0)),
                column_widths: Vec::new(),
            }],
            active_sheet: 0,
            tab_first: 0,
            clipboard: String::new(),
            paste_request: false,
            background_analyses: false,
            pending_analysis: None,
            show_find_dialog: false,
            find_text: String::new(),
            replace_text: String::new(),
            find_match_case: false,
            find_whole_cell: false,
            find_message: String::new(),
            column_widths: Vec::new(),
            formats: std::collections::HashMap::new(),
            show_format_dialog: false,
            format_decimals: 6,
            format_thousands: false,
            format_bold: false,
            format_align: CellAlign::Left,
            print_request: None,
            show_assistant_dialog: false,
            dark_mode: false,
            show_shortcuts: false,
            show_grid_lines: true,
            show_session: true,
            selected: Some("C1".to_owned()),
            selected_cell: Some((0, 0)),
            selection_anchor: Some((0, 0)),
            edit_target: Some((0, 0)),
            edit_buffer: String::new(),
            edit_dirty: false,
            formula_focused: false,
            inline_edit: None,
            inline_buffer: String::new(),
            inline_request_focus: false,
            column_summary: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            distribution: DistributionKind::Normal,
            show_probability_plot_dialog: false,
            dialog_columns: Vec::new(),
            dialog_group: None,
            dialog_layout: PlotLayout::Separate,
            dialog_same_x: false,
            dialog_same_y: false,
            show_cap_dialog: false,
            cap_column: String::new(),
            cap_distribution: DistributionKind::Normal,
            cap_lsl: String::new(),
            cap_usl: String::new(),
            cap_target: String::new(),
            cap_subgroup: 1,
            cap_confidence: 95.0,
            cap_bootstrap: 0,
            cap_box_cox: false,
            cap_require_stability: false,
            show_chart_dialog: false,
            chart_column: String::new(),
            chart_kind: ControlChartKind::IndividualsMovingRange,
            chart_subgroup: 5,
            chart_subgroup_column: String::new(),
            chart_tests: TestSet::default(),
            chart_cusum_k: 0.5,
            chart_cusum_h: 4.0,
            chart_ewma_lambda: 0.2,
            chart_ewma_limit: 3.0,
            show_attr_dialog: false,
            attr_column: String::new(),
            attr_kind: AttributeChartKind::P,
            attr_sizes: None,
            attr_size: "100".to_owned(),
            show_t_dialog: false,
            show_equivalence_dialog: false,
            rank_median: String::new(),
            show_rank_dialog: false,
            show_correlation_dialog: false,
            language: fp_core::i18n::Lang::English,
            show_curve_dialog: false,
            show_two_way_dialog: false,
            show_crosstab_dialog: false,
            show_g_chart_dialog: false,
            show_partial_dialog: false,
            show_box_cox_dialog: false,
            box_cox_column: String::new(),
            show_tolerance_dialog: false,
            show_sort_dialog: false,
            sort_keys: String::new(),
            show_stack_dialog: false,
            stack_columns: String::new(),
            show_unstack_dialog: false,
            unstack_value: String::new(),
            unstack_group: String::new(),
            show_subset_dialog: false,
            show_date_diff_dialog: false,
            date_diff_first: String::new(),
            date_diff_second: String::new(),
            date_diff_name: "days".to_owned(),
            show_date_shift_dialog: false,
            date_shift_column: String::new(),
            date_shift_days: 0,
            date_shift_name: "shifted".to_owned(),
            subset_column: String::new(),
            subset_op: fp_core::reshape::SubsetOp::Equal,
            subset_value: String::new(),
            show_agreement_dialog: false,
            agreement_part: String::new(),
            agreement_appraiser: String::new(),
            agreement_rating: String::new(),
            show_kmeans_dialog: false,
            kmeans_columns: String::new(),
            kmeans_clusters: 3,
            kmeans_standardise: true,
            kmeans_seed: 2026,
            show_taguchi_dialog: false,
            taguchi_response: String::new(),
            taguchi_factors: String::new(),
            taguchi_quality: fp_core::taguchi::Quality::Smaller,
            show_taguchi_design_dialog: false,
            taguchi_runs: 8,
            taguchi_design_factors: 3,
            show_sampling_dialog: false,
            sampling_plan_mode: false,
            sampling_sample: 20,
            sampling_accept: 1,
            sampling_lot: 0,
            sampling_aql: 1.0,
            sampling_ltpd: 5.0,
            sampling_alpha: 5.0,
            sampling_beta: 10.0,
            show_t2_dialog: false,
            t2_columns: String::new(),
            t2_subgroup: 3,
            t2_confidence: 95.0,
            show_pca_dialog: false,
            pca_columns: String::new(),
            pca_keep: 2,
            show_timeseries_dialog: false,
            timeseries_column: String::new(),
            timeseries_kind: TimeSeriesKind::MovingAverage,
            timeseries_window: 3,
            timeseries_period: 12,
            timeseries_model: fp_core::timeseries::DecompositionModel::Additive,
            timeseries_season: 12,
            timeseries_horizon: 4,
            show_alt_dialog: false,
            alt_time: String::new(),
            alt_stress: String::new(),
            alt_mode: String::new(),
            alt_horizon: String::new(),
            show_step_stress_dialog: false,
            ss_duration: String::new(),
            ss_stress: String::new(),
            ss_unit: String::new(),
            ss_event: String::new(),
            ss_use_stress: 300.0,
            ss_life: fp_core::alt::LifeModel::Weibull,
            ss_acceleration: fp_core::alt::AccelerationModel::Arrhenius,
            alt_use_stress: 300.0,
            alt_life: fp_core::alt::LifeModel::Weibull,
            alt_acceleration: fp_core::alt::AccelerationModel::Arrhenius,
            show_series_dialog: false,
            series_column: String::new(),
            series_time: String::new(),
            show_stem_dialog: false,
            stem_column: String::new(),
            show_ecdf_dialog: false,
            ecdf_column: String::new(),
            ecdf_confidence: 95.0,
            show_dot_dialog: false,
            dot_column: String::new(),
            dot_factor: String::new(),
            show_matrix_dialog: false,
            matrix_columns: String::new(),
            matrix_bins: 8,
            show_interval_dialog: false,
            interval_response: String::new(),
            interval_factor: String::new(),
            interval_confidence: 95.0,
            show_sheet_dialog: false,
            sheet_path: String::new(),
            sheet_names: Vec::new(),
            sheet_choice: String::new(),
            show_calc_dialog: false,
            calc_name: String::new(),
            calc_expression: String::new(),
            show_pareto_dialog: false,
            pareto_column: String::new(),
            show_ordinal_dialog: false,
            ordinal_response: String::new(),
            ordinal_predictors: String::new(),
            show_poisson_dialog: false,
            poisson_response: String::new(),
            poisson_predictors: String::new(),
            show_logistic_dialog: false,
            logistic_response: String::new(),
            logistic_predictors: String::new(),
            show_linearity_dialog: false,
            linearity_measurement: String::new(),
            linearity_reference: String::new(),
            tolerance_column: String::new(),
            tolerance_proportion: 95.0,
            tolerance_confidence: 95.0,
            tolerance_free: false,
            partial_x: String::new(),
            partial_y: String::new(),
            partial_control: String::new(),
            g_chart_counts: String::new(),
            g_chart_times: String::new(),
            crosstab_row: String::new(),
            crosstab_column: String::new(),
            two_way_response: String::new(),
            two_way_first: String::new(),
            two_way_second: String::new(),
            curve_x: String::new(),
            curve_y: String::new(),
            curve_model: CurveModel::Polynomial(2),
            curve_confidence: 95.0,
            correlation_columns: String::new(),
            show_diagnostic_dialog: false,
            show_power_dialog: false,
            power_procedure: PowerProcedure::OneSampleT,
            power_effect: "0.5".to_owned(),
            power_null: "0.5".to_owned(),
            power_n: "0".to_owned(),
            power_alpha: 5.0,
            power_target: 80.0,
            power_alternative: Alternative::TwoSided,
            power_groups: 2,
            diagnostic_kind: DiagnosticKind::ChiSquareGoodnessOfFit,
            diagnostic_column: String::new(),
            diagnostic_expected: String::new(),
            diagnostic_parameters: 0,
            diagnostic_alpha: 5.0,
            rank_kind: RankTestKind::MannWhitney,
            rank_first: String::new(),
            rank_second: String::new(),
            rank_factor: String::new(),
            rank_alternative: Alternative::TwoSided,
            rank_extra_columns: String::new(),
            equivalence_kind: EquivalenceKind::OneSample,
            equivalence_first: String::new(),
            equivalence_second: String::new(),
            equivalence_null: "0".to_owned(),
            equivalence_margin: "0.1".to_owned(),
            equivalence_confidence: 95.0,
            t_kind: TTestKind::OneSample,
            t_first: String::new(),
            t_second: String::new(),
            t_null: "0".to_owned(),
            t_alternative: Alternative::TwoSided,
            t_pooled: false,
            t_confidence: 95.0,
            show_test2_dialog: false,
            test2_kind: Test2Kind::OneProportion,
            test2_first: String::new(),
            test2_second: String::new(),
            test2_null: "0.5".to_owned(),
            test2_success: "1".to_owned(),
            test2_method: ProportionMethod::Exact,
            test2_variance_method: VarianceMethod::F,
            test2_alternative: Alternative::TwoSided,
            test2_confidence: 95.0,
            show_reg_dialog: false,
            reg_response: String::new(),
            reg_predictors: Vec::new(),
            reg_confidence: 95.0,
            show_step_dialog: false,
            step_response: String::new(),
            step_candidates: Vec::new(),
            step_method: StepMethod::Stepwise,
            step_criterion: StepCriterion::PValue,
            step_alpha_enter: 0.15,
            step_alpha_remove: 0.15,
            step_max_steps: 50,
            step_confidence: 95.0,
            show_anova_dialog: false,
            anova_response: String::new(),
            one_way_kind: OneWayKind::Anova,
            variance_center: VarianceCenter::Mean,
            anova_factor: String::new(),
            anova_confidence: 95.0,
            show_km_dialog: false,
            km_time: String::new(),
            km_censor: None,
            km_censor_value: "0".to_owned(),
            km_confidence: 95.0,
            show_weibull_dialog: false,
            weibull_time: String::new(),
            weibull_censor: None,
            weibull_censor_value: "0".to_owned(),
            show_exp_dialog: false,
            exp_time: String::new(),
            exp_censor: None,
            exp_censor_value: "0".to_owned(),
            show_log_dialog: false,
            log_time: String::new(),
            log_censor: None,
            log_censor_value: "0".to_owned(),
            show_gage_dialog: false,
            gage_measurement: String::new(),
            gage_part: String::new(),
            gage_operator: String::new(),
            gage_design: GageDesign::Crossed,
            show_type1_dialog: false,
            type1_measurement: String::new(),
            type1_reference: "0".to_owned(),
            type1_lsl: String::new(),
            type1_usl: String::new(),
            show_doe_dialog: false,
            doe_response: String::new(),
            doe_factors: Vec::new(),
            doe_block: String::new(),
            doe_main_effects: false,
            show_rsm_dialog: false,
            rsm_response: String::new(),
            rsm_factors: Vec::new(),
            rsm_optimize: false,
            rsm_contours: false,
            rsm_steepest_ascent: false,
            show_desirability_dialog: false,
            desirability_factors: Vec::new(),
            desirability_responses: Vec::new(),
            show_scatter_dialog: false,
            scatter_x: String::new(),
            scatter_y: String::new(),
            scatter_fit: true,
            show_design_dialog: false,
            show_bb_dialog: false,
            show_pb_dialog: false,
            design_runs: 12,
            design_factors: "A,B".to_owned(),
            design_replicates: 2,
            design_centers: 0,
            design_block_size: 0,
            design_randomize: true,
            design_seed: 2026,
            show_describe_dialog: false,
            describe_column: String::new(),
            describe_by: None,
            dialog_distribution: DistributionKind::Normal,
            dialog_display: PlotDisplay::Both,
            dialog_ci: true,
            dialog_ci_level: 95.0,
            session: String::new(),
            plot: None,
            show_plot: false,
            show_open: false,
            open_path: String::new(),
            open_kind: FileKind::Csv,
            open_request: None,
            preview_request: None,
            show_csv_options: false,
            csv_path: String::new(),
            csv_preview: None,
            csv_delimiter: b',',
            csv_decimal_comma: false,
            csv_header: true,
            show_save: false,
            save_path: String::new(),
            save_kind: FileKind::Csv,
            save_decimal_comma: true,
            save_request: None,
            browse_request: None,
            show_rename: false,
            rename_text: String::new(),
            show_sheet_rename: false,
            sheet_rename_text: String::new(),
            sheet_rename_target: 0,
            show_about: false,
            status: "New worksheet — 8 columns".to_owned(),
            error: None,
            light_visuals: false,
            zoom: 1.0,
            date_format: DateFormat::Iso,
            native_plot_windows: true,
            plot_viewport: None,
        }
    }

    /// Resets every dialog option to its default, without touching the data.
    pub fn reset_dialog_defaults(&mut self) {
        self.find_match_case = false;
        self.find_whole_cell = false;
        self.format_decimals = 6;
        self.format_thousands = false;
        self.format_bold = false;
        self.format_align = CellAlign::Left;
        self.dialog_columns = self.selected.clone().into_iter().collect();
        self.dialog_group = None;
        self.dialog_layout = PlotLayout::Separate;
        self.dialog_same_x = false;
        self.dialog_same_y = false;
        self.cap_column = self.selected.clone().unwrap_or_default();
        self.cap_distribution = DistributionKind::Normal;
        self.cap_lsl.clear();
        self.cap_usl.clear();
        self.cap_target.clear();
        self.cap_subgroup = 1;
        self.cap_confidence = 95.0;
        self.cap_bootstrap = 0;
        self.cap_box_cox = false;
        self.cap_require_stability = false;
        self.chart_column = self.selected.clone().unwrap_or_default();
        self.chart_kind = ControlChartKind::IndividualsMovingRange;
        self.chart_subgroup = 5;
        self.chart_subgroup_column.clear();
        self.chart_tests = TestSet::default();
        self.attr_column = self.selected.clone().unwrap_or_default();
        self.attr_kind = AttributeChartKind::P;
        self.attr_sizes = None;
        self.attr_size = "100".to_owned();
        self.t_first = self.selected.clone().unwrap_or_default();
        self.t_second = String::new();
        self.t_null = "0".to_owned();
        self.t_alternative = Alternative::TwoSided;
        self.t_pooled = false;
        self.t_confidence = 95.0;
        self.test2_first = self.selected.clone().unwrap_or_default();
        self.test2_second = String::new();
        self.test2_null = "0.5".to_owned();
        self.test2_success = "1".to_owned();
        self.test2_method = ProportionMethod::Exact;
        self.test2_alternative = Alternative::TwoSided;
        self.test2_confidence = 95.0;
        self.reg_response = self.selected.clone().unwrap_or_default();
        self.reg_predictors.clear();
        self.reg_confidence = 95.0;
        self.step_response = self.selected.clone().unwrap_or_default();
        self.step_candidates.clear();
        self.step_method = StepMethod::Stepwise;
        self.step_criterion = StepCriterion::PValue;
        self.step_alpha_enter = 0.15;
        self.step_alpha_remove = 0.15;
        self.step_max_steps = 50;
        self.step_confidence = 95.0;
        self.anova_response = self.selected.clone().unwrap_or_default();
        self.anova_factor = String::new();
        self.anova_confidence = 95.0;
        self.km_time = self.selected.clone().unwrap_or_default();
        self.km_censor = None;
        self.km_censor_value = "0".to_owned();
        self.km_confidence = 95.0;
        self.weibull_time = self.selected.clone().unwrap_or_default();
        self.weibull_censor = None;
        self.weibull_censor_value = "0".to_owned();
        self.exp_time = self.selected.clone().unwrap_or_default();
        self.exp_censor = None;
        self.exp_censor_value = "0".to_owned();
        self.log_time = self.selected.clone().unwrap_or_default();
        self.log_censor = None;
        self.log_censor_value = "0".to_owned();
        self.gage_measurement = self.selected.clone().unwrap_or_default();
        self.gage_part = String::new();
        self.gage_operator = String::new();
        self.gage_design = GageDesign::Crossed;
        self.type1_measurement = self.selected.clone().unwrap_or_default();
        self.type1_reference = "0".to_owned();
        self.type1_lsl.clear();
        self.type1_usl.clear();
        self.doe_response = self.selected.clone().unwrap_or_default();
        self.doe_factors.clear();
        self.doe_block.clear();
        self.doe_main_effects = false;
        self.rsm_response = self.selected.clone().unwrap_or_default();
        self.rsm_factors.clear();
        self.rsm_optimize = false;
        self.rsm_contours = false;
        self.rsm_steepest_ascent = false;
        self.scatter_x = self.selected.clone().unwrap_or_default();
        self.scatter_y = String::new();
        self.scatter_fit = true;
        self.design_factors = "A,B".to_owned();
        self.design_replicates = 2;
        self.design_centers = 0;
        self.design_block_size = 0;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.describe_column = self.selected.clone().unwrap_or_default();
        self.describe_by = None;
        self.dialog_distribution = self.distribution;
        self.dialog_display = PlotDisplay::Both;
        self.dialog_ci = true;
    }

    /// A fresh eight-column worksheet tab.
    fn blank_sheet(name: impl Into<String>) -> SheetState {
        SheetState {
            name: name.into(),
            data: Some(DataFrame::blank(8, 0).expect("blank worksheet")),
            source: "Untitled".to_owned(),
            undo: Vec::new(),
            redo: Vec::new(),
            selected_cell: Some((0, 0)),
            column_widths: Vec::new(),
        }
    }

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

    /// Selects a whole column (every existing row).
    pub fn select_column(&mut self, column: usize) {
        let Some((columns, rows)) = self.data.as_ref().map(|df| (df.n_columns(), df.n_rows()))
        else {
            return;
        };
        if columns == 0 {
            return;
        }
        let column = column.min(columns - 1);
        self.select_cell(column, 0);
        self.extend_selection(column, rows.saturating_sub(1));
    }

    /// Selects a whole row.
    pub fn select_row(&mut self, row: usize) {
        let Some(columns) = self.data.as_ref().map(DataFrame::n_columns) else {
            return;
        };
        if columns == 0 {
            return;
        }
        self.select_cell(0, row);
        self.extend_selection(columns - 1, row);
    }

    /// Selects the whole worksheet.
    pub fn select_all_cells(&mut self) {
        let Some((columns, rows)) = self.data.as_ref().map(|df| (df.n_columns(), df.n_rows()))
        else {
            return;
        };
        if columns == 0 {
            return;
        }
        self.select_cell(0, 0);
        self.extend_selection(columns - 1, rows.saturating_sub(1));
    }

    /// Selects one worksheet cell.
    pub fn select_cell(&mut self, column: usize, row: usize) {
        self.inline_edit = None;
        self.inline_request_focus = false;
        let Some(df) = self.data.as_ref() else {
            self.selected_cell = None;
            self.selection_anchor = None;
            self.edit_target = None;
            self.edit_buffer.clear();
            self.column_summary.clear();
            return;
        };
        if df.n_columns() == 0 {
            self.selected_cell = None;
            self.selection_anchor = None;
            self.edit_target = None;
            self.edit_buffer.clear();
            self.column_summary.clear();
            return;
        }
        let column = column.min(df.n_columns() - 1);
        self.selected_cell = Some((column, row));
        self.selection_anchor = Some((column, row));
        self.edit_target = Some((column, row));
        self.selected = Some(df.columns()[column].name().to_owned());
        self.edit_buffer = if row < df.n_rows() {
            df.cell_text(column, row).unwrap_or_default()
        } else {
            String::new()
        };
        self.edit_dirty = false;
        self.refresh_column_summary();
    }

    /// Extends the current selection to another cell, keeping the anchor.
    pub fn extend_selection(&mut self, column: usize, row: usize) {
        self.inline_edit = None;
        self.inline_request_focus = false;
        let Some(df) = self.data.as_ref() else {
            return;
        };
        if df.n_columns() == 0 {
            return;
        }
        let column = column.min(df.n_columns() - 1);
        if self.selection_anchor.is_none() {
            self.selection_anchor = self.selected_cell;
        }
        self.selected_cell = Some((column, row));
        self.edit_target = Some((column, row));
        self.selected = Some(df.columns()[column].name().to_owned());
        self.edit_buffer = if row < df.n_rows() {
            df.cell_text(column, row).unwrap_or_default()
        } else {
            String::new()
        };
        self.edit_dirty = false;
        self.refresh_column_summary();
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

    /// Number of columns and rows in the selection.
    #[must_use]
    pub fn selection_size(&self) -> (usize, usize) {
        self.selection_bounds()
            .map_or((0, 0), |(c0, r0, c1, r1)| (c1 - c0 + 1, r1 - r0 + 1))
    }

    /// Selection as tab-separated text; missing cells are empty.
    #[must_use]
    pub fn selection_tsv(&self) -> Option<String> {
        let (c0, r0, c1, r1) = self.selection_bounds()?;
        let df = self.data.as_ref()?;
        let last_column = c1.min(df.n_columns().saturating_sub(1));
        let last_row = r1.min(df.n_rows().saturating_sub(1));
        if last_column < c0 || last_row < r0 {
            return None;
        }
        let mut lines = Vec::with_capacity(last_row - r0 + 1);
        for row in r0..=last_row {
            let mut cells = Vec::with_capacity(last_column - c0 + 1);
            for column in c0..=last_column {
                cells.push(cell_state(df, column, row).unwrap_or_default());
            }
            lines.push(cells.join("\t"));
        }
        Some(lines.join("\n"))
    }

    /// Clears every existing cell in the selection as one undo group.
    pub fn clear_selection(&mut self) {
        let Some((c0, r0, c1, r1)) = self.selection_bounds() else {
            return;
        };
        let Some(df) = self.data.as_mut() else {
            return;
        };
        let before_rows = df.n_rows();
        let last_row = r1.min(df.n_rows().saturating_sub(1));
        let last_column = c1.min(df.n_columns().saturating_sub(1));
        if last_row < r0 || last_column < c0 {
            return;
        }
        let mut edits = Vec::new();
        for row in r0..=last_row {
            for column in c0..=last_column {
                let before = cell_state(df, column, row);
                if before.is_none() {
                    continue;
                }
                if let Err(e) = df.set_missing(column, row) {
                    self.set_error(e.to_string());
                    return;
                }
                edits.push(CellEdit {
                    column,
                    row,
                    before,
                    after: None,
                    before_rows,
                });
            }
        }
        if edits.is_empty() {
            return;
        }
        let count = edits.len();
        self.push_undo(EditGroup {
            actions: vec![EditAction::Cells(edits)],
        });
        self.edit_buffer.clear();
        self.edit_dirty = false;
        self.error = None;
        self.status = format!("Cleared {count} cell(s)");
        self.refresh_column_summary();
    }

    /// Current worksheet cell as `(column index, row index)`.
    #[must_use]
    pub fn selected_cell(&self) -> Option<(usize, usize)> {
        self.selected_cell
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

    /// Applies the formula bar to the selected cell.
    pub fn commit_cell(&mut self) -> bool {
        let Some((column, row)) = self.edit_target else {
            self.set_error("Select a cell first");
            return false;
        };
        // A leading "=" evaluates a spreadsheet formula and stores its result.
        if self.edit_buffer.trim_start().starts_with('=') {
            let Some(df) = self.data.as_ref() else {
                self.set_error("No worksheet open");
                return false;
            };
            match fp_core::formula::evaluate(df, &self.edit_buffer) {
                Ok(value) => self.edit_buffer = value.text(),
                Err(error) => {
                    self.set_error(error.to_string());
                    return false;
                }
            }
        }
        let Some(df) = self.data.as_mut() else {
            self.set_error("No worksheet open");
            return false;
        };
        let before_rows = df.n_rows();
        let before = cell_state(df, column, row);
        if row >= df.n_rows()
            && let Err(e) = df.ensure_rows(row + 1)
        {
            self.set_error(e.to_string());
            return false;
        }
        if let Err(e) = df.set_cell(column, row, &self.edit_buffer) {
            self.set_error(e.to_string());
            return false;
        }
        let after = cell_state(df, column, row);
        self.edit_dirty = false;
        self.error = None;
        self.status = format!("Edited C{} row {}", column + 1, row + 1);
        self.edit_buffer = df.cell_text(column, row).unwrap_or_default();
        if before != after || before_rows != df.n_rows() {
            self.push_undo(EditGroup {
                actions: vec![EditAction::Cells(vec![CellEdit {
                    column,
                    row,
                    before,
                    after,
                    before_rows,
                }])],
            });
        }
        self.refresh_column_summary();
        true
    }

    fn begin_inline_edit(&mut self, column: usize, row: usize) {
        self.select_cell(column, row);
        let missing = self
            .data
            .as_ref()
            .is_none_or(|df| df.cell_is_missing(column, row));
        self.inline_buffer = if missing {
            String::new()
        } else {
            self.edit_buffer.clone()
        };
        self.inline_edit = Some((column, row));
        self.inline_request_focus = true;
        self.edit_dirty = false;
    }

    /// Starts an inline edit with the first typed character already in place.
    fn begin_inline_edit_with(&mut self, column: usize, row: usize, text: &str) {
        self.select_cell(column, row);
        self.inline_buffer = text.to_owned();
        self.inline_edit = Some((column, row));
        self.inline_request_focus = true;
        self.edit_dirty = false;
    }

    fn commit_inline_edit(&mut self) -> bool {
        let Some((column, row)) = self.inline_edit else {
            return false;
        };
        self.edit_target = Some((column, row));
        self.edit_buffer = self.inline_buffer.clone();
        if self.commit_cell() {
            self.inline_edit = None;
            self.inline_request_focus = false;
            true
        } else {
            self.inline_request_focus = true;
            false
        }
    }

    fn cancel_inline_edit(&mut self) {
        self.inline_edit = None;
        self.inline_request_focus = false;
        self.edit_dirty = false;
        if let Some((column, row)) = self.selected_cell {
            self.select_cell(column, row);
        }
    }

    /// Pushes one undoable edit group and invalidates the redo stack.
    fn push_undo(&mut self, group: EditGroup) {
        if group.actions.is_empty() {
            return;
        }
        if self.undo_stack.len() == UNDO_LIMIT {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(group);
        self.redo_stack.clear();
    }

    /// Undoes the last cell/block or structural edit.
    pub fn undo(&mut self) {
        let Some(group) = self.undo_stack.pop() else {
            self.status = "Nothing to undo".to_owned();
            return;
        };
        let Some(df) = self.data.as_mut() else {
            return;
        };
        for action in group.actions.iter().rev() {
            if let Err(e) = undo_action(df, action) {
                self.set_error(e);
                return;
            }
        }
        self.redo_stack.push(group.clone());
        self.error = None;
        self.status = format!("Undo — {} change(s)", group.actions.len());
        self.restore_after_history();
    }

    /// Re-applies the last undone edit.
    pub fn redo(&mut self) {
        let Some(group) = self.redo_stack.pop() else {
            self.status = "Nothing to redo".to_owned();
            return;
        };
        let Some(df) = self.data.as_mut() else {
            return;
        };
        for action in &group.actions {
            if let Err(e) = redo_action(df, action) {
                self.set_error(e);
                return;
            }
        }
        self.undo_stack.push(group.clone());
        self.error = None;
        self.status = format!("Redo — {} change(s)", group.actions.len());
        self.restore_after_history();
    }

    fn restore_after_history(&mut self) {
        let target = self
            .undo_stack
            .last()
            .or_else(|| self.redo_stack.last())
            .and_then(group_target);
        if let Some((column, row)) = target {
            self.select_cell(column, row);
        } else {
            self.refresh_column_summary();
        }
    }

    /// `true` when a worksheet cell edit can be undone.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// `true` when an undone edit can be re-applied.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Empties the selected cell or rectangle.
    pub fn clear_selected_cell(&mut self) {
        self.clear_selection();
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

    fn refresh_column_summary(&mut self) {
        let Some((column, _)) = self.selected_cell else {
            self.column_summary.clear();
            return;
        };
        let Some(df) = self.data.as_ref() else {
            self.column_summary.clear();
            return;
        };
        let Some(column_ref) = df.columns().get(column) else {
            self.column_summary.clear();
            return;
        };
        let name = column_ref.name().to_owned();
        match column_ref.dtype() {
            fp_core::data::DType::Float | fp_core::data::DType::Integer => {
                match column_ref
                    .sample(fp_core::data::MissingPolicy::Drop)
                    .and_then(|s| fp_core::data::Descriptives::from_sample(&s))
                {
                    Ok(d) => {
                        self.column_summary = format!(
                            "{name}: n={}  mean={}  stdev={}  min={}  max={}",
                            d.n,
                            fmt_f64(d.mean),
                            fmt_f64(d.sd),
                            fmt_f64(d.min),
                            fmt_f64(d.max)
                        );
                    }
                    Err(_) => {
                        self.column_summary = format!("{name}: no numeric observations");
                    }
                }
            }
            dtype => {
                self.column_summary =
                    format!("{name}: {} column, {} rows", dtype.as_str(), df.n_rows());
            }
        }
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

    /// Opens the Session HTML export dialog.
    pub fn begin_session_export(&mut self) {
        self.save_kind = FileKind::Html;
        self.save_path = "session.html".to_owned();
        self.show_save = true;
    }

    /// Records that the Session was exported to `path`.
    pub fn mark_session_exported(&mut self, path: &str) {
        self.status = format!("Session exported to {path}");
        self.error = None;
    }

    /// Records that the worksheet was written to `path`.
    pub fn mark_saved(&mut self, path: &str) {
        self.source = path.to_owned();
        self.status = format!("Saved {path}");
        self.error = None;
    }

    /// Session text, for tests and the desktop runner.
    #[must_use]
    pub fn session_text(&self) -> &str {
        &self.session
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

    /// Selected column name.
    #[must_use]
    pub fn selected_column(&self) -> Option<&str> {
        self.selected.as_deref()
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

    /// Opens the classic statistical-desktop probability-plot options dialog.
    pub fn open_probability_plot_dialog(&mut self) {
        if self.data.is_none() {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        }
        if let Some(column) = &self.selected {
            self.dialog_columns = vec![column.clone()];
        }
        self.dialog_group = None;
        self.dialog_layout = PlotLayout::Separate;
        self.dialog_same_x = false;
        self.dialog_same_y = false;
        self.cap_column = self.selected.clone().unwrap_or_default();
        self.cap_distribution = DistributionKind::Normal;
        self.cap_lsl.clear();
        self.cap_usl.clear();
        self.cap_target.clear();
        self.cap_subgroup = 1;
        self.cap_confidence = 95.0;
        self.cap_bootstrap = 0;
        self.cap_box_cox = false;
        self.chart_column = self.selected.clone().unwrap_or_default();
        self.chart_kind = ControlChartKind::IndividualsMovingRange;
        self.chart_subgroup = 5;
        self.chart_subgroup_column.clear();
        self.chart_tests = TestSet::default();
        self.attr_column = self.selected.clone().unwrap_or_default();
        self.attr_kind = AttributeChartKind::P;
        self.attr_sizes = None;
        self.attr_size = "100".to_owned();
        self.t_first = self.selected.clone().unwrap_or_default();
        self.t_second = String::new();
        self.t_null = "0".to_owned();
        self.t_alternative = Alternative::TwoSided;
        self.t_pooled = false;
        self.t_confidence = 95.0;
        self.test2_first = self.selected.clone().unwrap_or_default();
        self.test2_second = String::new();
        self.test2_null = "0.5".to_owned();
        self.test2_success = "1".to_owned();
        self.test2_method = ProportionMethod::Exact;
        self.test2_alternative = Alternative::TwoSided;
        self.test2_confidence = 95.0;
        self.reg_response = self.selected.clone().unwrap_or_default();
        self.reg_predictors.clear();
        self.reg_confidence = 95.0;
        self.step_response = self.selected.clone().unwrap_or_default();
        self.step_candidates.clear();
        self.step_method = StepMethod::Stepwise;
        self.step_criterion = StepCriterion::PValue;
        self.step_alpha_enter = 0.15;
        self.step_alpha_remove = 0.15;
        self.step_max_steps = 50;
        self.step_confidence = 95.0;
        self.anova_response = self.selected.clone().unwrap_or_default();
        self.anova_factor = String::new();
        self.anova_confidence = 95.0;
        self.km_time = self.selected.clone().unwrap_or_default();
        self.km_censor = None;
        self.km_censor_value = "0".to_owned();
        self.km_confidence = 95.0;
        self.weibull_time = self.selected.clone().unwrap_or_default();
        self.weibull_censor = None;
        self.weibull_censor_value = "0".to_owned();
        self.exp_time = self.selected.clone().unwrap_or_default();
        self.exp_censor = None;
        self.exp_censor_value = "0".to_owned();
        self.log_time = self.selected.clone().unwrap_or_default();
        self.log_censor = None;
        self.log_censor_value = "0".to_owned();
        self.gage_measurement = self.selected.clone().unwrap_or_default();
        self.gage_part = String::new();
        self.gage_operator = String::new();
        self.gage_design = GageDesign::Crossed;
        self.type1_measurement = self.selected.clone().unwrap_or_default();
        self.type1_reference = "0".to_owned();
        self.type1_lsl.clear();
        self.type1_usl.clear();
        self.doe_response = self.selected.clone().unwrap_or_default();
        self.doe_factors.clear();
        self.doe_block.clear();
        self.doe_main_effects = false;
        self.rsm_response = self.selected.clone().unwrap_or_default();
        self.rsm_factors.clear();
        self.rsm_optimize = false;
        self.rsm_contours = false;
        self.rsm_steepest_ascent = false;
        self.scatter_x = self.selected.clone().unwrap_or_default();
        self.scatter_y = String::new();
        self.scatter_fit = true;
        self.design_factors = "A,B".to_owned();
        self.design_replicates = 2;
        self.design_centers = 0;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.describe_column = self.selected.clone().unwrap_or_default();
        self.describe_by = None;
        self.dialog_distribution = self.distribution;
        self.show_probability_plot_dialog = true;
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

    /// Opens the attribute-chart dialog.
    pub fn open_attribute_dialog(&mut self) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if !df
            .columns()
            .iter()
            .any(|column| matches!(column.dtype(), DType::Float | DType::Integer))
        {
            self.set_error("No numeric column available");
            return;
        }
        self.attr_column = self.selected.clone().unwrap_or_default();
        self.attr_kind = AttributeChartKind::P;
        self.attr_sizes = None;
        self.attr_size = "100".to_owned();
        self.show_attr_dialog = true;
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

    fn attribute_chart_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_attr_dialog {
            return;
        }
        let columns: Vec<String> = self
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
        let mut open = self.show_attr_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Attribute Control Chart"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Chart:");
                    egui::ComboBox::from_id_salt("attr_kind")
                        .selected_text(self.attr_kind.name())
                        .show_ui(ui, |ui| {
                            for kind in [
                                AttributeChartKind::P,
                                AttributeChartKind::Np,
                                AttributeChartKind::C,
                                AttributeChartKind::U,
                                AttributeChartKind::LaneyP,
                                AttributeChartKind::LaneyNp,
                                AttributeChartKind::LaneyC,
                                AttributeChartKind::LaneyU,
                            ] {
                                ui.selectable_value(&mut self.attr_kind, kind, kind.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Defects column:");
                    egui::ComboBox::from_id_salt("attr_column")
                        .selected_text(if self.attr_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.attr_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &columns {
                                ui.selectable_value(
                                    &mut self.attr_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.attr_kind.needs_size() {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Size column:");
                        egui::ComboBox::from_id_salt("attr_sizes")
                            .selected_text(
                                self.attr_sizes.as_deref().unwrap_or("None (use constant)"),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.attr_sizes,
                                    None,
                                    "None (use constant)",
                                );
                                for name in &columns {
                                    ui.selectable_value(
                                        &mut self.attr_sizes,
                                        Some(name.clone()),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Constant size:");
                        ui.add_enabled(
                            self.attr_sizes.is_none(),
                            egui::TextEdit::singleline(&mut self.attr_size).desired_width(90.0),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                }
                ui.small(match self.attr_kind {
                    AttributeChartKind::P => {
                        "Proportion defective; variable subgroup sizes allowed."
                    }
                    AttributeChartKind::Np => {
                        "Number defective; the subgroup size must be constant."
                    }
                    AttributeChartKind::C => {
                        "Defects per sample; constant opportunity (no denominator)."
                    }
                    AttributeChartKind::U => "Defects per unit; variable subgroup sizes allowed.",
                    AttributeChartKind::LaneyP => {
                        "Proportion defective with Laney limits adjusted for overdispersion."
                    }
                    AttributeChartKind::LaneyNp => {
                        "Number defective with Laney limits; the subgroup size must be constant."
                    }
                    AttributeChartKind::LaneyC => {
                        "Defects per sample with Laney limits; constant opportunity."
                    }
                    AttributeChartKind::LaneyU => {
                        "Defects per unit with Laney limits; variable subgroup sizes allowed."
                    }
                });
                ui.small("Tests 1 to 4 are applied to the plotted values.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_attr_dialog = open && !run;
        if run {
            self.run_attribute_chart();
        }
    }

    /// Opens the dialog for one of the three Student t tests.
    pub fn open_t_test_dialog(&mut self, kind: TTestKind) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let names: Vec<String> = df
            .columns()
            .iter()
            .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
            .map(|column| column.name().to_owned())
            .collect();
        if names.is_empty() {
            self.set_error("No numeric column available");
            return;
        }
        self.t_kind = kind;
        self.t_first = self
            .selected
            .clone()
            .filter(|name| names.contains(name))
            .unwrap_or_else(|| names[0].clone());
        self.t_second = names
            .iter()
            .find(|name| **name != self.t_first)
            .cloned()
            .unwrap_or_default();
        self.t_null = "0".to_owned();
        self.t_alternative = Alternative::TwoSided;
        self.t_pooled = false;
        self.t_confidence = 95.0;
        self.test2_first = self.selected.clone().unwrap_or_default();
        self.test2_second = String::new();
        self.test2_null = "0.5".to_owned();
        self.test2_success = "1".to_owned();
        self.test2_method = ProportionMethod::Exact;
        self.test2_alternative = Alternative::TwoSided;
        self.test2_confidence = 95.0;
        self.reg_response = self.selected.clone().unwrap_or_default();
        self.reg_predictors.clear();
        self.reg_confidence = 95.0;
        self.step_response = self.selected.clone().unwrap_or_default();
        self.step_candidates.clear();
        self.step_method = StepMethod::Stepwise;
        self.step_criterion = StepCriterion::PValue;
        self.step_alpha_enter = 0.15;
        self.step_alpha_remove = 0.15;
        self.step_max_steps = 50;
        self.step_confidence = 95.0;
        self.anova_response = self.selected.clone().unwrap_or_default();
        self.anova_factor = String::new();
        self.anova_confidence = 95.0;
        self.km_time = self.selected.clone().unwrap_or_default();
        self.km_censor = None;
        self.km_censor_value = "0".to_owned();
        self.km_confidence = 95.0;
        self.weibull_time = self.selected.clone().unwrap_or_default();
        self.weibull_censor = None;
        self.weibull_censor_value = "0".to_owned();
        self.exp_time = self.selected.clone().unwrap_or_default();
        self.exp_censor = None;
        self.exp_censor_value = "0".to_owned();
        self.log_time = self.selected.clone().unwrap_or_default();
        self.log_censor = None;
        self.log_censor_value = "0".to_owned();
        self.gage_measurement = self.selected.clone().unwrap_or_default();
        self.gage_part = String::new();
        self.gage_operator = String::new();
        self.gage_design = GageDesign::Crossed;
        self.type1_measurement = self.selected.clone().unwrap_or_default();
        self.type1_reference = "0".to_owned();
        self.type1_lsl.clear();
        self.type1_usl.clear();
        self.doe_response = self.selected.clone().unwrap_or_default();
        self.doe_factors.clear();
        self.doe_block.clear();
        self.doe_main_effects = false;
        self.rsm_response = self.selected.clone().unwrap_or_default();
        self.rsm_factors.clear();
        self.rsm_optimize = false;
        self.rsm_contours = false;
        self.rsm_steepest_ascent = false;
        self.scatter_x = self.selected.clone().unwrap_or_default();
        self.scatter_y = String::new();
        self.scatter_fit = true;
        self.design_factors = "A,B".to_owned();
        self.design_replicates = 2;
        self.design_centers = 0;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_t_dialog = true;
    }

    /// Builds and runs the t test selected in the dialog.
    pub fn run_t_test_dialog(&mut self) {
        if self.t_first.is_empty() {
            self.set_error("Select the first numeric column");
            return;
        }
        let spec = match self.t_kind {
            TTestKind::OneSample => {
                let Ok(null_mean) = self.t_null.trim().parse::<f64>() else {
                    self.set_error("Null mean must be a number");
                    return;
                };
                AnalysisSpec::one_sample_t(self.t_first.clone(), null_mean)
            }
            TTestKind::TwoSample | TTestKind::Paired => {
                if self.t_second.is_empty() {
                    self.set_error("Select the second numeric column");
                    return;
                }
                if self.t_second == self.t_first {
                    self.set_error("Select two different columns");
                    return;
                }
                if self.t_kind == TTestKind::TwoSample {
                    AnalysisSpec::two_sample_t(self.t_first.clone(), self.t_second.clone())
                        .with_pooled(self.t_pooled)
                } else {
                    AnalysisSpec::paired_t(self.t_first.clone(), self.t_second.clone())
                }
            }
        }
        .with_alternative(self.t_alternative)
        .with_t_confidence((self.t_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!(
            "{}: {} / {}",
            self.t_kind.name(),
            self.t_first,
            self.t_second
        );
        self.execute_spec(spec, label);
        self.show_t_dialog = false;
    }

    fn t_test_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_t_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_t_dialog;
        let mut run = false;
        dialog_window(self.t_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("First column:");
                    egui::ComboBox::from_id_salt("t_first_column")
                        .selected_text(if self.t_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.t_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(&mut self.t_first, name.clone(), name.as_str());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.t_kind != TTestKind::OneSample {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Second column:");
                        egui::ComboBox::from_id_salt("t_second_column")
                            .selected_text(if self.t_second.is_empty() {
                                "—".to_owned()
                            } else {
                                self.t_second.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(
                                        &mut self.t_second,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if self.t_kind == TTestKind::OneSample {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Null mean:");
                        ui.text_edit_singleline(&mut self.t_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Alternative:");
                    egui::ComboBox::from_id_salt("t_alternative")
                        .selected_text(self.t_alternative.as_str())
                        .show_ui(ui, |ui| {
                            for alternative in [
                                Alternative::TwoSided,
                                Alternative::Less,
                                Alternative::Greater,
                            ] {
                                ui.selectable_value(
                                    &mut self.t_alternative,
                                    alternative,
                                    alternative.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.t_kind == TTestKind::TwoSample {
                    ui.help_checkbox(&mut self.t_pooled, "Assume equal variances (pooled)");
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.t_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(match self.t_kind {
                    TTestKind::OneSample => {
                        "H₀: µ = null mean; sample standard deviation uses n − 1."
                    }
                    TTestKind::TwoSample => {
                        "H₀: µ₁ − µ₂ = 0; Welch is the default, pooled is optional."
                    }
                    TTestKind::Paired => "H₀: mean difference = 0; only complete pairs are used.",
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_t_dialog = open && !run;
        if run {
            self.run_t_test_dialog();
        }
    }

    /// Opens the dialog for an equivalence test.
    pub fn open_equivalence_dialog(&mut self, kind: EquivalenceKind) {
        self.equivalence_kind = kind;
        self.equivalence_first = self.selected.clone().unwrap_or_default();
        self.equivalence_second = String::new();
        self.equivalence_null = "0".to_owned();
        self.equivalence_margin = "0.1".to_owned();
        self.equivalence_confidence = 95.0;
        self.show_equivalence_dialog = true;
    }

    /// Builds and runs the equivalence test selected in the dialog.
    pub fn run_equivalence_dialog(&mut self) {
        if self.equivalence_first.is_empty() {
            self.set_error("Select the first numeric column");
            return;
        }
        let Ok(margin) = self.equivalence_margin.trim().parse::<f64>() else {
            self.set_error("The equivalence margin must be a number");
            return;
        };
        if margin <= 0.0 {
            self.set_error("The equivalence margin must be positive");
            return;
        }
        let spec = match self.equivalence_kind {
            EquivalenceKind::OneSample => {
                let Ok(null_mean) = self.equivalence_null.trim().parse::<f64>() else {
                    self.set_error("The captured run mean must be a number");
                    return;
                };
                AnalysisSpec::one_sample_equivalence(
                    self.equivalence_first.clone(),
                    null_mean,
                    margin,
                )
            }
            EquivalenceKind::TwoSample | EquivalenceKind::Paired => {
                if self.equivalence_second.is_empty() {
                    self.set_error("Select the second numeric column");
                    return;
                }
                if self.equivalence_second == self.equivalence_first {
                    self.set_error("Select two different columns");
                    return;
                }
                if self.equivalence_kind == EquivalenceKind::TwoSample {
                    AnalysisSpec::two_sample_equivalence(
                        self.equivalence_first.clone(),
                        self.equivalence_second.clone(),
                        margin,
                    )
                } else {
                    AnalysisSpec::paired_equivalence(
                        self.equivalence_first.clone(),
                        self.equivalence_second.clone(),
                        margin,
                    )
                }
            }
        }
        .with_equivalence_confidence((self.equivalence_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!(
            "{}: {} / {}",
            self.equivalence_kind.name(),
            self.equivalence_first,
            self.equivalence_second
        );
        self.execute_spec(spec, label);
        self.show_equivalence_dialog = false;
    }

    fn equivalence_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_equivalence_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_equivalence_dialog;
        let mut run = false;
        dialog_window(self.equivalence_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("First column:");
                    egui::ComboBox::from_id_salt("equivalence_first_column")
                        .selected_text(if self.equivalence_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.equivalence_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.equivalence_first,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.equivalence_kind != EquivalenceKind::OneSample {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Second column:");
                        egui::ComboBox::from_id_salt("equivalence_second_column")
                            .selected_text(if self.equivalence_second.is_empty() {
                                "—".to_owned()
                            } else {
                                self.equivalence_second.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(
                                        &mut self.equivalence_second,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if self.equivalence_kind == EquivalenceKind::OneSample {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Reference mean:");
                        ui.text_edit_singleline(&mut self.equivalence_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Margin:");
                    ui.text_edit_singleline(&mut self.equivalence_margin)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.equivalence_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "H₀: |effect| ≥ margin; the test declares equivalence when the (1 − 2α) \
                     interval is inside the margin.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_equivalence_dialog = open && !run;
        if run {
            self.run_equivalence_dialog();
        }
    }

    /// Opens the dialog for a non-parametric rank test.
    pub fn open_rank_test_dialog(&mut self, kind: RankTestKind) {
        self.rank_kind = kind;
        self.rank_first = self.selected.clone().unwrap_or_default();
        self.rank_second = String::new();
        self.rank_factor = String::new();
        self.rank_alternative = Alternative::TwoSided;
        self.rank_extra_columns = String::new();
        self.show_rank_dialog = true;
    }

    /// Builds and runs the rank test selected in the dialog.
    pub fn run_rank_test_dialog(&mut self) {
        if self.rank_kind.uses_factor() {
            if self.rank_first.is_empty() {
                self.set_error("Select the response column");
                return;
            }
            if self.rank_factor.is_empty() {
                self.set_error("Select the factor column");
                return;
            }
            let spec = if self.rank_kind == RankTestKind::MoodsMedian {
                AnalysisSpec::moods_median(self.rank_first.clone(), self.rank_factor.clone())
            } else {
                AnalysisSpec::kruskal_wallis(self.rank_first.clone(), self.rank_factor.clone())
            };
            let label = format!("{}: {}", self.rank_kind.name(), self.rank_first);
            self.execute_spec(spec, label);
            self.show_rank_dialog = false;
            return;
        }
        if self.rank_kind == RankTestKind::SignTest {
            if self.rank_first.is_empty() {
                self.set_error("Select the column to test");
                return;
            }
            // The captured run tests the median against 0 unless the user
            // types another value.
            let median = if self.rank_median.trim().is_empty() {
                0.0
            } else {
                match self.rank_median.trim().parse::<f64>() {
                    Ok(value) => value,
                    Err(_) => {
                        self.set_error("The hypothesised median must be a number");
                        return;
                    }
                }
            };
            let spec = AnalysisSpec::sign_test(self.rank_first.clone(), median);
            let label = format!("{}: {}", self.rank_kind.name(), self.rank_first);
            self.execute_spec(spec, label);
            self.show_rank_dialog = false;
            return;
        }
        if self.rank_kind == RankTestKind::Friedman {
            if self.rank_first.is_empty() {
                self.set_error("Select the first column");
                return;
            }
            let mut columns = vec![self.rank_first.clone()];
            for name in self.rank_extra_columns.split(',') {
                let name = name.trim();
                if name.is_empty() {
                    continue;
                }
                columns.push(name.to_owned());
            }
            if columns.len() < 2 {
                self.set_error("Add at least one more column");
                return;
            }
            let spec = AnalysisSpec::friedman(columns);
            let label = format!("{}: {}", self.rank_kind.name(), self.rank_first);
            self.execute_spec(spec, label);
            self.show_rank_dialog = false;
            return;
        }
        if self.rank_first.is_empty() {
            self.set_error("Select the first numeric column");
            return;
        }
        if self.rank_second.is_empty() {
            self.set_error("Select the second numeric column");
            return;
        }
        if self.rank_second == self.rank_first {
            self.set_error("Select two different columns");
            return;
        }
        let spec = if self.rank_kind == RankTestKind::MannWhitney {
            AnalysisSpec::mann_whitney(self.rank_first.clone(), self.rank_second.clone())
        } else {
            AnalysisSpec::wilcoxon_signed_rank(self.rank_first.clone(), self.rank_second.clone())
        }
        .with_alternative(self.rank_alternative);
        let label = format!(
            "{}: {} / {}",
            self.rank_kind.name(),
            self.rank_first,
            self.rank_second
        );
        self.execute_spec(spec, label);
        self.show_rank_dialog = false;
    }

    fn rank_test_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_rank_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_rank_dialog;
        let mut run = false;
        dialog_window(self.rank_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.label(if self.rank_kind == RankTestKind::KruskalWallis {
                        "Response:"
                    } else {
                        "First column:"
                    });
                    egui::ComboBox::from_id_salt("rank_first_column")
                        .selected_text(if self.rank_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.rank_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.rank_first,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.rank_kind == RankTestKind::Friedman {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("More columns:");
                        ui.text_edit_singleline(&mut self.rank_extra_columns)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if self.rank_kind.uses_factor() {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Factor:");
                        egui::ComboBox::from_id_salt("rank_factor_column")
                            .selected_text(if self.rank_factor.is_empty() {
                                "—".to_owned()
                            } else {
                                self.rank_factor.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &self
                                    .data
                                    .as_ref()
                                    .map(|df| {
                                        df.columns()
                                            .iter()
                                            .map(|column| column.name().to_owned())
                                            .collect::<Vec<String>>()
                                    })
                                    .unwrap_or_default()
                                {
                                    ui.selectable_value(
                                        &mut self.rank_factor,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                } else if self.rank_kind != RankTestKind::Friedman {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Second column:");
                        egui::ComboBox::from_id_salt("rank_second_column")
                            .selected_text(if self.rank_second.is_empty() {
                                "—".to_owned()
                            } else {
                                self.rank_second.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(
                                        &mut self.rank_second,
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
                        let label = ui.help_label("Alternative:");
                        egui::ComboBox::from_id_salt("rank_alternative")
                            .selected_text(self.rank_alternative.as_str())
                            .show_ui(ui, |ui| {
                                for alternative in [
                                    Alternative::TwoSided,
                                    Alternative::Less,
                                    Alternative::Greater,
                                ] {
                                    ui.selectable_value(
                                        &mut self.rank_alternative,
                                        alternative,
                                        alternative.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.small(match self.rank_kind {
                    RankTestKind::MannWhitney => {
                        "H₀: the two samples come from the same distribution; the p-value uses the \
                         normal approximation."
                    }
                    RankTestKind::Wilcoxon => {
                        "H₀: the median difference is zero; only complete pairs are used."
                    }
                    RankTestKind::KruskalWallis => {
                        "H₀: the groups come from the same distribution; the p-value uses the \
                         chi-square approximation."
                    }
                    RankTestKind::Friedman => {
                        "H₀: the groups come from the same distribution; every row is a block."
                    }
                    RankTestKind::MoodsMedian => {
                        "H₀: the groups have the same median; the counts around the grand median \
                         are compared."
                    }
                    RankTestKind::SignTest => {
                        "H₀: the median equals the hypothesised value; the p-value is the exact \
                         two-sided binomial."
                    }
                });
                if self.rank_kind == RankTestKind::SignTest {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Hypothesised median:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.rank_median)
                                .desired_width(120.0)
                                .hint_text("0"),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_rank_dialog = open && !run;
        if run {
            self.run_rank_test_dialog();
        }
    }

    /// Opens the dialog for a correlation matrix.
    pub fn open_correlation_dialog(&mut self) {
        self.correlation_columns = self.selected.clone().unwrap_or_default();
        self.show_correlation_dialog = true;
    }

    /// Builds and runs the correlation matrix of the dialog.
    pub fn run_correlation_dialog(&mut self) {
        let columns: Vec<String> = self
            .correlation_columns
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec = AnalysisSpec::correlation(columns);
        self.execute_spec(spec, "Correlation Matrix".to_owned());
        self.show_correlation_dialog = false;
    }

    fn correlation_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_correlation_dialog {
            return;
        }
        let mut open = self.show_correlation_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Correlation Matrix"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.text_edit_singleline(&mut self.correlation_columns)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Comma-separated column names; every pair is correlated.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_correlation_dialog = open && !run;
        if run {
            self.run_correlation_dialog();
        }
    }

    /// Opens the dialog for a curve fit.
    pub fn open_curve_dialog(&mut self) {
        self.curve_x = self.selected.clone().unwrap_or_default();
        self.curve_y = String::new();
        self.curve_model = CurveModel::Polynomial(2);
        self.curve_confidence = 95.0;
        self.show_curve_dialog = true;
    }

    /// Builds and runs the curve fit of the dialog.
    pub fn run_curve_dialog(&mut self) {
        if self.curve_x.is_empty() {
            self.set_error("Select the X column");
            return;
        }
        if self.curve_y.is_empty() {
            self.set_error("Select the Y column");
            return;
        }
        if self.curve_x == self.curve_y {
            self.set_error("Select two different columns");
            return;
        }
        let spec =
            AnalysisSpec::curve_fit(self.curve_x.clone(), self.curve_y.clone(), self.curve_model)
                .with_curve_fit_confidence((self.curve_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!("Curve Fit: {} vs {}", self.curve_y, self.curve_x);
        self.execute_spec(spec, label);
        self.show_curve_dialog = false;
    }

    fn curve_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_curve_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_curve_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Curve Fit"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("X:");
                    egui::ComboBox::from_id_salt("curve_x")
                        .selected_text(if self.curve_x.is_empty() {
                            "—".to_owned()
                        } else {
                            self.curve_x.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(&mut self.curve_x, name.clone(), name.as_str());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Y:");
                    egui::ComboBox::from_id_salt("curve_y")
                        .selected_text(if self.curve_y.is_empty() {
                            "—".to_owned()
                        } else {
                            self.curve_y.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(&mut self.curve_y, name.clone(), name.as_str());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Model:");
                    egui::ComboBox::from_id_salt("curve_model")
                        .selected_text(self.curve_model.name())
                        .show_ui(ui, |ui| {
                            for model in [
                                CurveModel::Polynomial(1),
                                CurveModel::Polynomial(2),
                                CurveModel::Polynomial(3),
                                CurveModel::Polynomial(4),
                                CurveModel::Polynomial(5),
                                CurveModel::Exponential,
                                CurveModel::Power,
                                CurveModel::Logistic,
                            ] {
                                ui.selectable_value(
                                    &mut self.curve_model,
                                    model,
                                    model.name().as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.curve_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "The exponential and the power model need positive values; they are fitted in \
                     the log scale.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_curve_dialog = open && !run;
        if run {
            self.run_curve_dialog();
        }
    }

    /// Opens the dialog for a two-way ANOVA.
    pub fn open_two_way_dialog(&mut self) {
        self.two_way_response = self.selected.clone().unwrap_or_default();
        self.two_way_first = String::new();
        self.two_way_second = String::new();
        self.show_two_way_dialog = true;
    }

    /// Builds and runs the two-way ANOVA of the dialog.
    pub fn run_two_way_dialog(&mut self) {
        if self.two_way_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        if self.two_way_first.is_empty() || self.two_way_second.is_empty() {
            self.set_error("Select the two factor columns");
            return;
        }
        if self.two_way_first == self.two_way_second {
            self.set_error("Select two different factor columns");
            return;
        }
        let spec = AnalysisSpec::two_way_anova(
            self.two_way_response.clone(),
            self.two_way_first.clone(),
            self.two_way_second.clone(),
        );
        let label = format!(
            "Two-Way ANOVA: {} by {} and {}",
            self.two_way_response, self.two_way_first, self.two_way_second
        );
        self.execute_spec(spec, label);
        self.show_two_way_dialog = false;
    }

    fn two_way_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_two_way_dialog {
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
        let all: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_two_way_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Two-Way ANOVA"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("two_way_response")
                        .selected_text(if self.two_way_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.two_way_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.two_way_response,
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
                    let label = ui.help_label("Factor A:");
                    egui::ComboBox::from_id_salt("two_way_first")
                        .selected_text(if self.two_way_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.two_way_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.two_way_first,
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
                    let label = ui.help_label("Factor B:");
                    egui::ComboBox::from_id_salt("two_way_second")
                        .selected_text(if self.two_way_second.is_empty() {
                            "—".to_owned()
                        } else {
                            self.two_way_second.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.two_way_second,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("The design must be complete and balanced.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_two_way_dialog = open && !run;
        if run {
            self.run_two_way_dialog();
        }
    }

    /// Opens the dialog for a cross-tabulation.
    pub fn open_crosstab_dialog(&mut self) {
        self.crosstab_row = self.selected.clone().unwrap_or_default();
        self.crosstab_column = String::new();
        self.show_crosstab_dialog = true;
    }

    /// Builds and runs the cross-tabulation of the dialog.
    pub fn run_crosstab_dialog(&mut self) {
        if self.crosstab_row.is_empty() || self.crosstab_column.is_empty() {
            self.set_error("Select the two variables");
            return;
        }
        if self.crosstab_row == self.crosstab_column {
            self.set_error("Select two different variables");
            return;
        }
        let spec = AnalysisSpec::chi_square_independence(
            self.crosstab_row.clone(),
            self.crosstab_column.clone(),
        );
        let label = format!(
            "Chi-Square Test: {} x {}",
            self.crosstab_row, self.crosstab_column
        );
        self.execute_spec(spec, label);
        self.show_crosstab_dialog = false;
    }

    fn crosstab_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_crosstab_dialog {
            return;
        }
        let all: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_crosstab_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Chi-Square Test for Independence"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Row:");
                    egui::ComboBox::from_id_salt("crosstab_row")
                        .selected_text(if self.crosstab_row.is_empty() {
                            "—".to_owned()
                        } else {
                            self.crosstab_row.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.crosstab_row,
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
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("crosstab_column")
                        .selected_text(if self.crosstab_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.crosstab_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.crosstab_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("The two variables need at least two levels each.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_crosstab_dialog = open && !run;
        if run {
            self.run_crosstab_dialog();
        }
    }

    /// Opens the dialog for the g chart (rare events).
    pub fn open_g_chart_dialog(&mut self) {
        self.g_chart_counts = self.selected.clone().unwrap_or_default();
        self.g_chart_times = String::new();
        self.show_g_chart_dialog = true;
    }

    /// Builds and runs the g chart of the dialog.
    pub fn run_g_chart_dialog(&mut self) {
        if self.g_chart_counts.is_empty() {
            self.set_error("Select the column with the number of events");
            return;
        }
        let times = if self.g_chart_times.is_empty() {
            None
        } else {
            Some(self.g_chart_times.clone())
        };
        let spec = AnalysisSpec::rare_events(self.g_chart_counts.clone(), times);
        let label = format!("G Chart (Rare Events): {}", self.g_chart_counts);
        self.execute_spec(spec, label);
        self.show_g_chart_dialog = false;
    }

    fn g_chart_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_g_chart_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_g_chart_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("G Chart (Rare Events)"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Events:");
                    egui::ComboBox::from_id_salt("g_chart_counts")
                        .selected_text(if self.g_chart_counts.is_empty() {
                            "—".to_owned()
                        } else {
                            self.g_chart_counts.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.g_chart_counts,
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
                    let label = ui.help_label("Times:");
                    egui::ComboBox::from_id_salt("g_chart_times")
                        .selected_text(if self.g_chart_times.is_empty() {
                            "— (sequence)".to_owned()
                        } else {
                            self.g_chart_times.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.g_chart_times,
                                String::new(),
                                "— (sequence)",
                            );
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.g_chart_times,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "The number of events between the failures of a unit; the limits are the \
                     Poisson limits.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_g_chart_dialog = open && !run;
        if run {
            self.run_g_chart_dialog();
        }
    }

    /// Opens the dialog for a partial correlation.
    pub fn open_partial_dialog(&mut self) {
        self.partial_x = self.selected.clone().unwrap_or_default();
        self.partial_y = String::new();
        self.partial_control = String::new();
        self.show_partial_dialog = true;
    }

    /// Builds and runs the partial correlation of the dialog.
    pub fn run_partial_dialog(&mut self) {
        if self.partial_x.is_empty() || self.partial_y.is_empty() || self.partial_control.is_empty()
        {
            self.set_error("Select the two variables and the control");
            return;
        }
        if self.partial_x == self.partial_y
            || self.partial_x == self.partial_control
            || self.partial_y == self.partial_control
        {
            self.set_error("Select three different columns");
            return;
        }
        let spec = AnalysisSpec::partial_correlation(
            self.partial_x.clone(),
            self.partial_y.clone(),
            self.partial_control.clone(),
        );
        let label = format!(
            "Partial Correlation: {} and {} given {}",
            self.partial_x, self.partial_y, self.partial_control
        );
        self.execute_spec(spec, label);
        self.show_partial_dialog = false;
    }

    fn partial_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_partial_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_partial_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Partial Correlation"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                for (label, slot) in [
                    ("X:", &mut self.partial_x),
                    ("Y:", &mut self.partial_y),
                    ("Control:", &mut self.partial_control),
                ] {
                    ui.horizontal(|ui| {
                        let label_response = ui.label(label);
                        egui::ComboBox::from_id_salt(format!("partial_{label}"))
                            .selected_text(if slot.is_empty() {
                                "—".to_owned()
                            } else {
                                slot.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(slot, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label_response.id)
                            .hover_from(&label_response);
                    });
                }
                ui.small("The linear effect of the control is removed from both variables.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_partial_dialog = open && !run;
        if run {
            self.run_partial_dialog();
        }
    }

    /// Opens the dialog for a tolerance interval.
    pub fn open_tolerance_dialog(&mut self) {
        self.tolerance_column = self.selected.clone().unwrap_or_default();
        self.tolerance_proportion = 95.0;
        self.tolerance_confidence = 95.0;
        self.tolerance_free = false;
        self.show_tolerance_dialog = true;
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

    /// Opens the attribute agreement dialog.
    pub fn open_agreement_dialog(&mut self) {
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        self.agreement_part = names.first().cloned().unwrap_or_default();
        self.agreement_appraiser = names.get(1).cloned().unwrap_or_default();
        self.agreement_rating = names.get(2).cloned().unwrap_or_default();
        self.show_agreement_dialog = true;
    }

    /// Runs the agreement analysis of the dialog.
    pub fn run_agreement_dialog(&mut self) {
        if self.agreement_part.is_empty()
            || self.agreement_appraiser.is_empty()
            || self.agreement_rating.is_empty()
        {
            self.set_error("Select the part, appraiser and rating columns");
            return;
        }
        if self.agreement_part == self.agreement_appraiser
            || self.agreement_part == self.agreement_rating
            || self.agreement_appraiser == self.agreement_rating
        {
            self.set_error("The three columns must be different");
            return;
        }
        let spec = AnalysisSpec::agreement(
            self.agreement_part.clone(),
            self.agreement_appraiser.clone(),
            self.agreement_rating.clone(),
        );
        self.execute_spec(spec, "Attribute Agreement".to_owned());
        self.show_agreement_dialog = false;
    }

    fn agreement_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_agreement_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_agreement_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Attribute Agreement"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Part:", &mut self.agreement_part),
                    ("Appraiser:", &mut self.agreement_appraiser),
                    ("Rating:", &mut self.agreement_rating),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.label(title);
                        egui::ComboBox::from_id_salt(title)
                            .selected_text(if value.is_empty() {
                                "—".to_owned()
                            } else {
                                value.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(value, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.small(
                    "Each appraiser should rate every part at least twice; kappa measures the \
                     agreement.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_agreement_dialog = open && !run;
        if run {
            self.run_agreement_dialog();
        }
    }

    /// Opens the Taguchi analysis dialog.
    pub fn open_taguchi_dialog(&mut self) {
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        self.taguchi_response = self
            .selected
            .clone()
            .unwrap_or_else(|| names.last().cloned().unwrap_or_default());
        self.taguchi_factors = names
            .iter()
            .filter(|name| **name != self.taguchi_response)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        self.taguchi_quality = fp_core::taguchi::Quality::Smaller;
        self.show_taguchi_dialog = true;
    }

    /// Runs the Taguchi analysis of the dialog.
    pub fn run_taguchi_dialog(&mut self) {
        if self.taguchi_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let factors: Vec<String> = self
            .taguchi_factors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if factors.is_empty() {
            self.set_error("List at least one factor column");
            return;
        }
        if factors.contains(&self.taguchi_response) {
            self.set_error("The response cannot also be a factor");
            return;
        }
        let spec =
            AnalysisSpec::taguchi(self.taguchi_response.clone(), factors, self.taguchi_quality);
        self.execute_spec(spec, format!("Taguchi: {}", self.taguchi_response));
        self.show_taguchi_dialog = false;
    }

    fn taguchi_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_taguchi_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_taguchi_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Taguchi Analysis"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("taguchi_response")
                        .selected_text(if self.taguchi_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.taguchi_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.taguchi_response,
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
                    let label = ui.help_label("Factors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.taguchi_factors)
                            .desired_width(240.0)
                            .hint_text("A,B,C"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_label("Quality:");
                    for (quality, label) in [
                        (fp_core::taguchi::Quality::Smaller, "Smaller is better"),
                        (fp_core::taguchi::Quality::Larger, "Larger is better"),
                        (fp_core::taguchi::Quality::Nominal, "Nominal is best"),
                    ] {
                        ui.selectable_value(&mut self.taguchi_quality, quality, label);
                    }
                });
                ui.small("Factor columns hold the level numbers 1, 2, …");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_taguchi_dialog = open && !run;
        if run {
            self.run_taguchi_dialog();
        }
    }

    /// Opens the Taguchi design generator.
    pub fn open_taguchi_design_dialog(&mut self) {
        self.taguchi_runs = 8;
        self.taguchi_design_factors = 3;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_taguchi_design_dialog = true;
    }

    /// Generates the Taguchi design and replaces the worksheet.
    pub fn run_taguchi_design_dialog(&mut self) {
        let df = match fp_core::taguchi::design(
            self.taguchi_runs,
            self.taguchi_design_factors,
            self.design_randomize,
            self.design_seed,
        ) {
            Ok(df) => df,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let rows = df.n_rows();
        self.set_data(df, "Taguchi design".to_owned());
        self.append_session(&format!(
            "Taguchi design generated: L{rows} with {} factor(s).\n",
            self.taguchi_design_factors
        ));
        self.show_taguchi_design_dialog = false;
    }

    fn taguchi_design_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_taguchi_design_dialog {
            return;
        }
        let mut open = self.show_taguchi_design_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Taguchi Design"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.help_label("Array:");
                    for runs in [8usize, 9, 12, 16, 18, 27] {
                        ui.selectable_value(&mut self.taguchi_runs, runs, format!("L{runs}"));
                    }
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Factors:");
                    ui.add(egui::DragValue::new(&mut self.taguchi_design_factors).range(1..=15))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_checkbox(&mut self.design_randomize, "Randomize run order");
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.design_seed).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Levels are written as 1, 2, … and analysed by the Taguchi method.");
                ui.separator();
                if ui.help_button(self.catalog().label("Generate")).clicked() {
                    run = true;
                }
            });
        self.show_taguchi_design_dialog = open && !run;
        if run {
            self.run_taguchi_design_dialog();
        }
    }

    /// Opens the acceptance sampling dialog (no worksheet required).
    pub fn open_sampling_dialog(&mut self) {
        self.sampling_plan_mode = false;
        self.show_sampling_dialog = true;
    }

    /// Runs the acceptance sampling of the dialog.
    pub fn run_sampling_dialog(&mut self) {
        let outcome = if self.sampling_plan_mode {
            fp_core::sampling::analyze_plan(
                (self.sampling_aql / 100.0).clamp(0.0001, 0.99),
                (self.sampling_ltpd / 100.0).clamp(0.0002, 0.999),
                (self.sampling_alpha / 100.0).clamp(0.001, 0.5),
                (self.sampling_beta / 100.0).clamp(0.001, 0.5),
                (self.sampling_lot > 0).then_some(self.sampling_lot),
            )
        } else {
            fp_core::sampling::analyze(
                (self.sampling_lot > 0).then_some(self.sampling_lot),
                self.sampling_sample,
                self.sampling_accept,
            )
        };
        match outcome {
            Ok(out) => {
                for result in &out.results {
                    self.append_result(result);
                }
                self.show_plot = out.plot.is_some();
                self.plot = out.plot;
                self.status = "Acceptance Sampling".to_owned();
                self.error = None;
                self.show_sampling_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    fn sampling_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_sampling_dialog {
            return;
        }
        let mut open = self.show_sampling_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Acceptance Sampling"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.sampling_plan_mode, false, "Evaluate a plan");
                    ui.selectable_value(&mut self.sampling_plan_mode, true, "Plan a scheme");
                });
                if self.sampling_plan_mode {
                    ui.horizontal(|ui| {
                        let aql = ui.help_label("AQL %:");
                        ui.add(
                            egui::DragValue::new(&mut self.sampling_aql)
                                .range(0.01..=20.0)
                                .speed(0.1),
                        )
                        .labelled_by(aql.id)
                        .hover_from(&aql);
                        let ltpd = ui.help_label("LTPD %:");
                        ui.add(
                            egui::DragValue::new(&mut self.sampling_ltpd)
                                .range(0.1..=50.0)
                                .speed(0.1),
                        )
                        .labelled_by(ltpd.id)
                        .hover_from(&ltpd);
                    });
                    ui.horizontal(|ui| {
                        let producer = ui.help_label("Producer risk %:");
                        ui.add(egui::DragValue::new(&mut self.sampling_alpha).range(0.5..=50.0))
                            .labelled_by(producer.id)
                            .hover_from(&producer);
                        let consumer = ui.help_label("Consumer risk %:");
                        ui.add(egui::DragValue::new(&mut self.sampling_beta).range(0.5..=50.0))
                            .labelled_by(consumer.id)
                            .hover_from(&consumer);
                    });
                    ui.small("The smallest single-sampling scheme that meets both risks.");
                } else {
                    ui.horizontal(|ui| {
                        let sample = ui.help_label("Sample size:");
                        ui.add(egui::DragValue::new(&mut self.sampling_sample).range(1..=5000))
                            .labelled_by(sample.id)
                            .hover_from(&sample);
                        let accept = ui.help_label("Acceptance:");
                        ui.add(egui::DragValue::new(&mut self.sampling_accept).range(0..=500))
                            .labelled_by(accept.id)
                            .hover_from(&accept);
                    });
                    ui.horizontal(|ui| {
                        let lot = ui.help_label("Lot size:");
                        ui.add(egui::DragValue::new(&mut self.sampling_lot).range(0..=10_000_000))
                            .labelled_by(lot.id)
                            .hover_from(&lot);
                        ui.small("0 = large lot (binomial)");
                    });
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_sampling_dialog = open && !run;
        if run {
            self.run_sampling_dialog();
        }
    }

    /// Opens the Hotelling T² chart dialog.
    pub fn open_t2_dialog(&mut self) {
        self.t2_columns = self.selected.clone().unwrap_or_default();
        self.t2_subgroup = 3;
        self.t2_confidence = 95.0;
        self.show_t2_dialog = true;
    }

    /// Runs the T² chart of the dialog.
    pub fn run_t2_dialog(&mut self) {
        let columns: Vec<String> = self
            .t2_columns
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec = AnalysisSpec::hotelling_t2(columns, self.t2_subgroup)
            .with_t2_confidence((self.t2_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, "Hotelling T² Chart".to_owned());
        self.show_t2_dialog = false;
    }

    fn t2_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_t2_dialog {
            return;
        }
        let mut open = self.show_t2_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Hotelling T² Chart"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.text_edit_singleline(&mut self.t2_columns)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let subgroup = ui.help_label("Subgroup:");
                    ui.add(egui::DragValue::new(&mut self.t2_subgroup).range(2..=25))
                        .labelled_by(subgroup.id)
                        .hover_from(&subgroup);
                    let confidence = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.t2_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(confidence.id)
                    .hover_from(&confidence);
                });
                ui.small("Rows are taken in order; an incomplete last subgroup is dropped.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_t2_dialog = open && !run;
        if run {
            self.run_t2_dialog();
        }
    }

    /// Opens the k-means clustering dialog.
    pub fn open_kmeans_dialog(&mut self) {
        self.kmeans_columns = self.selected.clone().unwrap_or_default();
        self.kmeans_clusters = 3;
        self.kmeans_standardise = true;
        self.kmeans_seed = 2026;
        self.show_kmeans_dialog = true;
    }

    /// Runs the clustering of the dialog.
    pub fn run_kmeans_dialog(&mut self) {
        let columns: Vec<String> = self
            .kmeans_columns
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec = AnalysisSpec::kmeans(columns, self.kmeans_clusters)
            .with_kmeans_options(self.kmeans_standardise, self.kmeans_seed);
        self.execute_spec(spec, "K-Means Clustering".to_owned());
        self.show_kmeans_dialog = false;
    }

    fn kmeans_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_kmeans_dialog {
            return;
        }
        let mut open = self.show_kmeans_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("K-Means Clustering"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.text_edit_singleline(&mut self.kmeans_columns)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Clusters:");
                    ui.add(egui::DragValue::new(&mut self.kmeans_clusters).range(2..=20))
                        .labelled_by(label.id)
                        .hover_from(&label);
                    ui.help_checkbox(&mut self.kmeans_standardise, "Standardise (z scores)");
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.kmeans_seed).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                    ui.small("The same seed gives the same clusters.");
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_kmeans_dialog = open && !run;
        if run {
            self.run_kmeans_dialog();
        }
    }

    /// Opens the principal component analysis dialog.
    pub fn open_pca_dialog(&mut self) {
        self.pca_columns = self.selected.clone().unwrap_or_default();
        self.pca_keep = 2;
        self.show_pca_dialog = true;
    }

    /// Runs the principal component analysis of the dialog.
    pub fn run_pca_dialog(&mut self) {
        let columns: Vec<String> = self
            .pca_columns
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec = AnalysisSpec::pca(columns, self.pca_keep);
        self.execute_spec(spec, "Principal Component Analysis".to_owned());
        self.show_pca_dialog = false;
    }

    fn pca_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_pca_dialog {
            return;
        }
        let mut open = self.show_pca_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Principal Component Analysis"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.text_edit_singleline(&mut self.pca_columns)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Score components:");
                    ui.add(egui::DragValue::new(&mut self.pca_keep).range(1..=10))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Comma-separated numeric columns; rows must be complete.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_pca_dialog = open && !run;
        if run {
            self.run_pca_dialog();
        }
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

    /// Opens the sort dialog.
    pub fn open_sort_dialog(&mut self) {
        self.sort_keys = self.selected.clone().unwrap_or_default();
        self.show_sort_dialog = true;
    }

    /// Sorts the worksheet with the keys of the dialog.
    pub fn run_sort_dialog(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let mut keys = Vec::new();
        for part in self.sort_keys.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (name, direction) = match part.split_once(':') {
                Some((name, direction)) => {
                    let Some(direction) = fp_core::reshape::SortDirection::parse(direction.trim())
                    else {
                        self.set_error(format!("Unknown direction: {direction}"));
                        return;
                    };
                    (name.trim(), direction)
                }
                None => (part, fp_core::reshape::SortDirection::Ascending),
            };
            keys.push((name.to_owned(), direction));
        }
        if keys.is_empty() {
            self.set_error("List at least one sort column");
            return;
        }
        match fp_core::reshape::sort(&df, &keys) {
            Ok(sorted) => {
                self.replace_worksheet(sorted, "Sorted worksheet");
                self.show_sort_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
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

    /// Opens the stack dialog.
    pub fn open_stack_dialog(&mut self) {
        self.stack_columns = self.selected.clone().unwrap_or_default();
        self.show_stack_dialog = true;
    }

    /// Stacks the columns of the dialog into `Value` and `Variable`.
    pub fn run_stack_dialog(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let columns: Vec<String> = self
            .stack_columns
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.is_empty() {
            self.set_error("List at least one column to stack");
            return;
        }
        match fp_core::reshape::stack(&df, &columns) {
            Ok(stacked) => {
                self.replace_worksheet(stacked, "Stacked worksheet");
                self.show_stack_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Opens the unstack dialog.
    pub fn open_unstack_dialog(&mut self) {
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        self.unstack_value = self
            .selected
            .clone()
            .unwrap_or_else(|| names.first().cloned().unwrap_or_default());
        self.unstack_group = names
            .iter()
            .find(|name| **name != self.unstack_value)
            .cloned()
            .unwrap_or_default();
        self.show_unstack_dialog = true;
    }

    /// Unstacks the value column by the levels of the group column.
    pub fn run_unstack_dialog(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        match fp_core::reshape::unstack(&df, &self.unstack_value, &self.unstack_group) {
            Ok(wide) => {
                self.replace_worksheet(wide, "Unstacked worksheet");
                self.show_unstack_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Opens the date-difference dialog.
    pub fn open_date_diff_dialog(&mut self) {
        let dates: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| column.dtype() == DType::Date)
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        self.date_diff_first = dates.first().cloned().unwrap_or_default();
        self.date_diff_second = dates.get(1).cloned().unwrap_or_default();
        self.date_diff_name = "days".to_owned();
        self.show_date_diff_dialog = true;
    }

    /// Runs the date difference of the dialog.
    pub fn run_date_diff_dialog(&mut self) {
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if self.date_diff_first.is_empty() || self.date_diff_second.is_empty() {
            self.set_error("Select the two date columns");
            return;
        }
        if self.date_diff_name.trim().is_empty() {
            self.set_error("Name the new column");
            return;
        }
        match fp_core::dates::add_difference_column(
            &mut df,
            &self.date_diff_first,
            &self.date_diff_second,
            self.date_diff_name.trim(),
        ) {
            Ok(()) => {
                let name = self.date_diff_name.trim().to_owned();
                self.set_data(df, "Date difference".to_owned());
                self.append_session(&format!(
                    "Column {name} = {} − {} in days.\n",
                    self.date_diff_second, self.date_diff_first
                ));
                self.status = format!("Column {name} added");
                self.show_date_diff_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Opens the add-days dialog.
    pub fn open_date_shift_dialog(&mut self) {
        let dates: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| column.dtype() == DType::Date)
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        self.date_shift_column = dates.first().cloned().unwrap_or_default();
        self.date_shift_days = 0;
        self.date_shift_name = "shifted".to_owned();
        self.show_date_shift_dialog = true;
    }

    /// Runs the date shift of the dialog.
    pub fn run_date_shift_dialog(&mut self) {
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if self.date_shift_column.is_empty() {
            self.set_error("Select the date column");
            return;
        }
        if self.date_shift_name.trim().is_empty() {
            self.set_error("Name the new column");
            return;
        }
        match fp_core::dates::add_shift_column(
            &mut df,
            &self.date_shift_column,
            self.date_shift_days,
            self.date_shift_name.trim(),
        ) {
            Ok(()) => {
                let name = self.date_shift_name.trim().to_owned();
                self.set_data(df, "Date shift".to_owned());
                self.append_session(&format!(
                    "Column {name} = {} {} days.\n",
                    self.date_shift_column, self.date_shift_days
                ));
                self.status = format!("Column {name} added");
                self.show_date_shift_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    /// Opens the subset dialog.
    pub fn open_subset_dialog(&mut self) {
        self.subset_column = self.selected.clone().unwrap_or_default();
        self.subset_op = fp_core::reshape::SubsetOp::Equal;
        self.subset_value.clear();
        self.show_subset_dialog = true;
    }

    /// Keeps the rows that match the condition of the dialog.
    pub fn run_subset_dialog(&mut self) {
        let Some(df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if self.subset_column.is_empty() {
            self.set_error("Select the column of the condition");
            return;
        }
        match fp_core::reshape::subset(&df, &self.subset_column, self.subset_op, &self.subset_value)
        {
            Ok(filtered) => {
                self.replace_worksheet(filtered, "Subset worksheet");
                self.show_subset_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    fn sort_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_sort_dialog {
            return;
        }
        let mut open = self.show_sort_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Sort Worksheet"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Keys:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.sort_keys)
                            .desired_width(260.0)
                            .hint_text("x1:desc,x2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Comma-separated columns; append :desc for a descending key.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_sort_dialog = open && !run;
        if run {
            self.run_sort_dialog();
        }
    }

    fn stack_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_stack_dialog {
            return;
        }
        let mut open = self.show_stack_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Stack Columns"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.stack_columns)
                            .desired_width(260.0)
                            .hint_text("C1,C2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("The result has the Value and Variable columns.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_stack_dialog = open && !run;
        if run {
            self.run_stack_dialog();
        }
    }

    fn unstack_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_unstack_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_unstack_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Unstack Column"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Value:", &mut self.unstack_value),
                    ("Group:", &mut self.unstack_group),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.label(title);
                        egui::ComboBox::from_id_salt(title)
                            .selected_text(if value.is_empty() {
                                "—".to_owned()
                            } else {
                                value.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(value, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.small("Each level of the group becomes a column of its values.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_unstack_dialog = open && !run;
        if run {
            self.run_unstack_dialog();
        }
    }

    fn date_diff_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_date_diff_dialog {
            return;
        }
        let dates: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| column.dtype() == DType::Date)
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_date_diff_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Date Difference"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("First date:", &mut self.date_diff_first),
                    ("Second date:", &mut self.date_diff_second),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.help_label(title);
                        egui::ComboBox::from_id_salt(title)
                            .selected_text(if value.is_empty() {
                                "—".to_owned()
                            } else {
                                value.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &dates {
                                    ui.selectable_value(value, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("New column:");
                    ui.text_edit_singleline(&mut self.date_diff_name)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("The result is second − first in days.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_date_diff_dialog = open && !run;
        if run {
            self.run_date_diff_dialog();
        }
    }

    fn date_shift_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_date_shift_dialog {
            return;
        }
        let dates: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|column| column.dtype() == DType::Date)
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_date_shift_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Add Days"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Date column:");
                    egui::ComboBox::from_id_salt("date_shift_column")
                        .selected_text(if self.date_shift_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.date_shift_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &dates {
                                ui.selectable_value(
                                    &mut self.date_shift_column,
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
                    let label = ui.help_label("Days:");
                    ui.add(
                        egui::DragValue::new(&mut self.date_shift_days)
                            .range(-100_000..=100_000)
                            .speed(1.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("New column:");
                    ui.text_edit_singleline(&mut self.date_shift_name)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Negative values move the dates backwards.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_date_shift_dialog = open && !run;
        if run {
            self.run_date_shift_dialog();
        }
    }

    fn subset_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_subset_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_subset_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Subset Worksheet"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("subset_column")
                        .selected_text(if self.subset_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.subset_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.subset_column,
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
                    ui.help_label("Condition:");
                    for op in [
                        fp_core::reshape::SubsetOp::Equal,
                        fp_core::reshape::SubsetOp::NotEqual,
                        fp_core::reshape::SubsetOp::Less,
                        fp_core::reshape::SubsetOp::LessOrEqual,
                        fp_core::reshape::SubsetOp::Greater,
                        fp_core::reshape::SubsetOp::GreaterOrEqual,
                        fp_core::reshape::SubsetOp::Contains,
                    ] {
                        ui.selectable_value(&mut self.subset_op, op, op.symbol());
                    }
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Value:");
                    ui.add(egui::TextEdit::singleline(&mut self.subset_value).desired_width(180.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Numeric comparisons are used when both sides are numbers.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_subset_dialog = open && !run;
        if run {
            self.run_subset_dialog();
        }
    }

    /// Opens the time-series dialog.
    pub fn open_timeseries_dialog(&mut self) {
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
        self.timeseries_column = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.show_timeseries_dialog = true;
    }

    /// Runs the time-series method of the dialog.
    pub fn run_timeseries_dialog(&mut self) {
        if self.timeseries_column.is_empty() {
            self.set_error("Select the series column");
            return;
        }
        let spec = match self.timeseries_kind {
            TimeSeriesKind::MovingAverage => {
                AnalysisSpec::moving_average(self.timeseries_column.clone(), self.timeseries_window)
            }
            TimeSeriesKind::Decomposition => AnalysisSpec::decomposition(
                self.timeseries_column.clone(),
                self.timeseries_period,
                self.timeseries_model,
            ),
            TimeSeriesKind::ExponentialSmoothing => AnalysisSpec::exponential_smoothing(
                self.timeseries_column.clone(),
                self.timeseries_season,
                self.timeseries_horizon,
            ),
        };
        let label = format!(
            "{}: {}",
            self.timeseries_kind.name(),
            self.timeseries_column
        );
        self.execute_spec(spec, label);
        self.show_timeseries_dialog = false;
    }

    fn timeseries_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_timeseries_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_timeseries_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Time Series"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("timeseries_column")
                        .selected_text(if self.timeseries_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.timeseries_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.timeseries_column,
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
                    ui.help_label("Method:");
                    for (kind, label) in [
                        (TimeSeriesKind::MovingAverage, "Moving average"),
                        (TimeSeriesKind::Decomposition, "Decomposition"),
                        (TimeSeriesKind::ExponentialSmoothing, "Smoothing"),
                    ] {
                        ui.selectable_value(&mut self.timeseries_kind, kind, label);
                    }
                });
                match self.timeseries_kind {
                    TimeSeriesKind::MovingAverage => {
                        ui.horizontal(|ui| {
                            let label = ui.help_label("Window:");
                            ui.add(
                                egui::DragValue::new(&mut self.timeseries_window).range(2..=100),
                            )
                            .labelled_by(label.id)
                            .hover_from(&label);
                        });
                    }
                    TimeSeriesKind::Decomposition => {
                        ui.horizontal(|ui| {
                            let label = ui.help_label("Period:");
                            ui.add(
                                egui::DragValue::new(&mut self.timeseries_period).range(2..=365),
                            )
                            .labelled_by(label.id)
                            .hover_from(&label);
                        });
                        ui.horizontal(|ui| {
                            ui.help_label("Model:");
                            ui.selectable_value(
                                &mut self.timeseries_model,
                                fp_core::timeseries::DecompositionModel::Additive,
                                "Additive",
                            );
                            ui.selectable_value(
                                &mut self.timeseries_model,
                                fp_core::timeseries::DecompositionModel::Multiplicative,
                                "Multiplicative",
                            );
                        });
                    }
                    TimeSeriesKind::ExponentialSmoothing => {
                        ui.horizontal(|ui| {
                            let label = ui.help_label("Season:");
                            ui.add(
                                egui::DragValue::new(&mut self.timeseries_season).range(1..=365),
                            )
                            .labelled_by(label.id)
                            .hover_from(&label);
                            ui.small("1 = simple smoothing");
                        });
                        ui.horizontal(|ui| {
                            let label = ui.help_label("Forecast periods:");
                            ui.add(
                                egui::DragValue::new(&mut self.timeseries_horizon).range(1..=100),
                            )
                            .labelled_by(label.id)
                            .hover_from(&label);
                        });
                    }
                }
                ui.small("The series is read in row order; missing values are dropped.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_timeseries_dialog = open && !run;
        if run {
            self.run_timeseries_dialog();
        }
    }

    /// Opens the accelerated life testing dialog.
    pub fn open_alt_dialog(&mut self) {
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
        self.alt_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.alt_stress = numeric
            .iter()
            .find(|name| **name != self.alt_time)
            .cloned()
            .unwrap_or_default();
        self.alt_mode.clear();
        self.alt_horizon.clear();
        self.alt_use_stress = 300.0;
        self.alt_life = fp_core::alt::LifeModel::Weibull;
        self.alt_acceleration = fp_core::alt::AccelerationModel::Arrhenius;
        self.show_alt_dialog = true;
    }

    /// Runs the accelerated life test of the dialog.
    pub fn run_alt_dialog(&mut self) {
        if self.alt_time.is_empty() || self.alt_stress.is_empty() {
            self.set_error("Select the time and the stress columns");
            return;
        }
        if self.alt_time == self.alt_stress {
            self.set_error("The time and the stress must be different columns");
            return;
        }
        let horizon = if self.alt_horizon.trim().is_empty() {
            None
        } else {
            match self.alt_horizon.trim().parse::<f64>() {
                Ok(value) if value > 0.0 => Some(value),
                _ => {
                    self.set_error("The horizon must be a positive number");
                    return;
                }
            }
        };
        let mode = (!self.alt_mode.is_empty()).then(|| self.alt_mode.clone());
        let spec = AnalysisSpec::accelerated_life(self.alt_time.clone(), self.alt_stress.clone())
            .with_accelerated_life_options(None, 1.0, self.alt_use_stress)
            .with_accelerated_life_model(self.alt_life, self.alt_acceleration)
            .with_competing_mode(mode)
            .with_competing_horizon(horizon);
        let label = format!("Accelerated Life: {}", self.alt_time);
        self.execute_spec(spec, label);
        self.show_alt_dialog = false;
    }

    fn alt_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_alt_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let all_names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_alt_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Accelerated Life Testing"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Time:", &mut self.alt_time),
                    ("Stress:", &mut self.alt_stress),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.label(title);
                        egui::ComboBox::from_id_salt(title)
                            .selected_text(if value.is_empty() {
                                "—".to_owned()
                            } else {
                                value.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(value, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Use stress:");
                    ui.add(egui::DragValue::new(&mut self.alt_use_stress).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Mode column:");
                    egui::ComboBox::from_id_salt("alt_mode")
                        .selected_text(if self.alt_mode.is_empty() {
                            "None".to_owned()
                        } else {
                            self.alt_mode.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.alt_mode, String::new(), "None");
                            for name in &all_names {
                                ui.selectable_value(
                                    &mut self.alt_mode,
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
                    let label = ui.help_label("Horizon:");
                    ui.text_edit_singleline(&mut self.alt_horizon)
                        .labelled_by(label.id)
                        .hover_from(&label);
                    ui.small("empty = last failure");
                });
                ui.horizontal(|ui| {
                    ui.help_label("Life:");
                    for (life, label) in [
                        (fp_core::alt::LifeModel::Weibull, "Weibull"),
                        (fp_core::alt::LifeModel::Lognormal, "Lognormal"),
                        (fp_core::alt::LifeModel::Exponential, "Exponential"),
                    ] {
                        ui.selectable_value(&mut self.alt_life, life, label);
                    }
                });
                ui.horizontal(|ui| {
                    ui.help_label("Acceleration:");
                    for (model, label) in [
                        (fp_core::alt::AccelerationModel::Arrhenius, "Arrhenius"),
                        (fp_core::alt::AccelerationModel::InversePower, "Power"),
                        (fp_core::alt::AccelerationModel::Eyring, "Eyring"),
                    ] {
                        ui.selectable_value(&mut self.alt_acceleration, model, label);
                    }
                });
                ui.small(
                    "Arrhenius/Weibull model: the stress should be an absolute temperature \
                     (kelvin); the life at the use stress is an extrapolation.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_alt_dialog = open && !run;
        if run {
            self.run_alt_dialog();
        }
    }

    /// Opens the step-stress ALT dialog.
    pub fn open_step_stress_dialog(&mut self) {
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
        let all: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        self.ss_duration = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.ss_stress = numeric
            .iter()
            .find(|name| **name != self.ss_duration)
            .cloned()
            .unwrap_or_default();
        self.ss_unit = all
            .iter()
            .find(|name| **name != self.ss_duration && **name != self.ss_stress)
            .cloned()
            .unwrap_or_default();
        self.ss_event = String::new();
        self.ss_use_stress = 300.0;
        self.ss_life = fp_core::alt::LifeModel::Weibull;
        self.ss_acceleration = fp_core::alt::AccelerationModel::Arrhenius;
        self.show_step_stress_dialog = true;
    }

    /// Runs the step-stress model of the dialog.
    pub fn run_step_stress_dialog(&mut self) {
        if self.ss_duration.is_empty()
            || self.ss_stress.is_empty()
            || self.ss_unit.is_empty()
            || self.ss_event.is_empty()
        {
            self.set_error("Select the duration, stress, unit and event columns");
            return;
        }
        let spec = AnalysisSpec::step_stress_alt(
            self.ss_duration.clone(),
            self.ss_stress.clone(),
            self.ss_unit.clone(),
            self.ss_event.clone(),
            self.ss_use_stress,
            self.ss_life,
            self.ss_acceleration,
        );
        self.execute_spec(spec, "Step-Stress ALT".to_owned());
        self.show_step_stress_dialog = false;
    }

    fn step_stress_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_step_stress_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_step_stress_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Step-Stress ALT"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Duration:", &mut self.ss_duration),
                    ("Stress:", &mut self.ss_stress),
                    ("Unit:", &mut self.ss_unit),
                    ("Event:", &mut self.ss_event),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.label(title);
                        egui::ComboBox::from_id_salt(title)
                            .selected_text(if value.is_empty() {
                                "—".to_owned()
                            } else {
                                value.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(value, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Use stress:");
                    ui.add(egui::DragValue::new(&mut self.ss_use_stress).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_label("Life:");
                    for (life, label) in [
                        (fp_core::alt::LifeModel::Weibull, "Weibull"),
                        (fp_core::alt::LifeModel::Exponential, "Exponential"),
                    ] {
                        ui.selectable_value(&mut self.ss_life, life, label);
                    }
                });
                ui.horizontal(|ui| {
                    ui.help_label("Acceleration:");
                    for (model, label) in [
                        (fp_core::alt::AccelerationModel::Arrhenius, "Arrhenius"),
                        (fp_core::alt::AccelerationModel::InversePower, "Power"),
                        (fp_core::alt::AccelerationModel::Eyring, "Eyring"),
                    ] {
                        ui.selectable_value(&mut self.ss_acceleration, model, label);
                    }
                });
                ui.small("One row per step; the failure row is the last step of its unit.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_step_stress_dialog = open && !run;
        if run {
            self.run_step_stress_dialog();
        }
    }

    /// Opens the time-series plot dialog.
    pub fn open_series_dialog(&mut self) {
        self.series_column = self.selected.clone().unwrap_or_default();
        self.series_time.clear();
        self.show_series_dialog = true;
    }

    /// Runs the time-series plot of the dialog.
    pub fn run_series_dialog(&mut self) {
        if self.series_column.is_empty() {
            self.set_error("Select the column to plot");
            return;
        }
        let time =
            (!self.series_time.trim().is_empty()).then(|| self.series_time.trim().to_owned());
        if time.as_deref() == Some(self.series_column.as_str()) {
            self.set_error("The time column must be different from the series");
            return;
        }
        let spec = AnalysisSpec::series_plot(self.series_column.clone()).with_series_time(time);
        self.execute_spec(spec, format!("Time Series Plot: {}", self.series_column));
        self.show_series_dialog = false;
    }

    fn series_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_series_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_series_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Time Series Plot"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("series_column")
                        .selected_text(if self.series_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.series_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.series_column,
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
                    let label = ui.help_label("Time (optional):");
                    egui::ComboBox::from_id_salt("series_time")
                        .selected_text(if self.series_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.series_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.series_time, String::new(), "—");
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.series_time,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Without a time column the observations are plotted in row order.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_series_dialog = open && !run;
        if run {
            self.run_series_dialog();
        }
    }

    /// Opens the stem-and-leaf dialog.
    pub fn open_stem_dialog(&mut self) {
        self.stem_column = self.selected.clone().unwrap_or_default();
        self.show_stem_dialog = true;
    }

    /// Runs the stem-and-leaf display of the dialog.
    pub fn run_stem_dialog(&mut self) {
        if self.stem_column.is_empty() {
            self.set_error("Select the column to display");
            return;
        }
        let spec = AnalysisSpec::stem_leaf(self.stem_column.clone());
        self.execute_spec(spec, format!("Stem-and-Leaf: {}", self.stem_column));
        self.show_stem_dialog = false;
    }

    fn stem_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_stem_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_stem_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Stem-and-Leaf"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("stem_column")
                        .selected_text(if self.stem_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.stem_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.stem_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("The leaf unit is chosen to give between five and twenty stems.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_stem_dialog = open && !run;
        if run {
            self.run_stem_dialog();
        }
    }

    /// Opens the empirical CDF dialog.
    pub fn open_ecdf_dialog(&mut self) {
        self.ecdf_column = self.selected.clone().unwrap_or_default();
        self.ecdf_confidence = 95.0;
        self.show_ecdf_dialog = true;
    }

    /// Runs the empirical CDF of the dialog.
    pub fn run_ecdf_dialog(&mut self) {
        if self.ecdf_column.is_empty() {
            self.set_error("Select the column to plot");
            return;
        }
        let spec = AnalysisSpec::ecdf(self.ecdf_column.clone())
            .with_ecdf_confidence((self.ecdf_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, format!("Empirical CDF: {}", self.ecdf_column));
        self.show_ecdf_dialog = false;
    }

    fn ecdf_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_ecdf_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_ecdf_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Empirical CDF"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("ecdf_column")
                        .selected_text(if self.ecdf_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.ecdf_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.ecdf_column,
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
                    let label = ui.help_label("Band confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.ecdf_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Step function with a Dvoretzky–Kiefer–Wolfowitz band.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_ecdf_dialog = open && !run;
        if run {
            self.run_ecdf_dialog();
        }
    }

    /// Opens the dotplot dialog.
    pub fn open_dotplot_dialog(&mut self) {
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
        self.dot_column = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.dot_factor.clear();
        self.show_dot_dialog = true;
    }

    /// Runs the dotplot of the dialog.
    pub fn run_dotplot_dialog(&mut self) {
        if self.dot_column.is_empty() {
            self.set_error("Select the column to plot");
            return;
        }
        let factor =
            (!self.dot_factor.trim().is_empty()).then(|| self.dot_factor.trim().to_owned());
        if factor.as_deref() == Some(self.dot_column.as_str()) {
            self.set_error("The factor must be a different column");
            return;
        }
        let spec = AnalysisSpec::dotplot(self.dot_column.clone()).with_dotplot_factor(factor);
        let label = format!("Dotplot: {}", self.dot_column);
        self.execute_spec(spec, label);
        self.show_dot_dialog = false;
    }

    fn dotplot_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_dot_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_dot_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Dotplot"))
            .open(&mut open)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("dot_column")
                        .selected_text(if self.dot_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.dot_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.dot_column,
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
                    let label = ui.help_label("By (optional):");
                    egui::ComboBox::from_id_salt("dot_factor")
                        .selected_text(if self.dot_factor.is_empty() {
                            "—".to_owned()
                        } else {
                            self.dot_factor.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.dot_factor, String::new(), "—");
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.dot_factor,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("One dot per observation; coincident values are stacked.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_dot_dialog = open && !run;
        if run {
            self.run_dotplot_dialog();
        }
    }

    /// Opens the matrix plot dialog.
    pub fn open_matrix_dialog(&mut self) {
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
        self.matrix_columns = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| {
                numeric
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            });
        self.matrix_bins = 8;
        self.show_matrix_dialog = true;
    }

    /// Runs the matrix plot of the dialog.
    pub fn run_matrix_dialog(&mut self) {
        let columns: Vec<String> = self
            .matrix_columns
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if columns.len() < 2 {
            self.set_error("List at least two columns, separated by commas");
            return;
        }
        let spec =
            AnalysisSpec::matrix_plot(columns).with_matrix_bins(self.matrix_bins.clamp(3, 30));
        self.execute_spec(spec, "Matrix Plot".to_owned());
        self.show_matrix_dialog = false;
    }

    fn matrix_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_matrix_dialog {
            return;
        }
        let mut open = self.show_matrix_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Matrix Plot"))
            .open(&mut open)
            .resizable(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Columns:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.matrix_columns)
                            .desired_width(250.0)
                            .hint_text("a,b,c"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Bins on the diagonal:");
                    ui.add(egui::DragValue::new(&mut self.matrix_bins).range(3..=30))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Up to six variables; rows must be complete.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_matrix_dialog = open && !run;
        if run {
            self.run_matrix_dialog();
        }
    }

    /// Opens the interval plot dialog.
    pub fn open_interval_dialog(&mut self) {
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
        self.interval_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.interval_factor = self
            .data
            .as_ref()
            .and_then(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .find(|name| *name != self.interval_response)
            })
            .unwrap_or_default();
        self.interval_confidence = 95.0;
        self.show_interval_dialog = true;
    }

    /// Runs the interval plot of the dialog.
    pub fn run_interval_dialog(&mut self) {
        if self.interval_response.is_empty() || self.interval_factor.is_empty() {
            self.set_error("Select the response and the factor columns");
            return;
        }
        if self.interval_response == self.interval_factor {
            self.set_error("The response and the factor must be different columns");
            return;
        }
        let spec = AnalysisSpec::interval_plot(
            self.interval_response.clone(),
            self.interval_factor.clone(),
        )
        .with_interval_confidence((self.interval_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!("Interval Plot: {}", self.interval_response);
        self.execute_spec(spec, label);
        self.show_interval_dialog = false;
    }

    fn interval_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_interval_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_interval_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Interval Plot"))
            .open(&mut open)
            .resizable(false)
            .default_width(450.0)
            .show(ctx, |ui| {
                for (title, value) in [
                    ("Response:", &mut self.interval_response),
                    ("Factor:", &mut self.interval_factor),
                ] {
                    ui.horizontal(|ui| {
                        let label = ui.label(title);
                        egui::ComboBox::from_id_salt(title)
                            .selected_text(if value.is_empty() {
                                "—".to_owned()
                            } else {
                                value.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(value, name.clone(), name.as_str());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.interval_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Level means with their Student-t intervals and the grand mean.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_interval_dialog = open && !run;
        if run {
            self.run_interval_dialog();
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

    fn sheet_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_sheet_dialog {
            return;
        }
        let mut open = self.show_sheet_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Choose Worksheet"))
            .without_clear()
            .open(&mut open)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.help_label("The workbook has several worksheets:");
                for name in &self.sheet_names {
                    ui.selectable_value(&mut self.sheet_choice, name.clone(), name.as_str());
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Open")).clicked() {
                    run = true;
                }
            });
        self.show_sheet_dialog = open && !run;
        if run {
            self.open_request = Some(OpenRequest {
                path: self.sheet_path.clone(),
                kind: FileKind::Xlsx,
                sheet: Some(self.sheet_choice.clone()),
                csv_options: None,
            });
            self.show_sheet_dialog = false;
        }
    }

    /// Opens the column calculator.
    pub fn open_calc_dialog(&mut self) {
        self.calc_name = format!(
            "C{}",
            self.data.as_ref().map_or(1, DataFrame::n_columns) + 1
        );
        self.calc_expression.clear();
        self.show_calc_dialog = true;
    }

    /// Adds the calculated column to the worksheet.
    pub fn run_calc_dialog(&mut self) {
        let Some(mut df) = self.data.clone() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        if self.calc_name.trim().is_empty() {
            self.set_error("Name the new column");
            return;
        }
        if self.calc_expression.trim().is_empty() {
            self.set_error("Write the expression");
            return;
        }
        match fp_core::calc::add_column(&mut df, self.calc_name.trim(), &self.calc_expression) {
            Ok(()) => {
                let name = self.calc_name.trim().to_owned();
                self.set_data(df, "Calculated column".to_owned());
                self.append_session(&format!(
                    "Column {name} = {} calculated.\n",
                    self.calc_expression.trim()
                ));
                self.status = format!("Column {name} added");
                self.show_calc_dialog = false;
            }
            Err(error) => self.set_error(error.to_string()),
        }
    }

    fn calc_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_calc_dialog {
            return;
        }
        let mut open = self.show_calc_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Calculator"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("New column:");
                    ui.add(egui::TextEdit::singleline(&mut self.calc_name).desired_width(140.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Expression:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.calc_expression)
                            .desired_width(250.0)
                            .hint_text("[weight] / ([height] ^ 2)"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "Columns in brackets or by name; + - * / ^, parentheses, sqrt, ln, log10, \
                     exp, abs, round, floor, ceil, min, max, pow.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_calc_dialog = open && !run;
        if run {
            self.run_calc_dialog();
        }
    }

    /// Opens the Pareto chart dialog.
    pub fn open_pareto_dialog(&mut self) {
        self.pareto_column = self.selected.clone().unwrap_or_default();
        self.show_pareto_dialog = true;
    }

    /// Runs the Pareto chart of the dialog.
    pub fn run_pareto_dialog(&mut self) {
        if self.pareto_column.is_empty() {
            self.set_error("Select the column to count");
            return;
        }
        let spec = AnalysisSpec::pareto(self.pareto_column.clone());
        self.execute_spec(spec, format!("Pareto: {}", self.pareto_column));
        self.show_pareto_dialog = false;
    }

    fn pareto_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_pareto_dialog {
            return;
        }
        let names: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_pareto_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Pareto Chart"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("pareto_column")
                        .selected_text(if self.pareto_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.pareto_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.pareto_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("The categories are counted and sorted by frequency.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_pareto_dialog = open && !run;
        if run {
            self.run_pareto_dialog();
        }
    }

    /// Opens the ordinal logistic regression dialog.
    pub fn open_ordinal_dialog(&mut self) {
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
        self.ordinal_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.ordinal_predictors = numeric
            .iter()
            .filter(|name| **name != self.ordinal_response)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        self.show_ordinal_dialog = true;
    }

    /// Runs the ordinal regression of the dialog.
    pub fn run_ordinal_dialog(&mut self) {
        if self.ordinal_response.is_empty() {
            self.set_error("Select the ordered response column");
            return;
        }
        let predictors: Vec<String> = self
            .ordinal_predictors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if predictors.is_empty() {
            self.set_error("Enter at least one predictor");
            return;
        }
        if predictors.contains(&self.ordinal_response) {
            self.set_error("The response cannot also be a predictor");
            return;
        }
        let spec = AnalysisSpec::ordinal(self.ordinal_response.clone(), predictors);
        let label = format!("Ordinal Regression: {}", self.ordinal_response);
        self.execute_spec(spec, label);
        self.show_ordinal_dialog = false;
    }

    fn ordinal_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_ordinal_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_ordinal_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Ordinal Logistic Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(470.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response (codes):");
                    egui::ComboBox::from_id_salt("ordinal_response")
                        .selected_text(if self.ordinal_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.ordinal_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.ordinal_response,
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
                    let label = ui.help_label("Predictors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.ordinal_predictors)
                            .desired_width(240.0)
                            .hint_text("x1,x2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "The response needs at least three ordered integer codes; the model uses \
                     proportional odds.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_ordinal_dialog = open && !run;
        if run {
            self.run_ordinal_dialog();
        }
    }

    /// Opens the Poisson regression dialog.
    pub fn open_poisson_dialog(&mut self) {
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
        self.poisson_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.poisson_predictors = numeric
            .iter()
            .filter(|name| **name != self.poisson_response)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        self.show_poisson_dialog = true;
    }

    /// Runs the Poisson regression of the dialog.
    pub fn run_poisson_dialog(&mut self) {
        if self.poisson_response.is_empty() {
            self.set_error("Select the count response column");
            return;
        }
        let predictors: Vec<String> = self
            .poisson_predictors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if predictors.is_empty() {
            self.set_error("Enter at least one predictor");
            return;
        }
        if predictors.contains(&self.poisson_response) {
            self.set_error("The response cannot also be a predictor");
            return;
        }
        let spec = AnalysisSpec::poisson(self.poisson_response.clone(), predictors);
        let label = format!("Poisson Regression: {}", self.poisson_response);
        self.execute_spec(spec, label);
        self.show_poisson_dialog = false;
    }

    fn poisson_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_poisson_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_poisson_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Poisson Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response (counts):");
                    egui::ComboBox::from_id_salt("poisson_response")
                        .selected_text(if self.poisson_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.poisson_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.poisson_response,
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
                    let label = ui.help_label("Predictors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.poisson_predictors)
                            .desired_width(240.0)
                            .hint_text("x1,x2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Comma-separated numeric predictors; the response must be a count.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_poisson_dialog = open && !run;
        if run {
            self.run_poisson_dialog();
        }
    }

    /// Opens the binary logistic regression dialog.
    pub fn open_logistic_dialog(&mut self) {
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
        self.logistic_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.logistic_predictors = numeric
            .iter()
            .filter(|name| **name != self.logistic_response)
            .cloned()
            .collect::<Vec<_>>()
            .join(",");
        self.show_logistic_dialog = true;
    }

    /// Runs the logistic regression of the dialog.
    pub fn run_logistic_dialog(&mut self) {
        if self.logistic_response.is_empty() {
            self.set_error("Select the binary response column");
            return;
        }
        let predictors: Vec<String> = self
            .logistic_predictors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if predictors.is_empty() {
            self.set_error("Enter at least one predictor");
            return;
        }
        if predictors.contains(&self.logistic_response) {
            self.set_error("The response cannot also be a predictor");
            return;
        }
        let spec = AnalysisSpec::logistic(self.logistic_response.clone(), predictors);
        let label = format!("Logistic Regression: {}", self.logistic_response);
        self.execute_spec(spec, label);
        self.show_logistic_dialog = false;
    }

    fn logistic_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_logistic_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_logistic_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Binary Logistic Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response (0/1):");
                    egui::ComboBox::from_id_salt("logistic_response")
                        .selected_text(if self.logistic_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.logistic_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.logistic_response,
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
                    let label = ui.help_label("Predictors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.logistic_predictors)
                            .desired_width(240.0)
                            .hint_text("x1,x2"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "Comma-separated predictors; the response must be 0 or 1. Text, boolean \
                     and date predictors are dummy coded with the first level as the reference.",
                );
                let columns: Vec<String> = self
                    .data
                    .as_ref()
                    .map(|df| {
                        df.columns()
                            .iter()
                            .map(|column| column.name().to_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                if !columns.is_empty() {
                    ui.small(format!("Columns: {}", columns.join(", ")));
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_logistic_dialog = open && !run;
        if run {
            self.run_logistic_dialog();
        }
    }

    /// Opens the Gage linearity and bias study dialog.
    pub fn open_linearity_dialog(&mut self) {
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
        self.linearity_measurement = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric.first().cloned().unwrap_or_default());
        self.linearity_reference = numeric
            .iter()
            .find(|name| **name != self.linearity_measurement)
            .cloned()
            .unwrap_or_default();
        self.show_linearity_dialog = true;
    }

    /// Runs the linearity study of the dialog.
    pub fn run_linearity_dialog(&mut self) {
        if self.linearity_measurement.is_empty() || self.linearity_reference.is_empty() {
            self.set_error("Select the measurement and the reference columns");
            return;
        }
        if self.linearity_measurement == self.linearity_reference {
            self.set_error("The measurement and the reference must be different columns");
            return;
        }
        let spec = AnalysisSpec::gage_linearity(
            self.linearity_measurement.clone(),
            self.linearity_reference.clone(),
        );
        let label = format!("Gage Linearity: {}", self.linearity_measurement);
        self.execute_spec(spec, label);
        self.show_linearity_dialog = false;
    }

    fn linearity_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_linearity_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_linearity_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Gage Linearity and Bias"))
            .open(&mut open)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Measurement:");
                    egui::ComboBox::from_id_salt("linearity_measurement")
                        .selected_text(if self.linearity_measurement.is_empty() {
                            "—".to_owned()
                        } else {
                            self.linearity_measurement.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.linearity_measurement,
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
                    let label = ui.help_label("Reference:");
                    egui::ComboBox::from_id_salt("linearity_reference")
                        .selected_text(if self.linearity_reference.is_empty() {
                            "—".to_owned()
                        } else {
                            self.linearity_reference.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.linearity_reference,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "Reference values must contain at least three distinct parts \
                     with repeated measurements.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_linearity_dialog = open && !run;
        if run {
            self.run_linearity_dialog();
        }
    }

    fn tolerance_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_tolerance_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_tolerance_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Tolerance Interval"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("tolerance_column")
                        .selected_text(if self.tolerance_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.tolerance_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.tolerance_column,
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
                    let label = ui.help_label("Proportion of the population:");
                    ui.add(
                        egui::DragValue::new(&mut self.tolerance_proportion)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.tolerance_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_label("Method:");
                    ui.selectable_value(&mut self.tolerance_free, false, "Normal");
                    ui.selectable_value(&mut self.tolerance_free, true, "Distribution-free");
                });
                ui.small(if self.tolerance_free {
                    "Order statistics: no assumption about the shape of the population, but \
                         it needs a larger sample (93 observations for 95 %/95 %)."
                } else {
                    "The interval covers the chosen proportion of the population with the \
                         chosen confidence; the population must be normal."
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_tolerance_dialog = open && !run;
        if run {
            self.run_tolerance_dialog();
        }
    }

    /// Opens the dialog for the Box–Cox transformation.
    pub fn open_box_cox_dialog(&mut self) {
        self.box_cox_column = self.selected.clone().unwrap_or_default();
        self.show_box_cox_dialog = true;
    }

    /// Builds and runs the Box–Cox transformation of the dialog.
    pub fn run_box_cox_dialog(&mut self) {
        if self.box_cox_column.is_empty() {
            self.set_error("Select the column to transform");
            return;
        }
        let spec = AnalysisSpec::box_cox(self.box_cox_column.clone());
        let label = format!("Box-Cox Transformation: {}", self.box_cox_column);
        self.execute_spec(spec, label);
        self.show_box_cox_dialog = false;
    }

    fn box_cox_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_box_cox_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_box_cox_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Box-Cox Transformation"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("box_cox_column")
                        .selected_text(if self.box_cox_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.box_cox_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.box_cox_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "The transformation needs positive values; the report compares the \
                     normality before and after.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_box_cox_dialog = open && !run;
        if run {
            self.run_box_cox_dialog();
        }
    }

    /// Opens the dialog for a goodness-of-fit or outlier test.
    pub fn open_diagnostic_dialog(&mut self, kind: DiagnosticKind) {
        self.diagnostic_kind = kind;
        self.diagnostic_column = self.selected.clone().unwrap_or_default();
        self.diagnostic_expected = String::new();
        self.diagnostic_parameters = 0;
        self.diagnostic_alpha = 5.0;
        self.show_diagnostic_dialog = true;
    }

    /// Builds and runs the diagnostic test selected in the dialog.
    pub fn run_diagnostic_dialog(&mut self) {
        if self.diagnostic_column.is_empty() {
            self.set_error("Select the numeric column");
            return;
        }
        let spec = match self.diagnostic_kind {
            DiagnosticKind::ChiSquareGoodnessOfFit => {
                let expected: Vec<f64> = self
                    .diagnostic_expected
                    .split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(|part| part.parse::<f64>().unwrap_or(f64::NAN))
                    .collect();
                if expected.len() < 2 {
                    self.set_error("Enter the expected counts as a comma-separated list");
                    return;
                }
                if expected.iter().any(|value| !value.is_finite()) {
                    self.set_error("The expected counts must be numbers");
                    return;
                }
                AnalysisSpec::chi_square_goodness_of_fit(self.diagnostic_column.clone(), expected)
                    .with_estimated_parameters(self.diagnostic_parameters)
            }
            DiagnosticKind::Grubbs => AnalysisSpec::grubbs(self.diagnostic_column.clone())
                .with_grubbs_alpha((self.diagnostic_alpha / 100.0).clamp(0.001, 0.5)),
        };
        let label = format!(
            "{}: {}",
            self.diagnostic_kind.name(),
            self.diagnostic_column
        );
        self.execute_spec(spec, label);
        self.show_diagnostic_dialog = false;
    }

    fn diagnostic_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_diagnostic_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_diagnostic_dialog;
        let mut run = false;
        dialog_window(self.diagnostic_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("diagnostic_column")
                        .selected_text(if self.diagnostic_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.diagnostic_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.diagnostic_column,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.diagnostic_kind == DiagnosticKind::ChiSquareGoodnessOfFit {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Expected counts:");
                        ui.text_edit_singleline(&mut self.diagnostic_expected)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Estimated parameters:");
                        ui.add(
                            egui::DragValue::new(&mut self.diagnostic_parameters)
                                .range(0..=10)
                                .speed(0.5),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                    ui.small(
                        "The column holds one observed count per cell; the expected counts are \
                         comma separated.",
                    );
                } else {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Significance:");
                        ui.add(
                            egui::DragValue::new(&mut self.diagnostic_alpha)
                                .range(0.1..=50.0)
                                .speed(0.5)
                                .suffix(" %"),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                    ui.small("The test looks for a single outlier in the column.");
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_diagnostic_dialog = open && !run;
        if run {
            self.run_diagnostic_dialog();
        }
    }

    /// Opens the dialog for a power and sample-size analysis.
    pub fn open_power_dialog(&mut self) {
        self.power_procedure = PowerProcedure::OneSampleT;
        self.power_effect = "0.5".to_owned();
        self.power_null = "0.5".to_owned();
        self.power_n = "0".to_owned();
        self.power_alpha = 5.0;
        self.power_target = 80.0;
        self.power_alternative = Alternative::TwoSided;
        self.power_groups = 2;
        self.show_power_dialog = true;
    }

    /// Builds and runs the power analysis selected in the dialog.
    pub fn run_power_dialog(&mut self) {
        let Ok(effect) = self.power_effect.trim().parse::<f64>() else {
            self.set_error("The effect size must be a number");
            return;
        };
        let Ok(null_value) = self.power_null.trim().parse::<f64>() else {
            self.set_error("The null proportion must be a number");
            return;
        };
        let Ok(n) = self.power_n.trim().parse::<usize>() else {
            self.set_error("The sample size must be a whole number (0 solves for it)");
            return;
        };
        let spec = AnalysisSpec::power(self.power_procedure, effect, n)
            .with_null_proportion(null_value)
            .with_power((self.power_target / 100.0).clamp(0.001, 0.999))
            .with_power_alpha((self.power_alpha / 100.0).clamp(0.0001, 0.5))
            .with_power_groups(self.power_groups.max(2))
            .with_alternative(self.power_alternative);
        let label = format!("Power and Sample Size: {}", self.power_procedure.name());
        self.execute_spec(spec, label);
        self.show_power_dialog = false;
    }

    fn power_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_power_dialog {
            return;
        }
        let mut open = self.show_power_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Power and Sample Size"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Test:");
                    egui::ComboBox::from_id_salt("power_procedure")
                        .selected_text(self.power_procedure.name())
                        .show_ui(ui, |ui| {
                            for procedure in [
                                PowerProcedure::OneSampleT,
                                PowerProcedure::TwoSampleT,
                                PowerProcedure::Proportion,
                                PowerProcedure::Correlation,
                                PowerProcedure::Anova,
                                PowerProcedure::Regression,
                            ] {
                                ui.selectable_value(
                                    &mut self.power_procedure,
                                    procedure,
                                    procedure.name(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.label(match self.power_procedure {
                        PowerProcedure::Proportion => "True proportion:",
                        PowerProcedure::Correlation => "Correlation:",
                        PowerProcedure::Anova => "Standardized difference:",
                        PowerProcedure::Regression => "Population R²:",
                        _ => "Effect size:",
                    });
                    ui.text_edit_singleline(&mut self.power_effect)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.power_procedure == PowerProcedure::Proportion {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Null proportion:");
                        ui.text_edit_singleline(&mut self.power_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if matches!(
                    self.power_procedure,
                    PowerProcedure::Anova | PowerProcedure::Regression
                ) {
                    ui.horizontal(|ui| {
                        let label = ui.label(if self.power_procedure == PowerProcedure::Anova {
                            "Groups:"
                        } else {
                            "Predictors:"
                        });
                        ui.add(
                            egui::DragValue::new(&mut self.power_groups)
                                .range(2..=20)
                                .speed(0.5),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                }
                if self.power_procedure != PowerProcedure::Correlation {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Sample size:");
                        ui.text_edit_singleline(&mut self.power_n)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Alternative:");
                        egui::ComboBox::from_id_salt("power_alternative")
                            .selected_text(self.power_alternative.as_str())
                            .show_ui(ui, |ui| {
                                for alternative in [
                                    Alternative::TwoSided,
                                    Alternative::Less,
                                    Alternative::Greater,
                                ] {
                                    ui.selectable_value(
                                        &mut self.power_alternative,
                                        alternative,
                                        alternative.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Significance:");
                    ui.add(
                        egui::DragValue::new(&mut self.power_alpha)
                            .range(0.1..=20.0)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Power:");
                    ui.add(
                        egui::DragValue::new(&mut self.power_target)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "A sample size of zero asks for the size that reaches the requested power; \
                     otherwise the power of that size is reported.",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_power_dialog = open && !run;
        if run {
            self.run_power_dialog();
        }
    }

    /// Opens the dialog for a proportion or variance test.
    pub fn open_test2_dialog(&mut self, kind: Test2Kind) {
        let Some(df) = self.data.as_ref() else {
            self.set_error("No worksheet open: use File → Open CSV…");
            return;
        };
        let names: Vec<String> = df
            .columns()
            .iter()
            .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
            .map(|column| column.name().to_owned())
            .collect();
        if names.is_empty() {
            self.set_error("No numeric column available");
            return;
        }
        self.test2_kind = kind;
        self.test2_first = self
            .selected
            .clone()
            .filter(|name| names.contains(name))
            .unwrap_or_else(|| names[0].clone());
        self.test2_second = names
            .iter()
            .find(|name| **name != self.test2_first)
            .cloned()
            .unwrap_or_default();
        self.test2_null = match kind {
            Test2Kind::OneProportion => "0.5".to_owned(),
            Test2Kind::OneVariance => "1".to_owned(),
            _ => String::new(),
        };
        self.test2_success = "1".to_owned();
        self.test2_method = ProportionMethod::Exact;
        self.test2_alternative = Alternative::TwoSided;
        self.test2_confidence = 95.0;
        self.reg_response = self.selected.clone().unwrap_or_default();
        self.reg_predictors.clear();
        self.reg_confidence = 95.0;
        self.step_response = self.selected.clone().unwrap_or_default();
        self.step_candidates.clear();
        self.step_method = StepMethod::Stepwise;
        self.step_criterion = StepCriterion::PValue;
        self.step_alpha_enter = 0.15;
        self.step_alpha_remove = 0.15;
        self.step_max_steps = 50;
        self.step_confidence = 95.0;
        self.anova_response = self.selected.clone().unwrap_or_default();
        self.anova_factor = String::new();
        self.anova_confidence = 95.0;
        self.km_time = self.selected.clone().unwrap_or_default();
        self.km_censor = None;
        self.km_censor_value = "0".to_owned();
        self.km_confidence = 95.0;
        self.weibull_time = self.selected.clone().unwrap_or_default();
        self.weibull_censor = None;
        self.weibull_censor_value = "0".to_owned();
        self.exp_time = self.selected.clone().unwrap_or_default();
        self.exp_censor = None;
        self.exp_censor_value = "0".to_owned();
        self.log_time = self.selected.clone().unwrap_or_default();
        self.log_censor = None;
        self.log_censor_value = "0".to_owned();
        self.gage_measurement = self.selected.clone().unwrap_or_default();
        self.gage_part = String::new();
        self.gage_operator = String::new();
        self.gage_design = GageDesign::Crossed;
        self.type1_measurement = self.selected.clone().unwrap_or_default();
        self.type1_reference = "0".to_owned();
        self.type1_lsl.clear();
        self.type1_usl.clear();
        self.doe_response = self.selected.clone().unwrap_or_default();
        self.doe_factors.clear();
        self.doe_block.clear();
        self.doe_main_effects = false;
        self.rsm_response = self.selected.clone().unwrap_or_default();
        self.rsm_factors.clear();
        self.rsm_optimize = false;
        self.rsm_contours = false;
        self.rsm_steepest_ascent = false;
        self.scatter_x = self.selected.clone().unwrap_or_default();
        self.scatter_y = String::new();
        self.scatter_fit = true;
        self.design_factors = "A,B".to_owned();
        self.design_replicates = 2;
        self.design_centers = 0;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_test2_dialog = true;
    }

    /// Builds and runs the proportion or variance test selected in the dialog.
    pub fn run_test2_dialog(&mut self) {
        if self.test2_first.is_empty() {
            self.set_error("Select the first column");
            return;
        }
        let proportion = matches!(
            self.test2_kind,
            Test2Kind::OneProportion | Test2Kind::TwoProportions
        );
        let success = if proportion {
            match self.test2_success.trim().parse::<f64>() {
                Ok(value) if value == 0.0 || value == 1.0 => value,
                _ => {
                    self.set_error("Success value must be 0 or 1");
                    return;
                }
            }
        } else {
            1.0
        };
        let spec = match self.test2_kind {
            Test2Kind::OneProportion => {
                let Ok(null_p) = self.test2_null.trim().parse::<f64>() else {
                    self.set_error("Null proportion must be a number");
                    return;
                };
                AnalysisSpec::one_proportion(self.test2_first.clone(), null_p)
                    .with_success(success)
                    .with_proportion_method(self.test2_method)
            }
            Test2Kind::TwoProportions => {
                if self.test2_second.is_empty() || self.test2_second == self.test2_first {
                    self.set_error("Select two different columns");
                    return;
                }
                AnalysisSpec::two_proportions(self.test2_first.clone(), self.test2_second.clone())
                    .with_success(success)
            }
            Test2Kind::OneVariance => {
                let Ok(null_variance) = self.test2_null.trim().parse::<f64>() else {
                    self.set_error("Null variance must be a number");
                    return;
                };
                AnalysisSpec::one_variance(self.test2_first.clone(), null_variance)
            }
            Test2Kind::TwoVariances => {
                if self.test2_second.is_empty() || self.test2_second == self.test2_first {
                    self.set_error("Select two different columns");
                    return;
                }
                AnalysisSpec::two_variances(self.test2_first.clone(), self.test2_second.clone())
                    .with_variance_method(self.test2_variance_method)
            }
        }
        .with_alternative(self.test2_alternative)
        .with_test_confidence((self.test2_confidence / 100.0).clamp(0.5, 0.999));
        let label = format!(
            "{}: {} / {}",
            self.test2_kind.name(),
            self.test2_first,
            self.test2_second
        );
        self.execute_spec(spec, label);
        self.show_test2_dialog = false;
    }

    fn test2_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_test2_dialog {
            return;
        }
        let names: Vec<String> = self
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
        let mut open = self.show_test2_dialog;
        let mut run = false;
        dialog_window(self.test2_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(410.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("First column:");
                    egui::ComboBox::from_id_salt("test2_first")
                        .selected_text(if self.test2_first.is_empty() {
                            "—".to_owned()
                        } else {
                            self.test2_first.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &names {
                                ui.selectable_value(
                                    &mut self.test2_first,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                if self.test2_kind.needs_second() {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Second column:");
                        egui::ComboBox::from_id_salt("test2_second")
                            .selected_text(if self.test2_second.is_empty() {
                                "—".to_owned()
                            } else {
                                self.test2_second.clone()
                            })
                            .show_ui(ui, |ui| {
                                for name in &names {
                                    ui.selectable_value(
                                        &mut self.test2_second,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                let proportion = matches!(
                    self.test2_kind,
                    Test2Kind::OneProportion | Test2Kind::TwoProportions
                );
                if proportion {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Success value (0/1):");
                        ui.text_edit_singleline(&mut self.test2_success)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.small("The selected columns must contain only 0 and 1.");
                }
                if matches!(self.test2_kind, Test2Kind::OneProportion) {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Null proportion:");
                        ui.text_edit_singleline(&mut self.test2_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Method:");
                        egui::ComboBox::from_id_salt("test2_method")
                            .selected_text(self.test2_method.as_str())
                            .show_ui(ui, |ui| {
                                for method in [ProportionMethod::Exact, ProportionMethod::Normal] {
                                    ui.selectable_value(
                                        &mut self.test2_method,
                                        method,
                                        method.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if matches!(self.test2_kind, Test2Kind::OneVariance) {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Null variance:");
                        ui.text_edit_singleline(&mut self.test2_null)
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                if matches!(self.test2_kind, Test2Kind::TwoVariances) {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Method:");
                        egui::ComboBox::from_id_salt("test2_variance_method")
                            .selected_text(self.test2_variance_method.name())
                            .show_ui(ui, |ui| {
                                for method in [VarianceMethod::F, VarianceMethod::Bonett] {
                                    ui.selectable_value(
                                        &mut self.test2_variance_method,
                                        method,
                                        method.name(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Alternative:");
                    egui::ComboBox::from_id_salt("test2_alternative")
                        .selected_text(self.test2_alternative.as_str())
                        .show_ui(ui, |ui| {
                            for alternative in [
                                Alternative::TwoSided,
                                Alternative::Less,
                                Alternative::Greater,
                            ] {
                                ui.selectable_value(
                                    &mut self.test2_alternative,
                                    alternative,
                                    alternative.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.test2_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(match self.test2_kind {
                    Test2Kind::OneProportion => {
                        "Exact binomial (Clopper–Pearson) or normal approximation (Wilson)."
                    }
                    Test2Kind::TwoProportions => {
                        "H₀: p₁ = p₂; pooled normal approximation, unpooled interval."
                    }
                    Test2Kind::OneVariance => {
                        "H₀: σ² = null variance; chi-square test with n − 1 df."
                    }
                    Test2Kind::TwoVariances => {
                        "H₀: σ₁² = σ₂²; F test (n₁−1, n₂−1 df) or Bonett's kurtosis-adjusted test."
                    }
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_test2_dialog = open && !run;
        if run {
            self.run_test2_dialog();
        }
    }

    /// Opens the regression dialog.
    pub fn open_regression_dialog(&mut self) {
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
        let all: Vec<String> = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .collect();
        if numeric.is_empty() || all.len() < 2 {
            self.set_error("Regression needs a numeric response and at least one predictor");
            return;
        }
        self.reg_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.reg_predictors = all
            .into_iter()
            .filter(|name| *name != self.reg_response)
            .collect();
        self.reg_confidence = 95.0;
        self.show_reg_dialog = true;
    }

    /// Fits the regression selected in the dialog.
    pub fn run_regression_dialog(&mut self) {
        if self.reg_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let predictors: Vec<String> = self
            .reg_predictors
            .iter()
            .filter(|name| **name != self.reg_response)
            .cloned()
            .collect();
        if predictors.is_empty() {
            self.set_error("Select at least one predictor");
            return;
        }
        let spec = AnalysisSpec::regression(self.reg_response.clone(), predictors)
            .with_regression_confidence((self.reg_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, format!("Regression: {}", self.reg_response));
        self.show_reg_dialog = false;
    }

    fn regression_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_reg_dialog {
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
        let all: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_reg_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("reg_response")
                        .selected_text(if self.reg_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.reg_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.reg_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.help_label("Predictors:");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for name in &all {
                            if *name == self.reg_response {
                                continue;
                            }
                            let mut selected = self.reg_predictors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.reg_predictors.push(name.clone());
                                } else {
                                    self.reg_predictors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.reg_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id).hover_from(&label);
                });
                ui.small("Ordinary least squares with intercept; categorical predictors use dummy coding.");
                ui.small("Rows missing the response or any selected predictor are removed.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_reg_dialog = open && !run;
        if run {
            self.run_regression_dialog();
        }
    }

    /// Opens the stepwise regression dialog.
    pub fn open_stepwise_dialog(&mut self) {
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
        if numeric.is_empty() || numeric.len() < 2 {
            self.set_error("Stepwise regression needs a numeric response and at least one other numeric column");
            return;
        }
        self.step_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.step_candidates = numeric
            .into_iter()
            .filter(|name| *name != self.step_response)
            .collect();
        self.step_method = StepMethod::Stepwise;
        self.step_criterion = StepCriterion::PValue;
        self.step_alpha_enter = 0.15;
        self.step_alpha_remove = 0.15;
        self.step_max_steps = 50;
        self.step_confidence = 95.0;
        self.show_step_dialog = true;
    }

    /// Runs the stepwise regression selected in the dialog.
    pub fn run_stepwise_dialog(&mut self) {
        if self.step_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let candidates: Vec<String> = self
            .step_candidates
            .iter()
            .filter(|name| **name != self.step_response)
            .cloned()
            .collect();
        if candidates.is_empty() {
            self.set_error("Select at least one candidate predictor");
            return;
        }
        let spec = AnalysisSpec::stepwise_regression(self.step_response.clone(), candidates)
            .with_stepwise_method(self.step_method)
            .with_stepwise_criterion(self.step_criterion)
            .with_stepwise_alphas(self.step_alpha_enter, self.step_alpha_remove)
            .with_stepwise_max_steps(self.step_max_steps)
            .with_regression_confidence((self.step_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, format!("Stepwise Regression: {}", self.step_response));
        self.show_step_dialog = false;
    }

    fn stepwise_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_step_dialog {
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
        let mut open = self.show_step_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Stepwise Regression"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("step_response")
                        .selected_text(if self.step_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.step_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.step_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_label("Candidate predictors:");
                egui::ScrollArea::vertical()
                    .max_height(150.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            if *name == self.step_response {
                                continue;
                            }
                            let mut selected = self.step_candidates.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.step_candidates.push(name.clone());
                                } else {
                                    self.step_candidates.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    ui.help_label("Method:");
                    ui.radio_value(&mut self.step_method, StepMethod::Forward, "Forward")
                        .on_hover_text(checkbox_help("Forward"));
                    ui.radio_value(&mut self.step_method, StepMethod::Backward, "Backward")
                        .on_hover_text(checkbox_help("Backward"));
                    ui.radio_value(&mut self.step_method, StepMethod::Stepwise, "Stepwise")
                        .on_hover_text(checkbox_help("Stepwise"));
                });
                ui.horizontal(|ui| {
                    ui.help_label("Criterion:");
                    ui.radio_value(&mut self.step_criterion, StepCriterion::PValue, "p-value")
                        .on_hover_text(checkbox_help("p-value"));
                    ui.radio_value(&mut self.step_criterion, StepCriterion::Aic, "AIC")
                        .on_hover_text(checkbox_help("AIC"));
                    ui.radio_value(&mut self.step_criterion, StepCriterion::Bic, "BIC")
                        .on_hover_text(checkbox_help("BIC"));
                });
                ui.add_enabled_ui(self.step_criterion == StepCriterion::PValue, |ui| {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Entry alpha:");
                        ui.add(
                            egui::DragValue::new(&mut self.step_alpha_enter)
                                .range(0.001..=0.5)
                                .speed(0.01)
                                .max_decimals(3),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Removal alpha:");
                        ui.add(
                            egui::DragValue::new(&mut self.step_alpha_remove)
                                .range(0.001..=0.5)
                                .speed(0.01)
                                .max_decimals(3),
                        )
                        .labelled_by(label.id)
                        .hover_from(&label);
                    });
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Max steps:");
                    ui.add(
                        egui::DragValue::new(&mut self.step_max_steps)
                            .range(1..=500)
                            .speed(1.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.step_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small(
                    "Numeric candidates only; every candidate model uses the same complete rows.",
                );
                ui.small("Selected coefficients are ordinary least squares on the final model.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_step_dialog = open && !run;
        if run {
            self.run_stepwise_dialog();
        }
    }

    /// Opens the one-way ANOVA dialog.
    pub fn open_anova_dialog(&mut self) {
        self.open_one_way_dialog(OneWayKind::Anova);
    }

    /// Opens the one-way analysis dialog for the chosen test.
    pub fn open_one_way_dialog(&mut self, kind: OneWayKind) {
        self.one_way_kind = kind;
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
        if numeric.is_empty() {
            self.set_error("ANOVA needs a numeric response");
            return;
        }
        self.anova_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.anova_factor = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .find(|name| *name != self.anova_response)
            .unwrap_or_default();
        self.anova_confidence = 95.0;
        self.show_anova_dialog = true;
    }

    /// Runs the one-way ANOVA selected in the dialog.
    pub fn run_anova_dialog(&mut self) {
        if self.anova_response.is_empty() || self.anova_factor.is_empty() {
            self.set_error("Select the response and factor columns");
            return;
        }
        if self.anova_response == self.anova_factor {
            self.set_error("The response and factor must be different");
            return;
        }
        let confidence = (self.anova_confidence / 100.0).clamp(0.5, 0.999);
        let spec = match self.one_way_kind {
            OneWayKind::Anova => {
                AnalysisSpec::anova(self.anova_response.clone(), self.anova_factor.clone())
                    .with_anova_confidence(confidence)
            }
            OneWayKind::Levene => {
                AnalysisSpec::levene(self.anova_response.clone(), self.anova_factor.clone())
                    .with_variance_center(self.variance_center)
                    .with_levene_confidence(confidence)
            }
            OneWayKind::Bartlett => {
                AnalysisSpec::bartlett(self.anova_response.clone(), self.anova_factor.clone())
            }
        };
        self.execute_spec(
            spec,
            format!(
                "{}: {} by {}",
                self.one_way_kind.name(),
                self.anova_response,
                self.anova_factor
            ),
        );
        self.show_anova_dialog = false;
    }

    fn anova_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_anova_dialog {
            return;
        }
        let all: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
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
        let mut open = self.show_anova_dialog;
        let mut run = false;
        dialog_window(self.one_way_kind.name())
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Test:");
                    egui::ComboBox::from_id_salt("one_way_kind")
                        .selected_text(self.one_way_kind.name())
                        .show_ui(ui, |ui| {
                            for kind in [OneWayKind::Anova, OneWayKind::Levene, OneWayKind::Bartlett] {
                                ui.selectable_value(&mut self.one_way_kind, kind, kind.name());
                            }
                        })
                        .response
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("anova_response")
                        .selected_text(if self.anova_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.anova_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.anova_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Factor:");
                    egui::ComboBox::from_id_salt("anova_factor")
                        .selected_text(if self.anova_factor.is_empty() {
                            "—".to_owned()
                        } else {
                            self.anova_factor.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.anova_factor,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id).hover_from(&label);
                });
                if self.one_way_kind == OneWayKind::Levene {
                    ui.horizontal(|ui| {
                        let label = ui.help_label("Centre:");
                        egui::ComboBox::from_id_salt("variance_center")
                            .selected_text(self.variance_center.as_str())
                            .show_ui(ui, |ui| {
                                for center in [VarianceCenter::Mean, VarianceCenter::Median] {
                                    ui.selectable_value(
                                        &mut self.variance_center,
                                        center,
                                        center.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id).hover_from(&label);
                    });
                }
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.anova_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id).hover_from(&label);
                });
                ui.small(match self.one_way_kind {
                    OneWayKind::Anova => {
                        "Groups need at least two observations; pairwise comparisons use Tukey HSD and Bonferroni."
                    }
                    OneWayKind::Levene => {
                        "The spread of each observation is measured against the centre of its \
                         group; the F test compares the groups."
                    }
                    OneWayKind::Bartlett => {
                        "Likelihood-ratio test of equal variances; sensitive to departures from \
                         normality."
                    }
                });
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_anova_dialog = open && !run;
        if run {
            self.run_anova_dialog();
        }
    }

    /// Opens the Kaplan–Meier dialog.
    pub fn open_kaplan_meier_dialog(&mut self) {
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
        if numeric.is_empty() {
            self.set_error("Kaplan–Meier needs a numeric time column");
            return;
        }
        self.km_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.km_censor = None;
        self.km_censor_value = "0".to_owned();
        self.km_confidence = 95.0;
        self.show_km_dialog = true;
    }

    /// Runs the Kaplan–Meier estimate selected in the dialog.
    pub fn run_kaplan_meier_dialog(&mut self) {
        if self.km_time.is_empty() {
            self.set_error("Select the time column");
            return;
        }
        let Ok(censor_value) = self.km_censor_value.trim().parse::<f64>() else {
            self.set_error("Censoring value must be a number");
            return;
        };
        let spec = AnalysisSpec::kaplan_meier(self.km_time.clone())
            .with_censoring(self.km_censor.clone(), censor_value)
            .with_reliability_confidence((self.km_confidence / 100.0).clamp(0.5, 0.999));
        self.execute_spec(spec, format!("Kaplan–Meier: {}", self.km_time));
        self.show_km_dialog = false;
    }

    fn kaplan_meier_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_km_dialog {
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
        let mut open = self.show_km_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Kaplan–Meier"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Time column:");
                    egui::ComboBox::from_id_salt("km_time")
                        .selected_text(if self.km_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.km_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(&mut self.km_time, name.clone(), name.as_str());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring column:");
                    egui::ComboBox::from_id_salt("km_censor")
                        .selected_text(self.km_censor.as_deref().unwrap_or("None (all events)"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.km_censor, None, "None (all events)");
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.km_censor,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.km_censor_value).desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.km_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Numeric censoring column: the configured value means right-censored.");
                ui.small("Greenwood standard error; normal interval clipped to [0, 1].");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_km_dialog = open && !run;
        if run {
            self.run_kaplan_meier_dialog();
        }
    }

    /// Opens the crossed Gage R&R dialog.
    pub fn open_gage_dialog(&mut self) {
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
        if numeric.is_empty() {
            self.set_error("Gage R&R needs a numeric measurement column");
            return;
        }
        self.gage_measurement = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.gage_part = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .find(|name| *name != self.gage_measurement)
            .unwrap_or_default();
        self.gage_operator = df
            .columns()
            .iter()
            .map(|column| column.name().to_owned())
            .find(|name| *name != self.gage_measurement && *name != self.gage_part)
            .unwrap_or_default();
        self.show_gage_dialog = true;
    }

    /// Runs the Gage R&R selected in the dialog.
    pub fn run_gage_dialog(&mut self) {
        if self.gage_measurement.is_empty()
            || self.gage_part.is_empty()
            || self.gage_operator.is_empty()
        {
            self.set_error("Select the measurement, part and operator columns");
            return;
        }
        if self.gage_measurement == self.gage_part
            || self.gage_measurement == self.gage_operator
            || self.gage_part == self.gage_operator
        {
            self.set_error("The three columns must be different");
            return;
        }
        let spec = AnalysisSpec::gage_rr(
            self.gage_measurement.clone(),
            self.gage_part.clone(),
            self.gage_operator.clone(),
        )
        .with_gage_design(self.gage_design);
        self.execute_spec(spec, format!("Gage R&R: {}", self.gage_measurement));
        self.show_gage_dialog = false;
    }

    fn gage_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_gage_dialog {
            return;
        }
        let all: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
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
        let mut open = self.show_gage_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Gage R&R"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Measurement:");
                    egui::ComboBox::from_id_salt("gage_measurement")
                        .selected_text(if self.gage_measurement.is_empty() {
                            "—".to_owned()
                        } else {
                            self.gage_measurement.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.gage_measurement,
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
                    let label = ui.help_label("Design:");
                    egui::ComboBox::from_id_salt("gage_design")
                        .selected_text(self.gage_design.name())
                        .show_ui(ui, |ui| {
                            for design in [GageDesign::Crossed, GageDesign::Nested] {
                                ui.selectable_value(&mut self.gage_design, design, design.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Part:");
                    egui::ComboBox::from_id_salt("gage_part")
                        .selected_text(if self.gage_part.is_empty() {
                            "—".to_owned()
                        } else {
                            self.gage_part.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.gage_part,
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
                    let label = ui.help_label("Operator:");
                    egui::ComboBox::from_id_salt("gage_operator")
                        .selected_text(if self.gage_operator.is_empty() {
                            "—".to_owned()
                        } else {
                            self.gage_operator.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &all {
                                ui.selectable_value(
                                    &mut self.gage_operator,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("Balanced crossed design: same replicates in every part×operator cell.");
                ui.small("Crossed ANOVA method; interaction included.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_gage_dialog = open && !run;
        if run {
            self.run_gage_dialog();
        }
    }

    /// Opens the two-level factorial dialog.
    pub fn open_doe_dialog(&mut self) {
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
            self.set_error("Factorial analysis needs a response and at least one factor");
            return;
        }
        self.doe_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.doe_factors = numeric
            .into_iter()
            .filter(|name| *name != self.doe_response)
            .collect();
        self.show_doe_dialog = true;
    }

    /// Runs the factorial analysis selected in the dialog.
    pub fn run_doe_dialog(&mut self) {
        if self.doe_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let factors: Vec<String> = self
            .doe_factors
            .iter()
            .filter(|name| **name != self.doe_response)
            .cloned()
            .collect();
        if factors.is_empty() {
            self.set_error("Select at least one factor");
            return;
        }
        let mut spec = AnalysisSpec::factorial(self.doe_response.clone(), factors)
            .with_main_effects(self.doe_main_effects);
        if !self.doe_block.is_empty() {
            spec = spec.with_block(self.doe_block.clone());
        }
        self.execute_spec(spec, format!("Factorial: {}", self.doe_response));
        self.show_doe_dialog = false;
    }

    fn doe_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_doe_dialog {
            return;
        }
        let (numeric, all_columns): (Vec<String>, Vec<String>) = self
            .data
            .as_ref()
            .map(|df| {
                let numeric = df
                    .columns()
                    .iter()
                    .filter(|column| matches!(column.dtype(), DType::Float | DType::Integer))
                    .map(|column| column.name().to_owned())
                    .collect();
                let all = df
                    .columns()
                    .iter()
                    .map(|column| column.name().to_owned())
                    .collect();
                (numeric, all)
            })
            .unwrap_or_default();
        let mut open = self.show_doe_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("2-Level Factorial"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("doe_response")
                        .selected_text(if self.doe_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.doe_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.doe_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_label("Factors (coded −1/+1):");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            if *name == self.doe_response {
                                continue;
                            }
                            let mut selected = self.doe_factors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.doe_factors.push(name.clone());
                                } else {
                                    self.doe_factors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Block:");
                    egui::ComboBox::from_id_salt("doe_block")
                        .selected_text(if self.doe_block.is_empty() {
                            "None".to_owned()
                        } else {
                            self.doe_block.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.doe_block, String::new(), "None");
                            for name in &all_columns {
                                ui.selectable_value(
                                    &mut self.doe_block,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("All main effects and interactions are fitted simultaneously.");
                ui.small(
                    "At least two residual degrees of freedom are required (replicate the design).",
                );
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_doe_dialog = open && !run;
        if run {
            self.run_doe_dialog();
        }
    }

    /// Opens the factorial-design generator.
    pub fn open_design_dialog(&mut self) {
        self.design_factors = "A,B".to_owned();
        self.design_replicates = 2;
        self.design_centers = 0;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_design_dialog = true;
    }

    /// Generates the design and replaces the worksheet.
    pub fn run_design_dialog(&mut self) {
        let factors: Vec<String> = self
            .design_factors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if factors.is_empty() {
            self.set_error("Enter at least one factor name");
            return;
        }
        let block_size = (self.design_block_size > 0).then_some(self.design_block_size);
        let df = match fp_core::doe::blocked_factorial_design(
            &factors,
            self.design_replicates,
            self.design_centers,
            block_size,
            self.design_randomize,
            self.design_seed,
        ) {
            Ok(df) => df,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let rows = df.n_rows();
        self.set_data(df, "DOE design".to_owned());
        self.append_session(&format!(
            "DOE design generated: {rows} runs, factors {}.\n",
            factors.join(", ")
        ));
        self.show_design_dialog = false;
    }

    /// Opens the Box–Behnken design generator.
    pub fn open_box_behnken_dialog(&mut self) {
        self.design_factors = "A,B,C".to_owned();
        self.design_centers = 3;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_bb_dialog = true;
    }

    /// Generates the Box–Behnken design and replaces the worksheet.
    pub fn run_box_behnken_dialog(&mut self) {
        let factors: Vec<String> = self
            .design_factors
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect();
        if factors.len() < 3 {
            self.set_error("A Box-Behnken design needs at least three factors");
            return;
        }
        let df = match fp_core::doe::box_behnken_design(
            &factors,
            self.design_centers,
            self.design_randomize,
            self.design_seed,
        ) {
            Ok(df) => df,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let rows = df.n_rows();
        self.set_data(df, "Box-Behnken design".to_owned());
        self.append_session(&format!(
            "Box-Behnken design generated: {rows} runs, factors {}.\n",
            factors.join(", ")
        ));
        self.show_bb_dialog = false;
    }

    /// Opens the Plackett–Burman design generator.
    pub fn open_pb_dialog(&mut self) {
        self.design_runs = 12;
        self.design_randomize = true;
        self.design_seed = 2026;
        self.show_pb_dialog = true;
    }

    /// Generates the Plackett–Burman design and replaces the worksheet.
    pub fn run_pb_dialog(&mut self) {
        let df = match fp_core::doe::plackett_burman_design(
            self.design_runs,
            self.design_randomize,
            self.design_seed,
        ) {
            Ok(df) => df,
            Err(error) => {
                self.set_error(error.to_string());
                return;
            }
        };
        let rows = df.n_rows();
        self.set_data(df, "Plackett-Burman design".to_owned());
        self.append_session(&format!(
            "Plackett-Burman design generated: {rows} runs, {} factors.\n",
            rows - 1
        ));
        self.show_pb_dialog = false;
    }

    fn pb_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_pb_dialog {
            return;
        }
        let mut open = self.show_pb_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Plackett-Burman Design"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Runs:");
                    ui.add(egui::DragValue::new(&mut self.design_runs).range(8..=48))
                        .labelled_by(label.id)
                        .hover_from(&label);
                    ui.small("8, 12, 20, 24 or 32");
                });
                ui.horizontal(|ui| {
                    ui.help_checkbox(&mut self.design_randomize, "Randomize run order");
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.design_seed).speed(1.0))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("A screening design with runs − 1 orthogonal factors.");
                ui.separator();
                if ui.help_button(self.catalog().label("Generate")).clicked() {
                    run = true;
                }
            });
        self.show_pb_dialog = open && !run;
        if run {
            self.run_pb_dialog();
        }
    }

    fn box_behnken_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_bb_dialog {
            return;
        }
        let mut open = self.show_bb_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Box-Behnken Design"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Factors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.design_factors)
                            .desired_width(220.0)
                            .hint_text("A,B,C"),
                    )
                    .labelled_by(label.id).hover_from(&label);
                });
                ui.small("Three to seven factors; every pair is measured at the four corners of a square.");
                ui.horizontal(|ui| {
                    let label = ui.help_label("Center points:");
                    ui.add(egui::DragValue::new(&mut self.design_centers).range(1..=50))
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.horizontal(|ui| {
                    ui.help_checkbox(&mut self.design_randomize, "Randomize run order");
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.design_seed).speed(1.0))
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.small("This replaces the current worksheet with StdOrder, RunOrder and factor columns.");
                ui.separator();
                if ui.help_button(self.catalog().label("Generate")).clicked() {
                    run = true;
                }
            });
        self.show_bb_dialog = open && !run;
        if run {
            self.run_box_behnken_dialog();
        }
    }

    fn design_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_design_dialog {
            return;
        }
        let mut open = self.show_design_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Create 2-Level Design"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Factors:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.design_factors)
                            .desired_width(220.0)
                            .hint_text("A,B,C"),
                    )
                    .labelled_by(label.id).hover_from(&label);
                });
                ui.small("Comma-separated factor names; values are generated as −1/+1.");
                ui.horizontal(|ui| {
                    let label = ui.help_label("Replicates:");
                    ui.add(egui::DragValue::new(&mut self.design_replicates).range(1..=100))
                        .labelled_by(label.id).hover_from(&label);
                    let label = ui.help_label("Center points:");
                    ui.add(egui::DragValue::new(&mut self.design_centers).range(0..=100))
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Block size:");
                    ui.add(egui::DragValue::new(&mut self.design_block_size).range(0..=100))
                        .labelled_by(label.id).hover_from(&label);
                    ui.small("0 = no blocks");
                });
                ui.horizontal(|ui| {
                    ui.help_checkbox(&mut self.design_randomize, "Randomize run order");
                    let label = ui.help_label("Seed:");
                    ui.add(egui::DragValue::new(&mut self.design_seed).speed(1.0))
                        .labelled_by(label.id).hover_from(&label);
                });
                ui.small("This replaces the current worksheet with StdOrder, RunOrder and factor columns.");
                ui.separator();
                if ui.help_button(self.catalog().label("Generate")).clicked() {
                    run = true;
                }
            });
        self.show_design_dialog = open && !run;
        if run {
            self.run_design_dialog();
        }
    }

    /// Opens the response-surface dialog.
    pub fn open_rsm_dialog(&mut self) {
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
        if numeric.len() < 3 {
            self.set_error("Response-surface analysis needs a response and at least two factors");
            return;
        }
        self.rsm_response = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.rsm_factors = numeric
            .into_iter()
            .filter(|name| *name != self.rsm_response)
            .collect();
        self.show_rsm_dialog = true;
    }

    /// Runs the response-surface model selected in the dialog.
    pub fn run_rsm_dialog(&mut self) {
        if self.rsm_response.is_empty() {
            self.set_error("Select the response column");
            return;
        }
        let factors: Vec<String> = self
            .rsm_factors
            .iter()
            .filter(|name| **name != self.rsm_response)
            .cloned()
            .collect();
        if factors.len() < 2 {
            self.set_error("Select at least two factors");
            return;
        }
        let spec = AnalysisSpec::response_surface(self.rsm_response.clone(), factors)
            .with_rsm_optimization(self.rsm_optimize)
            .with_rsm_contours(self.rsm_contours)
            .with_rsm_steepest_ascent(self.rsm_steepest_ascent);
        self.execute_spec(spec, format!("Response surface: {}", self.rsm_response));
        self.show_rsm_dialog = false;
    }

    fn rsm_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_rsm_dialog {
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
        let mut open = self.show_rsm_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Response Surface"))
            .open(&mut open)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Response:");
                    egui::ComboBox::from_id_salt("rsm_response")
                        .selected_text(if self.rsm_response.is_empty() {
                            "—".to_owned()
                        } else {
                            self.rsm_response.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.rsm_response,
                                    name.clone(),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_label("Factors (coded):");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            if *name == self.rsm_response {
                                continue;
                            }
                            let mut selected = self.rsm_factors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.rsm_factors.push(name.clone());
                                } else {
                                    self.rsm_factors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.help_checkbox(&mut self.rsm_optimize, "Stationary point and eigenvalues");
                ui.help_checkbox(&mut self.rsm_contours, "Contour plot (two factors)");
                ui.help_checkbox(&mut self.rsm_steepest_ascent, "Steepest ascent path");
                ui.small("Full quadratic model: main effects, two-way interactions and squares.");
                ui.small("Use a central composite design or another design with residual df.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_rsm_dialog = open && !run;
        if run {
            self.run_rsm_dialog();
        }
    }

    /// Opens the multi-response desirability optimiser.
    pub fn open_desirability_dialog(&mut self) {
        self.desirability_factors.clear();
        self.desirability_responses.clear();
        self.show_desirability_dialog = true;
    }

    /// Runs the response optimiser with the responses in the dialog.
    pub fn run_desirability_dialog(&mut self) {
        if self.desirability_factors.len() < 2 {
            self.set_error("Select at least two factor columns");
            return;
        }
        if self.desirability_responses.is_empty() {
            self.set_error("Add at least one response");
            return;
        }
        let parse = |text: &str, what: &str| -> Result<f64, String> {
            text.trim()
                .parse::<f64>()
                .map_err(|_| format!("{what} must be a number"))
        };
        let mut responses = Vec::with_capacity(self.desirability_responses.len());
        for row in &self.desirability_responses {
            let low = match parse(&row.low, "Low") {
                Ok(value) => value,
                Err(message) => return self.set_error(message),
            };
            let high = match parse(&row.high, "High") {
                Ok(value) => value,
                Err(message) => return self.set_error(message),
            };
            let target = if row.goal == Goal::Target {
                match parse(&row.target, "Target") {
                    Ok(value) => value,
                    Err(message) => return self.set_error(message),
                }
            } else {
                0.5 * (low + high)
            };
            let weight = if row.weight.trim().is_empty() {
                1.0
            } else {
                match parse(&row.weight, "Weight") {
                    Ok(value) => value,
                    Err(message) => return self.set_error(message),
                }
            };
            let importance = if row.importance.trim().is_empty() {
                1.0
            } else {
                match parse(&row.importance, "Importance") {
                    Ok(value) => value,
                    Err(message) => return self.set_error(message),
                }
            };
            responses.push(fp_core::desirability::Response {
                column: row.column.clone(),
                goal: row.goal,
                low,
                target,
                high,
                weight,
                importance,
            });
        }
        let spec = AnalysisSpec::desirability(self.desirability_factors.clone(), responses);
        self.execute_spec(spec, "Response Optimizer".to_owned());
        self.show_desirability_dialog = false;
    }

    fn desirability_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_desirability_dialog {
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
        let mut open = self.show_desirability_dialog;
        let mut run = false;
        let mut add: Option<String> = None;
        let mut remove: Option<usize> = None;
        dialog_window(self.catalog().label("Response Optimizer"))
            .open(&mut open)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.help_label("Factors (coded −1/+1):");
                egui::ScrollArea::vertical()
                    .max_height(90.0)
                    .show(ui, |ui| {
                        for name in &numeric {
                            let mut selected = self.desirability_factors.contains(name);
                            if ui.help_checkbox(&mut selected, name).changed() {
                                if selected {
                                    self.desirability_factors.push(name.clone());
                                } else {
                                    self.desirability_factors.retain(|value| value != name);
                                }
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let label = ui.help_label("Add response:");
                    egui::ComboBox::from_id_salt("desirability_add")
                        .selected_text("—")
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                if ui.selectable_label(false, format!("Add {name}")).clicked() {
                                    add = Some(name.clone());
                                }
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                for index in 0..self.desirability_responses.len() {
                    let row = &mut self.desirability_responses[index];
                    let column = row.column.clone();
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(&column);
                        let label = ui.help_label(format!("Goal ({column}):"));
                        egui::ComboBox::from_id_salt(("desirability_goal", index))
                            .selected_text(row.goal.name())
                            .show_ui(ui, |ui| {
                                for goal in [Goal::Maximize, Goal::Minimize, Goal::Target] {
                                    ui.selectable_value(&mut row.goal, goal, goal.name());
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                        if ui.help_button(format!("Remove {column}")).clicked() {
                            remove = Some(index);
                        }
                    });
                    ui.horizontal(|ui| {
                        for (caption, value) in [
                            ("Low", &mut row.low),
                            ("Target", &mut row.target),
                            ("High", &mut row.high),
                        ] {
                            let label = ui.help_label(format!("{caption} ({column}):"));
                            ui.text_edit_singleline(value)
                                .labelled_by(label.id)
                                .hover_from(&label);
                        }
                    });
                    ui.horizontal(|ui| {
                        for (caption, value) in [
                            ("Weight", &mut row.weight),
                            ("Importance", &mut row.importance),
                        ] {
                            let label = ui.help_label(format!("{caption} ({column}):"));
                            ui.text_edit_singleline(value)
                                .labelled_by(label.id)
                                .hover_from(&label);
                        }
                    });
                }
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        if let Some(column) = add {
            let (low, high) = self
                .data
                .as_ref()
                .and_then(|df| df.column(&column).ok())
                .and_then(|data| {
                    let sample = data.sample(fp_core::data::MissingPolicy::Drop).ok()?;
                    Some((sample.min().ok()?, sample.max().ok()?))
                })
                .unwrap_or((0.0, 1.0));
            let (low, high) = if low < high {
                (low, high)
            } else {
                (low - 1.0, high + 1.0)
            };
            self.desirability_responses.push(DesirabilityRow {
                column,
                goal: Goal::Maximize,
                low: format!("{low}"),
                target: format!("{}", 0.5 * (low + high)),
                high: format!("{high}"),
                weight: "1".to_owned(),
                importance: "1".to_owned(),
            });
        }
        if let Some(index) = remove {
            self.desirability_responses.remove(index);
        }
        self.show_desirability_dialog = open && !run;
        if run {
            self.run_desirability_dialog();
        }
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

    /// Opens the censored-Weibull dialog.
    pub fn open_weibull_dialog(&mut self) {
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
        if numeric.is_empty() {
            self.set_error("Weibull reliability needs a numeric time column");
            return;
        }
        self.weibull_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.weibull_censor = None;
        self.weibull_censor_value = "0".to_owned();
        self.show_weibull_dialog = true;
    }

    /// Runs the censored-Weibull fit selected in the dialog.
    pub fn run_weibull_dialog(&mut self) {
        if self.weibull_time.is_empty() {
            self.set_error("Select the time column");
            return;
        }
        let Ok(censor_value) = self.weibull_censor_value.trim().parse::<f64>() else {
            self.set_error("Censoring value must be a number");
            return;
        };
        let spec = AnalysisSpec::weibull_reliability(self.weibull_time.clone())
            .with_censoring(self.weibull_censor.clone(), censor_value);
        self.execute_spec(spec, format!("Weibull reliability: {}", self.weibull_time));
        self.show_weibull_dialog = false;
    }

    fn weibull_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_weibull_dialog {
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
        let mut open = self.show_weibull_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Weibull Reliability"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Time column:");
                    egui::ComboBox::from_id_salt("weibull_time")
                        .selected_text(if self.weibull_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.weibull_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.weibull_time,
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
                    let label = ui.help_label("Censoring column:");
                    egui::ComboBox::from_id_salt("weibull_censor")
                        .selected_text(
                            self.weibull_censor
                                .as_deref()
                                .unwrap_or("None (all events)"),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.weibull_censor,
                                None,
                                "None (all events)",
                            );
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.weibull_censor,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.weibull_censor_value)
                            .desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Right-censored Weibull MLE; shape optimized, scale profiled.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_weibull_dialog = open && !run;
        if run {
            self.run_weibull_dialog();
        }
    }

    /// Opens the Type 1 Gage dialog.
    pub fn open_type1_dialog(&mut self) {
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
        if numeric.is_empty() {
            self.set_error("Type 1 Gage needs a numeric measurement column");
            return;
        }
        self.type1_measurement = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.type1_reference = "0".to_owned();
        self.type1_lsl.clear();
        self.type1_usl.clear();
        self.show_type1_dialog = true;
    }

    /// Runs the Type 1 Gage study selected in the dialog.
    pub fn run_type1_dialog(&mut self) {
        if self.type1_measurement.is_empty() {
            self.set_error("Select the measurement column");
            return;
        }
        let Ok(reference) = self.type1_reference.trim().parse::<f64>() else {
            self.set_error("Reference value must be a number");
            return;
        };
        let parse_limit = |text: &str| -> Result<Option<f64>, String> {
            let text = text.trim();
            if text.is_empty() {
                Ok(None)
            } else {
                text.parse::<f64>()
                    .map(Some)
                    .map_err(|_| "Specification limits must be numbers".to_owned())
            }
        };
        let lsl = match parse_limit(&self.type1_lsl) {
            Ok(value) => value,
            Err(message) => {
                self.set_error(message);
                return;
            }
        };
        let usl = match parse_limit(&self.type1_usl) {
            Ok(value) => value,
            Err(message) => {
                self.set_error(message);
                return;
            }
        };
        let spec = AnalysisSpec::gage_type1(self.type1_measurement.clone(), reference)
            .with_gage_limits(lsl, usl);
        self.execute_spec(spec, format!("Type 1 Gage: {}", self.type1_measurement));
        self.show_type1_dialog = false;
    }

    fn type1_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_type1_dialog {
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
        let mut open = self.show_type1_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Gage Type 1"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Measurement:");
                    egui::ComboBox::from_id_salt("type1_measurement")
                        .selected_text(if self.type1_measurement.is_empty() {
                            "—".to_owned()
                        } else {
                            self.type1_measurement.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.type1_measurement,
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
                    let label = ui.help_label("Reference:");
                    ui.text_edit_singleline(&mut self.type1_reference)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let lsl = ui.help_label("LSL:");
                    ui.text_edit_singleline(&mut self.type1_lsl)
                        .labelled_by(lsl.id)
                        .hover_from(&lsl);
                    let usl = ui.help_label("USL:");
                    ui.text_edit_singleline(&mut self.type1_usl)
                        .labelled_by(usl.id)
                        .hover_from(&usl);
                });
                ui.small("Repeated measurements of one reference part; limits optional.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_type1_dialog = open && !run;
        if run {
            self.run_type1_dialog();
        }
    }

    /// Opens the censored-exponential dialog.
    pub fn open_exponential_dialog(&mut self) {
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
        if numeric.is_empty() {
            self.set_error("Exponential reliability needs a numeric time column");
            return;
        }
        self.exp_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.exp_censor = None;
        self.exp_censor_value = "0".to_owned();
        self.show_exp_dialog = true;
    }

    /// Runs the censored-exponential fit selected in the dialog.
    pub fn run_exponential_dialog(&mut self) {
        if self.exp_time.is_empty() {
            self.set_error("Select the time column");
            return;
        }
        let Ok(censor_value) = self.exp_censor_value.trim().parse::<f64>() else {
            self.set_error("Censoring value must be a number");
            return;
        };
        let spec = AnalysisSpec::exponential_reliability(self.exp_time.clone())
            .with_censoring(self.exp_censor.clone(), censor_value);
        self.execute_spec(spec, format!("Exponential reliability: {}", self.exp_time));
        self.show_exp_dialog = false;
    }

    fn exponential_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_exp_dialog {
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
        let mut open = self.show_exp_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Exponential Reliability"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Time column:");
                    egui::ComboBox::from_id_salt("exp_time")
                        .selected_text(if self.exp_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.exp_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.exp_time,
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
                    let label = ui.help_label("Censoring column:");
                    egui::ComboBox::from_id_salt("exp_censor")
                        .selected_text(self.exp_censor.as_deref().unwrap_or("None (all events)"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.exp_censor, None, "None (all events)");
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.exp_censor,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.exp_censor_value).desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Right-censored exponential MLE; θ = total time / events.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_exp_dialog = open && !run;
        if run {
            self.run_exponential_dialog();
        }
    }

    /// Opens the censored-lognormal dialog.
    pub fn open_lognormal_dialog(&mut self) {
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
        if numeric.is_empty() {
            self.set_error("Lognormal reliability needs a numeric time column");
            return;
        }
        self.log_time = self
            .selected
            .clone()
            .filter(|name| numeric.contains(name))
            .unwrap_or_else(|| numeric[0].clone());
        self.log_censor = None;
        self.log_censor_value = "0".to_owned();
        self.show_log_dialog = true;
    }

    /// Runs the censored-lognormal fit selected in the dialog.
    pub fn run_lognormal_dialog(&mut self) {
        if self.log_time.is_empty() {
            self.set_error("Select the time column");
            return;
        }
        let Ok(censor_value) = self.log_censor_value.trim().parse::<f64>() else {
            self.set_error("Censoring value must be a number");
            return;
        };
        let spec = AnalysisSpec::lognormal_reliability(self.log_time.clone())
            .with_censoring(self.log_censor.clone(), censor_value);
        self.execute_spec(spec, format!("Lognormal reliability: {}", self.log_time));
        self.show_log_dialog = false;
    }

    fn lognormal_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_log_dialog {
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
        let mut open = self.show_log_dialog;
        let mut run = false;
        dialog_window(self.catalog().label("Lognormal Reliability"))
            .open(&mut open)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Time column:");
                    egui::ComboBox::from_id_salt("log_time")
                        .selected_text(if self.log_time.is_empty() {
                            "—".to_owned()
                        } else {
                            self.log_time.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.log_time,
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
                    let label = ui.help_label("Censoring column:");
                    egui::ComboBox::from_id_salt("log_censor")
                        .selected_text(self.log_censor.as_deref().unwrap_or("None (all events)"))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.log_censor, None, "None (all events)");
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.log_censor,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Censoring value:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.log_censor_value).desired_width(90.0),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                });
                ui.small("Right-censored lognormal MLE; coordinate ascent on the logarithm of x.");
                ui.separator();
                if ui.help_button(self.catalog().label("Run")).clicked() {
                    run = true;
                }
            });
        self.show_log_dialog = open && !run;
        if run {
            self.run_lognormal_dialog();
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

    /// Appends a result to the Session, in the language of the interface.
    fn append_result(&mut self, result: &fp_core::report::AnalysisResult) {
        let text = result.to_session_text_lang(fp_core::i18n::Catalog::new(self.language));
        self.append_session(&text);
    }

    fn append_session(&mut self, text: &str) {
        if !self.session.is_empty() {
            self.session.push_str("\n\n");
        }
        self.session.push_str(text);
        // A long session is trimmed from the front, on a report boundary, so
        // the window stays responsive on machines with little memory.
        if self.session.len() > SESSION_LIMIT {
            let cut = self.session.len() - SESSION_LIMIT;
            let start = self
                .session
                .char_indices()
                .find(|(index, ch)| *index >= cut && ch.is_ascii_whitespace())
                .map_or(0, |(index, _)| index + 1);
            self.session.drain(..start);
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

    /// Draws one frame. The desktop runner calls this from `eframe::App::ui`.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll_analysis(ui.ctx());
        if ui.ctx().input(|input| input.key_pressed(egui::Key::Escape)) {
            self.close_dialogs();
        }
        let (zoom_in, zoom_out, zoom_reset) = ui.ctx().input(|input| {
            (
                input.modifiers.command
                    && (input.key_pressed(egui::Key::Plus) || input.key_pressed(egui::Key::Equals)),
                input.modifiers.command && input.key_pressed(egui::Key::Minus),
                input.modifiers.command && input.key_pressed(egui::Key::Num0),
            )
        });
        if zoom_in {
            self.zoom = (self.zoom * 1.1).min(2.5);
        }
        if zoom_out {
            self.zoom = (self.zoom / 1.1).max(0.5);
        }
        if zoom_reset {
            self.zoom = 1.0;
        }
        let (new, open, save, print, select_all, find) = ui.ctx().input(|input| {
            let command = input.modifiers.command;
            (
                command && input.key_pressed(egui::Key::N),
                command && input.key_pressed(egui::Key::O),
                command && input.key_pressed(egui::Key::S),
                command && input.key_pressed(egui::Key::P),
                command && input.key_pressed(egui::Key::A),
                command && input.key_pressed(egui::Key::F),
            )
        });
        if !ui.ctx().egui_wants_keyboard_input() {
            if new {
                self.new_worksheet();
            }
            if open {
                self.begin_open(FileKind::Csv);
            }
            if save {
                self.begin_save(FileKind::Csv);
            }
            if print {
                self.print_request = Some(std::env::temp_dir().join("FreePlots-session.pdf"));
            }
            if select_all {
                self.select_all_cells();
            }
            if find {
                self.open_find_dialog();
            }
        }
        ui.ctx().set_zoom_factor(self.zoom);
        if ui
            .ctx()
            .data_mut(|data| data.remove_temp::<bool>(egui::Id::new("fp_clear_dialog")))
            .unwrap_or(false)
        {
            self.reset_dialog_defaults();
        }
        let want_light = !self.dark_mode;
        if self.light_visuals != want_light {
            apply_theme(ui.ctx(), want_light);
            self.light_visuals = want_light;
        }

        egui::Panel::top("fp_menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("fp_toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("fp_status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::bottom("fp_sheets").show(ui, |ui| self.sheet_tabs(ui));
        if self.show_session {
            egui::Panel::right("fp_session")
                .resizable(true)
                .default_size(520.0)
                .size_range(320.0..=720.0)
                .show(ui, |ui| self.session_panel(ui));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(if self.dark_mode {
                        Color32::from_rgb(20, 23, 27)
                    } else {
                        Color32::from_rgb(252, 250, 246)
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        if self.dark_mode {
                            Color32::from_rgb(44, 52, 61)
                        } else {
                            Color32::from_rgb(218, 212, 201)
                        },
                    ))
                    .inner_margin(egui::Margin::same(2)),
            )
            .show(ui, |ui| self.worksheet(ui));

        let ctx = ui.ctx().clone();
        self.open_window(&ctx);
        self.csv_options_dialog(&ctx);
        self.save_window(&ctx);
        self.rename_window(&ctx);
        self.sheet_rename_window(&ctx);
        self.probability_plot_dialog(&ctx);
        self.capability_dialog(&ctx);
        self.control_chart_dialog(&ctx);
        self.attribute_chart_dialog(&ctx);
        self.t_test_dialog(&ctx);
        self.equivalence_dialog(&ctx);
        self.rank_test_dialog(&ctx);
        self.correlation_dialog(&ctx);
        self.curve_dialog(&ctx);
        self.two_way_dialog(&ctx);
        self.crosstab_dialog(&ctx);
        self.g_chart_dialog(&ctx);
        self.partial_dialog(&ctx);
        self.box_cox_dialog(&ctx);
        self.tolerance_dialog(&ctx);
        self.linearity_dialog(&ctx);
        self.logistic_dialog(&ctx);
        self.poisson_dialog(&ctx);
        self.ordinal_dialog(&ctx);
        self.pareto_dialog(&ctx);
        self.calc_dialog(&ctx);
        self.sheet_dialog(&ctx);
        self.interval_dialog(&ctx);
        self.matrix_dialog(&ctx);
        self.dotplot_dialog(&ctx);
        self.stem_dialog(&ctx);
        self.series_dialog(&ctx);
        self.ecdf_dialog(&ctx);
        self.alt_dialog(&ctx);
        self.step_stress_dialog(&ctx);
        self.timeseries_dialog(&ctx);
        self.pca_dialog(&ctx);
        self.kmeans_dialog(&ctx);
        self.t2_dialog(&ctx);
        self.sampling_dialog(&ctx);
        self.taguchi_dialog(&ctx);
        self.taguchi_design_dialog(&ctx);
        self.agreement_dialog(&ctx);
        self.sort_dialog(&ctx);
        self.stack_dialog(&ctx);
        self.unstack_dialog(&ctx);
        self.subset_dialog(&ctx);
        self.date_diff_dialog(&ctx);
        self.date_shift_dialog(&ctx);
        self.diagnostic_dialog(&ctx);
        self.power_dialog(&ctx);
        self.test2_dialog(&ctx);
        self.regression_dialog(&ctx);
        self.stepwise_dialog(&ctx);
        self.anova_dialog(&ctx);
        self.kaplan_meier_dialog(&ctx);
        self.weibull_dialog(&ctx);
        self.exponential_dialog(&ctx);
        self.lognormal_dialog(&ctx);
        self.gage_dialog(&ctx);
        self.type1_dialog(&ctx);
        self.doe_dialog(&ctx);
        self.rsm_dialog(&ctx);
        self.desirability_dialog(&ctx);
        self.scatter_dialog(&ctx);
        self.design_dialog(&ctx);
        self.box_behnken_dialog(&ctx);
        self.pb_dialog(&ctx);
        self.describe_by_dialog(&ctx);
        self.shortcuts_dialog(&ctx);
        self.assistant_dialog(&ctx);
        self.find_dialog(&ctx);
        self.format_dialog(&ctx);
        self.about_window(&ctx);
        self.graph_window(&ctx);
    }

    fn sheet_tabs(&mut self, ui: &mut egui::Ui) {
        let mut switch_to: Option<usize> = None;
        let mut add = false;
        let mut scroll_to: Option<usize> = None;
        let mut rename_to: Option<usize> = None;
        let mut close_to: Option<usize> = None;
        let last = self.sheets.len().saturating_sub(1);
        let arrow = |ui: &mut egui::Ui, glyph: &str, tooltip: &str| {
            ui.add(
                egui::Button::new(RichText::new(glyph).size(11.0))
                    .min_size(egui::vec2(20.0, 22.0))
                    .corner_radius(egui::CornerRadius::same(3)),
            )
            .on_hover_text(tooltip)
            .clicked()
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if arrow(ui, "|◀", "First worksheet") {
                scroll_to = Some(0);
            }
            if arrow(ui, "◀", "Scroll the worksheet tabs left") {
                scroll_to = Some(self.tab_first.saturating_sub(1));
            }
            if arrow(ui, "▶", "Scroll the worksheet tabs right") {
                scroll_to = Some((self.tab_first + 1).min(last));
            }
            if arrow(ui, "▶|", "Last worksheet") {
                scroll_to = Some(last);
            }
            if ui
                .add(
                    egui::Button::new(RichText::new("+").size(15.0).strong())
                        .min_size(egui::vec2(24.0, 22.0))
                        .corner_radius(egui::CornerRadius::same(3)),
                )
                .on_hover_text("Add a new worksheet")
                .clicked()
            {
                add = true;
            }
            egui::ScrollArea::horizontal()
                .id_salt("fp_sheet_tabs")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (index, sheet) in self.sheets.iter().enumerate().skip(self.tab_first) {
                            let active = index == self.active_sheet;
                            let text = RichText::new(&sheet.name)
                                .size(12.5)
                                .color(ui.visuals().text_color());
                            let response = ui
                                .add(
                                    egui::Button::new(text)
                                        .fill(if active {
                                            ui.visuals().selection.bg_fill
                                        } else {
                                            ui.visuals().widgets.inactive.weak_bg_fill
                                        })
                                        .stroke(ui.visuals().widgets.inactive.bg_stroke)
                                        .corner_radius(egui::CornerRadius {
                                            nw: 4,
                                            ne: 4,
                                            sw: 0,
                                            se: 0,
                                        })
                                        .min_size(egui::vec2(0.0, 22.0)),
                                )
                                .on_hover_text(format!("{} — click to switch", sheet.name));
                            if response.clicked() && !active {
                                switch_to = Some(index);
                            }
                            if response.double_clicked() {
                                rename_to = Some(index);
                            }
                            response.context_menu(|ui| {
                                if ui
                                    .button("Rename…")
                                    .on_hover_text(button_help("Rename…"))
                                    .clicked()
                                {
                                    rename_to = Some(index);
                                    ui.close();
                                }
                                if ui
                                    .button("Close worksheet")
                                    .on_hover_text(button_help("Close worksheet"))
                                    .clicked()
                                {
                                    close_to = Some(index);
                                    ui.close();
                                }
                                ui.separator();
                                if ui
                                    .button("New worksheet")
                                    .on_hover_text(button_help("New worksheet"))
                                    .clicked()
                                {
                                    add = true;
                                    ui.close();
                                }
                            });
                        }
                    });
                });
        });
        if let Some(index) = scroll_to {
            self.tab_first = index.min(last);
        }
        if let Some(index) = rename_to {
            self.open_sheet_rename(index);
        }
        if let Some(index) = close_to {
            self.close_sheet(index);
        }
        if let Some(index) = switch_to {
            self.switch_sheet(index);
        }
        if add {
            self.add_sheet();
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.style_mut().visuals.override_text_color = Some(ui.visuals().text_color());
            help_menu_button(ui, self.catalog().label("File"), |ui| {
                if menu_icon_item(
                    ui,
                    ToolIcon::New,
                    self.catalog().label("New Worksheet"),
                    None,
                    true,
                )
                .clicked()
                {
                    self.new_worksheet();
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Open, self.catalog().label("Open CSV…"), None, true)
                    .clicked()
                {
                    self.begin_open(FileKind::Csv);
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Open,
                    self.catalog().label("Open Project…"),
                    None,
                    true,
                )
                .clicked()
                {
                    self.begin_open(FileKind::Project);
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Open,
                    self.catalog().label("Open Excel…"),
                    None,
                    true,
                )
                .clicked()
                {
                    self.begin_open(FileKind::Xlsx);
                    ui.close();
                }
                let has_data = self.data.is_some();
                if menu_icon_item(
                    ui,
                    ToolIcon::Save,
                    self.catalog().label("Save CSV As…"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.begin_save(FileKind::Csv);
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Save,
                    self.catalog().label("Save Project As…"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.begin_save(FileKind::Project);
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Export,
                    self.catalog().label("Print…"),
                    None,
                    true,
                )
                .on_hover_text("Open the Session report in the system PDF viewer to print")
                .clicked()
                {
                    let path = std::env::temp_dir().join("FreePlots-session.pdf");
                    self.print_request = Some(path);
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Export,
                    self.catalog().label("Export Session PDF…"),
                    None,
                    !self.session.is_empty(),
                )
                .clicked()
                {
                    self.begin_pdf_export();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Export,
                    self.catalog().label("Export Session HTML…"),
                    None,
                    !self.session.is_empty(),
                )
                .clicked()
                {
                    self.begin_session_export();
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(ui, ToolIcon::Clear, self.catalog().label("Exit"), None, true)
                    .clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            help_menu_button(ui, self.catalog().label("Edit"), |ui| {
                let has_data = self.data.is_some();
                let can_undo = self.can_undo();
                let can_redo = self.can_redo();
                if menu_icon_item(
                    ui,
                    ToolIcon::Undo,
                    self.catalog().label("Undo"),
                    Some("Ctrl+Z"),
                    can_undo,
                )
                .clicked()
                {
                    self.undo();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Redo,
                    self.catalog().label("Redo"),
                    Some("Ctrl+Y"),
                    can_redo,
                )
                .clicked()
                {
                    self.redo();
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(
                    ui,
                    ToolIcon::Copy,
                    self.catalog().label("Copy"),
                    Some("Ctrl+C"),
                    has_data,
                )
                .clicked()
                {
                    if let Some(text) = self.selection_tsv() {
                        self.clipboard = text.clone();
                        ui.ctx().copy_text(text);
                    }
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Cut,
                    self.catalog().label("Cut"),
                    Some("Ctrl+X"),
                    has_data,
                )
                .clicked()
                {
                    if let Some(text) = self.selection_tsv() {
                        self.clipboard = text.clone();
                        ui.ctx().copy_text(text);
                    }
                    self.clear_selection();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Paste,
                    self.catalog().label("Paste"),
                    Some("Ctrl+V"),
                    has_data,
                )
                .clicked()
                {
                    if self.clipboard.is_empty() {
                        self.paste_request = true;
                    } else {
                        self.paste_internal();
                    }
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Grid,
                    self.catalog().label("Select All"),
                    Some("Ctrl+A"),
                    has_data,
                )
                .clicked()
                {
                    self.select_all_cells();
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(
                    ui,
                    ToolIcon::Info,
                    self.catalog().label("Find…"),
                    Some("Ctrl+F"),
                    has_data,
                )
                .clicked()
                {
                    self.open_find_dialog();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Sigma,
                    self.catalog().label("Format Cells…"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.open_format_dialog();
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(
                    ui,
                    ToolIcon::Clear,
                    self.catalog().label("Clear Cells"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.clear_selected_cell();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Pencil,
                    self.catalog().label("Rename Column…"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.begin_rename_column();
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(
                    ui,
                    ToolIcon::Clear,
                    self.catalog().label("Clear Session"),
                    None,
                    true,
                )
                .clicked()
                {
                    self.session.clear();
                    ui.close();
                }
            });

            help_menu_button(ui, self.catalog().label("Data"), |ui| {
                let has_data = self.data.is_some();
                if menu_icon_item(
                    ui,
                    ToolIcon::Plus,
                    self.catalog().label("Add Row"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.add_row();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Plus,
                    self.catalog().label("Insert Row Above"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.insert_row();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Minus,
                    self.catalog().label("Delete Row"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.delete_row();
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(
                    ui,
                    ToolIcon::Plus,
                    self.catalog().label("Add Numeric Column"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.add_column();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Plus,
                    self.catalog().label("Add Text Column"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.add_text_column();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Plus,
                    self.catalog().label("Add Date Column"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.add_date_column();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Minus,
                    self.catalog().label("Delete Column"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.delete_column();
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(
                    ui,
                    ToolIcon::Sort,
                    self.catalog().label("Sort…"),
                    None,
                    has_data,
                )
                .on_hover_text("Order the rows by one or more columns")
                .clicked()
                {
                    self.open_sort_dialog();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Swap,
                    self.catalog().label("Transpose"),
                    None,
                    has_data,
                )
                .on_hover_text("Columns become rows and rows become columns")
                .clicked()
                {
                    self.run_transpose();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Layers,
                    self.catalog().label("Stack…"),
                    None,
                    has_data,
                )
                .on_hover_text("Fold several columns into Value and Variable")
                .clicked()
                {
                    self.open_stack_dialog();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Layers,
                    self.catalog().label("Unstack…"),
                    None,
                    has_data,
                )
                .on_hover_text("One column per level of a categorical column")
                .clicked()
                {
                    self.open_unstack_dialog();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Filter,
                    self.catalog().label("Subset…"),
                    None,
                    has_data,
                )
                .on_hover_text("Keep the rows that match a condition")
                .clicked()
                {
                    self.open_subset_dialog();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Clock,
                    self.catalog().label("Date Difference…"),
                    None,
                    has_data,
                )
                .on_hover_text("Days between two date columns")
                .clicked()
                {
                    self.open_date_diff_dialog();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Clock,
                    self.catalog().label("Add Days…"),
                    None,
                    has_data,
                )
                .on_hover_text("Shift a date column by a number of days")
                .clicked()
                {
                    self.open_date_shift_dialog();
                    ui.close();
                }
                help_menu_button(ui, self.catalog().label("Date Format"), |ui| {
                        for format in [
                            DateFormat::Iso,
                            DateFormat::DayMonthYear,
                            DateFormat::MonthDayYear,
                            DateFormat::DayMonthNameYear,
                        ] {
                            if ui
                                .selectable_label(self.date_format == format, format.name())
                                .on_hover_text("How the date cells are shown")
                                .clicked()
                            {
                                self.date_format = format;
                                ui.close();
                            }
                        }
                    },
                );
                ui.separator();
                if menu_icon_item(
                    ui,
                    ToolIcon::Grid,
                    self.catalog().label("Select First Numeric Column"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    if let Some(df) = &self.data
                        && let Some(name) = first_numeric_column(df)
                        && let Ok(index) = df.column_index(name)
                    {
                        self.select_cell(index, 0);
                    }
                    ui.close();
                }
            });

            help_menu_button(ui, self.catalog().label("Calc"), |ui| {
                if menu_icon_item(
                    ui,
                    ToolIcon::Calculator,
                    self.catalog().label("Calculator…"),
                    None,
                    self.data.is_some(),
                )
                .on_hover_text("Add a column computed from an expression")
                .clicked()
                {
                    self.open_calc_dialog();
                    ui.close();
                }
            });

            help_menu_button(ui, self.catalog().label("Stat"), |ui| {
                help_menu_button(ui, self.catalog().label("Basic Statistics"), |ui| {
                    let ready = self.selection_is_numeric();
                    if menu_text_item(ui, self.catalog().label("Display Descriptive Statistics"), None, ready)
                        .on_hover_text("N, mean, SE mean, StDev, quartiles, mean CI")
                        .clicked()
                    {
                        self.run_descriptive_statistics();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Display Descriptive Statistics by…"), None, ready)
                        .on_hover_text("One block per level of a By column")
                        .clicked()
                    {
                        self.open_describe_by_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Probability Plot…"), None, ready)
                        .clicked()
                    {
                        self.open_probability_plot_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Normality Test (AD)"), None, ready)
                        .on_hover_text("Anderson–Darling normality test")
                        .clicked()
                    {
                        self.run_normality_test();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("D'Agostino–Pearson Test"), None, ready)
                        .on_hover_text("Omnibus test of the skewness and the kurtosis")
                        .clicked()
                    {
                        self.run_dagostino_test();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Kolmogorov–Smirnov Test"), None, ready)
                        .on_hover_text("Monte Carlo Lilliefors p-value")
                        .clicked()
                    {
                        self.run_ks_test();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Ryan–Joiner Test"), None, ready)
                        .on_hover_text("Normal-score correlation with Monte Carlo p-value")
                        .clicked()
                    {
                        self.run_rj_test();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("1-Sample t…"), None, ready)
                        .on_hover_text("Student t test of one mean")
                        .clicked()
                    {
                        self.open_t_test_dialog(TTestKind::OneSample);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("2-Sample t…"), None, ready)
                        .on_hover_text("Welch or pooled two-sample Student t test")
                        .clicked()
                    {
                        self.open_t_test_dialog(TTestKind::TwoSample);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Paired t…"), None, ready)
                        .on_hover_text("Student t test of paired differences")
                        .clicked()
                    {
                        self.open_t_test_dialog(TTestKind::Paired);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Equivalence Test…"), None, ready)
                        .on_hover_text("Two one-sided tests (TOST) for practical equivalence")
                        .clicked()
                    {
                        self.open_equivalence_dialog(EquivalenceKind::OneSample);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Chi-Square Goodness-of-Fit…"), None, ready)
                        .on_hover_text("Observed counts against expected counts")
                        .clicked()
                    {
                        self.open_diagnostic_dialog(DiagnosticKind::ChiSquareGoodnessOfFit);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Grubbs Test…"), None, ready)
                        .on_hover_text("Test for a single outlier")
                        .clicked()
                    {
                        self.open_diagnostic_dialog(DiagnosticKind::Grubbs);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Power and Sample Size…"), None, ready)
                        .on_hover_text("Power of a test, or the sample size it needs")
                        .clicked()
                    {
                        self.open_power_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Mann-Whitney…"), None, ready)
                        .on_hover_text("Rank-sum test of two independent samples")
                        .clicked()
                    {
                        self.open_rank_test_dialog(RankTestKind::MannWhitney);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Wilcoxon Signed-Rank…"), None, ready)
                        .on_hover_text("Signed-rank test of paired differences")
                        .clicked()
                    {
                        self.open_rank_test_dialog(RankTestKind::Wilcoxon);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Kruskal-Wallis…"), None, ready)
                        .on_hover_text("Rank test of k independent samples")
                        .clicked()
                    {
                        self.open_rank_test_dialog(RankTestKind::KruskalWallis);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Chi-Square Test…"), None, ready)
                        .on_hover_text("Cross-tabulation and test of independence")
                        .clicked()
                    {
                        self.open_crosstab_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Two-Way ANOVA…"), None, ready)
                        .on_hover_text("Balanced two-way ANOVA with the interaction")
                        .clicked()
                    {
                        self.open_two_way_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Curve Fit…"), None, ready)
                        .on_hover_text("Polynomial, exponential or power fit of a response")
                        .clicked()
                    {
                        self.open_curve_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Correlation Matrix…"), None, ready)
                        .on_hover_text("Pearson, Spearman and Kendall correlations of every pair")
                        .clicked()
                    {
                        self.open_correlation_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Tolerance Interval…"), None, ready)
                        .on_hover_text("Interval that covers a proportion of the population")
                        .clicked()
                    {
                        self.open_tolerance_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Box-Cox Transformation…"), None, ready)
                        .on_hover_text("Transform the data to a Normal shape")
                        .clicked()
                    {
                        self.open_box_cox_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Partial Correlation…"), None, ready)
                        .on_hover_text("Correlation of two variables with the effect of a third removed")
                        .clicked()
                    {
                        self.open_partial_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Friedman…"), None, ready)
                        .on_hover_text("Rank test of k groups measured on the same rows")
                        .clicked()
                    {
                        self.open_rank_test_dialog(RankTestKind::Friedman);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Sign Test…"), None, ready)
                        .on_hover_text("Exact test of a hypothesised median")
                        .clicked()
                    {
                        self.open_rank_test_dialog(RankTestKind::SignTest);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Mood's Median Test…"), None, ready)
                        .on_hover_text("Test of equal medians across the levels of a factor")
                        .clicked()
                    {
                        self.open_rank_test_dialog(RankTestKind::MoodsMedian);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("1 Proportion…"), None, ready)
                        .on_hover_text("Exact or normal one-sample proportion test")
                        .clicked()
                    {
                        self.open_test2_dialog(Test2Kind::OneProportion);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("2 Proportions…"), None, ready)
                        .on_hover_text("Pooled normal two-sample proportion test")
                        .clicked()
                    {
                        self.open_test2_dialog(Test2Kind::TwoProportions);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("1 Variance…"), None, ready)
                        .on_hover_text("Chi-square test for one variance")
                        .clicked()
                    {
                        self.open_test2_dialog(Test2Kind::OneVariance);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("2 Variances…"), None, ready)
                        .on_hover_text("F test for two variances")
                        .clicked()
                    {
                        self.open_test2_dialog(Test2Kind::TwoVariances);
                        ui.close();
                    }
                });
                help_menu_button(ui, self.catalog().label("Multivariate"), |ui| {
                    if menu_text_item(ui, self.catalog().label("Principal Components…"), None, self.data.is_some())
                        .on_hover_text("Eigenvalues, loadings and scores of the correlation matrix")
                        .clicked()
                    {
                        self.open_pca_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Taguchi Design…"), None, true)
                        .on_hover_text("L8, L9, L12 or L16 orthogonal array")
                        .clicked()
                    {
                        self.open_taguchi_design_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Taguchi Analysis…"), None, self.data.is_some())
                        .on_hover_text("Signal-to-noise ratios and factor ranking")
                        .clicked()
                    {
                        self.open_taguchi_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Acceptance Sampling…"), None, true)
                        .on_hover_text("OC curve and sampling plans by attributes")
                        .clicked()
                    {
                        self.open_sampling_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Time Series Plot…"), None, self.data.is_some())
                        .on_hover_text("The observations in row order with the Mann–Kendall trend")
                        .clicked()
                    {
                        self.open_series_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Stem-and-Leaf…"), None, self.data.is_some())
                        .on_hover_text("Every observation kept in stems and leaves")
                        .clicked()
                    {
                        self.open_stem_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Empirical CDF…"), None, self.data.is_some())
                        .on_hover_text("Step function with a DKW confidence band")
                        .clicked()
                    {
                        self.open_ecdf_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Dotplot…"), None, self.data.is_some())
                        .on_hover_text("Individual values with the mean of each level")
                        .clicked()
                    {
                        self.open_dotplot_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Matrix Plot…"), None, self.data.is_some())
                        .on_hover_text("Scatterplots of every pair with histograms on the diagonal")
                        .clicked()
                    {
                        self.open_matrix_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Hotelling T² Chart…"), None, self.data.is_some())
                        .on_hover_text("Simultaneous control chart of several variables")
                        .clicked()
                    {
                        self.open_t2_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("K-Means Clustering…"), None, self.data.is_some())
                        .on_hover_text("Groups of similar rows with k-means++ initialisation")
                        .clicked()
                    {
                        self.open_kmeans_dialog();
                        ui.close();
                    }
                });
                help_menu_button(ui, self.catalog().label("Time Series"), |ui| {
                    if menu_text_item(ui, self.catalog().label("Moving Average…"), None, self.data.is_some())
                        .on_hover_text("Centred moving average with its residuals")
                        .clicked()
                    {
                        self.timeseries_kind = TimeSeriesKind::MovingAverage;
                        self.open_timeseries_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Decomposition…"), None, self.data.is_some())
                        .on_hover_text("Trend, seasonal indices and residuals")
                        .clicked()
                    {
                        self.timeseries_kind = TimeSeriesKind::Decomposition;
                        self.open_timeseries_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Exponential Smoothing…"), None, self.data.is_some())
                        .on_hover_text("Level (or Holt-Winters) smoothing with forecasts")
                        .clicked()
                    {
                        self.timeseries_kind = TimeSeriesKind::ExponentialSmoothing;
                        self.open_timeseries_dialog();
                        ui.close();
                    }
                });
                help_menu_button(ui, self.catalog().label("Regression"), |ui| {
                    if menu_text_item(ui, self.catalog().label("Regression…"), None, self.data.is_some())
                        .on_hover_text("Ordinary least squares with numeric predictors")
                        .clicked()
                    {
                        self.open_regression_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Stepwise Regression…"),
                        None,
                        self.data.is_some(),
                    )
                    .on_hover_text("Forward, backward or stepwise selection of numeric predictors")
                    .clicked()
                    {
                        self.open_stepwise_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Ordinal Logistic Regression…"), None, self.data.is_some())
                        .on_hover_text("Proportional-odds model for ordered categories")
                        .clicked()
                    {
                        self.open_ordinal_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Poisson Regression…"), None, self.data.is_some())
                        .on_hover_text("Log-linear model for non-negative integer counts")
                        .clicked()
                    {
                        self.open_poisson_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Binary Logistic Regression…"), None, self.data.is_some())
                        .on_hover_text("Maximum-likelihood logit model for a 0/1 response")
                        .clicked()
                    {
                        self.open_logistic_dialog();
                        ui.close();
                    }
                });
                help_menu_button(ui, self.catalog().label("ANOVA"), |ui| {
                    if menu_text_item(ui, self.catalog().label("One-Way…"), None, self.data.is_some())
                        .on_hover_text("One numeric response by one factor")
                        .clicked()
                    {
                        self.open_anova_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Levene Test…"), None, self.data.is_some())
                        .on_hover_text("Test of equal variances across the levels of a factor")
                        .clicked()
                    {
                        self.open_one_way_dialog(OneWayKind::Levene);
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Bartlett Test…"), None, self.data.is_some())
                        .on_hover_text("Likelihood-ratio test of equal variances")
                        .clicked()
                    {
                        self.open_one_way_dialog(OneWayKind::Bartlett);
                        ui.close();
                    }
                });
                help_menu_button(ui, self.catalog().label("DOE"), |ui| {
                    if menu_text_item(ui, self.catalog().label("Create 2-Level Design…"), None, self.data.is_some())
                        .on_hover_text("Full factorial design, replicates and optional center points")
                        .clicked()
                    {
                        self.open_design_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Plackett-Burman Design…"), None, true)
                        .on_hover_text("Screening design with orthogonal factors")
                        .clicked()
                    {
                        self.open_pb_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Box-Behnken Design…"), None, true)
                        .on_hover_text("Three-level response-surface design for 3 to 7 factors")
                        .clicked()
                    {
                        self.open_box_behnken_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("2-Level Factorial…"), None, self.data.is_some())
                        .on_hover_text("Full factorial with coded −1/+1 factors")
                        .clicked()
                    {
                        self.open_doe_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Response Surface…"), None, self.data.is_some())
                        .on_hover_text("Full quadratic model with coded factors")
                        .clicked()
                    {
                        self.open_rsm_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Response Optimizer…"),
                        None,
                        self.data.is_some(),
                    )
                    .on_hover_text("Multi-response desirability over quadratic surfaces")
                    .clicked()
                    {
                        self.open_desirability_dialog();
                        ui.close();
                    }
                });
                help_menu_button(ui, self.catalog().label("Reliability"), |ui| {
                    if menu_text_item(ui, self.catalog().label("Accelerated Life Testing…"), None, self.data.is_some())
                        .on_hover_text("Arrhenius/Weibull fit extrapolated to a use stress")
                        .clicked()
                    {
                        self.open_alt_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Step-Stress ALT…"),
                        None,
                        self.data.is_some(),
                    )
                    .on_hover_text("Cumulative-exposure model for a stress that steps up")
                    .clicked()
                    {
                        self.open_step_stress_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Kaplan–Meier…"), None, self.data.is_some())
                        .on_hover_text("Survival curve with right censoring")
                        .clicked()
                    {
                        self.open_kaplan_meier_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Weibull (censored)…"), None, self.data.is_some())
                        .on_hover_text("Right-censored Weibull maximum likelihood")
                        .clicked()
                    {
                        self.open_weibull_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Exponential (censored)…"), None, self.data.is_some())
                        .on_hover_text("Right-censored exponential maximum likelihood")
                        .clicked()
                    {
                        self.open_exponential_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Lognormal (censored)…"), None, self.data.is_some())
                        .on_hover_text("Right-censored lognormal maximum likelihood")
                        .clicked()
                    {
                        self.open_lognormal_dialog();
                        ui.close();
                    }
                });
                help_menu_button(ui, self.catalog().label("Control Charts"), |ui| {
                    let ready = self.selection_is_numeric();
                    if menu_text_item(ui, self.catalog().label("I–MR…"), None, ready)
                        .on_hover_text("Individuals and moving range chart")
                        .clicked()
                    {
                        if let Some(column) = &self.selected {
                            self.chart_column = column.clone();
                        }
                        self.chart_kind = ControlChartKind::IndividualsMovingRange;
                        self.show_chart_dialog = true;
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("X̄–R…"), None, ready)
                        .on_hover_text("Subgroup mean and range chart")
                        .clicked()
                    {
                        if let Some(column) = &self.selected {
                            self.chart_column = column.clone();
                        }
                        self.chart_kind = ControlChartKind::XbarR;
                        self.show_chart_dialog = true;
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("X̄–S…"), None, ready)
                        .on_hover_text("Subgroup mean and standard deviation chart")
                        .clicked()
                    {
                        if let Some(column) = &self.selected {
                            self.chart_column = column.clone();
                        }
                        self.chart_kind = ControlChartKind::XbarS;
                        self.show_chart_dialog = true;
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("EWMA…"), None, ready)
                        .on_hover_text("Exponentially weighted moving average chart")
                        .clicked()
                    {
                        if let Some(column) = &self.selected {
                            self.chart_column = column.clone();
                        }
                        self.chart_kind = ControlChartKind::Ewma;
                        self.show_chart_dialog = true;
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Run Chart"), None, ready)
                        .on_hover_text("Median center line and runs tests")
                        .clicked()
                    {
                        if let Some(column) = self.selected.clone() {
                            self.run_run_chart(column);
                        }
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("P/NP/C/U Chart…"), None, ready)
                        .on_hover_text("Attribute charts with binomial or Poisson limits")
                        .clicked()
                    {
                        self.open_attribute_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("G Chart (Rare Events)…"), None, ready)
                        .on_hover_text("Control chart for rare events, with the Poisson limits")
                        .clicked()
                    {
                        self.open_g_chart_dialog();
                        ui.close();
                    }
                });
                help_menu_button(ui, self.catalog().label("Quality Tools"), |ui| {
                    let ready = self.selection_is_numeric();
                    if menu_text_item(ui, self.catalog().label("Capability Analysis…"), None, ready)
                        .on_hover_text("Normal indices/CIs or distribution-based PPM; needs specification limits")
                        .clicked()
                    {
                        if let Some(column) = &self.selected {
                            self.cap_column = column.clone();
                        }
                        self.show_cap_dialog = true;
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Distribution Comparison…"), None, ready)
                        .on_hover_text("Normal, Lognormal, Weibull, Exponential, Logistic, Loglogistic, Gamma, Extreme Value, Weibull (3-parameter), Lognormal (3-parameter)")
                        .clicked()
                    {
                        self.run_distribution_comparison();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Interval Plot…"), None, self.data.is_some())
                        .on_hover_text("Level means with confidence intervals")
                        .clicked()
                    {
                        self.open_interval_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Pareto Chart…"), None, self.data.is_some())
                        .on_hover_text("Categories sorted by frequency with the vital few")
                        .clicked()
                    {
                        self.open_pareto_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Gage Linearity…"), None, ready)
                        .on_hover_text("Bias of reference parts regressed on the reference")
                        .clicked()
                    {
                        self.open_linearity_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Attribute Agreement…"), None, ready)
                        .on_hover_text("Cohen's kappa within and between appraisers")
                        .clicked()
                    {
                        self.open_agreement_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Gage R&R…"), None, ready)
                        .on_hover_text("Crossed/nested ANOVA Gage R&R; balanced design")
                        .clicked()
                    {
                        self.open_gage_dialog();
                        ui.close();
                    }
                    if menu_text_item(ui, self.catalog().label("Gage Type 1…"), None, ready)
                        .on_hover_text("Bias and repeatability against a known reference")
                        .clicked()
                    {
                        self.open_type1_dialog();
                        ui.close();
                    }
                });
            });

            help_menu_button(ui, self.catalog().label("Graph"), |ui| {
                let ready = self.selection_is_numeric();
                if menu_icon_item(
                    ui,
                    ToolIcon::Plot,
                    self.catalog().label("Probability Plot…"),
                    None,
                    ready,
                )
                .clicked()
                {
                    self.open_probability_plot_dialog();
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Box,
                    self.catalog().label("Boxplot"),
                    None,
                    ready,
                )
                .on_hover_text("Five-number summary with 1.5×IQR outliers")
                .clicked()
                {
                    if let Some(column) = self.selected.clone() {
                        self.run_boxplot(column);
                    }
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Bars,
                    self.catalog().label("Histogram"),
                    None,
                    ready,
                )
                .on_hover_text("Automatic-bin frequency histogram with Normal overlay")
                .clicked()
                {
                    if let Some(column) = self.selected.clone() {
                        self.run_histogram(column);
                    }
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(ui, ToolIcon::Dots, self.catalog().label("Dotplot…"), None, ready)
                    .on_hover_text("One dot per observation, stacked when values repeat")
                    .clicked()
                {
                    self.open_dotplot_dialog();
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Plot, self.catalog().label("Empirical CDF…"), None, ready)
                    .on_hover_text("Step plot of the cumulative proportion with a DKW band")
                    .clicked()
                {
                    self.open_ecdf_dialog();
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Grid, self.catalog().label("Matrix Plot…"), None, ready)
                    .on_hover_text("Pairwise scatterplots with histograms on the diagonal")
                    .clicked()
                {
                    self.open_matrix_dialog();
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Plot, self.catalog().label("Interval Plot…"), None, ready)
                    .on_hover_text("Level means with 95% intervals")
                    .clicked()
                {
                    self.open_interval_dialog();
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Bars, self.catalog().label("Pareto Chart…"), None, ready)
                    .on_hover_text("Bars sorted by frequency with the cumulative percentage")
                    .clicked()
                {
                    self.open_pareto_dialog();
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Grid, self.catalog().label("Stem-and-Leaf…"), None, ready)
                    .on_hover_text("Text display that keeps every value")
                    .clicked()
                {
                    self.open_stem_dialog();
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Plot, self.catalog().label("Time Series Plot…"), None, ready)
                    .on_hover_text("Values in observation order with a trend test")
                    .clicked()
                {
                    self.open_series_dialog();
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(
                    ui,
                    ToolIcon::Dots,
                    self.catalog().label("Scatterplot…"),
                    None,
                    ready,
                )
                .on_hover_text("Two numeric columns with Pearson r and optional fit")
                .clicked()
                {
                    self.open_scatter_dialog();
                    ui.close();
                }
            });

            help_menu_button(ui, self.catalog().label("Assistant"), |ui| {
                if menu_icon_item(ui, ToolIcon::Info, self.catalog().label("Choose an analysis…"), None, true)
                    .on_hover_text("Guided choice of the analysis for your question")
                    .clicked()
                {
                    self.show_assistant_dialog = true;
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(ui, ToolIcon::Grid, self.catalog().label("Describe my data"), None, self.data.is_some()).clicked()
                {
                    self.run_descriptive_statistics();
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Plot, self.catalog().label("Test normality"), None, self.data.is_some()).clicked()
                {
                    self.open_probability_plot_dialog();
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Plot, self.catalog().label("Control chart"), None, self.data.is_some()).clicked()
                {
                    if let Some(column) = &self.selected {
                        self.chart_column = column.clone();
                    }
                    self.chart_kind = ControlChartKind::IndividualsMovingRange;
                    self.show_chart_dialog = true;
                    ui.close();
                }
                if menu_icon_item(ui, ToolIcon::Plot, self.catalog().label("Process capability"), None, self.data.is_some()).clicked()
                {
                    if let Some(column) = &self.selected {
                        self.cap_column = column.clone();
                    }
                    self.show_cap_dialog = true;
                    ui.close();
                }
            });

            help_menu_button(ui, self.catalog().label("Tools"), |ui| {
                let plugins: Vec<(String, String)> = fp_core::plugin::registry()
                    .lock()
                    .map(|registry| {
                        registry
                            .list()
                            .into_iter()
                            .map(|(id, name, _)| (id.to_owned(), name.to_owned()))
                            .collect()
                    })
                    .unwrap_or_default();
                ui.separator();
                help_menu_button(ui, self.catalog().label("Zoom"), |ui| {
                        for (factor, label) in [
                            (0.75, "75%"),
                            (1.0, "100%"),
                            (1.25, "125%"),
                            (1.5, "150%"),
                            (2.0, "200%"),
                        ] {
                            if ui
                                .selectable_label((self.zoom - factor).abs() < 0.01, label)
                                .on_hover_text("Scale the whole interface")
                                .clicked()
                            {
                                self.zoom = factor;
                                ui.ctx().set_zoom_factor(factor);
                                ui.close();
                            }
                        }
                    },
                );
                ui.checkbox(&mut self.dark_mode, "Dark mode").on_hover_text(checkbox_help("Dark mode"))
                    .on_hover_text("Dark theme for the whole application");
                ui.checkbox(
                    &mut self.native_plot_windows,
                    "Native plot windows",
                )
                .on_hover_text(
                    "Open each graph in its own operating-system window instead of the in-app window",
                );
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    paint_icon(
                        ui.painter(),
                        rect.center(),
                        ToolIcon::Globe,
                        egui::Stroke::new(1.4, ui.visuals().weak_text_color()),
                    );
                    ui.label(self.catalog().label("Language"));
                });
                for (language, label) in [
                    (fp_core::i18n::Lang::English, "English"),
                    (fp_core::i18n::Lang::Portuguese, "Português"),
                ] {
                    if ui
                        .selectable_label(self.language == language, self.catalog().label(label))
                        .clicked()
                    {
                        self.set_language(language);
                        ui.close();
                    }
                }
                if plugins.is_empty() {
                    return;
                }
                ui.separator();
                ui.help_label("Plugins");
                for (id, name) in plugins {
                    if menu_icon_item(ui, ToolIcon::Plug, &name, None, true).clicked() {
                        self.run_plugin(&id);
                        ui.close();
                    }
                }
            });

            help_menu_button(ui, self.catalog().label("Window"), |ui| {
                if menu_icon_item(
                    ui,
                    ToolIcon::Plot,
                    self.catalog().label("Probability Plot"),
                    None,
                    self.plot.is_some(),
                )
                .clicked()
                {
                    self.show_plot = true;
                    ui.close();
                }
            });

            help_menu_button(ui, self.catalog().label("Help"), |ui| {
                if menu_icon_item(
                    ui,
                    ToolIcon::Info,
                    self.catalog().label("Keyboard shortcuts"),
                    None,
                    true,
                )
                .on_hover_text("Every shortcut of the application")
                .clicked()
                {
                    self.show_shortcuts = true;
                    ui.close();
                }
                ui.separator();
                if menu_icon_item(
                    ui,
                    ToolIcon::Info,
                    self.catalog().label("About FreePlots"),
                    None,
                    true,
                )
                .clicked()
                {
                    self.show_about = true;
                    ui.close();
                }
            });
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let ready = self.selection_is_numeric();
            if tool_icon(ui, ToolIcon::Open, "Open CSV", true).clicked() {
                self.begin_open(FileKind::Csv);
            }
            if tool_icon(ui, ToolIcon::Save, "Save CSV", true).clicked() {
                self.begin_save(FileKind::Csv);
            }
            if tool_icon(ui, ToolIcon::Undo, "Undo", true).clicked() {
                self.undo();
            }
            if tool_icon(ui, ToolIcon::Redo, "Redo", true).clicked() {
                self.redo();
            }
            let plot_label = self.catalog().label("Probability Plot");
            if tool_icon(ui, ToolIcon::Plot, plot_label, ready).clicked() {
                self.open_probability_plot_dialog();
            }
            if tool_icon(ui, ToolIcon::Clear, "Clear Session", true).clicked() {
                self.session.clear();
            }
            ui.separator();
            let column_label = ui.help_label("Column:");
            let names: Vec<String> = self
                .data
                .as_ref()
                .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
                .unwrap_or_default();
            let current = self.selected.clone().unwrap_or_else(|| "—".to_owned());
            egui::ComboBox::from_id_salt("fp_column")
                .selected_text(current)
                .width(160.0)
                .show_ui(ui, |ui| {
                    for name in &names {
                        ui.selectable_value(&mut self.selected, Some(name.clone()), name.as_str());
                    }
                })
                .response
                .labelled_by(column_label.id)
                .hover_from(&column_label);

            let distribution_label = ui.help_label("Distribution:");
            egui::ComboBox::from_id_salt("fp_distribution")
                .selected_text(self.distribution.name())
                .width(110.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.distribution, DistributionKind::Normal, "Normal");
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Lognormal,
                        "Lognormal",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Weibull,
                        "Weibull",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Exponential,
                        "Exponential",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Logistic,
                        "Logistic",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Loglogistic,
                        "Loglogistic",
                    );
                    ui.selectable_value(&mut self.distribution, DistributionKind::Gamma, "Gamma");
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::ExtremeValue,
                        "Extreme Value",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Weibull3,
                        "Weibull (3-parameter)",
                    );
                    ui.selectable_value(
                        &mut self.distribution,
                        DistributionKind::Lognormal3,
                        "Lognormal (3-parameter)",
                    );
                })
                .response
                .labelled_by(distribution_label.id)
                .hover_from(&distribution_label);

            ui.separator();
            ui.label(RichText::new(&self.source).italics());
        });
    }

    fn grid_cell(ui: &egui::Ui, color: Color32) {
        let rect = ui.max_rect();
        let stroke = egui::Stroke::new(1.0, color);
        ui.painter().line_segment(
            [
                egui::pos2(rect.right() - 0.5, rect.top()),
                egui::pos2(rect.right() - 0.5, rect.bottom()),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                egui::pos2(rect.left(), rect.bottom() - 0.5),
                egui::pos2(rect.right(), rect.bottom() - 0.5),
            ],
            stroke,
        );
    }

    fn worksheet(&mut self, ui: &mut egui::Ui) {
        if self.data.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("No worksheet open.\nFile → Open CSV…")
                        .size(16.0)
                        .color(ui.visuals().text_color()),
                );
            });
            return;
        }

        let (cell_col, cell_row) = self.selected_cell.unwrap_or((0, 0));
        let mut commit = false;
        ui.horizontal(|ui| {
            ui.strong("fx")
                .on_hover_text("Formula bar: type a value or an expression for the selected cell");
            ui.help_label(format!("C{} R{}", cell_col + 1, cell_row + 1))
                .on_hover_text(format!(
                    "Selected cell: column C{}, row {}. Type in the field, then press Enter or \
                     the tick to write it here.",
                    cell_col + 1,
                    cell_row + 1
                ));
            let response = ui
                .add(
                    egui::TextEdit::singleline(&mut self.edit_buffer)
                        .desired_width((ui.available_width() - 40.0).max(80.0)),
                )
                .on_hover_text("Type a value or =SUM(C1R1:C1R10); Enter commits the result. References are C<column>R<row>");
            self.formula_focused = response.has_focus();
            if response.changed() {
                self.edit_dirty = true;
            }
            if response.lost_focus()
                && (self.edit_dirty || ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                commit = true;
            }
            if apply_icon_button(ui).clicked() {
                commit = true;
            }
        });
        if commit {
            self.commit_cell();
        }
        ui.separator();

        let Some(df) = self.data.as_ref() else {
            return;
        };
        let names: Vec<String> = df.columns().iter().map(|c| c.name().to_owned()).collect();
        let n_rows = df.n_rows();
        // classic worksheets always show spare columns and rows; painting a few blank ones
        // keeps the worksheet a grid instead of a small island of data.
        let display_cols = names.len().max(26);
        let display_rows = n_rows.max(60);
        let selected_cell = self.selected_cell;
        let selection = self.selection_bounds();
        let selection_size = self.selection_size();
        let shift = ui.input(|i| i.modifiers.shift);
        let mut clicked: Option<(usize, usize, bool)> = None;
        let mut clicked_header_column: Option<usize> = None;
        let mut clicked_header_row: Option<usize> = None;
        let mut select_all = false;
        let mut menu_action: Option<GridMenu> = None;
        let grid_lines = self.show_grid_lines;
        let dark = self.dark_mode;
        let cell_bg = if dark {
            Color32::from_rgb(20, 23, 27)
        } else {
            Color32::from_rgb(252, 250, 246)
        };
        let cell_fg = if dark {
            Color32::from_rgb(230, 236, 239)
        } else {
            Color32::from_rgb(34, 39, 43)
        };
        let header_bg = if dark {
            Color32::from_rgb(29, 34, 41)
        } else {
            Color32::from_rgb(244, 241, 234)
        };
        let line = if dark {
            Color32::from_rgb(44, 52, 61)
        } else {
            Color32::from_rgb(218, 212, 201)
        };
        let header_line = if dark {
            Color32::from_rgb(58, 68, 78)
        } else {
            Color32::from_rgb(208, 201, 189)
        };
        let selection_bg = if dark {
            Color32::from_rgb(28, 72, 68)
        } else {
            Color32::from_rgb(223, 242, 237)
        };
        let selected_bg = if dark {
            Color32::from_rgb(36, 92, 86)
        } else {
            Color32::from_rgb(205, 238, 230)
        };
        let row_hl = if dark {
            Color32::from_rgb(35, 41, 49)
        } else {
            Color32::from_rgb(238, 234, 225)
        };
        let muted = if dark {
            Color32::from_rgb(230, 236, 239)
        } else {
            Color32::from_rgb(34, 39, 43)
        };
        let hint = ui.visuals().text_color();
        let column_name = ui.visuals().text_color();
        let inline_cell = self.inline_edit;
        let mut inline_buffer = std::mem::take(&mut self.inline_buffer);
        let mut inline_focus = self.inline_request_focus;
        let mut inline_commit_cell: Option<(usize, usize)> = None;
        let mut inline_cancel = false;
        let mut inline_start: Option<(usize, usize)> = None;

        ui.horizontal(|ui| {
            ui.strong("Worksheet");
            ui.label(
                RichText::new(format!("{} × {} columns", rows_label(n_rows), names.len()))
                    .size(13.5)
                    .color(hint),
            );
            if selection_size.0 * selection_size.1 > 1 {
                ui.label(
                    RichText::new(format!(
                        "{} × {} selected",
                        selection_size.0, selection_size.1
                    ))
                    .size(13.5)
                    .color(hint),
                );
            }
        });
        ui.separator();

        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        let mut new_widths: Option<Vec<f32>> = None;
        egui::ScrollArea::horizontal()
            .id_salt("fp_grid_h")
            .auto_shrink([false, true])
            .show_viewport(ui, |ui, viewport| {
                let widths: Vec<f32> = (0..display_cols)
                    .map(|index| self.column_widths.get(index).copied().unwrap_or(72.0))
                    .collect();
                let total: f32 = widths.iter().sum();
                ui.set_min_width(44.0 + total);
                // Only the columns inside the viewport (plus a margin) are laid
                // out, so a thousand-column sheet costs what a screenful
                // costs. The columns left of the viewport become one spacer
                // column, which keeps the scroll width and the alignment.
                let mut first = 0usize;
                let mut prefix = 0.0;
                while first + 1 < display_cols && prefix + widths[first] < viewport.min.x - 144.0 {
                    prefix += widths[first];
                    first += 1;
                }
                let mut last = first;
                let mut visible_width = 0.0;
                while last < display_cols
                    && (last == first || visible_width < viewport.width() + 288.0)
                {
                    visible_width += widths[last];
                    last += 1;
                }
                let visible = first..last;
                let mut measured = widths.clone();
                let mut table = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .cell_layout(egui::Layout::left_to_right(Align::Center))
                    .column(TableColumn::exact(44.0));
                if first > 0 {
                    table = table.column(TableColumn::exact(prefix).resizable(false));
                }
                for width in &widths[visible.clone()] {
                    table =
                        table.column(TableColumn::initial(*width).at_least(48.0).resizable(true));
                }
                table
                    .header(36.0, |mut header| {
                        header.col(|ui| {
                            ui.painter().rect_filled(ui.max_rect(), 0.0, header_bg);
                            let response = ui
                                .add(
                                    egui::Label::new(RichText::new("#").strong())
                                        .sense(egui::Sense::click()),
                                )
                                .on_hover_text("Select the whole worksheet");
                            if response.clicked() {
                                select_all = true;
                            }
                            if grid_lines {
                                Self::grid_cell(ui, header_line);
                            }
                        });
                        if first > 0 {
                            header.col(|ui| {
                                ui.painter().rect_filled(ui.max_rect(), 0.0, header_bg);
                            });
                        }
                        for index in visible.clone() {
                            header.col(|ui| {
                                measured[index] = ui.max_rect().width();
                                ui.painter().rect_filled(ui.max_rect(), 0.0, header_bg);
                                if let Some(name) = names.get(index) {
                                    ui.vertical(|ui| {
                                        let is_selected = selected_cell
                                            .is_some_and(|(selected, _)| selected == index)
                                            || selection.is_some_and(|(c0, _, c1, _)| {
                                                index >= c0 && index <= c1
                                            });
                                        let header = ui
                                            .selectable_label(
                                                is_selected,
                                                RichText::new(format!("C{}", index + 1)).strong(),
                                            )
                                            .on_hover_text(format!(
                                                "Column C{}: {} — click to select the column",
                                                index + 1,
                                                name
                                            ));
                                        if header.clicked() {
                                            clicked_header_column = Some(index);
                                        }
                                        if header.secondary_clicked() {
                                            clicked_header_column = Some(index);
                                        }
                                        header.context_menu(|ui| {
                                            if ui
                                                .button("Add numeric column")
                                                .on_hover_text(button_help("Add numeric column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::AddColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Add text column")
                                                .on_hover_text(button_help("Add text column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::AddTextColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Add text column")
                                                .on_hover_text(button_help("Add text column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::AddTextColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Delete column")
                                                .on_hover_text(button_help("Delete column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::DeleteColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Rename column…")
                                                .on_hover_text(button_help("Rename column…"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::RenameColumn);
                                                ui.close();
                                            }
                                        });
                                        if name != &format!("C{}", index + 1) {
                                            ui.label(
                                                RichText::new(name).small().color(column_name),
                                            );
                                        }
                                    });
                                } else {
                                    ui.strong(format!("C{}", index + 1));
                                }
                                if grid_lines {
                                    Self::grid_cell(ui, header_line);
                                }
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(20.0, display_rows, |mut row| {
                            let r = row.index();
                            row.col(|ui| {
                                let in_selection =
                                    selection.is_some_and(|(_, r0, _, r1)| r >= r0 && r <= r1);
                                ui.painter().rect_filled(
                                    ui.max_rect(),
                                    0.0,
                                    if in_selection { row_hl } else { cell_bg },
                                );
                                // The whole cell is the target, not just the digits.
                                let rect = ui.max_rect();
                                let response = ui
                                    .interact(
                                        rect,
                                        egui::Id::new(("fp_row", r)),
                                        egui::Sense::click(),
                                    )
                                    .on_hover_text("Select the whole row");
                                ui.painter().text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    (r + 1).to_string(),
                                    egui::FontId::monospace(12.0),
                                    muted,
                                );
                                if response.clicked() {
                                    clicked_header_row = Some(r);
                                }
                                if response.secondary_clicked() {
                                    clicked_header_row = Some(r);
                                }
                                response.context_menu(|ui| {
                                    if ui
                                        .button("Insert row above")
                                        .on_hover_text(button_help("Insert row above"))
                                        .clicked()
                                    {
                                        menu_action = Some(GridMenu::InsertRowAbove);
                                        ui.close();
                                    }
                                    if ui
                                        .button("Insert row below")
                                        .on_hover_text(button_help("Insert row below"))
                                        .clicked()
                                    {
                                        menu_action = Some(GridMenu::InsertRowBelow);
                                        ui.close();
                                    }
                                    if ui
                                        .button("Delete row")
                                        .on_hover_text(button_help("Delete row"))
                                        .clicked()
                                    {
                                        menu_action = Some(GridMenu::DeleteRow);
                                        ui.close();
                                    }
                                });
                                if grid_lines {
                                    Self::grid_cell(ui, line);
                                }
                            });
                            if first > 0 {
                                row.col(|ui| {
                                    let rect = ui.max_rect();
                                    ui.painter()
                                        .with_clip_rect(rect)
                                        .rect_filled(rect, 0.0, cell_bg);
                                });
                            }
                            for index in visible.clone() {
                                row.col(|ui| {
                                    // Paint and hit-test the whole cell, so a
                                    // click anywhere selects it (an empty cell
                                    // has no text to click on).
                                    let rect = ui.max_rect();
                                    let painter = ui.painter().with_clip_rect(rect);
                                    painter.rect_filled(rect, 0.0, cell_bg);
                                    let is_selected = selected_cell == Some((index, r));
                                    let in_selection = selection.is_some_and(|(c0, r0, c1, r1)| {
                                        index >= c0 && index <= c1 && r >= r0 && r <= r1
                                    });
                                    if in_selection {
                                        painter.rect_filled(rect, 0.0, selection_bg);
                                    }
                                    if is_selected {
                                        painter.rect_filled(rect, 0.0, selected_bg);
                                    }
                                    let text = if r < n_rows && index < names.len() {
                                        self.display_cell(df, index, r)
                                    } else {
                                        String::new()
                                    };
                                    let format = names
                                        .get(index)
                                        .and_then(|name| self.formats.get(name))
                                        .copied();
                                    let text = format_cell_text(text, format);
                                    if inline_cell == Some((index, r)) {
                                        let response = ui.add(
                                            egui::TextEdit::singleline(&mut inline_buffer)
                                                .id(egui::Id::new(("fp_inline", index, r)))
                                                .desired_width(ui.available_width()),
                                        );
                                        if inline_focus {
                                            response.request_focus();
                                            inline_focus = false;
                                        }
                                        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                        if escape && response.has_focus() {
                                            inline_cancel = true;
                                        } else if enter && response.has_focus() {
                                            response.surrender_focus();
                                            inline_commit_cell = Some((index, r));
                                        } else if response.lost_focus() {
                                            inline_commit_cell = Some((index, r));
                                        }
                                    } else {
                                        let response = ui
                                            .interact(
                                                rect,
                                                egui::Id::new(("fp_cell", index, r)),
                                                egui::Sense::click(),
                                            )
                                            .on_hover_text(format!(
                                                "C{} R{} — double-click to edit",
                                                index + 1,
                                                r + 1
                                            ));
                                        let (anchor, align) = match format
                                            .map_or(CellAlign::Left, |format| format.align)
                                        {
                                            CellAlign::Left => (
                                                egui::pos2(rect.left() + 6.0, rect.center().y),
                                                egui::Align2::LEFT_CENTER,
                                            ),
                                            CellAlign::Center => (
                                                egui::pos2(rect.center().x, rect.center().y),
                                                egui::Align2::CENTER_CENTER,
                                            ),
                                            CellAlign::Right => (
                                                egui::pos2(rect.right() - 6.0, rect.center().y),
                                                egui::Align2::RIGHT_CENTER,
                                            ),
                                        };
                                        let font = egui::FontId::monospace(12.0);
                                        painter.text(anchor, align, &text, font.clone(), cell_fg);
                                        if format.is_some_and(|format| format.bold) {
                                            // egui has a single weight per family; drawing the
                                            // text twice fakes a bold face.
                                            painter.text(
                                                anchor + egui::vec2(0.6, 0.0),
                                                align,
                                                &text,
                                                font,
                                                cell_fg,
                                            );
                                        }
                                        if response.clicked() && index < names.len() {
                                            clicked = Some((index, r, shift));
                                        }
                                        if response.double_clicked() && index < names.len() {
                                            inline_start = Some((index, r));
                                        }
                                        if response.secondary_clicked()
                                            && index < names.len()
                                            && !in_selection
                                        {
                                            // Right-clicking inside the selection keeps it.
                                            clicked = Some((index, r, false));
                                        }
                                        response.context_menu(|ui| {
                                            if ui
                                                .button("Cut")
                                                .on_hover_text(button_help("Cut"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::Cut);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Copy")
                                                .on_hover_text(button_help("Copy"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::Copy);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Paste")
                                                .on_hover_text(button_help("Paste"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::Paste);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Clear")
                                                .on_hover_text(button_help("Clear"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::Clear);
                                                ui.close();
                                            }
                                            ui.separator();
                                            if ui
                                                .button("Insert row above")
                                                .on_hover_text(button_help("Insert row above"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::InsertRowAbove);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Insert row below")
                                                .on_hover_text(button_help("Insert row below"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::InsertRowBelow);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Delete row")
                                                .on_hover_text(button_help("Delete row"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::DeleteRow);
                                                ui.close();
                                            }
                                            ui.separator();
                                            if ui
                                                .button("Add numeric column")
                                                .on_hover_text(button_help("Add numeric column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::AddColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Delete column")
                                                .on_hover_text(button_help("Delete column"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::DeleteColumn);
                                                ui.close();
                                            }
                                            if ui
                                                .button("Rename column…")
                                                .on_hover_text(button_help("Rename column…"))
                                                .clicked()
                                            {
                                                menu_action = Some(GridMenu::RenameColumn);
                                                ui.close();
                                            }
                                        });
                                    }
                                    if grid_lines {
                                        Self::grid_cell(ui, line);
                                    }
                                });
                            }
                        });
                    });
                if measured.iter().all(|width| *width > 0.0) {
                    new_widths = Some(measured.clone());
                }
            });
        if let Some(widths) = new_widths {
            self.column_widths = widths;
        }

        self.inline_buffer = inline_buffer;
        self.inline_request_focus = inline_focus;
        if let Some(action) = menu_action {
            match action {
                GridMenu::Cut => {
                    if let Some(text) = self.selection_tsv() {
                        self.clipboard = text.clone();
                        ui.ctx().copy_text(text);
                    }
                    self.clear_selection();
                }
                GridMenu::Copy => {
                    if let Some(text) = self.selection_tsv() {
                        self.clipboard = text.clone();
                        ui.ctx().copy_text(text);
                    }
                }
                GridMenu::Paste => {
                    if self.clipboard.is_empty() {
                        self.paste_request = true;
                    } else {
                        self.paste_internal();
                    }
                }
                GridMenu::Clear => self.clear_selection(),
                GridMenu::InsertRowAbove => self.insert_row(),
                GridMenu::InsertRowBelow => self.insert_row_below(),
                GridMenu::DeleteRow => self.delete_row(),
                GridMenu::AddColumn => self.add_column(),
                GridMenu::AddTextColumn => self.add_text_column(),
                GridMenu::DeleteColumn => self.delete_column(),
                GridMenu::RenameColumn => self.begin_rename_column(),
            }
        }
        if select_all {
            self.select_all_cells();
        }
        if let Some(column) = clicked_header_column {
            self.select_column(column);
        }
        if let Some(row) = clicked_header_row {
            self.select_row(row);
        }
        if inline_cancel {
            self.cancel_inline_edit();
        }
        if let Some(cell) = inline_commit_cell {
            self.inline_edit = Some(cell);
            if !self.commit_inline_edit() {
                return;
            }
        }
        if let Some((column, row)) = inline_start {
            self.begin_inline_edit(column, row);
            return;
        }
        if let Some((column, row, extend)) = clicked {
            if self.edit_dirty && !self.commit_cell() {
                return;
            }
            if extend {
                self.extend_selection(column, row);
            } else {
                self.select_cell(column, row);
            }
        }
        self.handle_sheet_keys(ui);
    }

    fn handle_sheet_keys(&mut self, ui: &mut egui::Ui) {
        // Never steal the keystrokes of a focused text field (the formula bar,
        // an inline editor or any dialog field).
        if self.formula_focused
            || self.inline_edit.is_some()
            || ui.ctx().egui_wants_keyboard_input()
        {
            return;
        }
        let Some((column, row)) = self.selected_cell else {
            return;
        };
        let Some(df) = self.data.as_ref() else {
            return;
        };
        let columns = df.n_columns();
        let rows = df.n_rows().max(60);
        let mut next = (column, row);
        let mut clear = false;
        let mut copy = false;
        let mut cut = false;
        let mut paste: Option<String> = None;
        let mut undo = false;
        let mut redo = false;
        let mut extend = false;
        let mut fill_down = false;
        let mut fill_right = false;
        let mut typed: Option<String> = None;
        ui.input(|input| {
            let shift = input.modifiers.shift;
            if shift
                && (input.key_pressed(egui::Key::ArrowLeft)
                    || input.key_pressed(egui::Key::ArrowRight)
                    || input.key_pressed(egui::Key::ArrowUp)
                    || input.key_pressed(egui::Key::ArrowDown))
            {
                extend = true;
            }
            if input.key_pressed(egui::Key::ArrowLeft) {
                next.0 = next.0.saturating_sub(1);
            }
            if input.key_pressed(egui::Key::ArrowRight) {
                next.0 = (next.0 + 1).min(columns.saturating_sub(1));
            }
            if input.key_pressed(egui::Key::ArrowUp) {
                next.1 = next.1.saturating_sub(1);
            }
            if input.key_pressed(egui::Key::ArrowDown) || input.key_pressed(egui::Key::Enter) {
                next.1 += 1;
            }
            if input.key_pressed(egui::Key::Tab) {
                next.0 = (next.0 + 1).min(columns.saturating_sub(1));
            }
            if input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace) {
                clear = true;
            }
            // egui-winit turns Ctrl+C/X/V into Copy/Cut/Paste events and
            // swallows the key events, so the clipboard has to be read from
            // the events themselves.
            for event in &input.events {
                match event {
                    egui::Event::Copy => copy = true,
                    egui::Event::Cut => {
                        copy = true;
                        cut = true;
                    }
                    egui::Event::Paste(text) => paste = Some(text.clone()),
                    _ => {}
                }
            }
            if input.modifiers.command && input.key_pressed(egui::Key::C) {
                copy = true;
            }
            if input.modifiers.command
                && (input.key_pressed(egui::Key::Y)
                    || (input.modifiers.shift && input.key_pressed(egui::Key::Z)))
            {
                redo = true;
            }
            if input.modifiers.command && input.key_pressed(egui::Key::Z) && !input.modifiers.shift
            {
                undo = true;
            }
            if input.modifiers.command && input.key_pressed(egui::Key::D) {
                fill_down = true;
            }
            if input.modifiers.command && input.key_pressed(egui::Key::R) {
                fill_right = true;
            }

            // Excel behaviour: typing over a selected cell starts the
            // edit with that character instead of requiring a double-click.
            if !input.modifiers.command && !input.modifiers.alt {
                for event in &input.events {
                    if let egui::Event::Text(text) = event
                        && !text.is_empty()
                        && text.chars().all(|c| !c.is_control())
                    {
                        typed = Some(text.clone());
                    }
                }
            }
        });
        if fill_down {
            self.fill_down();
            return;
        }
        if fill_right {
            self.fill_right();
            return;
        }
        if undo {
            self.undo();
            return;
        }
        if redo {
            self.redo();
            return;
        }
        if copy && let Some(text) = self.selection_tsv() {
            self.clipboard = text.clone();
            ui.ctx().copy_text(text);
        }
        if cut {
            self.clear_selection();
        }
        if let Some(text) = typed {
            self.begin_inline_edit_with(column, row, &text);
            return;
        }
        if clear {
            self.clear_selected_cell();
            return;
        }
        if let Some(text) = paste {
            self.paste_tsv(&text);
            return;
        }
        next.1 = next.1.min(rows.saturating_sub(1));
        if next != (column, row) {
            if extend {
                self.extend_selection(next.0, next.1);
            } else {
                self.select_cell(next.0, next.1);
            }
        }
    }

    /// Pastes tab-separated text (a copied block) at the selected cell.
    pub fn paste_tsv(&mut self, text: &str) {
        let Some((column, row)) = self.selected_cell else {
            return;
        };
        let lines: Vec<&str> = text.lines().collect();
        if lines.is_empty() {
            return;
        }
        let width = lines
            .iter()
            .map(|line| line.split('\t').count())
            .max()
            .unwrap_or(1);
        let Some(df) = self.data.as_mut() else {
            return;
        };
        let first_new_column = df.n_columns();
        while df.n_columns() < column + width {
            df.add_column(fp_core::data::DType::Float);
        }
        let mut actions: Vec<EditAction> = (first_new_column..df.n_columns())
            .map(|index| add_column_action(df, index))
            .collect();
        let before_rows = df.n_rows();
        if let Err(e) = df.ensure_rows(row + lines.len()) {
            self.set_error(e.to_string());
            return;
        }
        let mut edits = Vec::new();
        for (r, line) in lines.iter().enumerate() {
            for (c, value) in line.split('\t').enumerate() {
                let target_column = column + c;
                let target_row = row + r;
                let before = cell_state(df, target_column, target_row);
                if let Err(e) = df.set_cell(target_column, target_row, value) {
                    self.set_error(e.to_string());
                    self.clear_history();
                    return;
                }
                let after = cell_state(df, target_column, target_row);
                edits.push(CellEdit {
                    column: target_column,
                    row: target_row,
                    before,
                    after,
                    before_rows,
                });
            }
        }
        actions.push(EditAction::Cells(edits));
        self.push_undo(EditGroup { actions });
        self.edit_dirty = false;
        self.error = None;
        self.status = format!("Pasted {} × {} cells", lines.len(), width);
        self.select_cell(column, row);
    }

    /// Writes a set of cells as one undo group, growing the frame if needed.
    fn set_cells(&mut self, cells: &[(usize, usize, String)]) -> bool {
        if cells.is_empty() {
            return false;
        }
        let Some(df) = self.data.as_mut() else {
            return false;
        };
        let before_rows = df.n_rows();
        let needed = cells.iter().map(|(_, row, _)| row + 1).max().unwrap_or(0);
        if needed > before_rows
            && let Err(e) = df.ensure_rows(needed)
        {
            self.set_error(e.to_string());
            return false;
        }
        let mut edits = Vec::with_capacity(cells.len());
        for (column, row, text) in cells {
            let before = cell_state(df, *column, *row);
            if let Err(e) = df.set_cell(*column, *row, text) {
                self.set_error(e.to_string());
                self.clear_history();
                return false;
            }
            let after = cell_state(df, *column, *row);
            edits.push(CellEdit {
                column: *column,
                row: *row,
                before,
                after,
                before_rows,
            });
        }
        self.push_undo(EditGroup {
            actions: vec![EditAction::Cells(edits)],
        });
        self.edit_dirty = false;
        self.error = None;
        true
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

    /// Opens the Find/Replace dialog.
    pub fn open_find_dialog(&mut self) {
        self.show_find_dialog = true;
        self.find_message.clear();
    }

    /// Selects the next cell that matches the find text, wrapping around.
    pub fn find_next(&mut self) {
        let found = {
            let Some(df) = self.data.as_ref() else {
                return;
            };
            let needle = self.find_text.clone();
            if needle.is_empty() {
                None
            } else {
                let columns = df.n_columns();
                let rows = df.n_rows();
                if columns == 0 || rows == 0 {
                    None
                } else {
                    let start = self.selected_cell.unwrap_or((0, 0));
                    let total = columns * rows;
                    let mut found = None;
                    let mut index = start.1.min(rows - 1) * columns + start.0.min(columns - 1);
                    for _ in 0..total {
                        index = (index + 1) % total;
                        let column = index % columns;
                        let row = index / columns;
                        let text = df.cell_text(column, row).unwrap_or_default();
                        if text_matches(&text, &needle, self.find_match_case, self.find_whole_cell)
                        {
                            found = Some((column, row));
                            break;
                        }
                    }
                    found
                }
            }
        };
        match found {
            Some((column, row)) => {
                self.find_message = format!("Found at C{} R{}", column + 1, row + 1);
                self.select_cell(column, row);
            }
            None => {
                self.find_message = if self.find_text.is_empty() {
                    "Type the text to find".to_owned()
                } else {
                    "No matches".to_owned()
                };
            }
        }
    }

    /// Replaces the current cell when it matches, then finds the next one.
    pub fn replace_current(&mut self) {
        let Some(df) = self.data.as_ref() else {
            return;
        };
        let Some((column, row)) = self.selected_cell else {
            return;
        };
        let text = df.cell_text(column, row).unwrap_or_default();
        let replaced = replace_in(
            &text,
            &self.find_text,
            &self.replace_text,
            self.find_match_case,
            self.find_whole_cell,
        );
        match replaced {
            Some(value) => {
                if self.set_cells(&[(column, row, value)]) {
                    self.status = "Replaced".to_owned();
                    self.find_next();
                }
            }
            None => self.find_next(),
        }
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

    /// Opens the Format Cells dialog for the selected column.
    pub fn open_format_dialog(&mut self) {
        let Some(name) = self.selected.clone() else {
            return;
        };
        let format = self.formats.get(&name).copied().unwrap_or_default();
        self.format_decimals = format.decimals;
        self.format_thousands = format.thousands;
        self.format_bold = format.bold;
        self.format_align = format.align;
        self.show_format_dialog = true;
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

    fn session_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Session");
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .small_button("Clear")
                    .on_hover_text("Clear the Session output")
                    .clicked()
                {
                    self.session.clear();
                }
            });
        });
        ui.separator();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.session.is_empty() {
                    ui.label(
                        RichText::new("Analysis output appears here.")
                            .italics()
                            .color(ui.visuals().text_color()),
                    );
                } else {
                    ui.add(
                        egui::Label::new(RichText::new(&self.session).monospace())
                            .wrap_mode(egui::TextWrapMode::Extend),
                    )
                    .on_hover_text("Analysis reports appear here");
                }
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if let Some(error) = &self.error {
                ui.colored_label(Color32::from_rgb(170, 0, 0), error)
                    .on_hover_text(error);
            } else {
                // The column summary lives in the tooltip: on a narrow window
                // it would otherwise run under the view and zoom controls.
                let hover = if self.column_summary.is_empty() {
                    self.status.clone()
                } else {
                    format!("{}\n\n{}", self.status, self.column_summary)
                };
                ui.label(&self.status).on_hover_text(hover);
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                help_menu_button(ui, "...", |ui| {
                    if ui
                        .button("Reset zoom")
                        .on_hover_text(button_help("Reset zoom"))
                        .clicked()
                    {
                        self.zoom = 1.0;
                        ui.close();
                    }
                    ui.checkbox(&mut self.dark_mode, "Dark mode")
                        .on_hover_text(checkbox_help("Dark mode"));
                    ui.checkbox(&mut self.show_grid_lines, "Grid lines")
                        .on_hover_text(checkbox_help("Grid lines"));
                    ui.checkbox(&mut self.show_session, "Session panel")
                        .on_hover_text(checkbox_help("Session panel"));
                    ui.separator();
                    if ui
                        .button("About FreePlots")
                        .on_hover_text(button_help("About FreePlots"))
                        .clicked()
                    {
                        self.show_about = true;
                        ui.close();
                    }
                })
                .on_hover_text("View options");
                if ui
                    .button(RichText::new(">_").monospace())
                    .on_hover_text("Calculator (fx)")
                    .clicked()
                {
                    self.open_calc_dialog();
                }
                ui.label(format!("{:.0}%", self.zoom * 100.0))
                    .on_hover_text("Interface zoom");
                if ui
                    .small_button("+")
                    .on_hover_text("Zoom in (Ctrl +)")
                    .clicked()
                {
                    self.zoom = (self.zoom * 1.1).min(2.5);
                }
                let mut percent = self.zoom * 100.0;
                ui.spacing_mut().slider_width = 80.0;
                if ui
                    .add(egui::Slider::new(&mut percent, 50.0..=250.0).show_value(false))
                    .on_hover_text("Interface zoom")
                    .changed()
                {
                    self.zoom = percent / 100.0;
                }
                if ui
                    .small_button("−")
                    .on_hover_text("Zoom out (Ctrl −)")
                    .clicked()
                {
                    self.zoom = (self.zoom / 1.1).max(0.5);
                }
                if view_icon_button(
                    ui,
                    ViewIcon::Panel,
                    self.show_session,
                    "Show or hide the Session panel",
                )
                .clicked()
                {
                    self.show_session = !self.show_session;
                }
                if view_icon_button(
                    ui,
                    ViewIcon::Grid,
                    self.show_grid_lines,
                    "Show or hide the grid lines",
                )
                .clicked()
                {
                    self.show_grid_lines = !self.show_grid_lines;
                }
                let _ = view_icon_button(ui, ViewIcon::Worksheet, true, "Worksheet view");
                if let Some(df) = &self.data {
                    ui.help_label(format!(
                        "{} × {} columns",
                        rows_label(df.n_rows()),
                        df.n_columns()
                    ))
                    .on_hover_text("Worksheet size");
                }
            });
        });
    }

    fn capability_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_cap_dialog {
            return;
        }
        let columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|c| matches!(c.dtype(), DType::Float | DType::Integer))
                    .map(|c| c.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();

        let mut open = self.show_cap_dialog;
        let mut close = false;
        let mut pending: Option<AnalysisSpec> = None;
        dialog_window(self.catalog().label("Capability Analysis"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("cap_column")
                        .selected_text(if self.cap_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.cap_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &columns {
                                ui.selectable_value(
                                    &mut self.cap_column,
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
                    let label = ui.help_label("Distribution:");
                    egui::ComboBox::from_id_salt("cap_distribution")
                        .selected_text(self.cap_distribution.name())
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for distribution in [
                                DistributionKind::Normal,
                                DistributionKind::Lognormal,
                                DistributionKind::Weibull,
                                DistributionKind::Exponential,
                                DistributionKind::Logistic,
                                DistributionKind::Loglogistic,
                                DistributionKind::Gamma,
                                DistributionKind::ExtremeValue,
                                DistributionKind::Weibull3,
                                DistributionKind::Lognormal3,
                            ] {
                                ui.selectable_value(
                                    &mut self.cap_distribution,
                                    distribution,
                                    distribution.name(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let lsl = ui.help_label("LSL:");
                    ui.text_edit_singleline(&mut self.cap_lsl)
                        .labelled_by(lsl.id)
                        .hover_from(&lsl);
                    let usl = ui.help_label("USL:");
                    ui.text_edit_singleline(&mut self.cap_usl)
                        .labelled_by(usl.id)
                        .hover_from(&usl);
                });
                ui.horizontal(|ui| {
                    let target = ui.help_label("Target:");
                    ui.text_edit_singleline(&mut self.cap_target)
                        .labelled_by(target.id)
                        .hover_from(&target);
                    ui.small("empty = midpoint for Cpm");
                });
                ui.horizontal(|ui| {
                    let subgroup = ui.help_label("Subgroup size:");
                    ui.add(egui::DragValue::new(&mut self.cap_subgroup).range(1..=100))
                        .labelled_by(subgroup.id)
                        .hover_from(&subgroup);
                    let confidence = ui.help_label("Confidence:");
                    ui.add(
                        egui::DragValue::new(&mut self.cap_confidence)
                            .range(50.0..=99.9)
                            .speed(0.5)
                            .suffix(" %"),
                    )
                    .labelled_by(confidence.id)
                    .hover_from(&confidence);
                });
                ui.horizontal(|ui| {
                    ui.add_enabled(
                        self.cap_distribution == DistributionKind::Normal,
                        egui::Checkbox::new(&mut self.cap_box_cox, "Box-Cox transformation"),
                    )
                    .on_hover_text(checkbox_help("Box-Cox transformation"));
                    ui.small("positive data; Normal model only");
                });
                ui.help_checkbox(
                    &mut self.cap_require_stability,
                    "Require a stable I–MR screen",
                );
                ui.horizontal(|ui| {
                    let label = ui.help_label("Bootstrap reps:");
                    ui.add(
                        egui::DragValue::new(&mut self.cap_bootstrap)
                            .range(0..=2000)
                            .speed(10),
                    )
                    .labelled_by(label.id)
                    .hover_from(&label);
                    ui.small("0 = auto for non-normal PPM");
                });
                if self.cap_distribution != DistributionKind::Normal {
                    ui.small(
                        "Non-normal capability fits by MLE and reports expected/observed PPM; \
                         subgroup, confidence and Cpm target apply only to the normal model.",
                    );
                }
                ui.small("At least one specification limit is required.");
                ui.separator();
                ui.horizontal(|ui| {
                    let ready = !self.cap_column.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new(self.catalog().label("OK")))
                        .clicked()
                    {
                        let parse = |text: &str| -> Result<Option<f64>, String> {
                            let text = text.trim();
                            if text.is_empty() {
                                return Ok(None);
                            }
                            text.parse::<f64>()
                                .map(Some)
                                .map_err(|_| format!("invalid limit: {text}"))
                        };
                        match (
                            parse(&self.cap_lsl),
                            parse(&self.cap_usl),
                            parse(&self.cap_target),
                        ) {
                            (Ok(lsl), Ok(usl), Ok(target)) => {
                                if lsl.is_none() && usl.is_none() {
                                    self.set_error(
                                        "Capability needs at least one specification limit",
                                    );
                                } else {
                                    let mut spec =
                                        AnalysisSpec::capability(self.cap_column.clone(), lsl, usl)
                                            .with_capability_distribution(self.cap_distribution)
                                            .with_capability_target(target)
                                            .with_capability_bootstrap(self.cap_bootstrap)
                                            .with_capability_box_cox(
                                                self.cap_box_cox
                                                    && self.cap_distribution
                                                        == DistributionKind::Normal,
                                            )
                                            .with_capability_stability(self.cap_require_stability);
                                    if let AnalysisSpec::Capability {
                                        subgroup: size,
                                        confidence,
                                        ..
                                    } = &mut spec
                                    {
                                        *size = self.cap_subgroup;
                                        *confidence = self.cap_confidence / 100.0;
                                    }
                                    pending = Some(spec);
                                    close = true;
                                }
                            }
                            (Err(error), _, _) | (_, Err(error), _) | (_, _, Err(error)) => {
                                self.set_error(error)
                            }
                        }
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_cap_dialog = open && !close;
        if let Some(spec) = pending {
            let label = format!("Capability: {}", spec.column());
            self.execute_spec(spec, label);
        }
    }

    fn describe_by_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_describe_dialog {
            return;
        }
        let all_columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let numeric: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|c| matches!(c.dtype(), DType::Float | DType::Integer))
                    .map(|c| c.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let mut open = self.show_describe_dialog;
        let mut close = false;
        let mut run = false;
        dialog_window(self.catalog().label("Descriptive Statistics by Column"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Numeric column:");
                    egui::ComboBox::from_id_salt("describe_column")
                        .selected_text(if self.describe_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.describe_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &numeric {
                                ui.selectable_value(
                                    &mut self.describe_column,
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
                    let label = ui.help_label("By column:");
                    egui::ComboBox::from_id_salt("describe_by")
                        .selected_text(self.describe_by.clone().unwrap_or_else(|| "—".to_owned()))
                        .show_ui(ui, |ui| {
                            for name in &all_columns {
                                ui.selectable_value(
                                    &mut self.describe_by,
                                    Some(name.clone()),
                                    name.as_str(),
                                );
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small("One statistics block is produced for each group level.");
                ui.separator();
                ui.horizontal(|ui| {
                    let ready = !self.describe_column.is_empty() && self.describe_by.is_some();
                    if ui
                        .add_enabled(ready, egui::Button::new(self.catalog().label("OK")))
                        .clicked()
                    {
                        run = true;
                        close = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_describe_dialog = open && !close;
        if run {
            self.run_describe_by_dialog();
        }
    }

    /// Opens the grouped descriptive statistics dialog.
    pub fn open_describe_by_dialog(&mut self) {
        if let Some(column) = &self.selected {
            self.describe_column = column.clone();
        }
        self.describe_by = None;
        self.show_describe_dialog = true;
    }

    /// Runs the grouped descriptive statistics with the options of the dialog.
    pub fn run_describe_by_dialog(&mut self) {
        if self.describe_column.is_empty() {
            self.set_error("Select the numeric column");
            return;
        }
        let Some(by) = self.describe_by.clone() else {
            self.set_error("Select the grouping column");
            return;
        };
        if by == self.describe_column {
            self.set_error("The grouping column must be different");
            return;
        }
        self.run_descriptive_statistics_by(self.describe_column.clone(), by);
        self.show_describe_dialog = false;
    }

    fn control_chart_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_chart_dialog {
            return;
        }
        let columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|c| matches!(c.dtype(), DType::Float | DType::Integer))
                    .map(|c| c.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        let all_columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let mut open = self.show_chart_dialog;
        let mut close = false;
        let mut pending: Option<(
            String,
            ControlChartKind,
            usize,
            TestSet,
            fp_core::spc::CusumOptions,
            fp_core::spc::EwmaOptions,
        )> = None;
        dialog_window(self.catalog().label("Control Chart"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Column:");
                    egui::ComboBox::from_id_salt("chart_column")
                        .selected_text(if self.chart_column.is_empty() {
                            "—".to_owned()
                        } else {
                            self.chart_column.clone()
                        })
                        .show_ui(ui, |ui| {
                            for name in &columns {
                                ui.selectable_value(
                                    &mut self.chart_column,
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
                    let label = ui.help_label("Chart:");
                    egui::ComboBox::from_id_salt("chart_kind")
                        .selected_text(self.chart_kind.name())
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for kind in [
                                ControlChartKind::IndividualsMovingRange,
                                ControlChartKind::XbarR,
                                ControlChartKind::XbarS,
                                ControlChartKind::Ewma,
                                ControlChartKind::Cusum,
                            ] {
                                ui.selectable_value(&mut self.chart_kind, kind, kind.name());
                            }
                        })
                        .response
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Subgroup column:");
                    let enabled = matches!(
                        self.chart_kind,
                        ControlChartKind::XbarR | ControlChartKind::XbarS
                    );
                    ui.add_enabled_ui(enabled, |ui| {
                        egui::ComboBox::from_id_salt("chart_subgroup_column")
                            .selected_text(if self.chart_subgroup_column.is_empty() {
                                "None".to_owned()
                            } else {
                                self.chart_subgroup_column.clone()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.chart_subgroup_column,
                                    String::new(),
                                    "None",
                                );
                                for name in &all_columns {
                                    ui.selectable_value(
                                        &mut self.chart_subgroup_column,
                                        name.clone(),
                                        name.as_str(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .hover_from(&label);
                    });
                });
                ui.horizontal(|ui| {
                    let cusum = self.chart_kind == ControlChartKind::Cusum;
                    let enabled = (matches!(
                        self.chart_kind,
                        ControlChartKind::XbarR | ControlChartKind::XbarS
                    ) && self.chart_subgroup_column.is_empty())
                        || cusum;
                    let mut value = egui::DragValue::new(&mut self.chart_subgroup);
                    value = if cusum {
                        value.range(1..=25)
                    } else {
                        value.range(2..=25)
                    };
                    let label = ui.help_label("Subgroup size:");
                    ui.add_enabled(enabled, value)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.small(
                    "The CUSUM chart uses the individuals when the subgroup size is 1 and the \
                     subgroup means otherwise.",
                );
                if self.chart_kind == ControlChartKind::Cusum {
                    ui.horizontal(|ui| {
                        let k = ui.help_label("k:");
                        ui.add(
                            egui::DragValue::new(&mut self.chart_cusum_k)
                                .range(0.05..=5.0)
                                .speed(0.05),
                        )
                        .labelled_by(k.id)
                        .hover_from(&k);
                        let h = ui.help_label("h:");
                        ui.add(
                            egui::DragValue::new(&mut self.chart_cusum_h)
                                .range(0.5..=20.0)
                                .speed(0.1),
                        )
                        .labelled_by(h.id)
                        .hover_from(&h);
                        ui.help_label("σ");
                    });
                }
                if self.chart_kind == ControlChartKind::Ewma {
                    ui.horizontal(|ui| {
                        let lambda = ui.help_label("λ:");
                        ui.add(
                            egui::DragValue::new(&mut self.chart_ewma_lambda)
                                .range(0.01..=1.0)
                                .speed(0.01),
                        )
                        .labelled_by(lambda.id)
                        .hover_from(&lambda);
                        let limit = ui.help_label("L:");
                        ui.add(
                            egui::DragValue::new(&mut self.chart_ewma_limit)
                                .range(0.5..=10.0)
                                .speed(0.1),
                        )
                        .labelled_by(limit.id)
                        .hover_from(&limit);
                        ui.help_label("σ");
                    });
                }
                ui.small("Control limits use the published Shewhart constants.");
                ui.separator();
                ui.help_label("Tests for special causes:");
                let descriptions = [
                    "1  point outside a control limit",
                    "2  K points in a row on the same side of the center line",
                    "3  K points in a row, all increasing or all decreasing",
                    "4  K points in a row, alternating up and down",
                    "5  K out of K+1 points more than 2σ from the center line, same side",
                    "6  K out of K+1 points more than 1σ from the center line, same side",
                    "7  K points in a row within 1σ of the center line",
                    "8  K points in a row more than 1σ from the center line",
                ];
                for row in 0..2 {
                    ui.horizontal(|ui| {
                        for column in 0..4 {
                            let test = (row * 4 + column + 1) as u8;
                            let mut enabled = self.chart_tests.is_enabled(test);
                            if ui
                                .checkbox(&mut enabled, test.to_string())
                                .on_hover_text(descriptions[(test - 1) as usize])
                                .changed()
                                && let Ok(updated) = self.chart_tests.set_enabled(test, enabled)
                            {
                                self.chart_tests = updated;
                            }
                        }
                    });
                }
                egui::CollapsingHeader::new("Sensitivity (DEFTEST)")
                    .id_salt("chart_sensitivity")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for test in 1..=8u8 {
                                let label = ui.help_label(format!("K{test}"));
                                let mut value = i32::from(self.chart_tests.sensitivity(test));
                                let response = ui.add_enabled(
                                    test != 1 && self.chart_tests.is_enabled(test),
                                    egui::DragValue::new(&mut value).range(1..=15).speed(0.1),
                                );
                                if response.changed()
                                    && let Ok(updated) =
                                        self.chart_tests.with_sensitivity(test, value as u8)
                                {
                                    self.chart_tests = updated;
                                }
                                response.labelled_by(label.id).hover_from(&label);
                            }
                        });
                        ui.small(
                            "One K per test, in test order. Test 1 is fixed at three sigma \
                             because that is where the limits are drawn.",
                        );
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    let ready = !self.chart_column.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new(self.catalog().label("OK")))
                        .clicked()
                    {
                        let subgroup = if matches!(
                            self.chart_kind,
                            ControlChartKind::IndividualsMovingRange | ControlChartKind::Ewma
                        ) {
                            1
                        } else {
                            self.chart_subgroup
                        };
                        let cusum =
                            fp_core::spc::CusumOptions::new(self.chart_cusum_k, self.chart_cusum_h)
                                .unwrap_or_default();
                        let ewma = fp_core::spc::EwmaOptions::new(
                            self.chart_ewma_lambda,
                            self.chart_ewma_limit,
                        )
                        .unwrap_or_default();
                        pending = Some((
                            self.chart_column.clone(),
                            self.chart_kind,
                            subgroup,
                            self.chart_tests,
                            cusum,
                            ewma,
                        ));
                        close = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_chart_dialog = open && !close;
        if let Some((column, kind, subgroup, tests, cusum, ewma)) = pending {
            self.run_control_chart_with_options(column, kind, subgroup, tests, cusum, ewma);
        }
    }

    fn probability_plot_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_probability_plot_dialog {
            return;
        }
        let all_columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| df.columns().iter().map(|c| c.name().to_owned()).collect())
            .unwrap_or_default();
        let columns: Vec<String> = self
            .data
            .as_ref()
            .map(|df| {
                df.columns()
                    .iter()
                    .filter(|c| matches!(c.dtype(), DType::Float | DType::Integer))
                    .map(|c| c.name().to_owned())
                    .collect()
            })
            .unwrap_or_default();

        let mut open = self.show_probability_plot_dialog;
        let mut close = false;
        let mut run = false;
        dialog_window(self.catalog().label("Probability Plot"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
                ui.help_label("Y variables:");
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(150.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = 4.0;
                                for name in &columns {
                                    let mut checked = self.dialog_columns.contains(name);
                                    if ui.help_checkbox(&mut checked, name).changed() {
                                        if checked {
                                            self.dialog_columns.push(name.clone());
                                        } else {
                                            self.dialog_columns.retain(|item| item != name);
                                        }
                                    }
                                }
                            });
                    });
                ui.add_space(4.0);

                egui::Grid::new("probability_plot_options")
                    .num_columns(2)
                    .spacing([14.0, 10.0])
                    .show(ui, |ui| {
                        let layout_label = ui.help_label("Layout:");
                        let multi = self.dialog_columns.len() > 1;
                        ui.vertical(|ui| {
                            ui.add_enabled_ui(multi, |ui| {
                                egui::ComboBox::from_id_salt("probability_plot_layout")
                                    .width(230.0)
                                    .selected_text(self.dialog_layout.name())
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.dialog_layout,
                                            PlotLayout::Separate,
                                            PlotLayout::Separate.name(),
                                        );
                                        ui.selectable_value(
                                            &mut self.dialog_layout,
                                            PlotLayout::Overlay,
                                            PlotLayout::Overlay.name(),
                                        );
                                    })
                                    .response
                                    .labelled_by(layout_label.id).hover_from(&layout_label);
                            });
                            ui.add_enabled_ui(multi, |ui| {
                                ui.horizontal(|ui| {
                                    ui.help_checkbox(&mut self.dialog_same_x, "Same X scale");
                                    ui.help_checkbox(&mut self.dialog_same_y, "Same Y scale");
                                });
                            });
                        });
                        ui.end_row();

                        let group_label = ui.help_label("Group:");
                        let single = self.dialog_columns.len() == 1;
                        ui.vertical(|ui| {
                            ui.add_enabled_ui(single, |ui| {
                                egui::ComboBox::from_id_salt("probability_plot_group")
                                    .width(230.0)
                                    .selected_text(
                                        self.dialog_group
                                            .as_deref()
                                            .filter(|_| single)
                                            .unwrap_or("None"),
                                    )
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut self.dialog_group, None, "None");
                                        for name in &all_columns {
                                            ui.selectable_value(
                                                &mut self.dialog_group,
                                                Some(name.clone()),
                                                name.as_str(),
                                            );
                                        }
                                    })
                                    .response
                                    .labelled_by(group_label.id).hover_from(&group_label);
                            });
                            if !single {
                                ui.small("Grouping applies to a single Y variable.");
                            }
                        });
                        ui.end_row();

                        let distribution_label = ui.help_label("Distribution:");
                        egui::ComboBox::from_id_salt("probability_plot_distribution")
                            .width(230.0)
                            .selected_text(self.dialog_distribution.name())
                            .show_ui(ui, |ui| {
                                for (kind, name) in [
                                    (DistributionKind::Normal, "Normal"),
                                    (DistributionKind::Lognormal, "Lognormal"),
                                    (DistributionKind::Weibull, "Weibull"),
                                    (DistributionKind::Exponential, "Exponential"),
                                    (DistributionKind::Logistic, "Logistic"),
                                    (DistributionKind::Loglogistic, "Loglogistic"),
                                ] {
                                    ui.selectable_value(&mut self.dialog_distribution, kind, name);
                                }
                            })
                            .response
                            .labelled_by(distribution_label.id).hover_from(&distribution_label);
                        ui.end_row();

                        let display_label = ui.help_label("Display:");
                        egui::ComboBox::from_id_salt("probability_plot_display")
                            .width(230.0)
                            .selected_text(self.dialog_display.name())
                            .show_ui(ui, |ui| {
                                for display in
                                    [PlotDisplay::Both, PlotDisplay::Symbols, PlotDisplay::Fit]
                                {
                                    ui.selectable_value(
                                        &mut self.dialog_display,
                                        display,
                                        display.name(),
                                    );
                                }
                            })
                            .response
                            .labelled_by(display_label.id).hover_from(&display_label);
                        ui.end_row();

                        let band_available = matches!(
                            self.dialog_distribution,
                            DistributionKind::Normal | DistributionKind::Lognormal
                        );
                        let ci_enabled = band_available && self.dialog_display != PlotDisplay::Symbols;
                        let _ = ui.help_label("Confidence:");
                        ui.vertical(|ui| {
                            ui.add_enabled_ui(ci_enabled, |ui| {
                                ui.help_checkbox(&mut self.dialog_ci, "Confidence band");
                                if self.dialog_ci {
                                    ui.horizontal(|ui| {
                                        let label = ui.help_label("Level:");
                                        ui.add(
                                            egui::DragValue::new(&mut self.dialog_ci_level)
                                                .range(1.0..=99.9)
                                                .speed(0.1)
                                                .suffix(" %"),
                                        )
                                        .labelled_by(label.id).hover_from(&label);
                                    });
                                }
                            });
                            if !band_available {
                                ui.small(
                                    "Band not recovered for this distribution (the unrecovered engine layer gap).",
                                );
                            } else if self.dialog_display == PlotDisplay::Symbols {
                                ui.small("CI is suppressed when only symbols are drawn.");
                            }
                        });
                        ui.end_row();
                    });

                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ready = !self.dialog_columns.is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new(self.catalog().label("OK")))
                        .clicked()
                    {
                        run = true;
                        close = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        close = true;
                    }
                });
            });
        self.show_probability_plot_dialog = open && !close;
        if run {
            self.run_probability_plot_dialog();
        }
    }

    /// Runs the probability plot with the options of the dialog.
    pub fn run_probability_plot_dialog(&mut self) {
        if self.dialog_columns.is_empty() {
            self.set_error("Select at least one Y variable");
            return;
        }
        let band_available = matches!(
            self.dialog_distribution,
            DistributionKind::Normal | DistributionKind::Lognormal
        );
        let ci_enabled = band_available && self.dialog_display != PlotDisplay::Symbols;
        let level = (self.dialog_ci_level / 100.0).clamp(0.001, 0.999);
        let confidence = if self.dialog_ci && ci_enabled {
            Some(level)
        } else {
            None
        };
        let mut spec = AnalysisSpec::probability_plot_multi(self.dialog_columns.clone())
            .with_distribution(self.dialog_distribution)
            .with_display(self.dialog_display)
            .with_layout(self.dialog_layout)
            .with_same_scale(self.dialog_same_x, self.dialog_same_y)
            .with_confidence(confidence);
        if self.dialog_columns.len() == 1
            && let Some(group) = &self.dialog_group
        {
            spec = spec.with_group(group.clone());
        }
        self.distribution = self.dialog_distribution;
        let label = format!("Probability plot: {}", spec.column());
        self.show_probability_plot_dialog = false;
        self.execute_spec(spec, label);
    }

    fn csv_options_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_csv_options {
            return;
        }
        let mut open = self.show_csv_options;
        let mut confirm = false;
        let mut cancel = false;
        let preview = self.csv_preview.clone();
        dialog_window(self.catalog().label("CSV Options"))
            .open(&mut open)
            .resizable(false)
            .default_width(560.0)
            .show(ctx, |ui| {
                ui.help_label(format!("File: {}", self.csv_path));
                if let Some(preview) = &preview {
                    ui.help_label(format!("Detected encoding: {}", preview.encoding));
                    egui::Frame::group(ui.style())
                        .inner_margin(egui::Margin::same(6))
                        .show(ui, |ui| {
                            egui::ScrollArea::both()
                                .max_height(160.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&preview.text).monospace().small(),
                                        )
                                        .wrap_mode(egui::TextWrapMode::Extend),
                                    );
                                });
                        });
                }
                ui.add_space(4.0);
                egui::Grid::new("csv_options_grid")
                    .num_columns(2)
                    .spacing([14.0, 10.0])
                    .show(ui, |ui| {
                        let delimiter_label = ui.help_label("Delimiter:");
                        egui::ComboBox::from_id_salt("csv_delimiter")
                            .width(160.0)
                            .selected_text(match self.csv_delimiter {
                                b';' => "Semicolon (;)",
                                b'\t' => "Tab",
                                _ => "Comma (,)",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.csv_delimiter, b',', "Comma (,)");
                                ui.selectable_value(&mut self.csv_delimiter, b';', "Semicolon (;)");
                                ui.selectable_value(&mut self.csv_delimiter, b'\t', "Tab");
                            })
                            .response
                            .labelled_by(delimiter_label.id)
                            .hover_from(&delimiter_label);
                        ui.end_row();
                        let _ = ui.help_label("Numbers:");
                        ui.help_checkbox(
                            &mut self.csv_decimal_comma,
                            "Comma is the decimal separator",
                        );
                        ui.end_row();
                        let _ = ui.help_label("Header:");
                        ui.help_checkbox(&mut self.csv_header, "First row is the header");
                        ui.end_row();
                    });
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.help_button(self.catalog().label("Open")).clicked() {
                        confirm = true;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        self.show_csv_options = open && !confirm && !cancel;
        if confirm {
            self.open_request = Some(OpenRequest {
                path: self.csv_path.clone(),
                kind: FileKind::Csv,
                sheet: None,
                csv_options: Some(CsvOptions {
                    delimiter: self.csv_delimiter,
                    decimal_comma: self.csv_decimal_comma,
                    has_header: self.csv_header,
                }),
            });
            self.show_csv_options = false;
        }
        if cancel {
            self.show_csv_options = false;
        }
    }

    fn open_window(&mut self, ctx: &egui::Context) {
        if !self.show_open {
            return;
        }
        let mut open = self.show_open;
        let kind = self.open_kind;
        let title = match kind {
            FileKind::Csv => "Open CSV",
            FileKind::Project => "Open Project",
            FileKind::Html => "Open HTML",
            FileKind::Pdf => "Open PDF",
            FileKind::Xlsx => "Open Excel",
        };
        dialog_window(title)
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label(match kind {
                    FileKind::Csv => "Path to a CSV file:",
                    FileKind::Project => "Path to a .fpp project:",
                    FileKind::Html => "Path to an HTML file:",
                    FileKind::Pdf => "Path to a PDF file:",
                    FileKind::Xlsx => "Path to an Excel workbook:",
                });
                ui.horizontal(|ui| {
                    let response = ui
                        .text_edit_singleline(&mut self.open_path)
                        .on_hover_text("Path of the file to open; Browse… opens the system dialog");
                    let submitted =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.help_button(self.catalog().label("Browse…")).clicked() {
                        self.request_browse(kind, false);
                    }
                    if ui.help_button(self.catalog().label("Open")).clicked() || submitted {
                        let path = self.open_path.trim();
                        if !path.is_empty() {
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
                        }
                        self.show_open = false;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_open = false;
                    }
                });
                ui.small(
                    "Browse opens the native file dialog of the operating system; the file itself \
                     is read by the desktop runner, not by the UI crate.",
                );
            });
        self.show_open = open && self.show_open;
    }

    fn save_window(&mut self, ctx: &egui::Context) {
        if !self.show_save {
            return;
        }
        let mut open = self.show_save;
        let kind = self.save_kind;
        let title = match kind {
            FileKind::Csv => "Save CSV As",
            FileKind::Project => "Save Project As",
            FileKind::Html => "Export Session HTML",
            FileKind::Pdf => "Export Session PDF",
            FileKind::Xlsx => "Save Excel Workbook",
        };
        dialog_window(title)
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.label(match kind {
                    FileKind::Csv => "Destination CSV file:",
                    FileKind::Project => "Destination .fpp project:",
                    FileKind::Html => "Destination HTML file:",
                    FileKind::Pdf => "Destination PDF file:",
                    FileKind::Xlsx => "Destination Excel workbook:",
                });
                ui.horizontal(|ui| {
                    let response = ui
                        .text_edit_singleline(&mut self.save_path)
                        .on_hover_text("Destination file; Browse… opens the system dialog");
                    let submitted =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if ui.help_button(self.catalog().label("Browse…")).clicked() {
                        self.request_browse(kind, true);
                    }
                    if kind == FileKind::Csv {
                        ui.help_checkbox(
                            &mut self.save_decimal_comma,
                            "Portuguese Excel format (`;` and decimal comma)",
                        );
                    }
                    if ui.help_button(self.catalog().label("Save")).clicked() || submitted {
                        let path = self.save_path.trim();
                        if !path.is_empty() {
                            self.save_request = Some(SaveRequest {
                                path: path.to_owned(),
                                kind,
                                decimal_comma: self.save_decimal_comma,
                            });
                        }
                        self.show_save = false;
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_save = false;
                    }
                });
                ui.small(
                    "Browse opens the native file dialog of the operating system; the file itself \
                     is written by the desktop runner, not by the UI crate.",
                );
            });
        self.show_save = open && self.show_save;
    }

    fn sheet_rename_window(&mut self, ctx: &egui::Context) {
        if !self.show_sheet_rename {
            return;
        }
        let mut open = self.show_sheet_rename;
        dialog_window(self.catalog().label("Rename Worksheet…"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                let label = ui.help_label("New name:");
                let response = ui
                    .text_edit_singleline(&mut self.sheet_rename_text)
                    .labelled_by(label.id)
                    .hover_from(&label);
                let submitted =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Rename")).clicked() || submitted {
                        self.commit_sheet_rename();
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_sheet_rename = false;
                    }
                });
            });
        self.show_sheet_rename = open && self.show_sheet_rename;
    }

    fn rename_window(&mut self, ctx: &egui::Context) {
        if !self.show_rename {
            return;
        }
        let mut open = self.show_rename;
        dialog_window(self.catalog().label("Rename Column"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                let label = ui.help_label("New name:");
                let response = ui
                    .text_edit_singleline(&mut self.rename_text)
                    .labelled_by(label.id)
                    .hover_from(&label);
                let submitted =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.help_button(self.catalog().label("Rename")).clicked() || submitted {
                        self.commit_rename_column();
                    }
                    if ui.help_button(self.catalog().label("Cancel")).clicked() {
                        self.show_rename = false;
                    }
                });
            });
        self.show_rename = open && self.show_rename;
    }

    fn find_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_find_dialog {
            return;
        }
        let mut open = self.show_find_dialog;
        let mut close = false;
        let mut next = false;
        let mut replace = false;
        let mut replace_all = false;
        let mut submitted = false;
        dialog_window(self.catalog().label("Find and Replace"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.help_label("Find what:");
                    let response = ui
                        .text_edit_singleline(&mut self.find_text)
                        .labelled_by(label.id)
                        .hover_from(&label);
                    submitted =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                });
                ui.horizontal(|ui| {
                    let label = ui.help_label("Replace with:");
                    ui.text_edit_singleline(&mut self.replace_text)
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_checkbox(&mut self.find_match_case, "Match case");
                ui.help_checkbox(&mut self.find_whole_cell, "Match the whole cell");
                if !self.find_message.is_empty() {
                    ui.small(&self.find_message);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let ready = !self.find_text.trim().is_empty();
                    if ui
                        .add_enabled(ready, egui::Button::new("Find Next"))
                        .on_hover_text("Select the next cell that matches")
                        .clicked()
                    {
                        next = true;
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Replace"))
                        .on_hover_text("Replace this cell, then find the next match")
                        .clicked()
                    {
                        replace = true;
                    }
                    if ui
                        .add_enabled(ready, egui::Button::new("Replace All"))
                        .on_hover_text("Replace every match in this worksheet")
                        .clicked()
                    {
                        replace_all = true;
                    }
                    if ui
                        .help_button("Close")
                        .on_hover_text("Close without replacing")
                        .clicked()
                    {
                        close = true;
                    }
                });
            });
        self.show_find_dialog = open && !close;
        if next || submitted {
            self.find_next();
        }
        if replace {
            self.replace_current();
        }
        if replace_all {
            self.replace_all();
        }
    }

    fn format_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_format_dialog {
            return;
        }
        let mut open = self.show_format_dialog;
        let mut close = false;
        let mut apply = false;
        let mut remove = false;
        let column = self.selected.clone().unwrap_or_else(|| "—".to_owned());
        dialog_window(self.catalog().label("Format Cells"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.label(format!("Column: {column}"));
                ui.horizontal(|ui| {
                    let label = ui.help_label("Decimal places:");
                    ui.add(egui::DragValue::new(&mut self.format_decimals).range(0..=8))
                        .labelled_by(label.id)
                        .hover_from(&label);
                });
                ui.help_checkbox(&mut self.format_thousands, "Thousands separator");
                ui.help_checkbox(&mut self.format_bold, "Bold");
                ui.horizontal(|ui| {
                    ui.help_label("Alignment:");
                    ui.radio_value(&mut self.format_align, CellAlign::Left, "Left")
                        .on_hover_text(checkbox_help("Left"));
                    ui.radio_value(&mut self.format_align, CellAlign::Center, "Center")
                        .on_hover_text(checkbox_help("Center"));
                    ui.radio_value(&mut self.format_align, CellAlign::Right, "Right")
                        .on_hover_text(checkbox_help("Right"));
                });
                ui.small(
                    "The format changes the worksheet display only; the stored values keep \
                          full precision.",
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.help_button("Apply").clicked() {
                        apply = true;
                    }
                    if ui
                        .help_button("Remove format")
                        .on_hover_text("Show the column with the default format")
                        .clicked()
                    {
                        remove = true;
                    }
                    if ui.help_button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        self.show_format_dialog = open && !close;
        if apply {
            self.apply_column_format();
            self.show_format_dialog = false;
        }
        if remove {
            self.clear_column_format();
            self.show_format_dialog = false;
        }
    }

    fn shortcuts_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_shortcuts {
            return;
        }
        let mut open = self.show_shortcuts;
        let shortcuts: &[(&str, &str)] = &[
            (
                "Ctrl+N / Ctrl+O / Ctrl+S",
                "New worksheet, open and save as",
            ),
            ("Ctrl+P", "Print the Session report"),
            ("Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z", "Undo and redo"),
            ("Ctrl+C / Ctrl+X / Ctrl+V", "Copy, cut and paste"),
            ("Ctrl+F", "Find and replace"),
            ("Ctrl+D / Ctrl+R", "Fill the selection down and right"),
            ("Ctrl+A", "Select the whole worksheet"),
            ("Delete / Backspace", "Clear the selected cells"),
            ("Arrows / Shift+Arrows", "Move or extend the selection"),
            ("Tab", "Next column"),
            ("Enter", "Commit the edit and move down"),
            ("Escape", "Cancel the edit or close the dialog"),
            ("Ctrl + / Ctrl − / Ctrl 0", "Zoom in, out and reset"),
            ("Double-click", "Edit a cell"),
            ("Right-click", "Context menu of the cell, header or tab"),
        ];
        dialog_window(self.catalog().label("Keyboard shortcuts"))
            .without_clear()
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(340.0)
                    .show(ui, |ui| {
                        egui::Grid::new("shortcuts_grid")
                            .num_columns(2)
                            .spacing([18.0, 8.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for (keys, action) in shortcuts {
                                    ui.label(RichText::new(*keys).monospace().size(12.5));
                                    ui.label(*action);
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                ui.label(
                    RichText::new(
                        "Ctrl and Shift combinations use the keyboard layout in use; the grid \
                         never captures keys while a text field has focus.",
                    )
                    .small()
                    .color(ui.visuals().text_color()),
                );
            });
        self.show_shortcuts = open;
    }

    fn assistant_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_assistant_dialog {
            return;
        }
        let mut open = self.show_assistant_dialog;
        let mut choice: Option<u8> = None;
        let goals = [
            "Describe my data",
            "Compare two groups",
            "Compare three or more groups",
            "Test a proportion",
            "Check a process (control chart)",
            "Measure process capability",
            "Fit a regression",
            "Design an experiment",
            "Test normality",
        ];
        dialog_window(self.catalog().label("Assistant"))
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.help_label("What do you want to do?");
                for (index, goal) in goals.iter().enumerate() {
                    if ui
                        .add_sized([ui.available_width(), 24.0], egui::Button::new(*goal))
                        .clicked()
                    {
                        choice = Some(index as u8);
                    }
                }
            });
        self.show_assistant_dialog = open && choice.is_none();
        match choice {
            Some(0) => self.run_descriptive_statistics(),
            Some(1) => self.open_t_test_dialog(TTestKind::TwoSample),
            Some(2) => self.open_anova_dialog(),
            Some(3) => self.open_test2_dialog(Test2Kind::OneProportion),
            Some(4) => {
                if let Some(column) = &self.selected {
                    self.chart_column = column.clone();
                }
                self.chart_kind = ControlChartKind::IndividualsMovingRange;
                self.show_chart_dialog = true;
            }
            Some(5) => {
                if let Some(column) = &self.selected {
                    self.cap_column = column.clone();
                }
                self.show_cap_dialog = true;
            }
            Some(6) => self.open_regression_dialog(),
            Some(7) => self.open_doe_dialog(),
            Some(8) => self.open_probability_plot_dialog(),
            _ => {}
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        if !self.show_about {
            return;
        }
        let mut open = self.show_about;
        dialog_window(self.catalog().label("About FreePlots"))
            .without_clear()
            .without_clear()
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(440.0)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    about_formula(ui);
                });
                ui.add_space(16.0);
                ui.label(RichText::new(format!("FreePlots Engine {}", fp_core::VERSION)).strong());
                ui.label(
                    RichText::new(
                        "Software de análise e estatística desenvolvido por AI em Rust, \
                         acessível para todos.",
                    )
                    .size(13.0),
                );
                ui.add_space(12.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("MIT license")
                            .size(12.0)
                            .color(ui.visuals().text_color()),
                    );
                });
            });
        self.show_about = open;
    }

    fn graph_window(&mut self, ctx: &egui::Context) {
        if !self.show_plot {
            if self.plot_viewport.take().is_some() {
                ctx.send_viewport_cmd_to(
                    egui::ViewportId::from_hash_of("fp_graph"),
                    egui::ViewportCommand::Close,
                );
            }
            return;
        }
        if self.native_plot_windows {
            let Some(plot) = self.plot.clone() else {
                self.show_plot = false;
                return;
            };
            let default_size = if plot.panels.is_empty() {
                [784.0, 620.0]
            } else {
                [1120.0, 460.0]
            };
            let title = if plot.title.is_empty() {
                self.catalog().label("Graph").to_owned()
            } else {
                plot.title.clone()
            };
            let flag = self
                .plot_viewport
                .get_or_insert_with(|| Arc::new(AtomicBool::new(true)))
                .clone();
            if !flag.load(Ordering::Relaxed) {
                self.plot_viewport = None;
                self.show_plot = false;
                return;
            }
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("fp_graph"),
                egui::ViewportBuilder::default()
                    .with_title(title)
                    .with_inner_size(default_size),
                move |ui, _class| {
                    // The viewport is cleared to black; paint the theme surface
                    // first so no black band shows under a short plot.
                    let background = ui.visuals().window_fill;
                    ui.painter().rect_filled(ui.max_rect(), 0.0, background);
                    // Tall multi-panel plots can be larger than the window on a
                    // small screen; scrolling keeps every panel reachable.
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::plot_view::show(ui, &plot);
                        });
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        flag.store(false, Ordering::Relaxed);
                    }
                },
            );
            return;
        }
        let mut open = self.show_plot;
        let default_size = if self
            .plot
            .as_ref()
            .is_some_and(|plot| !plot.panels.is_empty())
        {
            [1120.0, 460.0]
        } else {
            [784.0, 620.0]
        };
        // The window carries the title of the graph it shows, so a control
        // chart is not presented as a probability plot.
        let title = self
            .plot
            .as_ref()
            .map(|plot| plot.title.clone())
            .filter(|title| !title.is_empty())
            .unwrap_or_else(|| self.catalog().label("Graph").to_owned());
        dialog_window(title)
            .without_clear()
            .id(egui::Id::new("graph_window"))
            .open(&mut open)
            .default_pos(egui::pos2(8.0, 64.0))
            .default_size(default_size)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(spec) = &self.plot {
                    egui::ScrollArea::both()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::plot_view::show(ui, spec);
                        });
                } else {
                    ui.help_label("No graph yet.");
                }
            });
        self.show_plot = open;
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
    Worksheet,
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
        ViewIcon::Worksheet => {
            let y = frame.top() + frame.height() * 0.3;
            ui.painter().line_segment(
                [egui::pos2(frame.left(), y), egui::pos2(frame.right(), y)],
                stroke,
            );
            let x = frame.left() + frame.width() * 0.35;
            ui.painter()
                .line_segment([egui::pos2(x, y), egui::pos2(x, frame.bottom())], stroke);
        }
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
