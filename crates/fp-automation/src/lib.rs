//! Semantic automation of the real FreePlots interface.
//!
//! The automation drives the same `egui` widgets a person clicks, through
//! `egui_kittest` and AccessKit, but without a window. It is the layer used by
//! the native UI tests and by the optional `fp-mcp` adapter.
//!
//! A script is a sequence of stable, human-readable steps:
//!
//! ```no_run
//! # use fp_automation::Automation;
//! # fn main() -> Result<(), fp_automation::Error> {
//! let mut ui = Automation::new();
//! ui.load_csv("defects.csv")?;
//! ui.open_menu(&["Stat", "Control Charts", "P/NP/C/U Chart…"])?;
//! ui.select("Chart:", "P")?;
//! ui.select("Defects column:", "defects")?;
//! ui.set_text("Constant size:", "50")?;
//! ui.click("Run")?;
//! assert!(ui.session().contains("P Chart"));
//! # Ok(())
//! # }
//! ```
//!
//! Menu paths are arrays because labels such as `P/NP/C/U Chart…` contain
//! slashes. Widgets are found by their AccessKit label; the dialogs label each
//! control explicitly (`ui.label(...).labelled_by(...)`) so a field has one
//! stable name.

use std::fmt;
use std::path::Path;

use egui_kittest::kittest::{NodeT, Queryable};
use egui_kittest::{Harness, Node};
use fp_ui::FreePlotsApp;
use serde::Serialize;

/// Automation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

/// One visible widget of the current frame.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Widget {
    /// AccessKit role, e.g. `Button` or `ComboBox`.
    pub role: String,
    /// Accessible label.
    pub label: Option<String>,
    /// Accessible value.
    pub value: Option<String>,
    /// Bounding box in logical points: `[x0, y0, x1, y1]`.
    pub rect: [f32; 4],
}

/// A headless FreePlots session driven through the real interface.
pub struct Automation {
    harness: Harness<'static, FreePlotsApp>,
}

impl fmt::Debug for Automation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Automation")
            .field("status", &self.status())
            .finish_non_exhaustive()
    }
}

impl Default for Automation {
    fn default() -> Self {
        Self::new()
    }
}

impl Automation {
    /// Starts an empty session.
    #[must_use]
    pub fn new() -> Self {
        Self::with_size(1400.0, 900.0)
    }

    /// Starts an empty session with the given logical window size.
    #[must_use]
    pub fn with_size(width: f32, height: f32) -> Self {
        let mut harness = Harness::builder()
            .with_size(egui::vec2(width, height))
            .build_ui_state(|ui, app| app.show(ui), FreePlotsApp::new());
        harness.run();
        Self { harness }
    }

    /// Loads a CSV file through the same reader the desktop runner uses.
    pub fn load_csv(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref();
        let (frame, _) =
            fp_core::io::read_csv_file(path).map_err(|error| Error::new(error.to_string()))?;
        self.harness
            .state_mut()
            .set_data(frame, path.display().to_string());
        self.harness.run();
        Ok(())
    }

    /// Loads an in-memory frame.
    pub fn load_frame(&mut self, frame: fp_core::data::DataFrame, source: impl Into<String>) {
        self.harness.state_mut().set_data(frame, source.into());
        self.harness.run();
    }

    /// Clicks a menu button and every submenu on the path, then the last item.
    ///
    /// Each segment is matched by its exact AccessKit label.
    pub fn open_menu(&mut self, path: &[&str]) -> Result<(), Error> {
        let Some((last, rest)) = path.split_last() else {
            return Err(Error::new("the menu path is empty"));
        };
        for segment in rest {
            self.click(segment)?;
        }
        self.click(last)
    }

    /// Clicks the first visible widget with the given label.
    pub fn click(&mut self, label: &str) -> Result<(), Error> {
        let mut found = false;
        {
            if let Some(node) = self.visible(label) {
                // A check box exposes a toggle, not a click, to AccessKit;
                // a pointer click on its centre flips it.
                if matches!(
                    node.accesskit_node().role(),
                    egui::accesskit::Role::CheckBox | egui::accesskit::Role::RadioButton
                ) {
                    node.click();
                } else {
                    node.click_accesskit();
                }
                found = true;
            }
        }
        if !found {
            return Err(self.missing(label));
        }
        self.harness.run();
        Ok(())
    }

