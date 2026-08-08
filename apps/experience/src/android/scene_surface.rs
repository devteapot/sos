use std::{cell::RefCell, rc::Rc};

use experience_ir::{ClipRect, HitRegion, Interaction, PaintOp, SceneEvent, Transform2D};
use gpui::{
    canvas, div, point, prelude::*, px, quad, rgb, transparent_black, AnyElement, BorderStyle,
    Bounds, ContentMask, Context, Font, FontStyle, FontWeight, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, ShapedLine, SharedString, TextAlign,
    TextRun,
};

use super::{pointer_input, ExperienceHost};

enum PreparedPaint {
    FillBounds {
        color: u32,
        radius: f32,
    },
    Path {
        points: Vec<experience_ir::PaintPoint>,
        color: u32,
        width: Option<f32>,
        closed: bool,
    },
    Quad {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        color: u32,
    },
    Glyphs {
        x: f32,
        y: f32,
        size: f32,
        line_height: Option<f32>,
        max_width: Option<f32>,
        line: ShapedLine,
    },
    Layer {
        clip: Option<ClipRect>,
        transform: Transform2D,
        opacity: f32,
        operations: Vec<PreparedPaint>,
    },
}

#[derive(Clone, Copy)]
struct Affine {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    tx: f32,
    ty: f32,
}

impl Affine {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    fn from_scene(value: Transform2D) -> Self {
        let angle = value.rotation_degrees.to_radians();
        let (sin, cos) = angle.sin_cos();
        Self {
            a: cos * value.scale_x,
            b: sin * value.scale_x,
            c: -sin * value.scale_y,
            d: cos * value.scale_y,
            tx: value.translate_x,
            ty: value.translate_y,
        }
    }

    fn then(self, local: Self) -> Self {
        Self {
            a: self.a * local.a + self.c * local.b,
            b: self.b * local.a + self.d * local.b,
            c: self.a * local.c + self.c * local.d,
            d: self.b * local.c + self.d * local.d,
            tx: self.a * local.tx + self.c * local.ty + self.tx,
            ty: self.b * local.tx + self.d * local.ty + self.ty,
        }
    }

    fn point(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.tx,
            self.b * x + self.d * y + self.ty,
        )
    }

    fn scale(self) -> f32 {
        ((self.a.hypot(self.b) + self.c.hypot(self.d)) * 0.5).max(0.001)
    }

    fn axis_aligned(self) -> bool {
        self.b.abs() < 0.0001 && self.c.abs() < 0.0001
    }
}

pub(super) fn render(
    node_id: String,
    operations: Vec<PaintOp>,
    interaction: Interaction,
    host: gpui::WeakEntity<ExperienceHost>,
    _cx: &mut Context<ExperienceHost>,
) -> AnyElement {
    let bounds = Rc::new(RefCell::new(None::<Bounds<Pixels>>));
    let prepaint_bounds = bounds.clone();
    let prepaint_id = node_id.clone();
    let prepaint_interaction = interaction.clone();
    let prepared = canvas(
        move |canvas_bounds, window, _| {
            *prepaint_bounds.borrow_mut() = Some(canvas_bounds);
            pointer_input::record_surface(&prepaint_id, canvas_bounds, &prepaint_interaction);
            prepare(operations, window, Affine::identity(), 1.0)
        },
        move |canvas_bounds, operations, window, cx| {
            paint(
                &operations,
                canvas_bounds,
                Affine::identity(),
                1.0,
                false,
                window,
                cx,
            );
        },
    )
    .size_full();

    let down_id = node_id.clone();
    let down_spec = interaction.clone();
    let down_bounds = bounds.clone();
    let down_host = host.clone();
    let move_id = node_id.clone();
    let move_spec = interaction.clone();
    let move_bounds = bounds.clone();
    let move_host = host.clone();
    let up_id = node_id.clone();
    let up_spec = interaction;
    let up_bounds = bounds;

    div()
        .id(SharedString::from(node_id))
        .size_full()
        .child(prepared)
        .on_mouse_down(
            MouseButton::Left,
            move |event: &MouseDownEvent, window, app| {
                let Some(bounds) = down_bounds.borrow().as_ref().copied() else {
                    return;
                };
                let Some((region, x, y)) = hit(&down_spec, &down_id, bounds, event.position) else {
                    return;
                };
                window.prevent_default();
                app.stop_propagation();
                let _ = down_host.update(app, |host, cx| {
                    host.scene_surface_down(down_id.clone(), region, x, y, event.click_count, cx)
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
                host.scene_surface_move(move_id.clone(), &move_spec, x, y, cx)
            });
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
                    host.scene_surface_up(up_id.clone(), &up_spec, x, y, cx)
                });
            },
        )
        .into_any_element()
}

