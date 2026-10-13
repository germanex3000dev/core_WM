# core_WM

A small, minimalistic yet modern and featurefull X11 window manager, written in Rust.

> **Status: very early.** core_WM is currently at version `0.1.0` — at this point it is
> little more than a window management *handler*: it talks to the X server and reacts to
> a few core events. There is no configuration, no keybinding system, and no decoration
> yet. Expect things to break and the API to change.

## UPDATE
core_WM has now a *very* simple, vertical tiling logic (It's literally a function that calculates window size in a for loop).
See below for more details

## Goals

- **Small.** A tight, readable codebase you can actually hold in your head.
- **Minimalistic.** No bloat, no kitchen-sink feature set for its own sake.
- **Modern.** Written from scratch in Rust, using [x11rb] for a safe, pure-Rust X11 client.
- **Featurefull.** The long-term goal is a genuinely usable WM — see [Roadmap](#roadmap).

## Current features

- Connects to the running X11 display via `DISPLAY`.
- Selects `SubstructureRedirect` / `SubstructureNotify` on the root window.
- Reacts to `MapRequest` by mapping the client window.
- Reacts to `ConfigureRequest` by forwarding the ~~requested geometry~~ to the X server. (Has now a tiling mechanism)
- Can finally "tile" your windows in a vertical layout 
- Reacts to `DestroyNotify` and logs window closes.

## Current Progress
> This is probably the only section I will be constantly updating about small
> changes.

The latest features I implemented are:
 - A vertical tiling layout, achieved with dividing the screen in equal parts(it's not that complicated)
 - As I said, It's just a function that iterates through the windows and modifies their x position and width.
 - I'm too tired to write this properly, it's literally freaking 1 AM, so expect a better rewrite of this

## Requirements

- A running X11 server (the project targets X11, not Wayland)
- Rust (edition 2024) — or Nix, if you'd rather not install a toolchain

The only runtime dependency is `x11rb`, which speaks the X11 protocol directly over a
socket. No Xlib, no C bindings.

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
that it is *not* usable as a daily driver yet — you will not be able to move,
resize, or close windows.

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

## Core Architecture

Below is a diagram I sketched in draw.io, representing the core system architecture
of the WM (How I plan it to be)

![WM architecture](docs/core_system.drawio.svg)

Editable source: [core_wm.drawio](docs/diagrams/core_wm.drawio)
There you can also find all my other diagrams

## Roadmap

Completely not in order — this is a wish list, not a commitment:

- [ ] A configuration file and a sane set of defaults
- [ ] Keybindings and mouse bindings (move, resize, focus, close)
- [ ] Focus handling and focus stealing rules
- [ ] Window decorations
- [ ] Tiling and floating layouts
- [ ] Tabs, workspaces / tags, and multi-monitor support
- [ ] IPC (EWMH / ICCCM) so compositors and pagers work correctly

## Contributing

This is a personal project and contributions are not being solicited right now.
Patches, ideas and bug reports are still welcome once the codebase stabilises
a little — see the [issue tracker](https://github.com/germanex3000dev/core_WM/issues).

## License

Released under the [MIT License](LICENSE). Copyright (c) 2026 Germanex3000.

[x11rb]: https://github.com/baskerville/x11rb
