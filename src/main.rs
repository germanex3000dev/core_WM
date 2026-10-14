use std::collections::HashMap;

use x11rb::rust_connection::RustConnection;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    ChangeWindowAttributesAux, 
    ConfigWindow, 
    ConfigureWindowAux, 
    ConnectionExt, 
    EventMask, 
    Window,
    GrabMode,
    ModMask,
    InputFocus,
    StackMode,
};


// struct that has window info
struct ManagedWindow {
    id: Window,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    floating: bool,
}


// This function arranges the windows and is the "main" tiling logic
fn arrange_windows(
    conn: &RustConnection,
    windows: &HashMap<u32, ManagedWindow>,
    window_order: &Vec<u32>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(), Box<dyn std::error::Error>> {

    if windows.len() == 0 {
        return Ok(())
    }
    
    let window_width = screen_width / windows.len() as u32;

    for (index, window_id) in window_order.iter().enumerate() {

        if let Some(window) = windows.get(window_id) {
            let x = index as u32 * window_width;

            conn.configure_window(
                window.id,
                &ConfigureWindowAux::new()
                    .x(x as i32)
                    .y(0)
                    .width(window_width)
                    .height(screen_height),
            )?;
        }
    }

    Ok(())
}

fn keycode_for_keysym(
    conn: &RustConnection,
    target: xkeysym::Keysym,
) -> Result<Option<u8>, Box<dyn std::error::Error>> {
    let setup = conn.setup();

    let mapping = conn
        .get_keyboard_mapping(
            setup.min_keycode,
            setup.max_keycode - setup.min_keycode + 1,
        )?
        .reply()?;

    for keycode in setup.min_keycode..=setup.max_keycode {
        for column in 0..mapping.keysyms_per_keycode {
            let symbol = xkeysym::keysym(
                keycode.into(),
                column,
                setup.min_keycode.into(),
                mapping.keysyms_per_keycode,
                mapping. keysyms.as_slice(),
            );

            if symbol == Some(target) {
                return Ok(Some(keycode));
            }
        }
    }

    Ok(None)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to X11 i think
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let screen_width = u32::from(screen.width_in_pixels);
    let screen_height = u32::from(screen.height_in_pixels);

    let root = screen.root;

    println!("core_WM: connecting to X11 ... ");

    // Ask X11 to send window-managemant somethings
    let event_mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;

    conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(event_mask),
    )?
    .check()?;

    let mut windows: HashMap<u32, ManagedWindow> = HashMap::new();
    let mut window_order: Vec<u32> = Vec::new();

    let mut focused_index: usize = 0;

    println!("core_WM: running(hopefully. maybe.)");

    let j_keysym = xkeysym::Keysym::from_char('j');
    let k_keysym = xkeysym::Keysym::from_char('k');

    let super_j = keycode_for_keysym(&conn, j_keysym)?
        .ok_or("Could not find the j key in the keyboard mapping")?;

    let super_k = keycode_for_keysym(&conn, k_keysym)?
        .ok_or("Could not fing the k key in the keyboard mapping")?;

    conn.grab_key(
        false,
        root,
        ModMask::M1,
        super_j,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?
    .check()?;

    // Process X11 events for all eternity(not really)
    loop {
        let event = conn.wait_for_event()?;

        match event {
            
            Event::MapRequest(event) => {
                println!("New window {}", event.window);

                let window = event.window;

                // This "thing" retrieves position and size temporarily.
                // Although it feels kind of pointless, it works this way
                // and I'm too scared to touch it.
                let geometry = conn
                    .get_geometry(window)?
                    .reply()?;
                 

                /* Old window configuration
                conn.configure_window(
                    window,
                    &ConfigureWindowAux::new()
                        .x(100)
                        .y(100)
                        .width(800)
                        .height(600),
                )?;
                */ 


                // create the struct and give it the window values on a new window
                let managed_window = ManagedWindow {
                    id: window,
                    x: geometry.x,
                    y: geometry.y,
                    width: geometry.width,
                    height: geometry.height,
                    floating: false
                };

                windows.insert(window, managed_window);
                window_order.push(window);

                arrange_windows(
                    &conn,
                    &windows,
                    &window_order,
                    screen_width,
                    screen_height,
                )?;

                println!(
                    "Window {}: {}x{} at ({}, {})",
                    window,
                    geometry.width,
                    geometry.height,
                    geometry.x,
                    geometry.y
                );

                //make window visible
                conn.map_window(event.window)?;

            }

            Event::ConfigureRequest(event) => {
                // Forward the geometry fields requested
                // by the application. Or idk, I'm just following
                // a tutorial, too.
                let mut changes = ConfigureWindowAux::new();

                if event.value_mask.contains(ConfigWindow::X) {
                    changes = changes.x(i32::from(event.x));


                    // These if-statements update the data in the struct(managed_window)
                    if let Some(managed_window) = windows.get_mut(&event.window) {
                        managed_window.x = event.x;
                    }
                }
                if event.value_mask.contains(ConfigWindow::Y) {
                    changes = changes.y(i32::from(event.y));

                    if let Some(managed_window) = windows.get_mut(&event.window) {
                        managed_window.y = event.y;
                    }
                }
                if event.value_mask.contains(ConfigWindow::WIDTH) {
                    changes = changes.width(u32::from(event.width));

                    if let Some(managed_window) = windows.get_mut(&event.window) {
                        managed_window.width = event.width;
                    }
                }
                if event.value_mask.contains(ConfigWindow::HEIGHT) {
                    changes = changes.height(u32::from(event.height));

                    if let Some(managed_window) = windows.get_mut(&event.window) {
                        managed_window.height = event.height;
                    }
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
                windows.remove(&event.window);
                window_order.retain(|&id| id != event.window);

                println!("Window closed: {}", event.window);

                arrange_windows(
                    &conn,
                    &windows,
                    &window_order,
                    screen_width,
                    screen_height,
                )?;

                if window_order.is_empty() {
                    focused_index = 0;
                } else if focused_index >= window_order.len() {
                    focused_index = window_order.len() -1; 
                }

            }

            Event::KeyPress(event) => {
                if event.detail  == super_j && !window_order.is_empty() {
                    focused_index = (focused_index + 1) % window_order.len();

                    let window_id = window_order[focused_index];

                    conn.configure_window(
                        window_id,
                        &ConfigureWindowAux::new()
                            .stack_mode(StackMode::ABOVE),
                    )?;

                    conn.flush()?;
                }

                if event.detail == super_k && !window_order.is_empty() {
                    focused_index = (focused_index + window_order.len() - 1) % window_order.len();

                    let window_id = window_order[focused_index];

                    conn.configure_window(
                        window_id,
                        &ConfigureWindowAux::new()
                            .stack_mode(StackMode::ABOVE),
                    )?;

                    conn.flush()?;
                }
            }

            _ => {}
        }

        // Apparently, this sends queued requests to the X server.
        conn.flush()?;
    }
}
