# godot

This directory contains the Godot 4 browser shell that talks to the Rust backend over websocket signaling and WebRTC.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `.godot/`: local Godot editor metadata generated on developer machines. It is intentionally ignored from git.
- `content/`: checked-in client-side runtime manifests and other data files loaded directly by the Godot shell.
- `scenes/`: Godot scene files that define the shell's node layout.
- `scripts/content/`: runtime content loaders and registries that translate checked-in manifests into frontend lookups.
- `scripts/`: top-level Godot scripts that coordinate UI flow, networking, and state transitions.
- `tests/`: headless Godot checks for protocol behavior, shell layout, exports, and transport assumptions.
- `webrtc/`: local sync target for the optional native WebRTC extension bundle. It is intentionally ignored from git.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `export_presets.cfg`: Godot export preset definitions used by the web export script.
- `project.godot`: Godot project manifest for the browser shell.

## Future Spell Audio Assets
The frontend spell-audio seam is now checked in even though playback is not enabled yet.

Use these paths when real assets arrive:
- manifest: `client/godot/content/audio/spell_cues.json`
- registry loader: `client/godot/scripts/content/spell_audio_registry.gd`
- default asset root for actual files: `client/godot/assets/audio/spells/`

The manifest format is JSON because Godot can load it directly with `JSON.parse_string`:

```json
{
  "format_version": 1,
  "asset_root": "res://assets/audio/spells",
  "cues": {
    "mage_arc_bolt": {
      "file": "mage/arc_bolt.ogg"
    }
  }
}
```

To link that cue to an authored spell, add the same `audio_cue_id` in the backend skill YAML under `server/content/skills/*.yaml`:

```yaml
- tier: 1
  id: mage_arc_bolt
  name: Arc Bolt
  description: Fast projectile damage.
  audio_cue_id: mage_arc_bolt
  behavior:
    kind: projectile
    effect: skill_shot
    cooldown_ms: 700
    mana_cost: 16
    speed: 320
    range: 1600
    radius: 18
    payload:
      kind: damage
      amount: 18
```

The client currently resolves cue metadata only. Real playback and movement audio still belong to the remaining sound items in `0.9.7`.
