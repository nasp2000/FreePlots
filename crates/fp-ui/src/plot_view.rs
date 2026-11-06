//! Screen backend for [`fp_core::plot::PlotSpec`].
//!
//! Like the SVG backend, it only maps engine-owned axis coordinates to pixels:
//! it never computes a tick, a probability or a fitted value.

use egui::{
    Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, epaint::TextShape, pos2,
    vec2,
};
use fp_core::plot::{GraphFontFamily, GraphOverlay, PlotSpec, ReferenceAxis};

const LEFT: f32 = 62.0;
const RIGHT: f32 = 18.0;
const TOP: f32 = 34.0;
const BOTTOM: f32 = 44.0;

/// Draws a plot in the available space and returns the response.
pub fn show(ui: &mut egui::Ui, spec: &PlotSpec) -> egui::Response {
    show_with(ui, spec, &GraphOverlay::default())
}

/// Draws a plot with the presentation edits (subtitle, footer, reference lines).
pub fn show_with(ui: &mut egui::Ui, spec: &PlotSpec, overlay: &GraphOverlay) -> egui::Response {
    let mut marks = Vec::new();
    show_with_marks(ui, spec, overlay, &mut marks)
}

/// Draws the plot and lets Ctrl+click mark (brush) single points, as the
/// reference graph editor does. The marks are plain indices into
/// [`PlotSpec::points`]; the caller keeps them between frames.
pub fn show_with_marks(
    ui: &mut egui::Ui,
    spec: &PlotSpec,
    overlay: &GraphOverlay,
    marks: &mut Vec<(usize, usize)>,
) -> egui::Response {
    let rescaled;
    let spec = if overlay.x_range.is_some() || overlay.y_range.is_some() {
        rescaled = overlay.rescale(spec);
        &rescaled
    } else {
        spec
    };
    let available = ui.available_size();
    let width = available.x.max(320.0);
    let visible_height = if available.y.is_finite() && available.y > 0.0 {
        available.y.max(160.0)
    } else {
        spec.size.1 as f32
    };
    let (canvas, frame) = if spec.panels.is_empty() {
        fitted_frame(
            vec2(width, visible_height),
            vec2(spec.size.0.max(1) as f32, spec.size.1.max(1) as f32),
        )
    } else {
        let columns = spec.panel_columns.max(1);
        let rows = spec.panels.len().div_ceil(columns);
        let ratio = spec
            .panels
            .first()
            .map(|panel| panel.size.1 as f32 / panel.size.0.max(1) as f32)
            .unwrap_or(0.69);
        let (title_h, footer_h) = panel_bands(spec, Some(overlay));
        fitted_panels(
            vec2(width, visible_height),
            columns,
            rows,
            ratio,
            title_h,
            footer_h,
        )
    };
    // Clickable, not only hovered: the graph window opens its context menu on
    // this response (right-click anywhere over the plot).
    let (response, painter) = ui.allocate_painter(canvas, Sense::click());
    let rect = egui::Rect::from_center_size(response.rect.center(), frame);
    let plot = paint(&painter, rect, spec, Some(overlay));
    if spec.panels.is_empty() {
        let ctrl = ui.input(|input| input.modifiers.command || input.modifiers.ctrl);
        if ctrl
            && response.clicked()
            && let Some(position) = response.interact_pointer_pos()
            && let Some(index) = nearest_point(plot, spec, position)
        {
            if let Some(at) = marks.iter().position(|(point, _)| *point == index) {
                marks.remove(at);
            } else {
                marks.push((index, 0));
            }
        }
        for (index, _) in marks.iter() {
            if let Some(point) = spec.points.get(*index) {
                let x0 = spec.x_axis.min;
                let y0 = spec.y_axis.min;
                let xspan = (spec.x_axis.max - x0).abs().max(f64::MIN_POSITIVE);
                let yspan = (spec.y_axis.max - y0).abs().max(f64::MIN_POSITIVE);
                let x = plot.left() + ((point.x - x0) / xspan) as f32 * plot.width();
                let y = plot.bottom() - ((point.y - y0) / yspan) as f32 * plot.height();
                painter.circle_stroke(
                    pos2(x, y),
                    6.0,
                    Stroke::new(2.0, Color32::from_rgb(198, 64, 0)),
                );
            }
        }
    }
    response
}

