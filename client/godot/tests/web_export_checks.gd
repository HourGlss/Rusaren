extends SceneTree

const ClientStateScript := preload("res://scripts/state/client_state.gd")
const ArenaViewScript := preload("res://scripts/arena/arena_view.gd")
const DevSocketClientScript := preload("res://scripts/net/dev_socket_client.gd")
const WebSocketConfigScript := preload("res://scripts/net/websocket_config.gd")


func _init() -> void:
	var success := true
	success = _assert_same_origin_http_upgrade() and success
	success = _assert_same_origin_https_upgrade() and success
	success = _assert_custom_url_preserved() and success
	success = _assert_bootstrap_url_derivation() and success
	success = _assert_session_token_append() and success
	success = _assert_blank_origin_falls_back_to_local_default() and success
	success = _assert_signaling_socket_detaches_after_control_channel_open() and success
	success = _assert_directory_bbcode_exposes_join_links_for_open_lobbies() and success
	success = _assert_skill_buttons_only_unlock_next_tiers() and success
	success = _assert_arena_state_updates_local_combat_slots_and_effects() and success
	success = _assert_aura_deployables_stay_hidden_from_client_render_state() and success
	success = _assert_local_skill_labels_and_render_smoothing() and success
	success = _assert_round_and_match_summaries_and_combat_text() and success
	success = _assert_player_token_palette_and_team_rings() and success
	success = _assert_remote_cast_labels_use_known_roster_skills() and success
	success = _assert_resource_labels_only_show_for_local_player() and success
	success = _assert_fog_rounds_only_on_visibility_boundary() and success
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


func _assert_bootstrap_url_derivation() -> bool:
	var helper := WebSocketConfigScript.new()
	var actual: String = helper.bootstrap_url("wss://arena.example.com/ws")
	return _expect_equal(
		actual,
		"https://arena.example.com/session/bootstrap",
		"bootstrap url should be derived from the signaling origin"
	)


func _assert_session_token_append() -> bool:
	var helper := WebSocketConfigScript.new()
	var actual: String = helper.append_session_token("wss://arena.example.com/ws", "abc123")
	return _expect_equal(
		actual,
		"wss://arena.example.com/ws?token=abc123",
		"session bootstrap token should be appended as a query parameter"
	)


func _assert_blank_origin_falls_back_to_local_default() -> bool:
	var helper := WebSocketConfigScript.new()
	var actual: String = helper.derive_url("", "", true)
	return _expect_equal(
		actual,
		helper.DEFAULT_LOCAL_URL,
		"blank origin should fall back to the local default"
	)


func _assert_signaling_socket_detaches_after_control_channel_open() -> bool:
	if not DevSocketClientScript.should_detach_signaling_socket(WebRTCDataChannel.STATE_OPEN, false):
		return _fail("signaling websocket closure should be tolerated once the control data channel is open")
	if not DevSocketClientScript.should_detach_signaling_socket(WebRTCDataChannel.STATE_CLOSED, true):
		return _fail("signaling websocket closure should be tolerated once the transport has emitted open")
	if DevSocketClientScript.should_detach_signaling_socket(WebRTCDataChannel.STATE_CLOSED, false):
		return _fail("signaling websocket closure should remain fatal before WebRTC setup completes")
	return true


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
					"primary_cooldown_remaining_ms": 150,
					"primary_cooldown_total_ms": 600,
					"slot_cooldown_remaining_ms": [0, 250, 0, 0, 0],
					"slot_cooldown_total_ms": [0, 900, 0, 0, 0],
					"current_cast_slot": 2,
					"current_cast_remaining_ms": 300,
					"current_cast_total_ms": 500,
				},
			],
			"projectiles": [
				{
					"owner": 11,
					"slot": 1,
					"kind": "SkillShot",
					"x": -520,
					"y": 220,
					"radius": 28,
				},
			],
		},
	})
	if not state.can_send_combat_input():
		return _fail("combat input should unlock once the local arena player is alive in combat")
	if state.can_use_combat_slot(3):
		return _fail("combat slots should remain unavailable while the player is mid-cast")
	if state.can_use_combat_slot(4):
		return _fail("locked combat slots should stay unavailable")
	if state.can_use_primary_attack():
		return _fail("primary attack should be unavailable while its cooldown is active")
	if int(state.local_arena_player().get("x", 0)) != -640:
		return _fail("arena snapshots should update the local player's position")
	if state.arena_projectiles_list().size() != 1:
		return _fail("arena snapshots should retain projectile state")
	var cooldown_text := state.cooldown_summary_text()
	if not cooldown_text.contains("Casting"):
		return _fail("cooldown summary should include an active cast banner")
	if not cooldown_text.contains("Melee"):
		return _fail("cooldown summary should include the primary attack label")
	if cooldown_text.contains("Melee ready"):
		return _fail("cooldown summary should show the primary cooldown")

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


