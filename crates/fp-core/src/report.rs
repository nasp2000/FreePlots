//! Report: from typed results to the text of the Session window.
//!
//! The text is always generated from [`AnalysisResult`], never formatted at the
//! UI call sites. That is what guarantees that the Session, the SVG and the
//! `.fpp` tell the same story.

use crate::VERSION;
use crate::i18n::Catalog;

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Renders the Session text as a standalone HTML document.
///
/// The palette follows the light theme used by the desktop application.
#[must_use]
pub fn session_html(text: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>FreePlots Session</title><style>\
         :root{{--window:#f3f3f3;--paper:#fff;--text:#242424;--dim:#616161;--accent:#107c41;}}\
         *{{box-sizing:border-box}}body{{margin:0;background:var(--window);color:var(--text);\
         font-family:system-ui,-apple-system,'Segoe UI',sans-serif;padding:24px}}\
         main{{max-width:1100px;margin:0 auto;background:var(--paper);border:1px solid #e3e3e3;\
         border-radius:10px;box-shadow:0 4px 16px rgba(0,0,0,.08);overflow:hidden}}\
         header{{padding:14px 18px;border-bottom:1px solid #e3e3e3;color:var(--accent);\
         font-weight:700;font-size:19px}}pre{{margin:0;padding:18px;white-space:pre-wrap;\
         font:13px/1.5 ui-monospace,SFMono-Regular,Consolas,monospace;color:var(--text)}}\
         footer{{padding:8px 18px;border-top:1px solid #e3e3e3;color:var(--dim);font-size:12px}}\
         </style></head><body><main><header>Free Plots</header><pre>{}</pre>\
         <footer>FreePlots engine {VERSION}</footer></main></body></html>\n",
        escape_html(text)
    )
}

/// Extra structured content for the Session document.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A free-text line.
    Message(String),
    /// A table with a header row and string cells.
    Table {
        /// Column headers.
        headers: Vec<String>,
        /// Body rows; every row must have `headers.len()` cells.
        rows: Vec<Vec<String>>,
    },
}

/// An analysis result, with everything the report should show.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisResult {
    /// Title (e.g. `Probability Plot: Normal`).
    pub title: String,
    /// Estimation method, spelled out.
    pub method: String,
    /// Observations used.
    pub n: usize,
    /// Observations excluded.
    pub excluded: usize,
    /// Estimated parameters.
    pub parameters: Vec<(&'static str, f64)>,
    /// Relevant descriptive statistics.
    pub statistics: Vec<(&'static str, f64)>,
    /// Goodness-of-fit results (values already formatted, with the method).
    pub gof: Vec<(&'static str, String)>,
    /// Method notes (conventions, encoding, data policy).
    pub notes: Vec<(String, String)>,
    /// Extra structured content (tables, messages), rendered after the notes.
    pub extra: Vec<Item>,
    /// Warnings to the user.
    pub warnings: Vec<String>,
    /// Language of the report labels.
    pub lang: crate::i18n::Lang,
}

impl AnalysisResult {
    /// Creates an empty result, to be filled by the use case.
    #[must_use]
    pub fn new(title: impl Into<String>, method: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            method: method.into(),
            n: 0,
            excluded: 0,
            parameters: Vec::new(),
            statistics: Vec::new(),
            gof: Vec::new(),
            notes: Vec::new(),
            extra: Vec::new(),
            warnings: Vec::new(),
            lang: crate::i18n::Lang::default(),
        }
    }

    /// Sets the report language.
    pub fn set_lang(&mut self, lang: crate::i18n::Lang) {
        self.lang = lang;
    }

    /// Text of the Session window, in English.
    #[must_use]
    pub fn to_session_text(&self) -> String {
        self.to_session_text_lang(Catalog::new(self.lang))
    }

    /// Text of the Session window, in the catalogue's language.
    #[must_use]
    pub fn to_session_text_lang(&self, catalog: Catalog) -> String {
        let fixed_labels = [
            catalog.estimation_method(),
            catalog.observations(),
            catalog.excluded(),
        ];
        let width = fixed_labels
            .iter()
            .map(|k| k.chars().count())
            .chain(
                self.parameters
                    .iter()
                    .map(|(k, _)| catalog.label(k).chars().count()),
            )
            .chain(
                self.statistics
                    .iter()
                    .map(|(k, _)| catalog.label(k).chars().count()),
            )
            .chain(
                self.gof
                    .iter()
                    .map(|(k, _)| catalog.label(k).chars().count()),
            )
            .chain(self.notes.iter().map(|(k, _)| k.chars().count()))
            .max()
            .unwrap_or(20)
            .max(20);

        let mut out = String::new();
        out.push_str(&self.title);
        out.push('\n');
        out.push_str(&"=".repeat(self.title.chars().count().max(30)));
        out.push('\n');
        out.push_str(&format!("FreePlots engine {VERSION}\n\n"));

        out.push_str(&format!(
            "{:<width$} {}\n",
            catalog.estimation_method(),
            self.method
        ));
        out.push_str(&format!("{:<width$} {}\n", catalog.observations(), self.n));
        out.push_str(&format!(
            "{:<width$} {}\n",
            catalog.excluded(),
            self.excluded
        ));

        if !self.parameters.is_empty() {
            out.push_str(&format!("\n{}\n", catalog.estimated_parameters()));
            for (k, v) in &self.parameters {
                out.push_str(&format!("  {:<width$} {v:.6}\n", catalog.label(k)));
            }
        }

        if !self.statistics.is_empty() {
            out.push_str(&format!("\n{}\n", catalog.statistics()));
            for (k, v) in &self.statistics {
                out.push_str(&format!("  {:<width$} {v:.6}\n", catalog.label(k)));
            }
        }

        if !self.gof.is_empty() {
            out.push_str(&format!("\n{}\n", catalog.goodness_of_fit()));
            for (k, v) in &self.gof {
                out.push_str(&format!("  {:<width$} {v}\n", catalog.label(k)));
            }
        }

        if !self.notes.is_empty() {
            out.push_str(&format!("\n{}\n", catalog.method_notes()));
            for (k, v) in &self.notes {
                out.push_str(&format!("  {:<width$} {v}\n", catalog.label(k)));
            }
        }

        if !self.extra.is_empty() {
            out.push('\n');
            for item in &self.extra {
                match item {
                    Item::Message(message) => {
                        out.push_str(message);
                        out.push('\n');
                    }
                    Item::Table { headers, rows } => {
                        let mut widths: Vec<usize> =
                            headers.iter().map(|h| h.chars().count()).collect();
                        for row in rows {
                            for (index, cell) in row.iter().enumerate() {
                                if let Some(width) = widths.get_mut(index) {
                                    *width = (*width).max(cell.chars().count());
                                }
                            }
                        }
                        for (index, header) in headers.iter().enumerate() {
                            if index > 0 {
                                out.push_str("  ");
                            }
                            out.push_str(&format!(
                                "{:<width$}",
                                catalog.label(header),
                                width = widths[index]
                            ));
                        }
                        out.push('\n');
                        for (index, width) in widths.iter().enumerate() {
                            if index > 0 {
                                out.push_str("  ");
                            }
                            out.push_str(&"-".repeat(*width));
                        }
                        out.push('\n');
                        for row in rows {
                            for (index, cell) in row.iter().enumerate() {
                                if index > 0 {
                                    out.push_str("  ");
                                }
                                out.push_str(&format!("{cell:<width$}", width = widths[index]));
                            }
                            out.push('\n');
                        }
                        out.push('\n');
                    }
                }
            }
        }

        if !self.warnings.is_empty() {
            out.push_str(&format!("\n{}\n", catalog.warnings()));
            for w in &self.warnings {
                out.push_str(&format!("  * {w}\n"));
            }
        }

        out.push_str(&format!("\n{}\n", catalog.end_of_report()));
        out
    }
}

/// Standard warnings applicable to any univariate analysis.
///
/// Centralized here so that no UI path can omit them.
#[must_use]
pub fn standard_warnings(n: usize, excluded: usize, ties: usize) -> Vec<String> {
    let mut w = Vec::new();
    if n < 10 {
        w.push(format!(
            "very small sample (n = {n}): the fit and the plots carry little information"
        ));
    } else if n < 30 {
        w.push(format!(
            "small sample (n = {n} < 30): interpret with caution"
        ));
    }
    if excluded > 0 {
        w.push(format!(
            "{excluded} non-finite observations were excluded; the policy is recorded above"
        ));
    }
    if ties > 0 {
        let pct = 100.0 * ties as f64 / n.max(1) as f64;
        w.push(format!(
            "{ties} ties ({pct:.1}% of observations): plotting positions are approximate"
        ));
    }
    w
}
