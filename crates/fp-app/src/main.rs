//! FreePlots — application entry point.
//!
//! With the `desktop` feature the binary opens the classic statistical-desktop workspace;
//! without it (and for every subcommand) it is the CLI. All mathematics comes
//! from `fp-core`.

// The engine and the UI crate forbid unsafe code; the platform layer needs a
// single scoped exception for the Windows common-dialog FFI.
#![deny(unsafe_code)]
#![deny(missing_debug_implementations)]
// The desktop build is a Windows (GUI) subsystem program: double-clicking the
// executable must not open a console window. CLI invocations reattach to the
// parent console in `platform::prepare_console`.
#![cfg_attr(all(windows, feature = "desktop"), windows_subsystem = "windows")]

#[cfg(feature = "desktop")]
mod bridge;
mod cli;
#[cfg(feature = "desktop")]
mod gui;
mod platform;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    #[cfg(all(windows, feature = "desktop"))]
    platform::prepare_console(&args);

    #[cfg(feature = "desktop")]
    #[cfg(feature = "desktop")]
    if args.first().map(String::as_str) == Some("--mcp-live") {
        let address = args.get(1).map_or("127.0.0.1:8767", String::as_str);
        let (host, port) = address.rsplit_once(':').unwrap_or(("127.0.0.1", "8767"));
        let token = args
            .iter()
            .position(|arg| arg == "--token")
            .and_then(|index| args.get(index + 1))
            .cloned();
        return match fp_mcp::serve_live(host, port.parse().unwrap_or(8767), token) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("error: {error}");
                std::process::ExitCode::FAILURE
            }
        };
    }

    #[cfg(feature = "desktop")]
    if args.first().map(String::as_str) == Some("--mcp")
        || args.first().map(String::as_str) == Some("--mcp-http")
    {
        let mut mcp_args: Vec<String> = Vec::new();
        if args.first().map(String::as_str) == Some("--mcp-http") {
            mcp_args.push("--http".to_owned());
            if let Some(address) = args.get(1) {
                mcp_args.push(address.clone());
            }
        }
        if let Some(index) = args.iter().position(|arg| arg == "--token")
            && let Some(token) = args.get(index + 1)
        {
            mcp_args.push("--token".to_owned());
            mcp_args.push(token.clone());
        }
        fp_mcp::run(&mcp_args);
        return std::process::ExitCode::SUCCESS;
    }

    #[cfg(feature = "desktop")]
    if args.is_empty()
        || args.first().map(String::as_str) == Some("--gui")
        || args.first().map(String::as_str) == Some("--bridge")
    {
        let mut gui_args = args.clone();
        if gui_args.first().map(String::as_str) == Some("--bridge") {
            gui_args.insert(0, "--gui".to_owned());
        }
        return match gui::run(&gui_args) {
            Ok(()) => std::process::ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                std::process::ExitCode::FAILURE
            }
        };
    }

    match cli::run(&args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
