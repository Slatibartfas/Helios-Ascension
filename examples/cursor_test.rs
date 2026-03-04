use bevy::prelude::*;

fn main() {
    let mut w = Window::default();
    let c = CursorIcon::Custom(CustomCursor::Image {
        handle: Handle::default(),
        hotspot: (0, 0),
    });
    w.cursor.icon = c;
}