/// Index of the data point nearest to `position`, within a few pixels.
fn nearest_point(plot: Rect, spec: &PlotSpec, position: Pos2) -> Option<usize> {
    let x0 = spec.x_axis.min;
    let y0 = spec.y_axis.min;
    let xspan = (spec.x_axis.max - x0).abs().max(f64::MIN_POSITIVE);
    let yspan = (spec.y_axis.max - y0).abs().max(f64::MIN_POSITIVE);
    let mut best: Option<(f32, usize)> = None;
    for (index, point) in spec.points.iter().enumerate() {
        let x = plot.left() + ((point.x - x0) / xspan) as f32 * plot.width();
        let y = plot.bottom() - ((point.y - y0) / yspan) as f32 * plot.height();
        let distance = (x - position.x).hypot(y - position.y);
        if distance <= 10.0 && best.is_none_or(|(current, _)| distance < current) {
            best = Some((distance, index));
        }
    }
    best.map(|(_, index)| index)
}

/// Font for the text drawn on a graph, applying the typeface and scale the
/// user chose in the graph window.
fn graph_font(size: f32, overlay: Option<&GraphOverlay>) -> FontId {
    let Some(overlay) = overlay else {
        return FontId::proportional(size);
    };
    let family = match overlay.font_family {
        GraphFontFamily::Sans => egui::FontFamily::Proportional,
        GraphFontFamily::Serif => egui::FontFamily::Name("serif".into()),
        GraphFontFamily::Mono => egui::FontFamily::Monospace,
    };
    FontId::new(overlay.text_size(size), family)
}

