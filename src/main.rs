use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, ConfigWindow, ConfigureWindowAux, ConnectionExt, EventMask,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to X11 i think
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    println!("core_WM: connecting to X11 ... ");

    // Ask X11 to send window-managemant somethings
    let event_mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;

    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(event_mask),
    )?
    .check()?;

    println!("core_WM: running(hopefully. maybe.)");

    // Process X11 events for all eternity(not really)
    loop {
        let event = conn.wait_for_event()?;

        match event {
            Event::MapRequest(event) => {
                println!("New window {}", event.window);

                // Make window visible
                conn.map_window(event.window)?;
            }

            Event::ConfigureRequest(event) => {
                // Forward the geometry fields requested
                // by the application. Or idk, I'm just following
                // a tutorial, too.
                let mut changes = ConfigureWindowAux::new();

                if event.value_mask.contains(ConfigWindow::X) {
                    changes = changes.x(i32::from(event.x));
                }
                if event.value_mask.contains(ConfigWindow::Y) {
                    changes = changes.y(i32::from(event.y));
                }
                if event.value_mask.contains(ConfigWindow::WIDTH) {
                    changes = changes.width(u32::from(event.width));
                }
                if event.value_mask.contains(ConfigWindow::HEIGHT) {
                    changes = changes.height(u32::from(event.height));
                }
                if event.value_mask.contains(ConfigWindow::BORDER_WIDTH) {
                    changes = changes.border_width(u32::from(event.border_width));
                }
                if event.value_mask.contains(ConfigWindow::SIBLING) {
                    changes = changes.sibling(event.sibling);
                }
                if event.value_mask.contains(ConfigWindow::STACK_MODE) {
                    changes = changes.stack_mode(event.stack_mode);
                }

                conn.configure_window(event.window, &changes)?;
            }

            Event::DestroyNotify(event) => {
                println!("Window closed: {}", event.window);
            }

            _ => {}
        }

        // Apparently, this sends queued requests to the X server.
        conn.flush()?;
    }
}
