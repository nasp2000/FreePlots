//! Dependency-free PDF export of the Session report.
//!
//! The writer produces a small PDF 1.4 file that uses the base-14 fonts, so
//! no font file and no dependency are needed. The Session text is drawn in
//! Courier, so the tables keep the alignment of the text report, and an
//! optional plot is drawn as vector graphics with the engine-owned ticks.
//!
//! Characters outside the WinAnsi range (Greek letters, mathematical
//! symbols) are transliterated to ASCII; the text and HTML exports keep the
//! original characters.

use crate::plot::{PlotSpec, Point};

/// Page width (A4) in points.
const PAGE_WIDTH: f64 = 595.0;
/// Page height (A4) in points.
const PAGE_HEIGHT: f64 = 842.0;
/// Margin on every side, in points.
const MARGIN: f64 = 40.0;
/// Size of the Session body text, in points.
const BODY_SIZE: f64 = 8.0;
/// Distance between two body lines, in points.
const BODY_LEADING: f64 = 10.0;
/// Size of the report title, in points.
const TITLE_SIZE: f64 = 15.0;
/// Size of the plot labels, in points.
const LABEL_SIZE: f64 = 7.5;

/// Builds a PDF report from the Session text and an optional plot.
///
/// The text is laid out with the same line breaks as the Session report; the
/// plot, when given, is drawn above the text on the first page.
#[must_use]
pub fn session_pdf(title: &str, session: &str, plot: Option<&PlotSpec>) -> Vec<u8> {
    let mut pages: Vec<String> = Vec::new();
    let mut content = String::new();
    let mut y = PAGE_HEIGHT - MARGIN;

    content.push_str(&format!(
        "BT /F2 {TITLE_SIZE} Tf {MARGIN} {y} Td ({}) Tj ET\n",
        escape(&transliterate(title))
    ));
    y -= TITLE_SIZE + 10.0;

    if let Some(spec) = plot {
        let height = 250.0;
        let width = PAGE_WIDTH - 2.0 * MARGIN;
        draw_plot(&mut content, spec, MARGIN, y - height, width, height);
        y -= height + 34.0;
    }

    for line in session.lines() {
        for piece in wrap(line, max_characters()) {
            if y < MARGIN + BODY_LEADING {
                pages.push(std::mem::take(&mut content));
                y = PAGE_HEIGHT - MARGIN;
            }
            content.push_str(&format!(
                "BT /F1 {BODY_SIZE} Tf {MARGIN} {y} Td ({}) Tj ET\n",
                escape(&transliterate(&piece))
            ));
            y -= BODY_LEADING;
        }
    }
    pages.push(content);
    // Every page carries a footer with the program and the page number, so a
    // printed report can be reassembled in order.
    let total = pages.len();
    for (index, page) in pages.iter_mut().enumerate() {
        let footer = format!("FreePlots — page {} of {}", index + 1, total);
        page.push_str(&format!(
            "BT /F1 {BODY_SIZE} Tf {MARGIN} {} Td ({}) Tj ET\n",
            MARGIN / 2.0,
            escape(&transliterate(&footer))
        ));
    }
    build_pdf(pages)
}

/// Number of characters that fit on one line of the body text.
fn max_characters() -> usize {
    ((PAGE_WIDTH - 2.0 * MARGIN) / (0.6 * BODY_SIZE)).floor() as usize
}

