//! Screen backend for [`fp_core::plot::PlotSpec`].
//!
//! Like the SVG backend, it only maps engine-owned axis coordinates to pixels:
//! it never computes a tick, a probability or a fitted value.

use egui::{
    Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, epaint::TextShape, pos2,
    vec2,
};
use fp_core::plot::PlotSpec;

const LEFT: f32 = 62.0;
const RIGHT: f32 = 18.0;
const TOP: f32 = 34.0;
const BOTTOM: f32 = 44.0;

/// Draws a plot in the available space and returns the response.
pub fn show(ui: &mut egui::Ui, spec: &PlotSpec) -> egui::Response {
    let width = ui.available_width().max(560.0);
    let height = if spec.panels.is_empty() {
        560.0
    } else {
        let columns = spec.panel_columns.max(1) as f32;
        let rows = (spec.panels.len() as f32 / columns).ceil();
        let ratio = spec
            .panels
            .first()
            .map(|panel| panel.size.1 as f32 / panel.size.0.max(1) as f32)
            .unwrap_or(0.69);
        let cell_w = (width - 16.0) / columns;
        22.0 + rows * cell_w * ratio + 16.0
    };
    let (response, painter) = ui.allocate_painter(vec2(width, height), Sense::hover());
    paint(&painter, response.rect, spec);
    response
}

fn paint(painter: &egui::Painter, rect: Rect, spec: &PlotSpec) {
    if !spec.panels.is_empty() {
        paint_panels(painter, rect, spec);
        return;
    }
    painter.rect_filled(rect, 0.0, Color32::WHITE);

    // A secondary axis puts its ticks and title to the right of the frame,
    // so it needs a wider margin than a plain plot.
    let right = if spec.secondary.is_some() {
        78.0
    } else {
        RIGHT
    };
    let plot = Rect::from_min_max(
        pos2(rect.left() + LEFT, rect.top() + TOP),
        pos2(rect.right() - right, rect.bottom() - BOTTOM),
    );

    let (x0, x1) = (spec.x_axis.min, spec.x_axis.max);
    let (y0, y1) = (spec.y_axis.min, spec.y_axis.max);
    let xspan = if (x1 - x0).abs() > 0.0 { x1 - x0 } else { 1.0 };
    let yspan = if (y1 - y0).abs() > 0.0 { y1 - y0 } else { 1.0 };

    let sx = |x: f64| plot.left() + ((x - x0) / xspan) as f32 * plot.width();
    let sy = |y: f64| plot.bottom() - ((y - y0) / yspan) as f32 * plot.height();

    let grid = Stroke::new(1.0, Color32::from_gray(232));
    let tick = Color32::from_gray(70);
    let font = FontId::proportional(11.5);

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
        painter.line_segment([pos2(px, plot.top()), pos2(px, plot.bottom())], grid);
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
        painter.line_segment([pos2(plot.left(), py), pos2(plot.right(), py)], grid);
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
        painter.text(
            pos2(plot.right() + 56.0, plot.center().y),
            Align2::CENTER_CENTER,
            &axis.label,
            font.clone(),
            tick,
        );
    }

    painter.rect_stroke(
        plot,
        0.0,
        Stroke::new(1.0, Color32::BLACK),
        StrokeKind::Inside,
    );

    let clipped = painter.with_clip_rect(plot);
    for band in &spec.bands {
        if band.lower.len() < 2 || band.lower.len() != band.upper.len() {
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
        clipped.line_segment(
            [
                pos2(sx(seg.from.x), sy(seg.from.y)),
                pos2(sx(seg.to.x), sy(seg.to.y)),
            ],
            Stroke::new(seg.width as f32, Color32::from_rgb(192, 0, 0)),
        );
    }
    for p in &spec.points {
        clipped.circle_filled(pos2(sx(p.x), sy(p.y)), 2.6, Color32::from_rgb(31, 78, 121));
    }

    for (index, series) in spec.series.iter().enumerate() {
        let color = series_color32(index);
        for band in &series.bands {
            if band.lower.len() < 2 || band.lower.len() != band.upper.len() {
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
            clipped.line_segment(
                [
                    pos2(sx(seg.from.x), map_y(seg.from.y)),
                    pos2(sx(seg.to.x), map_y(seg.to.y)),
                ],
                Stroke::new(seg.width as f32, color),
            );
        }
        for p in &series.points {
            clipped.circle_filled(pos2(sx(p.x), map_y(p.y)), 2.6, color);
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
                FontId::proportional(11.0),
                Color32::from_gray(40),
            );
            y += 13.0;
        }
    }

    painter.text(
        pos2(rect.center().x, rect.top() + 8.0),
        Align2::CENTER_TOP,
        &spec.title,
        FontId::proportional(16.0),
        Color32::BLACK,
    );
    painter.text(
        pos2(plot.center().x, rect.bottom() - 6.0),
        Align2::CENTER_BOTTOM,
        &spec.x_axis.label,
        FontId::proportional(13.5),
        Color32::from_gray(40),
    );

    let label = spec.y_axis.label.clone();
    let galley = painter.layout_no_wrap(label, FontId::proportional(13.5), Color32::from_gray(40));
    let anchor = pos2(rect.left() + 12.0, plot.center().y + galley.size().x / 2.0);
    painter.add(Shape::Text(
        TextShape::new(anchor, galley, Color32::from_gray(40))
            .with_angle(-std::f32::consts::FRAC_PI_2),
    ));
}

