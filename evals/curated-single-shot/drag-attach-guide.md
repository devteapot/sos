# Curated drag/attach authoring context

This is a deliberately small authoring guide for one comparison against the
frozen raw `drag_attach` baseline. It is not a production prompt or skill.

- Implement one complete Luau module with `api_version = 2`,
  `render(model, state)`, and `update(model, state, event)`; do not modify the
  Rust host.
- Use one scene node with explicit width/height, at least two `path` and two
  `quad` paint operations, and its own `interaction.hit_regions`. This node is
  the visual and interaction surface, not decoration behind ordinary cards.
- Give the first note a stable hit region whose complete initial rectangle is
  at local `y <= 400`. Use `note_press`, `note_drag`, and `note_drop` actions.
- Store drag coordinates and dragging state in JSON-like state. During drag,
  update the note geometry from `event.x` and `event.y`. On drop, calculate
  rectangle overlap or point containment against the Design review target in
  Luau; do not let the host decide validity.
- A valid drop sets attached state and returns exactly one typed effect:
  `provider="notes"`, `action="attach_to_event"`, payload containing
  `note_id="note-1"` and `event_title="Design review"`.
- Render the strings `Design review` and `Interface thought`. Show attached
  state visibly after a valid drop.
- Semantic roles, when used, are only `button`, `image`, `text_field`,
  `header`, or `status`. Plain text needs no semantics table.
- Keep IDs unique, numbers finite, and all tables within the limits documented
  in `docs/experience-api.md`.

Use `experiences/android-exit-agent.luau` as the closest contract example and
`experiences/timeflow.luau` as a second geometry example. Invent a distinct
composition; do not copy either experience wholesale.