/// Splits a long line into pieces that fit the page width.
fn wrap(line: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![line.to_owned()];
    }
    let mut pieces = Vec::new();
    let mut current = String::new();
    for word in line.split(' ') {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if candidate.chars().count() > width && !current.is_empty() {
            pieces.push(std::mem::take(&mut current));
            current = word.to_owned();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() || pieces.is_empty() {
        pieces.push(current);
    }
    pieces
}

/// Assembles the PDF objects, the cross-reference table and the trailer.
fn build_pdf(pages: Vec<String>) -> Vec<u8> {
    let mut objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        // The page tree is written once the pages are known.
        Vec::new(),
    ];
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>".to_vec(),
    );
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );
    objects.push(
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_vec(),
    );

    let mut kids = String::new();
    for (index, page) in pages.iter().enumerate() {
        let page_object = 6 + 2 * index;
        let content_object = page_object + 1;
        kids.push_str(&format!("{} 0 R ", page_object));
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] \
             /Resources << /Font << /F1 3 0 R /F2 4 0 R /F3 5 0 R >> >> /Contents {content_object} 0 R >>"
        ).into_bytes());
        objects
            .push(format!("<< /Length {} >>\nstream\n{page}\nendstream", page.len()).into_bytes());
    }
    objects[1] = format!("<< /Type /Pages /Kids [{kids}] /Count {} >>", pages.len()).into_bytes();

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        out.extend_from_slice(object);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Draws a plot (with its panels) as vector graphics.
fn draw_plot(
    content: &mut String,
    spec: &PlotSpec,
    left: f64,
    bottom: f64,
    width: f64,
    height: f64,
) {
    let panels: Vec<&PlotSpec> = if spec.panels.is_empty() {
        vec![spec]
    } else {
        spec.panels.iter().collect()
    };
    let columns = spec.panel_columns.max(1);
    let rows = panels.len().div_ceil(columns);
    let gap = 18.0;
    let panel_width = (width - gap * (columns as f64 - 1.0)) / columns as f64;
    let panel_height = (height - gap * (rows as f64 - 1.0)) / rows as f64;
    for (index, panel) in panels.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let panel_left = left + column as f64 * (panel_width + gap);
        let panel_bottom = bottom + height - (row as f64 + 1.0) * panel_height - row as f64 * gap;
        draw_panel(
            content,
            panel,
            panel_left,
            panel_bottom,
            panel_width,
            panel_height,
        );
    }
}

/// Draws one panel: frame, ticks, bands, lines and points.
fn draw_panel(
    content: &mut String,
    spec: &PlotSpec,
    left: f64,
    bottom: f64,
    width: f64,
    height: f64,
) {
    let frame = Frame {
        left,
        bottom,
        width,
        height,
        x_min: spec.x_axis.min,
        x_max: spec.x_axis.max,
        y_min: spec.y_axis.min,
        y_max: spec.y_axis.max,
    };
    // Frame.
    content.push_str(&format!("{:.3} {:.3} {:.3} RG 0.6 w\n", 0.55, 0.55, 0.55));
    content.push_str(&format!(
        "{:.3} {:.3} {:.3} {:.3} re S\n",
        frame.left, frame.bottom, frame.width, frame.height
    ));
    // Grid and tick labels.
    content.push_str(&format!("{:.3} {:.3} {:.3} RG 0.4 w\n", 0.88, 0.88, 0.88));
    for tick in &spec.x_axis.ticks {
        let x = frame.x(tick.position);
        if !x.is_finite() {
            continue;
        }
        content.push_str(&format!(
            "{x:.3} {:.3} m {x:.3} {:.3} l S\n",
            frame.bottom,
            frame.bottom + 4.0
        ));
        content.push_str(&format!(
            "BT /F3 {LABEL_SIZE} Tf {:.3} {:.3} Td ({}) Tj ET\n",
            x - 12.0,
            frame.bottom - 9.0,
            escape(&transliterate(&tick.label))
        ));
    }
    for tick in &spec.y_axis.ticks {
        let y = frame.y(tick.position);
        if !y.is_finite() {
            continue;
        }
        content.push_str(&format!(
            "{:.3} {:.3} m {:.3} {:.3} l S\n",
            frame.left,
            y,
            frame.left + 4.0,
            y
        ));
        content.push_str(&format!(
            "BT /F3 {LABEL_SIZE} Tf {:.3} {:.3} Td ({}) Tj ET\n",
            frame.left - 34.0,
            y - 2.5,
            escape(&transliterate(&tick.label))
        ));
    }
    // Axis titles.
    content.push_str(&format!(
        "BT /F3 {LABEL_SIZE} Tf {:.3} {:.3} Td ({}) Tj ET\n",
        frame.left,
        frame.bottom - 20.0,
        escape(&transliterate(&spec.x_axis.label))
    ));
    content.push_str(&format!(
        "BT /F3 {LABEL_SIZE} Tf {:.3} {:.3} Td ({}) Tj ET\n",
        frame.left,
        frame.bottom + frame.height + 6.0,
        escape(&transliterate(&spec.y_axis.label))
    ));
    // Confidence ribbons.
    content.push_str("0.87 0.92 0.90 rg\n");
    for band in &spec.bands {
        if band.lower.len() < 2 || band.upper.len() < 2 {
            continue;
        }
        let mut path = String::new();
        for (index, point) in band.lower.iter().enumerate() {
            let x = frame.x(point.x);
            let y = frame.y(point.y);
            path.push_str(&format!(
                "{x:.3} {y:.3} {} ",
                if index == 0 { "m" } else { "l" }
            ));
        }
        for point in band.upper.iter().rev() {
            let x = frame.x(point.x);
            let y = frame.y(point.y);
            path.push_str(&format!("{x:.3} {y:.3} l "));
        }
        path.push_str("h f\n");
        content.push_str(&path);
    }
    // Fitted and reference lines.
    content.push_str(&format!("{:.3} {:.3} {:.3} RG 1.0 w\n", 0.06, 0.49, 0.25));
    for segment in &spec.lines {
        draw_segment(content, &frame, segment);
    }
    // Points.
    content.push_str(&format!("{:.3} {:.3} {:.3} rg\n", 0.14, 0.14, 0.14));
    for point in &spec.points {
        draw_point(content, &frame, point);
    }
    // Overlay series and their legend.
    for (index, series) in spec.series.iter().enumerate() {
        let tone = 0.2 + 0.15 * index as f64;
        content.push_str(&format!("{tone:.3} {tone:.3} 0.55 rg\n"));
        for point in &series.points {
            draw_point(content, &frame, point);
        }
        content.push_str(&format!("{tone:.3} {tone:.3} 0.55 RG 0.8 w\n"));
        for segment in &series.lines {
            draw_segment(content, &frame, segment);
        }
        content.push_str(&format!(
            "BT /F3 {LABEL_SIZE} Tf {:.3} {:.3} Td ({}) Tj ET\n",
            frame.left + 4.0,
            frame.bottom + frame.height + 6.0 - 9.0 * index as f64,
            escape(&transliterate(&series.label))
        ));
    }
}