fn paint_panels(painter: &egui::Painter, rect: Rect, spec: &PlotSpec) {
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    let columns = spec.panel_columns.max(1);
    let rows = spec.panels.len().div_ceil(columns);
    let margin = 8.0;
    let title_h = if spec.title.is_empty() { 0.0 } else { 22.0 };
    if title_h > 0.0 {
        painter.text(
            pos2(rect.center().x, rect.top() + 4.0),
            Align2::CENTER_TOP,
            &spec.title,
            FontId::proportional(16.0),
            Color32::BLACK,
        );
    }
    let cell_w = (rect.width() - 2.0 * margin) / columns as f32;
    let cell_h = (rect.height() - title_h - 2.0 * margin) / rows as f32;
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
        paint(painter, child, panel);
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use fp_core::data::MissingPolicy;
    use fp_core::dists::Normal;
    use fp_core::plot::{PlotSpec, PlottingPosition, normal_probability_plot};

    fn spec() -> PlotSpec {
        let s = fp_core::data::Sample::new(vec![1.0, 2.0, 3.0, 4.0, 5.0], MissingPolicy::Drop)
            .expect("sample");
        let d = Normal::fit_moments(&s).expect("fit");
        let p = normal_probability_plot(&s, &d, PlottingPosition::Benard).expect("plot");
        PlotSpec::normal_probability(&p, &d, "test")
    }

    #[test]
    fn plot_rect_leaves_room_for_labels() {
        let outer = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let inner = plot_rect(outer);
        assert!(inner.left() > outer.left());
        assert!(inner.right() < outer.right());
        assert!(inner.top() > outer.top());
        assert!(inner.bottom() < outer.bottom());
    }

    #[test]
    fn headless_panel_paint_produces_shapes() {
        let s1 = fp_core::Sample::new(vec![1.0, 2.0, 3.0, 4.0], MissingPolicy::Drop).unwrap();
        let d1 = Normal::fit_moments(&s1).unwrap();
        let p1 = normal_probability_plot(&s1, &d1, PlottingPosition::Benard).unwrap();
        let a = PlotSpec::normal_probability(&p1, &d1, "A");
        let s2 = fp_core::Sample::new(vec![11.0, 12.0, 13.0, 14.0], MissingPolicy::Drop).unwrap();
        let d2 = Normal::fit_moments(&s2).unwrap();
        let p2 = normal_probability_plot(&s2, &d2, PlottingPosition::Benard).unwrap();
        let b = PlotSpec::normal_probability(&p2, &d2, "B");
        let panels = fp_core::plot::panel_plot(vec![a, b], 2, "Panels").expect("panels");
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = show(ui, &panels);
        });
        let painted = output.shapes.len();
        output.textures_delta.clear();
        assert!(painted > 0);
    }

    #[test]
    fn headless_paint_produces_shapes() {
        let ctx = egui::Context::default();
        let s = spec();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = show(ui, &s);
        });
        let painted = output.shapes.len();
        output.textures_delta.clear();
        assert!(painted > 0);
    }
}
