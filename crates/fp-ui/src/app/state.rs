//! Application state: construction, defaults and a fresh worksheet tab.

use super::*;

impl Default for FreePlotsApp {
    fn default() -> Self {
        Self::new()
    }
}

impl FreePlotsApp {
    /// Empty workspace.
    #[must_use]
    pub fn new() -> Self {
        let (action_tx, action_rx) = std::sync::mpsc::channel();
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
            conditional_rules: Vec::new(),
            show_conditional_dialog: false,
            conditional_column: String::new(),
            conditional_kind: ConditionalKind::Greater,
            conditional_value: String::new(),
            conditional_value2: String::new(),
            conditional_style: ConditionalStyle::RedFill,
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
            pareto_weight: String::new(),
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
            two_way_interaction: false,
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
            show_overwrite: false,
            preferences_path: None,
            saved_preferences: None,
            preferences_error: None,
            show_agents_dialog: false,
            show_settings_info: false,
            show_licences: false,
            show_tour: false,
            tour_step: 0,
            tour_seen: false,
            graph_overlay: fp_core::plot::GraphOverlay::default(),
            graph_auto: None,
            show_graph_labels: false,
            graph_label_title: String::new(),
            graph_label_subtitle: String::new(),
            graph_label_footer: String::new(),
            graph_label_x: String::new(),
            graph_label_y: String::new(),
            show_graph_text: false,
            graph_text_family: fp_core::plot::GraphFontFamily::Sans,
            graph_text_size: 12.0,
            show_graph_annotation: false,
            annotation_text: String::new(),
            annotation_x: 5.0,
            annotation_y: 5.0,
            show_reference_line: false,
            reference_axis: fp_core::plot::ReferenceAxis::Vertical,
            reference_value: String::new(),
            reference_label: String::new(),
            plot_action_tx: action_tx,
            plot_action_rx: action_rx,
            overwrite_pending: None,
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
    pub(super) fn blank_sheet(name: impl Into<String>) -> SheetState {
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
}
