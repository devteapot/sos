# Resident Pi agent

The first integration uses `@earendil-works/pi-agent-core` and
`@earendil-works/pi-ai` 0.84.1 as a resident, unprivileged service. It is an
authoring loop for the currently running SOS experience, not a general-purpose
coding agent.

The user-facing integration is part of the generated experience, not a native
GPUI panel. Luau renders `model.agent`, owns the conversation layout and a
normal `text_session`, and emits the bounded `agent.prompt` effect. The Linux
host implements that capability over the resident service's Unix socket and
streams Pi events back into `model.agent`; Luau never receives the socket or a
GPUI context. This lets an on-the-fly revision redesign the surface while
retaining the trusted runtime boundary underneath.
The agent authoring broker rejects a candidate that removes every Luau
`text_session` with `submit_action = "agent_submit"`, so a successful rewrite
cannot strand the user without a way to request the next one.

The model receives exactly three tools on every platform:

1. `get_experience_context` reads the active revision, complete Luau entry
   source, and namespaced revision-local modules.
2. `validate_experience` stages a complete source-and-module package and
   returns every declared scenario's scene statistics or path-specific
   diagnostic.
3. `submit_experience` submits only the exact source and module bytes accepted
   by the preceding validation call.

Modules are optional bounded `{ id, source }` values loaded only by the
sandboxed revision-local `require`. Omitting the field preserves active modules;
an explicit empty list removes them. The broker preserves non-Luau sidecar
assets that the authoring model cannot inspect. Source or module drift between
validation and submission is rejected before the trusted host installs a
revision.

There is deliberately no shell, process, arbitrary filesystem, or general
network tool. On Linux the Pi process runs as `sos-agent`; a separate broker
running as `sos-supervisor` authenticates its Unix peer credentials before
touching the revision store. On AOSP, Pi can only stage the exact submitted
source in its bounded response. In both cases the permanent trusted host
performs authoritative compile/render/activation. A rejection leaves the prior
revision active.

## Deterministic live smoke test

Use three terminals in the current Wayland session. Start the normal Linux
session in the first:

```sh
./tools/sosctl linux-run --windowed
```

Start the resident service with Pi's faux provider in the second. This performs
the real SOS validation, staging, and activation path without a model API call:

```sh
./tools/sosctl linux-agent-run --fake tests/fixtures/stock-authoring-v4.luau
```

In the GPUI experience, type the request into the “Make it yours” field and
press Enter. The maintenance client remains useful for protocol diagnosis:

```sh
./tools/sosctl linux-agent-prompt "Turn this into a compact timeline"
./tools/sosctl linux-status
```

The expected tool order is context, validation, submission. The GPUI host must
remain the same process while the active Stock revision changes and the window
renders the v4 candidate.

The packaged direct-KMS gate automates this exact product path through the
semantic interface rather than calling the agent socket. It sets and submits
the Luau `agent-prompt` text session, waits for the assistant completion to
return to the Luau semantic tree, and requires the activated frame's compositor
evidence to be `drm_page_flip`.

## Live model test

Pi requires Node 22.19 or newer. SOS uses Pi's own `openai-codex` provider,
ChatGPT OAuth flow, token refresh, and request authentication; SOS only supplies
the durable credential file. Authenticate once with the headless device flow,
select an exact model from Pi's pinned catalog, and omit `--fake`:

```sh
export SOS_AGENT_PROVIDER=openai-codex
export SOS_AGENT_MODEL=gpt-5.6-sol
unset SOS_AGENT_FAKE_SOURCE
./tools/sosctl linux-agent-login
./tools/sosctl linux-agent-run
```

The login command prints `https://auth.openai.com/codex/device` and a short
code. Open that URL in any browser, enter the code, and approve the ChatGPT
Plus/Pro account. Pi writes and later refreshes the OAuth credential through
SOS's process-safe store at `.cache/linux-agent/auth.json`; the file is mode
`0600` and is never passed to Luau or an agent tool. The exact available model
IDs are reported if `SOS_AGENT_MODEL` is unknown. The developer service stores
the Pi conversation separately under `.cache/linux-agent/messages.json`;
revisions remain in the existing Linux revision store.

The API-key-backed `openai`, `openrouter`, and `anthropic` providers remain
supported. For a developer run, set `SOS_AGENT_API_KEY` as before instead of
running the OAuth login command. OpenRouter uses Pi's own provider catalog and
OpenAI-compatible transport; it is not an SOS reimplementation.

## Shared runner and AOSP native Node path

