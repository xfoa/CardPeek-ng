use std::error::Error;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, PropMode};
use x11rb::wrapper::ConnectionExt as _;

pub(crate) fn set_window_icon() {
    if let Err(error) = set_icon() {
        eprintln!("cardpeek-desktop: cannot set the X11 window icon: {error}");
    }
}

fn set_icon() -> Result<(), Box<dyn Error>> {
    let image = image::load_from_memory(include_bytes!("../../../resources/icon.png"))?.to_rgba8();
    let (width, height) = image.dimensions();
    let mut data = Vec::with_capacity(2 + image.len() / 4);
    data.push(width);
    data.push(height);
    for pixel in image.pixels() {
        let [red, green, blue, alpha] = pixel.0;
        data.push(
            u32::from(alpha) << 24 | u32::from(red) << 16 | u32::from(green) << 8 | u32::from(blue),
        );
    }

    let (connection, _) = x11rb::connect(None)?;
    let mut windows = vec![connection.setup().roots[0].root];
    while let Some(window) = windows.pop() {
        let class = connection
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)?
            .reply()?
            .value;
        if class
            .split(|byte| *byte == 0)
            .any(|part| part == b"cardpeek-ng")
        {
            let atom = connection
                .intern_atom(false, b"_NET_WM_ICON")?
                .reply()?
                .atom;
            connection.change_property32(
                PropMode::REPLACE,
                window,
                atom,
                AtomEnum::CARDINAL,
                &data,
            )?;
            connection.flush()?;
            return Ok(());
        }
        let children = connection.query_tree(window)?.reply()?.children;
        windows.extend(children);
    }
    Ok(())
}
