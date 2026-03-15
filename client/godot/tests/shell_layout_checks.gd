extends SceneTree

const MainScript := preload("res://scripts/main.gd")


func _init() -> void:
	call_deferred("_run")


func _run() -> void:
	var success := true
	success = await _assert_joined_shell_hides_setup_chrome() and success
	success = await _assert_skill_pick_layout_prioritizes_skill_buttons() and success
	success = await _assert_disconnect_resets_back_to_central_shell() and success
	quit(0 if success else 1)


func _assert_joined_shell_hides_setup_chrome() -> bool:
	var shell = await _spawn_shell()
	shell.app_state.mark_transport_state("open")
	shell.app_state.local_player_id = 11
	shell.app_state.local_player_name = "Alice"
	shell.app_state.apply_server_event({
		"type": "GameLobbyCreated",
		"lobby_id": 7,
	})
	shell._refresh_ui()

	var success := true
	if shell.connection_panel.visible:
		success = _fail("joined lobby view should hide the realtime transport panel")
	if shell.right_column.visible:
		success = _fail("joined lobby view should hide the right-side status stack") and success
	if not shell.lobby_panel.visible:
		success = _fail("joined lobby view should keep the lobby panel visible") and success

	await _despawn_shell(shell)
	return success


func _assert_skill_pick_layout_prioritizes_skill_buttons() -> bool:
	var shell = await _spawn_shell()
	shell.app_state.mark_transport_state("open")
	shell.app_state.local_player_id = 11
	shell.app_state.local_player_name = "Alice"
	shell.app_state.apply_server_event({
		"type": "MatchStarted",
		"match_id": 3,
		"round": 1,
		"skill_pick_seconds": 25,
	})
	shell._refresh_ui()

	var success := true
	if not shell.match_panel.visible:
		success = _fail("match screen should be visible during skill pick")
	if not shell.skill_pick_panel.visible:
		success = _fail("skill pick phase should show the dedicated skill picker panel") and success
	if shell.combat_panel.visible:
		success = _fail("skill pick phase should hide the combat arena panel") and success

	var has_enabled_button := false
	for button in shell.skill_buttons:
		if not button.disabled:
			has_enabled_button = true
			break
	if not has_enabled_button:
		success = _fail("skill pick phase should expose at least one enabled skill choice") and success

	await _despawn_shell(shell)
	return success


func _assert_disconnect_resets_back_to_central_shell() -> bool:
	var shell = await _spawn_shell()
	shell.app_state.mark_transport_state("open")
	shell.app_state.local_player_id = 11
	shell.app_state.local_player_name = "Alice"
	shell.app_state.apply_server_event({
		"type": "MatchStarted",
		"match_id": 8,
		"round": 2,
		"skill_pick_seconds": 25,
	})
	shell.app_state.apply_server_event({
		"type": "CombatStarted",
	})
	shell._refresh_ui()
	shell.app_state.mark_transport_closed("Disconnected by test.")
	shell._refresh_ui()

	var success := true
	if shell.app_state.screen != "central":
		success = _fail("disconnect should reset the shell back to the central screen")
	if shell.match_panel.visible:
		success = _fail("disconnect should hide the stale match panel") and success
	if not shell.connection_panel.visible:
		success = _fail("disconnect should restore the connection controls") and success
	if not shell.right_column.visible:
		success = _fail("disconnect should restore the central sidebar panels") and success

	await _despawn_shell(shell)
	return success


func _spawn_shell():
	var shell = MainScript.new()
	get_root().add_child(shell)
	await process_frame
	return shell


func _despawn_shell(shell: Node) -> void:
	shell.queue_free()
	await process_frame


func _fail(message: String) -> bool:
	printerr(message)
	return false
