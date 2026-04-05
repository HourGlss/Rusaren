extends SceneTree

const InputBindingsScript := preload("res://scripts/input/input_bindings.gd")

var _saved_controls_exists := false
var _saved_controls_bytes := PackedByteArray()


func _init() -> void:
	call_deferred("_run")


func _run() -> void:
	_backup_saved_controls()
	var success := true
	success = _assert_default_actions_install() and success
	success = _assert_rebind_persists_to_disk() and success
	_restore_saved_controls()
	InputBindingsScript.install()
	quit(0 if success else 1)


func _assert_default_actions_install() -> bool:
	InputBindingsScript.clear_saved_bindings()
	InputBindingsScript.reset_all_bindings()
	InputBindingsScript.install()

	var success := true
	for spec in InputBindingsScript.action_specs():
		var action_name := String(spec.get("action", ""))
		if not InputMap.has_action(action_name):
			success = _fail("missing input action %s" % action_name) and success
			continue
		if InputMap.action_get_events(action_name).is_empty():
			success = _fail("input action %s should have a default binding" % action_name) and success
	return success


func _assert_rebind_persists_to_disk() -> bool:
	var action_name := InputBindingsScript.ACTION_SLOT_1
	var rebound := InputEventKey.new()
	rebound.physical_keycode = KEY_Q
	rebound.keycode = KEY_Q
	if not InputBindingsScript.set_binding(action_name, rebound):
		return _fail("rebinding slot 1 to Q should succeed")
	if InputBindingsScript.binding_text(action_name) != rebound.as_text():
		return _fail("slot 1 binding text should reflect the rebound key")

	InputMap.action_erase_events(action_name)
	var placeholder := InputEventKey.new()
	placeholder.physical_keycode = KEY_E
	placeholder.keycode = KEY_E
	InputMap.action_add_event(action_name, placeholder)
	InputBindingsScript.load_saved_bindings()
	if InputBindingsScript.binding_text(action_name) != rebound.as_text():
		return _fail("saved bindings should reload from user://controls.cfg")
	return true


func _backup_saved_controls() -> void:
	var config_path := ProjectSettings.globalize_path(InputBindingsScript.CONFIG_PATH)
	_saved_controls_exists = FileAccess.file_exists(config_path)
	if not _saved_controls_exists:
		return
	var file := FileAccess.open(config_path, FileAccess.READ)
	if file == null:
		return
	_saved_controls_bytes = file.get_buffer(file.get_length())


func _restore_saved_controls() -> void:
	var config_path := ProjectSettings.globalize_path(InputBindingsScript.CONFIG_PATH)
	if not _saved_controls_exists:
		if FileAccess.file_exists(config_path):
			DirAccess.remove_absolute(config_path)
		return
	var file := FileAccess.open(config_path, FileAccess.WRITE)
	if file == null:
		push_error("unable to restore saved controls at %s" % config_path)
		return
	file.store_buffer(_saved_controls_bytes)


func _fail(message: String) -> bool:
	push_error(message)
	return false
