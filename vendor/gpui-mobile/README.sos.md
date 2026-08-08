# SOS GPUI Mobile adapter

This directory vendors upstream `gpui-mobile` commit
`1d3ec2a1d14a63b74d1f4269340441d4eeada27a`, the same commit used by the
original hardware gate.

The root workspace supplies the upstream async-task patch and build profiles;
the vendored manifest omits its ignored non-root copies and pins the existing
Core Foundation lock choice. Two source expressions are mechanically normalized
for the workspace's warning-clean Clippy gate.

SOS adds one permanent-host integration seam: Android touch points retain
pointer count and pressure, and a bounded callback observes ID/coordinates/
phase/count/pressure before GPUI Mobile maps the NDK stream onto its legacy
single mouse/scroll compatibility path. Luau never receives the platform
window or callback; the Rust scene router applies hit testing and capture.

Keep other upstream changes out of this directory. When updating the pin,
reapply and test this hook explicitly against the connected-device pointer,
scroll, IME, and accessibility gates.
