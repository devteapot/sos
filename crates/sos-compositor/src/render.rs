use anyhow::{Context as _, Result};
use smithay::{
    backend::renderer::{
        element::{
            surface::WaylandSurfaceRenderElement, utils::CropRenderElement, AsRenderElements,
        },
        gles::GlesRenderer,
    },
    output::Output,
    utils::{Logical, Rectangle, Scale},
};

use crate::state::SosCompositor;

smithay::backend::renderer::element::render_elements! {
    pub(crate) SosWindowRenderElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Clipped=CropRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
}

/// Render shell and compatibility windows while treating every application
/// rectangle as a hard compositor boundary. XDG clients may advertise minimum
/// sizes larger than a tile; clipping here keeps those buffers from painting
/// over sibling tiles or Luau-owned shell controls.
pub(crate) fn window_render_elements(
    renderer: &mut GlesRenderer,
    state: &SosCompositor,
    output: &Output,
) -> Result<Vec<SosWindowRenderElement>> {
    let output_geometry = state
        .space
        .output_geometry(output)
        .context("output is not mapped into compositor space")?;
    let output_scale = output.current_scale().fractional_scale();
    let scale = Scale::from(output_scale);
    let application_rectangles = state.application_window_rectangles();
    let mut rendered = Vec::new();

    for window in state.space.elements().rev() {
        let Some(mapped_location) = state.space.element_location(window) else {
            continue;
        };
        let render_location = mapped_location - window.geometry().loc - output_geometry.loc;
        let elements = window.render_elements::<WaylandSurfaceRenderElement<GlesRenderer>>(
            renderer,
            render_location.to_physical_precise_round(output_scale),
            scale,
            1.0,
        );
        if state.is_application_window(window) {
            let Some(rectangle) = application_rectangles
                .iter()
                .find_map(|(candidate, rectangle)| (candidate == window).then_some(*rectangle))
            else {
                continue;
            };
            let clip: Rectangle<i32, Logical> = Rectangle::new(
                (
                    rectangle.x - output_geometry.loc.x,
                    rectangle.y - output_geometry.loc.y,
                )
                    .into(),
                (rectangle.width, rectangle.height).into(),
            );
            let clip = clip.to_physical_precise_round(output_scale);
            rendered.extend(elements.into_iter().filter_map(|element| {
                CropRenderElement::from_element(element, scale, clip)
                    .map(SosWindowRenderElement::Clipped)
            }));
        } else {
            rendered.extend(elements.into_iter().map(SosWindowRenderElement::Surface));
        }
    }
    Ok(rendered)
}
