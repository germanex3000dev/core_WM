use x11rb::protocol::xproto::Window;

/// Layout modes for core_wm.
/// Change `LAYOUT_MODE` in main.rs to switch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LayoutMode {
    MasterStack = 0,
    Dwindle = 1,
}

/// Master Stack layout:
/// - One "master" window takes master_frac of screen (default 80%)
/// - Remaining windows stack vertically in the sidebar
/// - When a stack window is focused, it becomes the new master
///
/// Set `LAYOUT_MODE = LayoutMode::MasterStack` in main.rs to use.
pub fn master_stack_arrange(
    window_order: &[Window],
    screen_width: u32,
    screen_height: u32,
    master_frac: f64,
) -> Vec<(Window, i32, i32, u32, u32)> {
    let num = window_order.len();
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        let w = window_order[0];
        return vec![(w, 0, 0, screen_width, screen_height)];
    }

    let master_w = (screen_width as f64 * master_frac) as u32;
    let stack_w = screen_width - master_w;
    let stack_count = num - 1;
    let stack_h = screen_height / stack_count as u32;

    let mut result = Vec::new();

    // Master window on the left, full height
    result.push((window_order[0], 0, 0, master_w, screen_height));

    // Remaining windows stack vertically on the right
    let mut y: i32 = 0;
    for i in 1..window_order.len() {
        if i - 1 >= stack_count { break; }
        result.push((window_order[i], master_w as i32, y, stack_w, stack_h));
        y += stack_h as i32;
    }

    result
}

/// Dwindle layout:
/// - Starts with first window full screen
/// - Simple grid arrangement for up to 4 windows
/// - For more windows, falls back to master stack arrangement
///
/// Set `LAYOUT_MODE = LayoutMode::Dwindle` in main.rs to use.
pub fn dwindle_arrange(
    window_order: &[Window],
    screen_width: u32,
    screen_height: u32,
) -> Vec<(Window, i32, i32, u32, u32)> {
    let num = window_order.len();
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        let w = window_order[0];
        return vec![(w, 0, 0, screen_width, screen_height)];
    }

    // Grid arrangement for small number of windows (up to 4)
    let cols = ((num as f64).sqrt()).ceil() as usize;
    let rows = ((num as f64) / cols as f64).ceil() as usize;
    let cell_w = screen_width / cols.max(1) as u32;
    let cell_h = screen_height / rows.max(1) as u32;

    let mut result = Vec::new();
    for i in 0..num {
        let row = i / cols;
        let col = i % cols;
        let x = (col as u32) * cell_w;
        let y = (row as u32) * cell_h;
        if i < window_order.len() {
            result.push((window_order[i], x as i32, y as i32, cell_w, cell_h));
        }
    }
    result
}

/// Arrange windows according to the selected layout mode.
/// Called on MapRequest and DestroyNotify.
///
/// The active layout is determined by `LAYOUT_MODE` constant in main.rs.
pub fn arrange_windows(
    window_order: &[Window],
    screen_width: u32,
    screen_height: u32,
    mode: u32,
) -> Vec<(Window, i32, i32, u32, u32)> {
    match mode {
        0 => master_stack_arrange(window_order, screen_width, screen_height, 0.8),
        1 => dwindle_arrange(window_order, screen_width, screen_height),
        _ => master_stack_arrange(window_order, screen_width, screen_height, 0.8),
    }
}