fn draw_segment(content: &mut String, frame: &Frame, segment: &crate::plot::Segment) {
    let x1 = frame.x(segment.from.x);
    let y1 = frame.y(segment.from.y);
    let x2 = frame.x(segment.to.x);
    let y2 = frame.y(segment.to.y);
    if !x1.is_finite() || !x2.is_finite() || !y1.is_finite() || !y2.is_finite() {
        return;
    }
    content.push_str(&format!("{x1:.3} {y1:.3} m {x2:.3} {y2:.3} l S\n"));
}

fn draw_point(content: &mut String, frame: &Frame, point: &Point) {
    let x = frame.x(point.x);
    let y = frame.y(point.y);
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    content.push_str(&format!("{x:.3} {y:.3} 1.6 1.6 re f\n"));
}

/// The rectangle of a panel and the mapping from axis to page coordinates.
struct Frame {
    left: f64,
    bottom: f64,
    width: f64,
    height: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}

impl Frame {
    fn x(&self, value: f64) -> f64 {
        let span = self.x_max - self.x_min;
        if span.abs() < f64::EPSILON {
            return self.left;
        }
        self.left + (value - self.x_min) / span * self.width
    }

    fn y(&self, value: f64) -> f64 {
        let span = self.y_max - self.y_min;
        if span.abs() < f64::EPSILON {
            return self.bottom;
        }
        self.bottom + (value - self.y_min) / span * self.height
    }
}

/// Escapes a string for a PDF literal.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            other if (other as u32) < 128 => out.push(other),
            _ => out.push('?'),
        }
    }
    out
}