fn paint(
    painter: &egui::Painter,
    rect: Rect,
    spec: &PlotSpec,
    overlay: Option<&GraphOverlay>,
) -> Rect {
    if !spec.panels.is_empty() {
        paint_panels(painter, rect, spec, overlay);
        return rect;
    }
    painter.rect_filled(rect, 0.0, Color32::WHITE);

    // A secondary axis puts its ticks and title to the right of the frame,
    // so it needs a wider margin than a plain plot. The notes need a margin of
    // their own beyond that, so they never sit over the data.
    let base_right = if spec.secondary.is_some() {
        78.0
    } else {
        RIGHT
    };
    let show_report = overlay.is_none_or(|overlay| overlay.show_report);
    let note_content = if show_report {
        note_content_width(painter, spec, overlay)
    } else {
        0.0
    };
    let note_extent = if spec.notes.is_empty() {
        0.0
    } else {
        note_content + 20.0
    };
    let right = base_right + note_extent;
    // A subtitle takes its own line under the title, so the frame starts lower
    // and the text never sits over the top tick.
    let has_subtitle = overlay.is_some_and(|overlay| overlay.subtitle.is_some());
    let scale = overlay.map_or(1.0, GraphOverlay::font_scale);
    let top = if has_subtitle {
        46.0 + 10.0 * (scale - 1.0).max(0.0)
    } else {
        TOP
    };
    let plot = Rect::from_min_max(
        pos2(rect.left() + LEFT, rect.top() + top),
        pos2(rect.right() - right, rect.bottom() - BOTTOM),
    );

    if let Some(overlay) = overlay {
        let xspan = if (spec.x_axis.max - spec.x_axis.min).abs() > 0.0 {
            spec.x_axis.max - spec.x_axis.min
        } else {
            1.0
        };
        let yspan = if (spec.y_axis.max - spec.y_axis.min).abs() > 0.0 {
            spec.y_axis.max - spec.y_axis.min
        } else {
            1.0
        };
        for line in &overlay.reference_lines {
            // A line kept from another graph may fall outside this axis
            // range; drawing it would leave a stray mark by the frame.
            let inside = match line.axis {
                ReferenceAxis::Vertical => {
                    line.value >= spec.x_axis.min - 1e-12 && line.value <= spec.x_axis.max + 1e-12
                }
                ReferenceAxis::Horizontal => {
                    line.value >= spec.y_axis.min - 1e-12 && line.value <= spec.y_axis.max + 1e-12
                }
            };
            if !inside {
                continue;
            }
            let color = Color32::from_rgb(192, 57, 43);
            let stroke = Stroke::new(1.2, color);
            match line.axis {
                ReferenceAxis::Vertical => {
                    let x = plot.left()
                        + ((line.value - spec.x_axis.min) / xspan) as f32 * plot.width();
                    painter.add(Shape::dashed_line(
                        &[pos2(x, plot.top()), pos2(x, plot.bottom())],
                        stroke,
                        4.0,
                        3.0,
                    ));
                    if let Some(label) = &line.label {
                        painter.text(
                            pos2(x, plot.top() - 2.0),
                            Align2::CENTER_BOTTOM,
                            label,
                            graph_font(11.0, Some(overlay)),
                            color,
                        );
                    }
                }
                ReferenceAxis::Horizontal => {
                    let y = plot.bottom()
                        - ((line.value - spec.y_axis.min) / yspan) as f32 * plot.height();
                    painter.add(Shape::dashed_line(
                        &[pos2(plot.left(), y), pos2(plot.right(), y)],
                        stroke,
                        4.0,
                        3.0,
                    ));
                    if let Some(label) = &line.label {
                        painter.text(
                            pos2(plot.right() + 4.0, y),
                            Align2::LEFT_CENTER,
                            label,
                            graph_font(11.0, Some(overlay)),
                            color,
                        );
                    }
                }
            }
        }
    }

    let (x0, x1) = (spec.x_axis.min, spec.x_axis.max);
    let (y0, y1) = (spec.y_axis.min, spec.y_axis.max);
    let xspan = if (x1 - x0).abs() > 0.0 { x1 - x0 } else { 1.0 };
    let yspan = if (y1 - y0).abs() > 0.0 { y1 - y0 } else { 1.0 };

    let sx = |x: f64| plot.left() + ((x - x0) / xspan) as f32 * plot.width();
    let sy = |y: f64| plot.bottom() - ((y - y0) / yspan) as f32 * plot.height();

    let grid = Stroke::new(1.0, Color32::from_gray(232));
    let tick = Color32::from_gray(70);
    let font = graph_font(11.5, overlay);

    // Minor ticks: short marks on the axis edge, no grid line and no label.
    for x in &spec.x_axis.minor {
        let px = sx(*x);
        painter.line_segment(
            [pos2(px, plot.bottom()), pos2(px, plot.bottom() - 4.0)],
            Stroke::new(1.0, tick),
        );
    }
    for y in &spec.y_axis.minor {
        let py = sy(*y);
        painter.line_segment(
            [pos2(plot.left(), py), pos2(plot.left() + 4.0, py)],
            Stroke::new(1.0, tick),
        );
    }
    for t in &spec.x_axis.ticks {
        let px = sx(t.position);
        if overlay.is_none_or(|overlay| overlay.grid) {
            painter.line_segment([pos2(px, plot.top()), pos2(px, plot.bottom())], grid);
        }
        painter.text(
            pos2(px, plot.bottom() + 5.0),
            Align2::CENTER_TOP,
            &t.label,
            font.clone(),
            tick,
        );
    }
    for t in &spec.y_axis.ticks {
        let py = sy(t.position);
        if overlay.is_none_or(|overlay| overlay.grid) {
            painter.line_segment([pos2(plot.left(), py), pos2(plot.right(), py)], grid);
        }
        painter.text(
            pos2(plot.left() - 6.0, py),
            Align2::RIGHT_CENTER,
            &t.label,
            font.clone(),
            tick,
        );
    }

    if let Some(axis) = &spec.secondary {
        let span = if (axis.max - axis.min).abs() > 0.0 {
            axis.max - axis.min
        } else {
            1.0
        };
        for t in &axis.ticks {
            let py = plot.bottom() - ((t.position - axis.min) / span) as f32 * plot.height();
            painter.line_segment(
                [pos2(plot.right(), py), pos2(plot.right() + 5.0, py)],
                Stroke::new(1.0, tick),
            );
            painter.text(
                pos2(plot.right() + 8.0, py),
                Align2::LEFT_CENTER,
                &t.label,
                font.clone(),
                tick,
            );
        }
        // The right-hand title is rotated, like the Y title: horizontal text
        // here ran past the window edge on narrow charts.
        let galley = painter.layout_no_wrap(axis.label.clone(), font.clone(), tick);
        let anchor = pos2(plot.right() + 68.0, plot.center().y + galley.size().x / 2.0);
        painter.add(Shape::Text(
            TextShape::new(anchor, galley, tick).with_angle(-std::f32::consts::FRAC_PI_2),
        ));
    }

    painter.rect_stroke(
        plot,
        0.0,
        Stroke::new(1.0, Color32::BLACK),
        StrokeKind::Inside,
    );

    let clipped = painter.with_clip_rect(plot);
    let show_symbols = overlay.is_none_or(|overlay| overlay.show_symbols);
    let show_lines = overlay.is_none_or(|overlay| overlay.show_lines);
    let show_bands = overlay.is_none_or(|overlay| overlay.show_bands);
    for band in &spec.bands {
        if !show_bands || band.lower.len() < 2 || band.lower.len() != band.upper.len() {
            continue;
        }
        let fill = Color32::from_rgb(226, 226, 226);
        for i in 1..band.lower.len() {
            let quad = [
                pos2(sx(band.lower[i - 1].x), sy(band.lower[i - 1].y)),
                pos2(sx(band.lower[i].x), sy(band.lower[i].y)),
                pos2(sx(band.upper[i].x), sy(band.upper[i].y)),
                pos2(sx(band.upper[i - 1].x), sy(band.upper[i - 1].y)),
            ];
            clipped.add(Shape::convex_polygon(
                quad.to_vec(),
                fill,
                Stroke::new(1.0, fill),
            ));
        }
    }
    for seg in &spec.lines {
        if !show_lines {
            break;
        }
        clipped.line_segment(
            [
                pos2(sx(seg.from.x), sy(seg.from.y)),
                pos2(sx(seg.to.x), sy(seg.to.y)),
            ],
            Stroke::new(seg.width as f32, Color32::from_rgb(192, 0, 0)),
        );
    }
    if show_symbols {
        for p in &spec.points {
            clipped.circle_filled(pos2(sx(p.x), sy(p.y)), 2.6, Color32::from_rgb(31, 78, 121));
        }
    }

    for (index, series) in spec.series.iter().enumerate() {
        let color = series_color32(index);
        for band in &series.bands {
            if !show_bands || band.lower.len() < 2 || band.lower.len() != band.upper.len() {
                continue;
            }
            let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 30);
            for i in 1..band.lower.len() {
                let quad = [
                    pos2(sx(band.lower[i - 1].x), sy(band.lower[i - 1].y)),
                    pos2(sx(band.lower[i].x), sy(band.lower[i].y)),
                    pos2(sx(band.upper[i].x), sy(band.upper[i].y)),
                    pos2(sx(band.upper[i - 1].x), sy(band.upper[i - 1].y)),
                ];
                clipped.add(Shape::convex_polygon(
                    quad.to_vec(),
                    fill,
                    Stroke::new(1.0, fill),
                ));
            }
        }
        let map_y = |value: f64| -> f32 {
            match (&spec.secondary, series.secondary_axis) {
                (Some(axis), true) => {
                    let span = if (axis.max - axis.min).abs() > 0.0 {
                        axis.max - axis.min
                    } else {
                        1.0
                    };
                    plot.bottom() - ((value - axis.min) / span) as f32 * plot.height()
                }
                _ => sy(value),
            }
        };
        for seg in &series.lines {
            if !show_lines {
                break;
            }
            clipped.line_segment(
                [
                    pos2(sx(seg.from.x), map_y(seg.from.y)),
                    pos2(sx(seg.to.x), map_y(seg.to.y)),
                ],
                Stroke::new(seg.width as f32, color),
            );
        }
        if show_symbols {
            for p in &series.points {
                clipped.circle_filled(pos2(sx(p.x), map_y(p.y)), 2.6, color);
            }
        }
    }

    if spec.series.len() > 1 {
        let max_len = spec
            .series
            .iter()
            .map(|series| series.label.chars().count())
            .max()
            .unwrap_or(0);
        let legend_w = max_len as f32 * 5.5 + 44.0;
        let legend_h = spec.series.len() as f32 * 13.0 + 8.0;
        painter.rect_filled(
            Rect::from_min_size(
                pos2(plot.right() - legend_w, plot.top() + 4.0),
                vec2(legend_w, legend_h),
            ),
            0.0,
            Color32::from_rgba_unmultiplied(255, 255, 255, 225),
        );
        painter.rect_stroke(
            Rect::from_min_size(
                pos2(plot.right() - legend_w, plot.top() + 4.0),
                vec2(legend_w, legend_h),
            ),
            0.0,
            Stroke::new(1.0, Color32::from_gray(210)),
            StrokeKind::Inside,
        );
        let legend_left = plot.right() - legend_w;
        let mut y = plot.top() + 14.0;
        for (index, series) in spec.series.iter().enumerate() {
            let color = series_color32(index);
            painter.line_segment(
                [pos2(legend_left + 6.0, y), pos2(legend_left + 20.0, y)],
                Stroke::new(2.0, color),
            );
            painter.text(
                pos2(legend_left + 26.0, y),
                Align2::LEFT_CENTER,
                &series.label,
                graph_font(11.0, overlay),
                Color32::from_gray(40),
            );
            y += 13.0;
        }
    }

    if show_report && !spec.notes.is_empty() {
        let font = graph_font(12.0, overlay);
        let line_h = overlay.map_or(15.0, |overlay| overlay.text_size(15.0));
        let note_left = plot.right() + base_right + 4.0;
        let mut y = plot.top() + 2.0;
        for note in &spec.notes {
            let content_w = single_note_width(painter, note, overlay);
            let value_right = note_left + content_w;
            for row in &note.rows {
                painter.text(
                    pos2(note_left, y),
                    Align2::LEFT_TOP,
                    &row.label,
                    font.clone(),
                    Color32::from_gray(34),
                );
                painter.text(
                    pos2(value_right, y),
                    Align2::RIGHT_TOP,
                    &row.value,
                    font.clone(),
                    Color32::from_gray(34),
                );
                y += line_h;
            }
            for line in &note.lines {
                painter.text(
                    pos2(note_left, y),
                    Align2::LEFT_TOP,
                    line,
                    font.clone(),
                    Color32::from_gray(34),
                );
                y += 15.0;
            }
            y += 4.0;
        }
    }

    if let Some(overlay) = overlay {
        for annotation in &overlay.annotations {
            let x = plot.left() + annotation.x.clamp(0.0, 1.0) * plot.width();
            let y = plot.top() + annotation.y.clamp(0.0, 1.0) * plot.height();
            let color = match annotation.color {
                fp_core::plot::AnnotationColor::Black => Color32::from_gray(34),
                fp_core::plot::AnnotationColor::Red => Color32::from_rgb(176, 42, 55),
                fp_core::plot::AnnotationColor::Blue => Color32::from_rgb(31, 78, 121),
                fp_core::plot::AnnotationColor::Green => Color32::from_rgb(46, 125, 50),
            };
            let galley = painter.layout_no_wrap(
                annotation.text.clone(),
                graph_font(annotation.size.clamp(6.0, 48.0), Some(overlay)),
                color,
            );
            if annotation.angle.abs() < 1e-6 {
                let size = galley.size();
                let rect_annotation =
                    Rect::from_min_size(pos2(x - 4.0, y - 2.0), vec2(size.x + 8.0, size.y + 4.0));
                painter.rect_filled(
                    rect_annotation,
                    2.0,
                    Color32::from_rgba_unmultiplied(255, 255, 255, 217),
                );
                painter.galley(pos2(x, y), galley, color);
            } else {
                painter.add(Shape::Text(
                    TextShape::new(pos2(x, y), galley, color)
                        .with_angle(-annotation.angle.to_radians()),
                ));
            }
        }
        if let Some(mode) = overlay.data_labels {
            let font = graph_font(10.0, Some(overlay));
            for (index, point) in spec.points.iter().enumerate() {
                let label = match mode {
                    fp_core::plot::DataLabelMode::YValue => format!("{:.4}", point.y),
                    fp_core::plot::DataLabelMode::Index => (index + 1).to_string(),
                };
                painter.text(
                    pos2(sx(point.x) + 5.0, sy(point.y) - 5.0),
                    Align2::LEFT_BOTTOM,
                    label,
                    font.clone(),
                    Color32::from_gray(51),
                );
            }
        }
    }

    painter.text(
        pos2(rect.center().x, rect.top() + 4.0),
        Align2::CENTER_TOP,
        &spec.title,
        graph_font(16.0, overlay),
        Color32::BLACK,
    );
    if let Some(overlay) = overlay {
        if let Some(subtitle) = &overlay.subtitle {
            painter.text(
                pos2(rect.center().x, rect.top() + 23.0),
                Align2::CENTER_TOP,
                subtitle,
                graph_font(13.0, Some(overlay)),
                Color32::from_gray(70),
            );
        }
        if let Some(footer) = &overlay.footer {
            painter.text(
                pos2(rect.center().x, rect.bottom() - 2.0),
                Align2::CENTER_BOTTOM,
                footer,
                graph_font(12.0, Some(overlay)),
                Color32::from_gray(85),
            );
        }
    }
    // The axis title sits under the tick labels, not through them: on a short
    // panel the old fixed offset put it on top of the numbers. A footer keeps
    // its own line at the bottom, so the title moves up to make room.
    let has_footer = overlay.is_some_and(|overlay| overlay.footer.is_some());
    let (x_label_pos, x_label_anchor) = if has_footer {
        (
            pos2(plot.center().x, rect.bottom() - 30.0),
            Align2::CENTER_BOTTOM,
        )
    } else {
        (
            pos2(plot.center().x, plot.bottom() + 22.0),
            Align2::CENTER_TOP,
        )
    };
    painter.text(
        x_label_pos,
        x_label_anchor,
        &spec.x_axis.label,
        graph_font(13.5, overlay),
        Color32::from_gray(40),
    );

    let label = spec.y_axis.label.clone();
    let galley = painter.layout_no_wrap(label, graph_font(13.5, overlay), Color32::from_gray(40));
    let anchor = pos2(rect.left() + 12.0, plot.center().y + galley.size().x / 2.0);
    painter.add(Shape::Text(
        TextShape::new(anchor, galley, Color32::from_gray(40))
            .with_angle(-std::f32::consts::FRAC_PI_2),
    ));
    plot
}

