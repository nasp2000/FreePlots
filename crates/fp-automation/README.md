# fp-automation

Semantic automation of the real FreePlots interface, used by the automation
tools and by `fp-mcp`.

```rust
use fp_automation::Automation;

let mut ui = Automation::new();
ui.load_csv("defects.csv")?;
ui.open_menu(&["Stat", "Control Charts", "P/NP/C/U Chart…"])?;
ui.select("Chart:", "P")?;
ui.select("Defects column:", "defects")?;
ui.set_text("Constant size:", "50")?;
ui.click("Run")?;
assert!(ui.session().contains("P Chart"));
```

`Automation` wraps `egui_kittest` with `FreePlotsApp` as the state. Frames run
headless; widgets are found by their AccessKit label. Menu paths are arrays
because labels such as `P/NP/C/U Chart…` contain slashes.

The dialogs label each control explicitly (`ui.label(...).labelled_by(...)`);
`unlabeled_widgets()` lists the controls that still need a label. Screenshots
render through `wgpu` and are written as PNG.
