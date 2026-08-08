# Runtime evaluation: Luau, Rhai, and Flutter/Dart

## Decision

Use **Luau embedded in the permanent GPUI host** for the first vertical slice.
Keep the experience boundary runtime-neutral so Rhai remains a useful control.
Do not use Flutter Engine as a scripting layer for GPUI.

The intended boundary is:

```text
Luau source → sandboxed evaluation → bounded UiNode tree + typed actions
                                      ↓
                           stable Rust GPUI renderer
```

Luau never receives a GPUI `Context`, raw pointer, filesystem, network socket,
or provider object. A candidate revision runs in a fresh VM and replaces the
active tree only after compilation, bounded evaluation, decoding, validation,
and a successful first render. Persistent provider data remains Rust-owned.

## Measurements on the target phone

The target was the same Samsung SM-A336B ARM64 device used for Milestone 0.

| Candidate | Device observation | Conclusion |
| --- | --- | --- |
| Luau through `mlua 0.12.0` | A small UI tree evaluated in about 88 µs. A stripped ARM64 `cdylib` was about 1.5 MB; `libc++_shared.so` is also required. | Best first spike: fast, small, embeddable, and designed for sandboxed application code. |
| Rhai | A deeper representative tree compiled in about 127 µs and evaluated/validated in about 19 µs; measured footprint was about 1.13 MiB. | Strong control and possible fallback, but less aligned with the desired Luau authoring/tooling model. |
| Flutter Engine + Dart VM + Impeller | Debug hot reload still depends on a development host compiling and driving the VM; release apps use AOT snapshots. Flutter would also replace, rather than script, the GPUI rendering layer. | A credible alternative UI stack, not an on-device interpreted layer for GPUI. |

The measured cached native loop was roughly 14.5 seconds: 8.70 seconds Rust
release rebuild, 1.56 seconds APK packaging, 2.91 seconds install, and 1.34
seconds cold launch. This confirms that APK rebuild/install is a promotion path,
not the normal conversational mutation loop.

## Safety and lifecycle constraints

- Accept source only, force text compilation, and never accept remote bytecode.
- Start each candidate in a fresh VM with a 16 MiB memory limit.
- Cap source, tree depth, node count, child count, text, and numeric values.
- Interrupt render/update work at fixed deadlines. Rust callbacks must remain
  bounded because a VM interrupt cannot cancel blocking native code.
- Keep the Android Rust release profile at `panic = "unwind"`. Luau's protected
  calls use C++ exceptions internally; `panic = "abort"` caused an interrupted
  ARM64 candidate to escape as an uncaught `lua_exception` and abort the app.
- Keep asynchronous provider work in Rust. Luau emits effects and receives
  later events rather than retaining device futures or platform handles.
- Recreate disposable UI state on revision changes. Keep provider and user data
  outside the generated object graph.
- Retain the prior accepted source and state envelope for immediate rollback.

Luau's strict annotations help authoring but are not a runtime security
boundary. Static analysis belongs in workstation/CI tooling; the Rust decoder
and transactional swap remain authoritative on-device.

## Vertical-slice gates

The slice is confirmed when:

1. One APK renders weather, calendar, notes, and music from fake Rust providers.
2. A Luau-only revision visibly changes composition and interaction without an
   APK rebuild or process restart.
3. Invalid syntax, runaway execution, or an invalid tree leaves the accepted UI
   running.
4. Rollback restores the prior accepted experience.
5. Touch, scrolling, state preservation, and suspend/resume still work.

Initial targets are source-to-visible-frame p95 below 100 ms, update execution
below 5 ms, no crash across 1,000 swaps, and no leak trend across 20 lifecycle
cycles. These are product gates, not claims about the underlying projects.

## Primary references

- [mlua documentation](https://docs.rs/mlua/0.12.0/mlua/)
- [Luau sandboxing](https://luau.org/sandbox/)
- [Luau security policy](https://github.com/luau-lang/luau/blob/master/SECURITY.md)
- [Luau performance](https://luau.org/performance/)
- [Flutter hot reload](https://docs.flutter.dev/tools/hot-reload)
- [Flutter build modes](https://docs.flutter.dev/testing/build-modes)
- [Impeller](https://docs.flutter.dev/perf/impeller)
