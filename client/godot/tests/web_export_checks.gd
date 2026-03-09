extends SceneTree

const ClientStateScript := preload("res://scripts/state/client_state.gd")
const WebSocketConfigScript := preload("res://scripts/net/websocket_config.gd")


func _init() -> void:
	var success := true
	success = _assert_same_origin_http_upgrade() and success
	success = _assert_same_origin_https_upgrade() and success
	success = _assert_custom_url_preserved() and success
	success = _assert_blank_origin_falls_back_to_local_default() and success
	success = _assert_directory_bbcode_exposes_join_links_for_open_lobbies() and success
	success = _assert_skill_buttons_only_unlock_next_tiers() and success
	success = _assert_arena_state_updates_local_combat_slots_and_effects() and success
	quit(0 if success else 1)


func _assert_same_origin_http_upgrade() -> bool:
	var helper := WebSocketConfigScript.new()
	var actual: String = helper.derive_url(
		helper.DEFAULT_LOCAL_URL,
		"http://arena.example.com",
		true
	)
	return _expect_equal(actual, "ws://arena.example.com/ws", "http origin should upgrade to ws")


func _assert_same_origin_https_upgrade() -> bool:
	var helper := WebSocketConfigScript.new()
	var actual: String = helper.derive_url(
		helper.DEFAULT_LOCAL_URL,
		"https://arena.example.com",
		true
	)
	return _expect_equal(actual, "wss://arena.example.com/ws", "https origin should upgrade to wss")


func _assert_custom_url_preserved() -> bool:
	var helper := WebSocketConfigScript.new()
	var actual: String = helper.derive_url(
		"wss://staging.example.com/ws",
		"https://arena.example.com",
		false
	)
	return _expect_equal(
		actual,
		"wss://staging.example.com/ws",
		"explicit websocket URLs should not be overwritten"
	)


func _assert_blank_origin_falls_back_to_local_default() -> bool:
	var helper := WebSocketConfigScript.new()
	var actual: String = helper.derive_url("", "", true)
	return _expect_equal(
		actual,
		helper.DEFAULT_LOCAL_URL,
		"blank origin should fall back to the local default"
	)


func _assert_directory_bbcode_exposes_join_links_for_open_lobbies() -> bool:
	var state := ClientStateScript.new()
	state.lobby_directory = [
		{
			"lobby_id": 7,
			"player_count": 2,
			"team_a_count": 1,
			"team_b_count": 1,
			"ready_count": 2,
			"phase": {
				"name": "Open",
				"seconds_remaining": 0,
			},
		},
		{
			"lobby_id": 9,
			"player_count": 3,
			"team_a_count": 2,
			"team_b_count": 1,
			"ready_count": 3,
			"phase": {
				"name": "Launch Countdown",
				"seconds_remaining": 4,
			},
		},
	]
	var actual := state.lobby_directory_bbcode()
	if not actual.contains("[url=7]Join[/url]"):
		return _fail("open lobbies should expose a click-to-join link")
	if not actual.contains("Locked"):
		return _fail("locked lobbies should render as locked")
	return true


func _assert_skill_buttons_only_unlock_next_tiers() -> bool:
	var state := ClientStateScript.new()
	state.mark_transport_state("open")
	state.local_player_id = 11
	state.local_player_name = "Alice"
	state.apply_server_event({
		"type": "MatchStarted",
		"match_id": 3,
		"round": 1,
		"skill_pick_seconds": 25,
	})
	if not state.can_choose_skill_option("Mage", 1):
		return _fail("a new player should be allowed to choose tier 1")
	if state.can_choose_skill_option("Mage", 5):
		return _fail("a new player must not be allowed to choose tier 5")

	state.apply_server_event({
		"type": "SkillChosen",
		"player_id": 11,
		"tree": "Mage",
		"tier": 1,
	})
	if state.can_choose_skill():
		return _fail("local players should not be able to choose another skill in the same round")

	state.apply_server_event({
		"type": "RoundWon",
		"round": 1,
		"winning_team": "Team A",
		"score_a": 1,
		"score_b": 0,
	})
	if not state.can_choose_skill_option("Mage", 2):
		return _fail("the next tier in a started tree should unlock next round")
	if not state.can_choose_skill_option("Warrior", 1):
		return _fail("tier 1 in an unstarted tree should still be available")
	if state.can_choose_skill_option("Mage", 3):
		return _fail("skipping from tier 1 to tier 3 should remain blocked")
	return true


func _assert_arena_state_updates_local_combat_slots_and_effects() -> bool:
	var state := ClientStateScript.new()
	state.mark_transport_state("open")
	state.local_player_id = 11
	state.local_player_name = "Alice"
	state.apply_server_event({
		"type": "MatchStarted",
		"match_id": 8,
		"round": 3,
		"skill_pick_seconds": 25,
	})
	state.apply_server_event({
		"type": "CombatStarted",
	})
	state.apply_server_event({
		"type": "ArenaStateSnapshot",
		"snapshot": {
			"width": 1800,
			"height": 1200,
			"obstacles": [
				{
					"kind": "Shrub",
					"center_x": -220,
					"center_y": -150,
					"half_width": 92,
					"half_height": 92,
				},
			],
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
					"alive": true,
					"unlocked_skill_slots": 3,
				},
			],
		},
	})
	if not state.can_send_combat_input():
		return _fail("combat input should unlock once the local arena player is alive in combat")
	if not state.can_use_combat_slot(3):
		return _fail("unlocked combat slots should be usable")
	if state.can_use_combat_slot(4):
		return _fail("locked combat slots should stay unavailable")
	if int(state.local_arena_player().get("x", 0)) != -640:
		return _fail("arena snapshots should update the local player's position")

	state.apply_server_event({
		"type": "ArenaEffectBatch",
		"effects": [
			{
				"kind": "SkillShot",
				"owner": 11,
				"slot": 1,
				"x": -640,
				"y": 220,
				"target_x": 640,
				"target_y": 220,
				"radius": 28,
			},
		],
	})
	if state.arena_effects.size() != 1:
		return _fail("arena effect batches should be retained for rendering")
	state.advance_visuals(1.0)
	if not state.arena_effects.is_empty():
		return _fail("expired arena effects should be trimmed during visual advancement")
	return true


func _expect_equal(actual: String, expected: String, context: String) -> bool:
	if actual != expected:
		return _fail("%s: expected \"%s\" but received \"%s\"" % [context, expected, actual])
	return true


func _fail(message: String) -> bool:
	printerr(message)
	return false
