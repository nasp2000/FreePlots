//! Compile-time plugin registry.
//!
//! A plugin is an analysis that lives outside the built-in analyses but is
//! compiled into the same binary. The engine owns the registry and the trait;
//! a plugin crate only has to implement [`Plugin`] and call
//! [`register_plugins!`], so the registry stays extensible without a dynamic
//! loader and without a dependency.
//!
//! ```ignore
//! impl Plugin for MyPlugin {
//!     fn id(&self) -> &'static str { "my-plugin" }
//!     fn name(&self) -> &'static str { "My analysis" }
//!     fn description(&self) -> &'static str { "Does something useful" }
//!     fn run(&self, df: &DataFrame, options: &PluginOptions) -> Result<AnalysisOutput> { .. }
//! }
//!
//! register_plugins!(MyPlugin::default());
//! ```

use std::sync::{Mutex, OnceLock};

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::report::{AnalysisResult, Item};
use crate::workflow::AnalysisOutput;

/// Options passed to a plugin.
///
/// The command line sends the `--option value` pairs; the desktop application
/// sends the values of its dialog.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginOptions {
    /// The options, in the order they were given.
    pub values: Vec<(String, String)>,
}

impl PluginOptions {
    /// Builds the options from pairs.
    #[must_use]
    pub fn new(values: Vec<(String, String)>) -> Self {
        Self { values }
    }

    /// The value of an option, if it was given.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

/// An analysis that is compiled into the binary but is not built in.
pub trait Plugin: Send + Sync {
    /// Stable identifier used on the command line.
    fn id(&self) -> &'static str;

    /// Name shown in the menus.
    fn name(&self) -> &'static str;

    /// One-line description of the analysis.
    fn description(&self) -> &'static str;

    /// Runs the plugin on a worksheet.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] when the worksheet does not hold the data the plugin
    /// needs.
    fn run(&self, df: &DataFrame, options: &PluginOptions) -> Result<AnalysisOutput>;
}

/// The plugins known to the binary.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl std::fmt::Debug for PluginRegistry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PluginRegistry")
            .field("plugins", &self.plugin_ids())
            .finish()
    }
}

impl PluginRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Adds a plugin; an identifier that is already used is ignored.
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let id = plugin.id();
        if self.plugins.iter().any(|existing| existing.id() == id) {
            return;
        }
        self.plugins.push(plugin);
    }

    /// The identifiers of the registered plugins.
    #[must_use]
    pub fn plugin_ids(&self) -> Vec<&'static str> {
        self.plugins.iter().map(|plugin| plugin.id()).collect()
    }

    /// The registered plugins, as `(id, name, description)`.
    #[must_use]
    pub fn list(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        self.plugins
            .iter()
            .map(|plugin| (plugin.id(), plugin.name(), plugin.description()))
            .collect()
    }

    /// A plugin by identifier.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&dyn Plugin> {
        self.plugins
            .iter()
            .find(|plugin| plugin.id() == id)
            .map(|plugin| plugin.as_ref())
    }

    /// Runs a plugin by identifier.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] when the identifier is unknown.
    pub fn run(&self, id: &str, df: &DataFrame, options: &PluginOptions) -> Result<AnalysisOutput> {
        let plugin = self
            .get(id)
            .ok_or_else(|| Error::Unsupported(format!("unknown plugin: {id}")))?;
        plugin.run(df, options)
    }
}

/// Registers the plugins of a crate in the engine registry.
///
/// The macro generates a `register_plugins` function; the engine calls it
/// once, when the registry is built.
#[macro_export]
macro_rules! register_plugins {
    ($($plugin:expr),* $(,)?) => {
        /// Adds the plugins of this crate to the registry.
        pub fn register_plugins(registry: &mut $crate::plugin::PluginRegistry) {
            $( registry.register(Box::new($plugin)); )*
        }
    };
}

/// The global registry of the binary.
pub fn registry() -> &'static Mutex<PluginRegistry> {
    static REGISTRY: OnceLock<Mutex<PluginRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = PluginRegistry::new();
        register_builtin(&mut registry);
        Mutex::new(registry)
    })
}

/// The plugins that ship with the engine.
fn register_builtin(registry: &mut PluginRegistry) {
    registry.register(Box::new(DataQualityPlugin));
}

