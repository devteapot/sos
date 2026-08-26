use experience_ir::WindowSpaceContent;
use gpui::{canvas, div, prelude::*, AnyElement, Bounds, Context, Pixels};

pub(crate) trait WindowSpaceHost: Sized + 'static {
    fn record_window_space(
        &mut self,
        node_id: String,
        bounds: Bounds<Pixels>,
        specification: WindowSpaceContent,
        cx: &mut Context<Self>,
    );
}

pub(crate) fn render<H: WindowSpaceHost>(
    node_id: String,
    specification: WindowSpaceContent,
    host: gpui::WeakEntity<H>,
) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .child(
            canvas(
                move |bounds, _, app| {
                    let _ = host.update(app, |host, cx| {
                        host.record_window_space(node_id.clone(), bounds, specification.clone(), cx)
                    });
                },
                |_, _, _, _| {},
            )
            .size_full(),
        )
        .into_any_element()
}