/// Replaces the characters outside WinAnsi with readable ASCII.
fn transliterate(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            'σ' => out.push_str("sd"),
            'μ' => out.push_str("mean"),
            'π' => out.push_str("pi"),
            'χ' => out.push_str("chi"),
            'Δ' => out.push_str("Delta"),
            'λ' => out.push_str("lambda"),
            'ρ' => out.push_str("rho"),
            'τ' => out.push_str("tau"),
            'β' => out.push_str("beta"),
            'α' => out.push_str("alpha"),
            '√' => out.push_str("sqrt"),
            '∑' => out.push_str("sum"),
            '≤' => out.push_str("<="),
            '≥' => out.push_str(">="),
            '≈' => out.push('~'),
            '∞' => out.push_str("inf"),
            '→' => out.push_str("->"),
            '⇒' => out.push_str("=>"),
            '∈' => out.push_str("in"),
            '±' => out.push_str("+/-"),
            '×' => out.push('x'),
            '·' => out.push('*'),
            '⋅' => out.push('*'),
            '²' => out.push_str("^2"),
            '³' => out.push_str("^3"),
            '⁴' => out.push_str("^4"),
            '₀' => out.push('0'),
            '₁' => out.push('1'),
            '₂' => out.push('2'),
            'ᵢ' => out.push('i'),
            'ₙ' => out.push('n'),
            '–' | '—' => out.push('-'),
            '‘' | '’' => out.push('\''),
            '“' | '”' => out.push('"'),
            '…' => out.push_str("..."),
            '•' => out.push('*'),
            '⁄' => out.push('/'),
            '°' => out.push_str("deg"),
            other if (other as u32) < 128 => out.push(other),
            // Anything else is written as a question mark by `escape`.
            _ => out.push('?'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plot::{AxisSpec, Point, Scale, Segment, Tick};

    fn spec() -> PlotSpec {
        PlotSpec {
            title: "Demo".to_owned(),
            x_axis: AxisSpec {
                label: "x".to_owned(),
                scale: Scale::Linear,
                min: 0.0,
                max: 10.0,
                ticks: vec![
                    Tick {
                        position: 0.0,
                        label: "0".to_owned(),
                    },
                    Tick {
                        position: 10.0,
                        label: "10".to_owned(),
                    },
                ],
                minor: Vec::new(),
            },
            y_axis: AxisSpec {
                label: "y".to_owned(),
                scale: Scale::Linear,
                min: -1.0,
                max: 1.0,
                ticks: vec![Tick {
                    position: 0.0,
                    label: "0".to_owned(),
                }],
                minor: Vec::new(),
            },
            size: (900, 400),
            points: vec![Point { x: 1.0, y: 0.5 }],
            lines: vec![Segment {
                from: Point { x: 0.0, y: 0.0 },
                to: Point { x: 10.0, y: 0.0 },
                width: 1.5,
            }],
            bands: Vec::new(),
            series: Vec::new(),
            panels: Vec::new(),
            panel_columns: 1,
            secondary: None,
        }
    }

    #[test]
    fn pdf_has_a_header_a_trailer_and_the_session_text() {
        let bytes = session_pdf("Report", "Hello world\nSecond line", Some(&spec()));
        let text = String::from_utf8_lossy(&bytes).to_string();
        assert!(text.starts_with("%PDF-1.4"), "{}", &text[..20]);
        assert!(text.trim_end().ends_with("%%EOF"));
        assert!(text.contains("trailer"));
        assert!(text.contains("Hello world"));
        assert!(text.contains("Second line"));
        assert!(text.contains("/BaseFont /Courier"));
    }

    #[test]
    fn long_sessions_span_several_pages() {
        let session = (0..200)
            .map(|index| format!("line {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let bytes = session_pdf("Long", &session, None);
        let text = String::from_utf8_lossy(&bytes).to_string();
        assert!(text.contains("/Count 3"), "the page count is wrong");
    }

    #[test]
    fn unsupported_characters_are_transliterated() {
        assert_eq!(transliterate("σ ± 3"), "sd +/- 3");
        assert_eq!(transliterate("R²"), "R^2");
        let bytes = session_pdf("Title σ", "value ± 1", None);
        let text = String::from_utf8_lossy(&bytes).to_string();
        assert!(text.contains("Title sd"));
    }

    #[test]
    fn long_lines_are_wrapped() {
        let line = "word ".repeat(60);
        let pieces = wrap(&line, 40);
        assert!(pieces.len() > 1);
        for piece in &pieces {
            assert!(piece.chars().count() <= 40, "{piece}");
        }
        assert_eq!(pieces.join(" ").split_whitespace().count(), 60);
    }

    #[test]
    fn the_frame_maps_the_axis_range() {
        let frame = Frame {
            left: 10.0,
            bottom: 20.0,
            width: 100.0,
            height: 50.0,
            x_min: 0.0,
            x_max: 10.0,
            y_min: 0.0,
            y_max: 2.0,
        };
        assert!((frame.x(5.0) - 60.0).abs() < 1e-9);
        assert!((frame.y(1.0) - 45.0).abs() < 1e-9);
    }

    #[test]
    fn every_page_carries_its_number_and_an_a4_box() {
        let session = "row\n".repeat(100);
        let bytes = session_pdf("FreePlots Session", &session, None);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/MediaBox [0 0 595 842]"), "not A4");
        assert!(text.contains("FreePlots - page 1 of 2"), "{text:.0}");
        assert!(text.contains("FreePlots - page 2 of 2"), "{text:.0}");
        assert!(
            !text.contains("page 1 of 1"),
            "the count must match the pages"
        );
    }
}
