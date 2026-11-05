//! Platform layer — **the only module that may talk to the operating
//! system** (PLAN.md, sections 2.1 and 5).
//!
//! Today it only resolves the data directory with portable semantics. When the
//! GUI arrives, this is where fonts, DPI and "open with the default application"
//! live — never in `fp-core`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Name of the data directory next to the executable.
pub const DATA_DIR_NAME: &str = "FreePlots-data";

/// Opens a file with the operating system's default application.
///
/// # Errors
///
/// Returns the spawn error when no handler can be started.
#[cfg(feature = "desktop")]
pub fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(path);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("{}: {error}", path.display()))
}

/// Reads a CSV file through the engine parser.
///
/// The UI never opens files; the desktop runner calls this function.
#[cfg(feature = "desktop")]
pub fn read_csv(path: &Path) -> Result<(fp_core::data::DataFrame, &'static str), String> {
    fp_core::io::read_csv_file(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads every `*.csv` in a folder, in name order, for the batch report.
///
/// A file that cannot be read is returned with its error so the report can
/// name it instead of hiding it.
#[cfg(feature = "desktop")]
pub fn read_csv_folder(path: &Path) -> Vec<(String, Result<fp_core::data::DataFrame, String>)> {
    let mut entries: Vec<_> = match std::fs::read_dir(path) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("csv"))
            })
            .collect(),
        Err(error) => {
            return vec![(
                path.display().to_string(),
                Err(format!("{}: {error}", path.display())),
            )];
        }
    };
    entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    entries
        .into_iter()
        .map(|file| {
            let name = file.file_name().map_or_else(
                || file.display().to_string(),
                |name| name.to_string_lossy().to_string(),
            );
            match read_csv(&file) {
                Ok((frame, _encoding)) => (name, Ok(frame)),
                Err(error) => (name, Err(error)),
            }
        })
        .collect()
}

