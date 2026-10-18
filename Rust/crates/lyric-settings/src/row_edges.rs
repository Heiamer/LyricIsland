//! Scope the outer corners of a grouped panel without rounding each internal row.
use gpui::*;
use std::cell::Cell;
thread_local! { static EDGES: Cell<(bool, bool)> = const { Cell::new((false, false)) }; }
pub fn current() -> (bool, bool) { EDGES.with(Cell::get) }
struct Restore((bool, bool));
impl Drop for Restore { fn drop(&mut self) { EDGES.with(|edges| edges.set(self.0)); } }
fn scoped<T>(edges: (bool, bool), f: impl FnOnce() -> T) -> T {
    let _restore = Restore(EDGES.with(|cell| cell.replace(edges)));
    f()
}
pub fn wrap(child: AnyElement, top: bool, bottom: bool) -> AnyElement {
    RowEdges { child, edges: (top, bottom) }.into_any_element()
}
struct RowEdges { child: AnyElement, edges: (bool, bool) }
impl IntoElement for RowEdges {
    type Element = Self;
    fn into_element(self) -> Self { self }
}
impl Element for RowEdges {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> { None }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> { None }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (scoped(self.edges, || self.child.request_layout(window, cx)), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        self.child.prepaint(window, cx);
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        self.child.paint(window, cx);
    }
}
pub fn regression() -> bool {
    let original = current();
    let valid = [(true, false), (false, false), (false, true), (true, true)].into_iter().all(|edges| {
        scoped(edges, || {
            let outer = current() == edges;
            let inner = scoped((false, false), || current() == (false, false));
            outer && inner && current() == edges
        }) && current() == original
    });
    valid && original == (false, false)
}