/// Height reserved above and below the panel grid for the main title, the
/// subtitle and the footer.
fn panel_bands(spec: &PlotSpec, overlay: Option<&GraphOverlay>) -> (f32, f32) {
    let subtitle = overlay.and_then(|overlay| overlay.subtitle.as_ref());
    let footer = overlay.and_then(|overlay| overlay.footer.as_ref());
    let scale = overlay.map_or(1.0, GraphOverlay::font_scale);
    let title_h = if subtitle.is_some() {
        24.0 * scale + 14.0
    } else if spec.title.is_empty() {
        0.0
    } else {
        22.0 * scale
    };
    let footer_h = if footer.is_some() { 18.0 * scale } else { 0.0 };
    (title_h, footer_h)
}

/// Presentation edits for the panels: the subtitle and footer belong to the
/// whole grid, while reference lines apply to every panel's own axes.
fn panel_edits(overlay: Option<&GraphOverlay>) -> Option<GraphOverlay> {
    overlay.map(|overlay| GraphOverlay {
        subtitle: None,
        footer: None,
        reference_lines: overlay.reference_lines.clone(),
        font_family: overlay.font_family,
        font_scale: overlay.font_scale,
        annotations: overlay.annotations.clone(),
        grid: overlay.grid,
        data_labels: overlay.data_labels,
        x_range: None,
        y_range: None,
        show_symbols: overlay.show_symbols,
        show_lines: overlay.show_lines,
        show_bands: overlay.show_bands,
        show_report: overlay.show_report,
    })
}

