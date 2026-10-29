use x11rb::protocol::xproto::Window;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn area(self) -> u32 {
        u32::from(self.width) * u32::from(self.height)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Axis {
    Vertical,
    Horizontal,
}

pub enum LayoutNode {
    Leaf {
        window: Window,
        created_at: u64,
    },
    Split {
        axis: Axis,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

fn better_leaf(
    a: (Window, i32, i32, u32, u32, u64),
    b: (Window, i32, i32, u32, u32, u64),
) -> (Window, i32, i32, u32, u32, u64) {
    // Use width (index 3) and height (index 4) for area calculation
    let area_a = a.3 * a.4;
    let area_b = b.3 * b.4;

    if area_a > area_b {
        a
    } else if area_b > area_a {
        b
    } else {
        // Tiebreak: most recently created (highest created_at wins)
        if a.5 > b.5 {
            a
        } else {
            b
        }
    }
}

pub fn find_largest_leaf(
    node: &LayoutNode,
    rect: Rect,
) -> Option<(Window, i32, i32, u32, u32, u64)> {
    let leaves = get_all_leaves_with_rects(node, rect);
    if leaves.is_empty() {
        return None;
    }

    let mut best = leaves[0];
    for leaf in leaves.iter().skip(1) {
        // Use width (index 3) and height (index 4) for area calculation
        let area_best = best.3 * best.4;
        let area_leaf = leaf.3 * leaf.4;

        if area_leaf > area_best || (area_leaf == area_best && leaf.5 > best.5) {
            best = *leaf;
        }
    }

    Some(best)
}

pub fn get_all_leaves_with_rects(
    node: &LayoutNode,
    rect: Rect,
) -> Vec<(Window, i32, i32, u32, u32, u64)> {
    match node {
        LayoutNode::Leaf {
            window,
            created_at,
        } => vec![(
            *window,
            i32::from(rect.x),
            i32::from(rect.y),
            u32::from(rect.width),
            u32::from(rect.height),
            *created_at,
        )],

        LayoutNode::Split {
            axis,
            first,
            second,
        } => {
            let mut results = Vec::new();
            match axis {
                Axis::Vertical => {
                    let split_width = rect.width / 2;
                    results.extend_from_slice(&get_all_leaves_with_rects(
                        first,
                        Rect {
                            x: rect.x,
                            y: rect.y,
                            width: split_width as u16,
                            height: u16::from(rect.height),
                        },
                    ));
                    results.extend_from_slice(&get_all_leaves_with_rects(
                        second,
                        Rect {
                            x: i16::from(rect.x) + split_width as i16,
                            y: rect.y,
                            width: (rect.width - split_width) as u16,
                            height: u16::from(rect.height),
                        },
                    ));
                }
                Axis::Horizontal => {
                    let split_height = rect.height / 2;
                    results.extend_from_slice(&get_all_leaves_with_rects(
                        first,
                        Rect {
                            x: rect.x,
                            y: rect.y,
                            width: u16::from(rect.width),
                            height: split_height as u16,
                        },
                    ));
                    results.extend_from_slice(&get_all_leaves_with_rects(
                        second,
                        Rect {
                            x: rect.x,
                            y: i16::from(rect.y) + split_height as i16,
                            width: u16::from(rect.width),
                            height: (rect.height - split_height) as u16,
                        },
                    ));
                }
            }
            results
        }
    }
}

fn find_and_replace_leaf(
    node: &mut LayoutNode,
    target_created_at: u64,
    axis: Axis,
    new_window: Window,
    created_at_counter: &mut u64,
) -> Option<Window> {
    match node {
        LayoutNode::Leaf {
            window,
            created_at,
        } => {
            if *created_at == target_created_at {
                let existing = *window;
                *node = LayoutNode::Split {
                    axis,
                    first: Box::new(LayoutNode::Leaf {
                        window: existing,
                        created_at: *created_at_counter,
                    }),
                    second: Box::new(LayoutNode::Leaf {
                        window: new_window,
                        created_at: *created_at_counter + 1,
                    }),
                };
                *created_at_counter += 2;
                Some(existing)
            } else {
                None
            }
        }
        LayoutNode::Split { axis: _, first, second } => {
            // Try first child
            if let Some(result) = find_and_replace_leaf(first, target_created_at, axis, new_window, created_at_counter) {
                return Some(result);
            }
            // Try second child
            find_and_replace_leaf(second, target_created_at, axis, new_window, created_at_counter)
        }
    }
}

pub fn replace_leaf_with_split(
    node: &mut LayoutNode,
    target_created_at: u64,
    axis: Axis,
    new_window: Window,
    created_at_counter: &mut u64,
) -> Window {
    match find_and_replace_leaf(node, target_created_at, axis, new_window, created_at_counter) {
        Some(existing) => existing,
        None => panic!("Leaf with created_at {} not found", target_created_at),
    }
}

pub fn rebuild_layout_tree(
    window_order: &[u32],
    screen_width: u32,
    screen_height: u32,
    created_at_counter: &mut u64,
) -> LayoutNode {
    if window_order.is_empty() {
        // Return a dummy leaf; caller should handle empty case
        LayoutNode::Leaf {
            window: Window::default(),
            created_at: 0,
        }
    } else if window_order.len() == 1 {
        *created_at_counter += 1;
        LayoutNode::Leaf {
            window: window_order[0],
            created_at: *created_at_counter,
        }
    } else {
        // First window: entire screen
        *created_at_counter += 1;
        let root_ca = *created_at_counter;
        let mut root = LayoutNode::Leaf {
            window: window_order[0],
            created_at: root_ca,
        };

        let mut current_axis = Axis::Vertical; // First split (for 2nd window) is vertical

        // Process remaining windows
        for &window_id in window_order.iter().skip(1) {
            // Find the largest leaf in the current tree
            let screen_rect = Rect {
                x: 0,
                y: 0,
                width: screen_width as u16,
                height: screen_height as u16,
            };

            let largest = find_largest_leaf(&root, screen_rect);

            // Replace the largest leaf with a Split node
            // Use "long side" splitting: determine axis based on region dimensions
            if let Some((_largest_window, _largest_x, _largest_y, _largest_w, _largest_h, largest_ca)) = largest {
                // Split the chosen region in half, using the alternating axis
                let split_axis = current_axis;

                let _existing_window = replace_leaf_with_split(
                    &mut root,
                    largest_ca,
                    split_axis,
                    window_id,
                    created_at_counter,
                );
                // existing_window is the window that was in the region we split;
                // window_id is the new window that took the other half
            }

            // Alternate the split axis for the next window
            current_axis = match current_axis {
                Axis::Vertical => Axis::Horizontal,
                Axis::Horizontal => Axis::Vertical,
            };
        }

        root
    }
}