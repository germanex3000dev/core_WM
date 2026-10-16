# core_WM — Developer Documentation

How the project actually works, end to end. Read this if you want to know *why* the code
looks the way it does, or if you're about to add something and need to know which of the
four variables you're allowed to touch. Or read it for fun, I guess, if you don't have a life 
like me. Literally. Why am I even building this? I got that asked many times, but I don't 
know myself.

This documents core_WM at **version 0.1.5**, which lives entirely in `src/main.rs`
(~437 lines). Everything below is implemented code, not wishful thinking — where
something is planned but missing, it gets called out as missing.

For project status, keybindings and build instructions, see [`README.md`](../README.md).

---

## Table of contents

1. [What a window manager is, and why X11 needs one](#1-what-a-window-manager-is-and-why-x11-needs-one)
2. [Program shape](#2-program-shape)
3. [Startup sequence](#3-startup-sequence)
4. [The state model](#4-the-state-model)
5. [The event loop](#5-the-event-loop)
6. [Event handlers](#6-event-handlers)
7. [The tiling engine](#7-the-tiling-engine)
8. [The focus model](#8-the-focus-model)
9. [Input: grabs and keysym resolution](#9-input-grabs-and-keysym-resolution)
10. [Closing windows: the ICCCM handshake](#10-closing-windows-the-icccm-handshake)
11. [Spawning external programs](#11-spawning-external-programs)
12. [Error handling](#12-error-handling)
13. [Verified behaviour](#13-verified-behaviour)
14. [Known gaps and rough edges](#14-known-gaps-and-rough-edges)
15. [How to extend core_WM](#15-how-to-extend-core_wm)
16. [Glossary](#16-glossary)

---

## 1. What a window manager is, and why X11 needs one

X11 has no built-in window manager. The server draws rectangles and routes events — it
does not decide where windows go, and it does not decide which one gets the keyboard.
Something has to do both, and that something is the WM.

Run no WM at all and you get the classic experience: every window opens wherever its
application asked, which is usually a tidy stack in the middle of the screen, and clicking
one doesn't reliably hand it the keyboard. You end up with `xdotool` and a lot of
resentment.

core_WM claims the job by **taking over the root window**. On startup it selects
`SubstructureRedirect` on the root:

```rust
let event_mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;

conn.change_window_attributes(
    root,
    &ChangeWindowAttributesAux::new().event_mask(event_mask),
)?
.check()?;
```
<sub>`src/main.rs:166-172`</sub>

`SubstructureRedirect` is the one that matters. When it's selected on the root, the X
server stops routing `MapRequest` and `ConfigureRequest` to whichever application created
a window, and sends them to the WM instead. The application then sits there waiting,
doing nothing, until the WM calls `map_window`. **This is the redirect steal, and it's
the entire basis of WM authority.** The same setting is why starting core_WM under a
desktop session logs you straight out of it — you're fighting it for the same chair.

`SubstructureNotify` comes along for the ride so the WM is also told when windows are
mapped and unmapped by others, which is how `DestroyNotify` ends up on our doorstep.

---

## 2. Program shape

One file, five functions, and `main`. No modules yet — that refactor is on the roadmap
and it's the first thing I'd do with a free afternoon.

| Function | Lines | Responsibility |
| --- | --- | --- |
| `arrange_windows()` | 33-62 | The layout engine. Decides where every window sits. |
| `focus_window()` | 69-77 | Hands X11 input focus to a window. |
| `keycode_for_keysym()` | 79-106 | Turns a keysym into a keycode via the keyboard mapping. |
| `launch_terminal()` | 108-120 | Spawns an external terminal. |
| `close_window()` | 122-152 | Asks a client to close, the polite ICCCM way. |
| `main()` | 154-437 | Startup, state, and the event loop. |

The design is deliberately flat: **all state lives in `main()` as three variables**, and
every event handler reads and mutates them directly. There's no `WindowManager` struct
owning a window list, no trait to abstract over layouts, no event bus. That's the "small
and minimalistic" goal taken literally rather than aspirationally.

The honest trade-off: state management is currently smeared across the event handlers
instead of funnelled through one owner. It works fine at this size and it will *not* work
fine at ten times this size.

---

## 3. Startup sequence

`main()` runs these once, in order, before entering the loop. Nothing here happens again
afterwards.

### 3.1 Connect and read the screen

```rust
let (conn, screen_num) = x11rb::connect(None)?;
let screen = &conn.setup().roots[screen_num];
let screen_width = u32::from(screen.width_in_pixels);
let screen_height = u32::from(screen.height_in_pixels);
```
<sub>`src/main.rs:156-159`</sub>

`x11rb::connect(None)` with `None` reads `DISPLAY` from the environment and opens a
connection. The setup reply lists every screen the server offers; `screen_num` picks one,
and its pixel dimensions become the numbers the layout engine divides up later.

These are read **once, at startup, and never refreshed**. core_WM doesn't handle
`ConfigureNotify` on the root, so change your resolution mid-session and the WM keeps
tiling to the old size like nothing happened. That's a real bug, not a design choice —
see [section 14](#14-known-gaps-and-rough-edges).

### 3.2 Take the root window

The `change_window_attributes` call from
[section 1](#1-what-a-window-manager-is-and-why-x11-needs-one). If something else
already holds `SubstructureRedirect` — another WM, in other words — this returns
`BadAccess` and core_WM exits with an error, which is the correct and only sane outcome.

### 3.3 Create the state containers

```rust
let mut windows: HashMap<u32, ManagedWindow> = HashMap::new();
let mut window_order: Vec<u32> = Vec::new();
let mut focused_index: usize = 0;
```
<sub>`src/main.rs:174-177`</sub>

Three variables, and yes, the middle one exists for a reason. See
[section 4](#4-the-state-model).

### 3.4 Intern the ICCCM atoms

```rust
let wm_protocols = conn
    .intern_atom(false, b"WM_PROTOCOLS")?
    .reply()?
    .atom;

let wm_delete_window = conn
    .intern_atom(false, b"DELETE_WINDOW")?
    .reply()?
    .atom;
```
<sub>`src/main.rs:181-189`</sub>

Atoms are server-wide interned integer IDs standing in for names. `WM_PROTOCOLS` is the
standard "here's the list of things I understand" channel a client advertises on its own
window; `DELETE_WINDOW` is the message name inside that list. Doing it once here means
the close path never round-trips to the server on a keypress. Full story in
[section 10](#10-closing-windows-the-icccm-handshake).

### 3.5 Resolve the keybindings

```rust
let j_keysym = xkeysym::Keysym::from_char('j');
let k_keysym = xkeysym::Keysym::from_char('k');
let t_keysym = xkeysym::Keysym::from_char('t');
let q_keysym = xkeysym::Keysym::from_char('q');

let super_j = keycode_for_keysym(&conn, j_keysym)?
    .ok_or("Could not find the j key in the keyboard mapping")?;
```
<sub>`src/main.rs:191-206`</sub>

Characters into hardware keycodes. Each `.ok_or(...)?` makes the WM refuse to start rather
than run with a binding silently missing — a WM that boots and ignores half its
keybindings is a miserable thing to debug, so we made it a loud startup failure instead.

### 3.6 Register the passive grabs

Four `grab_key` calls, one per binding, all with `ModMask::M1`:

```rust
conn.grab_key(
    false,          // owner_events
    root,           // grab window
    ModMask::M1,    // modifier mask
    super_j,        // keycode
    GrabMode::ASYNC,
    GrabMode::ASYNC,
)?
.check()?;
```
<sub>`src/main.rs:211-249`, repeated four times</sub>

Yes, four near-identical blocks. No wildcards in X11. Without these the bindings would
never reach the WM at all — see [section 9](#9-input-grabs-and-keysym-resolution).

---

## 4. The state model

Everything core_WM knows lives in three variables. Get these three and you understand
the WM. Get these three wrong and you'll spend an afternoon wondering why focus jumps to
the wrong window.

### `ManagedWindow`

```rust
struct ManagedWindow {
    id: Window,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    floating: bool,
}
```
<sub>`src/main.rs:23-30`</sub>

One per client. `id` is the X11 handle; the geometry fields mirror what the client last
asked for; `floating` is a placeholder that **nothing currently reads** — the compiler
emits a `dead_code` warning for exactly this reason, and that warning is accurate.

The narrow types (`i16` / `u16`) weren't chosen, they just came along with the
`GetGeometry` reply type. On a very wide multi-monitor layout they will overflow.

### `windows: HashMap<u32, ManagedWindow>`

The collection, keyed by window id. O(1) lookup whenever an event names a window.

### `window_order: Vec<u32>`

**Why keep a second container?** Because a `HashMap` has no defined iteration order.
Iterating it would drop windows into arbitrary columns that could shuffle between runs —
and yes, that was the behaviour before this vector existed.

`window_order` records the sequence windows were opened in, and the layout engine
iterates *that*:

```rust
for (index, window_id) in window_order.iter().enumerate() {
    if let Some(window) = windows.get(window_id) {
```
<sub>`src/main.rs:46-49`</sub>

The `if let Some(...)` is defensive: the vector owns the order, the map owns the data, and
the lookup bridges them.

### `focused_index: usize`

An **index into `window_order`**, not a window id. This is the subtle one and it will
bite you.

When a window closes, every index after it shifts down by one. A stored index that used
to mean "BETA" now silently means whatever took its place. `DestroyNotify` exists largely
to repair this — see [section 6.3](#63-destroynotify).

### Invariants

| # | Invariant | Maintained by |
| --- | --- | --- |
| 1 | Every key in `windows` appears in `window_order` | `MapRequest` inserts into both |
| 2 | Every key in `window_order` appears in `windows` | `DestroyNotify` removes from both |
| 3 | `window_order` order equals window-open order | Only ever `push`ed to, `retain`ed from |
| 4 | `focused_index < window_order.len()`, or the list is empty and the index is `0` | `DestroyNotify` clamping; `KeyPress` guards on `!is_empty()` |

Invariants 1 and 2 are why the code always touches the containers in pairs:

```rust
windows.insert(window, managed_window);
window_order.push(window);
```
<sub>`src/main.rs:287-288`</sub>

```rust
windows.remove(&event.window);
window_order.retain(|&id| id != event.window);
```
<sub>`src/main.rs:363-364`</sub>

`retain` is the right call for the vector — it filters in place and preserves order.
`remove` is right for the map.

---

## 5. The event loop

```rust
loop {
    let event = conn.wait_for_event()?;

    match event {
        Event::MapRequest(event) => { /* ... */ }
        Event::ConfigureRequest(event) => { /* ... */ }
        Event::DestroyNotify(event) => { /* ... */ }
        Event::KeyPress(event) => { /* ... */ }

        _ => {}
    }

    conn.flush()?;
}
```
<sub>`src/main.rs:252-436`</sub>

Three things worth knowing:

- **`wait_for_event()` blocks.** No `poll()`, no `select()`, no timer. The WM is purely
  reactive — which is lovely right up until you want a status bar or an idle timeout, at
  which point this shape has to change.
- **`flush()` runs every iteration.** Requests like `configure_window` are buffered, and
  `flush()` pushes them to the server. These particular calls return `()`, i.e. they're
  fire-and-forget, so an unchecked failure here would be completely silent.
- **`_ => {}` swallows everything else.** Unhandled events vanish without a trace. That
  includes `UnmapNotify`, `Expose`, `EnterNotify`, `FocusIn`/`FocusOut`, and the entire
  `ClientMessage` family.

---

## 6. Event handlers

### 6.1 `MapRequest`

<sub>`src/main.rs:256-306`</sub>

A client asking to be shown. This is the busiest handler in the program.

**1. Ask the server where the window currently is.** `get_geometry` is a blocking round
trip, purely to seed the struct:

```rust
let geometry = conn.get_geometry(window)?.reply()?;
```

**2. Create and register the window.**

```rust
let managed_window = ManagedWindow {
    id: window,
    x: geometry.x,
    y: geometry.y,
    width: geometry.width,
    height: geometry.height,
    floating: false,
};

windows.insert(window, managed_window);
window_order.push(window);
```

`floating: false` is hardcoded — every new window is tiled. Nothing opts out yet.

**3. Re-run the layout**, so the newcomer gets a column and everyone else reflows around
it.

**4. Map the window.** Only now does the client actually draw:

```rust
conn.map_window(event.window)?;
```

Because of `SubstructureRedirect`, the client has been patiently blocked since step 1.

**5. Focus the first window only.**

```rust
if window_order.len() == 1 {
    focus_window(&conn, window_order[0])?;
}
```

This seeds `focused_index = 0` with something real, so the first `ALT+J` doesn't skip
window 0. Later windows deliberately **do not steal focus**.

**One design note worth noticing:** the layout is computed *before* `map_window`. At that
moment the window is unmapped-but-configured, and the geometry gets applied when it
becomes viewable.

### 6.2 `ConfigureRequest`

<sub>`src/main.rs:308-354`</sub>

A client asking to move or resize itself. Each field is forwarded conditionally, based on
which bits are set in the `value_mask`:

```rust
if event.value_mask.contains(ConfigWindow::X) {
    changes = changes.x(i32::from(event.x));

    // These if-statements update the data in the struct(managed_window)
    if let Some(managed_window) = windows.get_mut(&event.window) {
        managed_window.x = event.x;
    }
}
```

It's a **pure forwarder**: the server gets exactly what the client asked for, and the same
values land in the struct. It does *not* re-run the layout — so a client that resizes
itself keeps that geometry until the next map or destroy triggers a reflow. That's the
known gap you'll feel first.

The `.border_width`, `.sibling` and `.stack_mode` branches forward without touching the
struct, because the struct doesn't model those.

### 6.3 `DestroyNotify`

<sub>`src/main.rs:356-386`</sub>

A window is gone. Past removing it from both containers and re-tiling, this handler exists
mostly to **repair `focused_index`**.

The trick is to capture the focused window *by identity before deletion*, then find it
again afterwards:

```rust
let previously_focused = window_order.get(focused_index).copied();

windows.remove(&event.window);
window_order.retain(|&id| id != event.window);

arrange_windows(&conn, &windows, &window_order, screen_width, screen_height)?;
```

Then four branches:

| Condition | Action | Why |
| --- | --- | --- |
| `window_order.is_empty()` | `focused_index = 0` | Reset to a safe base for the next window |
| The destroyed window *was* focused | Clamp index, `focus_window()` the new occupant | Focus must not dangle on a dead window |
| `previously_focused` still exists | Look up its **new** index with `.position(...)` | Its index shifted down when an earlier window went away |
| Fallback | Clamp index | Defensive, for the case where it apparently vanished |

Branches two and three are what stop a stale index from silently pointing at the wrong
window, which is the exact failure this handler exists to prevent.

### 6.4 `KeyPress`

<sub>`src/main.rs:388-429`</sub>

Dispatched as an `if / else if` chain on `event.detail` (the raw keycode):

```rust
if event.detail == super_q {
    // close focused window
} else if event.detail == super_t {
    // spawn terminal
} else {
    // ALT+J / ALT+K focus cycling
}
```

The cycling branch computes a signed delta, then applies it with `rem_euclid`:

```rust
let delta = if event.detail == super_j {
    1i32
} else if event.detail == super_k {
    -1
} else {
    0
};

if delta != 0 && !window_order.is_empty() {
    focused_index = ((focused_index as i32 + delta)
        .rem_euclid(window_order.len() as i32))
        as usize;
```

`rem_euclid` rather than `%` is load-bearing. Plain `%` on a negative left operand returns
a negative remainder in Rust, so stepping back from window 0 would panic on the index.
`rem_euclid` always lands in `0..len`, wrapping cleanly in both directions.

Then the target gets raised **and** focused — two separate operations that constantly get
conflated. See [section 8](#8-the-focus-model).

---

## 7. The tiling engine

`arrange_windows()` is the entire layout system. All of it:

```rust
fn arrange_windows(
    conn: &RustConnection,
    windows: &HashMap<u32, ManagedWindow>,
    window_order: &Vec<u32>,
    screen_width: u32,
    screen_height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    if windows.len() == 0 {
        return Ok(());
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
```
<sub>`src/main.rs:33-62`</sub>

**The algorithm, entire:** divide screen width by window count, give window *n* the slice
starting at `n * window_width`, full height, at `y = 0`.

Despite the name "vertical tiling", the result is a row of **vertical columns** side by
side — every window is tall and narrow.

### Worked example (empirically verified)

Screen 1280x800, two windows opened in order ALPHA then BETA:

```
0x60000c "BETA":  640x800+640+0
0x40000c "ALPHA": 640x800+0+0
```

`1280 / 2 = 640`. BETA is index 1 → `x = 1 * 640 = 640`. ALPHA is index 0 → `x = 0`. Both
are 800 tall because height is always the full screen. Verified for real — see
[section 13](#13-verified-behaviour).

### Properties and limits

- **Re-entrant by design.** It takes `&HashMap` and `&Vec`, mutates nothing, and gets
  called after every structural change. No dirty flag, no diffing — every call rewrites
  every window's geometry from scratch.
- **No gaps or borders.** Windows sit flush. `ConfigureWindowAux` isn't given a border
  here, so there's nothing to decorate with either.
- **Integer division truncates.** Three windows on a 1000px screen gives `1000/3 = 333`,
  and the last window ends at 999. One pixel vanishes, permanently, on every reflow.
- **Collapses when windows exceed screen width.** Once the count passes the pixel width,
  `screen_width / windows.len()` becomes `0`. No panic, but every window gets configured
  with zero width and the whole layout disappears. A `.max(1)` would prevent it.
- **Fixed at startup.** `screen_width` / `screen_height` come from startup only. There's
  no notion of a work area, struts, or a panel — all of which a real WM needs the moment
  someone runs a bar.

---

## 8. The focus model

This is the part of X11 that confuses everybody, and core_WM is built around it rather
than on top of it.

### Stacking order and focus are unrelated

Raising a window does **not** focus it. Focus is a separate per-window property you set
explicitly with `SetInputFocus`. That's why the cycle handler does two things:

```rust
// Raise it so the focused window is on top...
conn.configure_window(
    window_id,
    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
)?;

// ...and actually give it the focus.
focus_window(&conn, window_id)?;
```
<sub>`src/main.rs:420-426`</sub>

Doing only the first raises the window and leaves the keyboard on the old one. Very easy
to get wrong, and very easy to ship by accident because it looks like it works.

### `focus_window()`

```rust
fn focus_window(
    conn: &RustConnection,
    window_id: Window,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.set_input_focus(InputFocus::POINTER_ROOT, window_id, Time::CURRENT_TIME)?
        .check()?;

    Ok(())
}
```
<sub>`src/main.rs:69-77`</sub>

The signature trips up nearly everyone. The X11 request takes its arguments in the order
**`(revert_to, focus, time)`** — the destination is *not* first:

- `revert_to = InputFocus::POINTER_ROOT` — if this window is destroyed while focused, the
  X server picks the next focus based on the **pointer's** position instead of falling
  back to a fixed window.
- `focus = window_id` — the window to actually focus.
- `time = Time::CURRENT_TIME` — use server time, which avoids timestamp comparisons
  against server-generated events.

### The hover side effect

Because revert-to is `POINTER_ROOT`, moving the mouse across the screen visibly shifts
focus. You can see it happen, which is a nice confirmation that focus is really being
moved at all.

I want to be straight with you here: the exact mechanism behind this isn't fully pinned
down. It's pointer-root revert semantics interacting with how the X server tracks focus,
and I've read it enough times to have an opinion but not enough to call it proven. It
works, it looks intentional, and it isn't going away — so it's documented here because
new contributors will otherwise find it baffling and assume they've broken something.

### Focus follows the client, not the index

`focused_index` is the WM's model of focus. `SetInputFocus` is the X server's model. The
two are kept in sync only by core_WM itself.

core_WM doesn't watch `FocusIn`/`FocusOut`, so if a client calls `SetInputFocus` on
itself — some toolkits do — the two diverge and the WM's idea goes stale until the next
`ALT+J` or window close. Another entry for [section 14](#14-known-gaps-and-rough-edges).

---

## 9. Input: grabs and keysym resolution

### Why a passive grab is mandatory

When you press a key, the X server delivers `KeyPress` to whatever window has focus —
which is a *client*, not the WM. Without intervention, `ALT+J` gets typed into whatever
application is focused and the WM never hears about it.

A **passive grab** ("this combination belongs to that window") is the fix:

```rust
conn.grab_key(
    false,       // owner_events: deliver to the WM, not the focused client
    root,        // grab window
    ModMask::M1, // Alt
    super_j,
    GrabMode::ASYNC,
    GrabMode::ASYNC,
)?
.check()?;
```
<sub>`src/main.rs:211-219`</sub>

Until the combination is pressed, nothing happens. The moment it is, the X server
activates the grab and routes the `KeyPress` to the grab window — the root — and from
there into the WM's event loop.

Three consequences:

- **Every binding needs its own `grab_key` call.** There is no wildcard, hence the four
  copy-pasted blocks.
- **`owner_events = false`** means the WM gets the event even though a client is focused.
  That's the entire point.
- `GrabMode::ASYNC` on keyboard and pointer keeps events flowing to the WM instead of
  freezing until an `XAllowEvents`. The trade-off is that a binding combined with a
  non-grabbed key can produce a grab/ungrip sequence, which is irrelevant for four
  uncomplicated bindings.

Worth noticing: the root event mask selected at startup
([section 1](#1-what-a-window-manager-is-and-why-x11-needs-one)) does **not** include
`KeyPressMask`. `KeyPress` still arrives, because the active grab redirects delivery
independently of that mask — which is why nobody had to remember to add it.

### Keysym → keycode resolution

Hardcoding keycodes would break the moment someone isn't on QWERTY, so core_WM resolves
them at startup by walking the server's keyboard mapping:

```rust
let mapping = conn
    .get_keyboard_mapping(setup.min_keycode, setup.max_keycode - setup.min_keycode + 1)?
    .reply()?;

for keycode in setup.min_keycode..=setup.max_keycode {
    for column in 0..mapping.keysyms_per_keycode {
        let symbol = xkeysym::keysym(
            keycode.into(),
            column,
            setup.min_keycode.into(),
            mapping.keysyms_per_keycode,
            mapping.keysyms.as_slice(),
        );

        if symbol == Some(target) {
            return Ok(Some(keycode));
        }
    }
}
```
<sub>`src/main.rs:85-99`</sub>

Each physical key maps to several keysyms — unshifted, shifted, AltGr — hence the inner
loop. First match wins.

**Limitation:** the lookup ignores shift level. On a layout where `J` lives somewhere
else entirely, you get the unshifted match. That's correct for the four lowercase bindings
and would be wrong the moment someone binds a capital letter.

### Why `ModMask::M1` and not `M4`

All four bindings use `M1` (Alt). Earlier iterations used `M4` (Super/Windows), which is
more conventional for a WM but frequently swallowed by the desktop environment before
the WM ever sees it. `M1` is effectively universal, which makes it the saner default for
something that has to compete with an already-running DE for the same keypress.

---

## 10. Closing windows: the ICCCM handshake

### Ask, don't kill

X11 gives you `XKillClient`, which destroys a client's connection outright. It's the wrong
tool — the client gets no chance to save work, prompt the user, or clean up. The correct
approach is the ICCCM `WM_DELETE_WINDOW` protocol, where the client *advertises* that it
understands a close request and the WM *asks*.

### The advertisement (client side)

A conforming client sets `WM_PROTOCOLS` on its own window to a list of the message names
it handles. Confirmed on a real client:

```
$ xprop -id 0x40000c WM_PROTOCOLS
WM_PROTOCOLS(ATOM): protocols  WM_DELETE_WINDOW
```

### The request (WM side)

```rust
let event = ClientMessageEvent::new(
    32,                 // format
    window,             // the client window
    wm_protocols,       // type: the WM_PROTOCOLS atom
    [
        wm_delete_window,       // data[0]: the message being sent
        x11rb::CURRENT_TIME,    // data[1]: timestamp
        0, 0, 0,                // data[2..5]: unused
    ],
);

conn.send_event(false, window, EventMask::NO_EVENT, event)?.check()?;
conn.flush()?;
```
<sub>`src/main.rs:128-149`</sub>

Read that as: *"send a client message to `window`, of type `WM_PROTOCOLS`, whose first
payload word is the `WM_DELETE_WINDOW` request."* The client receives it, shows its
"save changes?" prompt if it has unsaved work, and closes itself — which comes back to us
as an ordinary `DestroyNotify`, so the normal teardown path handles everything. Nothing
special is needed downstream.

`EventMask::NO_EVENT` is correct here: the destination is one specific window, not a
broadcast.

### The consequence

`ALT+Q` only closes clients that implement the protocol. One that ignores it stays open,
and core_WM won't force the issue.

That's the correct, polite behaviour, and I'd defend it in review — but it does mean
there's no escape hatch yet for a genuinely wedged client. You still have `xkill` and
friends in your back pocket.

---

## 11. Spawning external programs

```rust
fn launch_terminal() {
    match Command::new("xterm").spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }

        Err(error) => {
            eprintln!("Failed to launch terminal: {error}");
        }
    }
}
```
<sub>`src/main.rs:108-120`</sub>

Three deliberate choices:

- **`xterm`, hardcoded.** No config file, no `$TERMINAL` lookup. Kitty and foot and
  alacritty users get nothing, and the README tells them to start their terminal from the
  session file before handing over to core_WM.
- **The child is reaped on a background thread.** A `Child` that's never `wait()`ed on
  becomes a zombie when it exits. The WM would accumulate one per terminal opened
  forever, so the handle is moved into a detached thread purely to `wait()` on it.
- **Failure is non-fatal.** No `xterm` installed? It logs and carries on.

One thing to keep in mind: the spawned terminal is never tracked in `windows` or
`window_order`. It shows up a moment later as a normal `MapRequest`, like any other
client. That round trip is the whole mechanism — there's no special path for it.

---

## 12. Error handling

The pattern is uniform:

```rust
conn.grab_key(...)?
    .check()?;
```

Two stages:

1. **`?`** propagates `ConnectionError` from *sending* the request — socket died, that
   sort of thing.
2. **`.check()?`** turns the server's *reply* into an error. Most X11 requests hand back a
   cookie, and this is where `BadAccess`, `BadWindow` and `BadValue` surface.

Returning `Box<dyn std::error::Error>` from `main` means any unhandled failure prints and
exits non-zero.

**What isn't handled:** ordinary panics — an out-of-bounds index, say — are not caught, and
would abort the WM outright. The guards that currently prevent that (`is_empty()` checks,
`.min(len - 1)` clamping) are hand-written rather than enforced by the type system, which
is the main robustness gap as this thing grows. Someone will eventually forget one.

---

## 13. Verified behaviour

The following was confirmed for real, by running core_WM under a nested Xephyr server,
launching two terminals, and inspecting the tree with `xwininfo`:

```
$ Xephyr :78 -screen 1280x800 &
$ DISPLAY=:78 core_wm &
$ DISPLAY=:78 xterm -title ALPHA -e sleep 120 &
$ DISPLAY=:78 xterm -title BETA  -e sleep 120 &
$ DISPLAY=:78 xwininfo -root -tree

  Root window id: 0x230 (the root window) (has no name)
     2 children:
     0x60000c "BETA": ("xterm" "XTerm")  640x800+640+0  +640+0
     0x40000c "ALPHA": ("xterm" "XTerm")  640x800+0+0  +0+0
```

**Confirmed:**

- Two windows tiled into equal 640px columns filling the 1280px width, full 800px height.
- Column order matches open order (ALPHA first) — `window_order` is genuinely doing its
  job, not accidentally working out.
- Both clients advertise `WM_DELETE_WINDOW`, so the close handshake targets a real atom.
- No `_NET_ACTIVE_WINDOW` on the root, confirming EWMH really is unimplemented as the
  roadmap says.

**Not confirmed** — no synthetic key-event tool was available in the test environment:

- `ALT+J` / `ALT+K` focus cycling, and the `StackMode::ABOVE` restack.
- `ALT+Q` closing a window end to end.
- `ALT+T` spawning a terminal.
- The `POINTER_ROOT` hover-to-focus behaviour from [section 8](#8-the-focus-model).

Those follow from the code paths in [sections 6](#6-event-handlers),
[9](#9-input-grabs-and-keysym-resolution) and [10](#10-closing-windows-the-icccm-handshake),
but they're not empirically verified and I'd rather say so than imply otherwise.
`xdotool` or `xev` would close the gap in about a minute.

---

## 14. Known gaps and rough edges

The consolidated list, with code references. The README carries a shorter, user-facing
version.

| Gap | Where | Impact |
| --- | --- | --- |
| Clicking a window doesn't focus it | no `ButtonPress`/`FocusIn` handler | Focus only via `ALT+J` / `ALT+K` |
| Keybindings hardcoded | `src/main.rs:191-249` | No config; four bindings total |
| `floating` never read | `src/main.rs:29` | Dead field; no floating support |
| Client geometry changes get overwritten on next reflow | `src/main.rs:308-354` | User-driven resize/move doesn't persist |
| Layout collapses if windows exceed screen width | `src/main.rs:44` | `width / count` → `0` |
| Integer division drops remainder pixels | `src/main.rs:44` | Permanent sub-column-width gap |
| `ManagedWindow` geometry fields too narrow | `src/main.rs:25-28` | `i16` / `u16` overflow on huge screens |
| Screen size read once at startup | `src/main.rs:158-159` | Resolution changes ignored |
| Blocking `wait_for_event()`, no `poll()` | `src/main.rs:253` | No timers, no idle handling |
| `KeyPress` doesn't check `event.state` | `src/main.rs:388` | A client-side keyboard grab could make a plain `j`/`k` trigger bindings |
| Terminal binary hardcoded to `xterm` | `src/main.rs:109` | `ALT+T` only ever runs xterm |
| No EWMH / ICCCM property publishing | — | Panels, pagers, taskbars won't cooperate |
| No `_NET_SUPPORTING_WM_CHECK` | — | Nothing can identify core_WM as a WM |
| Unhandled events dropped silently | `src/main.rs:431` | Includes `UnmapNotify`, `FocusIn/Out`, `ClientMessage` |

---

## 15. How to extend core_WM

### Add a keybinding

Five touchpoints, in this order:

1. **Declare the keysym** in the block at `src/main.rs:191-194`:
   ```rust
   let s_keysym = xkeysym::Keysym::from_char('s');
   ```

2. **Resolve the keycode** alongside the existing ones at `src/main.rs:196-206`, with the
   same `.ok_or(...)?` treatment so a missing key fails loudly at startup.

3. **Register the passive grab** — copy one of the existing `grab_key` blocks at
   `src/main.rs:211-249` and change the keycode.

4. **Handle it** by adding a branch to the `if / else if` chain in `Event::KeyPress`
   (`src/main.rs:388-429`).

5. **Document it** — the README keybindings table *and* a progress entry.

Step 2 is the one everybody forgets, and the result is a WM that boots happily and
silently ignores half its bindings.

### Add a layout

`arrange_windows()` is a free function with no dependency on `main`'s state beyond its four
arguments, so a new layout can sit right next to it. The natural next step is replacing it
with an enum:

```rust
enum Layout { Vertical, Horizontal, Floating, MasterStack }
```

and dispatching on that. Floating is already anticipated — the `floating` field exists on
`ManagedWindow` and is set to `false` at creation, so the field is waiting for something
to read it.

### Introduce a config file

The entire hardcoded surface today is small and well-defined:

- the four keysyms and keycodes (`src/main.rs:191-206`)
- the four grabs (`src/main.rs:211-249`)
- the modifier (`ModMask::M1`)
- the terminal command (`launch_terminal`, `src/main.rs:109`)

Those four are the whole thing, which makes a first config file a genuinely tractable
afternoon rather than an archaeological dig.

### Set `_NET_SUPPORTING_WM_CHECK` and friends

Without this, no external tool can recognise core_WM as the WM at all. The sequence is:
create an `InputOnly` window owned by the root, set `_NET_SUPPORTING_WM_CHECK` and
`_NET_SUPPORTING_WM_LIST` on the root to point at that window, and select
`SubstructureRedirect` and `SubstructureNotify` on it.

This is the gateway to actual desktop integration — taskbars, panels and pagers all go
looking for it, and until it's there they'll politely ignore core_WM.

---

## 16. Glossary

| Term | Meaning |
| --- | --- |
| **Client** | An application with one or more windows. |
| **Window manager (WM)** | The process deciding window placement and owning input focus. |
| **Root window** | The X11 window every top-level window is a child of. Grabbing it claims the WM role. |
| **SubstructureRedirect** | Event mask routing `MapRequest` / `ConfigureRequest` to the WM instead of the client. The core of WM authority. |
| **Map / unmap** | Making a window viewable on screen, or not. |
| **MapRequest** | A client asking to be shown. The WM must answer with `map_window`. |
| **ConfigureRequest** | A client asking to move or resize itself. |
| **DestroyNotify** | A window is being destroyed. |
| **Keysym** | A symbolic key name (`j`, `Return`), independent of physical layout. |
| **Keycode** | A numeric key position on the keyboard, layout-independent. |
| **Passive grab** | A reservation: "this key combination on this window is mine." |
| **Atom** | A server-wide interned integer standing for a name, e.g. `WM_PROTOCOLS`. |
| **Client message** | A generic X11 event used for app-to-app and WM-to-app messages. |
| **ICCCM** | Inter-Client Communication Conventions Manual — the X11 WM/client contract. |
| **EWMH** | Extended Window Manager Hints, the modern `_NET_*` property set. Not implemented yet. |
| **Stacking order** | Which window is visually on top. Independent of focus. |
| **Input focus** | Which window receives keyboard events. Set by `SetInputFocus`. |
| **POINTER_ROOT** | Revert-to value meaning "let the pointer's position decide the next focus". |
| **`rem_euclid`** | Modulo that always returns non-negative — safe for wrapping in both directions. |

---

*Documented against core_WM 0.1.5 (`src/main.rs`, 437 lines).*
