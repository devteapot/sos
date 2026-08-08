# Nested SOS compositor gate

Date: 2026-08-09

SOS now has a minimal Smithay compositor that runs nested inside an existing
Wayland session. It is a policy and activation-fence prototype, not yet a
direct DRM/KMS session compositor. The slice proves that Wayland can remain
beneath the generated presentation layer while SOS owns client identity,
surface ordering, focus, input quiescing, and the evidence used to accept a
revision.

## Boundary

```text
revision supervisor
    | boot / prepare / present / confirm
    v
permanent sos-experience-host PID
    | authenticated bounded control protocol
    | register_shell / arm_presentation <- compositor evidence
    |
    | Luau -> Scene ABI v3 -> retained GPUI
    v
authenticated shell wl_surface
    |
sos-compositor (Smithay 0.7.0)
    | one shell + one fixed compatibility toplevel
    | focus/input policy + nested-backend submit fence
    v
outer development Wayland compositor
```

Luau still sees only the versioned Scene and provider capabilities. It never
receives a Wayland object, socket, file descriptor, placement primitive, or
compositor capability. `compositor-control-protocol` is a separate bounded
newline-JSON ABI between the trusted permanent host and compositor.

The compositor creates a mode-0600 control socket in a caller-owned mode-0700
runtime directory. A client must present the launch token and a PID equal to
the socket's `SO_PEERCRED` PID before opening its GPUI Wayland connection. The
compositor records that PID as the shell; all other Wayland client PIDs are
compatibility clients. This is a development-session authenticator. A
production session must also isolate service users/credentials so another
same-UID process cannot inspect launch credentials.

The policy currently admits one fullscreen shell toplevel and one 720 by 520
compatibility toplevel at the deterministic 1280 by 800 test location (280,
140). The compatibility surface is kept above the single shell surface. It is
not embedded into GPUI or exposed as a raw generated node. Popups are
constrained to the output; XWayland, layer shell, arbitrary placement, and
multiple compatibility clients are intentionally outside this gate.

## Activation fence

At `present`, the host first asks the Luau worker to commit the already prepared
VM. When the worker confirms that commit, it performs this handoff on the GPUI
event thread:

1. Ask the compositor to arm the exact request/revision pair and wait for its
   current shell-commit sequence.
2. Install the worker-confirmed retained scene and assets in the same host.
3. Request the new GPUI frame.
4. Let the compositor tag the first later root shell commit.
5. Keep compositor input quiesced until that shell commit participates in a
   successful nested backend render and submit.
6. Return the request ID, revision ID, shell-commit sequence, and backend-submit
   sequence to the host.
7. Only then emit the supervisor's `presented`; `confirm` still proves the host
   loop responds before `current` advances.

Arming at the actual retained-scene handoff matters. Arming in the earlier
supervisor request handler would permit an animated frame from the old scene to
commit while the Luau worker was still switching. Waiting for the worker's
commit result also prevents a visible revision from being certified while its
active VM is unavailable. If arming then fails, the host exits so the supervisor
recovers the durable committed revision instead of continuing with divergent
runtime and visible state.

The compositor also advertises `wp_presentation` and completes client feedback
at the same nested submit boundary. The SOS control event is stronger than the
old GPUI next-frame callback because the compositor saw the shell buffer in its
render-element state and successfully submitted the nested output. It is still
weaker than physical presentation: the outer compositor may later discard or
delay the nested compositor's buffer. The direct DRM backend must bind this
same event to KMS page-flip/presentation feedback before any hardware timing
claim.

If the permanent host dies, its control disconnect drops an armed fence and
releases input. The supervisor starts the committed revision in a new PID; the
compositor authenticates it, replaces any stale shell surface, and fences the
recovery frame without restarting itself.

## Reproduce

On a Linux development machine with Xvfb, Weston, and
`weston-simple-shm` installed:

```sh
./tools/linux-compositor/verify-nested
```

The verifier builds the locked graph and creates only disposable state. It
runs Weston's X11 backend in Xvfb, runs `sos-compositor` through its Smithay
winit backend, boots the coordinated Linux session inside that compositor,
activates `daily-flow.luau`, kills the exact host PID, waits for supervisor
recovery, and maps `weston-simple-shm` as the compatibility client. It requires:

- unchanged PID across the normal Luau activation;
- exact authority/current-pointer agreement;
- no `gpui_next_frame` fallback;
- three compositor submit fences: boot, activation, and recovered boot;
- a new authenticated PID after forced host failure;
- shell and compatibility role classification plus fixed placement.

Expected leading output:

```text
linux_nested_compositor_passed activation_pid=... restarted_pid=... revision_id=... evidence=nested_backend_submit
```

The gate passes both on the ARM64 Ubuntu 24.04 development host and inside the
reference Debian 13.6 ARM64 KVM guest. The Debian run activated revision
`552f06968bbc5c69de3db581454f60d4303289f304eaaf47a6e9dc3200297cdb` in
unchanged PID 11310, then recovered the same committed revision in PID 11514.
Its compositor evidence was:

```text
boot:      commit_sequence=1  submit_sequence=928
activate:  commit_sequence=9  submit_sequence=936
recovery:  commit_sequence=14 submit_sequence=1009
```

The compatibility client mapped at `(280, 140)`. Mesa software rendering is
acceptable for this functional gate and makes no GPU, latency, or physical
display claim.

## Source pin and remaining gate

The crate pins `smithay = 0.7.0`. Its state, input, and nested-winit setup were
reduced from the upstream MIT-licensed `smallvil` and `anvil` examples at tag
`v0.7.0`, commit `a166cf4c94b5aedc332a65aa1dd753e8148829c3`; the required notice is preserved
in `crates/sos-compositor/SMITHAY-LICENSE.txt`. See the
[Smithay repository](https://github.com/Smithay/smithay/tree/v0.7.0) and
[backend documentation](https://smithay.github.io/smithay/smithay/backend/index.html).

The next compositor gate is the same policy on the Debian VM's direct session
backend: logind/session ownership, DRM/GBM output, libinput seats, a permanent
recovery view independent of GPUI, and page-flip-backed presentation evidence.
Native Linux text editing/IME, clipboard, accessibility, touch/multi-pointer
policy, secure session credential delivery, XWayland, and boot-to-SOS service
packaging remain open. No nested result completes those gates or any physical
hardware/latency gate.