`services/sos-agent/src/runner.ts` is the single packaged Pi entrypoint for
Linux, Compat, and Core. Its `/usr/local/libexec/sos-agent/dist/agent-runner.cjs`
Linux commands retain the resident Unix-socket lifecycle. The same immutable
bundle's `stdio` command supplies AOSP's bounded one-request transport. Shared
TypeScript owns the prompt policy, provider registry, faux/real Pi runtime,
request byte limits, and the exact context → validate → submit contract;
platform adapters still own lifecycle and credential storage.

Android is ARM Linux at the kernel level, but its userspace ABI is Bionic, not
glibc. A normal Linux ARM64 Node tarball therefore does not run. SOS builds
Node v24.19.0 from the official source at
`cdc1b38d40cb567b7ad0b39c86addf830a0af0ae` with NDK r29/API 31 using
`tools/build-android-node`. The reproducible local patch supplies the missing
host-toolchain split, modern ARM64 hardware-capability lookup, and Android zlib
handling. V8's signal-based Wasm trap handler is disabled with Node's own
bundled Android patch; WebAssembly itself remains enabled.

The OTA places the ARM64/Bionic executable at `/system_ext/bin/sos-node`, its
NDK C++ runtime at `/system_ext/lib64/libc++_shared.so`, and a single-file Pi
bundle at `/system_ext/etc/sos-agent/agent-runner.cjs`. No WebView executes the
agent. Compat's platform-signed HOME launches Node as a child in the existing
privileged-app SELinux domain and exchanges one bounded JSON document over
anonymous stdin/stdout pipes. Core launches that same bundle from the fixed
native host for deterministic faux prompts, so it no longer substitutes a
Rust-local agent/tool sequence. The faux candidate fixture is passed through
Pi, which must execute context, validate, and submit before returning it.

Luau exposes provider selection, but never receives a secret. On Compat,
direct API keys and Pi's refreshed Codex OAuth document remain encrypted at
rest by an unlock-bound Android Keystore AES-GCM key. Core's OpenRouter action
instead opens a fixed Rust/GPUI password surface above the generated
experience and routes its masked input through the Core-native keyboard. The
20–512-byte visible-ASCII credential exists only in zeroized process memory,
is cleared on cancel/replacement/removal and normal process exit, and is lost
on every Core host restart. It never enters the experience model, Luau state,
revision source, accessibility semantics, argv, environment variables, files,
logs, or visible screenshots. Core copies it only into the zeroized JSON
request written to the generic runner's anonymous stdin pipe and accepts a
refreshed API-key credential only from a successful OpenRouter response.

Both Android profiles pin the exposed OpenRouter choice to
`deepseek/deepseek-v4-flash-0731`. The stdio decoder rejects any different
OpenRouter model before provider execution, including the older bare
`deepseek/deepseek-v4-flash` ID, `latest`, `:free`, and strings for which the
accepted ID is only a prefix. Every successful stdio prompt
response repeats the exact selected model (`faux` for the deterministic path),
and both the Core Rust adapter and Compat Java/Rust bridge reject a mismatch
before accepting source or refreshed credentials. The verified nonsecret
provider/model pair is logged for device evidence. Core keeps the faux child
at a 30-second monotonic deadline and gives a live provider 240 seconds; the
same managed-child guard kills and reaps either child on timeout or any later
pipe/parse failure. Pi still stages a complete source candidate; the Rust HOME
independently compiles, renders, validates, and transactionally activates that
exact source.

Failure observability is deliberately narrower than the provider response.
The runner emits only an allowlisted stage/category, the exact nonsecret model,
an optional numeric HTTP status, and a fixed safe message. Compat and Core
preserve those fields through the child/bridge boundary and surface the fixed
message until a new request, provider action, or credential change explicitly
clears it; routine provider-status polling does not erase it. Child launch,
exit, timeout, response-type, model, effect-dispatch, and agent-thread markers
are nonsecret. Stderr is drained or discarded but never surfaced. Credential
bytes, authorization headers, prompts, active/candidate source, assistant
response source, raw provider bodies, stderr, and arbitrary exception text are
never included in failure UI, logs, or failure persistence.

Android temporarily promotes HOME to an unexported `dataSync` foreground
service while native Pi is waiting on a provider or external OAuth browser.
This is required because Android otherwise treats the long-lived child of a
backgrounded HOME as a phantom process. The service and its low-importance
notification stop as soon as the bounded operation completes or is cancelled;
Pi is not kept alive while idle.

The initial Android surface intentionally offers the three requested live
choices:

- direct OpenAI API key with `gpt-5.6-luna`;
- OpenRouter API key with `deepseek/deepseek-v4-flash-0731`;
- Codex subscription device-code OAuth with `gpt-5.6-sol`.

Pi's provider registry remains underneath this narrow trusted UI, so extending
the selection does not require adding provider-specific HTTP code to SOS.

