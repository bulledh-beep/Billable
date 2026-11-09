# gpui-pre 0.3.11, patched for Billable

Copied from crates.io (Apache-2.0, see LICENSE-APACHE) with two changes in
`src/taffy.rs`, both marked `BILLABLE PATCH`:

1. Lengths (padding, gaps, sizes) stay fractional through layout instead of
   being rounded to device pixels first.
2. Measured text keeps its exact height instead of being rounded up.

Final element edges are still snapped to device pixels after layout. That
is how Chromium lays out and paints, so Billable's 13px-rem spacing lands in
the same places as in the Electron app. Examples, benches and tests were
removed to keep the copy small.
