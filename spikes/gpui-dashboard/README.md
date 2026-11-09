# Native Dashboard test (GPUI)

Billable's Dashboard drawn natively in Rust with [GPUI](https://gpui.rs), the UI
toolkit behind the Zed editor, to see how close a native port can get to the
Electron app. It is a test, not part of the app: nothing here ships, and it
is not wired into the build.

- Reads the active profile's real database **read-only**
  (`~/Library/Application Support/Billable`, or `BILLABLE_USER_DATA`).
- One screen only: the sidebar, toolbar and Dashboard. Buttons hover but do
  nothing; there is no navigation.

## Run it

```bash
cargo build --release
./bundle.sh billable-gpui release
open target/billable-gpui.app
```

The first build compiles GPUI and takes several minutes. Full Xcode is not
needed: the `runtime_shaders` feature compiles GPUI's Metal shaders at launch.

## What it took to match the look

- **SF Rounded weights.** The Mac ships SF Rounded as one variable font. GPUI
  loads it as a single face, so every weight came out Regular. `fonts.rs` cuts
  500/600/700 copies from the system file in memory at launch (with
  `allsorts`). Each copy gets its own family name, because Core Text reports
  every copy as weight 400.
- **Colours in SVGs.** GPUI 0.2.2 swaps red and blue in SVG images, so
  `art.rs` writes every colour pre-swapped.
- **Spacing.** Billable's root font size is 13px, so Tailwind's spacing units
  are 3.25px, not 4px (`r()` in `ui.rs`).

## What GPUI can't do here (yet)

- Letter-spacing (the wordmark and titles use negative tracking).
- Rotating or scaling ordinary elements, so no icon wiggle and no
  press-to-shrink on buttons.
- Inset shadows (the top highlight on the orange buttons).
- Scrollbars: the page scrolls, but no scrollbar is drawn.
- The mascot's eyes following the pointer.