    /// Replaces the text of a field with the given label.
    pub fn set_text(&mut self, label: &str, value: &str) -> Result<(), Error> {
        let mut found = false;
        {
            if let Some(node) = self.visible(label) {
                // A pointer click focuses a text field; the AccessKit click
                // action opens a combo box. Fields take the pointer path.
                node.click();
                found = true;
            }
        }
        if !found {
            return Err(self.missing(label));
        }
        self.harness.run();
        self.harness
            .key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        self.harness.run();
        if value.is_empty() {
            self.harness.key_press(egui::Key::Backspace);
            self.harness.run();
        }
        {
            if let Some(node) = self.visible(label) {
                node.type_text(value);
            }
        }
        self.harness.run();
        Ok(())
    }

    /// Opens a combo box with the given label and clicks one of its options.
    ///
    /// The option is selected through AccessKit, which also works when a long
    /// list is clipped by the dialog. The popup that stays open is then
    /// closed with a real pointer click on the combo, because a stale popup
    /// would swallow the next interaction.
    pub fn select(&mut self, label: &str, option: &str) -> Result<(), Error> {
        self.click(label)?;
        {
            let Some(node) = self.visible(option) else {
                return Err(self.missing(option));
            };
            // The option can be clipped by the dialog when the list is long;
            // the AccessKit action still selects it.
            node.click_accesskit();
        }
        self.harness.run();
        // A pointer click on the combo closes the popup it left open. The
        // popup is only open when its option is still a button in the tree.
        let popup_open = self
            .widgets()
            .iter()
            .any(|widget| widget.role == "Button" && widget.label.as_deref() == Some(option));
        if popup_open && let Some(combo) = self.visible(label) {
            combo.click();
            self.harness.run();
        }
        Ok(())
    }

    /// Sends a real pointer click at a point.
    pub fn click_at(&mut self, pos: egui::Pos2) {
        self.pointer_click(pos);
    }

    /// Presses a key (for example Escape to close a dialog).
    pub fn press_key(&mut self, key: egui::Key) {
        self.harness.key_press(key);
        self.harness.run();
    }

    /// Titles of the open dialog windows.
    #[must_use]
    pub fn window_titles(&self) -> Vec<String> {
        self.widgets()
            .into_iter()
            .filter(|widget| widget.role == "Window")
            .filter_map(|widget| widget.label)
            .collect()
    }

    /// The Session report.
    #[must_use]
    pub fn session(&self) -> &str {
        self.harness.state().session_text()
    }

    /// The status bar.
    #[must_use]
    pub fn status(&self) -> &str {
        self.harness.state().status()
    }

    /// The current plot, when an analysis produced one.
    #[must_use]
    pub fn plot(&self) -> Option<&fp_core::plot::PlotSpec> {
        self.harness.state().plot()
    }

    /// The application state, for assertions.
    #[must_use]
    pub fn app(&self) -> &FreePlotsApp {
        self.harness.state()
    }

    /// The application state, for direct setup.
    pub fn app_mut(&mut self) -> &mut FreePlotsApp {
        self.harness.state_mut()
    }

    /// Every visible widget with a label or a value.
    #[must_use]
    pub fn widgets(&self) -> Vec<Widget> {
        self.harness
            .root()
            .children_recursive()
            .filter_map(|node| {
                let node = node.accesskit_node();
                let rect = node.bounding_box()?;
                let label = node.label();
                let value = node.value();
                if label.is_none() && value.is_none() {
                    return None;
                }
                Some(Widget {
                    role: format!("{:?}", node.role()),
                    label,
                    value,
                    rect: [
                        rect.x0 as f32,
                        rect.y0 as f32,
                        rect.x1 as f32,
                        rect.y1 as f32,
                    ],
                })
            })
            .collect()
    }

    /// Widgets inside the topmost dialog window.
    #[must_use]
    pub fn dialog_widgets(&self) -> Vec<Widget> {
        let widgets = self.widgets();
        let Some(window) = widgets.iter().rev().find(|widget| widget.role == "Window") else {
            return Vec::new();
        };
        let [left, top, right, bottom] = window.rect;
        widgets
            .into_iter()
            .filter(|widget| {
                let [x0, y0, x1, y1] = widget.rect;
                x0 >= left - 1.0 && y0 >= top - 1.0 && x1 <= right + 1.0 && y1 <= bottom + 1.0
            })
            .collect()
    }

