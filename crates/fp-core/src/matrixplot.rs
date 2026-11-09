// SPDX-License-Identifier: MIT
//! Scatterplot matrix: every pair of variables in a grid of panels.
//!
//! For `k` variables the plot is a `k × k` grid. The **off-diagonal** panels
//! are the scatterplots of the row variable against the column variable; the
//! **diagonal** panels are histograms of the variable, drawn with the same
//! filled-band bars as the Pareto chart.
//!
//! The matrix is a picture of the pairwise relationships: it shows the shape of
//! each variable and whether the pairs move together, before any model is
//! fitted. Rows must be complete in every variable (listwise deletion).
//!
//! The documented behaviour has a matrix plot; its bin and panel conventions were
//! not documented, so the layout is our own — see the project notes.

use crate::data::DataFrame;
use crate::error::{Error, Result};
use crate::plot::{AxisSpec, Band, PlotSpec, Point, Scale, Tick, linear_axis};
use crate::report::{AnalysisResult, Item, standard_warnings};
use crate::workflow::AnalysisOutput;

/// Largest number of variables in one matrix (the grid grows with its square).
pub const MAX_VARIABLES: usize = 6;

/// Builds the panels of the matrix.
fn panels(columns: &[String], data: &[Vec<f64>], bins: usize) -> Vec<PlotSpec> {
    let k = columns.len();
    let mut panels = Vec::with_capacity(k * k);
    for row in 0..k {
        for column in 0..k {
            if row == column {
                panels.push(histogram_panel(&columns[row], data, row, bins));
            } else {
                panels.push(scatter_panel(
                    &columns[row],
                    &columns[column],
                    data,
                    row,
                    column,
                ));
            }
        }
    }
    panels
}

