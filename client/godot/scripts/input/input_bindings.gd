extends RefCounted
class_name InputBindings

const CONFIG_PATH := "user://controls.cfg"
const CONFIG_SECTION_BINDINGS := "bindings"

const ACTION_MOVE_UP := "rarena_move_up"
const ACTION_MOVE_DOWN := "rarena_move_down"
const ACTION_MOVE_LEFT := "rarena_move_left"
const ACTION_MOVE_RIGHT := "rarena_move_right"
const ACTION_PRIMARY_ATTACK := "rarena_primary_attack"
const ACTION_SLOT_1 := "rarena_skill_slot_1"
const ACTION_SLOT_2 := "rarena_skill_slot_2"
const ACTION_SLOT_3 := "rarena_skill_slot_3"
const ACTION_SLOT_4 := "rarena_skill_slot_4"
const ACTION_SLOT_5 := "rarena_skill_slot_5"
const ACTION_SELF_CAST := "rarena_self_cast"

const ALLOWED_MOUSE_BUTTONS := [
	MOUSE_BUTTON_LEFT,
	MOUSE_BUTTON_RIGHT,
	MOUSE_BUTTON_MIDDLE,
	MOUSE_BUTTON_XBUTTON1,
	MOUSE_BUTTON_XBUTTON2,
]

const ACTION_SPECS := [
	{
		"action": ACTION_MOVE_UP,
		"label": "Move Up",
		"type": "key",
		"physical_keycode": KEY_W,
		"keycode": KEY_W,
	},
	{
		"action": ACTION_MOVE_DOWN,
		"label": "Move Down",
		"type": "key",
		"physical_keycode": KEY_S,
		"keycode": KEY_S,
	},
	{
		"action": ACTION_MOVE_LEFT,
		"label": "Move Left",
		"type": "key",
		"physical_keycode": KEY_A,
		"keycode": KEY_A,
	},
	{
		"action": ACTION_MOVE_RIGHT,
		"label": "Move Right",
		"type": "key",
		"physical_keycode": KEY_D,
		"keycode": KEY_D,
	},
	{
		"action": ACTION_PRIMARY_ATTACK,
		"label": "Primary Attack",
		"type": "mouse_button",
		"button_index": MOUSE_BUTTON_LEFT,
	},
	{
		"action": ACTION_SLOT_1,
		"label": "Skill Slot 1",
		"type": "key",
		"physical_keycode": KEY_1,
		"keycode": KEY_1,
	},
	{
		"action": ACTION_SLOT_2,
		"label": "Skill Slot 2",
		"type": "key",
		"physical_keycode": KEY_2,
		"keycode": KEY_2,
	},
	{
		"action": ACTION_SLOT_3,
		"label": "Skill Slot 3",
		"type": "key",
		"physical_keycode": KEY_3,
		"keycode": KEY_3,
	},
	{
		"action": ACTION_SLOT_4,
		"label": "Skill Slot 4",
		"type": "key",
		"physical_keycode": KEY_4,
		"keycode": KEY_4,
	},
	{
		"action": ACTION_SLOT_5,
		"label": "Skill Slot 5",
		"type": "key",
		"physical_keycode": KEY_5,
		"keycode": KEY_5,
	},
	{
		"action": ACTION_SELF_CAST,
		"label": "Self Cast",
		"type": "key",
		"physical_keycode": KEY_X,
		"keycode": KEY_X,
	},
]


static func install() -> void:
	for spec_value in ACTION_SPECS:
		var spec := spec_value as Dictionary
		var action_name := String(spec.get("action", ""))
		if action_name == "":
			continue
		if not InputMap.has_action(action_name):
			InputMap.add_action(action_name)
		if InputMap.action_get_events(action_name).is_empty():
			var default_event := _default_event_for(spec)
			if default_event != null:
				InputMap.action_add_event(action_name, default_event)
	load_saved_bindings()


static func action_specs() -> Array[Dictionary]:
	var copied: Array[Dictionary] = []
	for spec_value in ACTION_SPECS:
		copied.append((spec_value as Dictionary).duplicate(true))
	return copied


