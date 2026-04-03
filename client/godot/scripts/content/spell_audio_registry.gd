extends RefCounted
class_name SpellAudioRegistry

const DEFAULT_MANIFEST_PATH := "res://content/audio/spell_cues.json"


static func load_default_manifest() -> Dictionary:
	return load_manifest(DEFAULT_MANIFEST_PATH)


static func load_manifest(path: String) -> Dictionary:
	var file := FileAccess.open(path, FileAccess.READ)
	if file == null:
		return {
			"ok": false,
			"path": path,
			"error": "could not open audio cue manifest",
			"asset_root": "res://assets/audio/spells",
			"cues": {},
		}

	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if typeof(parsed) != TYPE_DICTIONARY:
		return {
			"ok": false,
			"path": path,
			"error": "audio cue manifest must parse to a dictionary",
			"asset_root": "res://assets/audio/spells",
			"cues": {},
		}

	var manifest: Dictionary = parsed as Dictionary
	var cues: Variant = manifest.get("cues", {})
	if typeof(cues) != TYPE_DICTIONARY:
		cues = {}

	return {
		"ok": true,
		"path": path,
		"format_version": int(manifest.get("format_version", 1)),
		"asset_root": String(manifest.get("asset_root", "res://assets/audio/spells")),
		"cues": (cues as Dictionary).duplicate(true),
	}


static func lookup(manifest: Dictionary, cue_id: String) -> Dictionary:
	var cues: Dictionary = manifest.get("cues", {})
	var entry: Variant = cues.get(cue_id, {})
	if typeof(entry) != TYPE_DICTIONARY:
		return {}
	return (entry as Dictionary).duplicate(true)