fn prepare(
    operations: Vec<PaintOp>,
    window: &mut gpui::Window,
    transform: Affine,
    opacity: f32,
) -> Vec<PreparedPaint> {
    operations
        .into_iter()
        .map(|operation| match operation {
            PaintOp::FillBounds { color, radius } => PreparedPaint::FillBounds { color, radius },
            PaintOp::Path {
                points,
                color,
                width,
                closed,
            } => PreparedPaint::Path {
                points,
                color,
                width,
                closed,
            },
            PaintOp::Quad {
                x,
                y,
                width,
                height,
                radius,
                color,
            } => PreparedPaint::Quad {
                x,
                y,
                width,
                height,
                radius,
                color,
            },
            PaintOp::Glyphs {
                x,
                y,
                size,
                line_height,
                max_width,
                runs,
            } => {
                let scale = transform.scale();
                let mut text = String::new();
                let text_runs = runs
                    .into_iter()
                    .map(|run| {
                        text.push_str(&run.text);
                        TextRun {
                            len: run.text.len(),
                            font: Font {
                                family: run
                                    .font_family
                                    .unwrap_or_else(|| ".SystemUIFont".into())
                                    .into(),
                                features: Default::default(),
                                fallbacks: None,
                                weight: FontWeight(run.weight as f32),
                                style: if run.italic {
                                    FontStyle::Italic
                                } else {
                                    FontStyle::Normal
                                },
                            },
                            color: faded(run.color, opacity).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }
                    })
                    .collect::<Vec<_>>();
                let line = window.text_system().shape_line(
                    text.into(),
                    px(size * scale),
                    &text_runs,
                    None,
                );
                PreparedPaint::Glyphs {
                    x,
                    y,
                    size: size * scale,
                    line_height: line_height.map(|height| height * scale),
                    max_width: max_width.map(|width| width * scale),
                    line,
                }
            }
            PaintOp::Layer {
                clip,
                transform: local_transform,
                opacity: local_opacity,
                operations,
            } => {
                let combined = transform.then(Affine::from_scene(local_transform));
                PreparedPaint::Layer {
                    clip,
                    transform: local_transform,
                    opacity: local_opacity,
                    operations: prepare(operations, window, combined, opacity * local_opacity),
                }
            }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn paint(
    operations: &[PreparedPaint],
    canvas_bounds: Bounds<Pixels>,
    transform: Affine,
    opacity: f32,
    draw_fill_bounds: bool,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) {
    for operation in operations {
        match operation {
            PreparedPaint::FillBounds { color, radius } if draw_fill_bounds => {
                paint_quad(
                    0.0,
                    0.0,
                    f32::from(canvas_bounds.size.width),
                    f32::from(canvas_bounds.size.height),
                    *radius,
                    *color,
                    canvas_bounds,
                    transform,
                    opacity,
                    window,
                );
            }
            PreparedPaint::FillBounds { .. } => {}
            PreparedPaint::Path {
                points,
                color,
                width,
                closed,
            } => {
                let mut points = points.iter();
                let Some(first) = points.next() else { continue };
                let mut builder = width
                    .map(|width| PathBuilder::stroke(px(width * transform.scale())))
                    .unwrap_or_else(PathBuilder::fill);
                let (x, y) = transform.point(first.x, first.y);
                builder.move_to(point(
                    canvas_bounds.origin.x + px(x),
                    canvas_bounds.origin.y + px(y),
                ));
                for value in points {
                    let (x, y) = transform.point(value.x, value.y);
                    builder.line_to(point(
                        canvas_bounds.origin.x + px(x),
                        canvas_bounds.origin.y + px(y),
                    ));
                }
                if *closed {
                    builder.close();
                }
                if let Ok(path) = builder.build() {
                    window.paint_path(path, faded(*color, opacity));
                }
            }
            PreparedPaint::Quad {
                x,
                y,
                width,
                height,
                radius,
                color,
            } => paint_quad(
                *x,
                *y,
                *width,
                *height,
                *radius,
                *color,
                canvas_bounds,
                transform,
                opacity,
                window,
            ),
            PreparedPaint::Glyphs {
                x,
                y,
                size,
                line_height,
                max_width,
                line,
            } => {
                let (x, y) = transform.point(*x, *y);
                let line_height = px(line_height.unwrap_or(size * 1.2));
                let origin = point(
                    canvas_bounds.origin.x + px(x),
                    canvas_bounds.origin.y + px(y),
                );
                if let Some(width) = max_width {
                    let mask = ContentMask {
                        bounds: Bounds::new(origin, gpui::size(px(*width), line_height)),
                    };
                    window.with_content_mask(Some(mask), |window| {
                        let _ = line.paint(origin, line_height, TextAlign::Left, None, window, cx);
                    });
                } else {
                    let _ = line.paint(origin, line_height, TextAlign::Left, None, window, cx);
                }
            }
            PreparedPaint::Layer {
                clip,
                transform: local,
                opacity: local_opacity,
                operations,
            } => {
                let transform = transform.then(Affine::from_scene(*local));
                let opacity = opacity * local_opacity;
                let layer_bounds = clip
                    .map(|clip| transformed_bounds(canvas_bounds, clip, transform))
                    .unwrap_or(canvas_bounds);
                let mask = clip.map(|_| ContentMask {
                    bounds: layer_bounds,
                });
                window.with_content_mask(mask, |window| {
                    window.paint_layer(layer_bounds, |window| {
                        paint(
                            operations,
                            canvas_bounds,
                            transform,
                            opacity,
                            true,
                            window,
                            cx,
                        )
                    })
                });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_quad(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
    color: u32,
    canvas_bounds: Bounds<Pixels>,
    transform: Affine,
    opacity: f32,
    window: &mut gpui::Window,
) {
    if transform.axis_aligned() {
        let (left, top) = transform.point(x, y);
        let (right, bottom) = transform.point(x + width, y + height);
        window.paint_quad(quad(
            Bounds::from_corners(
                point(
                    canvas_bounds.origin.x + px(left.min(right)),
                    canvas_bounds.origin.y + px(top.min(bottom)),
                ),
                point(
                    canvas_bounds.origin.x + px(left.max(right)),
                    canvas_bounds.origin.y + px(top.max(bottom)),
                ),
            ),
            px(radius * transform.scale()),
            faded(color, opacity),
            px(0.0),
            transparent_black(),
            BorderStyle::default(),
        ));
        return;
    }

    let corners = [
        transform.point(x, y),
        transform.point(x + width, y),
        transform.point(x + width, y + height),
        transform.point(x, y + height),
    ];
    let mut builder = PathBuilder::fill();
    builder.move_to(point(
        canvas_bounds.origin.x + px(corners[0].0),
        canvas_bounds.origin.y + px(corners[0].1),
    ));
    for (x, y) in &corners[1..] {
        builder.line_to(point(
            canvas_bounds.origin.x + px(*x),
            canvas_bounds.origin.y + px(*y),
        ));
    }
    builder.close();
    if let Ok(path) = builder.build() {
        window.paint_path(path, faded(color, opacity));
    }
}

fn transformed_bounds(
    canvas_bounds: Bounds<Pixels>,
    clip: ClipRect,
    transform: Affine,
) -> Bounds<Pixels> {
    let points = [
        transform.point(clip.x, clip.y),
        transform.point(clip.x + clip.width, clip.y),
        transform.point(clip.x + clip.width, clip.y + clip.height),
        transform.point(clip.x, clip.y + clip.height),
    ];
    let min_x = points.iter().map(|point| point.0).fold(f32::MAX, f32::min);
    let max_x = points.iter().map(|point| point.0).fold(f32::MIN, f32::max);
    let min_y = points.iter().map(|point| point.1).fold(f32::MAX, f32::min);
    let max_y = points.iter().map(|point| point.1).fold(f32::MIN, f32::max);
    Bounds::from_corners(
        point(
            canvas_bounds.origin.x + px(min_x),
            canvas_bounds.origin.y + px(min_y),
        ),
        point(
            canvas_bounds.origin.x + px(max_x),
            canvas_bounds.origin.y + px(max_y),
        ),
    )
}

fn faded(color: u32, opacity: f32) -> gpui::Rgba {
    let mut color = rgb(color);
    color.a *= opacity.clamp(0.0, 1.0);
    color
}

fn local_point(bounds: Bounds<Pixels>, point: gpui::Point<Pixels>) -> (f32, f32) {
    (
        f32::from(point.x - bounds.origin.x),
        f32::from(point.y - bounds.origin.y),
    )
}

fn hit(
    interaction: &Interaction,
    node_id: &str,
    bounds: Bounds<Pixels>,
    point: gpui::Point<Pixels>,
) -> Option<(HitRegion, f32, f32)> {
    let (x, y) = local_point(bounds, point);
    if let Some(region) = interaction.hit_regions.iter().rev().find(|region| {
        x >= region.x
            && x <= region.x + region.width
            && y >= region.y
            && y <= region.y + region.height
    }) {
        return Some((region.clone(), x, y));
    }
    if interaction.tap_action.is_none()
        && interaction.double_tap_action.is_none()
        && interaction.long_press_action.is_none()
        && interaction.swipe_action.is_none()
    {
        return None;
    }
    Some((
        HitRegion {
            id: node_id.into(),
            x: 0.0,
            y: 0.0,
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
            press_action: None,
            drag_action: None,
            drop_action: None,
            tap_action: interaction.tap_action.clone(),
            double_tap_action: interaction.double_tap_action.clone(),
            long_press_action: interaction.long_press_action.clone(),
            swipe_action: interaction.swipe_action.clone(),
        },
        x,
        y,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn event(
    action: String,
    target: String,
    x: f32,
    y: f32,
    phase: &str,
    delta_x: f32,
    delta_y: f32,
    velocity_x: f32,
    velocity_y: f32,
) -> SceneEvent {
    SceneEvent {
        action,
        target: Some(target),
        x: Some(x),
        y: Some(y),
        delta_x: Some(delta_x),
        delta_y: Some(delta_y),
        velocity_x: Some(velocity_x),
        velocity_y: Some(velocity_y),
        phase: Some(phase.into()),
        ..Default::default()
    }
}
