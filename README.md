# core_WM

A small, minimalistic yet modern and featurefull X11 window manager, written in Rust.

> **Status: very early.** core_WM is at version `0.1.4`. It tiles windows, tracks focus,
> and answers a couple of hardcoded keybindings — but there is no configuration, no
> decoration, and no floating yet. Expect things to break and the API to change.

## Goals

- **Small.** A tight, readable codebase you can actually hold in your head.
- **Minimalistic.** No bloat, no kitchen-sink feature set for its own sake.
- **Modern.** Written from scratch in Rust, using [x11rb] for a safe, pure-Rust X11 client.
- **Featurefull.** The long-term goal is a genuinely usable WM — see [Roadmap](#roadmap).

## Current Features

- Connects to the running X11 display via `DISPLAY`.
- Selects `SubstructureRedirect` / `SubstructureNotify` on the root window.
- Tracks every client window in a `ManagedWindow` struct (id, geometry, floating flag).
- Reacts to `MapRequest` by registering the window, tiling it, and mapping it.
- Reacts to `ConfigureRequest` by forwarding the requested geometry to the X server and
  keeping the struct in sync.
- Reacts to `DestroyNotify` by dropping the window and re-running the layout.
- Arranges managed windows into equal-width vertical columns, in the order they were
  opened, so the layout stays the same between runs.
- Moves keyboard focus with `SetInputFocus`, raising the focused window on top.
- Binds `SUPER+J` / `SUPER+K` to cycle focus forward and backward, wrapping at both ends.
- Focuses the first window on startup, and re-points focus when the focused window closes.

## Current Progress

> This is the section I'll keep rewriting as things land — the running log of small steps.
> For the bigger picture, see [Goals](#goals) and [Roadmap](#roadmap).

> Yeah, I started adding dates

### Latest(major) 05.10.2026, 18:30: focus cycling with SUPER+J / SUPER+K

core_WM can now move focus between windows, which makes it more than just something that
tiles windows on its own.

- **Passive grabs.** `SUPER+J` and `SUPER+K` are registered on the root window with
  `grab_key` — that's what routes those combos to the WM instead of the focused client.
- **Keysym lookup.** `keycode_for_keysym()` walks the keyboard mapping at startup to turn
  `j` and `k` into keycodes, so the bindings follow the user's layout rather than assuming
  raw scancodes.
- **Raise, then focus.** Cycling restacks the target with `StackMode::ABOVE` *and* calls
  `set_input_focus`. Stacking alone does nothing here — focus is a separate per-window
  property in X11, not a consequence of being on top.
- **Focus follows the window.** `focused_index` is tracked across the event loop, advanced
  with `rem_euclid` so wrapping works in both directions, and corrected on `DestroyNotify`
  so focus is never left pointing at a window that's gone.

### Latest(small): deterministic window arrangement

The `windows` `HashMap` has no iteration order, so column positions could shuffle between
runs. Window ids are now recorded in a `Vec<u32>` alongside it and kept in sync by the
event loop:

- `MapRequest` pushes the new id onto the end.
- `DestroyNotify` removes it again.
- `arrange_windows()` iterates that vector instead of the map, so columns come out in the
  order the windows were opened.

### Latest(major): first working tiling layout

core_WM now tiles windows instead of letting them pile up wherever the application happens
to ask for. So far this means:

- **Window bookkeeping.** Every client is stored as a `ManagedWindow` — its X11 id,
  position, size, and a `floating` flag. The struct is updated on `ConfigureRequest` so it
  keeps up with what clients ask for.
- **`arrange_windows()`.** Splits the screen into equal-width columns and hands each
  managed window one column, full height. It's about fifteen lines of arithmetic.
- **Automatic re-layout.** Called on `MapRequest` and `DestroyNotify`, so opening or
  closing a window re-tiles everything and the remaining windows always fill the screen.

### Known Limitations

Deliberately minimal for a first cut, but worth knowing:

- ~~**Column order isn't stable.** Windows are iterated out of a `HashMap`, so the
  left-to-right order can shuffle between runs. Needs an ordered container.~~ **Fixed** by
  the `Latest(small)` entry above.
- ~~**Closing the last window panics.** `arrange_windows()` divides by the window count
  without guarding the empty case.~~ **Fixed** — `arrange_windows()` now returns early when
  the window list is empty.
- **No gaps or borders.** Windows sit flush against each other.
- **Floating isn't wired up.** The `floating` flag exists but nothing reads it yet.
- **User-driven changes don't stick.** A window that resizes or moves itself keeps that
  geometry until the next map/destroy triggers a re-layout.
- **Clicking a window doesn't move focus.** Focus only moves with `SUPER+J` / `SUPER+K`;
  there's no `ButtonPress` or `FocusIn` handling yet.
  - But somehow, moving your mouse over a window focuses on it. I don't know why, it should
    not happen, but who cares? It works.
- **Keybindings are hardcoded.** Two of them, compiled straight into the binary. There's no
  config file yet — see [Roadmap](#roadmap).

> I said I would rework it

## Requirements

- A running X11 server (the project targets X11, not Wayland)
- Rust (edition 2024) — or Nix, if you'd rather not install a toolchain

The runtime dependencies are `x11rb` and `xkeysym`. `x11rb` speaks the X11 protocol
directly over a socket — no Xlib, no C bindings — and `xkeysym` is used to translate
keycodes into keysyms when resolving keybindings.

## Building

With Cargo:

```sh
cargo build --release
```

The binary lands in `target/release/core_wm`.

With Nix (the flake provides a dev shell with `rustc`, `cargo`, `rustfmt`,
`clippy`, `rust-analyzer`, plus `xorgserver`, `xterm`, `xprop` and `xwininfo`
for testing):

```sh
nix develop
```
```sh
cargo build
```

## Running

core_WM takes no arguments. It replaces your current window manager, so be aware
that it is *not* usable as a daily driver yet — you cannot move, resize, or close
windows. The one interaction is `SUPER+J` / `SUPER+K` to cycle focus.

```sh
core_wm
```

Since it grabs the root window, launching it will log you out of most desktops.
To try it safely inside a nested X server:

```sh
Xephyr -screen 1280x800 &
DISPLAY=:1 core_wm &
DISPLAY=:1 xterm
```

Any window that misbehaves is still reachable through the raw X tools, for
example `xprop -root _NET_SUPPORTING_WM_CHECK` or `xwininfo -root -tree`.

## Running it the **real** way

If you don't want that stupid `Xephyr` setup, you can also run it somewhat like any X11 WM.
I recommend you make a seperate `corewm-session` file or whatever you want to name it. 
But before you do that:

- Compile the code with either `nix develop` or with `cargo build --release`. The binary lands in `target/release/corewm`
- Then, you can put `exec /path/to/corewm` into your `corewm-session file` (Don't forget the `#!bin/sh`)
    - **Remember:** Since core_WM has no keybind to launch a terminal, put `kitty &` before you call `exec`. Or xterm, or what kind of terminal you want to use. 
- For screen settings:
    - You can declare display settings in your `corewm-session` with `xrandr`
        - **Example:** `xrandr --output eDP-1`
    - If you want to modify your scale(*4K Laptops*):
        - `xrandr --output eDP-1 --scale 0.5x0.5`

- Save the file, and launch from a TTY with `startx ~/corewm-session -- :1`

**!IMPORTANT!**
Don't replace your `.xinitrc` if you're using another X11 WM. Even if you don't, I don't 
recommend launching core_WM via `.xinitrc`, since it's not at a stable release. I wouldn't
even say, we're at an unstable release.

## Architecture

These are sketches I made in draw.io. They describe where core_WM is *heading* — most of
the boxes below don't exist in the code yet. Editable source for all of them lives in
[core_wm.drawio](docs/diagrams/core_wm.drawio).

### System architecture

The layered view: the X11 server, the connection loop that feeds it events, the WM core
that owns state, and the layout and input systems sitting on top.

![System architecture](docs/system_architecture.drawio.svg)

### Core system

The same picture with the outside world included — the Xorg server, the panels and shell
that will talk to core_WM, and the applications it ends up managing.

![Core system](docs/core_system.drawio.svg)

### Core engine

A closer look inside the WM core: the window tree/stack in the middle, and the protocol
events it has to answer (`MapRequest`, `ConfigureRequest`, `KeyPress`).

![Core engine](docs/engine.drawio.svg)

### New window event flow

What happens when you launch an application — the seven steps from `MapRequest` through to
the window appearing on screen.

![New window event flow](docs/new_window_event_flow.drawio.svg)

### Infinite layout

The direction I want to grow the tiling engine into: an unbounded canvas that pans by
`ΔX`/`ΔY` offsets and only re-configures the windows currently visible in the viewport.

![Infinite layout](docs/infinite_layout.drawio.svg)

## Roadmap

Completely not in order — this is a wish list, not a commitment:

- [ ] A configuration file and a sane set of defaults
- [ ] Keybindings and mouse bindings — focus cycling is in, see [Current Progress](#current-progress)
- [ ] Focus handling and focus stealing rules — focus is tracked and cycled, click-to-focus is not
- [ ] Window decorations
- [ ] Tiling and floating layouts — vertical tiling is in, see [Current Progress](#current-progress)
- [ ] Tabs, workspaces / tags, and multi-monitor support
- [ ] IPC (EWMH / ICCCM) so compositors and pagers work correctly

## Contributing

This is a personal project and contributions are not being solicited right now.
Patches, ideas and bug reports are still welcome once the codebase stabilises
a little — see the [issue tracker](https://github.com/germanex3000dev/core_WM/issues).

## License

Released under the [MIT License](LICENSE). Copyright (c) 2026 Germanex3000.

[x11rb]: https://github.com/baskerville/x11rb