static func action_label(action_name: String) -> String:
	for spec_value in ACTION_SPECS:
		var spec := spec_value as Dictionary
		if String(spec.get("action", "")) == action_name:
			return String(spec.get("label", action_name))
	return action_name


static func binding_text(action_name: String) -> String:
	var event := primary_event_for(action_name)
	if event == null:
		return "Unassigned"
	return event.as_text()


static func primary_event_for(action_name: String) -> InputEvent:
	var events := InputMap.action_get_events(action_name)
	if events.is_empty():
		return null
	return events[0]


static func set_binding(action_name: String, event: InputEvent) -> bool:
	var normalized := normalize_capture_event(event)
	if normalized == null:
		return false
	_remove_conflicts(action_name, normalized)
	InputMap.action_erase_events(action_name)
	InputMap.action_add_event(action_name, normalized)
	save_bindings()
	return true


static func reset_binding(action_name: String) -> void:
	var spec := _spec_for_action(action_name)
	if spec.is_empty():
		return
	if not InputMap.has_action(action_name):
		InputMap.add_action(action_name)
	InputMap.action_erase_events(action_name)
	var default_event := _default_event_for(spec)
	if default_event != null:
		InputMap.action_add_event(action_name, default_event)
	save_bindings()


static func reset_all_bindings() -> void:
	for spec_value in ACTION_SPECS:
		var spec := spec_value as Dictionary
		reset_binding(String(spec.get("action", "")))
	save_bindings()


static func load_saved_bindings() -> void:
	var config := ConfigFile.new()
	if config.load(CONFIG_PATH) != OK:
		return

	for spec_value in ACTION_SPECS:
		var spec := spec_value as Dictionary
		var action_name := String(spec.get("action", ""))
		if not config.has_section_key(CONFIG_SECTION_BINDINGS, action_name):
			continue
		var event_data: Variant = config.get_value(CONFIG_SECTION_BINDINGS, action_name, {})
		var restored_event := _deserialize_event(event_data)
		if restored_event == null:
			continue
		InputMap.action_erase_events(action_name)
		InputMap.action_add_event(action_name, restored_event)


static func save_bindings() -> int:
	var config := ConfigFile.new()
	for spec_value in ACTION_SPECS:
		var spec := spec_value as Dictionary
		var action_name := String(spec.get("action", ""))
		config.set_value(
			CONFIG_SECTION_BINDINGS,
			action_name,
			_serialize_event(primary_event_for(action_name))
		)
	return config.save(CONFIG_PATH)


static func clear_saved_bindings() -> void:
	if FileAccess.file_exists(CONFIG_PATH):
		DirAccess.remove_absolute(ProjectSettings.globalize_path(CONFIG_PATH))


static func is_capture_candidate(event: InputEvent) -> bool:
	if event is InputEventKey:
		var key_event := event as InputEventKey
		return key_event.pressed and not key_event.echo and (
			key_event.physical_keycode != KEY_NONE or key_event.keycode != KEY_NONE
		)
	if event is InputEventMouseButton:
		var mouse_button := event as InputEventMouseButton
		return mouse_button.pressed and ALLOWED_MOUSE_BUTTONS.has(mouse_button.button_index)
	return false


static func normalize_capture_event(event: InputEvent) -> InputEvent:
	if event is InputEventKey:
		var key_event := event as InputEventKey
		var normalized_key := InputEventKey.new()
		normalized_key.physical_keycode = key_event.physical_keycode
		normalized_key.keycode = key_event.keycode
		normalized_key.shift_pressed = key_event.shift_pressed
		normalized_key.alt_pressed = key_event.alt_pressed
		normalized_key.ctrl_pressed = key_event.ctrl_pressed
		normalized_key.meta_pressed = key_event.meta_pressed
		return normalized_key
	if event is InputEventMouseButton:
		var mouse_button := event as InputEventMouseButton
		var normalized_button := InputEventMouseButton.new()
		normalized_button.button_index = mouse_button.button_index
		normalized_button.shift_pressed = mouse_button.shift_pressed
		normalized_button.alt_pressed = mouse_button.alt_pressed
		normalized_button.ctrl_pressed = mouse_button.ctrl_pressed
		normalized_button.meta_pressed = mouse_button.meta_pressed
		return normalized_button
	return null