func _assert_aura_deployables_stay_hidden_from_client_render_state() -> bool:
	var state := ClientStateScript.new()
	state.local_player_id = 11
	state.apply_server_event({
		"type": "ArenaStateSnapshot",
		"snapshot": {
			"width": 1800,
			"height": 1200,
			"deployables": [
				{
					"id": 1,
					"owner": 11,
					"team": "Team A",
					"kind": "Aura",
					"x": 0,
					"y": 0,
					"radius": 120,
					"hit_points": 1,
					"max_hit_points": 1,
					"remaining_ms": 2000,
				},
				{
					"id": 2,
					"owner": 11,
					"team": "Team A",
					"kind": "Ward",
					"x": 60,
					"y": 0,
					"radius": 60,
					"hit_points": 20,
					"max_hit_points": 20,
					"remaining_ms": 2000,
				},
			],
			"players": [],
			"projectiles": [],
		},
	})
	var deployables := state.arena_deployables_list()
	if deployables.size() != 1:
		return _fail("client render state should omit aura deployables entirely")
	if String(deployables[0].get("kind", "")) != "Ward":
		return _fail("non-aura deployables should still remain visible to the client")
	return true


func _assert_local_skill_labels_and_render_smoothing() -> bool:
	var state := ClientStateScript.new()
	state.mark_transport_state("open")
	state.local_player_id = 11
	state.local_player_name = "Alice"
	state.apply_server_event({
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
		],
	})
	state.apply_server_event({
		"type": "MatchStarted",
		"match_id": 8,
		"round": 3,
		"skill_pick_seconds": 25,
	})
	state.apply_server_event({
		"type": "SkillChosen",
		"player_id": 11,
		"tree": "Mage",
		"tier": 1,
	})
	state.local_round_skill_locked = false
	state.apply_server_event({
		"type": "RoundWon",
		"round": 3,
		"winning_team": "Team A",
		"score_a": 1,
		"score_b": 0,
	})
	state.apply_server_event({
		"type": "SkillChosen",
		"player_id": 11,
		"tree": "Mage",
		"tier": 2,
	})
	state.apply_server_event({
		"type": "CombatStarted",
	})
	state.apply_server_event({
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
					"current_cast_slot": 2,
					"current_cast_remaining_ms": 220,
					"current_cast_total_ms": 350,
				},
			],
			"projectiles": [],
		},
	})
	var cooldown_text := state.cooldown_summary_text()
	if not cooldown_text.contains("Magic Missile"):
		return _fail("cooldown summary should include the local player's first chosen skill name")
	if not cooldown_text.contains("Ice Lance"):
		return _fail("cooldown summary should include the local player's second chosen skill name")

	state.apply_server_event({
		"type": "ArenaDeltaSnapshot",
		"snapshot": {
			"players": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"x": -420,
					"y": 220,
					"aim_x": 180,
					"aim_y": 30,
					"hit_points": 94,
					"max_hit_points": 100,
					"mana": 72,
					"max_mana": 100,
					"alive": true,
					"unlocked_skill_slots": 2,
					"primary_cooldown_remaining_ms": 0,
					"primary_cooldown_total_ms": 600,
					"slot_cooldown_remaining_ms": [120, 0, 0, 0, 0],
					"slot_cooldown_total_ms": [800, 900, 0, 0, 0],
					"current_cast_slot": 2,
					"current_cast_remaining_ms": 120,
					"current_cast_total_ms": 350,
					"active_statuses": [],
				},
			],
			"projectiles": [],
		},
	})
	if int(state.local_arena_player().get("x", 0)) != -420:
		return _fail("authoritative local arena state should snap to the latest x position")
	var render_players := state.arena_players_list()
	if render_players.is_empty():
		return _fail("arena render list should retain players for drawing")
	if int(render_players[0].get("x", 0)) != -640:
		return _fail("rendered arena players should preserve the previous position until smoothing runs")

	state.advance_visuals(0.03)
	var smoothed_x := float(state.arena_players_list()[0].get("x", -640))
	if smoothed_x <= -640.0 or smoothed_x >= -420.0:
		return _fail("render smoothing should move player positions toward the new authoritative location")
	return true


