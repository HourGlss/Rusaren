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
	success = await _assert_training_shell_surfaces_live_loadout_and_reset_controls() and success
	success = await _assert_combat_hud_surfaces_local_skill_names() and success
	success = await _assert_round_and_match_summary_panels_surface_event_data() and success
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
				"skill_description": "Fast projectile damage.",
				"skill_summary": "CD 0.7s | Cast instant | Mana 16\nProjectile: range 1500, radius 16, speed 310\nEffect: 10 damage",
				"ui_category": "damage",
			},
			{
				"tree": "Mage",
				"tier": 2,
				"skill_id": "mage_t2_ice_lance",
				"skill_name": "Ice Lance",
				"skill_description": "Burst damage with chill.",
				"skill_summary": "CD 2.0s | Cast instant | Mana 30\nBurst: cast range 250, radius 86\nEffect: 14 damage\nStatus: Chill 20 for 2s (max 2)",
				"ui_category": "control",
			},
			{
				"tree": "Warrior",
				"tier": 1,
				"skill_id": "warrior_t1_bash",
				"skill_name": "Bash",
				"skill_description": "Short melee stun.",
				"skill_summary": "CD 0.9s | Cast instant\nBeam: range 90, radius 28\nStatus: Stun for 1s",
				"ui_category": "control",
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
	elif not shell.skill_buttons[0].tooltip_text.contains("Fast projectile damage.") or not shell.skill_buttons[0].tooltip_text.contains("Projectile: range 1500"):
		success = _fail("skill button tooltips should use the authored description plus the mechanics summary") and success
	elif shell.skill_buttons[0].tooltip_text.contains("Select "):
		success = _fail("skill button tooltips should not fall back to generic select text") and success
	var damage_color: Color = shell.skill_buttons[0].get_theme_color("font_color")
	var control_color: Color = shell.skill_buttons[1].get_theme_color("font_color")
	if damage_color == control_color:
		success = _fail("skill button labels should use category colors for readability") and success

	await _despawn_shell(shell)
	return success


func _assert_training_shell_surfaces_live_loadout_and_reset_controls() -> bool:
	var shell = await _spawn_shell()
	shell.app_state.mark_transport_state("open")
	shell.app_state.local_player_id = 11
	shell.app_state.local_player_name = "Alice"
	shell._refresh_ui()

	var success := true
	if shell.start_training_button == null or not shell.start_training_button.visible:
		success = _fail("central shell should expose the start-training action") and success
	elif shell.start_training_button.disabled:
		success = _fail("start-training should be enabled from the central shell when transport is open") and success

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
				"tree": "Warrior",
				"tier": 1,
				"skill_id": "warrior_t1_bash",
				"skill_name": "Bash",
				"skill_description": "Short melee stun.",
				"skill_summary": "CD 0.9s | Cast instant\nBeam: range 90, radius 28\nStatus: Stun for 1s",
				"ui_category": "control",
			},
			{
				"tree": "Ranger",
				"tier": 4,
				"skill_id": "ranger_t4_watchpost",
				"skill_name": "Watchpost",
				"skill_description": "Place a vision ward.",
				"skill_summary": "CD 7.0s | Cast instant | Mana 20\nWard: place 180 away, vision radius 160, 120 HP, lasts 60s",
				"ui_category": "utility",
			},
		],
	})
	shell.app_state.apply_server_event({
		"type": "TrainingStarted",
		"training_id": 14,
	})
	shell.app_state.apply_server_event({
		"type": "ArenaStateSnapshot",
		"snapshot": {
			"mode": "Training",
			"phase": "Combat",
			"width": 900,
			"height": 700,
			"tile_units": 50,
			"footprint_tiles": PackedByteArray([0x1F, 0x01]),
			"visible_tiles": PackedByteArray([0x1F, 0x01]),
			"explored_tiles": PackedByteArray([0x1F, 0x01]),
			"obstacles": [],
			"deployables": [
				{
					"id": 91,
					"owner": 11,
					"team": "Team A",
					"kind": "TrainingDummyResetFull",
					"x": -120,
					"y": 40,
					"radius": 28,
					"hit_points": 10000,
					"max_hit_points": 10000,
					"remaining_ms": 0,
				},
				{
					"id": 92,
					"owner": 11,
					"team": "Team A",
					"kind": "TrainingDummyExecute",
					"x": 120,
					"y": 40,
					"radius": 28,
					"hit_points": 500,
					"max_hit_points": 10000,
					"remaining_ms": 0,
				},
			],
			"players": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"x": -320,
					"y": 0,
					"aim_x": 160,
					"aim_y": 0,
					"hit_points": 100,
					"max_hit_points": 100,
					"mana": 100,
					"max_mana": 100,
					"alive": true,
					"unlocked_skill_slots": 5,
					"primary_cooldown_remaining_ms": 0,
					"primary_cooldown_total_ms": 600,
					"slot_cooldown_remaining_ms": [0, 0, 0, 0, 0],
					"slot_cooldown_total_ms": [0, 0, 0, 0, 0],
					"equipped_skill_trees": ["Warrior", "", "", "Ranger", ""],
					"current_cast_slot": 0,
					"current_cast_remaining_ms": 0,
					"current_cast_total_ms": 0,
					"active_statuses": [],
				},
			],
			"projectiles": [],
			"training_metrics": {
				"damage_done": 420,
				"healing_done": 150,
				"elapsed_ms": 5000,
			},
		},
	})
	shell._refresh_ui()

	if not shell.match_panel.visible:
		success = _fail("training should enter the match shell") and success
	if not shell.combat_panel.visible:
		success = _fail("training should also keep the combat panel visible") and success
	if shell.skill_pick_panel.visible:
		success = _fail("training should keep the loadout catalog behind a menu instead of pinning it into the shell") and success
	if shell.training_loadout_button == null or not shell.training_loadout_button.visible:
		success = _fail("training should expose a dedicated class loadout button") and success
	elif shell.training_loadout_button.text != "Class Loadout":
		success = _fail("training should label the loadout opener clearly") and success
	if shell.training_metrics_label == null or not shell.training_metrics_label.visible:
		success = _fail("training should surface the metrics label") and success
	elif not shell.training_metrics_label.text.contains("DPS") or not shell.training_metrics_label.text.contains("dmg 420"):
		success = _fail("training metrics should show damage, healing, and throughput") and success
	if shell.reset_training_button == null or not shell.reset_training_button.visible:
		success = _fail("training should expose the reset action") and success
	if shell.quit_arena_button == null or not shell.quit_arena_button.visible:
		success = _fail("training should expose the quit-training action") and success
	elif shell.quit_arena_button.text != "Back To Lobby Select":
		success = _fail("training should expose an explicit back-to-lobby button") and success
	if shell.score_label.text != "Round 1, Team A 0 : 0 Team B":
		success = _fail("training should surface the compact round-and-score header") and success
	if not _popup_contains_item(shell.menu_button.get_popup(), "Training Loadout"):
		success = _fail("training menu should expose the training loadout entry") and success

	shell._open_fullscreen_menu("loadout")
	if not shell.fullscreen_menu.visible or not shell.training_loadout_view.visible:
		success = _fail("training loadout should open inside the fullscreen menu overlay") and success
	if not shell.skill_pick_panel.visible:
		success = _fail("opening the training loadout menu should surface the skill catalog") and success
	if shell.skill_pick_panel.get_parent() != shell.training_loadout_host:
		success = _fail("training loadout menu should host the skill catalog inside the overlay") and success
	if shell.skill_buttons.is_empty() or not shell.skill_buttons[0].tooltip_text.contains("Training equip: replace slot 1 immediately."):
		success = _fail("training skill tooltips should explain immediate slot replacement") and success

	await _despawn_shell(shell)
	return success


