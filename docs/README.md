# SOS documentation

Start with the product vision, then use the platform guide for the environment
you are changing. `progress.md` is the chronological evidence ledger, not the
best introduction to the system.

## Current architecture

| Document | Scope |
| --- | --- |
| [`vision.md`](vision.md) | Product goal, trusted and generated boundaries, and the current phase |
| [`experience-api.md`](experience-api.md) | Experience API v4 module, scene, event, provider, and validation contract |
| [`experience-composition.md`](experience-composition.md) | Package v4 identities, graphs, fork/remix lineage, live mounts, containment, and acceptance status |
| [`stock-experience.md`](stock-experience.md) | Linux Stock Shell and Android Stock Mobile product contracts |
| [`system-providers-v1.md`](system-providers-v1.md) | Typed provider facts, actions, authority, and remaining platform coverage |
| [`runtime-evaluation.md`](runtime-evaluation.md) | Why SOS uses Luau inside the permanent GPUI host |
| [`revision-supervisor.md`](revision-supervisor.md) | Content-addressed revisions, graph activation, recovery, and host protocol |
| [`provider-state-service.md`](provider-state-service.md) | Durable provider state and single-Experience and graph transaction semantics |
| [`sos-agent.md`](sos-agent.md) | Resident Pi authoring service, credentials, and bounded tools |
| [`progress.md`](progress.md) | Dated commands, measurements, failures, artifact identities, decisions, and next gates |

## Android and Samsung

| Document | Scope |
| --- | --- |
| [`android-product-split.md`](android-product-split.md) | Current Compat/Core product boundary and physical gate matrix |
| [`android-ui-ownership-stages.md`](android-ui-ownership-stages.md) | Historical six-stage UI ownership campaign |
| [`samsung-sm-a336b.md`](samsung-sm-a336b.md) | Device preparation, rollback risk, build basis, and physical evidence |
| [`core1-provider-parity.md`](core1-provider-parity.md) | Native Core 1 System Providers v1 implementation and open physical provider gate |
| [`aosp-cuttlefish.md`](aosp-cuttlefish.md) | Reproducible Android 17 Cuttlefish product and recovery checks |

## Linux

| Document | Scope |
| --- | --- |
| [`linux-stable-host.md`](linux-stable-host.md) | Permanent host, selectable GDM session, direct session, and current physical status |
| [`linux-compositor.md`](linux-compositor.md) | Smithay nested/direct backends, activation fences, input, and recovery |
| [`linux-vm.md`](linux-vm.md) | Debian 13 nested, direct-DRM, and boot-session gates |
| [`linux-hardware-gate.md`](linux-hardware-gate.md) | Framework physical campaign procedure and verdict rules |
| [`linux-live-image.md`](linux-live-image.md) | Mutable Fedora development-live image and release boundary |

## Historical gate reports

These reports preserve the evidence and decisions that led to the current
architecture. They may name commands or fixtures removed from the current
tree. Reproduce them from the revision recorded in the report or
`progress.md`, not by applying their commands to the current v4 product.

| Document | Recorded gate |
| --- | --- |
| [`experiment.md`](experiment.md) | Unmodified GPUI Mobile hardware spike |
| [`vertical-slice.md`](vertical-slice.md) | Original Luau to catalog IR to GPUI mutation loop |
| [`worker-stress-gate.md`](worker-stress-gate.md) | Dedicated Luau worker and 1,000 swaps |
| [`stateful-experience-gate.md`](stateful-experience-gate.md) | Stateful generated experience and rollback |
| [`stable-host-device-gate.md`](stable-host-device-gate.md) | APK-scope permanent-host regression |
| [`raw-agent-evaluation.md`](raw-agent-evaluation.md) | Frozen raw single-shot authoring baseline |
| [`android-exit-gate.md`](android-exit-gate.md) | First Android laboratory exit audit |
| [`android-exit-followup.md`](android-exit-followup.md) | Raw generation, latency, recovery, and state follow-up |
| [`android-exit-verdict.md`](android-exit-verdict.md) | Prototype-scope Android laboratory exit verdict |
| [`coordinated-activation.md`](coordinated-activation.md) | Retired single-revision activation journal that preceded v4 graphs |
| [`coordinated-promotion.md`](coordinated-promotion.md) | Retired executable-per-revision promotion prototype |