func _assert_round_and_match_summaries_and_combat_text() -> bool:
	var state := ClientStateScript.new()
	state.mark_transport_state("open")
	state.local_player_id = 11
	state.local_player_name = "Alice"
	state.apply_server_event({
		"type": "RoundSummary",
		"summary": {
			"round": 2,
			"round_totals": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"damage_done": 120,
					"healing_to_allies": 35,
					"healing_to_enemies": 0,
					"cc_used": 3,
					"cc_hits": 2,
				},
			],
			"running_totals": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"damage_done": 240,
					"healing_to_allies": 50,
					"healing_to_enemies": 4,
					"cc_used": 5,
					"cc_hits": 3,
				},
			],
		},
	})
	var round_text := state.round_summary_text()
	if not round_text.contains("Round 2"):
		return _fail("round summaries should include the round number")
	if not round_text.contains("Running total"):
		return _fail("round summaries should include the running-total section")
	if not round_text.contains("Alice  |  Team A  |  dmg 240"):
		return _fail("round summaries should include formatted running totals for each player")

	state.apply_server_event({
		"type": "MatchSummary",
		"summary": {
			"rounds_played": 3,
			"totals": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"damage_done": 410,
					"healing_to_allies": 88,
					"healing_to_enemies": 9,
					"cc_used": 6,
					"cc_hits": 4,
				},
			],
		},
	})
	var match_text := state.match_summary_text()
	if not match_text.contains("Rounds played: 3"):
		return _fail("match summaries should include the total rounds played")
	if not match_text.contains("heal+ 88"):
		return _fail("match summaries should include ally-healing totals")

	state.apply_server_event({
		"type": "ArenaCombatTextBatch",
		"entries": [
			{
				"x": -120,
				"y": 45,
				"text": "-42",
				"style": "DamageIncoming",
			},
		],
	})
	var entries := state.local_combat_text_entries()
	if entries.size() != 1:
		return _fail("combat text batches should queue local-only scrolling text entries")
	if String(entries[0].get("style", "")) != "DamageIncoming":
		return _fail("combat text entries should preserve their authored style")

	state.advance_visuals(1.3)
	if not state.local_combat_text_entries().is_empty():
		return _fail("local combat text should expire after its configured lifetime")
	return true


func _assert_player_token_palette_and_team_rings() -> bool:
	var state := ClientStateScript.new()
	state.local_player_id = 11
	state.local_player_name = "Alice"
	state.apply_server_event({
		"type": "ArenaStateSnapshot",
		"snapshot": {
			"width": 1800,
			"height": 1200,
			"players": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"x": 0,
					"y": 0,
					"aim_x": 0,
					"aim_y": 0,
					"hit_points": 100,
					"max_hit_points": 100,
					"mana": 100,
					"max_mana": 100,
					"alive": true,
					"unlocked_skill_slots": 3,
					"equipped_skill_trees": ["Mage", "Rogue", "Bard", null, null],
					"active_statuses": [
						{
							"kind": "Shield",
							"remaining_duration_ms": 800,
						},
						{
							"kind": "Silence",
							"remaining_duration_ms": 500,
						},
					],
				},
			],
			"projectiles": [],
		},
	})
	var arena_view := ArenaViewScript.new()
	arena_view.set_client_state(state)

	if arena_view._skill_tree_color("Mage", true).to_html() != Color8(105, 204, 240).to_html():
		return _fail("mage rings should use the WoW mage color")
	if arena_view._skill_tree_color("Bard", true).to_html() != Color8(140, 59, 255).to_html():
		return _fail("bard rings should use the reserved Glasbey palette color")
	if arena_view._player_team_border_color("Team A", true).to_html() != Color8(27, 58, 128).to_html():
		return _fail("friendly team borders should render dark blue")
	if arena_view._player_team_border_color("Team B", true).to_html() != Color8(196, 61, 50).to_html():
		return _fail("enemy team borders should render red")
	if not arena_view._is_positive_status("Shield"):
		return _fail("shield should render on the positive status halo")
	if arena_view._is_positive_status("Silence"):
		return _fail("silence should render on the negative status halo")
	if arena_view._status_color("Shield", true).to_html() != Color8(147, 202, 255).to_html():
		return _fail("shield should use the expected positive halo color")
	if arena_view._status_color("Silence", true).to_html() != Color8(129, 116, 209).to_html():
		return _fail("silence should use the expected negative halo color")

	arena_view.free()
	return true