/// Histogram panel of one variable, with filled bands as bars.
fn histogram_panel(name: &str, data: &[Vec<f64>], index: usize, bins: usize) -> PlotSpec {
    let values: Vec<f64> = data.iter().map(|row| row[index]).collect();
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = if high > low { high - low } else { 1.0 };
    let bins = bins.max(3);
    let width = span / bins as f64;
    let mut counts = vec![0.0f64; bins];
    for value in &values {
        let position = ((value - low) / width).floor() as usize;
        counts[position.min(bins - 1)] += 1.0;
    }
    let peak = counts.iter().copied().fold(1.0, f64::max);
    let bands = counts
        .iter()
        .enumerate()
        .map(|(bin, count)| {
            let left = low + bin as f64 * width;
            let right = left + width;
            Band {
                lower: vec![Point { x: left, y: 0.0 }, Point { x: right, y: 0.0 }],
                upper: vec![
                    Point { x: left, y: *count },
                    Point {
                        x: right,
                        y: *count,
                    },
                ],
            }
        })
        .collect();
    PlotSpec {
        title: name.to_owned(),
        x_axis: linear_axis(name, low, low + span),
        y_axis: AxisSpec {
            label: String::new(),
            scale: Scale::Linear,
            min: 0.0,
            max: peak * 1.1,
            ticks: Vec::<Tick>::new(),
            minor: Vec::new(),
        },
        size: (400, 300),
        points: Vec::new(),
        lines: Vec::new(),
        bands,
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Scatter panel of two variables.
fn scatter_panel(
    row_name: &str,
    column_name: &str,
    data: &[Vec<f64>],
    row: usize,
    column: usize,
) -> PlotSpec {
    let xs: Vec<f64> = data.iter().map(|values| values[column]).collect();
    let ys: Vec<f64> = data.iter().map(|values| values[row]).collect();
    let x_min = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let x_max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let y_min = ys.iter().copied().fold(f64::INFINITY, f64::min);
    let y_max = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let x_pad = ((x_max - x_min).abs() * 0.05).max(1e-9);
    let y_pad = ((y_max - y_min).abs() * 0.05).max(1e-9);
    PlotSpec {
        title: format!("{row_name} vs {column_name}"),
        x_axis: linear_axis(column_name, x_min - x_pad, x_max + x_pad),
        y_axis: linear_axis(row_name, y_min - y_pad, y_max + y_pad),
        size: (400, 300),
        points: xs
            .iter()
            .zip(&ys)
            .map(|(x, y)| Point { x: *x, y: *y })
            .collect(),
        lines: Vec::new(),
        bands: Vec::new(),
        series: Vec::new(),
        panels: Vec::new(),
        panel_columns: 0,
        secondary: None,
        notes: Vec::new(),
    }
}

/// Builds the matrix plot and its report.
pub fn analyze(df: &DataFrame, columns: &[String], bins: usize) -> Result<AnalysisOutput> {
    if columns.len() < 2 {
        return Err(Error::Unsupported(
            "the matrix plot needs at least two variables".to_owned(),
        ));
    }
    if columns.len() > MAX_VARIABLES {
        return Err(Error::Unsupported(format!(
            "the matrix plot accepts at most {MAX_VARIABLES} variables"
        )));
    }
    let mut seen = columns.to_vec();
    seen.sort();
    seen.dedup();
    if seen.len() != columns.len() {
        return Err(Error::Unsupported(
            "the variables must be distinct columns".to_owned(),
        ));
    }
    let mut sources = Vec::with_capacity(columns.len());
    for name in columns {
        sources.push(df.column(name)?.numeric_by_row());
    }
    let mut data: Vec<Vec<f64>> = Vec::with_capacity(df.n_rows());
    for row in 0..df.n_rows() {
        let mut values = Vec::with_capacity(columns.len());
        let mut complete = true;
        for source in &sources {
            if !source[row].is_finite() {
                complete = false;
                break;
            }
            values.push(source[row]);
        }
        if complete {
            data.push(values);
        }
    }
    if data.len() < 3 {
        return Err(Error::SampleTooSmall {
            n: data.len(),
            min: 3,
        });
    }

    let k = columns.len();
    let plot = PlotSpec {
        title: "Matrix Plot".to_owned(),
        x_axis: linear_axis("", 0.0, 1.0),
        y_axis: linear_axis("", 0.0, 1.0),
        size: ((420 * k) as u32, (340 * k) as u32),
        points: Vec::new(),
        lines: Vec::new(),
        bands: Vec::new(),
        series: Vec::new(),
        panels: panels(columns, &data, bins),
        panel_columns: k,
        secondary: None,
        notes: Vec::new(),
    };

    let mut result = AnalysisResult::new(
        "Matrix Plot".to_owned(),
        format!(
            "{} variables, {} complete rows, histograms with {bins} bins on the diagonal",
            columns.len(),
            data.len()
        ),
    );
    result.n = data.len();
    result.excluded = df.n_rows() - data.len();
    result.statistics = vec![
        ("Variables", k as f64),
        ("Panels", (k * k) as f64),
        ("Observations", data.len() as f64),
    ];
    // Pairwise correlations make the picture quantitative.
    result.extra.push(Item::Table {
        headers: vec![
            "Pair".to_owned(),
            "Pearson r".to_owned(),
            "Observations".to_owned(),
        ],
        rows: {
            let mut rows = Vec::new();
            for i in 0..k {
                for j in (i + 1)..k {
                    let mut sx = 0.0;
                    let mut sy = 0.0;
                    let mut sxx = 0.0;
                    let mut syy = 0.0;
                    let mut sxy = 0.0;
                    for row in &data {
                        sx += row[j];
                        sy += row[i];
                    }
                    let n = data.len() as f64;
                    let mean_x = sx / n;
                    let mean_y = sy / n;
                    for row in &data {
                        let dx = row[j] - mean_x;
                        let dy = row[i] - mean_y;
                        sxx += dx * dx;
                        syy += dy * dy;
                        sxy += dx * dy;
                    }
                    let r = if sxx > 0.0 && syy > 0.0 {
                        sxy / (sxx * syy).sqrt()
                    } else {
                        f64::NAN
                    };
                    rows.push(vec![
                        format!("{} × {}", columns[i], columns[j]),
                        format!("{r:.6}"),
                        format!("{}", data.len()),
                    ]);
                }
            }
            rows
        },
    });
    result.notes = vec![
        (
            "Panels".to_owned(),
            "off the diagonal: the scatterplot of the row variable against the column \
             variable; on the diagonal: the histogram of the variable"
                .to_owned(),
        ),
        (
            "Bins".to_owned(),
            "equal-width bins between the minimum and the maximum of each variable".to_owned(),
        ),
        (
            "Data policy".to_owned(),
            "rows with a missing value in any variable are excluded (listwise) and counted"
                .to_owned(),
        ),
    ];
    result.warnings = standard_warnings(result.n, result.excluded, 0);
    result.warnings.push(
        "the panel layout and the bin rule are our own and have not been verified against \
         documented output"
            .to_owned(),
    );

    Ok(AnalysisOutput {
        results: vec![result],
        plot: Some(plot),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Column;

    fn frame() -> DataFrame {
        let mut a = Vec::new();
        let mut b = Vec::new();
        let mut c = Vec::new();
        for index in 0..30 {
            let x = index as f64;
            a.push(x);
            b.push(2.0 * x + ((index % 3) as f64) * 0.1);
            c.push(((index * 7) % 11) as f64);
        }
        DataFrame::new(vec![
            Column::new_f64("a", a),
            Column::new_f64("b", b),
            Column::new_f64("c", c),
        ])
        .expect("frame")
    }

    #[test]
    fn the_grid_has_a_panel_for_every_pair() {
        let out = analyze(
            &frame(),
            &["a".to_owned(), "b".to_owned(), "c".to_owned()],
            8,
        )
        .expect("analysis");
        let plot = out.plot.expect("plot");
        assert_eq!(plot.panel_columns, 3);
        assert_eq!(plot.panels.len(), 9);
        // Three diagonal histograms with bars, six scatterplots with points.
        let diagonal = plot.panels.iter().filter(|p| !p.bands.is_empty()).count();
        let scatter = plot.panels.iter().filter(|p| !p.points.is_empty()).count();
        assert_eq!(diagonal, 3);
        assert_eq!(scatter, 6);
        assert_eq!(plot.panels[1].title, "a vs b");
        assert_eq!(plot.panels[0].title, "a");
        // The rendered SVG contains every panel.
        let svg = crate::plot::render_svg(&plot);
        assert!(!svg.contains("NaN"));
    }

    #[test]
    fn the_report_gives_the_pairwise_correlations() {
        let out = analyze(&frame(), &["a".to_owned(), "b".to_owned()], 6).expect("analysis");
        let text = out.results[0].to_session_text();
        assert!(text.contains("Matrix Plot"), "{text}");
        assert!(text.contains("Pearson r"), "{text}");
        // a and b are almost perfectly correlated.
        let table = out.results[0]
            .extra
            .iter()
            .find_map(|item| match item {
                Item::Table { rows, .. } => Some(rows.clone()),
                _ => None,
            })
            .expect("table");
        let r: f64 = table[0][1].parse().expect("correlation");
        assert!(r > 0.99, "{r}");
    }

    #[test]
    fn invalid_requests_are_rejected() {
        let df = frame();
        assert!(matches!(
            analyze(&df, &["a".to_owned()], 6),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(
            analyze(&df, &["a".to_owned(), "a".to_owned()], 6),
            Err(Error::Unsupported(_))
        ));
        let many: Vec<String> = (0..7).map(|index| format!("C{index}")).collect();
        assert!(matches!(analyze(&df, &many, 6), Err(Error::Unsupported(_))));
    }

    #[test]
    fn a_missing_row_does_not_shift_the_columns() {
        let with_missing = DataFrame::new(vec![
            Column::new_f64("a", vec![f64::NAN, 1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("b", vec![9.0, 2.0, 4.0, 6.0, 8.0, 10.0]),
        ])
        .expect("frame");
        let without = DataFrame::new(vec![
            Column::new_f64("a", vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            Column::new_f64("b", vec![2.0, 4.0, 6.0, 8.0, 10.0]),
        ])
        .expect("frame");
        let columns = vec!["a".to_owned(), "b".to_owned()];
        let a = analyze(&with_missing, &columns, 8).expect("matrix");
        let b = analyze(&without, &columns, 8).expect("matrix");
        // The missing row must not shift the values: every panel holds the
        // same points as without it.
        let pa = a.plot.expect("plot");
        let pb = b.plot.expect("plot");
        assert_eq!(pa.panels.len(), pb.panels.len());
        for (left, right) in pa.panels.iter().zip(&pb.panels) {
            assert_eq!(left.points.len(), right.points.len());
            for (p, q) in left.points.iter().zip(&right.points) {
                assert!((p.x - q.x).abs() < 1e-12 && (p.y - q.y).abs() < 1e-12);
            }
        }
        assert!(a.results[0].to_session_text().contains("1.000000"));
    }

    #[test]
    fn more_than_six_variables_are_refused() {
        let columns: Vec<Column> = (0..7)
            .map(|column| {
                Column::new_f64(
                    format!("c{column}"),
                    (0..30).map(|row| (row + column) as f64).collect(),
                )
            })
            .collect();
        let df = DataFrame::new(columns).expect("frame");
        let names: Vec<String> = (0..7).map(|column| format!("c{column}")).collect();
        let error = analyze(&df, &names, 8).expect_err("the variable limit");
        assert!(error.to_string().contains("at most 6"), "{error}");
    }
}
