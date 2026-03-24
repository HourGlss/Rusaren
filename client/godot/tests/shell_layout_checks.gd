extends SceneTree

const MainScript := preload("res://scripts/main.gd")


func _init() -> void:
	call_deferred("_run")


func _run() -> void:
	var success := true
	success = await _assert_shell_removes_old_header_and_manual_connect_buttons() and success
	success = await _assert_menu_opens_fullscreen_views() and success
	success = await _assert_joined_shell_hides_setup_chrome() and success
	success = await _assert_lobby_panel_shows_roster_team_and_ready_state() and success
	success = await _assert_skill_pick_layout_prioritizes_skill_buttons() and success
	success = await _assert_disconnect_resets_back_to_central_shell() and success
	quit(0 if success else 1)


func _assert_shell_removes_old_header_and_manual_connect_buttons() -> bool:
	var shell = await _spawn_shell()
	var success := true
	if _tree_contains_label(shell, "Rusaren Control Shell"):
		success = _fail("shell should remove the old Rusaren Control Shell header") and success
	if _tree_contains_label(shell.connection_panel, "Signaling URL"):
		success = _fail("connection panel should not expose a signaling URL field to players") and success
	if _tree_contains_button(shell.connection_panel, "Connect"):
		success = _fail("connection panel should not expose a manual connect button") and success
	if _tree_contains_button(shell.connection_panel, "Disconnect"):
		success = _fail("connection panel should not expose a manual disconnect button") and success
	if _count_nodes_of_type(shell.connection_panel, LineEdit) != 1:
		success = _fail("connection panel should only expose a join lobby input") and success
	if shell.menu_button == null or shell.menu_button.text != "Menu":
		success = _fail("shell should expose a top-level Menu button") and success
	if shell.player_name_input == null:
		success = _fail("shell should keep a hidden name editor inside the fullscreen menu") and success

	await _despawn_shell(shell)
	return success


func _assert_menu_opens_fullscreen_views() -> bool:
	var shell = await _spawn_shell()
	var success := true

	shell._open_fullscreen_menu("record")
	if not shell.fullscreen_menu.visible:
		success = _fail("menu selections should open the fullscreen overlay") and success
	if not shell.record_view.visible or shell.roster_view.visible or shell.event_view.visible or shell.name_menu_view.visible:
		success = _fail("record menu should only show the record view") and success

	shell._open_fullscreen_menu("roster")
	if not shell.roster_view.visible or shell.record_view.visible or shell.event_view.visible or shell.name_menu_view.visible:
		success = _fail("roster menu should only show the roster view") and success

	shell._open_fullscreen_menu("events")
	if not shell.event_view.visible or shell.record_view.visible or shell.roster_view.visible or shell.name_menu_view.visible:
		success = _fail("event menu should only show the event view") and success

	shell._open_fullscreen_menu("name")
	if not shell.name_menu_view.visible or shell.record_view.visible or shell.roster_view.visible or shell.event_view.visible:
		success = _fail("change-name menu should only show the name editor") and success
	if shell.player_name_input.text.length() != 10:
		success = _fail("shell should seed a random ten-letter player alias") and success

	shell._close_fullscreen_menu()
	if shell.fullscreen_menu.visible:
		success = _fail("closing the menu should hide the fullscreen overlay") and success

	await _despawn_shell(shell)
	return success


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
		success = _fail("joined lobby view should hide the central actions panel")
	if not shell.lobby_panel.visible:
		success = _fail("joined lobby view should keep the lobby panel visible") and success
	if shell.fullscreen_menu.visible:
		success = _fail("joined lobby view should not force the fullscreen menu open") and success
	if shell.menu_button == null or shell.menu_button.text != "Menu":
		success = _fail("joined lobby view should keep the Menu button available") and success

	await _despawn_shell(shell)
	return success