func _assert_combat_hud_surfaces_local_skill_names() -> bool:
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
				"skill_description": "Fast projectile damage.",
				"skill_summary": "CD 0.7s | Cast instant | Mana 16\nProjectile: range 1500, radius 16, speed 310\nEffect: 10 damage",
				"ui_category": "damage",
			},
			{
				"tree": "Mage",
				"tier": 2,
				"skill_id": "mage_t2_ice_lance",
				"skill_name": "Ice Lance",
				"skill_description": "Burst damage with chill.",
				"skill_summary": "CD 2.0s | Cast instant | Mana 30\nBurst: cast range 250, radius 86\nEffect: 14 damage\nStatus: Chill 20 for 2s (max 2)",
				"ui_category": "control",
			},
		],
	})
	shell.app_state.apply_server_event({
		"type": "MatchStarted",
		"match_id": 3,
		"round": 1,
		"skill_pick_seconds": 25,
	})
	shell.app_state.apply_server_event({
		"type": "SkillChosen",
		"player_id": 11,
		"tree": "Mage",
		"tier": 1,
	})
	shell.app_state.local_round_skill_locked = false
	shell.app_state.apply_server_event({
		"type": "RoundWon",
		"round": 1,
		"winning_team": "Team A",
		"score_a": 1,
		"score_b": 0,
	})
	shell.app_state.apply_server_event({
		"type": "SkillChosen",
		"player_id": 11,
		"tree": "Mage",
		"tier": 2,
	})
	shell.app_state.apply_server_event({
		"type": "CombatStarted",
	})
	shell.app_state.apply_server_event({
		"type": "ArenaStateSnapshot",
		"snapshot": {
			"width": 1800,
			"height": 1200,
			"players": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"x": -640,
					"y": 220,
					"aim_x": 120,
					"aim_y": 0,
					"hit_points": 100,
					"max_hit_points": 100,
					"mana": 80,
					"max_mana": 100,
					"alive": true,
					"unlocked_skill_slots": 2,
					"primary_cooldown_remaining_ms": 0,
					"primary_cooldown_total_ms": 600,
					"slot_cooldown_remaining_ms": [0, 250, 0, 0, 0],
					"slot_cooldown_total_ms": [800, 900, 0, 0, 0],
					"active_statuses": [],
				},
			],
			"projectiles": [],
		},
	})
	shell._refresh_ui()

	var success := true
	if shell.cooldown_summary_label == null:
		success = _fail("combat panel should expose a cooldown summary label") and success
	elif not shell.cooldown_summary_label.text.contains("Magic Missile") or not shell.cooldown_summary_label.text.contains("Ice Lance"):
		success = _fail("combat panel should show local skill names alongside slot cooldowns") and success

	await _despawn_shell(shell)
	return success


