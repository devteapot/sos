use std::{cell::RefCell, rc::Rc};

use experience_ir::{Canvas, CanvasCommand, HitRegion, UiEvent};
use gpui::{
    canvas, div, point, prelude::*, px, quad, rgb, transparent_black, AnyElement, BorderStyle,
    Bounds, Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder,
    Pixels, SharedString,
};

use super::ExperienceHost;

pub(super) fn render(
    node_id: String,
    specification: Canvas,
    host: gpui::WeakEntity<ExperienceHost>,
    _cx: &mut Context<ExperienceHost>,
) -> AnyElement {
    let bounds = Rc::new(RefCell::new(None::<Bounds<Pixels>>));
    let prepaint_bounds = bounds.clone();
    let commands = specification.commands.clone();
    let drawing = canvas(
        move |canvas_bounds, _, _| {
            *prepaint_bounds.borrow_mut() = Some(canvas_bounds);
            commands
        },
        move |canvas_bounds, commands, window, _| {
            for command in commands {
                match command {
                    CanvasCommand::Path {
                        points,
                        color,
                        width,
                        closed,
                    } => {
                        let mut builder = width
                            .map(|width| PathBuilder::stroke(px(width)))
                            .unwrap_or_else(PathBuilder::fill);
                        let mut points = points.into_iter();
                        let Some(first) = points.next() else { continue };
                        builder.move_to(point(
                            canvas_bounds.origin.x + px(first.x),
                            canvas_bounds.origin.y + px(first.y),
                        ));
                        for point_value in points {
                            builder.line_to(point(
                                canvas_bounds.origin.x + px(point_value.x),
                                canvas_bounds.origin.y + px(point_value.y),
                            ));
                        }
                        if closed {
                            builder.close();
                        }
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, rgb(color));
                        }
                    }
                    CanvasCommand::Quad {
                        x,
                        y,
                        width,
                        height,
                        radius,
                        color,
                    } => window.paint_quad(quad(
                        Bounds::new(
                            point(
                                canvas_bounds.origin.x + px(x),
                                canvas_bounds.origin.y + px(y),
                            ),
                            gpui::size(px(width), px(height)),
                        ),
                        px(radius),
                        rgb(color),
                        px(0.),
                        transparent_black(),
                        BorderStyle::default(),
                    )),
                }
            }
        },
    )
    .size_full();

    let down_id = node_id.clone();
    let down_spec = specification.clone();
    let down_bounds = bounds.clone();
    let down_host = host.clone();
    let move_id = node_id.clone();
    let move_spec = specification.clone();
    let move_bounds = bounds.clone();
    let move_host = host.clone();
    let up_id = node_id.clone();
    let up_spec = specification;
    let up_bounds = bounds;

    div()
        .id(SharedString::from(node_id))
        .size_full()
        .child(drawing)
        .on_mouse_down(
            MouseButton::Left,
            move |event: &MouseDownEvent, window, app| {
                let Some(bounds) = down_bounds.borrow().as_ref().copied() else {
                    return;
                };
                let Some((region, x, y)) = hit(&down_spec, bounds, event.position) else {
                    return;
                };
                window.prevent_default();
                app.stop_propagation();
                let _ = down_host.update(app, |host, cx| {
                    host.native_canvas_down(down_id.clone(), region.clone(), x, y, cx)
                });
            },
        )
        .on_mouse_move(move |event: &MouseMoveEvent, window, app| {
            if !event.dragging() {
                return;
            }
            let Some(bounds) = move_bounds.borrow().as_ref().copied() else {
                return;
            };
            let (x, y) = local_point(bounds, event.position);
            window.prevent_default();
            app.stop_propagation();
            let _ = move_host.update(app, |host, cx| {
                host.native_canvas_move(move_id.clone(), &move_spec, x, y, cx)
            });
        })
        .on_scroll_wheel(|_, window, app| {
            window.prevent_default();
            app.stop_propagation();
        })
        .on_mouse_up(
            MouseButton::Left,
            move |event: &MouseUpEvent, window, app| {
                let Some(bounds) = up_bounds.borrow().as_ref().copied() else {
                    return;
                };
                let (x, y) = local_point(bounds, event.position);
                window.prevent_default();
                app.stop_propagation();
                let _ = host.update(app, |host, cx| {
                    host.native_canvas_up(up_id.clone(), &up_spec, x, y, cx)
                });
            },
        )
        .into_any_element()
}

fn local_point(bounds: Bounds<Pixels>, point: gpui::Point<Pixels>) -> (f32, f32) {
    (
        f32::from(point.x - bounds.origin.x),
        f32::from(point.y - bounds.origin.y),
    )
}

fn hit(
    canvas: &Canvas,
    bounds: Bounds<Pixels>,
    point: gpui::Point<Pixels>,
) -> Option<(&HitRegion, f32, f32)> {
    let (x, y) = local_point(bounds, point);
    canvas
        .hit_regions
        .iter()
        .rev()
        .find(|region| {
            x >= region.x
                && x <= region.x + region.width
                && y >= region.y
                && y <= region.y + region.height
        })
        .map(|region| (region, x, y))
}

pub(super) fn event(action: String, target: String, x: f32, y: f32) -> UiEvent {
    UiEvent {
        action,
        target: Some(target),
        x: Some(x),
        y: Some(y),
        ..Default::default()
    }
}