static func _remove_conflicts(action_name: String, event: InputEvent) -> void:
	var target_signature := _serialize_event(event)
	for spec_value in ACTION_SPECS:
		var spec := spec_value as Dictionary
		var other_action_name := String(spec.get("action", ""))
		if other_action_name == "" or other_action_name == action_name:
			continue
		var filtered_events: Array[InputEvent] = []
		for existing_event in InputMap.action_get_events(other_action_name):
			if _serialize_event(existing_event) != target_signature:
				filtered_events.append(existing_event)
		if filtered_events.size() == InputMap.action_get_events(other_action_name).size():
			continue
		InputMap.action_erase_events(other_action_name)
		for filtered_event in filtered_events:
			InputMap.action_add_event(other_action_name, filtered_event)


static func _spec_for_action(action_name: String) -> Dictionary:
	for spec_value in ACTION_SPECS:
		var spec := spec_value as Dictionary
		if String(spec.get("action", "")) == action_name:
			return spec.duplicate(true)
	return {}


static func _default_event_for(spec: Dictionary) -> InputEvent:
	match String(spec.get("type", "")):
		"key":
			var key_event := InputEventKey.new()
			key_event.physical_keycode = int(spec.get("physical_keycode", KEY_NONE))
			key_event.keycode = int(spec.get("keycode", key_event.physical_keycode))
			return key_event
		"mouse_button":
			var mouse_button := InputEventMouseButton.new()
			mouse_button.button_index = int(spec.get("button_index", MOUSE_BUTTON_LEFT))
			return mouse_button
		_:
			return null


static func _serialize_event(event: InputEvent) -> Dictionary:
	if event == null:
		return {}
	if event is InputEventKey:
		var key_event := event as InputEventKey
		return {
			"type": "key",
			"physical_keycode": key_event.physical_keycode,
			"keycode": key_event.keycode,
			"shift_pressed": key_event.shift_pressed,
			"alt_pressed": key_event.alt_pressed,
			"ctrl_pressed": key_event.ctrl_pressed,
			"meta_pressed": key_event.meta_pressed,
		}
	if event is InputEventMouseButton:
		var mouse_button := event as InputEventMouseButton
		return {
			"type": "mouse_button",
			"button_index": mouse_button.button_index,
			"shift_pressed": mouse_button.shift_pressed,
			"alt_pressed": mouse_button.alt_pressed,
			"ctrl_pressed": mouse_button.ctrl_pressed,
			"meta_pressed": mouse_button.meta_pressed,
		}
	return {}


static func _deserialize_event(data: Variant) -> InputEvent:
	if typeof(data) != TYPE_DICTIONARY:
		return null
	var event_data := data as Dictionary
	match String(event_data.get("type", "")):
		"key":
			var key_event := InputEventKey.new()
			key_event.physical_keycode = int(event_data.get("physical_keycode", KEY_NONE))
			key_event.keycode = int(event_data.get("keycode", key_event.physical_keycode))
			key_event.shift_pressed = bool(event_data.get("shift_pressed", false))
			key_event.alt_pressed = bool(event_data.get("alt_pressed", false))
			key_event.ctrl_pressed = bool(event_data.get("ctrl_pressed", false))
			key_event.meta_pressed = bool(event_data.get("meta_pressed", false))
			return key_event
		"mouse_button":
			var mouse_button := InputEventMouseButton.new()
			mouse_button.button_index = int(event_data.get("button_index", MOUSE_BUTTON_LEFT))
			mouse_button.shift_pressed = bool(event_data.get("shift_pressed", false))
			mouse_button.alt_pressed = bool(event_data.get("alt_pressed", false))
			mouse_button.ctrl_pressed = bool(event_data.get("ctrl_pressed", false))
			mouse_button.meta_pressed = bool(event_data.get("meta_pressed", false))
			return mouse_button
		_:
			return null