fn paint_panels(
    painter: &egui::Painter,
    rect: Rect,
    spec: &PlotSpec,
    overlay: Option<&GraphOverlay>,
) {
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let columns = spec.panel_columns.max(1);
    let rows = spec.panels.len().div_ceil(columns);
    let margin = 8.0;
    let (title_h, footer_h) = panel_bands(spec, overlay);
    if !spec.title.is_empty() {
        painter.text(
            pos2(rect.center().x, rect.top() + 4.0),
            Align2::CENTER_TOP,
            &spec.title,
            graph_font(16.0, overlay),
            Color32::BLACK,
        );
    }
    if let Some(subtitle) = overlay.and_then(|overlay| overlay.subtitle.as_ref()) {
        let scale = overlay.map_or(1.0, GraphOverlay::font_scale);
        let y = if spec.title.is_empty() {
            rect.top() + 4.0
        } else {
            rect.top() + 8.0 + 16.0 * scale
        };
        painter.text(
            pos2(rect.center().x, y),
            Align2::CENTER_TOP,
            subtitle,
            graph_font(13.0, overlay),
            Color32::from_gray(70),
        );
    }
    if let Some(footer) = overlay.and_then(|overlay| overlay.footer.as_ref()) {
        painter.text(
            pos2(rect.center().x, rect.bottom() - 2.0),
            Align2::CENTER_BOTTOM,
            footer,
            graph_font(12.0, overlay),
            Color32::from_gray(85),
        );
    }
    let edits = panel_edits(overlay);
    let cell_w = (rect.width() - 2.0 * margin) / columns as f32;
    let cell_h = (rect.height() - title_h - footer_h - 2.0 * margin) / rows as f32;
    for (index, panel) in spec.panels.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let child = Rect::from_min_size(
            pos2(
                rect.left() + margin + column as f32 * cell_w,
                rect.top() + title_h + margin + row as f32 * cell_h,
            ),
            vec2(cell_w, cell_h),
        );
        paint(painter, child, panel, edits.as_ref());
    }
}

