// The nested backend setup follows Smithay's MIT-licensed `smallvil` and
// `anvil` examples at tag v0.7.0. SOS additionally emits its own fence only
// after the armed shell commit participates in a successful backend submit.

use std::time::Duration;

use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker,
            element::{
                surface::{render_elements_from_surface_tree, WaylandSurfaceRenderElement},
                Kind,
            },
            gles::GlesRenderer,
        },
        winit::{self, WinitEvent},
    },
    desktop::{space::render_output, utils::OutputPresentationFeedback, Space, Window},
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::EventLoop,
        wayland_protocols::wp::presentation_time::server::wp_presentation_feedback,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{Rectangle, Transform},
    wayland::presentation::Refresh,
};

use crate::{mark_backend_ready, state::SosCompositor, CompositorData};

pub fn init_winit(
    event_loop: &mut EventLoop<CompositorData>,
    data: &mut CompositorData,
) -> anyhow::Result<()> {
    let (mut backend, winit) = winit::init()
        .map_err(|error| anyhow::anyhow!("initialize nested winit backend: {error}"))?;
    let mode = Mode {
        size: backend.window_size(),
        refresh: 60_000,
    };
    data.state.output_size = mode.size.into();
    let output = Output::new(
        "sos-nested".into(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "SOS".into(),
            model: "Nested development compositor".into(),
        },
    );
    let _global = output.create_global::<SosCompositor>(&data.display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    data.state.space.map_output(&output, (0, 0));
    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    event_loop
        .handle()
        .insert_source(winit, move |event, _, data| match event {
            WinitEvent::Resized { size, .. } => {
                let state = &mut data.state;
                state.output_size = size.into();
                output.change_current_state(
                    Some(Mode {
                        size,
                        refresh: 60_000,
                    }),
                    None,
                    None,
                    None,
                );
            }
            WinitEvent::Input(event) => {
                data.state.process_input_event(event);
                if data.state.take_session_exit_request() {
                    data.loop_signal.stop();
                }
            }
            WinitEvent::Redraw => {
                let state = &mut data.state;
                let redraw = (|| -> Result<_, String> {
                    let size = backend.window_size();
                    let result = {
                        let (renderer, mut framebuffer) = backend
                            .bind()
                            .map_err(|error| format!("bind nested framebuffer: {error}"))?;
                        let input_method_elements = state
                            .input_method_popups
                            .iter()
                            .filter(|popup| popup.alive())
                            .flat_map(|popup| {
                                let parent_location = popup
                                    .get_parent()
                                    .map(|parent| parent.location.loc)
                                    .unwrap_or_default();
                                render_elements_from_surface_tree(
                                    renderer,
                                    popup.wl_surface(),
                                    (parent_location + popup.location())
                                        .to_physical_precise_round(1.0),
                                    1.0,
                                    1.0,
                                    Kind::Unspecified,
                                )
                            })
                            .collect::<Vec<WaylandSurfaceRenderElement<GlesRenderer>>>();
                        render_output::<_, WaylandSurfaceRenderElement<GlesRenderer>, _, _>(
                            &output,
                            renderer,
                            &mut framebuffer,
                            1.0,
                            0,
                            [&state.space],
                            &input_method_elements,
                            &mut damage_tracker,
                            [0.025, 0.03, 0.035, 1.0],
                        )
                        .map_err(|error| format!("render nested output: {error}"))?
                    };
                    let damage = result
                        .damage
                        .cloned()
                        .unwrap_or_else(|| vec![Rectangle::from_size(size)]);
                    backend
                        .submit(Some(&damage))
                        .map_err(|error| format!("submit nested output: {error}"))?;
                    Ok(result.states)
                })();
                let states = match redraw {
                    Ok(states) => states,
                    Err(error) => {
                        tracing::error!(%error, "nested compositor redraw failed");
                        data.loop_signal.stop();
                        return;
                    }
                };

                let shell_rendered = state.shell_rendered(&states);
                state.publish_successful_submit(shell_rendered);
                present_client_feedback(state, &state.space, &output, &states);
                state.space.elements().for_each(|window| {
                    window.send_frame(&output, state.clock.now(), Some(Duration::ZERO), |_, _| {
                        Some(output.clone())
                    });
                });
                state.space.refresh();
                state.popups.cleanup();
                mark_backend_ready(data, "nested_backend_submit");
                let _ = data.display_handle.flush_clients();
                backend.window().request_redraw();
            }
            WinitEvent::CloseRequested => data.loop_signal.stop(),
            _ => {}
        })
        .map_err(|_| anyhow::anyhow!("insert nested winit backend source"))?;
    Ok(())
}

fn present_client_feedback(
    state: &SosCompositor,
    space: &Space<Window>,
    output: &Output,
    states: &smithay::backend::renderer::element::RenderElementStates,
) {
    let mut feedback = OutputPresentationFeedback::new(output);
    space.elements().for_each(|window| {
        if space.outputs_for_element(window).contains(output) {
            window.take_presentation_feedback(
                &mut feedback,
                |_: &WlSurface, _| Some(output.clone()),
                |surface, _| {
                    smithay::desktop::utils::surface_presentation_feedback_flags_from_states(
                        surface, states,
                    )
                },
            );
        }
    });
    feedback.presented(
        state.clock.now(),
        Refresh::fixed(Duration::from_nanos(16_666_667)),
        0,
        wp_presentation_feedback::Kind::Vsync,
    );
}
