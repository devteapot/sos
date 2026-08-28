# Runtime evaluation: Luau, Rhai, and Flutter/Dart

## Decision

Use **Luau embedded in the permanent GPUI host** for the first vertical slice.
Keep the experience boundary runtime-neutral so Rhai remains a useful control.
Do not use Flutter Engine as a scripting layer for GPUI.

This is a tactical latency decision, not a redefinition of the project. SOS's
[product vision](vision.md) permits the agent to invent component types, layout,
geometry, hit testing, navigation, and native behavior without a closed catalog.
The original bounded widget tree was the initial contract used to learn
quickly. It has now been replaced by Experience API v4, whose layout, content,
paint, interaction, animation, and semantics facets can be combined without a
node-type catalog. V4 also adds named exports and host-owned live mounts. The
executor supports nested paint layers,
clips/transforms, glyph runs, responsive layout programs, multi-pointer capture,
virtual Android accessibility nodes, composing IME sessions, and supervisor
sidecar assets; it can continue gaining low-level capabilities without a native
experience tier.
Agent-generated GPUI Rust is no longer an experience tier; Rust/GPUI changes
belong to the permanent host update path.

The intended boundary is:

```text
Luau source → sandboxed evaluation → bounded retained scene + typed effects
                                      ↓
                           permanent Rust/GPUI executor
```

Luau never receives a GPUI `Context`, raw pointer, filesystem, network socket,
or provider object. A candidate revision runs in a fresh VM and replaces the
active scene only after compilation, bounded evaluation, decoding, validation,
and a successful first render. Persistent provider data remains Rust-owned.

On the AOSP Cuttlefish product, Rust ownership now crosses the APK boundary.
An init-supervised native authority owns provider state, immutable revision
directories, the activation journal, and the atomic current pointer. The GPUI
process can prepare and present a candidate but cannot make it durable by
itself; it asks the authority to activate only from the next-frame callback.
Android ActivityManager restarts the HOME process independently, while init
restarts the authority. The protocol and recovery details are in
[`aosp-cuttlefish.md`](aosp-cuttlefish.md).

## Resident agent selection (2026-08-09)

Use the low-level `@earendil-works/pi-agent-core` plus
`@earendil-works/pi-ai`, pinned at 0.84.1, for the first Linux resident-agent
gate. Do not embed Pi's coding-agent harness or server. SOS owns the Unix
protocol, persistence, system prompt, tool definitions, authorization, and
activation transaction; Pi supplies the model/provider stream and tool-call
loop.

The evaluated alternatives did not fit this first boundary. Sloppy has the
closest resident-service shape, but brings its own broader workspace/tool model
and had no usable repository license at evaluation time. Prime Agent is aimed
at a remote computer-use/coding environment with substantially broader machine
authority. Pi's small core allows SOS to expose only context, validation, and
transactional submission while keeping shell and filesystem capability absent.
This is an orchestration choice above Luau, not a replacement for the sandboxed
experience runtime described here.

## Measurements on the target phone

The target was the same Samsung SM-A336B ARM64 device used for Milestone 0.

| Candidate | Device observation | Conclusion |
| --- | --- | --- |
| Luau through `mlua 0.12.0` | A small UI tree evaluated in about 88 µs. A stripped ARM64 `cdylib` was about 1.5 MB; `libc++_shared.so` is also required. | Best first spike: fast, small, embeddable, and designed for sandboxed application code. |
| Rhai | A deeper representative tree compiled in about 127 µs and evaluated/validated in about 19 µs; measured footprint was about 1.13 MiB. | Strong control and possible fallback, but less aligned with the desired Luau authoring/tooling model. |
| Flutter Engine + Dart VM + Impeller | Debug hot reload still depends on a development host compiling and driving the VM; release apps use AOT snapshots. Flutter would also replace, rather than script, the GPUI rendering layer. | A credible alternative UI stack, not an on-device interpreted layer for GPUI. |

The measured cached native loop was roughly 14.5 seconds: 8.70 seconds Rust
release rebuild, 1.56 seconds APK packaging, 2.91 seconds install, and 1.34
seconds cold launch. This confirms that APK rebuild/install belongs only to
rare permanent-host updates, not the conversational experience loop.

## Safety and lifecycle constraints

- Accept source only, force text compilation, and never accept remote bytecode.
- Start each candidate in a fresh VM with a 16 MiB memory limit.
- Cap source, scene depth, node count, child count, text, paint/hit data, and
  numeric values.
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
boundary. Workstation and CI tooling now prepend the canonical API v4 type prelude
and runs the official Luau analyzer pinned at tag `0.728`, commit
`ddcea05e1cc6f534e5eaac33325690c12f1ed274`. Every checked-in experience passes
that analyzer. Local and resident-agent validation then render the default plus
every declared hidden-state scenario and return per-scenario statistics and
path-specific diagnostics. The Rust decoder, bounded VM, and transactional
swap remain authoritative on-device; they also reject unknown keys rather than
letting a type or field typo degrade silently.

## Vertical-slice gates

The slice is confirmed when:

1. One APK renders weather, calendar, notes, and music from fake Rust providers.
2. A Luau-only revision visibly changes composition and interaction without an
   APK rebuild or process restart.
3. Invalid syntax, runaway execution, or an invalid scene leaves the accepted UI
   running.
4. Rollback restores the prior accepted experience.
5. Touch, scrolling, state preservation, and suspend/resume still work.

Initial targets are source-to-visible-frame p95 below 100 ms, update execution
below 5 ms, no crash across 1,000 swaps, and no leak trend across 20 lifecycle
cycles. These are product gates, not claims about the underlying projects.

Passing these gates proves that Luau is a viable mutation path. It does not
prove the central generative claim. The next expressiveness gate is a component
whose geometry, hit testing, interaction state, and provider operation are
implemented by the agent and were not already representable as a host node.

## Primary references

- [mlua documentation](https://docs.rs/mlua/0.12.0/mlua/)
- [Luau sandboxing](https://luau.org/sandbox/)
- [Luau security policy](https://github.com/luau-lang/luau/blob/master/SECURITY.md)
- [Luau performance](https://luau.org/performance/)
- [Flutter hot reload](https://docs.flutter.dev/tools/hot-reload)
- [Flutter build modes](https://docs.flutter.dev/testing/build-modes)
- [Impeller](https://docs.flutter.dev/perf/impeller)