func _assert_resource_labels_only_show_for_local_player() -> bool:
	var state := ClientStateScript.new()
	state.local_player_id = 11
	var arena_view = ArenaViewScript.new()
	arena_view.set_client_state(state)

	var local_label := arena_view._player_resource_label({
		"player_id": 11,
		"hit_points": 93,
		"mana": 41,
	})
	if local_label != "HP 93  Mana 41":
		return _fail("arena view should expose numeric health and mana for the local player only")

	var remote_label := arena_view._player_resource_label({
		"player_id": 22,
		"hit_points": 88,
		"mana": 77,
	})
	arena_view.free()
	if remote_label != "":
		return _fail("arena view should not expose numeric health and mana labels for remote players")
	return true


func _assert_remote_cast_labels_use_known_roster_skills() -> bool:
	var state := ClientStateScript.new()
	state.local_player_id = 11
	state.apply_server_event({
		"type": "JoinedCentralLobby",
		"player_id": 22,
		"player_name": "Other",
	})
	state.apply_server_event({
		"type": "SkillChosen",
		"player_id": 22,
		"tree": "Mage",
		"tier": 2,
	})
	var arena_view = ArenaViewScript.new()
	arena_view.set_client_state(state)
	var expected_label := state.skill_name_for("Mage", 2)
	var label := arena_view._cast_label_for_player({
		"player_id": 22,
		"current_cast_slot": 2,
	})
	arena_view.free()
	if label != expected_label:
		return _fail("remote cast bars should use known roster skill names when available")
	return true


func _assert_fog_rounds_only_on_visibility_boundary() -> bool:
	var state := ClientStateScript.new()
	state.arena_width = 300
	state.arena_height = 300
	state.arena_tile_units = 60
	state.visible_tiles = _mask_with_visible_tiles(5, 5, [Vector2i(2, 2)])
	var arena_view = ArenaViewScript.new()
	arena_view.set_client_state(state)
	if not arena_view._has_fog_edge(2, 1, Vector2i.DOWN):
		return _fail("fog tiles adjacent to visible tiles should render soft edge transitions")
	if arena_view._has_fog_edge(0, 0, Vector2i.RIGHT):
		return _fail("fog tiles away from visibility edges should remain solid")
	if arena_view._has_fog_edge(2, 2, Vector2i.DOWN):
		return _fail("visible tiles should not be treated as fog edges")
	arena_view.free()
	return true


func _expect_equal(actual: String, expected: String, context: String) -> bool:
	if actual != expected:
		return _fail("%s: expected \"%s\" but received \"%s\"" % [context, expected, actual])
	return true


func _fail(message: String) -> bool:
	printerr(message)
	return false


func _mask_with_visible_tiles(tile_width: int, tile_height: int, visible_tiles: Array[Vector2i]) -> PackedByteArray:
	var mask := PackedByteArray()
	var bit_count := tile_width * tile_height
	mask.resize(int(ceili(float(bit_count) / 8.0)))
	for tile in visible_tiles:
		var index := tile.y * tile_width + tile.x
		var byte_index := int(index / 8)
		var bit_index := int(index % 8)
		mask[byte_index] = int(mask[byte_index]) | (1 << bit_index)
	return mask