func _assert_lobby_panel_shows_roster_team_and_ready_state() -> bool:
	var shell = await _spawn_shell()
	shell.app_state.mark_transport_state("open")
	shell.app_state.local_player_id = 11
	shell.app_state.local_player_name = "Alice"
	shell.app_state.apply_server_event({
		"type": "GameLobbySnapshot",
		"lobby_id": 7,
		"players": [
			{
				"player_id": 11,
				"player_name": "Alice",
				"team": "Team A",
				"ready": "Ready",
			},
			{
				"player_id": 22,
				"player_name": "Bob",
				"team": "Team B",
				"ready": "Not Ready",
			},
		],
		"phase": {
			"name": "Open",
		},
	})
	shell._refresh_ui()

	var success := true
	if shell.lobby_roster_log == null:
		success = _fail("lobby panel should expose a roster log") and success
	else:
		if not shell.lobby_roster_log.text.contains("Alice  |  Team A  |  Ready"):
			success = _fail("lobby panel should show the local player's team and ready state") and success
		if not shell.lobby_roster_log.text.contains("Bob  |  Team B  |  Not Ready"):
			success = _fail("lobby panel should show other players, their teams, and ready states") and success

	await _despawn_shell(shell)
	return success


func _assert_skill_pick_layout_prioritizes_skill_buttons() -> bool:
	var shell = await _spawn_shell()
	shell.app_state.mark_transport_state("open")
	shell.app_state.local_player_id = 11
	shell.app_state.local_player_name = "Alice"
	shell.app_state.apply_server_event({
		"type": "Connected",
		"player_id": 11,
		"player_name": "Alice",
		"record": {
			"wins": 0,
			"losses": 0,
			"no_contests": 0,
		},
		"skill_catalog": [
			{
				"tree": "Mage",
				"tier": 1,
				"skill_id": "mage_t1_missile",
				"skill_name": "Magic Missile",
			},
			{
				"tree": "Mage",
				"tier": 2,
				"skill_id": "mage_t2_ice_lance",
				"skill_name": "Ice Lance",
			},
			{
				"tree": "Warrior",
				"tier": 1,
				"skill_id": "warrior_t1_bash",
				"skill_name": "Bash",
			},
		],
	})
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
	if shell.skill_scroll == null or shell.skill_scroll.vertical_scroll_mode == ScrollContainer.SCROLL_MODE_DISABLED:
		success = _fail("skill pick phase should expose a scrollable catalog for larger class sets") and success

	var has_enabled_button := false
	for button in shell.skill_buttons:
		if not button.disabled:
			has_enabled_button = true
			break
	if not has_enabled_button:
		success = _fail("skill pick phase should expose at least one enabled skill choice") and success
	if shell.skill_buttons.is_empty() or not shell.skill_buttons[0].text.contains("Magic Missile"):
		success = _fail("skill buttons should render backend-authored skill names") and success

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
		success = _fail("disconnect should restore the central actions panel") and success
	if shell.menu_button == null or shell.menu_button.text != "Menu":
		success = _fail("disconnect should preserve access to the Menu button") and success

	await _despawn_shell(shell)
	return success


func _spawn_shell():
	var shell = MainScript.new()
	shell.auto_connect_enabled = false
	get_root().add_child(shell)
	await process_frame
	return shell


func _despawn_shell(shell: Node) -> void:
	shell.queue_free()
	await process_frame


func _tree_contains_label(root: Node, text: String) -> bool:
	if root is Label and (root as Label).text == text:
		return true
	for child in root.get_children():
		if _tree_contains_label(child, text):
			return true
	return false


func _tree_contains_button(root: Node, text: String) -> bool:
	if root is Button and (root as Button).text == text:
		return true
	for child in root.get_children():
		if _tree_contains_button(child, text):
			return true
	return false


func _count_nodes_of_type(root: Node, script_type) -> int:
	var total := 1 if is_instance_of(root, script_type) else 0
	for child in root.get_children():
		total += _count_nodes_of_type(child, script_type)
	return total


func _fail(message: String) -> bool:
	printerr(message)
	return false