/// Canvas and frame for a multi-panel plot. The panels keep their natural cell
/// shape when the window has room; otherwise they shrink to the window (down
/// to a readable minimum) so a chart such as I–MR shows both panels without
/// scrolling. When the natural height is shorter than the window the grid is
/// centred in it.
fn fitted_panels(
    canvas: egui::Vec2,
    columns: usize,
    rows: usize,
    ratio: f32,
    title_h: f32,
    footer_h: f32,
) -> (egui::Vec2, egui::Vec2) {
    let cell_w = (canvas.x - 16.0) / columns.max(1) as f32;
    let bands = title_h + footer_h;
    let natural = bands + rows as f32 * cell_w * ratio + 16.0;
    let readable = bands + rows as f32 * 150.0 + 16.0;
    let height = if natural <= canvas.y {
        natural
    } else {
        canvas.y.max(readable)
    };
    (vec2(canvas.x, height.max(canvas.y)), vec2(canvas.x, height))
}

/// Canvas and frame for a single plot: the plot keeps its own proportions and
/// is fitted inside the canvas, so the spare room becomes an even band on both
/// sides instead of a frame glued to one corner.
fn fitted_frame(canvas: egui::Vec2, natural: egui::Vec2) -> (egui::Vec2, egui::Vec2) {
    let aspect = natural.y / natural.x.max(1.0);
    let mut frame_w = canvas.x;
    let mut frame_h = frame_w * aspect;
    if frame_h > canvas.y {
        frame_h = canvas.y;
        frame_w = frame_h / aspect;
    }
    (canvas, vec2(frame_w.max(160.0), frame_h.max(120.0)))
}