/// Reads a CSV file with the options confirmed in the preview.
#[cfg(feature = "desktop")]
pub fn read_csv_with_options(
    path: &Path,
    options: fp_core::io::CsvOptions,
) -> Result<(fp_core::data::DataFrame, &'static str), String> {
    fp_core::io::read_csv_file_with_options(path, options)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads the first lines of a CSV file for the options preview.
#[cfg(feature = "desktop")]
pub fn read_csv_preview(path: &Path) -> Result<fp_core::io::CsvPreview, String> {
    fp_core::io::preview_csv_file(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads an Excel workbook and reports formula cells read as cached values.
///
/// # Errors
///
/// As [`read_xlsx_sheet_with_warnings`].
#[cfg(feature = "desktop")]
pub fn read_xlsx_with_warnings(
    path: &Path,
) -> Result<(fp_core::data::DataFrame, Vec<String>), String> {
    read_xlsx_sheet_with_warnings(path, None)
}

/// Reads a worksheet and reports formula cells that were read as cached values.
///
/// # Errors
///
/// # Errors
///
/// Returns the file or workbook error.
#[cfg(feature = "desktop")]
pub fn read_xlsx_sheet_with_warnings(
    path: &Path,
    sheet: Option<&str>,
) -> Result<(fp_core::data::DataFrame, Vec<String>), String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let df = fp_core::xlsx_read::read_workbook_sheet(&bytes, sheet)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let formulas = fp_core::xlsx_read::sheet_formula_count(&bytes, sheet).unwrap_or(0);
    let warnings = if formulas > 0 {
        vec![format!(
            "{formulas} formula cell(s) were read as their cached values; FreePlots does not \
             evaluate formulas"
        )]
    } else {
        Vec::new()
    };
    Ok((df, warnings))
}

/// Names of the worksheets of an Excel workbook.
#[cfg(feature = "desktop")]
pub fn xlsx_sheet_names(path: &Path) -> Result<Vec<String>, String> {
    let bytes = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    fp_core::xlsx_read::sheet_names(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes a worksheet as CSV.
///
/// The UI only emits a path request; this is the function that touches the
/// file system, keeping the UI crate portable.
#[cfg(feature = "desktop")]
pub fn write_csv(
    path: &Path,
    df: &fp_core::data::DataFrame,
    decimal_comma: bool,
) -> Result<(), String> {
    let text = fp_core::io::write_csv_string(df, decimal_comma)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes a standalone text/HTML export.
#[cfg(feature = "desktop")]
pub fn write_text(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes raw bytes (used by the PDF export).
#[cfg(feature = "desktop")]
pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Loads every worksheet of a `.fpp` project.
#[cfg(feature = "desktop")]
pub fn load_project_sheets(path: &Path) -> Result<Vec<(String, fp_core::data::DataFrame)>, String> {
    fp_core::io::load_project_sheets(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Saves every worksheet of a `.fpp` project.
#[cfg(feature = "desktop")]
pub fn save_project_sheets(
    path: &Path,
    sheets: &[(String, fp_core::data::DataFrame)],
    name: &str,
) -> Result<(), String> {
    fp_core::io::save_project_sheets(sheets, path, name)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Resolved data directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    /// Effective path.
    pub path: PathBuf,
    /// `true` if it is next to the executable (portable mode).
    pub portable: bool,
}

/// Resolves the data directory.
///
/// Rule (D3): try `.\FreePlots-data\` next to the executable — that is what
/// makes the USB stick self-contained. Only if it is not possible to write
/// there (for example, `Program Files`) does it fall back to
/// `%APPDATA%\FreePlots\` on Windows or `~/.local/share/FreePlots` elsewhere.
pub fn resolve_data_dir(exe_dir: &Path) -> std::io::Result<DataDir> {
    let portable_path = exe_dir.join(DATA_DIR_NAME);
    if prepare(&portable_path).is_ok() {
        return Ok(DataDir {
            path: portable_path,
            portable: true,
        });
    }

    let path = fallback_base().join("FreePlots");
    prepare(&path)?;
    Ok(DataDir {
        path,
        portable: false,
    })
}

fn prepare(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    // Probe writing: a directory may exist and be read-only.
    let probe = dir.join(".fp-write-probe");
    let mut f = fs::File::create(&probe)?;
    f.write_all(b"fp")?;
    drop(f);
    let _ = fs::remove_file(&probe);
    Ok(())
}

fn fallback_base() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata);
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local").join("share");
    }
    std::env::temp_dir()
}

/// The extension of the first filter, without the `*.` prefix.
///
/// `"CSV files (*.csv)|*.csv"` gives `"csv"`; `"*.html;*.htm"` gives `"html"`.
#[cfg(feature = "desktop")]
fn default_extension(filters: &[String]) -> String {
    filters
        .first()
        .and_then(|entry| entry.split_once('|'))
        .map(|(_, pattern)| pattern)
        .and_then(|pattern| pattern.split(';').next())
        .map(|pattern| pattern.trim().trim_start_matches("*.").trim().to_owned())
        .filter(|extension| !extension.is_empty() && !extension.contains('*'))
        .unwrap_or_default()
}

/// Appends the selected extension when the user did not type one.
#[cfg(feature = "desktop")]
fn with_default_extension(path: PathBuf, filters: &[String]) -> PathBuf {
    if path.extension().is_some() {
        return path;
    }
    let extension = default_extension(filters);
    if extension.is_empty() {
        path
    } else {
        path.with_extension(extension)
    }
}

/// Paints the main window caption black (Windows 11). Called once, after the
/// window exists.
#[cfg(all(feature = "desktop", target_os = "windows"))]
pub fn apply_dark_caption() {
    windows::apply_dark_caption();
}

#[cfg(all(feature = "desktop", target_os = "windows"))]
pub fn prepare_console(args: &[String]) {
    if args.is_empty() || args.first().map(String::as_str) == Some("--gui") {
        return;
    }
    windows::attach_console();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "desktop")]
    #[test]
    fn a_folder_of_csv_files_is_read_in_name_order() {
        let dir = std::env::temp_dir().join(format!("fp-batch-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("b.csv"), "weight\n2\n3\n").expect("b");
        fs::write(dir.join("a.csv"), "weight\n1\n2\n").expect("a");
        fs::write(dir.join("notes.txt"), "ignored").expect("txt");
        let files = read_csv_folder(&dir);
        assert_eq!(files.len(), 2, "{files:?}");
        assert_eq!(files[0].0, "a.csv");
        assert_eq!(files[1].0, "b.csv");
        assert!(files.iter().all(|(_, frame)| frame.is_ok()));
        // A missing folder is an error row, not a panic.
        let missing = read_csv_folder(&dir.join("does-not-exist"));
        assert_eq!(missing.len(), 1);
        assert!(missing[0].1.is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn a_missing_extension_is_added_from_the_filter() {
        let filters = vec!["CSV files (*.csv)|*.csv".to_owned()];
        assert_eq!(
            with_default_extension(PathBuf::from("report"), &filters),
            PathBuf::from("report.csv")
        );
        assert_eq!(
            with_default_extension(PathBuf::from("a.b/c"), &filters),
            PathBuf::from("a.b/c.csv")
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn an_existing_extension_is_kept() {
        let filters = vec!["CSV files (*.csv)|*.csv".to_owned()];
        assert_eq!(
            with_default_extension(PathBuf::from("report.txt"), &filters),
            PathBuf::from("report.txt")
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn the_first_pattern_of_a_multi_extension_filter_wins() {
        let filters = vec!["HTML files (*.html)|*.html;*.htm".to_owned()];
        assert_eq!(default_extension(&filters), "html");
        assert_eq!(
            with_default_extension(PathBuf::from("page"), &filters),
            PathBuf::from("page.html")
        );
        assert_eq!(
            with_default_extension(PathBuf::from("page"), &["All files (*.*)|*.*".to_owned()]),
            PathBuf::from("page")
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn an_xlsx_written_by_the_runner_reads_back() {
        let dir = temp_dir("xlsx");
        let path = dir.join("sheet.xlsx");
        let df = fp_core::data::DataFrame::new(vec![
            fp_core::data::Column::new_f64("x", vec![1.5, 2.5, 3.5]),
            fp_core::data::Column::new_text(
                "g",
                vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
            ),
        ])
        .expect("frame");
        write_bytes(&path, &fp_core::xlsx::workbook(&df)).expect("write xlsx");
        let (back, warnings) = read_xlsx_with_warnings(&path).expect("read xlsx");
        assert_eq!(back.n_columns(), 2);
        assert_eq!(back.n_rows(), 3);
        assert_eq!(back.columns()[0].name(), "x");
        assert_eq!(back.cell_text(1, 2).as_deref(), Some("c"));
        assert!(warnings.is_empty());
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fp-platform-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos())
        ));
        fs::create_dir_all(&dir).expect("create temp");
        dir
    }

    #[test]
    fn creates_portable_directory_next_to_the_exe() {
        let base = temp_dir("portable");
        let data = resolve_data_dir(&base).expect("resolve");
        assert!(data.portable, "should be portable");
        assert_eq!(data.path, base.join(DATA_DIR_NAME));
        assert!(data.path.is_dir());
        fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn second_call_reuses_the_directory() {
        let base = temp_dir("reuse");
        let a = resolve_data_dir(&base).expect("1");
        let b = resolve_data_dir(&base).expect("2");
        assert_eq!(a, b);
        fs::remove_dir_all(&base).ok();
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn writes_a_csv_through_the_platform_layer() {
        let base = temp_dir("csv");
        let path = base.join("out.csv");
        let df = fp_core::data::DataFrame::new(vec![fp_core::data::Column::new_f64(
            "C1",
            vec![1.5, 2.5],
        )])
        .expect("frame");
        write_csv(&path, &df, false).expect("write");
        let text = fs::read_to_string(&path).expect("read");
        assert!(text.contains("1.5"));
        fs::remove_dir_all(&base).ok();
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn writes_and_reads_a_project_through_the_platform_layer() {
        let base = temp_dir("project");
        let path = base.join("out.fpp");
        let df = fp_core::data::DataFrame::new(vec![fp_core::data::Column::new_f64(
            "C1",
            vec![1.5, 2.5],
        )])
        .expect("frame");
        save_project_sheets(&path, &[("Sheet 1".to_owned(), df.clone())], "test").expect("save");
        let back = load_project_sheets(&path).expect("load");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].1, df);
        fs::remove_dir_all(&base).ok();
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn a_project_file_keeps_both_worksheets() {
        let base = temp_dir("project2");
        let path = base.join("two.fpp");
        let first = fp_core::data::DataFrame::new(vec![fp_core::data::Column::new_f64(
            "C1",
            vec![1.5, 2.5],
        )])
        .expect("first");
        let second = fp_core::data::DataFrame::new(vec![fp_core::data::Column::new_text(
            "g",
            vec!["Açores".to_owned(), "北京".to_owned()],
        )])
        .expect("second");
        let sheets = vec![
            ("diameter".to_owned(), first.clone()),
            ("defects".to_owned(), second.clone()),
        ];
        save_project_sheets(&path, &sheets, "two.fpp").expect("save");
        let back = load_project_sheets(&path).expect("load");
        assert_eq!(back.len(), 2);
        assert_eq!(back[0], ("diameter".to_owned(), first));
        assert_eq!(back[1], ("defects".to_owned(), second));
        fs::remove_dir_all(&base).ok();
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn a_missing_csv_is_reported_with_its_path() {
        let dir = temp_dir("missing");
        let path = dir.join("gone.csv");
        let error = read_csv(&path).expect_err("missing file");
        assert!(error.contains("gone.csv"), "{error}");
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(all(feature = "desktop", unix))]
    #[test]
    fn a_read_only_directory_refuses_the_write() {
        use std::os::unix::fs::PermissionsExt;

        let dir = temp_dir("readonly");
        let path = dir.join("out.csv");
        let df =
            fp_core::data::DataFrame::new(vec![fp_core::data::Column::new_f64("C1", vec![1.5])])
                .expect("frame");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).expect("chmod");
        let result = write_csv(&path, &df, false);
        // Running as root ignores the permission bits, so only the failure
        // path is asserted when the write is actually refused.
        if let Err(error) = result {
            assert!(error.contains("out.csv"), "{error}");
            assert!(!path.exists());
        }
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).ok();
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn a_path_with_spaces_and_unicode_round_trips() {
        let dir = temp_dir("unicode path");
        let folder = dir.join("peças de teste 2026");
        fs::create_dir_all(&folder).expect("folder");
        let csv = folder.join("medidas da máquina.csv");
        let df = fp_core::data::DataFrame::new(vec![
            fp_core::data::Column::new_f64("diâmetro", vec![12.5, 13.1]),
            fp_core::data::Column::new_text(
                "unidade",
                vec!["São Paulo".to_owned(), "Açores".to_owned()],
            ),
        ])
        .expect("frame");
        write_csv(&csv, &df, false).expect("write csv");
        let (back, _) = read_csv(&csv).expect("read csv");
        assert_eq!(back.n_rows(), 2);
        assert_eq!(back.cell_text(1, 0).as_deref(), Some("São Paulo"));

        let project = folder.join("trabalho do mês.fpp");
        save_project_sheets(
            &project,
            &[("diâmetro".to_owned(), df.clone())],
            "trabalho do mês.fpp",
        )
        .expect("save project");
        let sheets = load_project_sheets(&project).expect("load project");
        assert_eq!(sheets[0].0, "diâmetro");
        assert_eq!(sheets[0].1.n_rows(), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_probe_leaves_no_litter() {
        let base = temp_dir("probe");
        let data = resolve_data_dir(&base).expect("resolve");
        let leftovers: Vec<_> = fs::read_dir(&data.path)
            .expect("list")
            .filter_map(std::result::Result::ok)
            .map(|e| e.file_name())
            .collect();
        assert!(leftovers.is_empty(), "left over {leftovers:?}");
        fs::remove_dir_all(&base).ok();
    }
}

/// Shows a native file browser and returns the chosen path.
///
/// `filters` holds entries such as `"CSV files | *.csv"`; the desktop runner
/// calls this after the UI asks for a browser, so the UI crate stays free of
/// any operating-system knowledge. `None` means "cancelled" or "no native
/// dialog on this platform".
#[cfg(feature = "desktop")]
pub fn browse_file(title: &str, filters: &[String], save: bool) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let chosen = windows::browse_file(title, filters, save);
    #[cfg(all(unix, not(target_os = "macos")))]
    let chosen = unix::browse_file(title, filters, save);
    #[cfg(not(any(target_os = "windows", all(unix, not(target_os = "macos")))))]
    let chosen = {
        let _ = (title, filters, save);
        None
    };
    if save {
        chosen.map(|path| with_default_extension(path, filters))
    } else {
        chosen
    }
}

/// Windows implementation of the native file browser.
///
/// The common dialog of `comdlg32.dll` is called directly, so the portable
/// build keeps its small dependency-free footprint. The FFI block is the only
/// `unsafe` code of the whole program and it stays in the platform layer.
#[cfg(all(feature = "desktop", target_os = "windows"))]
#[allow(unsafe_code)]
mod windows {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::path::PathBuf;

    type Dword = u32;
    type Word = u16;
    type Lparam = isize;

    const OFN_OVERWRITEPROMPT: Dword = 0x0000_0002;
    const OFN_NOCHANGEDIR: Dword = 0x0000_0008;
    const OFN_PATHMUSTEXIST: Dword = 0x0000_0800;
    const OFN_FILEMUSTEXIST: Dword = 0x0000_1000;
    const OFN_EXPLORER: Dword = 0x0008_0000;
    const OFN_ENABLEHOOK: Dword = 0x0000_0020;

    const WM_INITDIALOG: u32 = 0x0110;
    const DWMWA_CAPTION_COLOR: Dword = 35;
    const DWMWA_TEXT_COLOR: Dword = 36;

    const ATTACH_PARENT_PROCESS: Dword = 0xFFFF_FFFF;
    const STD_INPUT_HANDLE: Dword = -10i32 as Dword;
    const STD_OUTPUT_HANDLE: Dword = -11i32 as Dword;
    const STD_ERROR_HANDLE: Dword = -12i32 as Dword;

    #[repr(C)]
    struct OpenFileNameW {
        l_struct_size: Dword,
        hwnd_owner: *mut c_void,
        h_instance: *mut c_void,
        lpstr_filter: *const u16,
        lpstr_custom_filter: *mut u16,
        n_max_cust_filter: Dword,
        n_filter_index: Dword,
        lpstr_file: *mut u16,
        n_max_file: Dword,
        lpstr_file_title: *mut u16,
        n_max_file_title: Dword,
        lpstr_initial_dir: *const u16,
        lpstr_title: *const u16,
        flags: Dword,
        n_file_offset: Word,
        n_file_extension: Word,
        lpstr_def_ext: *const u16,
        l_cust_data: Lparam,
        lpfn_hook: *mut c_void,
        lp_template_name: *const u16,
        pv_reserved: *mut c_void,
        dw_reserved: Dword,
        flags_ex: Dword,
    }

    #[link(name = "comdlg32")]
    unsafe extern "system" {
        fn GetOpenFileNameW(param: *mut OpenFileNameW) -> i32;
        fn GetSaveFileNameW(param: *mut OpenFileNameW) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: Dword) -> i32;
        fn SetStdHandle(std_handle: Dword, handle: *mut c_void) -> i32;
    }

    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            attribute: Dword,
            value: *const c_void,
            size: Dword,
        ) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class_name: *const u16, window_name: *const u16) -> *mut c_void;
    }

    /// Paints a window caption black with white text (Windows 11; on older
    /// builds the call fails and the system caption stays as it is).
    fn dark_caption(hwnd: *mut c_void) {
        let black: Dword = 0x0000_0000;
        let white: Dword = 0x00FF_FFFF; // COLORREF is 0x00BBGGRR
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_CAPTION_COLOR,
                std::ptr::addr_of!(black).cast(),
                std::mem::size_of::<Dword>() as Dword,
            );
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_TEXT_COLOR,
                std::ptr::addr_of!(white).cast(),
                std::mem::size_of::<Dword>() as Dword,
            );
        }
    }

    /// Hook for the common dialog: the dialog window exists on `WM_INITDIALOG`.
    unsafe extern "system" fn dialog_hook(
        hwnd: *mut c_void,
        message: u32,
        _wparam: usize,
        _lparam: isize,
    ) -> usize {
        if message == WM_INITDIALOG {
            dark_caption(hwnd);
        }
        0
    }

    /// Applies the black caption to the main FreePlots window, once it exists.
    pub fn apply_dark_caption() {
        let title = wide("FreePlots");
        let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if !hwnd.is_null() {
            dark_caption(hwnd);
        }
    }

    /// Reattaches the standard streams to the console that launched the
    /// process, so a CLI invocation of the GUI-subsystem binary prints there.
    pub fn attach_console() {
        let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
        if attached == 0 {
            return;
        }
        for handle in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            if let Ok(file) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
                unsafe {
                    SetStdHandle(handle, file.as_raw_handle());
                }
                std::mem::forget(file);
            }
        }
        if let Ok(file) = std::fs::OpenOptions::new().read(true).open("CONIN$") {
            unsafe {
                SetStdHandle(STD_INPUT_HANDLE, file.as_raw_handle());
            }
            std::mem::forget(file);
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Builds the double-null-terminated filter description.
    fn filter_string(filters: &[String]) -> Vec<u16> {
        let mut out: Vec<u16> = Vec::new();
        for entry in filters {
            let (name, pattern) = entry.split_once('|').unwrap_or((entry.as_str(), "*.*"));
            out.extend(wide(name.trim()));
            out.extend(wide(pattern.trim()));
        }
        out.push(0);
        out
    }

    pub fn browse_file(title: &str, filters: &[String], save: bool) -> Option<PathBuf> {
        let filter = filter_string(filters);
        let title = wide(title);
        let mut buffer = vec![0u16; 4096];
        let default_extension = super::default_extension(filters);
        let default_extension = wide(&default_extension);
        let mut flags = OFN_EXPLORER | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST | OFN_ENABLEHOOK;
        if save {
            flags |= OFN_OVERWRITEPROMPT;
        } else {
            flags |= OFN_FILEMUSTEXIST;
        }
        let mut ofn = OpenFileNameW {
            l_struct_size: std::mem::size_of::<OpenFileNameW>() as Dword,
            hwnd_owner: std::ptr::null_mut(),
            h_instance: std::ptr::null_mut(),
            lpstr_filter: filter.as_ptr(),
            lpstr_custom_filter: std::ptr::null_mut(),
            n_max_cust_filter: 0,
            n_filter_index: 1,
            lpstr_file: buffer.as_mut_ptr(),
            n_max_file: buffer.len() as Dword,
            lpstr_file_title: std::ptr::null_mut(),
            n_max_file_title: 0,
            lpstr_initial_dir: std::ptr::null(),
            lpstr_title: title.as_ptr(),
            flags,
            n_file_offset: 0,
            n_file_extension: 0,
            lpstr_def_ext: if default_extension.len() > 1 {
                default_extension.as_ptr()
            } else {
                std::ptr::null()
            },
            l_cust_data: 0,
            lpfn_hook: dialog_hook as *mut c_void,
            lp_template_name: std::ptr::null(),
            pv_reserved: std::ptr::null_mut(),
            dw_reserved: 0,
            flags_ex: 0,
        };
        let answer = unsafe {
            if save {
                GetSaveFileNameW(&mut ofn)
            } else {
                GetOpenFileNameW(&mut ofn)
            }
        };
        if answer <= 0 {
            return None;
        }
        let text: String = buffer
            .iter()
            .take_while(|unit| **unit != 0)
            .map(|unit| char::from_u32(u32::from(*unit)).unwrap_or_default())
            .collect();
        if text.is_empty() {
            None
        } else {
            Some(PathBuf::from(text))
        }
    }
}

/// Unix implementation of the file browser.
///
/// The portal helpers (`zenity` or `kdialog`) are used when they are
/// installed; otherwise the caller keeps the typed path.
#[cfg(all(feature = "desktop", unix, not(target_os = "macos")))]
mod unix {
    use std::path::PathBuf;
    use std::process::Command;

    fn patterns(filters: &[String]) -> Vec<String> {
        filters
            .iter()
            .map(|entry| {
                entry
                    .split_once('|')
                    .map(|(_, pattern)| pattern.trim().to_owned())
                    .unwrap_or_else(|| entry.trim().to_owned())
            })
            .collect()
    }

    fn zenity(title: &str, filters: &[String], save: bool) -> Option<PathBuf> {
        let mut command = Command::new("zenity");
        command.arg("--file-selection");
        command.arg(format!("--title={title}"));
        for entry in filters {
            command.arg(format!("--file-filter={entry}"));
        }
        if save {
            command.arg("--save");
            command.arg("--confirm-overwrite");
        }
        let output = command.output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if text.is_empty() {
            None
        } else {
            Some(PathBuf::from(text))
        }
    }

    fn kdialog(title: &str, filters: &[String], save: bool) -> Option<PathBuf> {
        let mut command = Command::new("kdialog");
        command.arg(if save {
            "--getsavefilename"
        } else {
            "--getopenfilename"
        });
        command.arg(title);
        for pattern in patterns(filters) {
            command.arg(pattern);
        }
        let output = command.output().ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if text.is_empty() {
            None
        } else {
            Some(PathBuf::from(text))
        }
    }

    pub fn browse_file(title: &str, filters: &[String], save: bool) -> Option<PathBuf> {
        zenity(title, filters, save).or_else(|| kdialog(title, filters, save))
    }
}
