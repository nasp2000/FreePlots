# FreePlots MCP

A thin MCP adapter over the semantic UI automation in `fp-automation`. It
drives the real FreePlots widgets through AccessKit, headless: menus, fields,
combo boxes, buttons, the Session report and PNG screenshots.

## Build and run

```bash
cargo build -p fp-mcp
./target/debug/freeplots-mcp
```

The server speaks newline-delimited JSON-RPC 2.0 on stdin/stdout (the MCP
stdio transport). Diagnostics go to stderr.

## Client configuration

```json
{
  "mcpServers": {
    "freeplots": {
      "command": "/path/to/FreePlots/target/debug/freeplots-mcp"
    }
  }
}
```

## Tools

| Tool | Purpose |
| --- | --- |
| `freeplots_open_csv` | Open a CSV file. |
| `freeplots_menu` | Click a menu path, e.g. `["Stat","Control Charts","P/NP/C/U Chart…"]`. |
| `freeplots_click` | Click a widget by its accessible label. |
| `freeplots_set_text` | Replace a text field (click, select all, type). |
| `freeplots_select` | Open a combo box and choose an option. |
| `freeplots_press_key` | Escape / Enter / Tab. |
| `freeplots_windows` | Titles of the open dialogs. |
| `freeplots_widgets` | Visible widgets with role, label, value and bounds. |
| `freeplots_unlabeled` | Interactive widgets without a label (the labelling checklist). |
| `freeplots_session` | The Session report. |
| `freeplots_status` | The status bar (errors appear here). |
| `freeplots_plot` | Summary of the current plot. |
| `freeplots_screenshot` | Render the frame to a PNG. |
| `freeplots_run_script` | Batch menu/click/set_text/select steps. |

## Example

```json
{"action":"menu","path":["Stat","Quality Tools","Capability Analysis…"]}
{"action":"select","label":"Column:","option":"weight"}
{"action":"set_text","label":"LSL:","value":"22"}
{"action":"set_text","label":"USL:","value":"26"}
{"action":"click","label":"OK"}
```

After the batch, read `freeplots_session` and `freeplots_plot`.

## Design

The adapter adds no statistics and no window system. Each command goes through
the same `egui` widgets a person clicks; `egui_kittest` renders the frames and
AccessKit names the controls. Screenshots use `wgpu` (Mesa lavapipe works on a
headless Linux host).

When a control has no label, `freeplots_unlabeled` reports it. The fix is to
label the widget in `fp-ui` (`ui.label(...)` plus `response.labelled_by(...)`),
not to add coordinates to the adapter.
