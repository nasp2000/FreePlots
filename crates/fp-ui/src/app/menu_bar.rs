//! Menu bar of the desktop window.
//!
//! Every menu, submenu and item lives in this file; the actions call the same
//! methods as the toolbar, the dialogs and the assistant, so nothing here
//! computes a statistic.

use super::*;

impl FreePlotsApp {
    pub(super) fn menu_bar(&mut self, ui: &mut egui::Ui) {
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
                    ToolIcon::Grid,
                    self.catalog().label("Batch descriptive statistics…"),
                    None,
                    true,
                )
                .on_hover_text("Summarise one column across every CSV in a folder")
                .clicked()
                {
                    self.open_batch_dialog();
                    ui.close();
                }
                help_menu_button(ui, self.catalog().label("Sample projects"), |ui| {
                    for label in [
                        "Filling line capability",
                        "Gage R&R (3 operators)",
                        "Screening experiment (2^2)",
                    ] {
                        if menu_text_item(ui, self.catalog().label(label), None, true).clicked() {
                            self.load_sample(label);
                            ui.close();
                        }
                    }
                });
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
                if menu_icon_item(
                    ui,
                    ToolIcon::Grid,
                    self.catalog().label("Conditional formatting…"),
                    None,
                    has_data,
                )
                .clicked()
                {
                    self.open_conditional_dialog();
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
                    self.catalog().label("Show the tour"),
                    None,
                    true,
                )
                .on_hover_text("A short guided tour of the window")
                .clicked()
                {
                    self.begin_tour();
                    ui.close();
                }
                ui.separator();
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
                if menu_icon_item(
                    ui,
                    ToolIcon::Info,
                    self.catalog().label("Where the settings are stored…"),
                    None,
                    true,
                )
                .on_hover_text("The file that keeps the options between runs")
                .clicked()
                {
                    self.show_settings_info = true;
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Info,
                    self.catalog().label("Assistant for agents…"),
                    None,
                    true,
                )
                .on_hover_text("How to connect an AI client through MCP")
                .clicked()
                {
                    self.show_agents_dialog = true;
                    ui.close();
                }
                if menu_icon_item(
                    ui,
                    ToolIcon::Info,
                    self.catalog().label("Third-party licences…"),
                    None,
                    true,
                )
                .on_hover_text("The notices that must accompany every copy")
                .clicked()
                {
                    self.show_licences = true;
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
}
