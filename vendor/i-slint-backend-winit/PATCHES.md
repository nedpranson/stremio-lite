# stremio-native patches to `i-slint-backend-winit`

Vendored verbatim from crates.io `i-slint-backend-winit 1.17.1`, then patched so
Slint honors winit's `ModifiersChanged` event. Wired up through
`[patch.crates-io]` in the root `Cargo.toml`. Every change is marked with a
`STREMIO-NATIVE PATCH` comment, so `rg "STREMIO-NATIVE PATCH"` lists the full
divergence.

## Why this lives in the engine

Slint's core reconstructs the keyboard modifiers from *physical modifier key
events* only — it has no other path (`i-slint-core` `input.rs`
`InternalKeyboardModifierState::state_update` flips the flags when it sees the
encoded Shift/Control/Alt/Meta key codes). The winit backend never forwards
winit's `WindowEvent::ModifiersChanged`, so a modifier that is set without a
modifier key event is invisible to Slint.

That is exactly what compositor-level shortcut injection does. Omarchy's
universal clipboard binds (`Super+C/V/X`) go through Hyprland's
`send_key_state`, which emits a `wl_keyboard.modifiers` event plus the target
key — but no modifier key event. Native toolkits read the modifier state, so
Ctrl+V works there; Slint did not, so `Ctrl` stayed released, `shortcut()`
returned `None`, and `TextInput` inserted the literal `v`. Feeding the real
modifier state into core fixes every injected chord, not just paste.

## The patch

### `event_loop.rs`

- `EventLoopState` gains a Linux-only `modifiers: ModifiersState` field holding
  the last state winit reported.
- A Linux-only `WindowEvent::ModifiersChanged` arm diffs the new state against
  that field and, for each modifier that actually changed, synthesizes the
  encoded Shift/Control/Alt/Meta `InternalKeyEvent` that core already consumes.
  Diffing (rather than re-asserting all four bits) avoids spurious modifier
  release events reaching FocusScope `key-released` handlers.

Linux-only on purpose: macOS swaps Command and Control (`swap_cmd_ctrl`), so
mapping winit's `SUPER` to Slint's Meta there would be wrong.

## Re-vendoring a new Slint release

1. Bump the `slint` / `slint-build` versions in the root `Cargo.toml` and build
   once so cargo unpacks the new crate.
2. `cp -r ~/.cargo/registry/src/*/i-slint-backend-winit-<version>/. vendor/i-slint-backend-winit/`,
   then delete `.cargo-ok` and `.cargo_vcs_info.json`.
3. Re-apply the changes above (`git diff` against the previous vendored tree is
   the fastest guide) and restore this file.
4. `cargo build --package stremio-native`.
