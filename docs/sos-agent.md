# Resident Pi agent: first Linux live test

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

The model receives exactly three tools:

1. `get_experience_context` reads the active revision and complete Luau source.
2. `validate_experience` compiles, migrates, and renders a complete candidate
   against SOS's deterministic provider snapshot.
3. `submit_experience` repeats validation, installs a content-addressed
   revision, stages provider state, and asks the existing supervisor to activate
   the candidate transactionally.

There is deliberately no shell, process, arbitrary filesystem, or general
network tool. The Pi process runs as `sos-agent`; a separate broker running as
`sos-supervisor` authenticates its Unix peer credentials before touching the
revision store. The permanent host still performs the authoritative candidate
prepare/render. A rejection leaves the prior revision active.

## Deterministic live smoke test

Use three terminals in the current Wayland session. Start the normal Linux
session in the first:

```sh
./tools/sosctl linux-run --windowed
```

Start the resident service with Pi's faux provider in the second. This performs
the real SOS validation, staging, and activation path without a model API call:

```sh
./tools/sosctl linux-agent-run --fake experiences/daily-flow.luau
```

In the GPUI experience, type the request into the “Make it yours” field and
press Enter. The maintenance client remains useful for protocol diagnosis:

```sh
./tools/sosctl linux-agent-prompt "Turn this into a calm daily flow"
./tools/sosctl linux-status
```

The expected tool order is context, validation, submission. The GPUI host must
remain the same process while the active revision changes and the window renders
the daily-flow candidate.

The packaged direct-KMS gate automates this exact product path through the
semantic interface rather than calling the agent socket. It sets and submits
the Luau `agent-prompt` text session, waits for the assistant completion to
return to the Luau semantic tree, and requires the activated frame's compositor
evidence to be `drm_page_flip`.

## Live model test

Pi requires Node 22.19 or newer. Select an exact model from Pi's pinned catalog,
provide a credential, and omit `--fake`:

```sh
export SOS_AGENT_PROVIDER=openai
export SOS_AGENT_MODEL=gpt-5.2
export SOS_AGENT_API_KEY='...'
./tools/sosctl linux-agent-run
```

`anthropic` is the other supported provider. The exact available model IDs are
reported if `SOS_AGENT_MODEL` is unknown. The developer service stores the Pi
conversation under `.cache/linux-agent/messages.json`; revisions remain in the
existing Linux revision store.

## Boot image wiring

Packaging includes `sos-agent.target`, `sos-agent-authoring.service`, the
`sos-agent.service` sandbox, and the `sos-agent` system account. The reference
Debian provisioner installs checksum-pinned Node 24.18.0, and the boot verifier
builds and installs the locked package at `/usr/local/libexec/sos-agent` along
with the reference experiences and docs used by the unit.

Create `/etc/sos/agent.env` with non-secret model selection:

```ini
SOS_AGENT_PROVIDER=openai
SOS_AGENT_MODEL=gpt-5.2
```

Place the API key alone in `/etc/sos/agent-api-key`, mode `0400`, then start
`sos-agent.target`. systemd exposes the key as a service credential rather than
an environment variable. `/run/sos-agent/agent.sock` is passed only to the
trusted Linux host capability bridge. The privileged broker socket
lives in a supervisor-owned directory that the agent can traverse but cannot
modify.

For diagnosis, a prompt can still be issued from an authenticated SOS
maintenance shell:

```sh
/usr/local/bin/node /usr/local/libexec/sos-agent/dist/src/main.js prompt \
  --socket /run/sos-agent/agent.sock \
  --request "Turn this into a calm daily flow"
```

The Luau conversation surface is now the intended first human interaction.
Asset generation, visual screenshot feedback/repair, and multi-candidate repair
remain outside this milestone. The next external gate is one credentialed model
prompt entered through the booted distro's Luau experience.
