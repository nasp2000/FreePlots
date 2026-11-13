// SPDX-License-Identifier: MIT
//! One analysis, end to end, independent of the front end.
//!
//! The CLI and the GUI both call this module; neither assembles a report by
//! hand. Keeping the use case here is what makes the Session text, the SVG and
//! the future screen show the same numbers.

use std::collections::BTreeMap;

use crate::data::{Column, ColumnData, DType, DataFrame, Descriptives, MissingPolicy, Sample};
use crate::dists::{
    Distribution, Exponential, ExtremeValue, Gamma, Logistic, Loglogistic, Lognormal, Lognormal3,
    Normal, Weibull, Weibull3,
};
use crate::error::{Error, Result};
use crate::estimate::c4;
use crate::gof::{
    ad_p_value, ad_p_value_exponential, ad_p_value_mc, ad_p_value_weibull, anderson_darling,
};
use crate::plot::{
    PlotNote, PlotSpec, PlottingPosition, exponential_probability_plot,
    extreme_value_probability_plot, gamma_probability_plot, logistic_probability_plot,
    loglogistic_probability_plot, lognormal_probability_plot, lognormal3_probability_plot,
    normal_probability_plot, overlay_plots, panel_plot_with_scales, weibull_probability_plot,
    weibull3_probability_plot,
};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::spc::{ControlChartKind, TestSet};
use crate::special::{SplitMix64, chi_square_quantile, normal_cdf, normal_quantile};

/// What a probability plot displays, mirroring the documented behaviour's `BOTH`/symbols/fit
/// display choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlotDisplay {
    /// Symbols and the fitted line (the documented behaviour's `BOTH`, the default).
    #[default]
    Both,
    /// Symbols only; the fitted line and band are suppressed.
    Symbols,
    /// Fitted line (and band) only; the observations are hidden.
    Fit,
}

impl PlotDisplay {
    /// Name used in the UI and reports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Both => "Symbols + fitted line",
            Self::Symbols => "Symbols only",
            Self::Fit => "Fitted line only",
        }
    }

    /// Stable identifier for the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Symbols => "symbols",
            Self::Fit => "fit",
        }
    }
}

/// Layout for multiple Y columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlotLayout {
    /// One panel per Y column (the documented behaviour's `separate panels`, the default).
    #[default]
    Separate,
    /// All Y columns in one set of axes (the documented behaviour's `overlay_continuous`).
    Overlay,
}

impl PlotLayout {
    /// Name used in the UI.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Separate => "Separate panels",
            Self::Overlay => "Overlay",
        }
    }

    /// Stable identifier for the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Separate => "separate",
            Self::Overlay => "overlay",
        }
    }
}

/// How the Anderson–Darling p-value is obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PValueMethod {
    /// Composite-AD approximation recovered from the documented behaviour (default).
    Ad,
    /// Independent Monte Carlo cross-check with a fixed seed.
    MonteCarlo {
        /// Replications.
        reps: usize,
        /// Seed for the in-tree generator.
        seed: u64,
    },
}

/// Distribution family available to an analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributionKind {
    /// Normal distribution.
    Normal,
    /// Two-parameter log-normal distribution (fit on `ln x`).
    Lognormal,
    /// Two-parameter Weibull distribution (profile likelihood).
    Weibull,
    /// One-parameter exponential distribution (`θ = mean`).
    Exponential,
    /// Two-parameter logistic distribution.
    Logistic,
    /// Two-parameter log-logistic distribution (`ln x` logistic).
    Loglogistic,
    /// Two-parameter gamma distribution with shape and scale.
    Gamma,
    /// Two-parameter extreme-value (type I Gumbel) distribution.
    ExtremeValue,
    /// Three-parameter Weibull distribution with a threshold.
    Weibull3,
    /// Three-parameter log-normal distribution with a threshold.
    Lognormal3,
}

impl DistributionKind {
    /// Name used in report titles.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Lognormal => "Lognormal",
            Self::Weibull => "Weibull",
            Self::Exponential => "Exponential",
            Self::Logistic => "Logistic",
            Self::Loglogistic => "Loglogistic",
            Self::Gamma => "Gamma",
            Self::ExtremeValue => "Extreme Value",
            Self::Weibull3 => "Weibull (3-parameter)",
            Self::Lognormal3 => "Lognormal (3-parameter)",
        }
    }

    /// Stable identifier for the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Lognormal => "lognormal",
            Self::Weibull => "weibull",
            Self::Exponential => "exponential",
            Self::Logistic => "logistic",
            Self::Loglogistic => "loglogistic",
            Self::Gamma => "gamma",
            Self::ExtremeValue => "extremevalue",
            Self::Weibull3 => "weibull3",
            Self::Lognormal3 => "lognormal3",
        }
    }
}

/// Normality test family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormalityTestKind {
    /// Anderson–Darling (the documented behaviour's default).
    AndersonDarling,
    /// Kolmogorov–Smirnov with a Monte Carlo Lilliefors p-value.
    KolmogorovSmirnov,
    /// Ryan–Joiner normal-score correlation with a Monte Carlo p-value.
    RyanJoiner,
    /// D'Agostino–Pearson omnibus test of skewness and kurtosis.
    DAgostinoK2,
}

impl NormalityTestKind {
    /// Name used in reports.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::AndersonDarling => "Anderson–Darling",
            Self::KolmogorovSmirnov => "Kolmogorov–Smirnov",
            Self::RyanJoiner => "Ryan–Joiner",
            Self::DAgostinoK2 => "D'Agostino–Pearson",
        }
    }

    /// Stable identifier for the CLI.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AndersonDarling => "ad",
            Self::KolmogorovSmirnov => "ks",
            Self::RyanJoiner => "rj",
            Self::DAgostinoK2 => "k2",
        }
    }

    /// Parses the CLI spelling.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().replace('_', "-").as_str() {
            "ad" | "anderson-darling" | "anderson" => Some(Self::AndersonDarling),
            "ks" | "kolmogorov-smirnov" | "kolmogorov" => Some(Self::KolmogorovSmirnov),
            "rj" | "ryan-joiner" | "ryan" => Some(Self::RyanJoiner),
            "k2" | "dagostino" | "d-agostino" | "pearson" => Some(Self::DAgostinoK2),
            _ => None,
        }
    }
}

/// Typed analysis request.
///
/// The dialog, the CLI and the tests build one of these; the engine runs it.
/// Defaults live in the constructor, not in the widgets.
#[derive(Debug, Clone, PartialEq)]
pub enum AnalysisSpec {
    /// Probability plot.
    ProbabilityPlot {
        /// One or more Y columns.
        columns: Vec<String>,
        /// Optional grouping column (single-Y overlay).
        group: Option<String>,
        /// Distribution family.
        distribution: DistributionKind,
        /// Plotting-position convention.
        plotting_position: PlottingPosition,
        /// p-value method.
        p_value: PValueMethod,
        /// Confidence level for the band (`None` = no band).
        confidence: Option<f64>,
        /// Display choice (symbols, fit, or both).
        display: PlotDisplay,
        /// Multi-Y layout (separate panels or overlay).
        layout: PlotLayout,
        /// Common X scale across panels (`SAME 1`).
        same_x: bool,
        /// Common Y scale across panels (`SAME 2`).
        same_y: bool,
    },
    /// Normal process capability.
    Capability {
        /// Column name.
        column: String,
        /// Lower specification limit.
        lsl: Option<f64>,
        /// Upper specification limit.
        usl: Option<f64>,
        /// Optional target for `Cpm`; without it the index is not computed.
        target: Option<f64>,
        /// Distribution family for the capability model.
        distribution: DistributionKind,
        /// Subgroup size (1 = individual observations / moving range).
        subgroup: usize,
        /// Optional column whose labels define the subgroups (normal model).
        subgroup_column: Option<String>,
        /// Draw the six-panel capability Sixpack instead of one plot.
        sixpack: bool,
        /// Apply the `c4` unbiasing constant to the within estimate.
        unbiasing_within: bool,
        /// Apply the `c4` unbiasing constant to the overall estimate.
        unbiasing_overall: bool,
        /// Two-sided confidence level for the capability intervals.
        confidence: f64,
        /// Replications for the non-normal PPM bootstrap (`0` = automatic).
        bootstrap_reps: usize,
        /// Estimate a Box-Cox transformation before the normal capability model.
        box_cox: bool,
        /// Refuse to report capability when the I–MR stability screen signals.
        require_stability: bool,
    },
    /// Fit several distributions and compare their GOF and information criteria.
    DistributionComparison {
        /// Column name.
        column: String,
    },
    /// Normality test.
    NormalityTest {
        /// Column name.
        column: String,
        /// Test family.
        test: NormalityTestKind,
        /// Draw/report percentile lines (the documented behaviour's default: off).
        percentile_lines: bool,
    },
    /// Descriptive statistics for one column.
    DescriptiveStatistics {
        /// Column name.
        column: String,
        /// Optional grouping (By) column.
        by: Option<String>,
    },
    /// Shewhart control chart.
    ControlChart {
        /// Column name.
        column: String,
        /// Chart family.
        kind: ControlChartKind,
        /// Subgroup size (1 for I–MR).
        subgroup: usize,
        /// Tests for special causes that run on the chart.
        tests: TestSet,
        /// CUSUM reference value and decision interval (ignored by the
        /// Shewhart charts).
        cusum: crate::spc::CusumOptions,
        /// EWMA smoothing constant and limit width (ignored by the other
        /// charts).
        ewma: crate::spc::EwmaOptions,
        /// Optional column whose labels define subgroups of different sizes
        /// (X̄–R and X̄–S only).
        subgroup_column: Option<String>,
        /// Optional column whose consecutive labels define phases with their
        /// own limits (I–MR, X̄–R and X̄–S only).
        phase_column: Option<String>,
    },
    /// K-means clustering of several numeric columns.
    KMeans {
        /// Variables.
        columns: Vec<String>,
        /// Number of clusters.
        k: usize,
        /// Standardise the variables before the distance.
        standardise: bool,
        /// Seed of the k-means++ initialisation.
        seed: u64,
    },
    /// Principal component analysis of several numeric columns.
    Pca {
        /// Variables.
        columns: Vec<String>,
        /// Components kept in the score table.
        keep: usize,
    },
    /// Accelerated life testing with the Arrhenius model.
    AcceleratedLife {
        /// Time column.
        time: String,
        /// Stress column.
        stress: String,
        /// Optional censoring column.
        censor: Option<String>,
        /// Value that marks a right-censored observation.
        censor_value: f64,
        /// Stress at which the life is extrapolated.
        use_stress: f64,
        /// Life distribution.
        life: crate::alt::LifeModel,
        /// Acceleration relationship.
        acceleration: crate::alt::AccelerationModel,
        /// Optional failure-mode column for competing risks.
        mode: Option<String>,
        /// Time at which the incidence is reported (`None` = last failure).
        horizon: Option<f64>,
    },
    /// Step-stress accelerated life testing (cumulative exposure).
    StepStressAlt {
        /// Duration column.
        duration: String,
        /// Stress column.
        stress: String,
        /// Unit identifier column.
        unit: String,
        /// Event indicator column (1 = failure on the last step).
        event: String,
        /// Stress at which the life is extrapolated.
        use_stress: f64,
        /// Life distribution.
        life: crate::alt::LifeModel,
        /// Acceleration relationship.
        acceleration: crate::alt::AccelerationModel,
    },
    /// One-sample sign test of a median.
    SignTest {
        /// Column to test.
        column: String,
        /// Hypothesised median.
        median: f64,
    },
    /// Time-series plot of a column.
    SeriesPlot {
        /// Column analysed.
        column: String,
        /// Optional time column for the x axis.
        time: Option<String>,
    },
    /// Stem-and-leaf display of a column.
    StemLeaf {
        /// Column analysed.
        column: String,
    },
    /// Empirical cumulative distribution function of a column.
    Ecdf {
        /// Column analysed.
        column: String,
        /// Confidence level of the band.
        confidence: f64,
    },
    /// Dotplot of a column, optionally by a factor.
    Dotplot {
        /// Column analysed.
        column: String,
        /// Optional factor column.
        factor: Option<String>,
    },
    /// Scatterplot matrix of several variables.
    MatrixPlot {
        /// Variables, in the requested order.
        columns: Vec<String>,
        /// Histogram bins of the diagonal panels.
        bins: usize,
    },
    /// Interval plot of a response by a factor.
    IntervalPlot {
        /// Response column.
        response: String,
        /// Factor column.
        factor: String,
        /// Confidence level of the intervals.
        confidence: f64,
    },
    /// Pareto chart of a categorical column.
    Pareto {
        /// Column to count.
        column: String,
        /// Optional numeric column whose values weight each row.
        weight: Option<String>,
    },
    /// Taguchi signal-to-noise analysis.
    Taguchi {
        /// Response column.
        response: String,
        /// Factor columns holding the level numbers.
        factors: Vec<String>,
        /// Quality characteristic.
        quality: crate::taguchi::Quality,
    },
    /// Acceptance sampling: the OC curve of a plan, or the plan itself.
    AcceptanceSampling {
        /// Plan to evaluate (sample size and acceptance number).
        plan: Option<crate::sampling::SamplingPlan>,
        /// Lot size (sampling without replacement) of an evaluated plan.
        lot_size: Option<usize>,
        /// AQL, LTPD, producer risk and consumer risk of a planned scheme.
        targets: Option<(f64, f64, f64, f64)>,
    },
    /// Hotelling T² control chart of several variables.
    HotellingT2 {
        /// Variables.
        columns: Vec<String>,
        /// Observations per subgroup.
        subgroup: usize,
        /// Confidence of the Phase I limits.
        confidence: f64,
    },
    /// Time-series method (moving average, decomposition or smoothing).
    TimeSeries {
        /// Column name.
        column: String,
        /// Method and its parameters.
        method: crate::timeseries::TimeSeriesMethod,
    },
    /// Run chart with runs tests.
    RunChart {
        /// Column name.
        column: String,
    },
    /// Attribute control chart (p, np, c, u).
    AttributeChart {
        /// Chart family.
        kind: crate::attribute::AttributeChartKind,
        /// Defects (or defectives) column.
        defects: String,
        /// Optional subgroup-size column.
        sizes: Option<String>,
        /// Optional constant subgroup size.
        size: Option<usize>,
    },
    /// One-sample Student t test.
    OneSampleT {
        /// Column name.
        column: String,
        /// Hypothesized mean.
        null_mean: f64,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
        /// Confidence level for the interval.
        confidence: f64,
    },
    /// Two-sample Student t test.
    TwoSampleT {
        /// First column.
        first: String,
        /// Second column.
        second: String,
        /// Use the pooled-variance test.
        pooled: bool,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
        /// Confidence level for the interval.
        confidence: f64,
    },
    /// Paired Student t test.
    PairedT {
        /// First column.
        first: String,
        /// Second column.
        second: String,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
        /// Confidence level for the interval.
        confidence: f64,
    },
    /// One-sample equivalence test (TOST).
    OneSampleEquivalence {
        /// Column name.
        column: String,
        /// Reference mean.
        null_mean: f64,
        /// Equivalence margin.
        margin: f64,
        /// Confidence level of the decision.
        confidence: f64,
    },
    /// Two-sample equivalence test (TOST, Welch).
    TwoSampleEquivalence {
        /// First column.
        first: String,
        /// Second column.
        second: String,
        /// Equivalence margin.
        margin: f64,
        /// Confidence level of the decision.
        confidence: f64,
    },
    /// Paired equivalence test (TOST).
    PairedEquivalence {
        /// First column.
        first: String,
        /// Second column.
        second: String,
        /// Equivalence margin.
        margin: f64,
        /// Confidence level of the decision.
        confidence: f64,
    },
    /// Mann–Whitney test of two independent samples.
    MannWhitney {
        /// First column.
        first: String,
        /// Second column.
        second: String,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
    },
    /// Wilcoxon signed-rank test of paired differences.
    WilcoxonSignedRank {
        /// First column.
        first: String,
        /// Second column.
        second: String,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
    },
    /// Kruskal–Wallis test of a response by a factor.
    KruskalWallis {
        /// Response column.
        response: String,
        /// Factor column.
        factor: String,
    },
    /// Friedman test of several columns measured on the same rows.
    Friedman {
        /// Columns, one per group.
        columns: Vec<String>,
    },
    /// Mood's median test of a response by a factor.
    MoodsMedian {
        /// Response column.
        response: String,
        /// Factor column.
        factor: String,
    },
    /// Partial correlation of two columns, controlling for a third.
    PartialCorrelation {
        /// First column.
        x_column: String,
        /// Second column.
        y_column: String,
        /// Control column.
        control: String,
    },
    /// Correlation matrix of several columns.
    Correlation {
        /// Columns to correlate.
        columns: Vec<String>,
    },
    /// G chart (rare events).
    RareEvents {
        /// Column with the number of events between the failures.
        counts: String,
        /// Optional column with the time of every interval.
        times: Option<String>,
    },
    /// Tolerance interval of a column (normal or distribution-free).
    ToleranceInterval {
        /// Column to analyse.
        column: String,
        /// Population proportion the interval must cover.
        proportion: f64,
        /// Confidence of the statement.
        confidence: f64,
        /// Build the interval from order statistics instead of the normal law.
        free: bool,
    },
    /// Box–Cox transformation of a column.
    BoxCox {
        /// Column to transform.
        column: String,
    },
    /// Chi-square test for independence of two categorical variables.
    ChiSquareIndependence {
        /// Column with the levels of the row variable.
        row: String,
        /// Column with the levels of the column variable.
        column: String,
    },
    /// Two-way ANOVA of a response by two factors.
    TwoWayAnova {
        /// Response column.
        response: String,
        /// First factor column.
        first: String,
        /// Second factor column.
        second: String,
        /// Fit the interaction term (the documented command fits main effects).
        interaction: bool,
    },
    /// Curve fit of a response to a predictor.
    CurveFit {
        /// Predictor column.
        x_column: String,
        /// Response column.
        y_column: String,
        /// Model to fit.
        model: crate::curvefit::CurveModel,
        /// Confidence level of the coefficient intervals.
        confidence: f64,
    },
    /// Levene's test of equal variances across the levels of a factor.
    Levene {
        /// Response column.
        response: String,
        /// Factor column.
        factor: String,
        /// Centre used to measure the spread inside a group.
        center: crate::variance::VarianceCenter,
        /// Confidence level of the interval.
        confidence: f64,
    },
    /// Bartlett's test of equal variances across the levels of a factor.
    Bartlett {
        /// Response column.
        response: String,
        /// Factor column.
        factor: String,
    },
    /// Chi-square goodness-of-fit test of observed against expected counts.
    ChiSquareGoodnessOfFit {
        /// Column with the observed counts.
        column: String,
        /// Expected count of every cell.
        expected: Vec<f64>,
        /// Number of parameters estimated from the data.
        estimated_parameters: usize,
    },
    /// Grubbs' test for a single outlier.
    Grubbs {
        /// Column with the observations.
        column: String,
        /// Significance level of the test.
        alpha: f64,
    },
    /// Power and sample size of a test.
    Power {
        /// Procedure to analyse.
        procedure: crate::power::PowerProcedure,
        /// Effect size: the standardised difference, the true proportion or
        /// the population correlation.
        effect: f64,
        /// Null proportion (proportion test only).
        null_value: f64,
        /// Sample size; zero asks for the size that reaches the power.
        n: usize,
        /// Number of groups (ANOVA) or predictors (regression).
        groups: usize,
        /// Significance level.
        alpha: f64,
        /// Requested power.
        power: f64,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
    },
    /// One-sample proportion test.
    OneProportion {
        /// Binary 0/1 column.
        column: String,
        /// Value counted as a success.
        success: f64,
        /// Hypothesized proportion.
        null_p: f64,
        /// Exact or normal method.
        method: crate::proportion::ProportionMethod,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
        /// Confidence level for the interval.
        confidence: f64,
    },
    /// Two-sample proportion test.
    TwoProportions {
        /// First binary column.
        first: String,
        /// Second binary column.
        second: String,
        /// Value counted as a success.
        success: f64,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
        /// Confidence level for the interval.
        confidence: f64,
    },
    /// One-sample variance test.
    OneVariance {
        /// Column name.
        column: String,
        /// Hypothesized variance.
        null_variance: f64,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
        /// Confidence level for the interval.
        confidence: f64,
    },
    /// Two-sample variance test.
    TwoVariances {
        /// First column.
        first: String,
        /// Second column.
        second: String,
        /// Alternative hypothesis.
        alternative: crate::hypothesis::Alternative,
        /// Confidence level for the interval.
        confidence: f64,
        /// F test or Bonett's test.
        method: crate::variance::VarianceMethod,
    },
    /// Ordinary least squares regression.
    Regression {
        /// Response column.
        response: String,
        /// Numeric predictor columns.
        predictors: Vec<String>,
        /// Confidence level for coefficient intervals.
        confidence: f64,
    },
    /// Stepwise ordinary least squares regression.
    StepwiseRegression {
        /// Response column.
        response: String,
        /// Numeric candidate predictor columns.
        candidates: Vec<String>,
        /// Search direction.
        method: crate::stepwise::StepMethod,
        /// Rule that accepts a move.
        criterion: crate::stepwise::StepCriterion,
        /// Entry alpha of the p-value criterion.
        alpha_enter: f64,
        /// Removal alpha of the p-value criterion.
        alpha_remove: f64,
        /// Maximum number of accepted moves.
        max_steps: usize,
        /// Confidence level for coefficient intervals.
        confidence: f64,
    },
    /// One-way analysis of variance.
    Anova {
        /// Response column.
        response: String,
        /// Factor column.
        factor: String,
        /// Confidence level for group means and pairwise intervals.
        confidence: f64,
    },
    /// Kaplan–Meier survival estimate.
    KaplanMeier {
        /// Positive time column.
        time: String,
        /// Optional numeric censoring column.
        censor: Option<String>,
        /// Value that identifies a right-censored observation.
        censor_value: f64,
        /// Confidence level for the survival intervals.
        confidence: f64,
    },
    /// Right-censored exponential reliability.
    CensoredExponential {
        /// Positive time column.
        time: String,
        /// Optional numeric censoring column.
        censor: Option<String>,
        /// Value that identifies a right-censored observation.
        censor_value: f64,
    },
    /// Right-censored lognormal reliability.
    CensoredLognormal {
        /// Positive time column.
        time: String,
        /// Optional numeric censoring column.
        censor: Option<String>,
        /// Value that identifies a right-censored observation.
        censor_value: f64,
    },
    /// Right-censored Weibull reliability.
    CensoredWeibull {
        /// Positive time column.
        time: String,
        /// Optional numeric censoring column.
        censor: Option<String>,
        /// Value that identifies a right-censored observation.
        censor_value: f64,
    },
    /// Type 1 Gage study against a known reference.
    GageType1 {
        /// Measurement column.
        measurement: String,
        /// Known reference value.
        reference: f64,
        /// Optional lower specification limit.
        lsl: Option<f64>,
        /// Optional upper specification limit.
        usl: Option<f64>,
    },
    /// Ordinal logistic regression.
    Ordinal {
        /// Ordered response codes.
        response: String,
        /// Predictor columns.
        predictors: Vec<String>,
    },
    /// Poisson regression.
    Poisson {
        /// Count response column.
        response: String,
        /// Predictor columns.
        predictors: Vec<String>,
    },
    /// Binary logistic regression.
    Logistic {
        /// Binary response column (0/1).
        response: String,
        /// Predictor columns.
        predictors: Vec<String>,
    },
    /// Attribute agreement analysis.
    Agreement {
        /// Part column.
        part: String,
        /// Appraiser column.
        appraiser: String,
        /// Rating column.
        rating: String,
    },
    /// Gage linearity and bias study.
    GageLinearity {
        /// Measurement column.
        measurement: String,
        /// Column holding the reference value of each part.
        reference: String,
    },
    /// Crossed Gage R&R.
    GageRr {
        /// Measurement column.
        measurement: String,
        /// Part column.
        part: String,
        /// Operator column.
        operator: String,
        /// Crossed or nested design.
        design: crate::gage::GageDesign,
    },
    /// Two-level full factorial analysis.
    Factorial {
        /// Response column.
        response: String,
        /// Factor columns coded −1/+1.
        factors: Vec<String>,
        /// Fit main effects only (for fractional designs).
        main_effects_only: bool,
        /// Block column; the block effect is fitted separately.
        block: Option<String>,
    },
    /// Quadratic response-surface analysis.
    ResponseSurface {
        /// Response column.
        response: String,
        /// Coded numeric factors.
        factors: Vec<String>,
        /// Compute the stationary point and eigenvalues.
        optimize: bool,
        /// Draw a contour plot for exactly two factors.
        contours: bool,
        /// Compute the steepest-ascent path from the design centre.
        steepest_ascent: bool,
    },
    /// Multi-response desirability optimisation.
    Desirability {
        /// Coded numeric factors.
        factors: Vec<String>,
        /// Responses with their goals and limits.
        responses: Vec<crate::desirability::Response>,
    },
    /// Boxplot with optional grouping.
    Boxplot {
        /// Numeric column.
        column: String,
        /// Optional grouping column.
        by: Option<String>,
    },
    /// Scatterplot with optional least-squares line.
    Scatterplot {
        /// X column.
        x: String,
        /// Y column.
        y: String,
        /// Draw the least-squares line.
        fit: bool,
    },
    /// Histogram with optional fixed bin count.
    Histogram {
        /// Numeric column.
        column: String,
        /// Number of bins (`None` = automatic).
        bins: Option<usize>,
    },
}

/// Replications for the Monte Carlo KS p-value.
///
/// 1000 for the sample sizes of the records; fewer on large samples so a
/// 200 000-row column does not freeze the interface for a minute.
fn ks_replications(n: usize) -> usize {
    (5_000_000 / n.max(1)).clamp(50, 1000)
}

impl AnalysisSpec {
    /// classic statistical-desktop defaults: Normal, Benard, composite AD, 95 % band.
    #[must_use]
    pub fn probability_plot(column: impl Into<String>) -> Self {
        Self::ProbabilityPlot {
            columns: vec![column.into()],
            group: None,
            distribution: DistributionKind::Normal,
            plotting_position: PlottingPosition::Benard,
            p_value: PValueMethod::Ad,
            confidence: Some(0.95),
            display: PlotDisplay::Both,
            layout: PlotLayout::Separate,
            same_x: false,
            same_y: false,
        }
    }

    /// Probability plot of several Y columns, overlaid.
    #[must_use]
    pub fn probability_plot_multi(columns: Vec<String>) -> Self {
        Self::ProbabilityPlot {
            columns,
            group: None,
            distribution: DistributionKind::Normal,
            plotting_position: PlottingPosition::Benard,
            p_value: PValueMethod::Ad,
            confidence: Some(0.95),
            display: PlotDisplay::Both,
            layout: PlotLayout::Separate,
            same_x: false,
            same_y: false,
        }
    }

    /// Normal capability with classic statistical-desktop defaults (subgroup 1, AMR within,
    /// unbiasing within on / overall off).
    #[must_use]
    pub fn capability(column: impl Into<String>, lsl: Option<f64>, usl: Option<f64>) -> Self {
        Self::Capability {
            column: column.into(),
            lsl,
            usl,
            target: None,
            distribution: DistributionKind::Normal,
            subgroup: 1,
            subgroup_column: None,
            sixpack: false,
            unbiasing_within: true,
            unbiasing_overall: false,
            confidence: 0.95,
            bootstrap_reps: 0,
            box_cox: false,
            require_stability: false,
        }
    }

    /// Draws the six-panel capability Sixpack (normal model only).
    #[must_use]
    pub fn with_capability_sixpack(mut self, enabled: bool) -> Self {
        if let Self::Capability { sixpack, .. } = &mut self {
            *sixpack = enabled;
        }
        self
    }

    /// Replaces the capability subgroup size.
    #[must_use]
    pub fn with_capability_subgroup(mut self, subgroup: usize) -> Self {
        if let Self::Capability { subgroup: slot, .. } = &mut self {
            *slot = subgroup;
        }
        self
    }

    /// Uses a column of subgroup labels instead of the numeric subgroup size
    /// (normal capability only).
    #[must_use]
    pub fn with_capability_subgroup_column(mut self, column: Option<String>) -> Self {
        if let Self::Capability {
            subgroup_column, ..
        } = &mut self
        {
            *subgroup_column = column;
        }
        self
    }

    /// Enables or disables an automatic Box-Cox transformation (Normal model only).
    #[must_use]
    pub fn with_capability_box_cox(mut self, enabled: bool) -> Self {
        if let Self::Capability { box_cox: slot, .. } = &mut self {
            *slot = enabled;
        }
        self
    }

    /// Refuses capability output when the I–MR stability screen finds signals.
    #[must_use]
    pub fn with_capability_stability(mut self, require: bool) -> Self {
        if let Self::Capability {
            require_stability, ..
        } = &mut self
        {
            *require_stability = require;
        }
        self
    }

    /// Replaces the non-normal PPM bootstrap replication count (`0` = automatic).
    #[must_use]
    pub fn with_capability_bootstrap(mut self, reps: usize) -> Self {
        if let Self::Capability {
            bootstrap_reps: slot,
            ..
        } = &mut self
        {
            *slot = reps;
        }
        self
    }

    /// Replaces the optional capability target used by `Cpm`.
    #[must_use]
    pub fn with_capability_target(mut self, target: Option<f64>) -> Self {
        if let Self::Capability { target: slot, .. } = &mut self {
            *slot = target;
        }
        self
    }