/// Width of one note's columns, measured with the same font the note uses.
fn single_note_width(
    painter: &egui::Painter,
    note: &fp_core::plot::PlotNote,
    overlay: Option<&GraphOverlay>,
) -> f32 {
    let font = graph_font(12.0, overlay);
    let label = note
        .rows
        .iter()
        .map(|row| {
            painter
                .layout_no_wrap(row.label.clone(), font.clone(), Color32::from_gray(34))
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let value = note
        .rows
        .iter()
        .map(|row| {
            painter
                .layout_no_wrap(row.value.clone(), font.clone(), Color32::from_gray(34))
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let lines = note
        .lines
        .iter()
        .map(|line| {
            painter
                .layout_no_wrap(line.clone(), font.clone(), Color32::from_gray(34))
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    if note.rows.is_empty() {
        lines
    } else if note.lines.is_empty() {
        label + 16.0 + value
    } else {
        (label + 16.0 + value).max(lines)
    }
}

/// Widest note of the plot, zero when there is none.
fn note_content_width(
    painter: &egui::Painter,
    spec: &PlotSpec,
    overlay: Option<&GraphOverlay>,
) -> f32 {
    spec.notes
        .iter()
        .map(|note| single_note_width(painter, note, overlay))
        .fold(0.0_f32, f32::max)
}

fn series_color32(index: usize) -> Color32 {
    match index % 6 {
        0 => Color32::from_rgb(31, 78, 121),
        1 => Color32::from_rgb(192, 0, 0),
        2 => Color32::from_rgb(46, 125, 50),
        3 => Color32::from_rgb(106, 27, 154),
        4 => Color32::from_rgb(239, 108, 0),
        _ => Color32::from_rgb(0, 131, 143),
    }
}

/// Small helper used by tests: the plot rectangle for a given outer rectangle.
#[must_use]
pub fn plot_rect(outer: Rect) -> Rect {
    Rect::from_min_max(
        Pos2::new(outer.left() + LEFT, outer.top() + TOP),
        Pos2::new(outer.right() - RIGHT, outer.bottom() - BOTTOM),
    )
}