/// A plugin that summarises the quality of a worksheet.
///
/// It is the built-in implementation of the trait and a useful check of the
/// registry: it reports, for every column, the type, the number of missing
/// values and the number of distinct values.
#[derive(Debug, Clone, Copy, Default)]
pub struct DataQualityPlugin;

impl Plugin for DataQualityPlugin {
    fn id(&self) -> &'static str {
        "data-quality"
    }

    fn name(&self) -> &'static str {
        "Data quality summary"
    }

    fn description(&self) -> &'static str {
        "Missing values, distinct values and ranges of every column"
    }

    fn run(&self, df: &DataFrame, options: &PluginOptions) -> Result<AnalysisOutput> {
        let rows = df.n_rows();
        let mut result = AnalysisResult::new(
            format!("Data Quality — {} column(s)", df.n_columns()),
            self.description(),
        );
        result.n = rows;
        let mut table = Vec::new();
        let mut total_missing = 0usize;
        for column in df.columns() {
            let missing = (0..rows).filter(|row| column.is_missing(*row)).count();
            total_missing += missing;
            let distinct = distinct_values(column);
            let range = match column.numeric(crate::data::MissingPolicy::Drop) {
                Ok(values) if !values.is_empty() => {
                    let mut low = f64::INFINITY;
                    let mut high = f64::NEG_INFINITY;
                    for value in &values {
                        low = low.min(*value);
                        high = high.max(*value);
                    }
                    format!("{low:.6} – {high:.6}")
                }
                _ => String::new(),
            };
            table.push(vec![
                column.name().to_owned(),
                column.dtype().as_str().to_owned(),
                missing.to_string(),
                distinct.to_string(),
                range,
            ]);
        }
        result.statistics = vec![
            ("Rows", rows as f64),
            ("Columns", df.n_columns() as f64),
            ("Missing values", total_missing as f64),
        ];
        result.extra.push(Item::Table {
            headers: vec![
                "Column".to_owned(),
                "Type".to_owned(),
                "Missing".to_owned(),
                "Distinct".to_owned(),
                "Range".to_owned(),
            ],
            rows: table,
        });
        if let Some(column) = options.get("column") {
            result
                .notes
                .push(("Filtered column".to_owned(), column.to_owned()));
        }
        result
            .warnings
            .push("plugin output has not been compared with documented output".to_owned());
        Ok(AnalysisOutput {
            results: vec![result],
            plot: None,
        })
    }
}

/// Counts the distinct values of a column, ignoring the missing ones.
fn distinct_values(column: &crate::data::Column) -> usize {
    let mut values: Vec<String> = Vec::new();
    for row in 0..column.len() {
        if column.is_missing(row) {
            continue;
        }
        let text = column.cell_text(row);
        if !values.contains(&text) {
            values.push(text);
        }
    }
    values.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Column, DataFrame};

    fn frame() -> DataFrame {
        DataFrame::new(vec![
            Column::new_f64("x", vec![1.0, 2.0, 2.0, f64::NAN]),
            Column::new_text(
                "label",
                vec![
                    "a".to_owned(),
                    "b".to_owned(),
                    "a".to_owned(),
                    "c".to_owned(),
                ],
            ),
        ])
        .expect("frame")
    }

    #[test]
    fn the_registry_lists_and_runs_the_builtin_plugin() {
        let registry = registry().lock().expect("registry");
        assert!(registry.plugin_ids().contains(&"data-quality"));
        let out = registry
            .run("data-quality", &frame(), &PluginOptions::default())
            .expect("plugin");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Data Quality"), "{text}");
        assert!(text.contains("Missing values"), "{text}");
    }

    #[test]
    fn an_unknown_plugin_is_an_error() {
        let registry = registry().lock().expect("registry");
        let error = registry
            .run("nope", &frame(), &PluginOptions::default())
            .expect_err("unknown plugin");
        assert!(error.to_string().contains("unknown plugin"), "{error}");
    }

    #[test]
    fn options_are_read_by_name() {
        let options = PluginOptions::new(vec![("column".to_owned(), "x".to_owned())]);
        assert_eq!(options.get("column"), Some("x"));
        assert_eq!(options.get("COLUMN"), Some("x"));
        assert_eq!(options.get("other"), None);
    }
}
