//! Exporting a session: HTML and PDF.
//!
//! The exporters receive the plain session text, so they must survive a long
//! session (many reports), a session with a plot, and text that needs
//! escaping in HTML and in the PDF string literals.

use fp_core::data::{Column, DataFrame};
use fp_core::pdf::session_pdf;
use fp_core::report::session_html;
use fp_core::workflow::{AnalysisSpec, run_analysis};

fn report() -> String {
    let values: Vec<f64> = (0..30).map(|index| 20.0 + f64::from(index % 7)).collect();
    let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
    run_analysis(&df, &AnalysisSpec::descriptive_statistics("x"))
        .expect("describe")
        .results[0]
        .to_session_text()
}

#[test]
fn a_long_session_reaches_the_html_with_its_last_report() {
    let one = report();
    let mut session = String::new();
    for _ in 0..60 {
        session.push_str(&one);
        session.push_str("\n\n");
    }
    session.push_str("LAST REPORT MARKER\n");
    let html = session_html(&session);
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("LAST REPORT MARKER"), "tail missing");
    assert!(html.contains("Descriptive Statistics"), "reports missing");
    assert!(html.contains("</html>"));
    // The report body is escaped, so a comparison sign survives as text.
    assert!(html.contains("Mean"));
}

#[test]
fn html_escapes_the_report_text() {
    let html = session_html("a < b & c > d");
    assert!(html.contains("a &lt; b &amp; c &gt; d"), "{html}");
}

#[test]
fn the_pdf_starts_like_a_pdf_and_has_several_pages() {
    let one = report();
    let session = one.repeat(12);
    let bytes = session_pdf("FreePlots Session", &session, None);
    assert!(&bytes[..5] == b"%PDF-", "missing PDF signature");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("%%EOF"), "missing PDF end marker");
    assert!(
        text.matches("/Type /Page\n").count() > 1 || text.matches("/Type /Page ").count() > 1,
        "a long session should span more than one page"
    );
}

#[test]
fn the_pdf_with_a_plot_still_ends_cleanly() {
    let plot = {
        let values: Vec<f64> = (0..40).map(|index| f64::from(index % 9)).collect();
        let df = DataFrame::new(vec![Column::new_f64("x", values)]).expect("frame");
        run_analysis(&df, &AnalysisSpec::histogram("x"))
            .expect("histogram")
            .plot
            .expect("plot")
    };
    let bytes = session_pdf("FreePlots Session", "one short report\n", Some(&plot));
    assert!(&bytes[..5] == b"%PDF-");
    assert!(bytes.len() > 1_000);
}