The SM-A336B physical gate passed with SELinux enforcing. Pi's Codex device
flow stored an encrypted subscription credential, a prompt from the Luau
composer produced and activated model-generated revision `fe7e19b3e635...`,
and the generated HOME survived both the final OTA and an independent HOME
process restart. A one-minute external-browser regression kept the same native
Node PID and an active foreground service, then cancellation stopped both and
left the prior credential intact. Direct OpenAI and OpenRouter dialogs were
also exercised as protected password fields, but no keys were entered and
their real-provider calls are still an explicit future gate.

## Boot image wiring

### Selectable GDM session

`tools/install-linux-login-session install` builds and installs the pinned Node
agent plus `sos-agent-authoring`, then runs per-user device-code authentication
when credentials are missing. `install --offline` instead records the checked-in
stock `default.luau` shell as an explicit faux provider; it performs no login
and does not require a credential or network request. A prompt therefore runs
the complete context/validate/submit path but resolves as `already_active`
instead of replacing the shell with a demo experience. Developers can still
pass another source explicitly to `linux-agent-run --fake` for activation tests.
Both modes use the same resident server, authoring broker, validation, submission,
transactional activation, and monitored lifecycle. In the SOS GDM session,
`sos-login-session` waits for that user's provider and revision-supervisor
sockets, starts the authoring broker and resident agent against those exact
paths, waits for the agent socket, and monitors both background processes until
logout. This is deliberately not the appliance's system-wide
`sos-agent.service`: the GDM session owns per-user state below
`${XDG_STATE_HOME:-$HOME/.local/state}/sos/agent` and a private runtime directory
that changes on every login.

Reauthenticate or change the exact model from GNOME or a text login, then start
a new SOS session:

```sh
SOS_AGENT_MODEL=gpt-5.6-sol \
  /usr/local/libexec/sos/sos-agent-login
```

This helper currently supports the subscription-backed `openai-codex` device

Without credentials or an explicit readable offline source, graphical login
refuses to start and names the helper required to repair it. An unexpected
agent or broker exit ends the SOS login so it cannot silently present a dead
agent as available. A normal `sos-agent-login` replaces offline configuration
with the authenticated provider/model configuration.

### Boot-owned appliance

Packaging includes `sos-agent.target`, `sos-agent-authoring.service`, the
`sos-agent.service` sandbox, and the `sos-agent` system account. The reference
Debian provisioner installs checksum-pinned Node 24.18.0, and the boot verifier
builds and installs the locked package at `/usr/local/libexec/sos-agent` along
with the reference experiences and docs used by the unit.

Create `/etc/sos/agent.env` with non-secret model selection:

```ini
SOS_AGENT_PROVIDER=openai-codex
SOS_AGENT_MODEL=gpt-5.6-sol
```

Ensure `SOS_AGENT_FAKE_SOURCE` is absent; that variable deliberately keeps the
reference boot gate offline and overrides live-provider selection.

Before starting the target, create the state directory and run Pi's device-code
flow as the isolated service account:

```sh
sudo install -d -o sos-agent -g sos-ipc -m 0750 /var/lib/sos-agent
sudo -u sos-agent /usr/local/bin/node \
  /usr/local/libexec/sos-agent/dist/agent-runner.cjs login \
  --provider openai-codex \
  --credentials /var/lib/sos-agent/auth.json \
  --device-code
sudo systemctl start sos-agent.target
```

Pi automatically refreshes expiring credentials and SOS persists the rotated
credential atomically. `/run/sos-agent/agent.sock` is passed only to the trusted
Linux host capability bridge. The privileged broker socket lives in a
supervisor-owned directory that the agent can traverse but cannot modify.

For an API-key boot deployment, install
`packaging/systemd/sos-agent-api-key.conf` as
`/etc/systemd/system/sos-agent.service.d/api-key.conf`, place the key alone in
`/etc/sos/agent-api-key` with mode `0400`, and select `openai` or `anthropic` in
`agent.env`. The optional drop-in exposes the key as a systemd service
credential rather than an environment variable; OAuth deployments do not need
the key file or drop-in.

For diagnosis, a prompt can still be issued from an authenticated SOS
maintenance shell:

```sh
/usr/local/bin/node /usr/local/libexec/sos-agent/dist/agent-runner.cjs prompt \
  --socket /run/sos-agent/agent.sock \
  --request "Turn this into a compact timeline"
```

The Luau conversation surface is now the intended first human interaction.
Asset generation, visual screenshot feedback/repair, and multi-candidate repair
remain outside this milestone. The next external gate is one credentialed model
prompt entered through the booted distro's Luau experience.