    /// Replaces the capability confidence level.
    #[must_use]
    pub fn with_capability_confidence(mut self, confidence: f64) -> Self {
        if let Self::Capability {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Replaces the distribution used by the capability model.
    #[must_use]
    pub fn with_capability_distribution(mut self, distribution: DistributionKind) -> Self {
        if let Self::Capability {
            distribution: slot, ..
        } = &mut self
        {
            *slot = distribution;
        }
        self
    }

    /// Distribution comparison (Normal, Lognormal, Weibull, Exponential).
    #[must_use]
    pub fn distribution_comparison(column: impl Into<String>) -> Self {
        Self::DistributionComparison {
            column: column.into(),
        }
    }

    /// Normality test with the documented behaviour's default test (Anderson–Darling) and no
    /// percentile lines.
    #[must_use]
    pub fn normality_test(column: impl Into<String>) -> Self {
        Self::NormalityTest {
            column: column.into(),
            test: NormalityTestKind::AndersonDarling,
            percentile_lines: false,
        }
    }

    /// Selects the normality-test family.
    #[must_use]
    pub fn with_normality_test(mut self, test: NormalityTestKind) -> Self {
        if let Self::NormalityTest { test: slot, .. } = &mut self {
            *slot = test;
        }
        self
    }

    /// Descriptive statistics with the default 95 % mean interval.
    #[must_use]
    pub fn descriptive_statistics(column: impl Into<String>) -> Self {
        Self::DescriptiveStatistics {
            column: column.into(),
            by: None,
        }
    }

    /// Descriptive statistics for one column, one block per level of `by`.
    #[must_use]
    pub fn descriptive_statistics_by(column: impl Into<String>, by: impl Into<String>) -> Self {
        Self::DescriptiveStatistics {
            column: column.into(),
            by: Some(by.into()),
        }
    }

    /// I–MR, X̄–R or X̄–S control chart.
    #[must_use]
    pub fn control_chart(
        column: impl Into<String>,
        kind: ControlChartKind,
        subgroup: usize,
    ) -> Self {
        Self::ControlChart {
            column: column.into(),
            kind,
            subgroup,
            tests: TestSet::default(),
            cusum: crate::spc::CusumOptions::default(),
            ewma: crate::spc::EwmaOptions::default(),
            subgroup_column: None,
            phase_column: None,
        }
    }

    /// The same control chart with explicit CUSUM reference and decision
    /// values.
    #[must_use]
    pub fn with_cusum_options(mut self, cusum: crate::spc::CusumOptions) -> Self {
        if let Self::ControlChart { cusum: current, .. } = &mut self {
            *current = cusum;
        }
        self
    }

    /// The same control chart with an explicit EWMA smoothing constant and
    /// limit width.
    #[must_use]
    pub fn with_ewma_options(mut self, ewma: crate::spc::EwmaOptions) -> Self {
        if let Self::ControlChart { ewma: current, .. } = &mut self {
            *current = ewma;
        }
        self
    }

    /// The same control chart with an explicit set of tests.
    #[must_use]
    pub fn with_control_chart_tests(mut self, tests: TestSet) -> Self {
        if let Self::ControlChart { tests: current, .. } = &mut self {
            *current = tests;
        }
        self
    }

    /// K-means clustering.
    #[must_use]
    pub fn kmeans(columns: Vec<String>, k: usize) -> Self {
        Self::KMeans {
            columns,
            k,
            standardise: true,
            seed: 2026,
        }
    }

    /// Replaces the standardisation and the seed of the clustering.
    #[must_use]
    pub fn with_kmeans_options(mut self, standardise: bool, seed: u64) -> Self {
        if let Self::KMeans {
            standardise: flag,
            seed: value,
            ..
        } = &mut self
        {
            *flag = standardise;
            *value = seed;
        }
        self
    }

    /// Principal component analysis.
    #[must_use]
    pub fn pca(columns: Vec<String>, keep: usize) -> Self {
        Self::Pca { columns, keep }
    }

    /// Accelerated life testing.
    #[must_use]
    pub fn accelerated_life(time: impl Into<String>, stress: impl Into<String>) -> Self {
        Self::AcceleratedLife {
            time: time.into(),
            stress: stress.into(),
            censor: None,
            censor_value: 1.0,
            use_stress: 300.0,
            life: crate::alt::LifeModel::Weibull,
            acceleration: crate::alt::AccelerationModel::Arrhenius,
            mode: None,
            horizon: None,
        }
    }

    /// Replaces the censoring and the use stress of the accelerated life test.
    #[must_use]
    pub fn with_accelerated_life_options(
        mut self,
        censor: Option<String>,
        censor_value: f64,
        use_stress: f64,
    ) -> Self {
        if let Self::AcceleratedLife {
            censor: censor_slot,
            censor_value: value_slot,
            use_stress: stress_slot,
            ..
        } = &mut self
        {
            *censor_slot = censor;
            *value_slot = censor_value;
            *stress_slot = use_stress;
        }
        self
    }

    /// Replaces the life distribution and the acceleration law.
    #[must_use]
    pub fn with_accelerated_life_model(
        mut self,
        life: crate::alt::LifeModel,
        acceleration: crate::alt::AccelerationModel,
    ) -> Self {
        if let Self::AcceleratedLife {
            life: life_slot,
            acceleration: acceleration_slot,
            ..
        } = &mut self
        {
            *life_slot = life;
            *acceleration_slot = acceleration;
        }
        self
    }

    /// One-sample sign test.
    #[must_use]
    pub fn sign_test(column: impl Into<String>, median: f64) -> Self {
        Self::SignTest {
            column: column.into(),
            median,
        }
    }

    /// Time-series plot.
    #[must_use]
    pub fn series_plot(column: impl Into<String>) -> Self {
        Self::SeriesPlot {
            column: column.into(),
            time: None,
        }
    }

    /// Uses a time column for the x axis.
    #[must_use]
    pub fn with_series_time(mut self, time: Option<String>) -> Self {
        if let Self::SeriesPlot { time: slot, .. } = &mut self {
            *slot = time;
        }
        self
    }

    /// Stem-and-leaf display.
    #[must_use]
    pub fn stem_leaf(column: impl Into<String>) -> Self {
        Self::StemLeaf {
            column: column.into(),
        }
    }

    /// Empirical CDF.
    #[must_use]
    pub fn ecdf(column: impl Into<String>) -> Self {
        Self::Ecdf {
            column: column.into(),
            confidence: 0.95,
        }
    }

    /// Replaces the confidence of the empirical CDF band.
    #[must_use]
    pub fn with_ecdf_confidence(mut self, confidence: f64) -> Self {
        if let Self::Ecdf {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Dotplot of a column.
    #[must_use]
    pub fn dotplot(column: impl Into<String>) -> Self {
        Self::Dotplot {
            column: column.into(),
            factor: None,
        }
    }

    /// Groups the dotplot by a factor.
    #[must_use]
    pub fn with_dotplot_factor(mut self, factor: Option<String>) -> Self {
        if let Self::Dotplot { factor: slot, .. } = &mut self {
            *slot = factor;
        }
        self
    }

    /// Scatterplot matrix.
    #[must_use]
    pub fn matrix_plot(columns: Vec<String>) -> Self {
        Self::MatrixPlot { columns, bins: 8 }
    }

    /// Replaces the histogram bins of the matrix plot.
    #[must_use]
    pub fn with_matrix_bins(mut self, bins: usize) -> Self {
        if let Self::MatrixPlot { bins: slot, .. } = &mut self {
            *slot = bins;
        }
        self
    }

    /// Interval plot of a response by a factor.
    #[must_use]
    pub fn interval_plot(response: impl Into<String>, factor: impl Into<String>) -> Self {
        Self::IntervalPlot {
            response: response.into(),
            factor: factor.into(),
            confidence: 0.95,
        }
    }

    /// Replaces the confidence level of the interval plot.
    #[must_use]
    pub fn with_interval_confidence(mut self, confidence: f64) -> Self {
        if let Self::IntervalPlot {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Pareto chart of a column.
    #[must_use]
    pub fn pareto(column: impl Into<String>) -> Self {
        Self::Pareto {
            column: column.into(),
            weight: None,
        }
    }

    /// Weighted Pareto chart: each row adds the value of `weight`.
    #[must_use]
    pub fn pareto_weighted(column: impl Into<String>, weight: impl Into<String>) -> Self {
        Self::Pareto {
            column: column.into(),
            weight: Some(weight.into()),
        }
    }

    /// Taguchi signal-to-noise analysis.
    #[must_use]
    pub fn taguchi(
        response: impl Into<String>,
        factors: Vec<String>,
        quality: crate::taguchi::Quality,
    ) -> Self {
        Self::Taguchi {
            response: response.into(),
            factors,
            quality,
        }
    }

    /// Acceptance sampling with a known plan.
    #[must_use]
    pub fn acceptance_sampling(sample: usize, acceptance: usize, lot_size: Option<usize>) -> Self {
        Self::AcceptanceSampling {
            plan: Some(crate::sampling::SamplingPlan {
                sample,
                acceptance,
                pa_aql: f64::NAN,
                pa_ltpd: f64::NAN,
            }),
            lot_size,
            targets: None,
        }
    }

    /// Acceptance sampling planned from the AQL, the LTPD and the two risks.
    #[must_use]
    pub fn acceptance_plan(aql: f64, ltpd: f64, alpha: f64, beta: f64) -> Self {
        Self::AcceptanceSampling {
            plan: None,
            lot_size: None,
            targets: Some((aql, ltpd, alpha, beta)),
        }
    }

    /// Sets the lot size used by the AOQ and ATI figures of a sampling plan.
    #[must_use]
    pub fn with_sampling_lot(mut self, lot_size: usize) -> Self {
        if let Self::AcceptanceSampling { lot_size: lot, .. } = &mut self {
            *lot = Some(lot_size);
        }
        self
    }

    /// Hotelling T² control chart.
    #[must_use]
    pub fn hotelling_t2(columns: Vec<String>, subgroup: usize) -> Self {
        Self::HotellingT2 {
            columns,
            subgroup,
            confidence: 0.95,
        }
    }

    /// Replaces the confidence of the T² limits.
    #[must_use]
    pub fn with_t2_confidence(mut self, confidence: f64) -> Self {
        if let Self::HotellingT2 {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Moving average of a column.
    #[must_use]
    pub fn moving_average(column: impl Into<String>, window: usize) -> Self {
        Self::TimeSeries {
            column: column.into(),
            method: crate::timeseries::TimeSeriesMethod::MovingAverage { window },
        }
    }

    /// Classical decomposition of a column.
    #[must_use]
    pub fn decomposition(
        column: impl Into<String>,
        period: usize,
        model: crate::timeseries::DecompositionModel,
    ) -> Self {
        Self::TimeSeries {
            column: column.into(),
            method: crate::timeseries::TimeSeriesMethod::Decomposition { period, model },
        }
    }

    /// Exponential smoothing of a column.
    #[must_use]
    pub fn exponential_smoothing(column: impl Into<String>, season: usize, horizon: usize) -> Self {
        Self::TimeSeries {
            column: column.into(),
            method: crate::timeseries::TimeSeriesMethod::ExponentialSmoothing { season, horizon },
        }
    }

    /// Run chart with runs tests.
    #[must_use]
    pub fn run_chart(column: impl Into<String>) -> Self {
        Self::RunChart {
            column: column.into(),
        }
    }

    /// Attribute control chart with no size configuration yet.
    #[must_use]
    pub fn attribute_chart(
        kind: crate::attribute::AttributeChartKind,
        defects: impl Into<String>,
    ) -> Self {
        Self::AttributeChart {
            kind,
            defects: defects.into(),
            sizes: None,
            size: None,
        }
    }

    /// Supplies the subgroup-size column and/or constant subgroup size.
    #[must_use]
    pub fn with_attribute_size(mut self, sizes: Option<String>, size: Option<usize>) -> Self {
        if let Self::AttributeChart {
            sizes: sizes_slot,
            size: size_slot,
            ..
        } = &mut self
        {
            *sizes_slot = sizes;
            *size_slot = size;
        }
        self
    }

    /// One-sample Student t test against `null_mean`.
    #[must_use]
    pub fn one_sample_t(column: impl Into<String>, null_mean: f64) -> Self {
        Self::OneSampleT {
            column: column.into(),
            null_mean,
            alternative: crate::hypothesis::Alternative::TwoSided,
            confidence: 0.95,
        }
    }

    /// Two-sample Student t test (Welch by default).
    #[must_use]
    pub fn two_sample_t(first: impl Into<String>, second: impl Into<String>) -> Self {
        Self::TwoSampleT {
            first: first.into(),
            second: second.into(),
            pooled: false,
            alternative: crate::hypothesis::Alternative::TwoSided,
            confidence: 0.95,
        }
    }

    /// Paired Student t test.
    #[must_use]
    pub fn paired_t(first: impl Into<String>, second: impl Into<String>) -> Self {
        Self::PairedT {
            first: first.into(),
            second: second.into(),
            alternative: crate::hypothesis::Alternative::TwoSided,
            confidence: 0.95,
        }
    }

    /// One-sample equivalence test (two one-sided tests).
    #[must_use]
    pub fn one_sample_equivalence(column: impl Into<String>, null_mean: f64, margin: f64) -> Self {
        Self::OneSampleEquivalence {
            column: column.into(),
            null_mean,
            margin,
            confidence: 0.95,
        }
    }

    /// Two-sample equivalence test (two one-sided tests, Welch).
    #[must_use]
    pub fn two_sample_equivalence(
        first: impl Into<String>,
        second: impl Into<String>,
        margin: f64,
    ) -> Self {
        Self::TwoSampleEquivalence {
            first: first.into(),
            second: second.into(),
            margin,
            confidence: 0.95,
        }
    }

    /// Paired equivalence test (two one-sided tests).
    #[must_use]
    pub fn paired_equivalence(
        first: impl Into<String>,
        second: impl Into<String>,
        margin: f64,
    ) -> Self {
        Self::PairedEquivalence {
            first: first.into(),
            second: second.into(),
            margin,
            confidence: 0.95,
        }
    }

    /// Mann–Whitney test of two independent samples.
    #[must_use]
    pub fn mann_whitney(first: impl Into<String>, second: impl Into<String>) -> Self {
        Self::MannWhitney {
            first: first.into(),
            second: second.into(),
            alternative: crate::hypothesis::Alternative::TwoSided,
        }
    }

    /// Wilcoxon signed-rank test of paired differences.
    #[must_use]
    pub fn wilcoxon_signed_rank(first: impl Into<String>, second: impl Into<String>) -> Self {
        Self::WilcoxonSignedRank {
            first: first.into(),
            second: second.into(),
            alternative: crate::hypothesis::Alternative::TwoSided,
        }
    }

    /// Kruskal–Wallis test of a response by a factor.
    #[must_use]
    pub fn kruskal_wallis(response: impl Into<String>, factor: impl Into<String>) -> Self {
        Self::KruskalWallis {
            response: response.into(),
            factor: factor.into(),
        }
    }

    /// Friedman test of several columns measured on the same rows.
    #[must_use]
    pub fn friedman(columns: Vec<String>) -> Self {
        Self::Friedman { columns }
    }

    /// Mood's median test of a response by a factor.
    #[must_use]
    pub fn moods_median(response: impl Into<String>, factor: impl Into<String>) -> Self {
        Self::MoodsMedian {
            response: response.into(),
            factor: factor.into(),
        }
    }

    /// Correlation matrix of several columns.
    #[must_use]
    pub fn correlation(columns: Vec<String>) -> Self {
        Self::Correlation { columns }
    }

    /// Partial correlation of two columns, controlling for a third.
    #[must_use]
    pub fn partial_correlation(
        x_column: impl Into<String>,
        y_column: impl Into<String>,
        control: impl Into<String>,
    ) -> Self {
        Self::PartialCorrelation {
            x_column: x_column.into(),
            y_column: y_column.into(),
            control: control.into(),
        }
    }

    /// G chart (rare events).
    #[must_use]
    pub fn rare_events(counts: impl Into<String>, times: Option<String>) -> Self {
        Self::RareEvents {
            counts: counts.into(),
            times,
        }
    }

    /// Normal tolerance interval of a column.
    #[must_use]
    pub fn tolerance_interval(column: impl Into<String>) -> Self {
        Self::ToleranceInterval {
            column: column.into(),
            proportion: 0.95,
            confidence: 0.95,
            free: false,
        }
    }

    /// Uses order statistics (distribution-free) instead of the normal law.
    #[must_use]
    pub fn with_tolerance_free(mut self, free: bool) -> Self {
        if let Self::ToleranceInterval { free: slot, .. } = &mut self {
            *slot = free;
        }
        self
    }

    /// Replaces the population proportion of the tolerance interval.
    #[must_use]
    pub fn with_tolerance_proportion(mut self, proportion: f64) -> Self {
        if let Self::ToleranceInterval {
            proportion: slot, ..
        } = &mut self
        {
            *slot = proportion;
        }
        self
    }

    /// Replaces the confidence of the tolerance interval.
    #[must_use]
    pub fn with_tolerance_confidence(mut self, confidence: f64) -> Self {
        if let Self::ToleranceInterval {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Box–Cox transformation of a column.
    #[must_use]
    pub fn box_cox(column: impl Into<String>) -> Self {
        Self::BoxCox {
            column: column.into(),
        }
    }

    /// Chi-square test for independence of two categorical variables.
    #[must_use]
    pub fn chi_square_independence(row: impl Into<String>, column: impl Into<String>) -> Self {
        Self::ChiSquareIndependence {
            row: row.into(),
            column: column.into(),
        }
    }

    /// Two-way ANOVA of a response by two factors.
    #[must_use]
    pub fn two_way_anova(
        response: impl Into<String>,
        first: impl Into<String>,
        second: impl Into<String>,
    ) -> Self {
        Self::TwoWayAnova {
            response: response.into(),
            first: first.into(),
            second: second.into(),
            interaction: false,
        }
    }

    /// Adds the interaction term to a two-way ANOVA.
    #[must_use]
    pub fn with_interaction(mut self, interaction: bool) -> Self {
        if let Self::TwoWayAnova {
            interaction: fit, ..
        } = &mut self
        {
            *fit = interaction;
        }
        self
    }

    /// Curve fit of a response to a predictor.
    #[must_use]
    pub fn curve_fit(
        x_column: impl Into<String>,
        y_column: impl Into<String>,
        model: crate::curvefit::CurveModel,
    ) -> Self {
        Self::CurveFit {
            x_column: x_column.into(),
            y_column: y_column.into(),
            model,
            confidence: 0.95,
        }
    }

    /// Replaces the confidence level of a curve fit.
    #[must_use]
    pub fn with_curve_fit_confidence(mut self, confidence: f64) -> Self {
        if let Self::CurveFit {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Levene's test of equal variances across the levels of a factor.
    #[must_use]
    pub fn levene(response: impl Into<String>, factor: impl Into<String>) -> Self {
        Self::Levene {
            response: response.into(),
            factor: factor.into(),
            center: crate::variance::VarianceCenter::Mean,
            confidence: 0.95,
        }
    }

    /// Bartlett's test of equal variances across the levels of a factor.
    #[must_use]
    pub fn bartlett(response: impl Into<String>, factor: impl Into<String>) -> Self {
        Self::Bartlett {
            response: response.into(),
            factor: factor.into(),
        }
    }

    /// Chi-square goodness-of-fit test against expected counts.
    #[must_use]
    pub fn chi_square_goodness_of_fit(column: impl Into<String>, expected: Vec<f64>) -> Self {
        Self::ChiSquareGoodnessOfFit {
            column: column.into(),
            expected,
            estimated_parameters: 0,
        }
    }

    /// Grubbs' test for a single outlier.
    #[must_use]
    pub fn grubbs(column: impl Into<String>) -> Self {
        Self::Grubbs {
            column: column.into(),
            alpha: 0.05,
        }
    }

    /// Power and sample size of a test.
    #[must_use]
    pub fn power(procedure: crate::power::PowerProcedure, effect: f64, n: usize) -> Self {
        Self::Power {
            procedure,
            effect,
            null_value: 0.0,
            n,
            groups: 2,
            alpha: 0.05,
            power: 0.8,
            alternative: crate::hypothesis::Alternative::TwoSided,
        }
    }

    /// Replaces the number of groups (ANOVA) or predictors (regression).
    #[must_use]
    pub fn with_power_groups(mut self, groups: usize) -> Self {
        if let Self::Power { groups: slot, .. } = &mut self {
            *slot = groups;
        }
        self
    }

    /// Replaces the null proportion of the proportion power analysis.
    #[must_use]
    pub fn with_null_proportion(mut self, null_value: f64) -> Self {
        if let Self::Power {
            null_value: slot, ..
        } = &mut self
        {
            *slot = null_value;
        }
        self
    }

    /// Replaces the requested power.
    #[must_use]
    pub fn with_power(mut self, power: f64) -> Self {
        if let Self::Power { power: slot, .. } = &mut self {
            *slot = power;
        }
        self
    }

    /// Replaces the significance level of the power analysis.
    #[must_use]
    pub fn with_power_alpha(mut self, alpha: f64) -> Self {
        if let Self::Power { alpha: slot, .. } = &mut self {
            *slot = alpha;
        }
        self
    }

    /// Replaces the number of parameters estimated from the data.
    #[must_use]
    pub fn with_estimated_parameters(mut self, parameters: usize) -> Self {
        if let Self::ChiSquareGoodnessOfFit {
            estimated_parameters: slot,
            ..
        } = &mut self
        {
            *slot = parameters;
        }
        self
    }

    /// Replaces the significance level of Grubbs' test.
    #[must_use]
    pub fn with_grubbs_alpha(mut self, alpha: f64) -> Self {
        if let Self::Grubbs { alpha: slot, .. } = &mut self {
            *slot = alpha;
        }
        self
    }

    /// Replaces the centre used by Levene's test.
    #[must_use]
    pub fn with_variance_center(mut self, center: crate::variance::VarianceCenter) -> Self {
        if let Self::Levene { center: slot, .. } = &mut self {
            *slot = center;
        }
        self
    }

    /// Replaces the confidence level of a Levene test.
    #[must_use]
    pub fn with_levene_confidence(mut self, confidence: f64) -> Self {
        if let Self::Levene {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Replaces the confidence level of an equivalence test.
    #[must_use]
    pub fn with_equivalence_confidence(mut self, confidence: f64) -> Self {
        match &mut self {
            Self::OneSampleEquivalence {
                confidence: slot, ..
            }
            | Self::TwoSampleEquivalence {
                confidence: slot, ..
            }
            | Self::PairedEquivalence {
                confidence: slot, ..
            } => *slot = confidence,
            _ => {}
        }
        self
    }

    /// One-sample proportion test against `null_p`, exact by default.
    #[must_use]
    pub fn one_proportion(column: impl Into<String>, null_p: f64) -> Self {
        Self::OneProportion {
            column: column.into(),
            success: 1.0,
            null_p,
            method: crate::proportion::ProportionMethod::Exact,
            alternative: crate::hypothesis::Alternative::TwoSided,
            confidence: 0.95,
        }
    }

    /// Two-sample proportion test.
    #[must_use]
    pub fn two_proportions(first: impl Into<String>, second: impl Into<String>) -> Self {
        Self::TwoProportions {
            first: first.into(),
            second: second.into(),
            success: 1.0,
            alternative: crate::hypothesis::Alternative::TwoSided,
            confidence: 0.95,
        }
    }

    /// One-sample chi-square variance test.
    #[must_use]
    pub fn one_variance(column: impl Into<String>, null_variance: f64) -> Self {
        Self::OneVariance {
            column: column.into(),
            null_variance,
            alternative: crate::hypothesis::Alternative::TwoSided,
            confidence: 0.95,
        }
    }

    /// Two-sample F variance test.
    #[must_use]
    pub fn two_variances(first: impl Into<String>, second: impl Into<String>) -> Self {
        Self::TwoVariances {
            first: first.into(),
            second: second.into(),
            alternative: crate::hypothesis::Alternative::TwoSided,
            confidence: 0.95,
            method: crate::variance::VarianceMethod::F,
        }
    }

    /// Ordinary least squares regression with an intercept.
    #[must_use]
    pub fn regression(response: impl Into<String>, predictors: Vec<String>) -> Self {
        Self::Regression {
            response: response.into(),
            predictors,
            confidence: 0.95,
        }
    }

    /// Stepwise ordinary least squares regression.
    #[must_use]
    pub fn stepwise_regression(response: impl Into<String>, candidates: Vec<String>) -> Self {
        Self::StepwiseRegression {
            response: response.into(),
            candidates,
            method: crate::stepwise::StepMethod::Stepwise,
            criterion: crate::stepwise::StepCriterion::PValue,
            alpha_enter: 0.15,
            alpha_remove: 0.15,
            max_steps: 50,
            confidence: 0.95,
        }
    }

    /// One-way ANOVA of a numeric response by a factor.
    #[must_use]
    pub fn anova(response: impl Into<String>, factor: impl Into<String>) -> Self {
        Self::Anova {
            response: response.into(),
            factor: factor.into(),
            confidence: 0.95,
        }
    }

    /// Kaplan–Meier survival estimate.
    #[must_use]
    pub fn kaplan_meier(time: impl Into<String>) -> Self {
        Self::KaplanMeier {
            time: time.into(),
            censor: None,
            censor_value: 0.0,
            confidence: 0.95,
        }
    }

    /// Right-censored exponential reliability fit.
    #[must_use]
    pub fn exponential_reliability(time: impl Into<String>) -> Self {
        Self::CensoredExponential {
            time: time.into(),
            censor: None,
            censor_value: 0.0,
        }
    }

    /// Right-censored lognormal reliability fit.
    #[must_use]
    pub fn lognormal_reliability(time: impl Into<String>) -> Self {
        Self::CensoredLognormal {
            time: time.into(),
            censor: None,
            censor_value: 0.0,
        }
    }

    /// Right-censored Weibull reliability fit.
    #[must_use]
    pub fn weibull_reliability(time: impl Into<String>) -> Self {
        Self::CensoredWeibull {
            time: time.into(),
            censor: None,
            censor_value: 0.0,
        }
    }

    /// Type 1 Gage study.
    #[must_use]
    pub fn gage_type1(measurement: impl Into<String>, reference: f64) -> Self {
        Self::GageType1 {
            measurement: measurement.into(),
            reference,
            lsl: None,
            usl: None,
        }
    }

    /// Ordinal logistic regression.
    #[must_use]
    pub fn ordinal(response: impl Into<String>, predictors: Vec<String>) -> Self {
        Self::Ordinal {
            response: response.into(),
            predictors,
        }
    }

    /// Poisson regression.
    #[must_use]
    pub fn poisson(response: impl Into<String>, predictors: Vec<String>) -> Self {
        Self::Poisson {
            response: response.into(),
            predictors,
        }
    }

    /// Binary logistic regression.
    #[must_use]
    pub fn logistic(response: impl Into<String>, predictors: Vec<String>) -> Self {
        Self::Logistic {
            response: response.into(),
            predictors,
        }
    }

    /// Attribute agreement analysis.
    #[must_use]
    pub fn agreement(
        part: impl Into<String>,
        appraiser: impl Into<String>,
        rating: impl Into<String>,
    ) -> Self {
        Self::Agreement {
            part: part.into(),
            appraiser: appraiser.into(),
            rating: rating.into(),
        }
    }

    /// Gage linearity and bias study.
    #[must_use]
    pub fn gage_linearity(measurement: impl Into<String>, reference: impl Into<String>) -> Self {
        Self::GageLinearity {
            measurement: measurement.into(),
            reference: reference.into(),
        }
    }

    /// Supplies optional specification limits for a Type 1 study.
    #[must_use]
    pub fn with_gage_limits(mut self, lsl: Option<f64>, usl: Option<f64>) -> Self {
        if let Self::GageType1 {
            lsl: lsl_slot,
            usl: usl_slot,
            ..
        } = &mut self
        {
            *lsl_slot = lsl;
            *usl_slot = usl;
        }
        self
    }

    /// Crossed Gage R&R.
    #[must_use]
    pub fn gage_rr(
        measurement: impl Into<String>,
        part: impl Into<String>,
        operator: impl Into<String>,
    ) -> Self {
        Self::GageRr {
            measurement: measurement.into(),
            part: part.into(),
            operator: operator.into(),
            design: crate::gage::GageDesign::Crossed,
        }
    }

    /// Selects the crossed or nested Gage R&R design.
    #[must_use]
    pub fn with_gage_design(mut self, design: crate::gage::GageDesign) -> Self {
        if let Self::GageRr { design: slot, .. } = &mut self {
            *slot = design;
        }
        self
    }

    /// Two-level full factorial analysis.
    #[must_use]
    pub fn factorial(response: impl Into<String>, factors: Vec<String>) -> Self {
        Self::Factorial {
            response: response.into(),
            factors,
            main_effects_only: false,
            block: None,
        }
    }

    /// Adds a block column; the block effect is fitted as a separate term.
    #[must_use]
    pub fn with_block(mut self, column: impl Into<String>) -> Self {
        if let Self::Factorial { block, .. } = &mut self {
            *block = Some(column.into());
        }
        self
    }

    /// Selects a main-effects-only fit for fractional designs.
    #[must_use]
    pub fn with_main_effects(mut self, enabled: bool) -> Self {
        if let Self::Factorial {
            main_effects_only, ..
        } = &mut self
        {
            *main_effects_only = enabled;
        }
        self
    }

    /// Quadratic response-surface analysis.
    #[must_use]
    pub fn response_surface(response: impl Into<String>, factors: Vec<String>) -> Self {
        Self::ResponseSurface {
            response: response.into(),
            factors,
            optimize: false,
            contours: false,
            steepest_ascent: false,
        }
    }

    /// Enables stationary-point/eigenvalue analysis for a response surface.
    #[must_use]
    pub fn with_rsm_optimization(mut self, enabled: bool) -> Self {
        if let Self::ResponseSurface { optimize, .. } = &mut self {
            *optimize = enabled;
        }
        self
    }

    /// Enables a contour plot for a two-factor response surface.
    #[must_use]
    pub fn with_rsm_contours(mut self, enabled: bool) -> Self {
        if let Self::ResponseSurface { contours, .. } = &mut self {
            *contours = enabled;
        }
        self
    }

    /// Uses a column of subgroup labels instead of a fixed subgroup size.
    #[must_use]
    pub fn with_subgroup_column(mut self, column: Option<String>) -> Self {
        if let Self::ControlChart {
            subgroup_column, ..
        } = &mut self
        {
            *subgroup_column = column;
        }
        self
    }

    /// Uses a column of phase labels: each consecutive run gets its own limits.
    #[must_use]
    pub fn with_phase_column(mut self, column: Option<String>) -> Self {
        if let Self::ControlChart { phase_column, .. } = &mut self {
            *phase_column = column;
        }
        self
    }

    /// Uses a failure-mode column and fits cause-specific competing risks.
    #[must_use]
    pub fn with_competing_mode(mut self, mode: Option<String>) -> Self {
        if let Self::AcceleratedLife { mode: slot, .. } = &mut self {
            *slot = mode;
        }
        self
    }

    /// Sets the time at which the competing-risks incidence is reported.
    #[must_use]
    pub fn with_competing_horizon(mut self, horizon: Option<f64>) -> Self {
        if let Self::AcceleratedLife { horizon: slot, .. } = &mut self {
            *slot = horizon;
        }
        self
    }

    /// Enables the steepest-ascent path for a response surface.
    #[must_use]
    pub fn with_rsm_steepest_ascent(mut self, enabled: bool) -> Self {
        if let Self::ResponseSurface {
            steepest_ascent, ..
        } = &mut self
        {
            *steepest_ascent = enabled;
        }
        self
    }

    /// Step-stress accelerated life testing.
    #[must_use]
    pub fn step_stress_alt(
        duration: impl Into<String>,
        stress: impl Into<String>,
        unit: impl Into<String>,
        event: impl Into<String>,
        use_stress: f64,
        life: crate::alt::LifeModel,
        acceleration: crate::alt::AccelerationModel,
    ) -> Self {
        Self::StepStressAlt {
            duration: duration.into(),
            stress: stress.into(),
            unit: unit.into(),
            event: event.into(),
            use_stress,
            life,
            acceleration,
        }
    }

    /// Multi-response desirability optimisation.
    #[must_use]
    pub fn desirability(
        factors: Vec<String>,
        responses: Vec<crate::desirability::Response>,
    ) -> Self {
        Self::Desirability { factors, responses }
    }

    /// Boxplot of one numeric column.
    #[must_use]
    pub fn boxplot(column: impl Into<String>) -> Self {
        Self::Boxplot {
            column: column.into(),
            by: None,
        }
    }

    /// Boxplot of one numeric column split by a grouping column.
    #[must_use]
    pub fn boxplot_by(column: impl Into<String>, by: impl Into<String>) -> Self {
        Self::Boxplot {
            column: column.into(),
            by: Some(by.into()),
        }
    }

    /// Scatterplot of `y` against `x`.
    #[must_use]
    pub fn scatterplot(x: impl Into<String>, y: impl Into<String>) -> Self {
        Self::Scatterplot {
            x: x.into(),
            y: y.into(),
            fit: true,
        }
    }

    /// Histogram with automatic bins.
    #[must_use]
    pub fn histogram(column: impl Into<String>) -> Self {
        Self::Histogram {
            column: column.into(),
            bins: None,
        }
    }

    /// Replaces the histogram bin count.
    #[must_use]
    pub fn with_bins(mut self, bins: Option<usize>) -> Self {
        if let Self::Histogram { bins: slot, .. } = &mut self {
            *slot = bins;
        }
        self
    }

    /// Supplies the censoring column and the value that marks censoring.
    #[must_use]
    pub fn with_censoring(mut self, censor: Option<String>, censor_value: f64) -> Self {
        match &mut self {
            Self::KaplanMeier {
                censor: censor_slot,
                censor_value: value_slot,
                ..
            }
            | Self::CensoredWeibull {
                censor: censor_slot,
                censor_value: value_slot,
                ..
            }
            | Self::CensoredExponential {
                censor: censor_slot,
                censor_value: value_slot,
                ..
            }
            | Self::CensoredLognormal {
                censor: censor_slot,
                censor_value: value_slot,
                ..
            } => {
                *censor_slot = censor;
                *value_slot = censor_value;
            }
            _ => {}
        }
        self
    }

    /// Replaces the confidence level of the survival intervals.
    #[must_use]
    pub fn with_reliability_confidence(mut self, confidence: f64) -> Self {
        if let Self::KaplanMeier {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Replaces the confidence level of an ANOVA interval.
    #[must_use]
    pub fn with_anova_confidence(mut self, confidence: f64) -> Self {
        if let Self::Anova {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Replaces the confidence level of a regression interval.
    #[must_use]
    pub fn with_regression_confidence(mut self, confidence: f64) -> Self {
        if let Self::Regression {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        if let Self::StepwiseRegression {
            confidence: slot, ..
        } = &mut self
        {
            *slot = confidence;
        }
        self
    }

    /// Replaces the search direction of a stepwise regression.
    #[must_use]
    pub fn with_stepwise_method(mut self, method: crate::stepwise::StepMethod) -> Self {
        if let Self::StepwiseRegression { method: slot, .. } = &mut self {
            *slot = method;
        }
        self
    }

    /// Replaces the criterion of a stepwise regression.
    #[must_use]
    pub fn with_stepwise_criterion(mut self, criterion: crate::stepwise::StepCriterion) -> Self {
        if let Self::StepwiseRegression {
            criterion: slot, ..
        } = &mut self
        {
            *slot = criterion;
        }
        self
    }

    /// Replaces the entry and removal alphas of a stepwise regression.
    #[must_use]
    pub fn with_stepwise_alphas(mut self, enter: f64, remove: f64) -> Self {
        if let Self::StepwiseRegression {
            alpha_enter,
            alpha_remove,
            ..
        } = &mut self
        {
            *alpha_enter = enter;
            *alpha_remove = remove;
        }
        self
    }

    /// Replaces the maximum number of steps of a stepwise regression.
    #[must_use]
    pub fn with_stepwise_max_steps(mut self, max_steps: usize) -> Self {
        if let Self::StepwiseRegression {
            max_steps: slot, ..
        } = &mut self
        {
            *slot = max_steps;
        }
        self
    }

    /// Selects the value counted as a success in a proportion analysis.
    #[must_use]
    pub fn with_success(mut self, success: f64) -> Self {
        match &mut self {
            Self::OneProportion { success: slot, .. }
            | Self::TwoProportions { success: slot, .. } => *slot = success,
            _ => {}
        }
        self
    }

    /// Selects the exact or normal one-sample proportion method.
    #[must_use]
    pub fn with_proportion_method(mut self, method: crate::proportion::ProportionMethod) -> Self {
        if let Self::OneProportion { method: slot, .. } = &mut self {
            *slot = method;
        }
        self
    }

    /// Replaces the confidence level of a test interval.
    #[must_use]
    pub fn with_test_confidence(mut self, confidence: f64) -> Self {
        match &mut self {
            Self::OneProportion {
                confidence: slot, ..
            }
            | Self::TwoProportions {
                confidence: slot, ..
            }
            | Self::OneVariance {
                confidence: slot, ..
            }
            | Self::TwoVariances {
                confidence: slot, ..
            } => *slot = confidence,
            _ => {}
        }
        self
    }

    /// Selects the two-sample variance method.
    #[must_use]
    pub fn with_variance_method(mut self, method: crate::variance::VarianceMethod) -> Self {
        if let Self::TwoVariances { method: slot, .. } = &mut self {
            *slot = method;
        }
        self
    }

    /// Replaces the alternative hypothesis.
    #[must_use]
    pub fn with_alternative(mut self, alternative: crate::hypothesis::Alternative) -> Self {
        match &mut self {
            Self::OneSampleT {
                alternative: slot, ..
            }
            | Self::TwoSampleT {
                alternative: slot, ..
            }
            | Self::PairedT {
                alternative: slot, ..
            }
            | Self::OneProportion {
                alternative: slot, ..
            }
            | Self::TwoProportions {
                alternative: slot, ..
            }
            | Self::OneVariance {
                alternative: slot, ..
            }
            | Self::TwoVariances {
                alternative: slot, ..
            }
            | Self::MannWhitney {
                alternative: slot, ..
            }
            | Self::WilcoxonSignedRank {
                alternative: slot, ..
            } => *slot = alternative,
            _ => {}
        }
        self
    }

    /// Selects the pooled-variance two-sample test.
    #[must_use]
    pub fn with_pooled(mut self, pooled: bool) -> Self {
        if let Self::TwoSampleT { pooled: slot, .. } = &mut self {
            *slot = pooled;
        }
        self
    }

    /// Replaces the confidence level of a t-test interval.
    #[must_use]
    pub fn with_t_confidence(mut self, confidence: f64) -> Self {
        match &mut self {
            Self::OneSampleT {
                confidence: slot, ..
            }
            | Self::TwoSampleT {
                confidence: slot, ..
            }
            | Self::PairedT {
                confidence: slot, ..
            } => *slot = confidence,
            _ => {}
        }
        self
    }

    /// Column used by the analysis.
    #[must_use]
    pub fn column(&self) -> &str {
        match self {
            Self::ProbabilityPlot { columns, .. } => columns.first().map_or("", String::as_str),
            Self::NormalityTest { column, .. }
            | Self::DistributionComparison { column }
            | Self::DescriptiveStatistics { column, .. }
            | Self::ControlChart { column, .. }
            | Self::RunChart { column }
            | Self::OneSampleT { column, .. }
            | Self::OneSampleEquivalence { column, .. }
            | Self::OneProportion { column, .. }
            | Self::OneVariance { column, .. }
            | Self::Capability { column, .. }
            | Self::ChiSquareGoodnessOfFit { column, .. }
            | Self::Grubbs { column, .. } => column,
            Self::Regression { response, .. }
            | Self::StepwiseRegression { response, .. }
            | Self::Anova { response, .. }
            | Self::KruskalWallis { response, .. }
            | Self::MoodsMedian { response, .. }
            | Self::Levene { response, .. }
            | Self::Bartlett { response, .. } => response,
            Self::KaplanMeier { time, .. }
            | Self::CensoredWeibull { time, .. }
            | Self::CensoredExponential { time, .. }
            | Self::CensoredLognormal { time, .. } => time,
            Self::GageRr { measurement, .. }
            | Self::GageType1 { measurement, .. }
            | Self::GageLinearity { measurement, .. } => measurement,
            Self::Factorial { response, .. } => response,
            Self::ResponseSurface { response, .. } => response,
            Self::Desirability { factors, .. } => factors.first().map_or("", String::as_str),
            Self::Boxplot { column, .. } | Self::Histogram { column, .. } => column,
            Self::Scatterplot { x, .. } => x,
            Self::AttributeChart { defects, .. } => defects,
            Self::Friedman { columns } | Self::Correlation { columns } => {
                columns.first().map_or("", String::as_str)
            }
            Self::CurveFit { x_column, .. } => x_column,
            Self::PartialCorrelation { x_column, .. } => x_column,
            Self::TwoWayAnova { response, .. } => response,
            Self::ChiSquareIndependence { row, .. } => row,
            Self::BoxCox { column } => column,
            Self::ToleranceInterval { column, .. } => column,
            Self::RareEvents { counts, .. } => counts,
            Self::Power { .. } => "",
            Self::TwoSampleT { first, .. }
            | Self::PairedT { first, .. }
            | Self::TwoSampleEquivalence { first, .. }
            | Self::PairedEquivalence { first, .. }
            | Self::MannWhitney { first, .. }
            | Self::WilcoxonSignedRank { first, .. }
            | Self::TwoProportions { first, .. }
            | Self::TwoVariances { first, .. } => first,
            Self::Logistic { response, .. }
            | Self::Poisson { response, .. }
            | Self::Ordinal { response, .. } => response,
            Self::TimeSeries { column, .. } => column,
            Self::Agreement { part, .. } => part,
            Self::Pca { columns, .. }
            | Self::KMeans { columns, .. }
            | Self::HotellingT2 { columns, .. } => columns.first().map_or("", String::as_str),
            Self::AcceptanceSampling { .. } => "",
            Self::Taguchi { response, .. } => response,
            Self::Pareto { column, .. } => column,
            Self::IntervalPlot { response, .. } => response,
            Self::MatrixPlot { columns, .. } => columns.first().map_or("", String::as_str),
            Self::Dotplot { column, .. }
            | Self::StemLeaf { column }
            | Self::Ecdf { column, .. }
            | Self::SeriesPlot { column, .. } => column,
            Self::SignTest { column, .. } => column,
            Self::AcceleratedLife { time, .. } => time,
            Self::StepStressAlt { duration, .. } => duration,
        }
    }

    /// Y columns of a probability plot (one for a single-variable analysis).
    #[must_use]
    pub fn columns(&self) -> &[String] {
        match self {
            Self::ProbabilityPlot { columns, .. } => columns,
            Self::NormalityTest { .. }
            | Self::DistributionComparison { .. }
            | Self::DescriptiveStatistics { .. }
            | Self::ControlChart { .. }
            | Self::RunChart { .. }
            | Self::AttributeChart { .. }
            | Self::OneSampleT { .. }
            | Self::TwoSampleT { .. }
            | Self::PairedT { .. }
            | Self::OneSampleEquivalence { .. }
            | Self::TwoSampleEquivalence { .. }
            | Self::PairedEquivalence { .. }
            | Self::MannWhitney { .. }
            | Self::WilcoxonSignedRank { .. }
            | Self::KruskalWallis { .. }
            | Self::Friedman { .. }
            | Self::MoodsMedian { .. }
            | Self::Correlation { .. }
            | Self::PartialCorrelation { .. }
            | Self::CurveFit { .. }
            | Self::TwoWayAnova { .. }
            | Self::ChiSquareIndependence { .. }
            | Self::BoxCox { .. }
            | Self::ToleranceInterval { .. }
            | Self::RareEvents { .. }
            | Self::Levene { .. }
            | Self::Bartlett { .. }
            | Self::ChiSquareGoodnessOfFit { .. }
            | Self::Grubbs { .. }
            | Self::Power { .. }
            | Self::OneProportion { .. }
            | Self::TwoProportions { .. }
            | Self::OneVariance { .. }
            | Self::TwoVariances { .. }
            | Self::Regression { .. }
            | Self::StepwiseRegression { .. }
            | Self::Anova { .. }
            | Self::KaplanMeier { .. }
            | Self::CensoredWeibull { .. }
            | Self::CensoredExponential { .. }
            | Self::CensoredLognormal { .. }
            | Self::GageRr { .. }
            | Self::GageType1 { .. }
            | Self::GageLinearity { .. }
            | Self::Logistic { .. }
            | Self::Ordinal { .. }
            | Self::Poisson { .. }
            | Self::TimeSeries { .. }
            | Self::Pca { .. }
            | Self::KMeans { .. }
            | Self::HotellingT2 { .. }
            | Self::AcceptanceSampling { .. }
            | Self::Taguchi { .. }
            | Self::Pareto { .. }
            | Self::IntervalPlot { .. }
            | Self::MatrixPlot { .. }
            | Self::Dotplot { .. }
            | Self::StemLeaf { .. }
            | Self::SeriesPlot { .. }
            | Self::Ecdf { .. }
            | Self::SignTest { .. }
            | Self::AcceleratedLife { .. }
            | Self::StepStressAlt { .. }
            | Self::Agreement { .. }
            | Self::Factorial { .. }
            | Self::ResponseSurface { .. }
            | Self::Desirability { .. }
            | Self::Boxplot { .. }
            | Self::Scatterplot { .. }
            | Self::Histogram { .. }
            | Self::Capability { .. } => &[],
        }
    }

    /// Sets the grouping column (single Y only).
    #[must_use]
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        match &mut self {
            Self::ProbabilityPlot { group: slot, .. } => *slot = Some(group.into()),
            Self::DescriptiveStatistics { by: slot, .. } => *slot = Some(group.into()),
            Self::NormalityTest { .. }
            | Self::DistributionComparison { .. }
            | Self::ControlChart { .. }
            | Self::RunChart { .. }
            | Self::AttributeChart { .. }
            | Self::OneSampleT { .. }
            | Self::TwoSampleT { .. }
            | Self::PairedT { .. }
            | Self::OneSampleEquivalence { .. }
            | Self::TwoSampleEquivalence { .. }
            | Self::PairedEquivalence { .. }
            | Self::MannWhitney { .. }
            | Self::WilcoxonSignedRank { .. }
            | Self::KruskalWallis { .. }
            | Self::Friedman { .. }
            | Self::MoodsMedian { .. }
            | Self::Correlation { .. }
            | Self::PartialCorrelation { .. }
            | Self::CurveFit { .. }
            | Self::TwoWayAnova { .. }
            | Self::ChiSquareIndependence { .. }
            | Self::BoxCox { .. }
            | Self::ToleranceInterval { .. }
            | Self::RareEvents { .. }
            | Self::Levene { .. }
            | Self::Bartlett { .. }
            | Self::ChiSquareGoodnessOfFit { .. }
            | Self::Grubbs { .. }
            | Self::Power { .. }
            | Self::OneProportion { .. }
            | Self::TwoProportions { .. }
            | Self::OneVariance { .. }
            | Self::TwoVariances { .. }
            | Self::Regression { .. }
            | Self::StepwiseRegression { .. }
            | Self::Anova { .. }
            | Self::KaplanMeier { .. }
            | Self::CensoredWeibull { .. }
            | Self::CensoredExponential { .. }
            | Self::CensoredLognormal { .. }
            | Self::GageRr { .. }
            | Self::GageType1 { .. }
            | Self::GageLinearity { .. }
            | Self::Logistic { .. }
            | Self::Ordinal { .. }
            | Self::Poisson { .. }
            | Self::TimeSeries { .. }
            | Self::Pca { .. }
            | Self::KMeans { .. }
            | Self::HotellingT2 { .. }
            | Self::AcceptanceSampling { .. }
            | Self::Taguchi { .. }
            | Self::Pareto { .. }
            | Self::IntervalPlot { .. }
            | Self::MatrixPlot { .. }
            | Self::Dotplot { .. }
            | Self::StemLeaf { .. }
            | Self::SeriesPlot { .. }
            | Self::Ecdf { .. }
            | Self::SignTest { .. }
            | Self::AcceleratedLife { .. }
            | Self::StepStressAlt { .. }
            | Self::Agreement { .. }
            | Self::Factorial { .. }
            | Self::ResponseSurface { .. }
            | Self::Desirability { .. }
            | Self::Boxplot { .. }
            | Self::Scatterplot { .. }
            | Self::Histogram { .. } => {}
            Self::Capability { .. } => {}
        }
        self
    }

    /// Replaces the distribution family.
    #[must_use]
    pub fn with_distribution(mut self, distribution: DistributionKind) -> Self {
        match &mut self {
            Self::ProbabilityPlot {
                distribution: slot, ..
            } => *slot = distribution,
            Self::DescriptiveStatistics { .. }
            | Self::NormalityTest { .. }
            | Self::DistributionComparison { .. }
            | Self::ControlChart { .. }
            | Self::RunChart { .. }
            | Self::AttributeChart { .. }
            | Self::OneSampleT { .. }
            | Self::TwoSampleT { .. }
            | Self::PairedT { .. }
            | Self::OneSampleEquivalence { .. }
            | Self::TwoSampleEquivalence { .. }
            | Self::PairedEquivalence { .. }
            | Self::MannWhitney { .. }
            | Self::WilcoxonSignedRank { .. }
            | Self::KruskalWallis { .. }
            | Self::Friedman { .. }
            | Self::MoodsMedian { .. }
            | Self::Correlation { .. }
            | Self::PartialCorrelation { .. }
            | Self::CurveFit { .. }
            | Self::TwoWayAnova { .. }
            | Self::ChiSquareIndependence { .. }
            | Self::BoxCox { .. }
            | Self::ToleranceInterval { .. }
            | Self::RareEvents { .. }
            | Self::Levene { .. }
            | Self::Bartlett { .. }
            | Self::ChiSquareGoodnessOfFit { .. }
            | Self::Grubbs { .. }
            | Self::Power { .. }
            | Self::OneProportion { .. }
            | Self::TwoProportions { .. }
            | Self::OneVariance { .. }
            | Self::TwoVariances { .. }
            | Self::Regression { .. }
            | Self::StepwiseRegression { .. }
            | Self::Anova { .. }
            | Self::KaplanMeier { .. }
            | Self::CensoredWeibull { .. }
            | Self::CensoredExponential { .. }
            | Self::CensoredLognormal { .. }
            | Self::GageRr { .. }
            | Self::GageType1 { .. }
            | Self::GageLinearity { .. }
            | Self::Logistic { .. }
            | Self::Ordinal { .. }
            | Self::Poisson { .. }
            | Self::TimeSeries { .. }
            | Self::Pca { .. }
            | Self::KMeans { .. }
            | Self::HotellingT2 { .. }
            | Self::AcceptanceSampling { .. }
            | Self::Taguchi { .. }
            | Self::Pareto { .. }
            | Self::IntervalPlot { .. }
            | Self::MatrixPlot { .. }
            | Self::Dotplot { .. }
            | Self::StemLeaf { .. }
            | Self::SeriesPlot { .. }
            | Self::Ecdf { .. }
            | Self::SignTest { .. }
            | Self::AcceleratedLife { .. }
            | Self::StepStressAlt { .. }
            | Self::Agreement { .. }
            | Self::Factorial { .. }
            | Self::ResponseSurface { .. }
            | Self::Desirability { .. }
            | Self::Boxplot { .. }
            | Self::Scatterplot { .. }
            | Self::Histogram { .. } => {}
            Self::Capability { .. } => {}
        }
        self
    }

    /// Replaces the p-value method.
    #[must_use]
    pub fn with_p_value(mut self, p_value: PValueMethod) -> Self {
        match &mut self {
            Self::ProbabilityPlot { p_value: slot, .. } => *slot = p_value,
            Self::DescriptiveStatistics { .. }
            | Self::NormalityTest { .. }
            | Self::DistributionComparison { .. }
            | Self::ControlChart { .. }
            | Self::RunChart { .. }
            | Self::AttributeChart { .. }
            | Self::OneSampleT { .. }
            | Self::TwoSampleT { .. }
            | Self::PairedT { .. }
            | Self::OneSampleEquivalence { .. }
            | Self::TwoSampleEquivalence { .. }
            | Self::PairedEquivalence { .. }
            | Self::MannWhitney { .. }
            | Self::WilcoxonSignedRank { .. }
            | Self::KruskalWallis { .. }
            | Self::Friedman { .. }
            | Self::MoodsMedian { .. }
            | Self::Correlation { .. }
            | Self::PartialCorrelation { .. }
            | Self::CurveFit { .. }
            | Self::TwoWayAnova { .. }
            | Self::ChiSquareIndependence { .. }
            | Self::BoxCox { .. }
            | Self::ToleranceInterval { .. }
            | Self::RareEvents { .. }
            | Self::Levene { .. }
            | Self::Bartlett { .. }
            | Self::ChiSquareGoodnessOfFit { .. }
            | Self::Grubbs { .. }
            | Self::Power { .. }
            | Self::OneProportion { .. }
            | Self::TwoProportions { .. }
            | Self::OneVariance { .. }
            | Self::TwoVariances { .. }
            | Self::Regression { .. }
            | Self::StepwiseRegression { .. }
            | Self::Anova { .. }
            | Self::KaplanMeier { .. }
            | Self::CensoredWeibull { .. }
            | Self::CensoredExponential { .. }
            | Self::CensoredLognormal { .. }
            | Self::GageRr { .. }
            | Self::GageType1 { .. }
            | Self::GageLinearity { .. }
            | Self::Logistic { .. }
            | Self::Ordinal { .. }
            | Self::Poisson { .. }
            | Self::TimeSeries { .. }
            | Self::Pca { .. }
            | Self::KMeans { .. }
            | Self::HotellingT2 { .. }
            | Self::AcceptanceSampling { .. }
            | Self::Taguchi { .. }
            | Self::Pareto { .. }
            | Self::IntervalPlot { .. }
            | Self::MatrixPlot { .. }
            | Self::Dotplot { .. }
            | Self::StemLeaf { .. }
            | Self::SeriesPlot { .. }
            | Self::Ecdf { .. }
            | Self::SignTest { .. }
            | Self::AcceleratedLife { .. }
            | Self::StepStressAlt { .. }
            | Self::Agreement { .. }
            | Self::Factorial { .. }
            | Self::ResponseSurface { .. }
            | Self::Desirability { .. }
            | Self::Boxplot { .. }
            | Self::Scatterplot { .. }
            | Self::Histogram { .. } => {}
            Self::Capability { .. } => {}
        }
        self
    }

    /// Replaces the confidence band setting (`None` disables it).
    #[must_use]
    pub fn with_confidence(mut self, confidence: Option<f64>) -> Self {
        match &mut self {
            Self::ProbabilityPlot {
                confidence: slot, ..
            } => *slot = confidence,
            Self::DescriptiveStatistics { .. }
            | Self::NormalityTest { .. }
            | Self::DistributionComparison { .. }
            | Self::ControlChart { .. }
            | Self::RunChart { .. }
            | Self::AttributeChart { .. }
            | Self::OneSampleT { .. }
            | Self::TwoSampleT { .. }
            | Self::PairedT { .. }
            | Self::OneSampleEquivalence { .. }
            | Self::TwoSampleEquivalence { .. }
            | Self::PairedEquivalence { .. }
            | Self::MannWhitney { .. }
            | Self::WilcoxonSignedRank { .. }
            | Self::KruskalWallis { .. }
            | Self::Friedman { .. }
            | Self::MoodsMedian { .. }
            | Self::Correlation { .. }
            | Self::PartialCorrelation { .. }
            | Self::CurveFit { .. }
            | Self::TwoWayAnova { .. }
            | Self::ChiSquareIndependence { .. }
            | Self::BoxCox { .. }
            | Self::ToleranceInterval { .. }
            | Self::RareEvents { .. }
            | Self::Levene { .. }
            | Self::Bartlett { .. }
            | Self::ChiSquareGoodnessOfFit { .. }
            | Self::Grubbs { .. }
            | Self::Power { .. }
            | Self::OneProportion { .. }
            | Self::TwoProportions { .. }
            | Self::OneVariance { .. }
            | Self::TwoVariances { .. }
            | Self::Regression { .. }
            | Self::StepwiseRegression { .. }
            | Self::Anova { .. }
            | Self::KaplanMeier { .. }
            | Self::CensoredWeibull { .. }
            | Self::CensoredExponential { .. }
            | Self::CensoredLognormal { .. }
            | Self::GageRr { .. }
            | Self::GageType1 { .. }
            | Self::GageLinearity { .. }
            | Self::Logistic { .. }
            | Self::Ordinal { .. }
            | Self::Poisson { .. }
            | Self::TimeSeries { .. }
            | Self::Pca { .. }
            | Self::KMeans { .. }
            | Self::HotellingT2 { .. }
            | Self::AcceptanceSampling { .. }
            | Self::Taguchi { .. }
            | Self::Pareto { .. }
            | Self::IntervalPlot { .. }
            | Self::MatrixPlot { .. }
            | Self::Dotplot { .. }
            | Self::StemLeaf { .. }
            | Self::SeriesPlot { .. }
            | Self::Ecdf { .. }
            | Self::SignTest { .. }
            | Self::AcceleratedLife { .. }
            | Self::StepStressAlt { .. }
            | Self::Agreement { .. }
            | Self::Factorial { .. }
            | Self::ResponseSurface { .. }
            | Self::Desirability { .. }
            | Self::Boxplot { .. }
            | Self::Scatterplot { .. }
            | Self::Histogram { .. } => {}
            Self::Capability { .. } => {}
        }
        self
    }

    /// Replaces the display choice.
    #[must_use]
    pub fn with_display(mut self, display: PlotDisplay) -> Self {
        match &mut self {
            Self::ProbabilityPlot { display: slot, .. } => *slot = display,
            Self::DescriptiveStatistics { .. }
            | Self::NormalityTest { .. }
            | Self::DistributionComparison { .. }
            | Self::ControlChart { .. }
            | Self::RunChart { .. }
            | Self::AttributeChart { .. }
            | Self::OneSampleT { .. }
            | Self::TwoSampleT { .. }
            | Self::PairedT { .. }
            | Self::OneSampleEquivalence { .. }
            | Self::TwoSampleEquivalence { .. }
            | Self::PairedEquivalence { .. }
            | Self::MannWhitney { .. }
            | Self::WilcoxonSignedRank { .. }
            | Self::KruskalWallis { .. }
            | Self::Friedman { .. }
            | Self::MoodsMedian { .. }
            | Self::Correlation { .. }
            | Self::PartialCorrelation { .. }
            | Self::CurveFit { .. }
            | Self::TwoWayAnova { .. }
            | Self::ChiSquareIndependence { .. }
            | Self::BoxCox { .. }
            | Self::ToleranceInterval { .. }
            | Self::RareEvents { .. }
            | Self::Levene { .. }
            | Self::Bartlett { .. }
            | Self::ChiSquareGoodnessOfFit { .. }
            | Self::Grubbs { .. }
            | Self::Power { .. }
            | Self::OneProportion { .. }
            | Self::TwoProportions { .. }
            | Self::OneVariance { .. }
            | Self::TwoVariances { .. }
            | Self::Regression { .. }
            | Self::StepwiseRegression { .. }
            | Self::Anova { .. }
            | Self::KaplanMeier { .. }
            | Self::CensoredWeibull { .. }
            | Self::CensoredExponential { .. }
            | Self::CensoredLognormal { .. }
            | Self::GageRr { .. }
            | Self::GageType1 { .. }
            | Self::GageLinearity { .. }
            | Self::Logistic { .. }
            | Self::Ordinal { .. }
            | Self::Poisson { .. }
            | Self::TimeSeries { .. }
            | Self::Pca { .. }
            | Self::KMeans { .. }
            | Self::HotellingT2 { .. }
            | Self::AcceptanceSampling { .. }
            | Self::Taguchi { .. }
            | Self::Pareto { .. }
            | Self::IntervalPlot { .. }
            | Self::MatrixPlot { .. }
            | Self::Dotplot { .. }
            | Self::StemLeaf { .. }
            | Self::SeriesPlot { .. }
            | Self::Ecdf { .. }
            | Self::SignTest { .. }
            | Self::AcceleratedLife { .. }
            | Self::StepStressAlt { .. }
            | Self::Agreement { .. }
            | Self::Factorial { .. }
            | Self::ResponseSurface { .. }
            | Self::Desirability { .. }
            | Self::Boxplot { .. }
            | Self::Scatterplot { .. }
            | Self::Histogram { .. } => {}
            Self::Capability { .. } => {}
        }
        self
    }

    /// Replaces the multi-Y layout.
    #[must_use]
    pub fn with_layout(mut self, layout: PlotLayout) -> Self {
        match &mut self {
            Self::ProbabilityPlot { layout: slot, .. } => *slot = layout,
            Self::DescriptiveStatistics { .. }
            | Self::NormalityTest { .. }
            | Self::DistributionComparison { .. }
            | Self::ControlChart { .. }
            | Self::RunChart { .. }
            | Self::AttributeChart { .. }
            | Self::OneSampleT { .. }
            | Self::TwoSampleT { .. }
            | Self::PairedT { .. }
            | Self::OneSampleEquivalence { .. }
            | Self::TwoSampleEquivalence { .. }
            | Self::PairedEquivalence { .. }
            | Self::MannWhitney { .. }
            | Self::WilcoxonSignedRank { .. }
            | Self::KruskalWallis { .. }
            | Self::Friedman { .. }
            | Self::MoodsMedian { .. }
            | Self::Correlation { .. }
            | Self::PartialCorrelation { .. }
            | Self::CurveFit { .. }
            | Self::TwoWayAnova { .. }
            | Self::ChiSquareIndependence { .. }
            | Self::BoxCox { .. }
            | Self::ToleranceInterval { .. }
            | Self::RareEvents { .. }
            | Self::Levene { .. }
            | Self::Bartlett { .. }
            | Self::ChiSquareGoodnessOfFit { .. }
            | Self::Grubbs { .. }
            | Self::Power { .. }
            | Self::OneProportion { .. }
            | Self::TwoProportions { .. }
            | Self::OneVariance { .. }
            | Self::TwoVariances { .. }
            | Self::Regression { .. }
            | Self::StepwiseRegression { .. }
            | Self::Anova { .. }
            | Self::KaplanMeier { .. }
            | Self::CensoredWeibull { .. }
            | Self::CensoredExponential { .. }
            | Self::CensoredLognormal { .. }
            | Self::GageRr { .. }
            | Self::GageType1 { .. }
            | Self::GageLinearity { .. }
            | Self::Logistic { .. }
            | Self::Ordinal { .. }
            | Self::Poisson { .. }
            | Self::TimeSeries { .. }
            | Self::Pca { .. }
            | Self::KMeans { .. }
            | Self::HotellingT2 { .. }
            | Self::AcceptanceSampling { .. }
            | Self::Taguchi { .. }
            | Self::Pareto { .. }
            | Self::IntervalPlot { .. }
            | Self::MatrixPlot { .. }
            | Self::Dotplot { .. }
            | Self::StemLeaf { .. }
            | Self::SeriesPlot { .. }
            | Self::Ecdf { .. }
            | Self::SignTest { .. }
            | Self::AcceleratedLife { .. }
            | Self::StepStressAlt { .. }
            | Self::Agreement { .. }
            | Self::Factorial { .. }
            | Self::ResponseSurface { .. }
            | Self::Desirability { .. }
            | Self::Boxplot { .. }
            | Self::Scatterplot { .. }
            | Self::Histogram { .. } => {}
            Self::Capability { .. } => {}
        }
        self
    }

    /// Shares the X and/or Y scale across multi-Y panels.
    #[must_use]
    pub fn with_same_scale(mut self, x: bool, y: bool) -> Self {
        match &mut self {
            Self::ProbabilityPlot {
                same_x: sx,
                same_y: sy,
                ..
            } => {
                *sx = x;
                *sy = y;
            }
            Self::DescriptiveStatistics { .. }
            | Self::NormalityTest { .. }
            | Self::DistributionComparison { .. }
            | Self::ControlChart { .. }
            | Self::RunChart { .. }
            | Self::AttributeChart { .. }
            | Self::OneSampleT { .. }
            | Self::TwoSampleT { .. }
            | Self::PairedT { .. }
            | Self::OneSampleEquivalence { .. }
            | Self::TwoSampleEquivalence { .. }
            | Self::PairedEquivalence { .. }
            | Self::MannWhitney { .. }
            | Self::WilcoxonSignedRank { .. }
            | Self::KruskalWallis { .. }
            | Self::Friedman { .. }
            | Self::MoodsMedian { .. }
            | Self::Correlation { .. }
            | Self::PartialCorrelation { .. }
            | Self::CurveFit { .. }
            | Self::TwoWayAnova { .. }
            | Self::ChiSquareIndependence { .. }
            | Self::BoxCox { .. }
            | Self::ToleranceInterval { .. }
            | Self::RareEvents { .. }
            | Self::Levene { .. }
            | Self::Bartlett { .. }
            | Self::ChiSquareGoodnessOfFit { .. }
            | Self::Grubbs { .. }
            | Self::Power { .. }
            | Self::OneProportion { .. }
            | Self::TwoProportions { .. }
            | Self::OneVariance { .. }
            | Self::TwoVariances { .. }
            | Self::Regression { .. }
            | Self::StepwiseRegression { .. }
            | Self::Anova { .. }
            | Self::KaplanMeier { .. }
            | Self::CensoredWeibull { .. }
            | Self::CensoredExponential { .. }
            | Self::CensoredLognormal { .. }
            | Self::GageRr { .. }
            | Self::GageType1 { .. }
            | Self::GageLinearity { .. }
            | Self::Logistic { .. }
            | Self::Ordinal { .. }
            | Self::Poisson { .. }
            | Self::TimeSeries { .. }
            | Self::Pca { .. }
            | Self::KMeans { .. }
            | Self::HotellingT2 { .. }
            | Self::AcceptanceSampling { .. }
            | Self::Taguchi { .. }
            | Self::Pareto { .. }
            | Self::IntervalPlot { .. }
            | Self::MatrixPlot { .. }
            | Self::Dotplot { .. }
            | Self::StemLeaf { .. }
            | Self::SeriesPlot { .. }
            | Self::Ecdf { .. }
            | Self::SignTest { .. }
            | Self::AcceleratedLife { .. }
            | Self::StepStressAlt { .. }
            | Self::Agreement { .. }
            | Self::Factorial { .. }
            | Self::ResponseSurface { .. }
            | Self::Desirability { .. }
            | Self::Boxplot { .. }
            | Self::Scatterplot { .. }
            | Self::Histogram { .. } => {}
            Self::Capability { .. } => {}
        }
        self
    }

    /// Stable, human-readable description used in the Session notes.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::ProbabilityPlot {
                columns,
                distribution,
                plotting_position,
                p_value,
                confidence,
                display,
                layout,
                same_x,
                same_y,
                group,
            } => {
                let p = match p_value {
                    PValueMethod::Ad => "AD approximation",
                    PValueMethod::MonteCarlo { .. } => "Monte Carlo",
                };
                let ci = match confidence {
                    Some(level) => format!("{:.0}% CI", level * 100.0),
                    None => "CI off".to_owned(),
                };
                let grouped = group
                    .as_ref()
                    .map(|name| format!(", grouped by {name}"))
                    .unwrap_or_default();
                let layout_note = if columns.len() > 1 {
                    let same = match (*same_x, *same_y) {
                        (true, true) => ", same X/Y",
                        (true, false) => ", same X",
                        (false, true) => ", same Y",
                        (false, false) => "",
                    };
                    format!(", {}{same}", layout.name())
                } else {
                    String::new()
                };
                format!(
                    "Probability Plot — {}, {}, {p}, {ci}, {}{layout_note}{grouped}",
                    distribution.name(),
                    plotting_position.name(),
                    display.name()
                )
            }
            Self::NormalityTest {
                test,
                percentile_lines,
                ..
            } => format!(
                "Normality Test — {}, percentile lines {}",
                test.name(),
                if *percentile_lines { "on" } else { "off" }
            ),
            Self::DescriptiveStatistics { by, .. } => match by {
                Some(by) => format!(
                    "Descriptive Statistics — n−1 standard deviation, interpolated quartiles (h = (n+1)p), 95% mean and median intervals, by {by}"
                ),
                None => {
                    "Descriptive Statistics — n−1 standard deviation, interpolated quartiles (h = (n+1)p), 95% mean and median intervals"
                        .to_owned()
                }
            },
            Self::DistributionComparison { .. } => {
                "Distribution Comparison — MLE, AD/AIC/BIC".to_owned()
            }
            Self::ControlChart {
                kind,
                subgroup,
                tests,
                cusum,
                ewma,
                subgroup_column,
                phase_column,
                ..
            } => {
                if *kind == ControlChartKind::Cusum {
                    format!(
                        "Control Chart — CUSUM, k = {}σ, h = {}σ, subgroup {subgroup}",
                        cusum.reference, cusum.decision
                    )
                } else if *kind == ControlChartKind::Ewma {
                    format!(
                        "Control Chart — EWMA, λ = {}, L = {}σ, subgroup {subgroup}",
                        ewma.lambda, ewma.limit
                    )
                } else if let Some(phase) = phase_column {
                    format!(
                        "Control Chart — {}, tests {}, phase column {phase}",
                        kind.name(),
                        tests.summary()
                    )
                } else if let Some(group) = subgroup_column {
                    format!(
                        "Control Chart — {}, tests {}, subgroup column {group}",
                        kind.name(),
                        tests.summary()
                    )
                } else {
                    format!(
                        "Control Chart — {}, tests {}, subgroup {subgroup}",
                        kind.name(),
                        tests.summary()
                    )
                }
            }
            Self::RunChart { .. } => {
                "Run Chart — median center line, runs tests, Normal approximation".to_owned()
            }
            Self::AttributeChart {
                kind, sizes, size, ..
            } => {
                format!(
                    "{} Chart — {}; {}",
                    kind.name(),
                    kind.description(),
                    if let Some(sizes) = sizes {
                        format!("sizes column {sizes}")
                    } else if let Some(size) = size {
                        format!("constant size {size}")
                    } else if kind.needs_size() {
                        "size not configured".to_owned()
                    } else {
                        "no denominator".to_owned()
                    }
                )
            }
            Self::OneSampleT {
                null_mean,
                alternative,
                confidence,
                ..
            } => format!(
                "1-Sample t — H₀ µ = {null_mean}, {} alternative, {:.0}% interval",
                alternative.as_str(),
                confidence * 100.0
            ),
            Self::TwoSampleT {
                pooled,
                alternative,
                confidence,
                ..
            } => format!(
                "2-Sample t — {}, {} alternative, {:.0}% interval",
                if *pooled { "pooled" } else { "Welch" },
                alternative.as_str(),
                confidence * 100.0
            ),
            Self::PairedT {
                alternative,
                confidence,
                ..
            } => format!(
                "Paired t — {} alternative, {:.0}% interval",
                alternative.as_str(),
                confidence * 100.0
            ),
            Self::OneSampleEquivalence {
                column,
                null_mean,
                margin,
                confidence,
            } => format!(
                "Equivalence test (TOST) — {column} within ±{margin} of {null_mean}, {:.0}% \
                 confidence",
                confidence * 100.0
            ),
            Self::TwoSampleEquivalence {
                first,
                second,
                margin,
                confidence,
            } => format!(
                "Equivalence test (TOST) — {first} − {second} within ±{margin}, {:.0}% \
                 confidence",
                confidence * 100.0
            ),
            Self::PairedEquivalence {
                first,
                second,
                margin,
                confidence,
            } => format!(
                "Equivalence test (TOST) — paired {first} − {second} within ±{margin}, {:.0}% \
                 confidence",
                confidence * 100.0
            ),
            Self::MannWhitney {
                first,
                second,
                alternative,
            } => format!(
                "Mann-Whitney test — {first} vs {second}, {} alternative",
                alternative.as_str()
            ),
            Self::WilcoxonSignedRank {
                first,
                second,
                alternative,
            } => format!(
                "Wilcoxon signed-rank test — {first} vs {second}, {} alternative",
                alternative.as_str()
            ),
            Self::KruskalWallis { response, factor } => {
                format!("Kruskal-Wallis test — {response} by {factor}")
            }
            Self::Friedman { columns } => {
                format!("Friedman test — {}", columns.join(" vs "))
            }
            Self::MoodsMedian { response, factor } => {
                format!("Mood's median test — {response} by {factor}")
            }
            Self::Correlation { columns } => {
                format!("Correlation — {}", columns.join(", "))
            }
            Self::PartialCorrelation {
                x_column,
                y_column,
                control,
            } => format!(
                "Partial correlation — {x_column} and {y_column} given {control}"
            ),
            Self::TwoWayAnova {
                response,
                first,
                second,
                ..
            } => format!("Two-way ANOVA — {response} by {first} and {second}"),
            Self::ChiSquareIndependence { row, column } => {
                format!("Chi-square test for independence — {row} × {column}")
            }
            Self::BoxCox { column } => format!("Box-Cox transformation — {column}"),
            Self::ToleranceInterval {
                column,
                proportion,
                confidence,
                free,
            } => format!(
                "Tolerance interval ({}) — {column}, {:.1}% / {:.1}%",
                if *free { "distribution-free" } else { "normal" },
                proportion * 100.0,
                confidence * 100.0
            ),
            Self::RareEvents { counts, .. } => {
                format!("G chart (rare events) — {counts}")
            }
            Self::CurveFit {
                x_column,
                y_column,
                model,
                confidence,
            } => format!(
                "Curve fit — {y_column} vs {x_column}, {}, {:.0}% intervals",
                model.name(),
                confidence * 100.0
            ),
            Self::Levene {
                response,
                factor,
                center,
                ..
            } => format!(
                "Levene test — {response} by {factor}, {} centred",
                center.name()
            ),
            Self::Bartlett { response, factor } => {
                format!("Bartlett test — {response} by {factor}")
            }
            Self::ChiSquareGoodnessOfFit {
                column,
                expected,
                estimated_parameters,
            } => format!(
                "Chi-square goodness-of-fit — {column}, {} cells, {} estimated parameters",
                expected.len(),
                estimated_parameters
            ),
            Self::Grubbs { column, alpha } => {
                format!("Grubbs test — {column}, alpha {alpha}")
            }
            Self::Power {
                procedure,
                effect,
                n,
                groups,
                alpha,
                power,
                ..
            } => format!(
                "Power and sample size — {}, effect {effect}, alpha {alpha}, power {power}, n {n}, \
                 groups {groups}",
                procedure.name()
            ),
            Self::OneProportion {
                null_p,
                success,
                method,
                alternative,
                confidence,
                ..
            } => format!(
                "1 Proportion — p₀={null_p}, success={success}, {}, {} alternative, {:.0}% interval",
                method.as_str(),
                alternative.as_str(),
                confidence * 100.0
            ),
            Self::TwoProportions {
                success,
                alternative,
                confidence,
                ..
            } => format!(
                "2 Proportions — success={success}, {} alternative, {:.0}% interval",
                alternative.as_str(),
                confidence * 100.0
            ),
            Self::OneVariance {
                null_variance,
                alternative,
                confidence,
                ..
            } => format!(
                "1 Variance — σ₀²={null_variance}, {} alternative, {:.0}% interval",
                alternative.as_str(),
                confidence * 100.0
            ),
            Self::TwoVariances {
                alternative,
                confidence,
                method,
                ..
            } => format!(
                "2 Variances — {}, {} alternative, {:.0}% interval",
                method.name(),
                alternative.as_str(),
                confidence * 100.0
            ),
            Self::Regression {
                predictors,
                confidence,
                ..
            } => format!(
                "Regression — OLS, {} predictor(s), {:.0}% coefficient intervals",
                predictors.len(),
                confidence * 100.0
            ),
            Self::StepwiseRegression {
                method,
                criterion,
                alpha_enter,
                alpha_remove,
                max_steps,
                confidence,
                ..
            } => match criterion {
                crate::stepwise::StepCriterion::PValue => format!(
                    "Stepwise Regression — {}, enter α={}, remove α={}, at most {max_steps} steps, {:.0}% coefficient intervals",
                    method.name(),
                    alpha_enter,
                    alpha_remove,
                    confidence * 100.0
                ),
                crate::stepwise::StepCriterion::Aic | crate::stepwise::StepCriterion::Bic => {
                    format!(
                        "Stepwise Regression — {}, {} criterion, at most {max_steps} steps, {:.0}% coefficient intervals",
                        method.name(),
                        criterion.name(),
                        confidence * 100.0
                    )
                }
            },
            Self::Anova {
                factor, confidence, ..
            } => format!(
                "One-Way ANOVA — factor {factor}, {:.0}% intervals, Bonferroni comparisons",
                confidence * 100.0
            ),
            Self::KaplanMeier {
                censor,
                censor_value,
                confidence,
                ..
            } => format!(
                "Kaplan–Meier — {}, {:.0}% Greenwood intervals",
                censor.as_ref().map_or_else(
                    || "no censoring column".to_owned(),
                    |column| format!("censoring {column} = {censor_value}")
                ),
                confidence * 100.0
            ),
            Self::CensoredLognormal {
                censor,
                censor_value,
                ..
            } => format!(
                "Lognormal Reliability — {}, right-censored MLE",
                censor.as_ref().map_or_else(
                    || "no censoring column".to_owned(),
                    |column| format!("censoring {column} = {censor_value}")
                )
            ),
            Self::CensoredExponential {
                censor,
                censor_value,
                ..
            } => format!(
                "Exponential Reliability — {}, right-censored MLE",
                censor.as_ref().map_or_else(
                    || "no censoring column".to_owned(),
                    |column| format!("censoring {column} = {censor_value}")
                )
            ),
            Self::CensoredWeibull {
                censor,
                censor_value,
                ..
            } => format!(
                "Weibull Reliability — {}, right-censored MLE",
                censor.as_ref().map_or_else(
                    || "no censoring column".to_owned(),
                    |column| format!("censoring {column} = {censor_value}")
                )
            ),
            Self::GageType1 {
                reference,
                lsl,
                usl,
                ..
            } => format!(
                "Gage Type 1 — reference {reference}, limits {} / {}",
                lsl.map_or("—".to_owned(), |value| format!("{value}")),
                usl.map_or("—".to_owned(), |value| format!("{value}"))
            ),
            Self::Agreement {
                part,
                appraiser,
                rating,
            } => format!("Attribute Agreement — {part} × {appraiser} × {rating}"),
            Self::KMeans {
                columns,
                k,
                standardise,
                seed,
            } => format!(
                "K-Means — {} variables, {k} clusters, {} (seed {seed})",
                columns.len(),
                if *standardise {
                    "standardised"
                } else {
                    "original scale"
                }
            ),
            Self::AcceleratedLife {
                time,
                stress,
                use_stress,
                life,
                acceleration,
                mode,
                ..
            } => match mode {
                Some(mode) => format!(
                    "Accelerated Life — competing risks on {mode}, {} / {}, use stress {use_stress}",
                    acceleration.name(),
                    life.name()
                ),
                None => format!(
                    "Accelerated Life — {time} against {stress}, {} / {}, use stress {use_stress}",
                    acceleration.name(),
                    life.name()
                ),
            },
            Self::StepStressAlt {
                duration,
                stress,
                use_stress,
                life,
                acceleration,
                ..
            } => format!(
                "Step-Stress ALT — {duration} against {stress}, {} / {}, use stress {use_stress}",
                acceleration.name(),
                life.name()
            ),
            Self::SignTest { column, median } => format!("Sign Test — {column}, median {median}"),
            Self::SeriesPlot { column, time } => match time {
                Some(time) => format!("Time Series Plot — {column} against {time}"),
                None => format!("Time Series Plot — {column}"),
            },
            Self::StemLeaf { column } => format!("Stem-and-Leaf — {column}"),
            Self::Ecdf { column, confidence } => format!(
                "Empirical CDF — {column}, {:.0}% band",
                confidence * 100.0
            ),
            Self::Dotplot { column, factor } => match factor {
                Some(factor) => format!("Dotplot — {column} by {factor}"),
                None => format!("Dotplot — {column}"),
            },
            Self::MatrixPlot { columns, bins } => format!(
                "Matrix Plot — {} variables, {bins} bins",
                columns.len()
            ),
            Self::IntervalPlot {
                response,
                factor,
                confidence,
            } => format!(
                "Interval Plot — {response} by {factor}, {:.0}% intervals",
                confidence * 100.0
            ),
            Self::Pareto { column, weight } => match weight {
                Some(weight) => format!("Pareto Chart — {column}, weighted by {weight}"),
                None => format!("Pareto Chart — {column}"),
            },
            Self::Taguchi {
                response,
                factors,
                quality,
            } => format!(
                "Taguchi — {response} on {} factor(s), {}",
                factors.len(),
                quality.name()
            ),
            Self::AcceptanceSampling { plan, targets, .. } => match (plan, targets) {
                (Some(plan), _) => format!(
                    "Acceptance Sampling — n = {}, c = {}",
                    plan.sample, plan.acceptance
                ),
                (_, Some((aql, ltpd, alpha, beta))) => format!(
                    "Acceptance Sampling — plan for AQL {:.2}%, LTPD {:.2}%, risks {:.0}%/{:.0}%",
                    aql * 100.0,
                    ltpd * 100.0,
                    alpha * 100.0,
                    beta * 100.0
                ),
                _ => "Acceptance Sampling".to_owned(),
            },
            Self::HotellingT2 {
                columns,
                subgroup,
                confidence,
            } => format!(
                "Hotelling T² — {} variables, subgroups of {subgroup}, {:.0}% limits",
                columns.len(),
                confidence * 100.0
            ),
            Self::Pca { columns, keep } => format!(
                "Principal Component Analysis — {} variables, {keep} score component(s)",
                columns.len()
            ),
            Self::TimeSeries { column, method } => match method {
                crate::timeseries::TimeSeriesMethod::MovingAverage { window } => {
                    format!("Moving average — {column}, window {window}")
                }
                crate::timeseries::TimeSeriesMethod::Decomposition { period, model } => {
                    format!(
                        "Decomposition — {column}, period {period}, {}",
                        model.name()
                    )
                }
                crate::timeseries::TimeSeriesMethod::ExponentialSmoothing { season, horizon } => {
                    format!(
                        "Exponential smoothing — {column}, season {season}, {horizon} forecast(s)"
                    )
                }
            },
            Self::Ordinal {
                response,
                predictors,
            } => format!(
                "Ordinal Logistic Regression — {response} on {}",
                predictors.join(", ")
            ),
            Self::Poisson {
                response,
                predictors,
            } => format!(
                "Poisson Regression — {response} on {}",
                predictors.join(", ")
            ),
            Self::Logistic {
                response,
                predictors,
            } => format!(
                "Binary Logistic Regression — {response} on {}",
                predictors.join(", ")
            ),
            Self::GageLinearity {
                measurement,
                reference,
            } => format!("Gage Linearity — {measurement} against {reference}"),
            Self::GageRr { part, operator, .. } => {
                format!("Gage R&R — crossed ANOVA, part {part}, operator {operator}")
            }
            Self::Factorial { factors, block, .. } => format!(
                "2-Level Factorial — {} factor(s), all interactions{}",
                factors.len(),
                match block {
                    Some(block) => format!(", block column {block}"),
                    None => String::new(),
                }
            ),
            Self::Desirability { factors, responses } => format!(
                "Response Optimizer — {} response(s) on {} factor(s)",
                responses.len(),
                factors.len()
            ),
            Self::ResponseSurface {
                factors,
                steepest_ascent,
                ..
            } => format!(
                "Response Surface — {} factor(s), quadratic model{}",
                factors.len(),
                if *steepest_ascent {
                    ", steepest ascent"
                } else {
                    ""
                }
            ),
            Self::Boxplot { by, .. } => match by {
                Some(by) => format!("Boxplot — grouped by {by}, interpolated quartiles, 1.5×IQR"),
                None => "Boxplot — interpolated quartiles, 1.5×IQR".to_owned(),
            },
            Self::Scatterplot { fit, .. } => {
                format!(
                    "Scatterplot — Pearson r; fit line {}",
                    if *fit { "on" } else { "off" }
                )
            }
            Self::Histogram { bins, .. } => match bins {
                Some(bins) => format!("Histogram — {bins} bins"),
                None => "Histogram — automatic bins".to_owned(),
            },
            Self::Capability {
                distribution,
                confidence,
                subgroup_column,
                sixpack,
                ..
            } => {
                let model = if *distribution == DistributionKind::Normal {
                    format!("within/overall, {:.0}% CI", confidence * 100.0)
                } else {
                    "MLE fit, expected PPM, AD gate, bootstrap CI".to_owned()
                };
                let heading = if *sixpack {
                    "Capability Sixpack"
                } else {
                    "Capability Analysis"
                };
                match subgroup_column {
                    Some(column) => format!(
                        "{heading} — {}, {model}, subgroup column {column}",
                        distribution.name()
                    ),
                    None => format!("{heading} — {}, {model}", distribution.name()),
                }
            }
        }
    }
}

/// Options of a Normal probability plot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProbabilityPlotOptions {
    /// Plotting-position convention.
    pub method: PlottingPosition,
    /// p-value method.
    pub p_value: PValueMethod,
    /// Confidence level for the band (`None` = no band).
    pub confidence: Option<f64>,
    /// Display choice.
    pub display: PlotDisplay,
}

impl Default for ProbabilityPlotOptions {
    fn default() -> Self {
        Self {
            method: PlottingPosition::Benard,
            p_value: PValueMethod::Ad,
            confidence: Some(0.95),
            display: PlotDisplay::Both,
        }
    }
}

/// Options for a normal capability analysis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapabilityOptions {
    /// Target for `Cpm`; `None` leaves the index uncomputed.
    pub target: Option<f64>,
    /// Bootstrap replications for non-normal PPM (`0` = automatic).
    pub bootstrap_reps: usize,
    /// Estimate and apply a Box-Cox transformation first.
    pub box_cox: bool,
    /// Subgroup size (1 = individual observations / moving range).
    pub subgroup: usize,
    /// Apply `c4` to the within estimate.
    pub unbiasing_within: bool,
    /// Apply `c4` to the overall estimate.
    pub unbiasing_overall: bool,
    /// Two-sided confidence level for the capability intervals.
    pub confidence: f64,
    /// Refuse output when the I–MR stability screen signals.
    pub require_stability: bool,
}

impl Default for CapabilityOptions {
    fn default() -> Self {
        Self {
            target: None,
            bootstrap_reps: 0,
            box_cox: false,
            subgroup: 1,
            unbiasing_within: true,
            unbiasing_overall: false,
            confidence: 0.95,
            require_stability: false,
        }
    }
}

/// Result of a probability-plot analysis: the Session document and the plot.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbabilityPlotAnalysis {
    /// Report for the Session window.
    pub result: AnalysisResult,
    /// Backend-independent plot, ready for SVG or the screen.
    pub plot: PlotSpec,
}

/// Result of any analysis: one report per series and, when the analysis draws
/// one, a plot (an overlay when there are several Y columns).
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisOutput {
    /// Reports for the Session window.
    pub results: Vec<AnalysisResult>,
    /// Backend-independent plot, when the analysis produces one.
    pub plot: Option<PlotSpec>,
}

/// First numeric column (floating point or integer), if any.
#[must_use]
pub fn first_numeric_column(df: &DataFrame) -> Option<&str> {
    df.columns()
        .iter()
        .find(|c| matches!(c.dtype(), DType::Float | DType::Integer))
        .map(|c| c.name())
}

/// Fits a Normal, computes the probability plot, the AD test and the report.
pub fn normal_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    validate_confidence(options.confidence)?;

    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Normal::fit_moments(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = normal_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Normal — column {column_name}");
    let plot = apply_display(
        PlotSpec::normal_probability_with_ci(
            &probability,
            &dist,
            title.clone(),
            options.confidence,
        )
        .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts(&sample, a2, options.p_value)?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "mean and sample standard deviation (n−1), as used by the probability-plot path"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
        ("Confidence band".to_owned(), ci_note(options.confidence)),
    ];
    Ok(assemble(
        title,
        "Sample moments (n−1)",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

/// Fits a log-normal on `ln x`, computes the plot, the AD test and the report.
pub fn lognormal_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    validate_confidence(options.confidence)?;

    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Lognormal::fit_moments(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = lognormal_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Lognormal — column {column_name}");
    let plot = apply_display(
        PlotSpec::lognormal_probability_with_ci(
            &probability,
            &dist,
            title.clone(),
            options.confidence,
        )
        .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts(&sample, a2, options.p_value)?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "mean and sample standard deviation (n−1) on ln(x), as used by the probability-plot path"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted; x > 0 required".to_owned(),
        ),
        ("Confidence band".to_owned(), ci_note(options.confidence)),
    ];
    Ok(assemble(
        title,
        "Sample moments (n−1) on ln(x)",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

/// Fits a Weibull by profile likelihood, computes the plot, AD test and report.
pub fn weibull_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Weibull::fit_mle(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = weibull_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Weibull — column {column_name}");
    let plot = apply_display(
        PlotSpec::weibull_probability_with_ci(
            &probability,
            &dist,
            title.clone(),
            options.confidence,
        )
        .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts_weibull(&sample, a2, options.p_value)?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "profile likelihood on centered ln(x), as used by the probability-plot path".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted; x > 0 required".to_owned(),
        ),
        (
            "Confidence band".to_owned(),
            "plot band from the covariance of the MLE (delta method, normal quantile); the documented covariance construction is not recovered".to_owned(),
        ),
    ];
    Ok(assemble(
        title,
        "Maximum likelihood (profile)",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

/// Fits an exponential (`θ = mean`), computes the plot, AD test and report.
pub fn exponential_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Exponential::fit_mle(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = exponential_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Exponential — column {column_name}");
    let plot = apply_display(
        PlotSpec::exponential_probability(&probability, &dist, title.clone())
            .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts_exponential(&sample, a2, options.p_value)?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "θ = mean(x), the exponential MLE (probability-plot path)".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted; x > 0 required".to_owned(),
        ),
        (
            "Confidence band".to_owned(),
            "plot band from the covariance of the MLE (delta method, normal quantile); the documented covariance construction is not recovered".to_owned(),
        ),
    ];
    Ok(assemble(
        title,
        "Maximum likelihood (mean)",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

/// Fits a logistic by maximum likelihood, computes the plot and AD test.
pub fn logistic_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    validate_confidence(options.confidence)?;
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Logistic::fit_mle(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = logistic_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Logistic — column {column_name}");
    let plot = apply_display(
        PlotSpec::logistic_probability_with_ci(
            &probability,
            &dist,
            title.clone(),
            options.confidence,
        )
        .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts_provisional(&sample, a2, options.p_value, "logistic")?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "maximum likelihood by coordinate ascent on the two score equations".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
        (
            "Confidence band".to_owned(),
            "plot band from the covariance of the MLE (delta method, normal quantile); the documented covariance construction is not recovered".to_owned(),
        ),
    ];
    Ok(assemble(
        title,
        "Maximum likelihood",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

/// Fits a log-logistic by maximum likelihood on `ln x`.
pub fn loglogistic_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    validate_confidence(options.confidence)?;
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Loglogistic::fit_mle(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = loglogistic_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Loglogistic — column {column_name}");
    let plot = apply_display(
        PlotSpec::loglogistic_probability_with_ci(
            &probability,
            &dist,
            title.clone(),
            options.confidence,
        )
        .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts_provisional(&sample, a2, options.p_value, "log-logistic")?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "maximum likelihood on ln(x), equivalent to a logistic fit of the logs".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted; x > 0 required".to_owned(),
        ),
        (
            "Confidence band".to_owned(),
            "plot band from the covariance of the MLE (delta method, normal quantile); the documented covariance construction is not recovered".to_owned(),
        ),
    ];
    Ok(assemble(
        title,
        "Maximum likelihood on ln(x)",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

/// Fits a gamma distribution by maximum likelihood.
pub fn gamma_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    validate_confidence(options.confidence)?;
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Gamma::fit_mle(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = gamma_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Gamma — column {column_name}");
    let plot = apply_display(
        PlotSpec::gamma_probability_with_ci(&probability, &dist, title.clone(), options.confidence)
            .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts_provisional(&sample, a2, options.p_value, "gamma")?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "maximum likelihood: Newton on ln α − ψ(α) = ln(x̄) − mean(ln x), then β = x̄/α"
                .to_owned(),
        ),
        (
            "Probability axis".to_owned(),
            "percent axis (Normal scale); the fitted curve is a polyline because the gamma CDF \
             has no linearizing transformation"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted; x > 0 required".to_owned(),
        ),
        (
            "Confidence band".to_owned(),
            "off (covariance construction not available for this family)".to_owned(),
        ),
    ];
    Ok(assemble(
        title,
        "Maximum likelihood",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

/// Fits a three-parameter Weibull distribution.
pub fn weibull3_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    validate_confidence(options.confidence)?;
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Weibull3::fit_mle(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = weibull3_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Weibull (3-parameter) — column {column_name}");
    let plot = apply_display(
        PlotSpec::weibull3_probability(&probability, &dist, title.clone())
            .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts_provisional(&sample, a2, options.p_value, "three-parameter Weibull")?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "the threshold is the value that makes the Weibull probability plot straightest \
             (probability-plot method); the shape and scale follow by maximum likelihood with \
             that threshold fixed"
                .to_owned(),
        ),
        (
            "Caveat".to_owned(),
            "the likelihood of a three-parameter model is not bounded, so the threshold is not a \
             plain maximum-likelihood estimate and depends on the searched range"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted; x > 0 required".to_owned(),
        ),
    ];
    Ok(assemble(
        title,
        "Probability-plot threshold, maximum likelihood shape and scale",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

/// Fits a three-parameter log-normal distribution.
pub fn lognormal3_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    validate_confidence(options.confidence)?;
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Lognormal3::fit_mle(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = lognormal3_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Lognormal (3-parameter) — column {column_name}");
    let plot = apply_display(
        PlotSpec::lognormal3_probability(&probability, &dist, title.clone())
            .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts_provisional(&sample, a2, options.p_value, "three-parameter log-normal")?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "the threshold is the value that makes the normal probability plot of ln(x − γ) \
             straightest (probability-plot method); the location and scale follow by maximum \
             likelihood with that threshold fixed"
                .to_owned(),
        ),
        (
            "Caveat".to_owned(),
            "the likelihood of a three-parameter model is not bounded, so the threshold is not a \
             plain maximum-likelihood estimate and depends on the searched range"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted; x > 0 required".to_owned(),
        ),
    ];
    Ok(assemble(
        title,
        "Probability-plot threshold, maximum likelihood location and scale",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

/// Fits an extreme-value (type I Gumbel) distribution by maximum likelihood.
pub fn extreme_value_probability(
    df: &DataFrame,
    column_name: &str,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    validate_confidence(options.confidence)?;
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = ExtremeValue::fit_mle(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;

    let probability = extreme_value_probability_plot(&sample, &dist, options.method)?;
    let title = format!("Probability Plot: Extreme Value — column {column_name}");
    let plot = apply_display(
        PlotSpec::extreme_value_probability_with_ci(
            &probability,
            &dist,
            title.clone(),
            options.confidence,
        )
        .with_x_label(column_name),
        options.display,
    );

    let a2 = anderson_darling(&dist, &sample)?;
    let p = p_value_parts_provisional(&sample, a2, options.p_value, "extreme-value")?;
    let notes = vec![
        (
            "Plotting positions".to_owned(),
            format!("{} — {}", options.method.name(), options.method.formula()),
        ),
        (
            "Estimator".to_owned(),
            "maximum likelihood: the score equations mean(exp(−(x − µ)/σ)) = 1 and \
             σ = x̄ − mean(x·exp(−(x − µ)/σ)) are solved alternately"
                .to_owned(),
        ),
        (
            "Probability axis".to_owned(),
            "percent axis (extreme-value scale y = −ln(−ln p)); the fit is a straight line"
                .to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "non-finite values excluded and counted".to_owned(),
        ),
        (
            "Confidence band".to_owned(),
            "off (covariance construction not available for this family)".to_owned(),
        ),
    ];
    Ok(assemble(
        title,
        "Maximum likelihood",
        dist.parameters(),
        &desc,
        &sample,
        a2,
        p,
        notes,
        plot,
    ))
}

fn apply_display(mut plot: PlotSpec, display: PlotDisplay) -> PlotSpec {
    match display {
        PlotDisplay::Both => {}
        PlotDisplay::Symbols => {
            plot.lines.clear();
            plot.bands.clear();
        }
        PlotDisplay::Fit => {
            plot.points.clear();
        }
    }
    plot
}

fn validate_confidence(confidence: Option<f64>) -> Result<()> {
    if let Some(level) = confidence
        && !(level > 0.0 && level < 1.0)
    {
        return Err(Error::InvalidParameter {
            name: "confidence",
            value: level,
        });
    }
    Ok(())
}

fn ci_note(confidence: Option<f64>) -> String {
    match confidence {
        Some(level) => format!("{:.0}% (s and c4; normal quantile)", level * 100.0),
        None => "off".to_owned(),
    }
}

type PValueParts = (&'static str, String, &'static str);

fn p_value_parts(
    sample: &crate::data::Sample,
    a2: f64,
    method: PValueMethod,
) -> Result<PValueParts> {
    Ok(match method {
        PValueMethod::Ad => {
            let p = ad_p_value(sample.n(), a2)?;
            (
                "p-value (Anderson–Darling)",
                format!("{p:.6}"),
                "D'Agostino–Stephens composite-AD approximation",
            )
        }
        PValueMethod::MonteCarlo { reps, seed } => {
            let p = ad_p_value_mc(sample.n(), a2, reps, seed)?;
            (
                "p-value (Monte Carlo)",
                format!("{p:.6}  [{reps} replications, seed {seed}]"),
                "Monte Carlo cross-check (parameters re-estimated per replicate)",
            )
        }
    })
}

fn p_value_parts_weibull(
    sample: &crate::data::Sample,
    a2: f64,
    method: PValueMethod,
) -> Result<PValueParts> {
    match method {
        PValueMethod::Ad => {
            let p = ad_p_value_weibull(sample.n(), a2)?;
            Ok((
                "p-value (Weibull AD table)",
                format!("{p:.6}"),
                "Weibull small-sample table, 1 + 0.2/√n",
            ))
        }
        PValueMethod::MonteCarlo { .. } => Err(Error::Unsupported(
            "Monte Carlo p-value is implemented for the Normal distribution only".to_owned(),
        )),
    }
}

fn p_value_parts_exponential(
    sample: &crate::data::Sample,
    a2: f64,
    method: PValueMethod,
) -> Result<PValueParts> {
    match method {
        PValueMethod::Ad => {
            let p = ad_p_value_exponential(sample.n(), a2)?;
            Ok((
                "p-value (exponential AD formula)",
                format!("{p:.6}"),
                "exponential four-branch formula, 1 + 0.6/n (the documented behaviour)",
            ))
        }
        PValueMethod::MonteCarlo { .. } => Err(Error::Unsupported(
            "Monte Carlo p-value is implemented for the Normal distribution only".to_owned(),
        )),
    }
}

fn p_value_parts_provisional(
    sample: &crate::data::Sample,
    a2: f64,
    method: PValueMethod,
    family: &'static str,
) -> Result<PValueParts> {
    match method {
        PValueMethod::Ad => {
            let p = ad_p_value(sample.n(), a2)?;
            Ok((
                "p-value (AD approximation)",
                format!("{p:.6}"),
                "Normal composite-AD approximation; a family-specific table is not available",
            ))
        }
        PValueMethod::MonteCarlo { .. } => Err(Error::Unsupported(format!(
            "Monte Carlo p-values are available for the Normal distribution only; \
             use the AD approximation for {family}"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_result(
    title: String,
    method: &str,
    parameters: Vec<(&'static str, f64)>,
    desc: &Descriptives,
    sample: &crate::data::Sample,
    a2: f64,
    p: PValueParts,
    mut notes: Vec<(String, String)>,
) -> AnalysisResult {
    let mut result = AnalysisResult::new(title, method);
    result.n = sample.n();
    result.excluded = sample.excluded();
    result.parameters = parameters;
    result.statistics = vec![
        ("Median", desc.median),
        ("Minimum", desc.min),
        ("Maximum", desc.max),
    ];
    result.gof = vec![("Anderson–Darling (A²)", format!("{a2:.6}")), (p.0, p.1)];
    notes.push(("p-value method".to_owned(), p.2.to_owned()));
    result.notes = notes;
    result.warnings = standard_warnings(sample.n(), sample.excluded(), sample.ties());
    result
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    title: String,
    method: &str,
    parameters: Vec<(&'static str, f64)>,
    desc: &Descriptives,
    sample: &crate::data::Sample,
    a2: f64,
    p: PValueParts,
    notes: Vec<(String, String)>,
    plot: PlotSpec,
) -> ProbabilityPlotAnalysis {
    ProbabilityPlotAnalysis {
        result: build_result(title, method, parameters, desc, sample, a2, p, notes),
        plot,
    }
}

fn probability_one(
    df: &DataFrame,
    column: &str,
    distribution: DistributionKind,
    options: ProbabilityPlotOptions,
) -> Result<ProbabilityPlotAnalysis> {
    let mut out = match distribution {
        DistributionKind::Normal => normal_probability(df, column, options),
        DistributionKind::Lognormal => lognormal_probability(df, column, options),
        DistributionKind::Weibull => weibull_probability(df, column, options),
        DistributionKind::Exponential => exponential_probability(df, column, options),
        DistributionKind::Logistic => logistic_probability(df, column, options),
        DistributionKind::Loglogistic => loglogistic_probability(df, column, options),
        DistributionKind::Gamma => gamma_probability(df, column, options),
        DistributionKind::ExtremeValue => extreme_value_probability(df, column, options),
        DistributionKind::Weibull3 => weibull3_probability(df, column, options),
        DistributionKind::Lognormal3 => lognormal3_probability(df, column, options),
    }?;
    out.plot.notes.push(probability_note(&out.result));
    Ok(out)
}

/// Short name of a fitted parameter for the block drawn inside a plot.
fn short_parameter(label: &str) -> &str {
    if label.starts_with("Mean") {
        "Mean"
    } else if label.starts_with("Standard deviation") {
        "StDev"
    } else {
        label.split(" (").next().unwrap_or(label)
    }
}

/// N, the fitted parameters and the goodness of fit, as drawn inside a
/// probability plot (the same values the Session prints).
fn probability_note(result: &AnalysisResult) -> PlotNote {
    let mut rows = vec![("N", result.n.to_string())];
    for (label, value) in &result.parameters {
        rows.push((short_parameter(label), format!("{value:.4}")));
    }
    for (label, value) in &result.gof {
        let short = if label.contains("p-value") {
            "P"
        } else if label.contains("Anderson") || label.contains("A²") {
            "AD"
        } else {
            label.split(" (").next().unwrap_or(label)
        };
        let shown = value
            .parse::<f64>()
            .map_or_else(|_| value.clone(), |number| format!("{number:.4}"));
        rows.push((short, shown));
    }
    PlotNote::new(rows)
}

/// One line per series for an overlay: its name next to N, mean and spread, so
/// no value sits in an unlabelled column.
fn overlay_results_note(labels: &[String], results: &[AnalysisResult]) -> PlotNote {
    let mut rows: Vec<(String, String)> = Vec::new();
    for (label, result) in labels.iter().zip(results) {
        rows.push((label.clone(), format!("n {}", result.n)));
        for (name, value) in &result.parameters {
            rows.push((
                format!("{label} {}", short_parameter(name)),
                format!("{value:.4}"),
            ));
        }
    }
    PlotNote::new(rows)
}

/// Mean, spread and the capability indices, as drawn inside a capability
/// histogram. The within/overall table of the report is the source.
fn capability_note(result: &AnalysisResult) -> PlotNote {
    let mut rows = vec![("N", result.n.to_string())];
    for (label, value) in &result.parameters {
        if label.starts_with("Mean") {
            rows.push(("Mean", format!("{value:.4}")));
        }
    }
    let table = result.extra.iter().find_map(|item| match item {
        Item::Table { headers, rows } if headers.first().map(String::as_str) == Some("Metric") => {
            Some(rows)
        }
        _ => None,
    });
    if let Some(rows_table) = table {
        let cell = |name: &str, column: usize| {
            rows_table
                .iter()
                .find(|row| row.first().map(String::as_str) == Some(name))
                .and_then(|row| row.get(column))
                .filter(|value| value.as_str() != "—")
                .cloned()
        };
        let pair = |first: Option<String>, second: Option<String>| match (first, second) {
            (Some(a), Some(b)) => format!("{a} / {b}"),
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => "—".to_owned(),
        };
        rows.push(("StDev", pair(cell("StDev", 1), cell("StDev", 2))));
        rows.push(("Cp", cell("Cp", 1).unwrap_or_else(|| "—".into())));
        rows.push(("Cpk", cell("Cpk", 1).unwrap_or_else(|| "—".into())));
        rows.push(("Pp", cell("Pp", 2).unwrap_or_else(|| "—".into())));
        rows.push(("Ppk", cell("Ppk", 2).unwrap_or_else(|| "—".into())));
        if let Some(ppm) = cell("PPM (observed)", 1) {
            rows.push(("PPM", ppm));
        }
    }
    PlotNote::new(rows)
}

fn numeric_at(column: &Column, row: usize) -> Option<f64> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::F64(values) => values.get(row).copied(),
        ColumnData::I64(values) => values.get(row).map(|&value| value as f64),
        _ => None,
    }
}

fn group_at(column: &Column, row: usize) -> Option<String> {
    if column.is_missing(row) {
        return None;
    }
    match column.data() {
        ColumnData::Text(values) => values.get(row).cloned(),
        ColumnData::Date(values) => Some(crate::data::format_date(values[row])),
        ColumnData::I64(values) => values.get(row).map(ToString::to_string),
        ColumnData::F64(values) => values.get(row).map(ToString::to_string),
        ColumnData::Bool(values) => values
            .get(row)
            .map(|&value| if value { "T" } else { "F" }.to_owned()),
    }
}

#[allow(clippy::too_many_arguments)]
fn grouped_probability(
    df: &DataFrame,
    column: &str,
    group: &str,
    distribution: DistributionKind,
    options: ProbabilityPlotOptions,
    description: &str,
    layout: PlotLayout,
    same_x: bool,
    same_y: bool,
) -> Result<AnalysisOutput> {
    let y = df.column(column)?;
    let group_column = df.column(group)?;
    let mut levels: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for row in 0..df.n_rows() {
        let Some(value) = numeric_at(y, row) else {
            continue;
        };
        let Some(level) = group_at(group_column, row) else {
            continue;
        };
        levels.entry(level).or_default().push(value);
    }
    if levels.len() > 100 {
        return Err(Error::Unsupported(
            "grouping is limited to 100 distinct levels".to_owned(),
        ));
    }
    if levels.is_empty() {
        return Err(Error::EmptySample);
    }

    let mut results = Vec::with_capacity(levels.len());
    let mut plots = Vec::with_capacity(levels.len());
    for (level, values) in levels {
        let label = format!("{column} ({level})");
        let sub = DataFrame::new(vec![Column::new_f64(label.clone(), values)])?;
        let mut out = probability_one(&sub, &label, distribution, options)?;
        out.result
            .notes
            .push(("Analysis".to_owned(), description.to_owned()));
        results.push(out.result);
        plots.push((label, out.plot));
    }

    let plot = if plots.len() == 1 {
        plots.pop().map(|(_, plot)| plot)
    } else {
        let labels: Vec<String> = plots.iter().map(|(label, _)| label.clone()).collect();
        let mut plot = match layout {
            PlotLayout::Overlay => overlay_plots(
                plots,
                format!(
                    "Probability Plot: {} — grouped by {group}",
                    distribution.name()
                ),
            ),
            PlotLayout::Separate => panel_plot_with_scales(
                plots.into_iter().map(|(_, plot)| plot).collect(),
                2,
                format!(
                    "Probability Plot: {} — grouped by {group}, separate panels",
                    distribution.name()
                ),
                same_x,
                same_y,
            ),
        };
        if layout == PlotLayout::Overlay
            && let Some(plot) = &mut plot
        {
            plot.notes.push(overlay_results_note(&labels, &results));
        }
        plot
    };
    Ok(AnalysisOutput { results, plot })
}

/// Two-sided chi-square interval for `Cp` or `Pp`.
///
/// `df` is the degrees of freedom of the standard-deviation estimate. The
/// moving-range within estimate uses `n − 1` as an approximation; a pooled
/// subgroup estimate uses `n − groups`.
fn cp_confidence_interval(value: f64, df: f64, confidence: f64) -> Option<(f64, f64)> {
    if !value.is_finite() || df <= 0.0 || !(0.0..1.0).contains(&confidence) {
        return None;
    }
    let alpha = 1.0 - confidence;
    let low = chi_square_quantile(alpha / 2.0, df);
    let high = chi_square_quantile(1.0 - alpha / 2.0, df);
    Some((value * (low / df).sqrt(), value * (high / df).sqrt()))
}

/// Normal-approximation interval for `Cpk`/`Ppk`.
///
/// This is the common NIST approximation; it is wide for small `n` and is not
/// a substitute for a bootstrap or an exact conditional interval.
fn cpk_confidence_interval(value: f64, n: usize, confidence: f64) -> Option<(f64, f64)> {
    if !value.is_finite() || n < 2 || !(0.0..1.0).contains(&confidence) {
        return None;
    }
    let z = normal_quantile(1.0 - (1.0 - confidence) / 2.0);
    let n = n as f64;
    let se = (1.0 / (9.0 * n) + value * value / (2.0 * (n - 1.0))).sqrt();
    Some((value - z * se, value + z * se))
}

fn wilson_interval(successes: usize, n: usize, confidence: f64) -> Option<(f64, f64)> {
    if n == 0 || successes > n || !(0.0..1.0).contains(&confidence) {
        return None;
    }
    let z = normal_quantile(1.0 - (1.0 - confidence) / 2.0);
    let p = successes as f64 / n as f64;
    let n = n as f64;
    let denominator = 1.0 + z * z / n;
    let center = (p + z * z / (2.0 * n)) / denominator;
    let half = z * (p * (1.0 - p) / n + z * z / (4.0 * n * n)).sqrt() / denominator;
    Some(((center - half).max(0.0), (center + half).min(1.0)))
}

/// Expected PPM below and above the limits, with confidence intervals.
#[derive(Debug, Clone, Copy)]
struct ExpectedPpm {
    low: f64,
    high: f64,
    total: f64,
    low_ci: (f64, f64),
    high_ci: (f64, f64),
    total_ci: (f64, f64),
}

/// Normal-approximation interval for the expected PPM.
///
/// Each tail uses the z scale: `z = (limit - mean)/sigma`, with
/// `SE(z) = sqrt(1/n + z²/(2·df))`. The total uses the logit of the summed
/// tail probability and the delta-method variance of that sum.
fn expected_ppm_interval(
    mean: f64,
    sigma: f64,
    df: f64,
    n: usize,
    lsl: Option<f64>,
    usl: Option<f64>,
    confidence: f64,
) -> Option<ExpectedPpm> {
    if !mean.is_finite()
        || !sigma.is_finite()
        || sigma <= 0.0
        || !df.is_finite()
        || df <= 0.0
        || n < 2
        || !(0.0..1.0).contains(&confidence)
    {
        return None;
    }
    let z_critical = normal_quantile(1.0 - (1.0 - confidence) / 2.0);
    let density = |z: f64| (-0.5 * z * z).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let (mut low, mut low_lo, mut low_hi) = (0.0, 0.0, 0.0);
    let (mut high, mut high_lo, mut high_hi) = (0.0, 0.0, 0.0);
    let (mut d_mean, mut d_sigma) = (0.0, 0.0);
    if let Some(limit) = lsl {
        let z = (limit - mean) / sigma;
        let se = (1.0 / n as f64 + z * z / (2.0 * df)).sqrt();
        low = normal_cdf(z);
        low_lo = normal_cdf(z - z_critical * se);
        low_hi = normal_cdf(z + z_critical * se);
        d_mean += -density(z) / sigma;
        d_sigma += -density(z) * z / sigma;
    }
    if let Some(limit) = usl {
        let z = (limit - mean) / sigma;
        let se = (1.0 / n as f64 + z * z / (2.0 * df)).sqrt();
        high = 1.0 - normal_cdf(z);
        high_lo = 1.0 - normal_cdf(z + z_critical * se);
        high_hi = 1.0 - normal_cdf(z - z_critical * se);
        d_mean += density(z) / sigma;
        d_sigma += density(z) * z / sigma;
    }
    let total = low + high;
    let total_ci = if lsl.is_some() && usl.is_some() && total > 0.0 && total < 1.0 {
        let variance = d_mean * d_mean * sigma * sigma / n as f64
            + d_sigma * d_sigma * sigma * sigma / (2.0 * df);
        let logit = (total / (1.0 - total)).ln();
        let se = variance.sqrt() / (total * (1.0 - total));
        let lo = 1.0 / (1.0 + (-(logit - z_critical * se)).exp());
        let hi = 1.0 / (1.0 + (-(logit + z_critical * se)).exp());
        (lo, hi)
    } else {
        // One-sided total, or a probability at the boundary.
        (low_lo + high_lo, low_hi + high_hi)
    };
    Some(ExpectedPpm {
        low,
        high,
        total,
        low_ci: (low_lo, low_hi),
        high_ci: (high_lo, high_hi),
        total_ci,
    })
}

fn expected_ppm_table(overall: ExpectedPpm, within: ExpectedPpm, confidence: f64) -> Item {
    let ppm = |value: f64| format!("{:.1}", value * 1_000_000.0);
    let ci =
        |(low, high): (f64, f64)| format!("{:.1} – {:.1}", low * 1_000_000.0, high * 1_000_000.0);
    Item::Table {
        headers: vec![
            "PPM".to_owned(),
            "Expected overall".to_owned(),
            format!("{:.0}% CI", confidence * 100.0),
            "Expected within".to_owned(),
            format!("{:.0}% CI", confidence * 100.0),
        ],
        rows: vec![
            vec![
                "PPM < LSL".to_owned(),
                ppm(overall.low),
                ci(overall.low_ci),
                ppm(within.low),
                ci(within.low_ci),
            ],
            vec![
                "PPM > USL".to_owned(),
                ppm(overall.high),
                ci(overall.high_ci),
                ppm(within.high),
                ci(within.high_ci),
            ],
            vec![
                "PPM total".to_owned(),
                ppm(overall.total),
                ci(overall.total_ci),
                ppm(within.total),
                ci(within.total_ci),
            ],
        ],
    }
}

fn observed_ppm_table(
    low: Option<usize>,
    high: Option<usize>,
    total: usize,
    n: usize,
    confidence: f64,
) -> Item {
    let ppm = |count: usize| count as f64 / n as f64 * 1_000_000.0;
    let row = |label: &str, count: Option<usize>| match count {
        Some(count) => {
            let interval = wilson_interval(count, n, confidence)
                .map(|(low, high)| format!("{:.1} – {:.1}", low * 1_000_000.0, high * 1_000_000.0))
                .unwrap_or_else(|| "—".to_owned());
            vec![label.to_owned(), format!("{:.1}", ppm(count)), interval]
        }
        None => vec![label.to_owned(), "—".to_owned(), "—".to_owned()],
    };
    Item::Table {
        headers: vec![
            "Observed PPM".to_owned(),
            "Estimate".to_owned(),
            format!("{:.0}% CI", confidence * 100.0),
        ],
        rows: vec![
            row("< LSL", low),
            row("> USL", high),
            row("Total", Some(total)),
        ],
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct BootstrapPpm {
    low: Option<(f64, f64)>,
    high: Option<(f64, f64)>,
    total: Option<(f64, f64)>,
    reps: usize,
    failures: usize,
}

fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let position = (sorted.len() - 1) as f64 * p;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        return sorted.get(lower).copied();
    }
    let weight = position - lower as f64;
    Some(sorted[lower] * (1.0 - weight) + sorted[upper] * weight)
}

fn fitted_ppm(
    sample: &Sample,
    distribution: DistributionKind,
    lsl: Option<f64>,
    usl: Option<f64>,
) -> Result<(Option<f64>, Option<f64>)> {
    match distribution {
        DistributionKind::Lognormal => {
            let fitted = Lognormal::fit_mle(sample)?;
            Ok((
                lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
            ))
        }
        DistributionKind::Weibull => {
            let fitted = Weibull::fit_mle(sample)?;
            Ok((
                lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
            ))
        }
        DistributionKind::Exponential => {
            let fitted = Exponential::fit_mle(sample)?;
            Ok((
                lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
            ))
        }
        DistributionKind::Logistic => {
            let fitted = Logistic::fit_mle(sample)?;
            Ok((
                lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
            ))
        }
        DistributionKind::Loglogistic => {
            let fitted = Loglogistic::fit_mle(sample)?;
            Ok((
                lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
            ))
        }
        DistributionKind::Gamma => {
            let fitted = Gamma::fit_mle(sample)?;
            Ok((
                lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
            ))
        }
        DistributionKind::ExtremeValue => {
            let fitted = ExtremeValue::fit_mle(sample)?;
            Ok((
                lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
            ))
        }
        DistributionKind::Weibull3 => {
            let fitted = Weibull3::fit_mle(sample)?;
            Ok((
                lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
            ))
        }
        DistributionKind::Lognormal3 => {
            let fitted = Lognormal3::fit_mle(sample)?;
            Ok((
                lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
            ))
        }
        DistributionKind::Normal => Err(Error::Unsupported(
            "bootstrap intervals are only used for non-normal capability".to_owned(),
        )),
    }
}

fn bootstrap_ppm(
    sample: &Sample,
    distribution: DistributionKind,
    lsl: Option<f64>,
    usl: Option<f64>,
    requested: usize,
    confidence: f64,
) -> BootstrapPpm {
    let automatic = match sample.n() {
        0..=500 => 400,
        501..=5000 => 200,
        _ => 0,
    };
    let reps = if requested == 0 {
        automatic
    } else {
        requested.min(2_000)
    };
    if reps == 0 {
        return BootstrapPpm {
            low: None,
            high: None,
            total: None,
            reps: 0,
            failures: 0,
        };
    }
    let values = sample.values();
    let mut rng = SplitMix64::new(2026);
    let mut lows = Vec::with_capacity(reps);
    let mut highs = Vec::with_capacity(reps);
    let mut totals = Vec::with_capacity(reps);
    let mut failures = 0usize;
    for _ in 0..reps {
        let mut resampled = Vec::with_capacity(values.len());
        for _ in 0..values.len() {
            let index = (rng.next_u64() % values.len() as u64) as usize;
            resampled.push(values[index]);
        }
        let Ok(resample) = Sample::from_valid(resampled, 0) else {
            failures += 1;
            continue;
        };
        match fitted_ppm(&resample, distribution, lsl, usl) {
            Ok((low, high)) => {
                if let Some(value) = low {
                    lows.push(value);
                }
                if let Some(value) = high {
                    highs.push(value);
                }
                let total = match (low, high) {
                    (Some(low), Some(high)) => Some(low + high),
                    (Some(low), None) => Some(low),
                    (None, Some(high)) => Some(high),
                    (None, None) => None,
                };
                if let Some(value) = total {
                    totals.push(value);
                }
            }
            Err(_) => failures += 1,
        }
    }
    let interval = |values: &mut Vec<f64>| {
        values.sort_by(f64::total_cmp);
        let alpha = (1.0 - confidence) / 2.0;
        match (percentile(values, alpha), percentile(values, 1.0 - alpha)) {
            (Some(low), Some(high)) => Some((low, high)),
            _ => None,
        }
    };
    BootstrapPpm {
        low: interval(&mut lows),
        high: interval(&mut highs),
        total: interval(&mut totals),
        reps,
        failures,
    }
}

fn bootstrap_ppm_table(bootstrap: &BootstrapPpm, confidence: f64) -> Item {
    let row = |label: &str, interval: Option<(f64, f64)>| {
        vec![
            label.to_owned(),
            interval
                .map(|(low, high)| format!("{low:.1} – {high:.1}"))
                .unwrap_or_else(|| "—".to_owned()),
        ]
    };
    Item::Table {
        headers: vec![
            "Bootstrap expected PPM".to_owned(),
            format!("{:.0}% percentile CI", confidence * 100.0),
        ],
        rows: vec![
            row("< LSL", bootstrap.low),
            row("> USL", bootstrap.high),
            row("Total", bootstrap.total),
        ],
    }
}

fn fit_gate_table(a2: f64, p: f64) -> Item {
    Item::Table {
        headers: vec!["Fit check".to_owned(), "Value".to_owned()],
        rows: vec![
            vec!["Anderson–Darling (A²)".to_owned(), format!("{a2:.6}")],
            vec!["p-value".to_owned(), format!("{p:.6}")],
            vec![
                "Decision at 5%".to_owned(),
                if p < 0.05 {
                    "rejected".to_owned()
                } else {
                    "not rejected".to_owned()
                },
            ],
        ],
    }
}

/// Distribution-based capability for Lognormal, Weibull and Exponential.
///
/// The fitted distribution supplies the expected fraction below/above the
/// specification limits. This is deliberately separate from the normal
/// within/overall indices: there is no `Cp`/`Cpk` analogue in the report.
pub fn capability_non_normal(
    df: &DataFrame,
    column_name: &str,
    lsl: Option<f64>,
    usl: Option<f64>,
    distribution: DistributionKind,
    confidence: f64,
    bootstrap_reps: usize,
) -> Result<AnalysisResult> {
    if lsl.is_none() && usl.is_none() {
        return Err(Error::Unsupported(
            "capability needs at least one specification limit".to_owned(),
        ));
    }
    if let (Some(lsl), Some(usl)) = (lsl, usl)
        && lsl >= usl
    {
        return Err(Error::Unsupported(
            "lower specification limit must be below the upper limit".to_owned(),
        ));
    }
    if distribution == DistributionKind::Normal {
        return Err(Error::Unsupported(
            "use the normal-capability path for a Normal distribution".to_owned(),
        ));
    }

    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    if sample.n() < 2 {
        return Err(Error::SampleTooSmall {
            n: sample.n(),
            min: 2,
        });
    }

    let (name, parameters, expected_low, expected_high, cdf_lsl, cdf_usl, fit_a2, fit_p) =
        match distribution {
            DistributionKind::Lognormal => {
                // The documented lognormal capability report uses the same
                // sample moments (n−1) on ln(x) as the probability-plot path,
                // not the MLE (which divides by n).
                let fitted = Lognormal::fit_moments(&sample)?;
                let parameters = fitted.parameters();
                let a2 = anderson_darling(&fitted, &sample)?;
                let p = ad_p_value(sample.n(), a2)?;
                (
                    fitted.name(),
                    parameters,
                    lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                    usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
                    lsl.map(|limit| fitted.cdf(limit)),
                    usl.map(|limit| fitted.cdf(limit)),
                    a2,
                    p,
                )
            }
            DistributionKind::Weibull => {
                let fitted = Weibull::fit_mle(&sample)?;
                let parameters = fitted.parameters();
                let a2 = anderson_darling(&fitted, &sample)?;
                let p = ad_p_value_weibull(sample.n(), a2)?;
                (
                    fitted.name(),
                    parameters,
                    lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                    usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
                    lsl.map(|limit| fitted.cdf(limit)),
                    usl.map(|limit| fitted.cdf(limit)),
                    a2,
                    p,
                )
            }
            DistributionKind::Exponential => {
                let fitted = Exponential::fit_mle(&sample)?;
                let parameters = fitted.parameters();
                let a2 = anderson_darling(&fitted, &sample)?;
                let p = ad_p_value_exponential(sample.n(), a2)?;
                (
                    fitted.name(),
                    parameters,
                    lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                    usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
                    lsl.map(|limit| fitted.cdf(limit)),
                    usl.map(|limit| fitted.cdf(limit)),
                    a2,
                    p,
                )
            }
            DistributionKind::Logistic => {
                let fitted = Logistic::fit_mle(&sample)?;
                let parameters = fitted.parameters();
                let a2 = anderson_darling(&fitted, &sample)?;
                let p = ad_p_value(sample.n(), a2)?;
                (
                    fitted.name(),
                    parameters,
                    lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                    usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
                    lsl.map(|limit| fitted.cdf(limit)),
                    usl.map(|limit| fitted.cdf(limit)),
                    a2,
                    p,
                )
            }
            DistributionKind::Loglogistic => {
                let fitted = Loglogistic::fit_mle(&sample)?;
                let parameters = fitted.parameters();
                let a2 = anderson_darling(&fitted, &sample)?;
                let p = ad_p_value(sample.n(), a2)?;
                (
                    fitted.name(),
                    parameters,
                    lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                    usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
                    lsl.map(|limit| fitted.cdf(limit)),
                    usl.map(|limit| fitted.cdf(limit)),
                    a2,
                    p,
                )
            }
            DistributionKind::Gamma => {
                let fitted = Gamma::fit_mle(&sample)?;
                let parameters = fitted.parameters();
                let a2 = anderson_darling(&fitted, &sample)?;
                let p = ad_p_value(sample.n(), a2)?;
                (
                    fitted.name(),
                    parameters,
                    lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                    usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
                    lsl.map(|limit| fitted.cdf(limit)),
                    usl.map(|limit| fitted.cdf(limit)),
                    a2,
                    p,
                )
            }
            DistributionKind::ExtremeValue => {
                let fitted = ExtremeValue::fit_mle(&sample)?;
                let parameters = fitted.parameters();
                let a2 = anderson_darling(&fitted, &sample)?;
                let p = ad_p_value(sample.n(), a2)?;
                (
                    fitted.name(),
                    parameters,
                    lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                    usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
                    lsl.map(|limit| fitted.cdf(limit)),
                    usl.map(|limit| fitted.cdf(limit)),
                    a2,
                    p,
                )
            }
            DistributionKind::Weibull3 => {
                let fitted = Weibull3::fit_mle(&sample)?;
                let parameters = fitted.parameters();
                let a2 = anderson_darling(&fitted, &sample)?;
                let p = ad_p_value(sample.n(), a2)?;
                (
                    fitted.name(),
                    parameters,
                    lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                    usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
                    lsl.map(|limit| fitted.cdf(limit)),
                    usl.map(|limit| fitted.cdf(limit)),
                    a2,
                    p,
                )
            }
            DistributionKind::Lognormal3 => {
                let fitted = Lognormal3::fit_mle(&sample)?;
                let parameters = fitted.parameters();
                let a2 = anderson_darling(&fitted, &sample)?;
                let p = ad_p_value(sample.n(), a2)?;
                (
                    fitted.name(),
                    parameters,
                    lsl.map(|limit| fitted.cdf(limit) * 1_000_000.0),
                    usl.map(|limit| fitted.sf(limit) * 1_000_000.0),
                    lsl.map(|limit| fitted.cdf(limit)),
                    usl.map(|limit| fitted.cdf(limit)),
                    a2,
                    p,
                )
            }
            DistributionKind::Normal => unreachable!(),
        };

    // Non-normal capability uses the normal-equivalent Z of the fitted
    // distribution: Z.LSL = Φ⁻¹(F(LSL)), Z.USL = Φ⁻¹(F(USL)). The documented behaviour
    // report shows Pp/PPL/PPU/Ppk built from these scores.
    let z_of = |p: Option<f64>| p.and_then(|p| (p > 0.0 && p < 1.0).then(|| normal_quantile(p)));
    let z_lsl = z_of(cdf_lsl);
    let z_usl = z_of(cdf_usl);
    let ppl = z_lsl.map(|z| -z / 3.0);
    let ppu = z_usl.map(|z| z / 3.0);
    let ppk = match (ppl, ppu) {
        (Some(low), Some(high)) => Some(low.min(high)),
        (Some(low), None) => Some(low),
        (None, Some(high)) => Some(high),
        (None, None) => None,
    };
    let pp = match (z_lsl, z_usl) {
        (Some(low), Some(high)) => Some((high - low) / 6.0),
        _ => None,
    };
    let fmt3 = |value: Option<f64>| match value {
        Some(value) => format!("{value:.3}"),
        None => "—".to_owned(),
    };

    let n = sample.n();
    let observed_low_count = lsl.map(|limit| {
        sample
            .values()
            .iter()
            .filter(|&&value| value < limit)
            .count()
    });
    let observed_high_count = usl.map(|limit| {
        sample
            .values()
            .iter()
            .filter(|&&value| value > limit)
            .count()
    });
    let observed_low = observed_low_count.map(|count| count as f64 / n as f64 * 1_000_000.0);
    let observed_high = observed_high_count.map(|count| count as f64 / n as f64 * 1_000_000.0);
    let expected_total = match (expected_low, expected_high) {
        (Some(low), Some(high)) => Some(low + high),
        (Some(low), None) => Some(low),
        (None, Some(high)) => Some(high),
        (None, None) => None,
    };
    let observed_total_count = observed_low_count.unwrap_or(0) + observed_high_count.unwrap_or(0);
    let observed_total = Some(observed_total_count as f64 / n as f64 * 1_000_000.0);
    let fmt = |value: Option<f64>| match value {
        Some(value) => format!("{value:.1}"),
        None => "—".to_owned(),
    };
    let rows = vec![
        vec!["PPM < LSL".to_owned(), fmt(expected_low), fmt(observed_low)],
        vec![
            "PPM > USL".to_owned(),
            fmt(expected_high),
            fmt(observed_high),
        ],
        vec![
            "PPM total".to_owned(),
            fmt(expected_total),
            fmt(observed_total),
        ],
    ];

    let mut result = AnalysisResult::new(
        format!("Capability Analysis: {name} — column {column_name}"),
        "Distribution fit; expected and observed PPM",
    );
    result.n = sample.n();
    result.excluded = sample.excluded();
    result.parameters = parameters;
    result.extra.push(Item::Table {
        headers: vec![
            "Metric".to_owned(),
            "Expected".to_owned(),
            "Observed".to_owned(),
        ],
        rows,
    });
    result.extra.push(fit_gate_table(fit_a2, fit_p));
    result.extra.push(Item::Table {
        headers: vec!["Index".to_owned(), "Value".to_owned()],
        rows: vec![
            vec!["Pp".to_owned(), fmt3(pp)],
            vec!["PPL".to_owned(), fmt3(ppl)],
            vec!["PPU".to_owned(), fmt3(ppu)],
            vec!["Ppk".to_owned(), fmt3(ppk)],
        ],
    });
    result.extra.push(observed_ppm_table(
        observed_low_count,
        observed_high_count,
        observed_total_count,
        n,
        confidence,
    ));
    let bootstrap = bootstrap_ppm(&sample, distribution, lsl, usl, bootstrap_reps, confidence);
    if bootstrap.reps > 0 {
        result.extra.push(Item::Message(format!(
            "Nonparametric bootstrap: {} resamples, {} failed fits.",
            bootstrap.reps, bootstrap.failures
        )));
        result
            .extra
            .push(bootstrap_ppm_table(&bootstrap, confidence));
    }
    result.notes = vec![
        (
            "Estimator".to_owned(),
            if distribution == DistributionKind::Lognormal {
                "sample moments (n−1) on ln(x), as in the probability-plot path".to_owned()
            } else {
                "maximum likelihood on the original scale".to_owned()
            },
        ),
        (
            "Data policy".to_owned(),
            "empty/non-finite values excluded and counted".to_owned(),
        ),
        (
            "Specification".to_owned(),
            format!(
                "LSL={}, USL={}",
                lsl.map_or("—".to_owned(), |value| format!("{value:.6}")),
                usl.map_or("—".to_owned(), |value| format!("{value:.6}"))
            ),
        ),
        (
            "Confidence intervals".to_owned(),
            format!(
                "observed PPM uses a Wilson interval at {:.0}%; expected PPM uses a nonparametric \
                 bootstrap percentile interval when resampling is enabled",
                confidence * 100.0
            ),
        ),
        (
            "Capability indices".to_owned(),
            "normal-equivalent Z of the fitted distribution: Pp/PPL/PPU/Ppk from              Φ⁻¹(F(LSL)), Φ⁻¹(F(USL)) and the median (Z = 0), as the documented behaviour does"
                .to_owned(),
        ),
        (
            "Bootstrap".to_owned(),
            "observations resampled with replacement, deterministically seeded (2026); 0 = \
             automatic (400 for n ≤ 500, 200 for n ≤ 5000, off above that)"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(sample.n(), sample.excluded(), sample.ties());
    if matches!(
        distribution,
        DistributionKind::Logistic
            | DistributionKind::Loglogistic
            | DistributionKind::Gamma
            | DistributionKind::ExtremeValue
    ) {
        result.notes.push((
            "Fit check".to_owned(),
            "AD p-value uses the Normal composite-AD approximation; a family-specific table is \
             not available"
                .to_owned(),
        ));
    }
    if bootstrap.reps == 0 {
        result.warnings.push(
            "bootstrap interval for expected PPM was skipped; request it explicitly or use a \
             smaller sample"
                .to_owned(),
        );
    } else if bootstrap.failures > 0 {
        result.warnings.push(format!(
            "{} bootstrap resamples could not be fitted and were skipped",
            bootstrap.failures
        ));
    }
    result.warnings.push(
        "non-normal capability indices and PPM are validated for Weibull, lognormal and \
         exponential; the other families use the same construction but have no documented \
         documented output yet"
            .to_owned(),
    );
    Ok(result)
}

/// Clean numeric sample of a normal capability study.
///
/// With a subgroup column, a row without a label cannot enter a subgroup, so it
/// is excluded from every estimate and counted in [`Sample::excluded`].
pub(crate) fn capability_sample(
    df: &DataFrame,
    column_name: &str,
    subgroup_column: Option<&str>,
) -> Result<Sample> {
    match subgroup_column {
        Some(label) => {
            let values = df.column(column_name)?;
            let labels = df.column(label)?;
            let mut kept = Vec::with_capacity(df.n_rows());
            let mut excluded = 0usize;
            for row in 0..df.n_rows() {
                match (values.value(row), labels.label(row)) {
                    (Some(value), Some(_)) => kept.push(value),
                    _ => excluded += 1,
                }
            }
            Sample::from_valid(kept, excluded)
        }
        None => df.column(column_name)?.sample(MissingPolicy::Drop),
    }
}

/// Spread estimates of a normal capability study.
pub(crate) struct SigmaEstimates {
    /// Pooled within-subgroup standard deviation.
    pub within: f64,
    /// Degrees of freedom of the within estimate.
    pub within_df: f64,
    /// Overall sample standard deviation.
    pub overall: f64,
    /// Number of label subgroups, when a subgroup column supplied them.
    pub groups: Option<usize>,
}

/// Computes the within and overall spread of a normal capability study.
///
/// The label groups keep their first-appearance order; the values arrive on the
/// original scale and are Box-Cox transformed with `box_lambda` when it is set.
pub(crate) fn sigma_estimates(
    sample: &Sample,
    df: &DataFrame,
    column_name: &str,
    subgroup: usize,
    subgroup_column: Option<&str>,
    unbiasing_within: bool,
    unbiasing_overall: bool,
    box_lambda: Option<f64>,
) -> Result<SigmaEstimates> {
    let n = sample.n();
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let mut overall = sample.sd()?;
    if overall <= 0.0 {
        return Err(Error::ConstantSample);
    }

    // Subgroups from a label column: the pooled within estimate uses each
    // subgroup's own mean.
    let grouped: Option<Vec<Vec<f64>>> = match subgroup_column {
        Some(label) => Some(
            crate::spc::group_samples(df, column_name, label)?
                .iter()
                .map(|group| group.values().to_vec())
                .collect(),
        ),
        None => None,
    };

    let (within, within_df) = if let Some(groups) = &grouped {
        // One subgroup per distinct label, in first-appearance order.
        if groups.len() < 2 {
            return Err(Error::Unsupported(
                "subgroup capability needs at least two subgroups".to_owned(),
            ));
        }
        let mut total = 0usize;
        let mut ss = 0.0;
        for group in groups {
            let values: Vec<f64> = match box_lambda {
                Some(lambda) => group
                    .iter()
                    .map(|&value| crate::boxcox::transform(value, lambda))
                    .collect::<Result<Vec<_>>>()?,
                None => group.clone(),
            };
            if values.len() < 2 {
                return Err(Error::Unsupported(
                    "every subgroup needs at least two observations".to_owned(),
                ));
            }
            let group_mean = values.iter().sum::<f64>() / values.len() as f64;
            ss += values
                .iter()
                .map(|value| (value - group_mean).powi(2))
                .sum::<f64>();
            total += values.len();
        }
        let degrees = total - groups.len();
        let pooled = (ss / degrees as f64).sqrt();
        let within = if unbiasing_within {
            pooled / c4(degrees + 1)
        } else {
            pooled
        };
        (within, degrees as f64)
    } else if subgroup == 1 {
        let moving = sample
            .values()
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .sum::<f64>()
            / (n - 1) as f64;
        // Published d2 for n = 2 (1.128), the constant the documented report
        // uses; 2/sqrt(pi) differs by 0.03 % and was visible in the record.
        (moving / 1.128, (n - 1) as f64)
    } else {
        if n % subgroup != 0 {
            return Err(Error::Unsupported(
                "the sample size must be divisible by the subgroup size".to_owned(),
            ));
        }
        let groups = n / subgroup;
        if groups < 2 {
            return Err(Error::Unsupported(
                "subgroup capability needs at least two subgroups".to_owned(),
            ));
        }
        let mut ss = 0.0;
        for group in sample.values().chunks(subgroup) {
            let group_mean = group.iter().sum::<f64>() / subgroup as f64;
            ss += group
                .iter()
                .map(|value| (value - group_mean).powi(2))
                .sum::<f64>();
        }
        let degrees = n - groups;
        let pooled = (ss / degrees as f64).sqrt();
        let within = if unbiasing_within {
            pooled / c4(degrees + 1)
        } else {
            pooled
        };
        (within, degrees as f64)
    };
    if unbiasing_overall {
        overall /= c4(n);
    }
    Ok(SigmaEstimates {
        within,
        within_df,
        overall,
        groups: grouped.as_ref().map(Vec::len),
    })
}

/// Normal process capability with within/overall sigma and ppm.
///
/// This is a documented standard implementation, not yet verified against a
/// documented capability report: the exact unbiasing and
/// confidence-interval conventions remain pending.
pub fn capability_normal(
    df: &DataFrame,
    column_name: &str,
    lsl: Option<f64>,
    usl: Option<f64>,
    options: CapabilityOptions,
    subgroup_column: Option<&str>,
) -> Result<AnalysisResult> {
    let CapabilityOptions {
        target,
        bootstrap_reps: _,
        box_cox,
        subgroup,
        unbiasing_within,
        unbiasing_overall,
        confidence,
        require_stability,
    } = options;
    if !confidence.is_finite() || !(0.5..1.0).contains(&confidence) {
        return Err(Error::InvalidParameter {
            name: "capability confidence level",
            value: confidence,
        });
    }
    if lsl.is_none() && usl.is_none() {
        return Err(Error::Unsupported(
            "capability needs at least one specification limit".to_owned(),
        ));
    }
    if let (Some(lsl), Some(usl)) = (lsl, usl)
        && lsl >= usl
    {
        return Err(Error::Unsupported(
            "lower specification limit must be below the upper limit".to_owned(),
        ));
    }
    if subgroup == 0 {
        return Err(Error::Unsupported(
            "subgroup size must be at least 1".to_owned(),
        ));
    }

    let mut sample = capability_sample(df, column_name, subgroup_column)?;
    let stability = crate::spc::individuals_moving_range(&sample).ok();
    if require_stability {
        let chart = stability.as_ref().ok_or_else(|| {
            Error::Unsupported("the capability stability screen could not be computed".to_owned())
        })?;
        if !chart.signals.is_empty() {
            return Err(Error::Unsupported(format!(
                "the process shows {} I–MR special-cause signal(s); capability was not computed",
                chart.signals.len()
            )));
        }
    }
    let original_lsl = lsl;
    let original_usl = usl;
    let mut lsl = lsl;
    let mut usl = usl;
    let mut target = target;
    let mut box_lambda: Option<f64> = None;
    if box_cox {
        if sample.n() < 3 {
            return Err(Error::SampleTooSmall {
                n: sample.n(),
                min: 3,
            });
        }
        let lambda = crate::boxcox::lambda(&sample)?;
        let transformed = sample
            .values()
            .iter()
            .map(|&value| crate::boxcox::transform(value, lambda))
            .collect::<Result<Vec<_>>>()?;
        sample = Sample::from_valid(transformed, sample.excluded())?;
        let transform_limit = |limit: f64| -> Result<f64> {
            if limit <= 0.0 {
                return Err(Error::Unsupported(
                    "Box-Cox capability requires positive specification limits".to_owned(),
                ));
            }
            crate::boxcox::transform(limit, lambda)
        };
        lsl = lsl.map(transform_limit).transpose()?;
        usl = usl.map(transform_limit).transpose()?;
        target = target.map(transform_limit).transpose()?;
        box_lambda = Some(lambda);
    }
    let n = sample.n();
    if n < 2 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let mean = sample.mean()?;
    let estimates = sigma_estimates(
        &sample,
        df,
        column_name,
        subgroup,
        subgroup_column,
        unbiasing_within,
        unbiasing_overall,
        box_lambda,
    )?;
    let within = estimates.within;
    let within_df = estimates.within_df;
    let overall = estimates.overall;
    let grouped = estimates.groups;

    let normal_fit = Normal::fit_moments(&sample)?;
    let fit_a2 = anderson_darling(&normal_fit, &sample)?;
    let fit_p = ad_p_value(n, fit_a2)?;

    let cp = |sigma: f64| match (lsl, usl) {
        (Some(lsl), Some(usl)) => Some((usl - lsl) / (6.0 * sigma)),
        _ => None,
    };
    let cpk = |sigma: f64| {
        let lower = lsl.map(|limit| (mean - limit) / (3.0 * sigma));
        let upper = usl.map(|limit| (limit - mean) / (3.0 * sigma));
        match (lower, upper) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    };
    let cpm = |sigma: f64| match (lsl, usl, target) {
        (Some(lsl), Some(usl), Some(target)) => {
            Some((usl - lsl) / (6.0 * (sigma * sigma + (mean - target).powi(2)).sqrt()))
        }
        _ => None,
    };
    let overall_ppm = expected_ppm_interval(mean, overall, (n - 1) as f64, n, lsl, usl, confidence);
    let within_ppm = expected_ppm_interval(mean, within, within_df, n, lsl, usl, confidence);
    let observed_low = lsl.map(|limit| {
        sample
            .values()
            .iter()
            .filter(|&&value| value < limit)
            .count()
    });
    let observed_high = usl.map(|limit| {
        sample
            .values()
            .iter()
            .filter(|&&value| value > limit)
            .count()
    });
    let observed_total = observed_low.unwrap_or(0) + observed_high.unwrap_or(0);
    let observed = observed_total as f64 / n as f64 * 1_000_000.0;

    let cp_within = cp(within);
    let cp_overall = cp(overall);
    let cpk_within = cpk(within);
    let cpk_overall = cpk(overall);
    let cp_within_ci =
        cp_within.and_then(|value| cp_confidence_interval(value, within_df, confidence));
    let cp_overall_ci =
        cp_overall.and_then(|value| cp_confidence_interval(value, (n - 1) as f64, confidence));
    let cpk_within_ci = cpk_within.and_then(|value| cpk_confidence_interval(value, n, confidence));
    let cpk_overall_ci =
        cpk_overall.and_then(|value| cpk_confidence_interval(value, n, confidence));

    let fmt = |value: Option<f64>| match value {
        Some(value) => format!("{value:.3}"),
        None => "—".to_owned(),
    };
    let fmt_ci = |value: Option<(f64, f64)>| match value {
        Some((low, high)) => format!("{low:.3} – {high:.3}"),
        None => "—".to_owned(),
    };
    let rows = vec![
        vec![
            "StDev".to_owned(),
            format!("{within:.6}"),
            format!("{overall:.6}"),
        ],
        vec!["Cp".to_owned(), fmt(cp_within), "—".to_owned()],
        vec!["Cpm".to_owned(), fmt(cpm(within)), "—".to_owned()],
        vec![
            "Cpm (overall)".to_owned(),
            "—".to_owned(),
            fmt(cpm(overall)),
        ],
        vec![
            format!("Cp {:.0}% CI", confidence * 100.0),
            fmt_ci(cp_within_ci),
            "—".to_owned(),
        ],
        vec!["Cpk".to_owned(), fmt(cpk_within), "—".to_owned()],
        vec![
            format!("Cpk {:.0}% CI", confidence * 100.0),
            fmt_ci(cpk_within_ci),
            "—".to_owned(),
        ],
        vec!["Pp".to_owned(), "—".to_owned(), fmt(cp_overall)],
        vec![
            format!("Pp {:.0}% CI", confidence * 100.0),
            "—".to_owned(),
            fmt_ci(cp_overall_ci),
        ],
        vec!["Ppk".to_owned(), "—".to_owned(), fmt(cpk_overall)],
        vec![
            format!("Ppk {:.0}% CI", confidence * 100.0),
            "—".to_owned(),
            fmt_ci(cpk_overall_ci),
        ],
        vec![
            "PPM (observed)".to_owned(),
            format!("{observed:.1}"),
            format!("{observed:.1}"),
        ],
    ];

    let mut result = AnalysisResult::new(
        if box_lambda.is_some() {
            format!("Capability Analysis: Normal (Box-Cox) — column {column_name}")
        } else {
            format!("Capability Analysis: Normal — column {column_name}")
        },
        if box_lambda.is_some() {
            "Box-Cox transformation; within/overall normal capability"
        } else {
            "Within/overall normal capability"
        },
    );
    result.n = n;
    result.excluded = sample.excluded();
    result.parameters.push(("Mean (µ)", mean));
    if let Some(lambda) = box_lambda {
        result.parameters.push(("Box-Cox λ", lambda));
    }
    if let Some(chart) = &stability {
        result
            .statistics
            .push(("I–MR signals", chart.signals.len() as f64));
        result.extra.push(Item::Table {
            headers: vec![
                "Stability test".to_owned(),
                "Chart".to_owned(),
                "Point".to_owned(),
                "Signal".to_owned(),
            ],
            rows: crate::spc::signal_rows(&chart.signals),
        });
        if !chart.signals.is_empty() {
            result.warnings.push(
                "the I–MR stability screen found special-cause signals; capability may be                  misleading"
                    .to_owned(),
            );
        }
    }
    result.extra.push(Item::Message(match (grouped, subgroup) {
        (Some(_), _) => {
            "Within = pooled standard deviation over the subgroups (n − g degrees of freedom); \
             overall = sample standard deviation."
                .to_owned()
        }
        (None, 1) => {
            "Within = average moving range / d2(2) for subgroup 1; overall = sample standard \
             deviation."
                .to_owned()
        }
        (None, _) => {
            "Within = pooled standard deviation over consecutive subgroups (n − g degrees of \
             freedom); overall = sample standard deviation."
                .to_owned()
        }
    }));
    result.extra.push(Item::Table {
        headers: vec![
            "Metric".to_owned(),
            "Within".to_owned(),
            "Overall".to_owned(),
        ],
        rows,
    });
    result.extra.push(fit_gate_table(fit_a2, fit_p));
    result.extra.push(observed_ppm_table(
        observed_low,
        observed_high,
        observed_total,
        n,
        confidence,
    ));
    if let (Some(overall_ppm), Some(within_ppm)) = (overall_ppm, within_ppm) {
        result.extra.push(Item::Message(
            "Expected PPM from the fitted normal; intervals use the z scale (total by logit)."
                .to_owned(),
        ));
        result
            .extra
            .push(expected_ppm_table(overall_ppm, within_ppm, confidence));
    }
    result.notes = vec![
        (
            if grouped.is_some() {
                "Subgroups"
            } else {
                "Subgroup size"
            }
            .to_owned(),
            match (subgroup_column, grouped) {
                (Some(label), Some(groups)) => {
                    format!("column {label}; {groups} subgroups")
                }
                _ => subgroup.to_string(),
            },
        ),
        (
            "Specification".to_owned(),
            format!(
                "LSL={}, USL={}",
                lsl.map_or("—".to_owned(), |value| format!("{value:.6}")),
                usl.map_or("—".to_owned(), |value| format!("{value:.6}"))
            ),
        ),
        (
            "Unbiasing".to_owned(),
            format!(
                "within {}, overall {}",
                if unbiasing_within { "on" } else { "off" },
                if unbiasing_overall { "on" } else { "off" }
            ),
        ),
        (
            "Confidence intervals".to_owned(),
            format!(
                "{:.0}%; Cp/Pp chi-square, Cpk/Ppk normal approximation, expected PPM \
                 z-scale normal approximation (total by logit and delta method)",
                confidence * 100.0
            ),
        ),
        (
            "Cpm target".to_owned(),
            match (lsl, usl, target) {
                (Some(_), Some(_), Some(target)) => format!("{target:.6}"),
                (Some(_), Some(_), None) => {
                    "not specified; Cpm is not computed (the documented behaviour leaves it blank)"
                        .to_owned()
                }
                _ => "not defined for one-sided specifications".to_owned(),
            },
        ),
    ];
    if let Some(lambda) = box_lambda {
        result.notes.push((
            "Box-Cox".to_owned(),
            format!(
                "λ = {lambda:.6}; y = (x^λ − 1)/λ for λ ≠ 0, ln x for λ = 0; no rounding applied"
            ),
        ));
        result.notes.push((
            "Original specification".to_owned(),
            format!(
                "LSL={}, USL={}",
                original_lsl.map_or("—".to_owned(), |value| format!("{value:.6}")),
                original_usl.map_or("—".to_owned(), |value| format!("{value:.6}"))
            ),
        ));
        result.notes.push((
            "Transformed specification".to_owned(),
            format!(
                "LSL={}, USL={}",
                lsl.map_or("—".to_owned(), |value| format!("{value:.6}")),
                usl.map_or("—".to_owned(), |value| format!("{value:.6}"))
            ),
        ));
    }
    result.warnings = standard_warnings(n, sample.excluded(), sample.ties());
    if fit_p < 0.05 {
        result.warnings.push(
            "the Normal fit is rejected by Anderson–Darling at 5%; capability indices may be misleading"
                .to_owned(),
        );
    }
    result.warnings.push(
        "capability assumes a stable process; no control-chart stability test was run".to_owned(),
    );
    result.warnings.push(
        "Cp/Ppk intervals use standard approximations; expected PPM intervals use the \
         published z-scale and logit approximations"
            .to_owned(),
    );
    Ok(result)
}

/// Selection rule for the distribution comparison.
///
/// Returns the name and whether it was accepted at 5%. Among the accepted
/// fits the lowest AIC wins; otherwise the lowest Anderson–Darling statistic.
fn best_distribution(fits: &[(String, f64, f64, f64)]) -> Option<(&str, bool)> {
    if fits.is_empty() {
        return None;
    }
    if let Some(best) = fits
        .iter()
        .filter(|(_, _, p, _)| *p >= 0.05)
        .min_by(|a, b| a.3.total_cmp(&b.3))
    {
        return Some((&best.0, true));
    }
    fits.iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|fit| (fit.0.as_str(), false))
}

/// Fits the families of the comparison and ranks them by Anderson–Darling,
/// AIC and BIC.
///
/// The rows are normal, Box-Cox normal, Johnson (the best of SU/SB/SL),
/// logistic, extreme value, lognormal, Weibull and exponential. A family whose
/// fit does not converge is reported as a note instead of a row.
pub fn distribution_comparison(df: &DataFrame, column_name: &str) -> Result<AnalysisResult> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let positive = sample.values().iter().all(|&x| x > 0.0);

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut fits: Vec<(String, f64, f64, f64)> = Vec::new();
    let mut notes: Vec<(String, String)> = Vec::new();

    let mut add_row =
        |name: &str, parameters: String, a2: f64, p: f64, log_likelihood: f64, k: usize| {
            let n = sample.n() as f64;
            let aic = 2.0 * k as f64 - 2.0 * log_likelihood;
            let bic = k as f64 * n.ln() - 2.0 * log_likelihood;
            rows.push(vec![
                name.to_owned(),
                parameters,
                format!("{a2:.6}"),
                format!("{p:.6}"),
                format!("{aic:.3}"),
                format!("{bic:.3}"),
            ]);
            fits.push((name.to_owned(), a2, p, aic));
        };

    // Normal (always applicable).
    let normal = Normal::fit(&sample)?;
    let a2 = anderson_darling(&normal, &sample)?;
    add_row(
        "Normal",
        format!("µ={:.6}, σ={:.6}", normal.mu(), normal.sigma()),
        a2,
        ad_p_value(sample.n(), a2)?,
        log_likelihood(&normal, &sample),
        normal.n_params(),
    );

    // Box–Cox normal: the profile-likelihood transformation followed by a
    // normal fit. The log-likelihood carries the Jacobian so that it is on the
    // original-data scale and comparable with the other rows.
    if positive {
        match crate::boxcox::identify_normal(&sample) {
            Ok(identification) => add_row(
                "Box-Cox Normal",
                format!(
                    "λ={:.6}, µ={:.6}, σ={:.6} (transformed)",
                    identification.lambda, identification.mu, identification.sigma
                ),
                identification.a2,
                identification.p_value,
                identification.log_likelihood,
                identification.n_params,
            ),
            Err(error) => notes.push(("Box-Cox fit".to_owned(), format!("skipped: {error}"))),
        }
    }

    // Johnson SU/SB/SL, the best of the three by AIC.
    match crate::johnson::Johnson::fit_best(&sample) {
        Ok(johnson) => {
            let a2 = anderson_darling(&johnson, &sample)?;
            let parameters = johnson
                .parameters()
                .iter()
                .map(|(name, value)| format!("{name}={value:.6}"))
                .collect::<Vec<String>>()
                .join(", ");
            add_row(
                johnson.name(),
                parameters,
                a2,
                ad_p_value(sample.n(), a2)?,
                johnson.log_likelihood(&sample)?,
                johnson.n_params(),
            );
        }
        Err(error) => notes.push(("Johnson fit".to_owned(), format!("skipped: {error}"))),
    }

    match Logistic::fit_mle(&sample) {
        Ok(logistic) => {
            let a2 = anderson_darling(&logistic, &sample)?;
            add_row(
                "Logistic",
                format!("µ={:.6}, σ={:.6}", logistic.mu(), logistic.sigma()),
                a2,
                ad_p_value(sample.n(), a2)?,
                log_likelihood(&logistic, &sample),
                logistic.n_params(),
            );
        }
        Err(error) => notes.push(("Logistic fit".to_owned(), format!("skipped: {error}"))),
    }

    match ExtremeValue::fit_mle(&sample) {
        Ok(extreme_value) => {
            let a2 = anderson_darling(&extreme_value, &sample)?;
            add_row(
                "Extreme Value",
                format!(
                    "µ={:.6}, σ={:.6}",
                    extreme_value.location(),
                    extreme_value.scale()
                ),
                a2,
                ad_p_value(sample.n(), a2)?,
                log_likelihood(&extreme_value, &sample),
                extreme_value.n_params(),
            );
        }
        Err(error) => notes.push(("Extreme Value fit".to_owned(), format!("skipped: {error}"))),
    }

    if positive {
        let lognormal = Lognormal::fit_mle(&sample)?;
        let a2 = anderson_darling(&lognormal, &sample)?;
        add_row(
            "Lognormal",
            format!("µ={:.6}, σ={:.6} (ln)", lognormal.mu(), lognormal.sigma()),
            a2,
            ad_p_value(sample.n(), a2)?,
            log_likelihood(&lognormal, &sample),
            lognormal.n_params(),
        );

        let loglogistic = Loglogistic::fit_mle(&sample)?;
        let a2 = anderson_darling(&loglogistic, &sample)?;
        add_row(
            "Loglogistic",
            format!("α={:.6}, β={:.6}", loglogistic.scale(), loglogistic.shape()),
            a2,
            ad_p_value(sample.n(), a2)?,
            log_likelihood(&loglogistic, &sample),
            loglogistic.n_params(),
        );

        match Gamma::fit_mle(&sample) {
            Ok(gamma) => {
                let a2 = anderson_darling(&gamma, &sample)?;
                add_row(
                    "Gamma",
                    format!("α={:.6}, β={:.6}", gamma.shape(), gamma.scale()),
                    a2,
                    ad_p_value(sample.n(), a2)?,
                    log_likelihood(&gamma, &sample),
                    gamma.n_params(),
                );
            }
            Err(error) => notes.push(("Gamma fit".to_owned(), format!("skipped: {error}"))),
        }

        match Weibull::fit_mle(&sample) {
            Ok(weibull) => {
                let a2 = anderson_darling(&weibull, &sample)?;
                add_row(
                    "Weibull",
                    format!("α={:.6}, β={:.6}", weibull.shape(), weibull.scale()),
                    a2,
                    ad_p_value_weibull(sample.n(), a2)?,
                    log_likelihood(&weibull, &sample),
                    weibull.n_params(),
                );
            }
            Err(error) => notes.push(("Weibull fit".to_owned(), format!("skipped: {error}"))),
        }

        let exponential = Exponential::fit_mle(&sample)?;
        let a2 = anderson_darling(&exponential, &sample)?;
        add_row(
            "Exponential",
            format!("θ={:.6}", exponential.theta()),
            a2,
            ad_p_value_exponential(sample.n(), a2)?,
            log_likelihood(&exponential, &sample),
            exponential.n_params(),
        );
    } else {
        notes.push((
            "Positive-data families".to_owned(),
            "Lognormal, Loglogistic, Weibull, Exponential and Gamma skipped: the column has x ≤ 0"
                .to_owned(),
        ));
    }

    let mut result = AnalysisResult::new(
        format!("Distribution Comparison — column {column_name}"),
        "Maximum likelihood",
    );
    result.n = sample.n();
    result.excluded = sample.excluded();
    result.extra.push(Item::Message(
        "Fitted distributions on the same sample; AIC/BIC are comparable only across these fits."
            .to_owned(),
    ));
    result.extra.push(Item::Table {
        headers: vec![
            "Distribution".to_owned(),
            "Parameters".to_owned(),
            "AD".to_owned(),
            "p".to_owned(),
            "AIC".to_owned(),
            "BIC".to_owned(),
        ],
        rows,
    });
    if let Some((name, accepted)) = best_distribution(&fits) {
        if accepted {
            result.extra.push(Item::Message(format!(
                "Best fit: {name} (lowest AIC among the distributions not rejected at 5%)"
            )));
        } else {
            result.extra.push(Item::Message(format!(
                "Best fit: {name} (lowest Anderson–Darling statistic; no distribution is accepted at 5%)"
            )));
            result
                .warnings
                .push("no candidate distribution is accepted at the 5% level".to_owned());
        }
        notes.push((
            "Selection rule".to_owned(),
            "among the distributions whose Anderson–Darling p-value is at least 0.05, the one with \
             the lowest AIC; when none is accepted, the lowest Anderson–Darling statistic"
                .to_owned(),
        ));
    }
    notes.push((
        "Estimator".to_owned(),
        "MLE for comparison; the probability plot uses n−1 for Normal/Lognormal".to_owned(),
    ));
    notes.push((
        "AD p-values".to_owned(),
        "Normal/Lognormal use the composite normal formula; Logistic/Loglogistic/Gamma/Extreme \
         Value use the same approximation; Weibull and Exponential use their recovered formulas"
            .to_owned(),
    ));
    notes.push((
        "Scope".to_owned(),
        "Normal, Box-Cox Normal, Lognormal, Weibull, Exponential, Logistic, Loglogistic, Gamma, \
         Extreme Value and Johnson SU/SB/SL (best AIC)"
            .to_owned(),
    ));
    result.notes = notes;
    result.warnings = standard_warnings(sample.n(), sample.excluded(), sample.ties());
    Ok(result)
}

fn log_likelihood<D: Distribution>(dist: &D, sample: &crate::data::Sample) -> f64 {
    sample.values().iter().map(|&x| dist.ln_pdf(x)).sum()
}

/// The skewness and the excess kurtosis of a sample.
///
/// The central moments use the divisor `n`; the excess kurtosis is the fourth
/// central moment over the squared variance, minus three.
fn sample_shape(sample: &Sample) -> Result<(f64, f64)> {
    let values = sample.values();
    let n = values.len();
    if n < 4 {
        return Err(Error::SampleTooSmall { n, min: 4 });
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let mut m2 = 0.0;
    let mut m3 = 0.0;
    let mut m4 = 0.0;
    for value in values {
        let deviation = value - mean;
        let squared = deviation * deviation;
        m2 += squared;
        m3 += squared * deviation;
        m4 += squared * squared;
    }
    let nf = n as f64;
    let m2 = m2 / nf;
    let m3 = m3 / nf;
    let m4 = m4 / nf;
    if m2 <= 0.0 {
        return Err(Error::ConstantSample);
    }
    Ok((m3 / m2.powf(1.5), m4 / (m2 * m2) - 3.0))
}

/// Anderson–Darling normality test with the documented behaviour's defaults.
pub fn normality_test(
    df: &DataFrame,
    column_name: &str,
    test: NormalityTestKind,
    percentile_lines: bool,
) -> Result<AnalysisResult> {
    if percentile_lines {
        return Err(Error::Unsupported(
            "percentile lines in the Normality Test".to_owned(),
        ));
    }

    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let dist = Normal::fit_moments(&sample)?;
    let desc = Descriptives::from_sample(&sample)?;
    match test {
        NormalityTestKind::AndersonDarling => {
            let a2 = anderson_darling(&dist, &sample)?;
            let p = ad_p_value(sample.n(), a2)?;
            let p_parts = (
                "p-value (Anderson–Darling)",
                format!("{p:.6}"),
                "D'Agostino–Stephens composite-AD approximation",
            );
            let notes = vec![
                (
                    "Estimator".to_owned(),
                    "mean and sample standard deviation (n−1); normality test call site inferred"
                        .to_owned(),
                ),
                (
                    "Data policy".to_owned(),
                    "non-finite values excluded and counted".to_owned(),
                ),
                (
                    "Percentile lines".to_owned(),
                    "off (the documented behaviour default)".to_owned(),
                ),
            ];
            Ok(build_result(
                format!("Normality Test: Anderson–Darling — column {column_name}"),
                "Sample moments (n−1)",
                dist.parameters(),
                &desc,
                &sample,
                a2,
                p_parts,
                notes,
            ))
        }
        NormalityTestKind::KolmogorovSmirnov => {
            let statistic = kolmogorov_smirnov_statistic(&sample, dist.mu(), dist.sigma())?;
            let reps = ks_replications(sample.n());
            let (p, reps) = kolmogorov_smirnov_p_value(&sample, statistic, reps, 2026)?;
            let mut result = AnalysisResult::new(
                format!("Normality Test: Kolmogorov–Smirnov — column {column_name}"),
                "Sample moments (n−1); Monte Carlo Lilliefors p-value",
            );
            result.n = sample.n();
            result.excluded = sample.excluded();
            result.parameters = dist.parameters();
            result.statistics = vec![
                ("Median", desc.median),
                ("Minimum", desc.min),
                ("Maximum", desc.max),
            ];
            result.gof = vec![
                ("Kolmogorov–Smirnov (D)", format!("{statistic:.6}")),
                (
                    "p-value (Monte Carlo KS)",
                    format!("{p:.6}  [{reps} replications, seed 2026]"),
                ),
            ];
            result.notes = vec![
                (
                    "Estimator".to_owned(),
                    "mean and sample standard deviation (n−1); parameters re-estimated per replicate"
                        .to_owned(),
                ),
                (
                    "Data policy".to_owned(),
                    "non-finite values excluded and counted".to_owned(),
                ),
                (
                    "Percentile lines".to_owned(),
                    "off (the documented behaviour default)".to_owned(),
                ),
                (
                    "Replications".to_owned(),
                    if reps < 1000 {
                        "reduced for a large sample; the p-value is still a Monte Carlo \
                         Lilliefors estimate"
                            .to_owned()
                    } else {
                        "1000 replications, the documented-behaviour value".to_owned()
                    },
                ),
            ];
            result.warnings = standard_warnings(sample.n(), sample.excluded(), sample.ties());
            Ok(result)
        }
        NormalityTestKind::DAgostinoK2 => {
            let (skewness, kurtosis) = sample_shape(&sample)?;
            let n = sample.n() as f64;
            let z_skewness = skewness / (6.0 / n).sqrt();
            let z_kurtosis = kurtosis / (24.0 / n).sqrt();
            let statistic = z_skewness * z_skewness + z_kurtosis * z_kurtosis / 2.0;
            let p = (1.0 - crate::special::chi_square_cdf(statistic, 2.0)).clamp(0.0, 1.0);
            let mut result = AnalysisResult::new(
                format!("Normality Test: D'Agostino–Pearson — column {column_name}"),
                "Omnibus test of the skewness and the kurtosis; chi-square with 2 df",
            );
            result.n = sample.n();
            result.excluded = sample.excluded();
            result.parameters = dist.parameters();
            result.statistics = vec![("Skewness", skewness), ("Excess kurtosis", kurtosis)];
            result.gof = vec![
                ("K² (omnibus)", format!("{statistic:.6}")),
                ("p-value (chi-square, 2 df)", format!("{p:.6}")),
            ];
            result.notes = vec![
                (
                    "Statistic".to_owned(),
                    "K² = z₁² + z₂²/2 with z₁ = g₁/√(6/n) and z₂ = g₂/√(24/n), where g₁ is the \
                     skewness and g₂ the excess kurtosis"
                        .to_owned(),
                ),
                (
                    "Data policy".to_owned(),
                    "non-finite values excluded and counted".to_owned(),
                ),
                (
                    "Percentile lines".to_owned(),
                    "off (the documented behaviour default)".to_owned(),
                ),
            ];
            result.warnings = standard_warnings(sample.n(), sample.excluded(), sample.ties());
            Ok(result)
        }
        NormalityTestKind::RyanJoiner => {
            let scores = ryan_joiner_scores(sample.n())?;
            let statistic = ryan_joiner_statistic(&sample.sorted(), &scores)?;
            let (p, reps) = ryan_joiner_p_value(sample.n(), statistic, 1000, 2026)?;
            let mut result = AnalysisResult::new(
                format!("Normality Test: Ryan–Joiner — column {column_name}"),
                "Normal-score correlation; Monte Carlo p-value",
            );
            result.n = sample.n();
            result.excluded = sample.excluded();
            result.parameters = dist.parameters();
            result.statistics = vec![
                ("Median", desc.median),
                ("Minimum", desc.min),
                ("Maximum", desc.max),
            ];
            result.gof = vec![
                ("Ryan–Joiner (R)", format!("{statistic:.6}")),
                (
                    "p-value (Monte Carlo RJ)",
                    format!("{p:.6}  [{reps} replications, seed 2026]"),
                ),
            ];
            result.notes = vec![
                (
                    "Scores".to_owned(),
                    "normal scores at (i − 0.375)/(n + 0.25)".to_owned(),
                ),
                (
                    "p-value".to_owned(),
                    "Monte Carlo null distribution of R; 1000 replicates, seed 2026".to_owned(),
                ),
                (
                    "Data policy".to_owned(),
                    "non-finite values excluded and counted".to_owned(),
                ),
            ];
            result.warnings = standard_warnings(sample.n(), sample.excluded(), sample.ties());
            Ok(result)
        }
    }
}

fn ryan_joiner_scores(n: usize) -> Result<Vec<f64>> {
    if n < 3 {
        return Err(Error::SampleTooSmall { n, min: 3 });
    }
    Ok((1..=n)
        .map(|rank| normal_quantile((rank as f64 - 0.375) / (n as f64 + 0.25)))
        .collect())
}

fn ryan_joiner_statistic(sorted: &[f64], scores: &[f64]) -> Result<f64> {
    if sorted.len() != scores.len() || sorted.len() < 3 {
        return Err(Error::SampleTooSmall {
            n: sorted.len(),
            min: 3,
        });
    }
    let n = sorted.len() as f64;
    let mean_x = sorted.iter().sum::<f64>() / n;
    let mean_z = scores.iter().sum::<f64>() / n;
    let mut covariance = 0.0;
    let mut var_x = 0.0;
    let mut var_z = 0.0;
    for (x, z) in sorted.iter().zip(scores) {
        covariance += (x - mean_x) * (z - mean_z);
        var_x += (x - mean_x).powi(2);
        var_z += (z - mean_z).powi(2);
    }
    if var_x <= 0.0 || var_z <= 0.0 {
        return Err(Error::ConstantSample);
    }
    Ok((covariance / (var_x * var_z).sqrt()).clamp(-1.0, 1.0))
}

fn ryan_joiner_p_value(n: usize, observed: f64, reps: usize, seed: u64) -> Result<(f64, usize)> {
    if n < 3 || reps == 0 {
        return Err(Error::SampleTooSmall { n, min: 3 });
    }
    let scores = ryan_joiner_scores(n)?;
    let mut rng = SplitMix64::new(seed);
    let mut exceed = 0usize;
    for _ in 0..reps {
        let mut values: Vec<f64> = (0..n).map(|_| rng.next_normal()).collect();
        values.sort_by(f64::total_cmp);
        let statistic = ryan_joiner_statistic(&values, &scores)?;
        if statistic <= observed {
            exceed += 1;
        }
    }
    Ok(((exceed + 1) as f64 / (reps + 1) as f64, reps))
}

fn kolmogorov_smirnov_statistic(sample: &Sample, mean: f64, sd: f64) -> Result<f64> {
    if sample.n() == 0 || !sd.is_finite() || sd <= 0.0 {
        return Err(Error::ConstantSample);
    }
    let n = sample.n() as f64;
    let mut statistic: f64 = 0.0;
    for (index, value) in sample.sorted().iter().enumerate() {
        let cdf = normal_cdf((value - mean) / sd);
        let rank = (index + 1) as f64;
        statistic = statistic
            .max((rank / n - cdf).abs())
            .max((cdf - (rank - 1.0) / n).abs());
    }
    Ok(statistic)
}

fn kolmogorov_smirnov_p_value(
    sample: &Sample,
    observed: f64,
    reps: usize,
    seed: u64,
) -> Result<(f64, usize)> {
    let n = sample.n();
    if n < 2 || reps == 0 {
        return Err(Error::SampleTooSmall { n, min: 2 });
    }
    let mut rng = SplitMix64::new(seed);
    let mut exceed = 0usize;
    for _ in 0..reps {
        let mut values = Vec::with_capacity(n);
        for _ in 0..n {
            values.push(rng.next_normal());
        }
        let generated = Sample::from_valid(values, 0)?;
        let mean = generated.mean()?;
        let sd = generated.sd()?;
        let statistic = kolmogorov_smirnov_statistic(&generated, mean, sd)?;
        if statistic >= observed {
            exceed += 1;
        }
    }
    Ok(((exceed + 1) as f64 / (reps + 1) as f64, reps))
}

/// Descriptive statistics for one numeric column.
pub fn descriptive_statistics(df: &DataFrame, column_name: &str) -> Result<AnalysisResult> {
    let column = df.column(column_name)?;
    let sample = column.sample(MissingPolicy::Drop)?;
    let summary = crate::data::ColumnSummary::from_sample(&sample, 0.95)?;

    let mut result = AnalysisResult::new(
        format!("Descriptive Statistics — column {column_name}"),
        "Mean, n−1 standard deviation, interpolated quartiles (h = (n+1)p), 95% mean and median intervals",
    );
    result.n = summary.n;
    result.excluded = summary.excluded;
    result.statistics = vec![
        ("Mean", summary.mean),
        ("Minimum", summary.min),
        ("First quartile (Q1)", summary.q1),
        ("Median", summary.median),
        ("Third quartile (Q3)", summary.q3),
        ("Maximum", summary.max),
        ("Interquartile range", summary.iqr),
    ];
    if let Some(sd) = summary.sd {
        result.statistics.push(("Standard deviation (n−1)", sd));
    }
    if let Some(se) = summary.se_mean {
        result.statistics.push(("Standard error of the mean", se));
    }
    if let Some((low, high)) = summary.ci_mean {
        result.statistics.push(("95% CI for the mean (lower)", low));
        result
            .statistics
            .push(("95% CI for the mean (upper)", high));
    }
    if let Some((low, high)) = summary.ci_median {
        result
            .statistics
            .push(("95% CI for the median (lower)", low));
        result
            .statistics
            .push(("95% CI for the median (upper)", high));
    }
    if let Some((low, high)) = summary.ci_sd {
        result
            .statistics
            .push(("95% CI for the standard deviation (lower)", low));
        result
            .statistics
            .push(("95% CI for the standard deviation (upper)", high));
    }

    result.notes = vec![
        (
            "Data policy".to_owned(),
            "empty cells and non-finite values excluded and counted".to_owned(),
        ),
        (
            "Quartiles".to_owned(),
            "linear interpolation between order statistics with h = (n + 1)p, the \
             documented convention (n = 20: Q1 19.4198, Q3 21.7969)"
                .to_owned(),
        ),
        (
            "Mean interval".to_owned(),
            "two-sided Student t with n−1 degrees of freedom".to_owned(),
        ),
        (
            "Median interval".to_owned(),
            "distribution-free order-statistic interval; its coverage is at least the nominal \
             level and is not exactly 95 % for every sample size"
                .to_owned(),
        ),
        (
            "Standard-deviation interval".to_owned(),
            "chi-square interval for the variance, square-rooted; it assumes a Normal sample"
                .to_owned(),
        ),
    ];
    if summary.excluded > 0 {
        result.warnings.push(format!(
            "{} empty/non-finite observations were excluded",
            summary.excluded
        ));
    }
    if summary.n == 1 {
        result.warnings.push(
            "n = 1: standard deviation, standard error and the mean interval are not defined"
                .to_owned(),
        );
    }
    Ok(result)
}

/// Descriptive statistics for one column, one block per level of a By column.
///
/// Rows with a missing group label or a missing value are excluded and counted
/// in the block where they belong.
pub fn descriptive_statistics_by(
    df: &DataFrame,
    column_name: &str,
    by: &str,
) -> Result<Vec<AnalysisResult>> {
    let column = df.column(column_name)?;
    let group = df.column(by)?;
    if !matches!(column.dtype(), DType::Float | DType::Integer) {
        return Err(Error::Unsupported(format!(
            "column {column_name} is not numeric"
        )));
    }
    let mut groups: BTreeMap<String, (Vec<f64>, usize)> = BTreeMap::new();
    let mut missing_labels = 0usize;
    for row in 0..df.n_rows() {
        if group.is_missing(row) {
            missing_labels += 1;
            continue;
        }
        let level = group.cell_text(row);
        let entry = groups.entry(level).or_insert_with(|| (Vec::new(), 0));
        if column.is_missing(row) {
            entry.1 += 1;
            continue;
        }
        match column.data() {
            ColumnData::F64(values) => entry.0.push(values[row]),
            ColumnData::I64(values) => entry.0.push(values[row] as f64),
            _ => {
                return Err(Error::Unsupported(format!(
                    "column {column_name} is not numeric"
                )));
            }
        }
    }
    let mut results = Vec::with_capacity(groups.len());
    for (level, (values, excluded)) in groups {
        if values.is_empty() {
            continue;
        }
        let temporary = DataFrame::new(vec![Column::new_f64(column_name, values)])?;
        let mut result = descriptive_statistics(&temporary, column_name)?;
        result.title = format!("Descriptive Statistics — column {column_name}, {by} = {level}");
        result.excluded += excluded;
        result
            .notes
            .push(("Group".to_owned(), format!("{by} = {level}")));
        if missing_labels > 0 {
            result.warnings.push(format!(
                "{missing_labels} row(s) with a missing group label were excluded"
            ));
        }
        if result.excluded > 0 {
            result.warnings.push(format!(
                "{} empty/non-finite observation(s) were excluded",
                result.excluded
            ));
        }
        results.push(result);
    }
    if results.is_empty() {
        return Err(Error::EmptySample);
    }
    Ok(results)
}

/// Runs a typed analysis request.
pub fn run_analysis(df: &DataFrame, spec: &AnalysisSpec) -> Result<AnalysisOutput> {
    let mut output = analyze_spec(df, spec)?;
    // Every graph carries the key numbers of its report beside the frame,
    // unless the analysis already provided a tailored block.
    if let Some(plot) = &mut output.plot {
        if plot.panels.is_empty() {
            if plot.notes.is_empty()
                && let Some(result) = output.results.first()
            {
                plot.notes.push(generic_report_note(result));
            }
        } else if let Some(result) = output.results.first() {
            let note = generic_report_note(result);
            for panel in &mut plot.panels {
                if panel.notes.is_empty() {
                    panel.notes.push(note.clone());
                }
            }
        }
    }
    Ok(output)
}

/// The default statistics block: N, the estimated parameters and the report
/// statistics, at four decimals and without the parenthetical units.
fn generic_report_note(result: &AnalysisResult) -> PlotNote {
    let short = |label: &str| -> String {
        let label = label.split(" (").next().unwrap_or(label);
        label
            .replace("Standard deviation", "StDev")
            .replace("Anderson–Darling", "AD")
    };
    let mut rows: Vec<(String, String)> = vec![("N".to_owned(), result.n.to_string())];
    for (label, value) in &result.parameters {
        rows.push((short(label), format!("{value:.4}")));
    }
    for (label, value) in &result.statistics {
        rows.push((short(label), format!("{value:.4}")));
    }
    rows.truncate(14);
    PlotNote::new(rows)
}

fn analyze_spec(df: &DataFrame, spec: &AnalysisSpec) -> Result<AnalysisOutput> {
    match spec {
        AnalysisSpec::ProbabilityPlot {
            columns,
            group,
            distribution,
            plotting_position,
            p_value,
            confidence,
            display,
            layout,
            same_x,
            same_y,
        } => {
            if columns.is_empty() {
                return Err(Error::Unsupported("no Y column selected".to_owned()));
            }
            let options = ProbabilityPlotOptions {
                method: *plotting_position,
                p_value: *p_value,
                confidence: *confidence,
                display: *display,
            };
            if let Some(group) = group {
                if columns.len() != 1 {
                    return Err(Error::Unsupported(
                        "grouping is only available for a single Y column".to_owned(),
                    ));
                }
                return grouped_probability(
                    df,
                    &columns[0],
                    group,
                    *distribution,
                    options,
                    &spec.description(),
                    *layout,
                    *same_x,
                    *same_y,
                );
            }

            let mut analyses = Vec::with_capacity(columns.len());
            for column in columns {
                let out = probability_one(df, column, *distribution, options)?;
                analyses.push((column.clone(), out));
            }

            if analyses.len() == 1 {
                let (_, mut out) = analyses.remove(0);
                out.result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
                return Ok(AnalysisOutput {
                    results: vec![out.result],
                    plot: Some(out.plot),
                });
            }

            let mut results = Vec::with_capacity(analyses.len());
            let mut plots = Vec::with_capacity(analyses.len());
            for (column, mut out) in analyses {
                out.result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
                results.push(out.result);
                plots.push((column, out.plot));
            }
            let labels: Vec<String> = plots.iter().map(|(label, _)| label.clone()).collect();
            let mut plot = match layout {
                PlotLayout::Separate => panel_plot_with_scales(
                    plots.into_iter().map(|(_, plot)| plot).collect(),
                    2,
                    format!(
                        "Probability Plot: {} — separate panels",
                        distribution.name()
                    ),
                    *same_x,
                    *same_y,
                ),
                PlotLayout::Overlay => overlay_plots(
                    plots,
                    format!("Probability Plot: {} — overlay", distribution.name()),
                ),
            }
            .ok_or_else(|| Error::Unsupported("invalid multi-Y layout".to_owned()))?;
            if *layout == PlotLayout::Overlay {
                plot.notes.push(overlay_results_note(&labels, &results));
            }
            Ok(AnalysisOutput {
                results,
                plot: Some(plot),
            })
        }
        AnalysisSpec::ControlChart {
            column,
            kind,
            subgroup,
            tests,
            cusum,
            ewma,
            subgroup_column,
            phase_column,
        } => {
            if phase_column.is_some() && subgroup_column.is_some() {
                return Err(Error::Unsupported(
                    "choose either a phase column or a subgroup column, not both".to_owned(),
                ));
            }
            let mut out = if let Some(phase) = phase_column {
                crate::spc::analyze_with_phase(df, column, *kind, *subgroup, *tests, phase)?
            } else if let Some(group) = subgroup_column {
                crate::spc::analyze_variable_subgroups(df, column, group, *kind, *tests)?
            } else {
                crate::spc::analyze_with_options(
                    df, column, *kind, *subgroup, *tests, *cusum, *ewma,
                )?
            };
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::RunChart { column } => {
            let mut out = crate::runchart::analyze(df, column)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::AttributeChart {
            kind,
            defects,
            sizes,
            size,
        } => {
            let mut out = crate::attribute::analyze(df, *kind, defects, sizes.as_deref(), *size)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::OneSampleT {
            column,
            null_mean,
            alternative,
            confidence,
        } => {
            let mut result =
                crate::hypothesis::one_sample(df, column, *null_mean, *alternative, *confidence)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::TwoSampleT {
            first,
            second,
            pooled,
            alternative,
            confidence,
        } => {
            let mut result = crate::hypothesis::two_sample(
                df,
                first,
                second,
                *pooled,
                *alternative,
                *confidence,
            )?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::PairedT {
            first,
            second,
            alternative,
            confidence,
        } => {
            let mut result =
                crate::hypothesis::paired(df, first, second, *alternative, *confidence)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::OneSampleEquivalence {
            column,
            null_mean,
            margin,
            confidence,
        } => {
            let mut result =
                crate::equivalence::one_sample(df, column, *null_mean, *margin, *confidence)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::TwoSampleEquivalence {
            first,
            second,
            margin,
            confidence,
        } => {
            let mut result =
                crate::equivalence::two_sample(df, first, second, *margin, *confidence)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::PairedEquivalence {
            first,
            second,
            margin,
            confidence,
        } => {
            let mut result = crate::equivalence::paired(df, first, second, *margin, *confidence)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::MannWhitney {
            first,
            second,
            alternative,
        } => {
            let mut result =
                crate::nonparametric::mann_whitney_columns(df, first, second, *alternative)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::WilcoxonSignedRank {
            first,
            second,
            alternative,
        } => {
            let mut result = crate::nonparametric::wilcoxon_signed_rank_columns(
                df,
                first,
                second,
                *alternative,
            )?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::KruskalWallis { response, factor } => {
            let mut result = crate::nonparametric::kruskal_wallis_columns(df, response, factor)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::Friedman { columns } => {
            let mut result = crate::nonparametric::friedman_columns(df, columns)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::MoodsMedian { response, factor } => {
            let mut result = crate::nonparametric::moods_median_columns(df, response, factor)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::PartialCorrelation {
            x_column,
            y_column,
            control,
        } => {
            let mut result =
                crate::correlation::partial_correlation(df, x_column, y_column, control)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::Correlation { columns } => {
            let mut result = crate::correlation::correlate(df, columns)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::RareEvents { counts, times } => {
            let out = crate::rare::analyze(df, counts, times.as_deref())?;
            let mut results = out.results;
            if let Some(result) = results.first_mut() {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(AnalysisOutput {
                results,
                plot: out.plot,
            })
        }
        AnalysisSpec::ToleranceInterval {
            column,
            proportion,
            confidence,
            free,
        } => {
            let mut result = if *free {
                crate::tolerance::analyze_distribution_free(df, column, *proportion, *confidence)?
            } else {
                crate::tolerance::analyze(df, column, *proportion, *confidence)?
            };
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::BoxCox { column } => {
            let out = crate::boxcox::analyze(df, column)?;
            let mut results = out.results;
            if let Some(result) = results.first_mut() {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(AnalysisOutput {
                results,
                plot: out.plot,
            })
        }
        AnalysisSpec::ChiSquareIndependence { row, column } => {
            let model = crate::contingency::test(df, row, column)?;
            let mut result = crate::contingency::report(row, column, &model);
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::TwoWayAnova {
            response,
            first,
            second,
            interaction,
        } => {
            let model = crate::twoway::fit_with(df, response, first, second, *interaction)?;
            let mut result = crate::twoway::report(response, first, second, &model);
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            let plot = crate::twoway::residual_plot(response, &model);
            Ok(AnalysisOutput {
                results: vec![result],
                plot: Some(plot),
            })
        }
        AnalysisSpec::CurveFit {
            x_column,
            y_column,
            model,
            confidence,
        } => {
            let fit = crate::curvefit::fit(df, x_column, y_column, *model, *confidence)?;
            let mut result = crate::curvefit::report(x_column, y_column, &fit);
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            let plot = crate::curvefit::plot(x_column, &fit);
            Ok(AnalysisOutput {
                results: vec![result],
                plot: Some(plot),
            })
        }
        AnalysisSpec::Levene {
            response,
            factor,
            center,
            confidence,
        } => {
            let mut result = crate::variance::levene(df, response, factor, *center, *confidence)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::Bartlett { response, factor } => {
            let mut result = crate::variance::bartlett(df, response, factor)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::ChiSquareGoodnessOfFit {
            column,
            expected,
            estimated_parameters,
        } => {
            let mut result = crate::gof::chi_square_goodness_of_fit_report(
                df,
                column,
                expected.clone(),
                *estimated_parameters,
            )?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::Grubbs { column, alpha } => {
            let mut result = crate::gof::grubbs_report(df, column, *alpha)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::Power {
            procedure,
            effect,
            null_value,
            n,
            groups,
            alpha,
            power,
            alternative,
        } => {
            let mut result = crate::power::power_report(
                *procedure,
                *effect,
                *null_value,
                *n,
                *groups,
                *alpha,
                *power,
                *alternative,
            )?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::OneProportion {
            column,
            success,
            null_p,
            method,
            alternative,
            confidence,
        } => {
            let mut result = crate::proportion::one_sample(
                df,
                column,
                *success,
                *null_p,
                *method,
                *alternative,
                *confidence,
            )?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::TwoProportions {
            first,
            second,
            success,
            alternative,
            confidence,
        } => {
            let mut result = crate::proportion::two_sample(
                df,
                first,
                second,
                *success,
                *alternative,
                *confidence,
            )?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::OneVariance {
            column,
            null_variance,
            alternative,
            confidence,
        } => {
            let mut result =
                crate::variance::one_sample(df, column, *null_variance, *alternative, *confidence)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::TwoVariances {
            first,
            second,
            alternative,
            confidence,
            method,
        } => {
            let mut result = match method {
                crate::variance::VarianceMethod::F => {
                    crate::variance::two_sample(df, first, second, *alternative, *confidence)?
                }
                crate::variance::VarianceMethod::Bonett => {
                    crate::variance::bonett_analysis(df, first, second, *alternative, *confidence)?
                }
            };
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::Regression {
            response,
            predictors,
            confidence,
        } => {
            let mut out = crate::regression::analyze(df, response, predictors, *confidence)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::StepwiseRegression {
            response,
            candidates,
            method,
            criterion,
            alpha_enter,
            alpha_remove,
            max_steps,
            confidence,
        } => {
            let options = crate::stepwise::StepwiseOptions {
                method: *method,
                criterion: *criterion,
                alpha_enter: *alpha_enter,
                alpha_remove: *alpha_remove,
                max_steps: *max_steps,
                confidence: *confidence,
            };
            let mut out = crate::stepwise::analyze(df, response, candidates, options)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::Anova {
            response,
            factor,
            confidence,
        } => crate::anova::analyze(df, response, factor, *confidence),
        AnalysisSpec::KaplanMeier {
            time,
            censor,
            censor_value,
            confidence,
        } => {
            let mut out = crate::reliability::analyze(
                df,
                time,
                censor.as_deref(),
                *censor_value,
                *confidence,
            )?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::CensoredLognormal {
            time,
            censor,
            censor_value,
        } => {
            let mut out =
                crate::reliability::analyze_lognormal(df, time, censor.as_deref(), *censor_value)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::CensoredExponential {
            time,
            censor,
            censor_value,
        } => {
            let mut out = crate::reliability::analyze_exponential(
                df,
                time,
                censor.as_deref(),
                *censor_value,
            )?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::CensoredWeibull {
            time,
            censor,
            censor_value,
        } => {
            let mut out =
                crate::reliability::analyze_weibull(df, time, censor.as_deref(), *censor_value)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::Agreement {
            part,
            appraiser,
            rating,
        } => crate::agreement::analyze(df, part, appraiser, rating),
        AnalysisSpec::KMeans {
            columns,
            k,
            standardise,
            seed,
        } => crate::cluster::analyze(df, columns, *k, *standardise, *seed),
        AnalysisSpec::AcceleratedLife {
            time,
            stress,
            censor,
            censor_value,
            use_stress,
            life,
            acceleration,
            mode,
            horizon,
        } => {
            if let Some(mode) = mode {
                crate::alt::analyze_competing(
                    df,
                    time,
                    stress,
                    censor.as_deref(),
                    *censor_value,
                    mode,
                    *use_stress,
                    *life,
                    *acceleration,
                    *horizon,
                )
            } else {
                crate::alt::analyze_with(
                    df,
                    time,
                    stress,
                    censor.as_deref(),
                    *censor_value,
                    *use_stress,
                    *life,
                    *acceleration,
                )
            }
        }
        AnalysisSpec::StepStressAlt {
            duration,
            stress,
            unit,
            event,
            use_stress,
            life,
            acceleration,
        } => crate::alt::step_stress_analyze(
            df,
            duration,
            stress,
            unit,
            event,
            *use_stress,
            *life,
            *acceleration,
        ),
        AnalysisSpec::SignTest { column, median } => {
            crate::nonparametric::sign_test_columns(df, column, *median).map(|result| {
                AnalysisOutput {
                    results: vec![result],
                    plot: None,
                }
            })
        }
        AnalysisSpec::SeriesPlot { column, time } => {
            crate::seriesplot::analyze(df, column, time.as_deref())
        }
        AnalysisSpec::StemLeaf { column } => crate::stemleaf::analyze(df, column),
        AnalysisSpec::Ecdf { column, confidence } => crate::ecdf::analyze(df, column, *confidence),
        AnalysisSpec::Dotplot { column, factor } => {
            crate::dotplot::analyze(df, column, factor.as_deref())
        }
        AnalysisSpec::MatrixPlot { columns, bins } => {
            crate::matrixplot::analyze(df, columns, *bins)
        }
        AnalysisSpec::IntervalPlot {
            response,
            factor,
            confidence,
        } => crate::intervalplot::analyze(df, response, factor, *confidence),
        AnalysisSpec::Pareto { column, weight } => match weight {
            Some(weight) => crate::pareto::analyze_weighted(df, column, weight),
            None => crate::pareto::analyze(df, column),
        },
        AnalysisSpec::Taguchi {
            response,
            factors,
            quality,
        } => crate::taguchi::analyze(df, response, factors, *quality),
        AnalysisSpec::AcceptanceSampling {
            plan,
            lot_size,
            targets,
        } => match (plan, targets) {
            (Some(plan), _) => crate::sampling::analyze(*lot_size, plan.sample, plan.acceptance),
            (_, Some((aql, ltpd, alpha, beta))) => {
                crate::sampling::analyze_plan(*aql, *ltpd, *alpha, *beta, *lot_size)
            }
            _ => Err(Error::Unsupported(
                "the acceptance-sampling spec needs a plan or a set of targets".to_owned(),
            )),
        },
        AnalysisSpec::HotellingT2 {
            columns,
            subgroup,
            confidence,
        } => crate::mvchart::analyze(df, columns, *subgroup, *confidence),
        AnalysisSpec::Pca { columns, keep } => crate::pca::analyze(df, columns, *keep),
        AnalysisSpec::TimeSeries { column, method } => {
            crate::timeseries::analyze(df, column, *method)
        }
        AnalysisSpec::Ordinal {
            response,
            predictors,
        } => crate::ordinal::analyze(df, response, predictors),
        AnalysisSpec::Poisson {
            response,
            predictors,
        } => crate::poisson::analyze(df, response, predictors),
        AnalysisSpec::Logistic {
            response,
            predictors,
        } => crate::logistic::analyze(df, response, predictors),
        AnalysisSpec::GageLinearity {
            measurement,
            reference,
        } => crate::linearity::analyze(df, measurement, reference),
        AnalysisSpec::GageType1 {
            measurement,
            reference,
            lsl,
            usl,
        } => {
            let mut out = crate::gage_type1::analyze(df, measurement, *reference, *lsl, *usl)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::GageRr {
            measurement,
            part,
            operator,
            design,
        } => {
            let mut out = crate::gage::analyze_design(df, measurement, part, operator, *design)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::Factorial {
            response,
            factors,
            main_effects_only,
            block,
        } => {
            let mut out = if *main_effects_only {
                crate::doe::analyze_main_effects(df, response, factors)?
            } else {
                crate::doe::analyze_with_blocks(df, response, factors, block.as_deref())?
            };
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::Desirability { factors, responses } => {
            let mut out = crate::desirability::analyze(df, factors, responses)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::ResponseSurface {
            response,
            factors,
            optimize,
            contours,
            steepest_ascent,
        } => {
            let mut out = crate::rsm::analyze(
                df,
                response,
                factors,
                *optimize,
                *contours,
                *steepest_ascent,
            )?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::Boxplot { column, by } => {
            let mut out = crate::boxplot::analyze(df, column, by.as_deref())?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::Scatterplot { x, y, fit } => {
            let mut out = crate::scatter::analyze(df, x, y, *fit)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::Histogram { column, bins } => {
            let mut out = crate::histogram::analyze(df, column, *bins)?;
            for result in &mut out.results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(out)
        }
        AnalysisSpec::Capability {
            column,
            lsl,
            usl,
            target,
            distribution,
            subgroup,
            subgroup_column,
            sixpack,
            unbiasing_within,
            unbiasing_overall,
            confidence,
            bootstrap_reps,
            box_cox,
            require_stability,
        } => {
            if *box_cox && *distribution != DistributionKind::Normal {
                return Err(Error::Unsupported(
                    "Box-Cox is only available for the normal capability model".to_owned(),
                ));
            }
            if subgroup_column.is_some() && *distribution != DistributionKind::Normal {
                return Err(Error::Unsupported(
                    "a subgroup column is only available for the normal capability model"
                        .to_owned(),
                ));
            }
            if *sixpack && *distribution != DistributionKind::Normal {
                return Err(Error::Unsupported(
                    "the capability Sixpack is only available for the normal model".to_owned(),
                ));
            }
            if *sixpack && *box_cox {
                return Err(Error::Unsupported(
                    "the capability Sixpack does not apply a Box-Cox transformation".to_owned(),
                ));
            }
            let mut result = if *distribution == DistributionKind::Normal {
                capability_normal(
                    df,
                    column,
                    *lsl,
                    *usl,
                    CapabilityOptions {
                        target: *target,
                        bootstrap_reps: *bootstrap_reps,
                        box_cox: *box_cox,
                        subgroup: *subgroup,
                        unbiasing_within: *unbiasing_within,
                        unbiasing_overall: *unbiasing_overall,
                        confidence: *confidence,
                        require_stability: *require_stability,
                    },
                    subgroup_column.as_deref(),
                )?
            } else {
                capability_non_normal(
                    df,
                    column,
                    *lsl,
                    *usl,
                    *distribution,
                    *confidence,
                    *bootstrap_reps,
                )?
            };
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            if *sixpack {
                result.title = format!("Capability Sixpack: Normal — column {column}");
                result.notes.push((
                    "Panels".to_owned(),
                    "histogram, normal probability plot, control chart, last 25 and capability"
                        .to_owned(),
                ));
            }
            let mut plot = if *sixpack {
                crate::sixpack::sixpack_plot(
                    df,
                    column,
                    *lsl,
                    *usl,
                    *subgroup,
                    subgroup_column.as_deref(),
                )?
            } else {
                crate::histogram::capability_plot(
                    df,
                    column,
                    *lsl,
                    *usl,
                    *distribution == DistributionKind::Normal && !*box_cox,
                )?
            };
            if !*sixpack {
                plot.notes.push(capability_note(&result));
            }
            Ok(AnalysisOutput {
                results: vec![result],
                plot: Some(plot),
            })
        }
        AnalysisSpec::DistributionComparison { column } => {
            let mut result = distribution_comparison(df, column)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::NormalityTest {
            column,
            test,
            percentile_lines,
        } => {
            let mut result = normality_test(df, column, *test, *percentile_lines)?;
            result
                .notes
                .push(("Analysis".to_owned(), spec.description()));
            Ok(AnalysisOutput {
                results: vec![result],
                plot: None,
            })
        }
        AnalysisSpec::DescriptiveStatistics { column, by } => {
            let mut results = match by {
                Some(by) => descriptive_statistics_by(df, column, by)?,
                None => vec![descriptive_statistics(df, column)?],
            };
            for result in &mut results {
                result
                    .notes
                    .push(("Analysis".to_owned(), spec.description()));
            }
            Ok(AnalysisOutput {
                results,
                plot: None,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("weight", vec![12.5, 13.1, 11.9, 12.8, 12.2]),
            Column::new_text("batch", vec!["A".into(); 5]),
        ])
        .expect("frame")
    }

    #[test]
    fn workflow_builds_report_and_plot() {
        let out = normal_probability(&frame(), "weight", ProbabilityPlotOptions::default())
            .expect("analysis");
        assert_eq!(out.result.n, 5);
        assert_eq!(out.result.parameters.len(), 2);
        assert!(out.result.to_session_text().contains("Anderson–Darling"));
        assert_eq!(out.plot.points.len(), 5);
        assert_eq!(out.plot.x_axis.label, "weight");
        assert_eq!(out.plot.y_axis.label, "Percent");
        let labels: Vec<&str> = out
            .plot
            .y_axis
            .ticks
            .iter()
            .map(|t| t.label.as_str())
            .collect();
        assert!(labels.contains(&"50"));
    }

    #[test]
    fn the_plots_carry_the_values_that_matter() {
        let df = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![12.5, 13.1, 11.9, 12.8, 12.2, 12.9, 12.4, 13.0, 12.1, 12.7],
        )])
        .expect("frame");

        // A probability plot shows n, the fit and the goodness of fit.
        let out = run_analysis(&df, &AnalysisSpec::probability_plot("weight")).expect("plot");
        let note = out.plot.expect("plot").notes.remove(0);
        let label = |name: &str| note.rows.iter().find(|row| row.label == name);
        assert_eq!(label("N").map(|row| row.value.as_str()), Some("10"));
        assert!(label("Mean").is_some(), "{:?}", note.rows);
        assert!(label("StDev").is_some(), "{:?}", note.rows);
        assert!(label("AD").is_some(), "{:?}", note.rows);
        assert!(label("P").is_some(), "{:?}", note.rows);

        // A capability histogram carries the indices and the spread.
        let out = run_analysis(
            &df,
            &AnalysisSpec::capability("weight", Some(12.0), Some(13.2)),
        )
        .expect("capability");
        let note = out.plot.expect("plot").notes.remove(0);
        let label = |name: &str| note.rows.iter().find(|row| row.label == name);
        for name in ["Cp", "Cpk", "Pp", "Ppk", "StDev"] {
            assert!(label(name).is_some(), "{name} missing from {:?}", note.rows);
        }
    }

    #[test]
    fn first_numeric_column_skips_text() {
        let df = frame();
        assert_eq!(first_numeric_column(&df), Some("weight"));
        let err = normal_probability(&df, "batch", ProbabilityPlotOptions::default());
        assert!(err.is_err());
    }

    #[test]
    fn integer_columns_are_numeric() {
        let df = DataFrame::new(vec![
            Column::from_parts(
                "count",
                crate::data::ColumnData::I64(vec![1, 2, 3, 4, 5]),
                vec![false; 5],
            )
            .expect("column"),
        ])
        .expect("frame");
        assert_eq!(first_numeric_column(&df), Some("count"));
        let out =
            normal_probability(&df, "count", ProbabilityPlotOptions::default()).expect("analysis");
        assert_eq!(out.result.n, 5);
    }

    #[test]
    fn probability_plot_uses_sample_moments() {
        let df = frame();
        let out =
            normal_probability(&df, "weight", ProbabilityPlotOptions::default()).expect("analysis");
        let sample = df
            .column("weight")
            .expect("column")
            .sample(MissingPolicy::Drop)
            .expect("sample");
        assert!((out.result.parameters[0].1 - sample.mean().unwrap()).abs() < 1e-12);
        assert!((out.result.parameters[1].1 - sample.sd().unwrap()).abs() < 1e-12);
        assert!(out.result.method.contains("Sample moments"));
    }

    #[test]
    fn lognormal_spec_uses_log_axis_and_log_scale_parameters() {
        let df = DataFrame::new(vec![Column::new_f64(
            "life",
            vec![1.0, 2.0, 4.0, 8.0, 16.0],
        )])
        .expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Lognormal);
        assert!(spec.description().contains("Lognormal"));
        let out = run_analysis(&df, &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("Lognormal"));
        assert!(result.method.contains("ln(x)"));
        assert_eq!(result.parameters[0].0, "Location (µ)");
        assert_eq!(result.parameters[1].0, "Scale (σ)");
        let plot = out.plot.as_ref().expect("plot");
        assert!(matches!(plot.x_axis.scale, crate::plot::Scale::Log10));
        assert!(!plot.bands.is_empty());
    }

    #[test]
    fn weibull_spec_uses_the_gumbel_axis_and_table_p_value() {
        let n = 40;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                10.0 * (-(1.0 - p).ln()).powf(0.5)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Weibull);
        let out = run_analysis(&df, &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("Weibull"));
        assert_eq!(result.parameters[0].0, "Shape (α)");
        assert_eq!(result.parameters[1].0, "Scale (β)");
        assert!(result.to_session_text().contains("Weibull AD table"));
        let plot = out.plot.as_ref().expect("plot");
        assert!(matches!(
            plot.y_axis.scale,
            crate::plot::Scale::Probability(crate::plot::ProbabilityScale::Gumbel)
        ));
        assert_eq!(
            plot.bands.len(),
            1,
            "the band comes from the MLE covariance"
        );
    }

    #[test]
    fn weibull_monte_carlo_is_rejected() {
        let df = DataFrame::new(vec![Column::new_f64("life", vec![1.0, 2.0, 3.0, 4.0, 5.0])])
            .expect("frame");
        let spec = AnalysisSpec::probability_plot("life")
            .with_distribution(DistributionKind::Weibull)
            .with_p_value(PValueMethod::MonteCarlo { reps: 10, seed: 1 });
        assert!(matches!(
            run_analysis(&df, &spec),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn exponential_spec_uses_the_four_branch_p_value() {
        let df = DataFrame::new(vec![Column::new_f64(
            "life",
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        )])
        .expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Exponential);
        let out = run_analysis(&df, &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("Exponential"));
        assert_eq!(result.parameters[0].0, "Mean (θ)");
        assert!(result.to_session_text().contains("exponential AD formula"));
        let plot = out.plot.as_ref().expect("plot");
        assert!(matches!(
            plot.y_axis.scale,
            crate::plot::Scale::Probability(crate::plot::ProbabilityScale::Gumbel)
        ));
        assert!(plot.bands.is_empty());
    }

    #[test]
    fn logistic_spec_uses_the_logit_axis() {
        let n = 60;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                2.0 + 0.7 * (p / (1.0 - p)).ln()
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("score", values)]).expect("frame");
        let spec =
            AnalysisSpec::probability_plot("score").with_distribution(DistributionKind::Logistic);
        assert!(spec.description().contains("Logistic"));
        let out = run_analysis(&df, &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("Logistic"));
        assert_eq!(result.parameters[0].0, "Location (µ)");
        assert_eq!(result.parameters[1].0, "Scale (σ)");
        assert!(result.to_session_text().contains("AD approximation"));
        let plot = out.plot.as_ref().expect("plot");
        assert!(matches!(
            plot.y_axis.scale,
            crate::plot::Scale::Probability(crate::plot::ProbabilityScale::Logit)
        ));
        assert_eq!(
            plot.bands.len(),
            1,
            "the band comes from the MLE covariance"
        );
        assert_eq!(plot.points.len(), n);
    }

    #[test]
    fn loglogistic_spec_uses_log_axis_and_gumbel_scale() {
        let n = 60;
        let values: Vec<f64> = (1..=n)
            .map(|i| {
                let p = (i as f64 - 0.5) / n as f64;
                10.0 * (p / (1.0 - p)).powf(0.5)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Loglogistic);
        assert!(spec.description().contains("Loglogistic"));
        let out = run_analysis(&df, &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("Loglogistic"));
        assert_eq!(result.parameters[0].0, "Scale (α)");
        assert_eq!(result.parameters[1].0, "Shape (β)");
        let plot = out.plot.as_ref().expect("plot");
        assert!(matches!(plot.x_axis.scale, crate::plot::Scale::Log10));
        assert!(matches!(
            plot.y_axis.scale,
            crate::plot::Scale::Probability(crate::plot::ProbabilityScale::Gumbel)
        ));
        assert_eq!(
            plot.bands.len(),
            1,
            "the band comes from the MLE covariance"
        );
    }

    #[test]
    fn loglogistic_rejects_non_positive_data() {
        let df = DataFrame::new(vec![Column::new_f64("life", vec![1.0, 0.0, 2.0])]).expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Loglogistic);
        assert!(matches!(
            run_analysis(&df, &spec),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn gamma_probability_plot_reports_shape_and_scale() {
        let truth = Gamma::new(2.0, 5.0).expect("gamma");
        let values: Vec<f64> = (1..=300)
            .map(|i| {
                let p = (i as f64 - 0.5) / 300.0;
                truth.quantile(p)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Gamma);
        let out = run_analysis(&df, &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("Gamma"), "{}", result.title);
        assert_eq!(result.parameters[0].0, "Shape (α)");
        assert_eq!(result.parameters[1].0, "Scale (β)");
        assert!((result.parameters[0].1 - 2.0).abs() < 0.05, "shape");
        assert!((result.parameters[1].1 - 5.0).abs() < 0.15, "scale");
        let plot = out.plot.as_ref().expect("plot");
        assert!(matches!(plot.x_axis.scale, crate::plot::Scale::Linear));
        assert!(matches!(
            plot.y_axis.scale,
            crate::plot::Scale::Probability(crate::plot::ProbabilityScale::Normal)
        ));
        assert_eq!(
            plot.bands.len(),
            1,
            "the band comes from the MLE covariance"
        );
        // The gamma CDF has no linearizing transform: the fit is a polyline.
        assert!(plot.lines.len() > 10, "lines {}", plot.lines.len());
        let text = out.results[0].to_session_text();
        assert!(text.contains("Probability Plot: Gamma"), "{text}");
    }

    #[test]
    fn extreme_value_probability_plot_reports_location_and_scale() {
        let truth = ExtremeValue::new(2.0, 1.5).unwrap();
        let values: Vec<f64> = (1..=300)
            .map(|i| {
                let p = (i as f64 - 0.5) / 300.0;
                truth.quantile(p)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
        let spec = AnalysisSpec::probability_plot("life")
            .with_distribution(DistributionKind::ExtremeValue);
        let out = run_analysis(&df, &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("Extreme Value"), "{}", result.title);
        assert_eq!(result.parameters[0].0, "Location (µ)");
        assert_eq!(result.parameters[1].0, "Scale (σ)");
        assert!((result.parameters[0].1 - 2.0).abs() < 0.05, "location");
        assert!((result.parameters[1].1 - 1.5).abs() < 0.05, "scale");
        let plot = out.plot.as_ref().expect("plot");
        assert!(matches!(
            plot.y_axis.scale,
            crate::plot::Scale::Probability(crate::plot::ProbabilityScale::ExtremeValue)
        ));
        assert_eq!(
            plot.bands.len(),
            1,
            "the band comes from the MLE covariance"
        );
    }

    #[test]
    fn diagnostic_and_power_specs_run_through_the_workflow() {
        let counts =
            DataFrame::new(vec![Column::new_f64("count", vec![20.0, 30.0, 50.0])]).expect("frame");
        let spec = AnalysisSpec::chi_square_goodness_of_fit("count", vec![25.0, 25.0, 50.0]);
        let out = run_analysis(&counts, &spec).expect("goodness of fit");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Chi-Square Goodness-of-Fit"), "{text}");
        assert!(text.contains("Chi-square"), "{text}");

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

        let spec = AnalysisSpec::grubbs("weight");
        let out = run_analysis(&df, &spec).expect("grubbs");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Grubbs Test"), "{text}");

        let spec = AnalysisSpec::levene("weight", "g");
        let out = run_analysis(&df, &spec).expect("levene");
        assert!(out.results[0].to_session_text().contains("Levene Test"));

        let spec = AnalysisSpec::bartlett("weight", "g");
        let out = run_analysis(&df, &spec).expect("bartlett");
        assert!(out.results[0].to_session_text().contains("Bartlett Test"));

        let spec = AnalysisSpec::power(crate::power::PowerProcedure::OneSampleT, 0.5, 64);
        let out = run_analysis(&df, &spec).expect("power");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Power and Sample Size"), "{text}");
        assert!(text.contains("Power"), "{text}");

        let spec = AnalysisSpec::power(crate::power::PowerProcedure::Proportion, 0.6, 0)
            .with_null_proportion(0.5)
            .with_power(0.8)
            .with_power_alpha(0.05);
        let out = run_analysis(&df, &spec).expect("proportion power");
        assert!(
            out.results[0]
                .to_session_text()
                .contains("observation(s) are needed")
        );
    }

    #[test]
    fn chi_square_independence_spec_runs_through_the_workflow() {
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
        let spec = AnalysisSpec::chi_square_independence("r", "c");
        let out = run_analysis(&df, &spec).expect("chi-square independence");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Chi-Square Test for Independence"), "{text}");
        assert!(text.contains("Cramer's V"), "{text}");
        assert!(out.plot.is_none());
    }

    #[test]
    fn two_way_anova_spec_runs_through_the_workflow() {
        let mut rows = Vec::new();
        for a in ["a1", "a2"] {
            for b in ["b1", "b2"] {
                for k in 0..3 {
                    let base = if a == "a1" { 10.0 } else { 13.0 };
                    let value = base + if b == "b2" { 1.0 } else { 0.0 } + k as f64 * 0.01;
                    rows.push((value, a.to_owned(), b.to_owned()));
                }
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("y", rows.iter().map(|(v, _, _)| *v).collect()),
            Column::new_text("a", rows.iter().map(|(_, a, _)| a.clone()).collect()),
            Column::new_text("b", rows.iter().map(|(_, _, b)| b.clone()).collect()),
        ])
        .expect("frame");
        // The default is main effects (the documented convention); the
        // interaction is requested explicitly.
        let spec = AnalysisSpec::two_way_anova("y", "a", "b");
        let out = run_analysis(&df, &spec).expect("two-way anova");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Two-Way ANOVA"), "{text}");
        assert!(!text.contains("Interaction"), "{text}");
        assert!(out.plot.is_some());
        let spec = spec.with_interaction(true);
        let out = run_analysis(&df, &spec).expect("two-way anova with interaction");
        assert!(out.results[0].to_session_text().contains("Interaction"));
    }

    #[test]
    fn curve_fit_spec_runs_through_the_workflow() {
        let x: Vec<f64> = (0..20).map(|i| i as f64 * 0.5).collect();
        let y: Vec<f64> = x.iter().map(|v| 1.0 + 2.0 * v + 0.5 * v * v).collect();
        let df =
            DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
        let spec = AnalysisSpec::curve_fit("x", "y", crate::curvefit::CurveModel::Polynomial(2));
        let out = run_analysis(&df, &spec).expect("curve fit");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Curve Fit"), "{text}");
        assert!(text.contains("Polynomial"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn friedman_and_mood_specs_run_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_f64("b", vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0]),
            Column::new_f64("c", vec![20.0, 21.0, 22.0, 23.0, 24.0, 25.0]),
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

        let spec = AnalysisSpec::friedman(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
        let out = run_analysis(&df, &spec).expect("friedman");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Friedman Test"), "{text}");
        assert!(text.contains("Q"), "{text}");

        let spec = AnalysisSpec::moods_median("a", "g");
        let out = run_analysis(&df, &spec).expect("mood");
        assert!(
            out.results[0]
                .to_session_text()
                .contains("Mood's Median Test")
        );
    }

    #[test]
    fn rank_test_specs_run_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]),
            Column::new_f64("b", vec![10.0, 11.0, 12.0, 13.0, 14.0, 15.0]),
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

        let spec = AnalysisSpec::mann_whitney("a", "b");
        assert!(spec.description().contains("Mann-Whitney"));
        let out = run_analysis(&df, &spec).expect("mann-whitney");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Mann-Whitney Test"), "{text}");
        assert!(text.contains("p-value"), "{text}");
        assert!(out.plot.is_none());

        let spec = AnalysisSpec::wilcoxon_signed_rank("a", "b");
        let out = run_analysis(&df, &spec).expect("wilcoxon");
        assert!(
            out.results[0]
                .to_session_text()
                .contains("Wilcoxon Signed-Rank Test")
        );

        let spec = AnalysisSpec::kruskal_wallis("a", "g");
        let out = run_analysis(&df, &spec).expect("kruskal");
        assert!(
            out.results[0]
                .to_session_text()
                .contains("Kruskal-Wallis Test")
        );
    }

    #[test]
    fn equivalence_specs_run_through_the_workflow() {
        let values: Vec<f64> = (1..=20)
            .map(|i| 10.0 + if i % 2 == 0 { 0.02 } else { -0.02 })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");

        let spec = AnalysisSpec::one_sample_equivalence("life", 10.0, 0.5);
        assert!(spec.description().contains("Equivalence test"));
        let out = run_analysis(&df, &spec).expect("one-sample equivalence");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Equivalence Test: 1-Sample"), "{text}");
        assert!(text.contains("p-value (TOST)"), "{text}");
        assert!(text.contains("equivalent"), "{text}");
        assert!(out.plot.is_none());

        let spec = AnalysisSpec::one_sample_equivalence("life", 10.0, 0.5)
            .with_equivalence_confidence(0.90);
        let out = run_analysis(&df, &spec).expect("one-sample equivalence");
        assert!(
            out.results[0]
                .to_session_text()
                .contains("equivalence interval at 80%")
        );
    }

    #[test]
    fn three_parameter_lognormal_spec_runs_through_the_workflow() {
        let truth = crate::dists::Lognormal3::new(5.0, 2.0, 0.4).expect("lognormal");
        let values: Vec<f64> = (1..=120)
            .map(|i| {
                let p = (i as f64 - 0.5) / 120.0;
                truth.quantile(p)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Lognormal3);
        let out = run_analysis(&df, &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("3-parameter"), "{}", result.title);
        assert_eq!(result.parameters[0].0, "Threshold (γ)");
        assert!((result.parameters[0].1 - 5.0).abs() < 1.0, "threshold");
        let plot = out.plot.as_ref().expect("plot");
        assert!(matches!(plot.x_axis.scale, crate::plot::Scale::Linear));
        assert!(plot.lines.len() > 10, "the fit is a polyline");
    }

    #[test]
    fn three_parameter_weibull_spec_runs_through_the_workflow() {
        let truth = crate::dists::Weibull3::new(5.0, 2.0, 10.0).expect("weibull");
        let values: Vec<f64> = (1..=120)
            .map(|i| {
                let p = (i as f64 - 0.5) / 120.0;
                truth.quantile(p)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Weibull3);
        let out = run_analysis(&df, &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("3-parameter"), "{}", result.title);
        assert_eq!(result.parameters[0].0, "Threshold (γ)");
        assert!((result.parameters[0].1 - 5.0).abs() < 1.0, "threshold");
        assert!((result.parameters[1].1 - 2.0).abs() < 0.3, "shape");
        let plot = out.plot.as_ref().expect("plot");
        assert!(matches!(plot.x_axis.scale, crate::plot::Scale::Linear));
        assert!(matches!(
            plot.y_axis.scale,
            crate::plot::Scale::Probability(crate::plot::ProbabilityScale::Gumbel)
        ));
        assert!(plot.lines.len() > 10, "the fit is a polyline");
    }

    #[test]
    fn gamma_rejects_non_positive_data() {
        let df = DataFrame::new(vec![Column::new_f64("life", vec![1.0, 0.0, 2.0])]).expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Gamma);
        assert!(matches!(
            run_analysis(&df, &spec),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn gamma_capability_reports_expected_ppm() {
        let truth = Gamma::new(3.0, 2.0).expect("gamma");
        let values: Vec<f64> = (1..=200)
            .map(|i| {
                let p = (i as f64 - 0.5) / 200.0;
                truth.quantile(p)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", values)]).expect("frame");
        let spec = AnalysisSpec::capability("life", Some(2.0), Some(10.0))
            .with_capability_distribution(DistributionKind::Gamma)
            .with_capability_bootstrap(10);
        let out = run_analysis(&df, &spec).expect("gamma capability");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Capability Analysis: Gamma"), "{text}");
        assert!(text.contains("PPM"), "{text}");
        assert!(text.contains("Fit check"), "{text}");
    }

    #[test]
    fn logistic_and_loglogistic_capability_report_expected_ppm() {
        let logistic_values: Vec<f64> = (1..=80)
            .map(|i| {
                let p = (i as f64 - 0.5) / 80.0;
                50.0 + 4.0 * (p / (1.0 - p)).ln()
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("score", logistic_values)]).expect("frame");
        let spec = AnalysisSpec::capability("score", Some(35.0), Some(65.0))
            .with_capability_distribution(DistributionKind::Logistic)
            .with_capability_bootstrap(20);
        let out = run_analysis(&df, &spec).expect("logistic capability");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Capability Analysis: Logistic"), "{text}");
        assert!(text.contains("PPM"), "{text}");
        assert!(text.contains("Fit check"), "{text}");

        let loglogistic_values: Vec<f64> = (1..=80)
            .map(|i| {
                let p = (i as f64 - 0.5) / 80.0;
                10.0 * (p / (1.0 - p)).powf(0.5)
            })
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("life", loglogistic_values)]).expect("frame");
        let spec = AnalysisSpec::capability("life", Some(2.0), Some(40.0))
            .with_capability_distribution(DistributionKind::Loglogistic)
            .with_capability_bootstrap(20);
        let out = run_analysis(&df, &spec).expect("loglogistic capability");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Capability Analysis: Loglogistic"), "{text}");
        assert!(text.contains("PPM"), "{text}");
    }

    #[test]
    fn lognormal_rejects_non_positive_data() {
        let df = DataFrame::new(vec![Column::new_f64("life", vec![1.0, 0.0, 2.0])]).expect("frame");
        let spec =
            AnalysisSpec::probability_plot("life").with_distribution(DistributionKind::Lognormal);
        assert!(run_analysis(&df, &spec).is_err());
    }

    #[test]
    fn attribute_chart_spec_runs_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("defects", vec![2.0, 4.0, 3.0, 5.0, 1.0, 4.0]),
            Column::new_f64("size", vec![100.0; 6]),
        ])
        .expect("frame");
        let spec =
            AnalysisSpec::attribute_chart(crate::attribute::AttributeChartKind::P, "defects")
                .with_attribute_size(Some("size".to_owned()), None);
        let out = run_analysis(&df, &spec).expect("p chart");
        assert!(out.results[0].title.contains("P Chart"));
        assert!(out.results[0].to_session_text().contains("Center line"));
        assert_eq!(out.plot.expect("plot").points.len(), 6);
    }

    #[test]
    fn factorial_spec_runs_through_the_workflow() {
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
        let out = run_analysis(
            &df,
            &AnalysisSpec::factorial("y", vec!["A".to_owned(), "B".to_owned()]),
        )
        .expect("factorial");
        assert!(out.results[0].title.contains("2-Level Factorial"));
        assert!(out.results[0].to_session_text().contains("Effect"));
        assert!(out.plot.is_some());
    }

    #[test]
    fn censored_lognormal_spec_runs_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 10.0]),
            Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0]),
        ])
        .expect("frame");
        let spec = AnalysisSpec::lognormal_reliability("time")
            .with_censoring(Some("status".to_owned()), 0.0);
        let out = run_analysis(&df, &spec).expect("lognormal");
        assert!(out.results[0].title.contains("Lognormal Reliability"));
        assert!(out.results[0].to_session_text().contains("B10 life"));
        assert!(out.plot.is_some());
    }

    #[test]
    fn censored_exponential_spec_runs_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 10.0]),
            Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0]),
        ])
        .expect("frame");
        let spec = AnalysisSpec::exponential_reliability("time")
            .with_censoring(Some("status".to_owned()), 0.0);
        let out = run_analysis(&df, &spec).expect("exponential");
        assert!(out.results[0].title.contains("Exponential Reliability"));
        assert!(out.results[0].to_session_text().contains("B10 life"));
        assert!(out.plot.is_some());
    }

    #[test]
    fn censored_weibull_spec_runs_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0]),
            Column::new_f64("status", vec![1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]),
        ])
        .expect("frame");
        let spec = AnalysisSpec::weibull_reliability("time")
            .with_censoring(Some("status".to_owned()), 0.0);
        let out = run_analysis(&df, &spec).expect("weibull");
        assert!(out.results[0].title.contains("Weibull Reliability"));
        assert!(out.results[0].to_session_text().contains("B10 life"));
        assert!(out.plot.is_some());
    }

    #[test]
    fn agreement_runs_through_the_workflow() {
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
        let spec = AnalysisSpec::agreement("part", "appraiser", "rating");
        assert_eq!(
            spec.description(),
            "Attribute Agreement — part × appraiser × rating"
        );
        assert_eq!(spec.column(), "part");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Attribute Agreement"), "{text}");
        assert!(out.plot.is_none());
    }

    #[test]
    fn kmeans_runs_through_the_workflow() {
        let mut x = Vec::new();
        let mut y = Vec::new();
        for centre in [(0.0, 0.0), (10.0, 10.0)] {
            for step in 0..6 {
                x.push(centre.0 + step as f64 * 0.1);
                y.push(centre.1 + step as f64 * 0.1);
            }
        }
        let df =
            DataFrame::new(vec![Column::new_f64("x", x), Column::new_f64("y", y)]).expect("frame");
        let spec = AnalysisSpec::kmeans(vec!["x".to_owned(), "y".to_owned()], 2)
            .with_kmeans_options(true, 11);
        assert_eq!(
            spec.description(),
            "K-Means — 2 variables, 2 clusters, standardised (seed 11)"
        );
        assert_eq!(spec.column(), "x");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("K-Means"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn accelerated_life_runs_through_the_workflow() {
        // Two stress levels are enough for the workflow test; the fit itself is
        // covered in the module.
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
        let spec = AnalysisSpec::accelerated_life("time", "stress")
            .with_accelerated_life_options(None, 1.0, 300.0);
        assert_eq!(
            spec.description(),
            "Accelerated Life — time against stress, Arrhenius (1/S) / Weibull, use stress 300"
        );
        assert_eq!(spec.column(), "time");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Accelerated Life"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn grouped_probability_plots_can_be_separate_or_overlaid() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5];
        let groups = vec![
            "A".to_owned(),
            "A".to_owned(),
            "A".to_owned(),
            "A".to_owned(),
            "A".to_owned(),
            "A".to_owned(),
            "B".to_owned(),
            "B".to_owned(),
            "B".to_owned(),
            "B".to_owned(),
            "B".to_owned(),
            "B".to_owned(),
        ];
        let df = DataFrame::new(vec![
            Column::new_f64("y", values),
            Column::new_text("g", groups),
        ])
        .expect("frame");
        let separate = AnalysisSpec::probability_plot("y")
            .with_group("g")
            .with_layout(PlotLayout::Separate);
        let out = run_analysis(&df, &separate).expect("separate");
        assert_eq!(out.results.len(), 2);
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 2, "one panel per level");
        let overlay = AnalysisSpec::probability_plot("y")
            .with_group("g")
            .with_layout(PlotLayout::Overlay);
        let out = run_analysis(&df, &overlay).expect("overlay");
        let plot = out.plot.expect("plot");
        assert!(plot.panels.is_empty());
        assert!(!plot.series.is_empty());
    }

    #[test]
    fn series_plot_runs_through_the_workflow() {
        let values: Vec<f64> = (1..=20).map(|index| index as f64 * 0.5).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let spec = AnalysisSpec::series_plot("x");
        assert_eq!(spec.description(), "Time Series Plot — x");
        assert_eq!(spec.column(), "x");
        let out = run_analysis(&df, &spec).expect("analysis");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.points.len(), 20);
        assert_eq!(plot.lines.len(), 19);
        let with_time = AnalysisSpec::series_plot("x").with_series_time(Some("x".to_owned()));
        assert_eq!(with_time.description(), "Time Series Plot — x against x");
    }

    #[test]
    fn stem_leaf_and_ecdf_run_through_the_workflow() {
        let values: Vec<f64> = (1..=40).map(|index| 10.0 + index as f64 * 0.5).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let stem = AnalysisSpec::stem_leaf("x");
        assert_eq!(stem.description(), "Stem-and-Leaf — x");
        assert_eq!(stem.column(), "x");
        let out = run_analysis(&df, &stem).expect("stem");
        assert!(out.plot.is_none());

        let ecdf = AnalysisSpec::ecdf("x").with_ecdf_confidence(0.90);
        assert_eq!(ecdf.description(), "Empirical CDF — x, 90% band");
        let out = run_analysis(&df, &ecdf).expect("ecdf");
        assert!(out.plot.is_some());
        assert_eq!(out.plot.expect("plot").bands.len(), 1);
    }

    #[test]
    fn dotplot_runs_through_the_workflow() {
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
        let spec = AnalysisSpec::dotplot("y").with_dotplot_factor(Some("g".to_owned()));
        assert_eq!(spec.description(), "Dotplot — y by g");
        assert_eq!(spec.column(), "y");
        let out = run_analysis(&df, &spec).expect("analysis");
        assert_eq!(out.plot.expect("plot").lines.len(), 2);
        // Without a factor the description is shorter.
        let plain = AnalysisSpec::dotplot("y");
        assert_eq!(plain.description(), "Dotplot — y");
    }

    #[test]
    fn matrix_plot_runs_through_the_workflow() {
        let mut a = Vec::new();
        let mut b = Vec::new();
        for index in 0..20 {
            a.push(index as f64);
            b.push(2.0 * index as f64 + 0.5);
        }
        let df =
            DataFrame::new(vec![Column::new_f64("a", a), Column::new_f64("b", b)]).expect("frame");
        let spec =
            AnalysisSpec::matrix_plot(vec!["a".to_owned(), "b".to_owned()]).with_matrix_bins(5);
        assert_eq!(spec.description(), "Matrix Plot — 2 variables, 5 bins");
        assert_eq!(spec.column(), "a");
        let out = run_analysis(&df, &spec).expect("analysis");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 4);
        assert_eq!(plot.panel_columns, 2);
    }

    #[test]
    fn interval_plot_runs_through_the_workflow() {
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
        let spec = AnalysisSpec::interval_plot("y", "g").with_interval_confidence(0.90);
        assert_eq!(spec.description(), "Interval Plot — y by g, 90% intervals");
        assert_eq!(spec.column(), "y");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Interval Plot"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn pareto_runs_through_the_workflow() {
        let df = DataFrame::new(vec![Column::new_text(
            "defect",
            vec![
                "scratch".to_owned(),
                "scratch".to_owned(),
                "dent".to_owned(),
                "crack".to_owned(),
            ],
        )])
        .expect("frame");
        let spec = AnalysisSpec::pareto("defect");
        assert_eq!(spec.description(), "Pareto Chart — defect");
        assert_eq!(spec.column(), "defect");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Pareto Chart"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn distribution_comparison_skips_a_family_that_does_not_converge() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/normal_n20.csv");
        let text = std::fs::read_to_string(path).expect("dataset");
        let values: Vec<f64> = text
            .lines()
            .skip(1)
            .filter_map(|line| line.trim().replace(',', ".").parse().ok())
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("data", values)]).expect("frame");
        let result = distribution_comparison(&df, "data").expect("comparison");
        assert!(result.n == 20);
        // The extreme-value fit does not converge on this sample; the report
        // must still be produced, with the other families and a note.
        let text = result.to_session_text();
        assert!(text.contains("Normal"), "{text}");
        assert!(
            text.contains("skipped") || text.contains("Extreme"),
            "{text}"
        );
    }

    #[test]
    fn a_phased_control_chart_runs_through_the_workflow() {
        let values = vec![10.0, 10.2, 9.8, 10.1, 20.0, 20.2, 19.8, 20.1];
        let phases: Vec<String> = std::iter::repeat_n("before".to_owned(), 4)
            .chain(std::iter::repeat_n("after".to_owned(), 4))
            .collect();
        let df = DataFrame::new(vec![
            Column::new_f64("weight", values),
            Column::new_text("phase", phases),
        ])
        .expect("frame");
        let spec = AnalysisSpec::control_chart(
            "weight",
            crate::spc::ControlChartKind::IndividualsMovingRange,
            1,
        )
        .with_phase_column(Some("phase".to_owned()));
        assert!(spec.description().contains("phase column phase"));
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Phases"), "{text}");
        assert!(out.plot.is_some());
        // Phases and subgroup labels are alternatives.
        let both = spec.clone().with_subgroup_column(Some("phase".to_owned()));
        assert!(run_analysis(&df, &both).is_err());
    }

    #[test]
    fn every_plot_gets_a_report_summary_by_default() {
        let df = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![12.5, 13.1, 11.9, 12.8, 12.2, 12.9, 12.4, 13.0],
        )])
        .expect("frame");
        let out = run_analysis(&df, &AnalysisSpec::histogram("weight")).expect("histogram");
        let plot = out.plot.expect("plot");
        let note = plot.notes.first().expect("generic report note");
        assert!(note.rows.iter().any(|row| row.label == "N"));
        assert!(
            note.rows.len() > 2,
            "the report block has more than the sample size: {:?}",
            note.rows
        );
        // Hiding the report removes the block from the SVG and gives the plot
        // the space back.
        let hidden = crate::plot::GraphOverlay {
            show_report: false,
            ..crate::plot::GraphOverlay::default()
        };
        let svg = crate::plot::render_svg_with(&plot, &hidden);
        let first_label = &note.rows.first().expect("row").label;
        assert!(
            !svg.contains(&format!(">{first_label}</text>")),
            "the report block is still drawn"
        );
    }

    #[test]
    fn an_overlay_labelled_each_series_mean() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("b", vec![6.0, 7.0, 8.0, 9.0, 10.0]),
        ])
        .expect("frame");
        let mut spec = AnalysisSpec::probability_plot_multi(vec!["a".to_owned(), "b".to_owned()]);
        if let AnalysisSpec::ProbabilityPlot { layout, .. } = &mut spec {
            *layout = PlotLayout::Overlay;
        }
        let out = run_analysis(&df, &spec).expect("overlay");
        let plot = out.plot.expect("plot");
        let note = plot.notes.first().expect("overlay note");
        let labels: Vec<&str> = note.rows.iter().map(|row| row.label.as_str()).collect();
        assert!(labels.contains(&"a"), "{labels:?}");
        assert!(labels.contains(&"a Mean"), "{labels:?}");
        assert!(labels.contains(&"a StDev"), "{labels:?}");
        assert!(labels.contains(&"b Mean"), "{labels:?}");
        // The mean values are the sample means.
        let row = |label: &str| {
            note.rows
                .iter()
                .find(|row| row.label == label)
                .map(|row| row.value.clone())
                .expect(label)
        };
        assert_eq!(row("a Mean"), "3.0000");
        assert_eq!(row("b Mean"), "8.0000");
        // The same block reaches the SVG (outside the frame).
        let svg = crate::plot::render_svg_with(&plot, &Default::default());
        assert!(svg.contains(">a Mean</text>"), "{svg}");
    }

    #[test]
    fn a_weighted_pareto_runs_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_text(
                "defect",
                vec!["scratch".to_owned(), "dent".to_owned(), "dent".to_owned()],
            ),
            Column::new_f64("cost", vec![1.0, 4.0, 4.0]),
        ])
        .expect("frame");
        let spec = AnalysisSpec::pareto_weighted("defect", "cost");
        assert_eq!(
            spec.description(),
            "Pareto Chart — defect, weighted by cost"
        );
        assert_eq!(spec.column(), "defect");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("weighted by cost"), "{text}");
        assert!(text.contains("Total weight"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn taguchi_runs_through_the_workflow() {
        let df = crate::taguchi::design(8, 3, false, 2026).expect("design");
        let levels = df
            .column("A")
            .expect("A")
            .numeric(MissingPolicy::Drop)
            .expect("levels")
            .to_vec();
        let mut with_response = df.clone();
        with_response.add_column(crate::data::DType::Float);
        let last = with_response.n_columns() - 1;
        with_response.rename_column(last, "y").expect("rename");
        for (row, level) in levels.iter().enumerate() {
            with_response
                .set_cell(last, row, &(level * 10.0).to_string())
                .expect("set");
        }
        let spec = AnalysisSpec::taguchi(
            "y",
            vec!["A".to_owned(), "B".to_owned()],
            crate::taguchi::Quality::Smaller,
        );
        assert_eq!(
            spec.description(),
            "Taguchi — y on 2 factor(s), smaller is better"
        );
        assert_eq!(spec.column(), "y");
        let out = run_analysis(&with_response, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Taguchi Analysis"), "{text}");
        assert!(out.plot.is_none());
    }

    #[test]
    fn acceptance_sampling_runs_through_the_workflow() {
        let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0])]).expect("frame");
        let evaluated = AnalysisSpec::acceptance_sampling(20, 1, Some(500));
        assert_eq!(
            evaluated.description(),
            "Acceptance Sampling — n = 20, c = 1"
        );
        let out = run_analysis(&df, &evaluated).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Acceptance Sampling"), "{text}");
        assert!(out.plot.is_some());

        let planned = AnalysisSpec::acceptance_plan(0.01, 0.05, 0.05, 0.10);
        let text = planned.description();
        assert!(text.contains("plan for AQL 1.00%"), "{text}");
        let out = run_analysis(&df, &planned).expect("plan");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Producer risk"), "{text}");
        assert_eq!(planned.column(), "");
    }

    #[test]
    fn hotelling_t2_runs_through_the_workflow() {
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
        let spec = AnalysisSpec::hotelling_t2(vec!["a".to_owned(), "b".to_owned()], 3)
            .with_t2_confidence(0.95);
        assert_eq!(
            spec.description(),
            "Hotelling T² — 2 variables, subgroups of 3, 95% limits"
        );
        assert_eq!(spec.column(), "a");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Hotelling T"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn pca_runs_through_the_workflow() {
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
        let spec = AnalysisSpec::pca(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()], 2);
        assert_eq!(
            spec.description(),
            "Principal Component Analysis — 3 variables, 2 score component(s)"
        );
        assert_eq!(spec.column(), "a");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Principal Component Analysis"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn timeseries_specs_run_through_the_workflow() {
        let mut values = Vec::new();
        for index in 0..36 {
            values.push(100.0 + 0.8 * index as f64 + 8.0 * ((index % 12) as f64).sin());
        }
        let df = DataFrame::new(vec![Column::new_f64("sales", values)]).expect("frame");

        let average = AnalysisSpec::moving_average("sales", 4);
        assert_eq!(average.description(), "Moving average — sales, window 4");
        assert_eq!(average.column(), "sales");
        let out = run_analysis(&df, &average).expect("moving average");
        assert!(out.results[0].to_session_text().contains("Moving average"));
        assert!(out.plot.is_some());

        let decomposition = AnalysisSpec::decomposition(
            "sales",
            12,
            crate::timeseries::DecompositionModel::Multiplicative,
        );
        assert_eq!(
            decomposition.description(),
            "Decomposition — sales, period 12, multiplicative"
        );
        let out = run_analysis(&df, &decomposition).expect("decomposition");
        assert!(out.results[0].to_session_text().contains("multiplicative"));

        let smoothing = AnalysisSpec::exponential_smoothing("sales", 12, 3);
        assert_eq!(
            smoothing.description(),
            "Exponential smoothing — sales, season 12, 3 forecast(s)"
        );
        let out = run_analysis(&df, &smoothing).expect("smoothing");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Holt-Winters"), "{text}");
        assert!(text.contains("Forecast"), "{text}");
    }

    #[test]
    fn ordinal_regression_runs_through_the_workflow() {
        let mut y = Vec::new();
        let mut x = Vec::new();
        for index in 0..30 {
            let value = index as f64 / 10.0;
            x.push(value);
            // Overlapping categories keep the ordinal likelihood bounded.
            let latent = 0.8 * value + ((index * 37) % 11) as f64 / 4.0 - 1.25;
            y.push(if latent < 1.4 {
                1.0
            } else if latent < 2.6 {
                2.0
            } else {
                3.0
            });
        }
        let df =
            DataFrame::new(vec![Column::new_f64("y", y), Column::new_f64("x", x)]).expect("frame");
        let spec = AnalysisSpec::ordinal("y", vec!["x".to_owned()]);
        assert_eq!(spec.description(), "Ordinal Logistic Regression — y on x");
        assert_eq!(spec.column(), "y");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Ordinal Logistic Regression"), "{text}");
        assert!(out.plot.is_none());
    }

    #[test]
    fn poisson_regression_runs_through_the_workflow() {
        let mut x = Vec::new();
        let mut y = Vec::new();
        for index in 0..40 {
            let value = index as f64 / 10.0;
            x.push(value);
            y.push((0.5 + 0.7 * value).exp().round());
        }
        let df =
            DataFrame::new(vec![Column::new_f64("y", y), Column::new_f64("x", x)]).expect("frame");
        let spec = AnalysisSpec::poisson("y", vec!["x".to_owned()]);
        assert_eq!(spec.description(), "Poisson Regression — y on x");
        assert_eq!(spec.column(), "y");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Poisson Regression"), "{text}");
        assert!(out.plot.is_none());
    }

    #[test]
    fn logistic_regression_runs_through_the_workflow() {
        let mut y = Vec::new();
        let mut x = Vec::new();
        for index in 0..40 {
            x.push(index as f64);
            let event = (index >= 20 && index % 4 != 0) || (index < 5 && index % 2 == 0);
            y.push(if event { 1.0 } else { 0.0 });
        }
        let df =
            DataFrame::new(vec![Column::new_f64("y", y), Column::new_f64("x", x)]).expect("frame");
        let spec = AnalysisSpec::logistic("y", vec!["x".to_owned()]);
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Binary Logistic Regression"), "{text}");
        assert!(text.contains("Odds ratio"), "{text}");
        assert!(out.plot.is_none());
        assert_eq!(spec.description(), "Binary Logistic Regression — y on x");
        assert_eq!(spec.column(), "y");
        // The slope is positive and the null model fits worse.
        assert!(out.results[0].statistics[4].1 > 0.0);
    }

    #[test]
    fn gage_linearity_runs_through_the_workflow() {
        let mut measurement = Vec::new();
        let mut reference = Vec::new();
        for target in [10.0, 20.0, 30.0, 40.0] {
            for offset in [-0.2, 0.0, 0.2] {
                measurement.push(target + 0.1 * target - 0.1 + offset);
                reference.push(target);
            }
        }
        let df = DataFrame::new(vec![
            Column::new_f64("m", measurement),
            Column::new_f64("ref", reference),
        ])
        .expect("frame");
        let spec = AnalysisSpec::gage_linearity("m", "ref");
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Gage Linearity"), "{text}");
        assert!(text.contains("Slope"), "{text}");
        assert!(out.plot.is_some());
        assert_eq!(spec.description(), "Gage Linearity — m against ref");
        // The slope is recovered and the plot has one point per part.
        assert!((out.results[0].statistics[3].1 - 0.1).abs() < 1e-12);
        assert_eq!(out.plot.expect("plot").points.len(), 4);
    }

    #[test]
    fn gage_type1_spec_runs_through_the_workflow() {
        let df = DataFrame::new(vec![Column::new_f64(
            "m",
            vec![10.1, 10.2, 10.0, 10.3, 10.1, 10.2],
        )])
        .expect("frame");
        let out = run_analysis(
            &df,
            &AnalysisSpec::gage_type1("m", 10.0).with_gage_limits(Some(9.0), Some(11.0)),
        )
        .expect("type1");
        assert!(out.results[0].title.contains("Gage Type 1"));
        assert!(out.results[0].to_session_text().contains("%Tolerance"));
    }

    #[test]
    fn gage_rr_spec_runs_through_the_workflow() {
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
        let out = run_analysis(&df, &AnalysisSpec::gage_rr("m", "part", "operator")).expect("gage");
        assert!(out.results[0].title.contains("Gage R&R"));
        assert!(out.results[0].to_session_text().contains("%Gage R&R"));
    }

    #[test]
    fn kaplan_meier_spec_runs_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("time", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("status", vec![1.0, 0.0, 1.0, 1.0, 0.0]),
        ])
        .expect("frame");
        let spec =
            AnalysisSpec::kaplan_meier("time").with_censoring(Some("status".to_owned()), 0.0);
        let out = run_analysis(&df, &spec).expect("km");
        assert!(out.results[0].title.contains("Kaplan–Meier"));
        assert!(out.results[0].to_session_text().contains("Median survival"));
        assert!(out.plot.is_some());
    }

    #[test]
    fn anova_spec_runs_through_the_workflow() {
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
        let out = run_analysis(&df, &AnalysisSpec::anova("y", "g")).expect("anova");
        assert!(out.results[0].title.contains("One-Way ANOVA"));
        assert!(out.results[0].to_session_text().contains("Bonferroni"));
        assert!(out.plot.is_some());
    }

    #[test]
    fn regression_spec_runs_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("x1", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("x2", vec![2.0, 1.0, 4.0, 3.0, 6.0]),
            Column::new_f64("y", vec![2.0, 4.0, 5.0, 4.0, 5.0]),
        ])
        .expect("frame");
        let spec = AnalysisSpec::regression("y", vec!["x1".to_owned(), "x2".to_owned()]);
        let out = run_analysis(&df, &spec).expect("regression");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Regression"), "{text}");
        assert!(text.contains("Regression equation"), "{text}");
        assert!(out.plot.is_some());
    }

    #[test]
    fn bonett_variance_spec_runs_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![10.0, 20.0, 30.0, 40.0, 50.0]),
            Column::new_f64("b", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
        ])
        .expect("frame");
        let spec = AnalysisSpec::two_variances("a", "b")
            .with_variance_method(crate::variance::VarianceMethod::Bonett);
        let out = run_analysis(&df, &spec).expect("bonett");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Bonett"), "{text}");
        assert!(text.contains("Z"), "{text}");
        assert!(text.contains("Variance ratio"), "{text}");
    }

    #[test]
    fn proportion_and_variance_specs_run_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 1.0, 1.0, 0.0, 0.0]),
            Column::new_f64("b", vec![1.0, 1.0, 0.0, 0.0, 0.0]),
        ])
        .expect("frame");
        let one = run_analysis(&df, &AnalysisSpec::one_proportion("a", 0.5)).expect("1 prop");
        assert!(one.results[0].title.contains("1 Proportion"));
        let two = run_analysis(&df, &AnalysisSpec::two_proportions("a", "b")).expect("2 prop");
        assert!(two.results[0].title.contains("2 Proportions"));
        let var = run_analysis(&df, &AnalysisSpec::one_variance("a", 1.0)).expect("1 var");
        assert!(var.results[0].title.contains("1 Variance"));
        let vars = run_analysis(&df, &AnalysisSpec::two_variances("a", "b")).expect("2 var");
        assert!(vars.results[0].title.contains("2 Variances"));
    }

    #[test]
    fn t_test_specs_run_through_the_workflow() {
        let df = DataFrame::new(vec![
            Column::new_f64("before", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("after", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
        ])
        .expect("frame");

        let one = run_analysis(&df, &AnalysisSpec::one_sample_t("before", 3.0)).expect("1-sample");
        assert!(one.results[0].title.contains("1-Sample t"));
        assert!(one.results[0].to_session_text().contains("H₀"));
        assert!(one.results[0].to_session_text().contains("p-value"));

        let two =
            run_analysis(&df, &AnalysisSpec::two_sample_t("before", "after")).expect("2-sample");
        assert!(two.results[0].title.contains("2-Sample t"));
        assert!(two.results[0].method.contains("Welch"));

        let pooled = run_analysis(
            &df,
            &AnalysisSpec::two_sample_t("before", "after").with_pooled(true),
        )
        .expect("pooled");
        assert!(pooled.results[0].method.contains("Pooled"));

        let paired = run_analysis(&df, &AnalysisSpec::paired_t("before", "after")).expect("paired");
        assert!(paired.results[0].title.contains("Paired t"));
        assert_eq!(paired.results[0].n, 5);
    }

    #[test]
    fn t_test_alternative_changes_the_bound() {
        let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0])])
            .expect("frame");
        let less = run_analysis(
            &df,
            &AnalysisSpec::one_sample_t("x", 0.0)
                .with_alternative(crate::hypothesis::Alternative::Less),
        )
        .expect("less");
        assert!(
            less.results[0]
                .statistics
                .iter()
                .any(|(name, _)| *name == "Upper bound")
        );
    }

    #[test]
    fn spec_defaults_and_run() {
        let spec = AnalysisSpec::probability_plot("weight");
        assert!(spec.description().contains("Normal"));
        assert!(spec.description().contains("95% CI"));
        let out = run_analysis(&frame(), &spec).expect("analysis");
        assert!(out.results[0].to_session_text().contains("Analysis"));
        assert!(!out.plot.as_ref().expect("plot").bands.is_empty());
    }

    #[test]
    fn display_choice_hides_the_requested_layer() {
        let df = frame();
        let symbols = run_analysis(
            &df,
            &AnalysisSpec::probability_plot("weight").with_display(PlotDisplay::Symbols),
        )
        .expect("symbols");
        let plot = symbols.plot.expect("plot");
        assert_eq!(plot.points.len(), 5);
        assert!(plot.lines.is_empty());
        assert!(plot.bands.is_empty());

        let fit = run_analysis(
            &df,
            &AnalysisSpec::probability_plot("weight").with_display(PlotDisplay::Fit),
        )
        .expect("fit");
        let plot = fit.plot.expect("plot");
        assert!(plot.points.is_empty());
        assert!(!plot.lines.is_empty());
    }

    #[test]
    fn multi_y_overlays_two_series() {
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![12.5, 13.1, 11.9, 12.8, 12.2]),
            Column::new_f64("height", vec![21.5, 22.1, 20.9, 21.8, 21.2]),
        ])
        .expect("frame");
        let spec =
            AnalysisSpec::probability_plot_multi(vec!["weight".to_owned(), "height".to_owned()]);
        let out = run_analysis(&df, &spec).expect("analysis");
        assert_eq!(out.results.len(), 2);
        let panels = out.plot.expect("plot");
        assert_eq!(panels.panels.len(), 2);
        assert!(panels.series.is_empty());

        let overlay =
            run_analysis(&df, &spec.clone().with_layout(PlotLayout::Overlay)).expect("overlay");
        let plot = overlay.plot.expect("plot");
        assert_eq!(plot.series.len(), 2);
        assert_eq!(plot.series[0].label, "weight");
        assert_eq!(plot.series[1].label, "height");
        assert!(plot.points.is_empty());

        let shared = run_analysis(&df, &spec.with_same_scale(true, true)).expect("shared");
        let panels = shared.plot.expect("panels");
        assert_eq!(panels.panels[0].x_axis.min, panels.panels[1].x_axis.min);
        assert_eq!(panels.panels[0].y_axis.max, panels.panels[1].y_axis.max);
    }

    #[test]
    fn grouping_overlays_one_series_per_level() {
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
        // Grouping follows the layout switch, so the overlay is asked for.
        let spec = AnalysisSpec::probability_plot("weight")
            .with_group("batch")
            .with_layout(PlotLayout::Overlay);
        let out = run_analysis(&df, &spec).expect("analysis");
        assert_eq!(out.results.len(), 2);
        let plot = out.plot.expect("plot");
        assert_eq!(plot.series.len(), 2);
        assert_eq!(plot.series[0].label, "weight (A)");
        assert_eq!(plot.series[1].label, "weight (B)");
    }

    #[test]
    fn grouping_rejects_multi_y() {
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![11.0, 11.5, 12.0, 13.0]),
            Column::new_f64("height", vec![21.0, 21.5, 22.0, 23.0]),
            Column::new_text(
                "batch",
                vec!["A".into(), "A".into(), "B".into(), "B".into()],
            ),
        ])
        .expect("frame");
        let spec =
            AnalysisSpec::probability_plot_multi(vec!["weight".to_owned(), "height".to_owned()])
                .with_group("batch");
        assert!(matches!(
            run_analysis(&df, &spec),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn expected_ppm_intervals_follow_the_recorded_values() {
        let overall =
            expected_ppm_interval(19.8411, 3.16297, 29.0, 30, Some(15.0), Some(25.0), 0.95)
                .expect("overall intervals");
        assert!(
            (overall.low * 1e6 - 62_941.03).abs() < 5.0,
            "{}",
            overall.low * 1e6
        );
        assert!((overall.high * 1e6 - 51_440.66).abs() < 5.0);
        assert!((overall.total * 1e6 - 114_381.69).abs() < 5.0);
        assert!((overall.low_ci.0 * 1e6 - 19_570.04).abs() < 5.0);
        assert!((overall.low_ci.1 * 1e6 - 159_048.19).abs() < 5.0);
        assert!((overall.high_ci.0 * 1e6 - 14_531.63).abs() < 5.0);
        assert!((overall.high_ci.1 * 1e6 - 140_191.83).abs() < 5.0);
        assert!((overall.total_ci.0 * 1e6 - 48_940.14).abs() < 5.0);
        assert!((overall.total_ci.1 * 1e6 - 244_805.56).abs() < 5.0);

        let within =
            expected_ppm_interval(19.8411, 3.10733, 29.0, 30, Some(15.0), Some(25.0), 0.95)
                .expect("within intervals");
        assert!((within.low * 1e6 - 59_622.58).abs() < 10.0);
        assert!((within.high * 1e6 - 48_432.32).abs() < 10.0);
        assert!((within.total * 1e6 - 108_054.90).abs() < 10.0);
        assert!((within.low_ci.0 * 1e6 - 18_069.59).abs() < 10.0);
        assert!((within.low_ci.1 * 1e6 - 153_735.84).abs() < 10.0);
        assert!((within.high_ci.0 * 1e6 - 13_291.96).abs() < 10.0);
        assert!((within.high_ci.1 * 1e6 - 135_030.57).abs() < 10.0);
        assert!((within.total_ci.0 * 1e6 - 45_131.09).abs() < 10.0);
        assert!((within.total_ci.1 * 1e6 - 236_940.41).abs() < 10.0);

        let one_sided = expected_ppm_interval(19.8411, 3.16297, 29.0, 30, Some(15.0), None, 0.95)
            .expect("one-sided intervals");
        assert_eq!(one_sided.high, 0.0);
        assert_eq!(one_sided.total_ci, one_sided.low_ci);
        assert!(expected_ppm_interval(1.0, 0.0, 10.0, 10, Some(0.0), None, 0.95).is_none());
    }

    #[test]
    fn capability_confidence_intervals_follow_standard_formulas() {
        let (low, high) = cp_confidence_interval(1.0, 10.0, 0.95).expect("Cp CI");
        assert!((low - 0.5698).abs() < 1e-3, "low={low}");
        assert!((high - 1.4310).abs() < 1e-3, "high={high}");

        let (low, high) = cpk_confidence_interval(1.0, 30, 0.95).expect("Cpk CI");
        assert!((low - 0.7160).abs() < 2e-3, "low={low}");
        assert!((high - 1.2840).abs() < 2e-3, "high={high}");
        assert!(cp_confidence_interval(1.0, 0.0, 0.95).is_none());
        assert!(cpk_confidence_interval(1.0, 1, 0.95).is_none());
    }

    #[test]
    fn normal_capability_reports_indices_and_ppm() {
        let df = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![10.0, 11.0, 9.0, 10.5, 9.5, 10.2, 9.8, 10.3],
        )])
        .expect("frame");
        let spec = AnalysisSpec::capability("weight", Some(8.0), Some(12.0));
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        for needle in [
            "Capability Analysis",
            "Cp",
            "Cpk",
            "Cpm",
            "Pp",
            "Ppk",
            "PPM",
            "Cp 95% CI",
            "Cpk 95% CI",
            "Pp 95% CI",
            "Ppk 95% CI",
            "Anderson–Darling (A²)",
            "Decision at 5%",
            "Observed PPM",
            "Expected PPM",
        ] {
            assert!(text.contains(needle), "{needle} missing:\n{text}");
        }
        let spec = spec.with_capability_confidence(0.90);
        let out = run_analysis(&df, &spec).expect("90% analysis");
        assert!(out.results[0].to_session_text().contains("Cpk 90% CI"));
        let spec = spec.with_capability_target(Some(10.5));
        let out = run_analysis(&df, &spec).expect("target analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Cpm target"));
        assert!(text.contains("10.500000"));
        assert!(
            out.plot
                .as_ref()
                .is_some_and(|plot| plot.title.contains("Capability Histogram"))
        );
    }

    #[test]
    fn capability_omits_cpm_without_a_target_and_reports_it_with_one() {
        let df = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![10.0, 11.0, 9.0, 10.5, 9.5, 10.2, 9.8, 10.3],
        )])
        .expect("frame");
        let spec = AnalysisSpec::capability("weight", Some(8.0), Some(12.0));
        let text = run_analysis(&df, &spec).expect("analysis").results[0].to_session_text();
        assert!(
            text.contains("not specified; Cpm is not computed"),
            "{text}"
        );
        let cpm = text
            .lines()
            .find(|line| {
                line.trim_start().starts_with("Cpm ")
                    && !line.contains("(overall)")
                    && !line.contains("target")
            })
            .expect("Cpm row");
        assert!(
            cpm.contains('—'),
            "Cpm must be blank without a target: {cpm}"
        );

        let spec = spec.with_capability_target(Some(10.5));
        let text = run_analysis(&df, &spec).expect("analysis").results[0].to_session_text();
        assert!(text.contains("10.500000"), "{text}");
        let cpm = text
            .lines()
            .find(|line| {
                line.trim_start().starts_with("Cpm ")
                    && !line.contains("(overall)")
                    && !line.contains("target")
            })
            .expect("Cpm row");
        assert!(
            cpm.contains("0.659"),
            "Cpm must be computed with a target: {cpm}"
        );
    }

    #[test]
    fn capability_within_uses_the_published_d2_constant() {
        let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0])])
            .expect("frame");
        let spec = AnalysisSpec::capability("x", Some(0.0), Some(10.0));
        let text = run_analysis(&df, &spec).expect("analysis").results[0].to_session_text();
        let line = text
            .lines()
            .find(|line| line.trim_start().starts_with("StDev "))
            .expect("StDev row");
        // Average moving range 1.0 divided by the published d2(2) = 1.128.
        assert!(line.contains("0.886525"), "{line}");
    }

    #[test]
    fn capability_groups_the_rows_by_a_label_column() {
        // A: 10.0, 10.2, 9.8  →  ss = 0.08
        // B: 20.0, 20.4       →  ss = 0.08
        // n = 5, g = 2, df = 3, pooled = sqrt(0.16 / 3), c4(4) = 0.9213177.
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![10.0, 20.0, 10.2, 20.4, 9.8]),
            Column::new_text(
                "part",
                vec!["A", "B", "A", "B", "A"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
        ])
        .expect("frame");
        let spec = AnalysisSpec::capability("weight", Some(0.0), Some(30.0))
            .with_capability_subgroup_column(Some("part".to_owned()));
        let out = run_analysis(&df, &spec).expect("grouped capability");
        let text = out.results[0].to_session_text();
        assert!(text.contains("subgroup column part"), "{text}");
        assert!(text.contains("column part; 2 subgroups"), "{text}");
        let line = text
            .lines()
            .find(|line| line.trim_start().starts_with("StDev "))
            .expect("StDev row");
        assert!(line.contains("0.250663"), "{line}");
        assert!(
            text.contains("pooled standard deviation over the subgroups"),
            "{text}"
        );
    }

    #[test]
    fn capability_drops_rows_with_a_missing_value_or_a_missing_label() {
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![10.0, 10.2, f64::NAN, 10.1, 10.4, 10.0, 10.2]),
            Column::new_text(
                "part",
                vec!["A", "A", "A", "", "B", "B", "B"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
        ])
        .expect("frame");
        let spec = AnalysisSpec::capability("weight", Some(0.0), Some(30.0))
            .with_capability_subgroup_column(Some("part".to_owned()));
        let out = run_analysis(&df, &spec).expect("grouped capability");
        assert_eq!(out.results[0].n, 5);
        assert_eq!(out.results[0].excluded, 2);
        assert!(out.results[0].to_session_text().contains("2 subgroups"));
    }

    #[test]
    fn a_capability_subgroup_column_rejects_degenerate_groups() {
        let singleton = DataFrame::new(vec![
            Column::new_f64("weight", vec![10.0, 10.2, 9.9, 10.1, 10.3]),
            Column::new_text(
                "part",
                vec!["A", "A", "A", "A", "B"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
        ])
        .expect("frame");
        let spec = AnalysisSpec::capability("weight", Some(0.0), Some(30.0))
            .with_capability_subgroup_column(Some("part".to_owned()));
        let error = run_analysis(&singleton, &spec).expect_err("a singleton subgroup");
        assert!(
            error
                .to_string()
                .contains("every subgroup needs at least two"),
            "{error}"
        );

        // A single label is not a subgroup study.
        let single = DataFrame::new(vec![
            Column::new_f64("weight", vec![10.0, 10.2, 9.9, 10.1]),
            Column::new_text(
                "part",
                vec!["A"; 4].into_iter().map(str::to_owned).collect(),
            ),
        ])
        .expect("frame");
        let error = run_analysis(&single, &spec).expect_err("a single subgroup");
        assert!(
            error.to_string().contains("at least two subgroups"),
            "{error}"
        );

        // The column is a normal-model option.
        let error = run_analysis(
            &single,
            &AnalysisSpec::capability("weight", Some(0.0), Some(30.0))
                .with_capability_distribution(DistributionKind::Weibull)
                .with_capability_subgroup_column(Some("part".to_owned())),
        )
        .expect_err("a subgroup column on a non-normal model");
        assert!(
            error.to_string().contains("only available for the normal"),
            "{error}"
        );
        let error = run_analysis(
            &single,
            &AnalysisSpec::capability("weight", Some(0.0), Some(30.0))
                .with_capability_subgroup_column(Some("missing".to_owned())),
        )
        .expect_err("an unknown column");
        assert!(matches!(error, Error::ColumnNotFound(_)), "{error}");
    }

    #[test]
    fn capability_sixpack_has_the_six_panels() {
        let values: Vec<f64> = (0..40)
            .map(|index| 10.0 + 0.3 * f64::from(index % 4) / 1.5 + 0.02 * f64::from(index))
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("weight", values)]).expect("frame");
        let spec =
            AnalysisSpec::capability("weight", Some(8.0), Some(12.0)).with_capability_sixpack(true);
        let out = run_analysis(&df, &spec).expect("sixpack");

        let result = &out.results[0];
        assert!(
            result.title.contains("Capability Sixpack"),
            "{}",
            result.title
        );
        let text = result.to_session_text();
        assert!(text.contains("Capability Sixpack"), "{text}");
        assert!(text.contains("histogram"), "{text}");

        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 6);
        assert_eq!(plot.panel_columns, 2);
        let titles: Vec<&str> = plot
            .panels
            .iter()
            .map(|panel| panel.title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec![
                "Histogram",
                "Normal probability plot",
                "Individuals",
                "Moving range",
                "Last 25 observations",
                "Capability",
            ]
        );
        // The control chart keeps every point; the last panel keeps the last 25.
        assert_eq!(plot.panels[2].points.len(), 40);
        assert_eq!(plot.panels[3].points.len(), 39);
        assert_eq!(plot.panels[4].points.len(), 25);
        assert!(!plot.panels[0].lines.is_empty(), "the histogram has bars");
        assert!(
            !plot.panels[1].lines.is_empty(),
            "the probability plot has a fit"
        );

        // The capability panel reports the same within estimate as the report.
        let sample = capability_sample(&df, "weight", None).expect("sample");
        let estimates =
            sigma_estimates(&sample, &df, "weight", 1, None, true, false, None).expect("sigma");
        let cp = format!("{:.2}", (12.0 - 8.0) / (6.0 * estimates.within));
        let note = &plot.panels[5].notes[0];
        let labels: Vec<&str> = note.rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(labels, vec!["Cp", "Cpk", "Pp", "Ppk"]);
        let cp_row = note.rows.iter().find(|row| row.label == "Cp").expect("Cp");
        assert_eq!(cp_row.value, cp);
    }

    #[test]
    fn the_sixpack_control_chart_follows_the_subgroup_size() {
        let values: Vec<f64> = (0..40)
            .map(|index| 10.0 + 0.2 * f64::from(index % 5) + 0.01 * f64::from(index))
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("weight", values)]).expect("frame");

        let two = AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
            .with_capability_sixpack(true)
            .with_capability_subgroup(2);
        let plot = run_analysis(&df, &two)
            .expect("xbar-r sixpack")
            .plot
            .expect("plot");
        assert_eq!(plot.panels[2].title, "Subgroup mean");
        assert_eq!(plot.panels[3].title, "Sample range");
        assert_eq!(plot.panels[4].title, "Last 25 subgroups");
        assert_eq!(plot.panels[2].points.len(), 20);

        let wide = AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
            .with_capability_sixpack(true)
            .with_capability_subgroup(10);
        let plot = run_analysis(&df, &wide)
            .expect("xbar-s sixpack")
            .plot
            .expect("plot");
        assert_eq!(plot.panels[3].title, "Sample standard deviation");

        let short =
            AnalysisSpec::capability("weight", Some(8.0), Some(12.0)).with_capability_sixpack(true);
        let ten = DataFrame::new(vec![Column::new_f64(
            "weight",
            (0..10).map(|index| 10.0 + 0.1 * f64::from(index)).collect(),
        )])
        .expect("frame");
        let plot = run_analysis(&ten, &short)
            .expect("short sixpack")
            .plot
            .expect("plot");
        assert_eq!(plot.panels[4].points.len(), 10);
    }

    #[test]
    fn the_sixpack_reads_the_subgroup_column() {
        let df = DataFrame::new(vec![
            Column::new_f64(
                "weight",
                vec![10.0, 10.4, 10.2, 10.2, 10.6, 10.3, 9.9, 10.5, 10.7],
            ),
            Column::new_text(
                "part",
                vec!["A", "B", "C", "A", "B", "C", "A", "B", "C"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
        ])
        .expect("frame");
        let spec = AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
            .with_capability_sixpack(true)
            .with_capability_subgroup_column(Some("part".to_owned()));
        let out = run_analysis(&df, &spec).expect("sixpack from labels");
        let text = out.results[0].to_session_text();
        assert!(text.contains("subgroup column part"), "{text}");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panels.len(), 6);
        assert_eq!(plot.panels[2].title, "Subgroup mean");
        assert_eq!(plot.panels[3].title, "Sample range");
        assert_eq!(plot.panels[2].points.len(), 3);
    }

    #[test]
    fn the_sixpack_rejects_models_and_sizes_it_cannot_draw() {
        let values: Vec<f64> = (0..40)
            .map(|index| 10.0 + 0.2 * f64::from(index % 5))
            .collect();
        let df = DataFrame::new(vec![Column::new_f64("weight", values)]).expect("frame");

        let non_normal = AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
            .with_capability_sixpack(true)
            .with_capability_distribution(DistributionKind::Weibull);
        let error = run_analysis(&df, &non_normal).expect_err("non-normal Sixpack");
        assert!(error.to_string().contains("normal model"), "{error}");

        let box_cox = AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
            .with_capability_sixpack(true)
            .with_capability_box_cox(true);
        let error = run_analysis(&df, &box_cox).expect_err("Box-Cox Sixpack");
        assert!(error.to_string().contains("Box-Cox"), "{error}");

        let wide = AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
            .with_capability_sixpack(true)
            .with_capability_subgroup(26);
        let error = run_analysis(&df, &wide).expect_err("26-observation subgroups");
        assert!(error.to_string().contains("2 to 25"), "{error}");
    }

    #[test]
    fn non_normal_capability_reports_expected_and_observed_ppm() {
        let df = DataFrame::new(vec![Column::new_f64(
            "life",
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
        )])
        .expect("frame");
        let spec = AnalysisSpec::capability("life", Some(1.5), Some(7.5))
            .with_capability_distribution(DistributionKind::Weibull);
        let out = run_analysis(&df, &spec).expect("Weibull capability");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Capability Analysis: Weibull"), "{text}");
        assert!(text.contains("PPM < LSL"), "{text}");
        assert!(text.contains("PPM > USL"), "{text}");
        assert!(text.contains("PPM total"), "{text}");
        assert!(text.contains("Scale (β)"), "{text}");
        assert!(text.contains("Anderson–Darling (A²)"), "{text}");
        assert!(text.contains("Decision at 5%"), "{text}");
        assert!(text.contains("Observed PPM"), "{text}");
        assert!(text.contains("95% CI"), "{text}");
        assert!(text.contains("Nonparametric bootstrap"), "{text}");
        assert!(text.contains("Bootstrap expected PPM"), "{text}");
        let again = run_analysis(&df, &spec).expect("deterministic bootstrap");
        assert_eq!(text, again.results[0].to_session_text());
        assert!(
            out.plot
                .as_ref()
                .is_some_and(|plot| plot.title.contains("Capability Histogram"))
        );
    }

    #[test]
    fn weibull_capability_matches_the_documented_indices() {
        let values = vec![
            9.297698035,
            5.188917866,
            10.00382011,
            4.486709268,
            12.15748079,
            7.071756986,
            11.93892268,
            8.221635693,
            10.28177434,
            3.250955736,
            12.01911919,
            13.26757998,
            6.272874133,
            11.52933153,
            12.93590319,
            4.430935053,
            6.107360357,
            12.03072534,
            13.19264388,
            8.659288989,
        ];
        let df = DataFrame::new(vec![Column::new_f64("data", values)]).expect("frame");
        let spec = AnalysisSpec::capability("data", Some(5.0), Some(12.0))
            .with_capability_distribution(DistributionKind::Weibull);
        let text = run_analysis(&df, &spec)
            .expect("Weibull capability")
            .results[0]
            .to_session_text();
        for (label, value) in [
            ("Pp", "0.374"),
            ("PPL", "0.445"),
            ("PPU", "0.303"),
            ("Ppk", "0.303"),
        ] {
            let line = text
                .lines()
                .find(|line| line.trim_start().starts_with(&format!("{label} ")))
                .unwrap_or_else(|| panic!("{label} row missing:\n{text}"));
            assert!(line.contains(value), "{label}: {line}");
        }
    }

    #[test]
    fn lognormal_capability_matches_the_documented_moments() {
        let values = vec![
            1.605526965,
            2.119455038,
            3.679055506,
            3.083303189,
            2.384044187,
            2.286685052,
            3.284326246,
            1.273255801,
            4.004098924,
            3.329474906,
            3.421629474,
            3.259506625,
            1.861076927,
            2.089264493,
            1.847311858,
            4.349997071,
            2.209941199,
            4.799587709,
            3.249128513,
            4.228635952,
            3.876433247,
            2.193422902,
            1.671097565,
            4.293648234,
            1.968768626,
            1.663404001,
            4.972165022,
            2.290313227,
            2.479111138,
            2.221031909,
        ];
        let df = DataFrame::new(vec![Column::new_f64("data", values)]).expect("frame");
        let spec = AnalysisSpec::capability("data", Some(1.0), Some(5.0))
            .with_capability_distribution(DistributionKind::Lognormal);
        let text = run_analysis(&df, &spec)
            .expect("lognormal capability")
            .results[0]
            .to_session_text();
        for (label, value) in [
            ("Scale (σ)", "0.369528"),
            ("Pp", "0.726"),
            ("PPL", "0.891"),
            ("PPU", "0.561"),
            ("Ppk", "0.561"),
        ] {
            let line = text
                .lines()
                .find(|line| line.trim_start().starts_with(&format!("{label} ")))
                .unwrap_or_else(|| panic!("{label} row missing:\n{text}"));
            assert!(line.contains(value), "{label}: {line}");
        }
    }

    #[test]
    fn non_normal_capability_rejects_normal_distribution() {
        let df = DataFrame::new(vec![Column::new_f64("life", vec![1.0, 2.0, 3.0, 4.0])]).unwrap();
        assert!(matches!(
            capability_non_normal(
                &df,
                "life",
                Some(1.0),
                Some(5.0),
                DistributionKind::Normal,
                0.95,
                0
            ),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn capability_stability_screen_and_gate() {
        let stable = DataFrame::new(vec![Column::new_f64(
            "x",
            vec![10.0, 10.1, 9.9, 10.2, 9.8, 10.0, 10.1, 9.9, 10.05, 9.95],
        )])
        .unwrap();
        let base = AnalysisSpec::capability("x", Some(9.0), Some(11.0));
        let out = run_analysis(&stable, &base).expect("stable capability");
        assert!(out.results[0].to_session_text().contains("I–MR signals"));
        let gated = run_analysis(&stable, &base.clone().with_capability_stability(true))
            .expect("stable gate");
        assert!(gated.results[0].to_session_text().contains("I–MR signals"));

        let unstable = DataFrame::new(vec![Column::new_f64(
            "x",
            vec![10.0, 10.1, 9.9, 10.2, 9.8, 10.0, 20.0, 9.9, 10.05, 9.95],
        )])
        .unwrap();
        let spec =
            AnalysisSpec::capability("x", Some(9.0), Some(11.0)).with_capability_stability(true);
        assert!(matches!(
            run_analysis(&unstable, &spec),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn box_cox_capability_estimates_a_lambda_and_transforms_the_specification() {
        let df = DataFrame::new(vec![Column::new_f64(
            "weight",
            vec![1.0, 1.2, 1.5, 1.8, 2.2, 2.8, 3.5, 4.5, 5.5, 6.8],
        )])
        .unwrap();
        let spec =
            AnalysisSpec::capability("weight", Some(0.8), Some(7.5)).with_capability_box_cox(true);
        let out = run_analysis(&df, &spec).expect("Box-Cox capability");
        let text = out.results[0].to_session_text();
        assert!(
            text.contains("Capability Analysis: Normal (Box-Cox)"),
            "{text}"
        );
        assert!(text.contains("Box-Cox λ"), "{text}");
        assert!(text.contains("Transformed specification"), "{text}");
        assert!(text.contains("λ ="), "{text}");
        let again = run_analysis(&df, &spec).expect("deterministic");
        assert_eq!(text, again.results[0].to_session_text());
    }

    #[test]
    fn tolerance_interval_runs_through_the_workflow() {
        let values: Vec<f64> = (1..=30).map(|i| 10.0 + (i % 7) as f64 * 0.3).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let spec = AnalysisSpec::tolerance_interval("x")
            .with_tolerance_proportion(0.99)
            .with_tolerance_confidence(0.90);
        let out = run_analysis(&df, &spec).expect("analysis");
        assert!(out.plot.is_none());
        assert_eq!(out.results.len(), 1);
        let text = out.results[0].to_session_text();
        assert!(text.contains("Tolerance Interval"), "{text}");
        assert!(text.contains("99.0%"), "{text}");
        assert!(text.contains("90.0%"), "{text}");
        assert_eq!(
            spec.description(),
            "Tolerance interval (normal) — x, 99.0% / 90.0%"
        );
        // A wider content needs a wider interval.
        let loose = run_analysis(&df, &AnalysisSpec::tolerance_interval("x")).expect("default");
        assert!(out.results[0].statistics[3].1 < loose.results[0].statistics[3].1);
    }

    #[test]
    fn the_distribution_free_tolerance_interval_runs_through_the_workflow() {
        let values: Vec<f64> = (1..=120).map(|index| index as f64).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let spec = AnalysisSpec::tolerance_interval("x").with_tolerance_free(true);
        assert_eq!(
            spec.description(),
            "Tolerance interval (distribution-free) — x, 95.0% / 95.0%"
        );
        let out = run_analysis(&df, &spec).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Distribution-Free"), "{text}");
        // The normal method is still the default.
        let normal = AnalysisSpec::tolerance_interval("x");
        assert!(normal.description().contains("(normal)"));
    }

    #[test]
    fn tolerance_interval_rejects_a_missing_column() {
        let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0])]).expect("frame");
        assert!(matches!(
            run_analysis(&df, &AnalysisSpec::tolerance_interval("y")),
            Err(Error::ColumnNotFound { .. })
        ));
    }

    #[test]
    fn box_cox_rejects_non_positive_data() {
        let df = DataFrame::new(vec![Column::new_f64("weight", vec![0.0, 1.0, 2.0, 3.0])]).unwrap();
        assert!(matches!(
            run_analysis(
                &df,
                &AnalysisSpec::capability("weight", Some(1.0), Some(2.0))
                    .with_capability_box_cox(true)
            ),
            Err(Error::NonPositive { .. })
        ));
    }

    #[test]
    fn box_cox_rejects_non_positive_limits() {
        let df = DataFrame::new(vec![Column::new_f64("weight", vec![1.0, 2.0, 3.0, 4.0])])
            .expect("frame");
        let error = run_analysis(
            &df,
            &AnalysisSpec::capability("weight", Some(0.0), Some(4.0)).with_capability_box_cox(true),
        )
        .expect_err("the limit is not positive");
        assert!(
            error.to_string().contains("positive specification limits"),
            "{error}"
        );
    }

    #[test]
    fn box_cox_is_rejected_for_non_normal_capability() {
        let df = DataFrame::new(vec![Column::new_f64("life", vec![1.0, 2.0, 3.0, 4.0])]).unwrap();
        assert!(matches!(
            run_analysis(
                &df,
                &AnalysisSpec::capability("life", Some(1.0), Some(4.0))
                    .with_capability_distribution(DistributionKind::Weibull)
                    .with_capability_box_cox(true)
            ),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn capability_requires_limits_and_a_positive_spread() {
        let df =
            DataFrame::new(vec![Column::new_f64("weight", vec![10.0, 11.0, 9.0])]).expect("frame");
        assert!(matches!(
            run_analysis(&df, &AnalysisSpec::capability("weight", None, None)),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            run_analysis(
                &df,
                &AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
                    .with_capability_confidence(1.0)
            ),
            Err(Error::InvalidParameter { .. })
        ));
        let constant =
            DataFrame::new(vec![Column::new_f64("weight", vec![10.0, 10.0, 10.0])]).expect("frame");
        assert!(matches!(
            run_analysis(
                &constant,
                &AnalysisSpec::capability("weight", Some(8.0), Some(12.0))
            ),
            Err(Error::ConstantSample)
        ));
    }

    #[test]
    fn descriptive_statistics_reports_quartiles_and_mean_interval() {
        let spec = AnalysisSpec::descriptive_statistics("weight");
        let out = run_analysis(&frame(), &spec).expect("descriptive statistics");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Descriptive Statistics"));
        assert!(text.contains("First quartile (Q1)"));
        assert!(text.contains("Interquartile range"));
        assert!(text.contains("95% CI for the mean (lower)"));
        assert!(text.contains("Standard deviation (n−1)"));
        assert!(out.plot.is_none());
    }

    #[test]
    fn descriptive_statistics_can_be_split_by_a_group_column() {
        let df = DataFrame::new(vec![
            Column::new_f64("weight", vec![1.0, 2.0, 3.0, 10.0, 11.0, 12.0]),
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
        .unwrap();
        let out = run_analysis(
            &df,
            &AnalysisSpec::descriptive_statistics_by("weight", "batch"),
        )
        .expect("grouped statistics");
        assert_eq!(out.results.len(), 2);
        assert!(out.results[0].title.contains("batch = A"));
        assert!(out.results[1].title.contains("batch = B"));
        assert_eq!(out.results[0].n, 3);
        assert_eq!(out.results[1].n, 3);
        assert!(out.plot.is_none());
    }

    #[test]
    fn descriptive_statistics_of_one_observation_omits_sd_and_ci() {
        let df = DataFrame::new(vec![Column::new_f64("one", vec![42.0])]).unwrap();
        let out = run_analysis(&df, &AnalysisSpec::descriptive_statistics("one")).unwrap();
        let text = out.results[0].to_session_text();
        assert!(text.contains("n = 1"));
        assert!(!text.contains("Standard deviation (n−1)"));
        assert!(!text.contains("95% CI"));
    }

    #[test]
    fn descriptive_statistics_report_the_sd_interval() {
        let df = DataFrame::new(vec![Column::new_f64("x", vec![1.0, 2.0, 3.0, 4.0, 5.0])])
            .expect("frame");
        let out =
            run_analysis(&df, &AnalysisSpec::descriptive_statistics("x")).expect("descriptive");
        let text = out.results[0].to_session_text();
        assert!(
            text.contains("95% CI for the standard deviation (lower)"),
            "{text}"
        );
        assert!(text.contains("0.947313"), "{text}");
    }

    #[test]
    fn distribution_comparison_has_six_rows_for_positive_data() {
        let df = DataFrame::new(vec![Column::new_f64(
            "life",
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        )])
        .expect("frame");
        let spec = AnalysisSpec::distribution_comparison("life");
        let out = run_analysis(&df, &spec).expect("analysis");
        assert_eq!(out.results.len(), 1);
        assert!(out.plot.is_none());
        let text = out.results[0].to_session_text();
        for name in [
            "Normal",
            "Logistic",
            "Lognormal",
            "Loglogistic",
            "Weibull",
            "Exponential",
        ] {
            assert!(text.contains(name), "{name} missing:\n{text}");
        }
        assert!(text.contains("AIC"));
        assert!(text.contains("Distribution Comparison"));
    }

    #[test]
    fn the_best_distribution_rule_prefers_an_accepted_low_aic_fit() {
        let fits = vec![
            ("Normal".to_owned(), 0.2, 0.20, 120.0),
            ("Weibull".to_owned(), 0.4, 0.30, 110.0),
            ("Gamma".to_owned(), 0.1, 0.01, 90.0),
        ];
        assert_eq!(best_distribution(&fits), Some(("Weibull", true)));
        let rejected = vec![
            ("Normal".to_owned(), 1.2, 0.01, 90.0),
            ("Gamma".to_owned(), 0.9, 0.02, 95.0),
        ];
        assert_eq!(best_distribution(&rejected), Some(("Gamma", false)));
        assert_eq!(best_distribution(&[]), None);
    }

    #[test]
    fn distribution_comparison_reports_the_best_fit() {
        let df = DataFrame::new(vec![Column::new_f64(
            "life",
            vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        )])
        .expect("frame");
        let out =
            run_analysis(&df, &AnalysisSpec::distribution_comparison("life")).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Best fit:"), "{text}");
        assert!(text.contains("Selection rule"), "{text}");
    }

    #[test]
    fn distribution_comparison_skips_positive_families_for_non_positive_data() {
        let df =
            DataFrame::new(vec![Column::new_f64("life", vec![0.0, 1.0, 2.0, 3.0])]).expect("frame");
        let out =
            run_analysis(&df, &AnalysisSpec::distribution_comparison("life")).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Normal"));
        assert!(text.contains("Logistic"));
        assert!(text.contains("Loglogistic"));
        assert!(text.contains("x ≤ 0"));
        assert!(text.contains("skipped"));
    }

    #[test]
    fn the_dagostino_test_detects_a_skewed_sample() {
        let values: Vec<f64> = (1..=30).map(|i| (i as f64).powi(3)).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        let spec =
            AnalysisSpec::normality_test("x").with_normality_test(NormalityTestKind::DAgostinoK2);
        let out = run_analysis(&df, &spec).expect("normality");
        let text = out.results[0].to_session_text();
        assert!(text.contains("D'Agostino"), "{text}");
        assert!(text.contains("K²"), "{text}");
        assert!(text.contains("Skewness"), "{text}");

        let normal: Vec<f64> = (1..=30).map(|i| i as f64).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", normal)]).expect("frame");
        let out = run_analysis(&df, &spec).expect("normality");
        let text = out.results[0].to_session_text();
        assert!(text.contains("p-value"), "{text}");
    }

    #[test]
    fn normality_test_spec_produces_a_report_without_a_plot() {
        let spec = AnalysisSpec::normality_test("weight");
        assert!(spec.description().contains("Anderson–Darling"));
        let out = run_analysis(&frame(), &spec).expect("analysis");
        let result = &out.results[0];
        assert!(result.title.contains("Normality Test"));
        assert!(result.title.contains("Anderson–Darling"));
        assert!(result.method.contains("Sample moments"));
        assert!(result.to_session_text().contains("Anderson–Darling"));
        assert!(out.plot.is_none());
    }

    #[test]
    fn normality_percentile_lines_are_not_silently_ignored() {
        let spec = AnalysisSpec::NormalityTest {
            column: "weight".to_owned(),
            test: NormalityTestKind::AndersonDarling,
            percentile_lines: true,
        };
        assert!(matches!(
            run_analysis(&frame(), &spec),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn ryan_joiner_reports_a_monte_carlo_p_value() {
        let df = DataFrame::new(vec![Column::new_f64(
            "x",
            vec![
                -1.2, -0.8, -0.5, -0.2, 0.0, 0.1, 0.3, 0.6, 0.9, 1.1, -0.3, 0.2, 0.4, -0.7, 0.8,
                -1.0, 0.5, -0.1, 1.3, -0.4,
            ],
        )])
        .unwrap();
        let spec =
            AnalysisSpec::normality_test("x").with_normality_test(NormalityTestKind::RyanJoiner);
        let out = run_analysis(&df, &spec).expect("RJ");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Ryan–Joiner"), "{text}");
        assert!(text.contains("Monte Carlo RJ"), "{text}");
    }

    #[test]
    fn ryan_joiner_detects_a_strongly_skewed_sample() {
        let df = DataFrame::new(vec![Column::new_f64(
            "x",
            (1..=40).map(|value| (value as f64).powi(3)).collect(),
        )])
        .unwrap();
        let out = run_analysis(
            &df,
            &AnalysisSpec::normality_test("x").with_normality_test(NormalityTestKind::RyanJoiner),
        )
        .expect("RJ");
        let p_line = out.results[0]
            .gof
            .iter()
            .find(|(name, _)| *name == "p-value (Monte Carlo RJ)")
            .map(|(_, value)| value)
            .unwrap();
        let p: f64 = p_line.split_whitespace().next().unwrap().parse().unwrap();
        assert!(p < 0.05, "p={p}");
    }

    #[test]
    fn the_ks_replications_shrink_on_large_samples() {
        assert_eq!(ks_replications(20), 1000);
        assert_eq!(ks_replications(5000), 1000);
        assert_eq!(ks_replications(50_000), 100);
        assert_eq!(ks_replications(200_000), 50);
        assert_eq!(ks_replications(0), 1000);
    }

    #[test]
    fn kolmogorov_smirnov_reports_a_monte_carlo_p_value() {
        let df = DataFrame::new(vec![Column::new_f64(
            "x",
            vec![
                -1.2, -0.8, -0.5, -0.2, 0.0, 0.1, 0.3, 0.6, 0.9, 1.1, -0.3, 0.2, 0.4, -0.7, 0.8,
                -1.0, 0.5, -0.1, 1.3, -0.4,
            ],
        )])
        .unwrap();
        let spec = AnalysisSpec::normality_test("x")
            .with_normality_test(NormalityTestKind::KolmogorovSmirnov);
        let out = run_analysis(&df, &spec).expect("KS");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Kolmogorov–Smirnov"), "{text}");
        assert!(text.contains("Monte Carlo KS"), "{text}");
        assert!(text.contains("1000 replications"), "{text}");
    }

    #[test]
    fn kolmogorov_smirnov_detects_a_strongly_skewed_sample() {
        let df = DataFrame::new(vec![Column::new_f64(
            "x",
            (1..=40).map(|value| (value as f64).powi(3)).collect(),
        )])
        .unwrap();
        let out = run_analysis(
            &df,
            &AnalysisSpec::normality_test("x")
                .with_normality_test(NormalityTestKind::KolmogorovSmirnov),
        )
        .expect("KS");
        let p_line = out.results[0]
            .gof
            .iter()
            .find(|(name, _)| *name == "p-value (Monte Carlo KS)")
            .map(|(_, value)| value)
            .unwrap();
        let p: f64 = p_line.split_whitespace().next().unwrap().parse().unwrap();
        assert!(p < 0.05, "p={p}");
    }

    #[test]
    fn monte_carlo_is_available() {
        let out = normal_probability(
            &frame(),
            "weight",
            ProbabilityPlotOptions {
                method: PlottingPosition::Benard,
                p_value: PValueMethod::MonteCarlo { reps: 20, seed: 7 },
                confidence: Some(0.95),
                display: PlotDisplay::Both,
            },
        )
        .expect("analysis");
        assert!(out.result.to_session_text().contains("Monte Carlo"));
    }
}