    /// Interactive widgets of the topmost dialog that have no accessible label.
    ///
    /// These are the controls the automation cannot name yet; the audit is the
    /// checklist for labelling the dialogs.
    #[must_use]
    pub fn unlabeled_widgets(&self) -> Vec<Widget> {
        const INTERACTIVE: &[&str] = &[
            "Button",
            "ComboBox",
            "TextInput",
            "CheckBox",
            "RadioButton",
            "Slider",
            "SpinButton",
        ];
        self.dialog_widgets()
            .into_iter()
            .filter(|widget| widget.label.is_none() && INTERACTIVE.contains(&widget.role.as_str()))
            .collect()
    }

    /// Renders the current frame to a PNG.
    ///
    /// Only available with the `wgpu-render` feature, which pulls a GPU stack
    /// into the binary. The default build screenshots the live window instead.
    #[cfg(feature = "wgpu-render")]
    pub fn screenshot(&mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        let image = self.harness.render().map_err(Error::new)?;
        image
            .save(path.as_ref())
            .map_err(|error| Error::new(error.to_string()))
    }

    /// Headless screenshots are not compiled in this build.
    #[cfg(not(feature = "wgpu-render"))]
    pub fn screenshot(&mut self, _path: impl AsRef<Path>) -> Result<(), Error> {
        Err(Error::new(
            "headless screenshots are not compiled in; use the live window bridge",
        ))
    }

    /// Runs frames until the interface is stable.
    pub fn settle(&mut self) {
        self.harness.run();
    }

    fn visible<'a>(&'a self, label: &'a str) -> Option<Node<'a>> {
        let matches = |node: &Node<'_>| {
            let accesskit = node.accesskit_node();
            accesskit.bounding_box().is_some()
                && accesskit.label().is_some_and(|text| {
                    // Submenu buttons carry a trailing arrow in AccessKit.
                    text == label
                        || text.strip_suffix(" \u{23f5}") == Some(label)
                        || text.strip_suffix(" \u{25b6}") == Some(label)
                })
        };
        // A dialog wins over the toolbar: several controls share a label with
        // the worksheet chrome ("Distribution:", "Column:"). The dialog is
        // painted last, so its control is the last match in tree order. A
        // control also wins over the static label that names it.
        if let Some([left, top, right, bottom]) = self.window_rect() {
            let inside = |node: &Node<'_>| {
                if !matches(node) {
                    return false;
                }
                let rect = node.rect();
                rect.left() >= left - 1.0
                    && rect.top() >= top - 1.0
                    && rect.right() <= right + 1.0
                    && rect.bottom() <= bottom + 1.0
            };
            if let Some(node) = self
                .harness
                .query_all_by_label_contains(label)
                .rfind(|node| inside(node) && interactive(node))
            {
                return Some(node);
            }
            if let Some(node) = self
                .harness
                .query_all_by_label_contains(label)
                .rfind(inside)
            {
                return Some(node);
            }
        }
        if let Some(node) = self
            .harness
            .query_all_by_label_contains(label)
            .find(|node| matches(node) && interactive(node))
        {
            return Some(node);
        }
        self.harness
            .query_all_by_label_contains(label)
            .find(matches)
    }

    fn pointer_click(&mut self, pos: egui::Pos2) {
        self.harness.event(egui::Event::PointerMoved(pos));
        self.harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        self.harness.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
        self.harness.run();
    }

    fn window_rect(&self) -> Option<[f32; 4]> {
        self.widgets()
            .into_iter()
            .rev()
            .find(|widget| widget.role == "Window")
            .map(|widget| widget.rect)
    }

    fn missing(&self, label: &str) -> Error {
        let known: Vec<String> = self
            .widgets()
            .into_iter()
            .filter_map(|widget| widget.label)
            .take(12)
            .collect();
        Error::new(format!(
            "no visible widget labelled {label:?}; visible labels: {known:?}"
        ))
    }
}

fn interactive(node: &Node<'_>) -> bool {
    matches!(
        format!("{:?}", node.accesskit_node().role()).as_str(),
        "Button"
            | "ComboBox"
            | "TextInput"
            | "CheckBox"
            | "RadioButton"
            | "Slider"
            | "SpinButton"
            | "ToggleButton"
    )
}