func _assert_round_and_match_summary_panels_surface_event_data() -> bool:
	var shell = await _spawn_shell()
	shell.app_state.mark_transport_state("open")
	shell.app_state.local_player_id = 11
	shell.app_state.local_player_name = "Alice"
	shell.app_state.apply_server_event({
		"type": "MatchStarted",
		"match_id": 9,
		"round": 2,
		"skill_pick_seconds": 25,
	})
	shell.app_state.apply_server_event({
		"type": "RoundSummary",
		"summary": {
			"round": 2,
			"round_totals": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"damage_done": 140,
					"healing_to_allies": 24,
					"healing_to_enemies": 0,
					"cc_used": 2,
					"cc_hits": 1,
				},
			],
			"running_totals": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"damage_done": 290,
					"healing_to_allies": 44,
					"healing_to_enemies": 3,
					"cc_used": 4,
					"cc_hits": 2,
				},
			],
		},
	})
	shell._refresh_ui()

	var success := true
	if shell.round_summary_log == null or not shell.round_summary_log.visible:
		success = _fail("skill-pick layout should surface the round summary panel when summary data exists") and success
	elif not shell.round_summary_log.text.contains("Running total"):
		success = _fail("round summary panel should render the running totals block") and success

	shell.app_state.apply_server_event({
		"type": "MatchEnded",
		"outcome": "Victory",
		"score_a": 3,
		"score_b": 1,
		"message": "Team A wins the match.",
	})
	shell.app_state.apply_server_event({
		"type": "MatchSummary",
		"summary": {
			"rounds_played": 4,
			"totals": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"damage_done": 480,
					"healing_to_allies": 96,
					"healing_to_enemies": 5,
					"cc_used": 6,
					"cc_hits": 4,
				},
			],
		},
	})
	shell._refresh_ui()

	if not shell.results_panel.visible:
		success = _fail("results phase should show the results panel") and success
	if shell.match_summary_log == null or not shell.match_summary_log.visible:
		success = _fail("results screen should surface the match summary panel when summary data exists") and success
	elif not shell.match_summary_log.text.contains("Rounds played: 4"):
		success = _fail("match summary panel should render the final rounds-played line") and success

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


func _popup_contains_item(popup: PopupMenu, item_text: String) -> bool:
	for index in range(popup.get_item_count()):
		if popup.get_item_text(index) == item_text:
			return true
	return false


func _fail(message: String) -> bool:
	printerr(message)
	return false
