use nsh_core::desktop::ResizeEdge;
use nsh_core::prelude::*;

const RESIZE_HANDLE_SIZE: f32 = 6.0;

/// Invisible strips along the edges and corners that resize the window.
pub(super) fn resize_edges() -> [View; 8] {
    use ResizeEdge::*;
    [
        Top,
        Bottom,
        Left,
        Right,
        TopLeft,
        TopRight,
        BottomLeft,
        BottomRight,
    ]
    .map(edge)
}

fn edge(edge: ResizeEdge) -> View {
    let handle = px(RESIZE_HANDLE_SIZE);
    let zero = px(0.0);
    let v = view().absolute().window_resize(edge);
    match edge {
        ResizeEdge::Top => v.top(zero).left(zero).right(zero).h(handle),
        ResizeEdge::Bottom => v.bottom(zero).left(zero).right(zero).h(handle),
        ResizeEdge::Left => v.left(zero).top(zero).bottom(zero).w(handle),
        ResizeEdge::Right => v.right(zero).top(zero).bottom(zero).w(handle),
        ResizeEdge::TopLeft => v.top(zero).left(zero).w(handle).h(handle),
        ResizeEdge::TopRight => v.top(zero).right(zero).w(handle).h(handle),
        ResizeEdge::BottomLeft => v.bottom(zero).left(zero).w(handle).h(handle),
        ResizeEdge::BottomRight => v.bottom(zero).right(zero).w(handle).h(handle),
    }
}
