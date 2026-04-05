extends SceneTree

const Protocol := preload("res://scripts/net/protocol.gd")


func _init() -> void:
	var success := true
	success = _assert_valid_connect_command() and success
	success = _assert_connect_rejects_empty_name() and success
	success = _assert_valid_primary_input() and success
	success = _assert_move_axis_rejection() and success
	success = _assert_missing_cast_context_rejection() and success
	success = _assert_self_cast_requires_cast_rejection() and success
	success = _assert_unexpected_context_rejection() and success
	success = _assert_aim_range_rejection() and success
	success = _assert_start_training_uses_kind_five() and success
	success = _assert_choose_skill_uses_tree_strings() and success
	success = _assert_reset_training_uses_kind_nine() and success
	success = _assert_decode_connected_with_skill_catalog() and success
	success = _assert_decode_training_started() and success
	success = _assert_decode_arena_state_snapshot() and success
	success = _assert_decode_arena_delta_snapshot() and success
	success = _assert_decode_training_state_snapshot() and success
	success = _assert_decode_arena_effect_batch() and success
	success = _assert_decode_round_summary() and success
	success = _assert_decode_arena_combat_text_batch() and success
	quit(0 if success else 1)


func _assert_valid_connect_command() -> bool:
	var encoded := Protocol.encode_client_command("Connect", {
		"player_name": "Alice",
	}, 1, 0)
	if not bool(encoded.get("ok", false)):
		return _fail("valid connect command should encode")

	var decoded := Protocol.decode_packet(encoded.get("packet", PackedByteArray()))
	if not bool(decoded.get("ok", false)):
		return _fail("encoded connect command should decode as a packet")

	var payload: PackedByteArray = decoded.get("payload", PackedByteArray())
	if payload.size() != 7:
		return _fail("encoded connect payload should contain kind + len + name bytes")
	if int(payload[0]) != 1:
		return _fail("encoded connect command should use kind 1")
	if int(payload[1]) != 5:
		return _fail("encoded connect command should prefix the player name length")
	return true


func _assert_connect_rejects_empty_name() -> bool:
	var encoded := Protocol.encode_client_command("Connect", {
		"player_name": "",
	}, 1, 0)
	return _expect_error(encoded, "player name must not be empty")


func _assert_valid_primary_input() -> bool:
	var encoded := Protocol.encode_input_frame({
		"client_input_tick": 9,
		"move_horizontal_q": 0,
		"move_vertical_q": 0,
		"aim_horizontal_q": 0,
		"aim_vertical_q": 0,
		"primary": true,
	}, 7, 11)
	if not bool(encoded.get("ok", false)):
		return _fail("valid primary attack frame should encode")

	var decoded := Protocol.decode_packet(encoded.get("packet", PackedByteArray()))
	if not bool(decoded.get("ok", false)):
		return _fail("encoded input frame should decode as a packet")

	var header: Dictionary = decoded.get("header", {})
	if int(header.get("channel_id", -1)) != Protocol.CHANNEL_INPUT:
		return _fail("encoded input frame should use the input channel")
	if int(header.get("packet_kind", -1)) != Protocol.PACKET_KIND_INPUT_FRAME:
		return _fail("encoded input frame should use the input packet kind")

	var payload: PackedByteArray = decoded.get("payload", PackedByteArray())
	if payload.size() != 16:
		return _fail("encoded input frame payload should be 16 bytes")
	if int(payload[12]) != 1 or int(payload[13]) != 0:
		return _fail("encoded input frame should set only the primary button bit")
	return true


func _assert_move_axis_rejection() -> bool:
	var encoded := Protocol.encode_input_frame({
		"client_input_tick": 1,
		"move_horizontal_q": 2,
		"move_vertical_q": 0,
	}, 1, 0)
	return _expect_error(encoded, "move_horizontal_q=2 is outside the allowed range -1..=1")


func _assert_missing_cast_context_rejection() -> bool:
	var encoded := Protocol.encode_input_frame({
		"client_input_tick": 1,
		"cast": true,
	}, 1, 0)
	return _expect_error(encoded, "cast input requires a non-zero ability_or_context")


func _assert_self_cast_requires_cast_rejection() -> bool:
	var encoded := Protocol.encode_input_frame({
		"client_input_tick": 1,
		"self_cast": true,
	}, 1, 0)
	return _expect_error(encoded, "self-cast requires cast to be requested")


func _assert_unexpected_context_rejection() -> bool:
	var encoded := Protocol.encode_input_frame({
		"client_input_tick": 1,
		"ability_or_context": 4,
	}, 1, 0)
	return _expect_error(encoded, "non-cast input must not provide ability_or_context")


func _assert_aim_range_rejection() -> bool:
	var encoded := Protocol.encode_input_frame({
		"client_input_tick": 1,
		"aim_horizontal_q": 40000,
	}, 1, 0)
	return _expect_error(encoded, "aim_horizontal_q=40000 is outside the allowed range -32768..=32767")


func _assert_start_training_uses_kind_five() -> bool:
	var encoded := Protocol.encode_client_command("StartTraining", {}, 5, 0)
	if not bool(encoded.get("ok", false)):
		return _fail("start-training command should encode")
	var decoded := Protocol.decode_packet(encoded.get("packet", PackedByteArray()))
	if not bool(decoded.get("ok", false)):
		return _fail("start-training packet should decode")
	var payload: PackedByteArray = decoded.get("payload", PackedByteArray())
	if payload.size() != 1 or int(payload[0]) != 5:
		return _fail("start-training command should use kind 5 with no trailing payload")
	return true


func _assert_choose_skill_uses_tree_strings() -> bool:
	var encoded := Protocol.encode_client_command("ChooseSkill", {
		"tree": "Druid",
		"tier": 1,
	}, 5, 0)
	if not bool(encoded.get("ok", false)):
		return _fail("custom class choose-skill command should encode")
	var decoded := Protocol.decode_packet(encoded.get("packet", PackedByteArray()))
	if not bool(decoded.get("ok", false)):
		return _fail("custom class choose-skill packet should decode")
	var payload: PackedByteArray = decoded.get("payload", PackedByteArray())
	if payload.size() < 4:
		return _fail("choose-skill payload should include kind, tree string, and tier")
	if int(payload[0]) != 8:
		return _fail("choose-skill command should use kind 8")
	if int(payload[1]) != 5:
		return _fail("choose-skill command should prefix the tree string length")
	return true


func _assert_reset_training_uses_kind_nine() -> bool:
	var encoded := Protocol.encode_client_command("ResetTrainingSession", {}, 6, 0)
	if not bool(encoded.get("ok", false)):
		return _fail("reset-training command should encode")
	var decoded := Protocol.decode_packet(encoded.get("packet", PackedByteArray()))
	if not bool(decoded.get("ok", false)):
		return _fail("reset-training packet should decode")
	var payload: PackedByteArray = decoded.get("payload", PackedByteArray())
	if payload.size() != 1 or int(payload[0]) != 9:
		return _fail("reset-training command should use kind 9 with no trailing payload")
	return true


func _assert_decode_connected_with_skill_catalog() -> bool:
	var payload := PackedByteArray([1])
	_push_u32(payload, 11)
	_push_string(payload, "Alice")
	_push_u16(payload, 1)
	_push_u16(payload, 2)
	_push_u16(payload, 3)
	_push_u16(payload, 4)
	_push_u16(payload, 5)
	_push_u32(payload, 600)
	_push_u32(payload, 120)
	_push_u32(payload, 30000)
	_push_u16(payload, 7)
	_push_u16(payload, 6)
	_push_u16(payload, 1)
	_push_string(payload, "mage_t1_missile")
	_push_u16(payload, 9)
	_push_u16(payload, 2)
	_push_string(payload, "Mage")
	payload.append(1)
	_push_string(payload, "mage_t1_missile")
	_push_string(payload, "Magic Missile")
	_push_string(payload, "Fast projectile damage.")
	_push_string(payload, "CD 0.7s | Cast instant | Mana 16\nProjectile: range 1500, radius 16, speed 310\nEffect: 10 damage")
	_push_string(payload, "damage")
	_push_string(payload, "mage_arc_bolt")
	_push_string(payload, "Cleric")
	payload.append(1)
	_push_string(payload, "cleric_t1_minor_heal")
	_push_string(payload, "Minor Heal")
	_push_string(payload, "Beam heal for the first ally hit.")
	_push_string(payload, "CD 0.9s | Cast 0.3s | Mana 16\nBeam: range 280, radius 28\nEffect: 18 heal")
	_push_string(payload, "heal")
	_push_string(payload, "")
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 7, 0))
	if not bool(decoded.get("ok", false)):
		return _fail("connected event with skill catalog should decode")
	var event: Dictionary = decoded.get("event", {})
	var record: Dictionary = event.get("record", {})
	var skill_catalog: Array = event.get("skill_catalog", [])
	if String(event.get("type", "")) != "Connected":
		return _fail("connected payload should decode as a Connected event")
	if int(record.get("round_wins", 0)) != 4 or int(record.get("round_losses", 0)) != 5:
		return _fail("connected event should decode extended round record fields")
	if int(record.get("total_damage_done", 0)) != 600 or int(record.get("cc_hits", 0)) != 6:
		return _fail("connected event should decode extended combat record fields")
	if int((record.get("skill_pick_counts", {}) as Dictionary).get("mage_t1_missile", 0)) != 9:
		return _fail("connected event should decode per-skill pick counters")
	if skill_catalog.size() != 2:
		return _fail("connected event should decode the skill catalog entries")
	if String(skill_catalog[0].get("skill_name", "")) != "Magic Missile":
		return _fail("connected event should preserve catalog skill names")
	if String(skill_catalog[0].get("skill_description", "")) != "Fast projectile damage.":
		return _fail("connected event should decode skill descriptions")
	if String(skill_catalog[1].get("ui_category", "")) != "heal":
		return _fail("connected event should decode skill UI categories")
	if String(skill_catalog[0].get("audio_cue_id", "")) != "mage_arc_bolt":
		return _fail("connected event should decode skill audio cue ids")
	return true


func _assert_decode_training_started() -> bool:
	var payload := PackedByteArray([25])
	_push_u32(payload, 19)
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 8, 0))
	if not bool(decoded.get("ok", false)):
		return _fail("training-started event should decode")
	var event: Dictionary = decoded.get("event", {})
	if String(event.get("type", "")) != "TrainingStarted":
		return _fail("training-started payload should decode as a TrainingStarted event")
	if int(event.get("training_id", 0)) != 19:
		return _fail("training-started payload should preserve the training id")
	return true


func _assert_decode_arena_state_snapshot() -> bool:
	var payload := PackedByteArray([19])
	payload.append(1)
	payload.append(3)
	payload.append(0)
	_push_u16(payload, 1800)
	_push_u16(payload, 1200)
	_push_u16(payload, 50)
	_push_blob(payload, PackedByteArray([0x7F, 0x03]))
	_push_blob(payload, PackedByteArray([0x0C, 0x03]))
	_push_blob(payload, PackedByteArray([0x3F, 0x03]))
	_push_blob(payload, PackedByteArray([0xFF, 0x0F]))
	_push_u32(payload, 180000)
	_push_u32(payload, 42000)
	_push_u32(payload, 37500)
	_push_u16(payload, 1)
	payload.append(1)
	_push_i16(payload, -220)
	_push_i16(payload, -150)
	_push_u16(payload, 70)
	_push_u16(payload, 70)
	_push_u16(payload, 0)
	_push_u16(payload, 1)
	_push_u32(payload, 11)
	_push_string(payload, "Alice")
	payload.append(Protocol.TEAM_A)
	_push_i16(payload, -640)
	_push_i16(payload, 220)
	_push_i16(payload, 120)
	_push_i16(payload, 0)
	_push_u16(payload, 100)
	_push_u16(payload, 100)
	_push_u16(payload, 72)
	_push_u16(payload, 100)
	payload.append(1)
	payload.append(3)
	_push_u16(payload, 180)
	_push_u16(payload, 600)
	for value in [0, 400, 0, 1200, 0]:
		_push_u16(payload, value)
	for value in [0, 900, 0, 1400, 0]:
		_push_u16(payload, value)
	payload.append(1)
	_push_string(payload, "Mage")
	payload.append(1)
	_push_string(payload, "Cleric")
	payload.append(1)
	_push_string(payload, "Rogue")
	payload.append(0)
	payload.append(0)
	payload.append(1)
	payload.append(2)
	_push_u16(payload, 400)
	_push_u16(payload, 600)
	payload.append(1)
	_push_u32(payload, 12)
	payload.append(2)
	payload.append(1)
	payload.append(2)
	_push_u16(payload, 1800)
	_push_u16(payload, 1)
	_push_u32(payload, 11)
	payload.append(1)
	payload.append(2)
	_push_i16(payload, -520)
	_push_i16(payload, 220)
	_push_u16(payload, 28)
	payload.append(0)
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 8, 21))
	if not bool(decoded.get("ok", false)):
		return _fail("arena state snapshot should decode")
	var event: Dictionary = decoded.get("event", {})
	var snapshot: Dictionary = event.get("snapshot", {})
	var players: Array = snapshot.get("players", [])
	var projectiles: Array = snapshot.get("projectiles", [])
	var deployables: Array = snapshot.get("deployables", [])
	if String(event.get("type", "")) != "ArenaStateSnapshot":
		return _fail("arena state snapshot should use the ArenaStateSnapshot event type")
	if players.size() != 1:
		return _fail("arena state snapshot should decode one player")
	if String(snapshot.get("mode", "")) != "Match":
		return _fail("arena state snapshot should decode the arena session mode")
	if String(snapshot.get("phase", "")) != "Combat":
		return _fail("arena state snapshot should decode the arena phase")
	if int(snapshot.get("tile_units", 0)) != 50:
		return _fail("arena state snapshot should decode tile units")
	var footprint_tiles: PackedByteArray = snapshot.get("footprint_tiles", PackedByteArray())
	if footprint_tiles.size() != 2 or int(footprint_tiles[0]) != 0x7F:
		return _fail("arena state snapshot should decode footprint tile masks")
	var objective_tiles: PackedByteArray = snapshot.get("objective_tiles", PackedByteArray())
	if objective_tiles.size() != 2 or int(objective_tiles[0]) != 0x0C:
		return _fail("arena state snapshot should decode objective tile masks")
	var visible_tiles: PackedByteArray = snapshot.get("visible_tiles", PackedByteArray())
	if visible_tiles.size() != 2 or int(visible_tiles[0]) != 0x3F:
		return _fail("arena state snapshot should decode visible tile masks")
	if int(snapshot.get("objective_target_ms", 0)) != 180000:
		return _fail("arena state snapshot should decode the objective timer target")
	if int(snapshot.get("objective_team_a_ms", 0)) != 42000 or int(snapshot.get("objective_team_b_ms", 0)) != 37500:
		return _fail("arena state snapshot should decode team objective timers")
	if deployables.size() != 0:
		return _fail("arena state snapshot should decode deployable arrays")
	if int(players[0].get("unlocked_skill_slots", 0)) != 3:
		return _fail("arena state snapshot should preserve unlocked combat slots")
	if int(players[0].get("mana", 0)) != 72:
		return _fail("arena state snapshot should decode mana state")
	var statuses: Array = players[0].get("active_statuses", [])
	if statuses.size() != 1 or String(statuses[0].get("kind", "")) != "Poison":
		return _fail("arena state snapshot should decode active statuses")
	if int(players[0].get("primary_cooldown_remaining_ms", 0)) != 180:
		return _fail("arena state snapshot should decode primary cooldown state")
	var equipped_trees: Array = players[0].get("equipped_skill_trees", [])
	if equipped_trees.size() != 5 or String(equipped_trees[0]) != "Mage" or String(equipped_trees[1]) != "Cleric":
		return _fail("arena state snapshot should decode equipped skill trees")
	if int(players[0].get("current_cast_slot", 0)) != 2:
		return _fail("arena state snapshot should decode active cast slots")
	if int(players[0].get("current_cast_remaining_ms", 0)) != 400:
		return _fail("arena state snapshot should decode cast remaining state")
	if projectiles.size() != 1 or String(projectiles[0].get("kind", "")) != "SkillShot":
		return _fail("arena state snapshot should decode projectile state")
	if not Dictionary(snapshot.get("training_metrics", {})).is_empty():
		return _fail("match arena snapshots should decode without training metrics")
	return true


func _assert_decode_arena_delta_snapshot() -> bool:
	var payload := PackedByteArray([20])
	payload.append(1)
	payload.append(3)
	payload.append(0)
	_push_u16(payload, 50)
	_push_blob(payload, PackedByteArray([0x7F, 0x03]))
	_push_blob(payload, PackedByteArray([0x0C, 0x03]))
	_push_blob(payload, PackedByteArray([0x3F, 0x03]))
	_push_blob(payload, PackedByteArray([0xFF, 0x0F]))
	_push_u32(payload, 180000)
	_push_u32(payload, 45250)
	_push_u32(payload, 38900)
	_push_u16(payload, 1)
	payload.append(2)
	_push_i16(payload, -220)
	_push_i16(payload, -150)
	_push_u16(payload, 92)
	_push_u16(payload, 92)
	_push_u16(payload, 0)
	_push_u16(payload, 1)
	_push_u32(payload, 11)
	_push_string(payload, "Alice")
	payload.append(Protocol.TEAM_A)
	_push_i16(payload, -620)
	_push_i16(payload, 220)
	_push_i16(payload, 96)
	_push_i16(payload, -24)
	_push_u16(payload, 91)
	_push_u16(payload, 100)
	_push_u16(payload, 64)
	_push_u16(payload, 100)
	payload.append(1)
	payload.append(3)
	_push_u16(payload, 0)
	_push_u16(payload, 600)
	for value in [100, 0, 700, 0, 0]:
		_push_u16(payload, value)
	for value in [700, 1700, 2200, 0, 0]:
		_push_u16(payload, value)
	payload.append(1)
	_push_string(payload, "Mage")
	payload.append(1)
	_push_string(payload, "Cleric")
	payload.append(1)
	_push_string(payload, "Rogue")
	payload.append(0)
	payload.append(0)
	payload.append(0)
	_push_u16(payload, 0)
	_push_u16(payload, 0)
	payload.append(1)
	_push_u32(payload, 18)
	payload.append(3)
	payload.append(3)
	payload.append(1)
	_push_u16(payload, 1200)
	_push_u16(payload, 0)
	payload.append(0)
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 9, 22))
	if not bool(decoded.get("ok", false)):
		return _fail("arena delta snapshot should decode")
	var event: Dictionary = decoded.get("event", {})
	if String(event.get("type", "")) != "ArenaDeltaSnapshot":
		return _fail("arena delta snapshot should use the ArenaDeltaSnapshot event type")
	var snapshot: Dictionary = event.get("snapshot", {})
	var players: Array = snapshot.get("players", [])
	if String(snapshot.get("mode", "")) != "Match":
		return _fail("arena delta snapshot should preserve the arena session mode")
	if String(snapshot.get("phase", "")) != "Combat":
		return _fail("arena delta snapshot should preserve phase")
	if int(snapshot.get("tile_units", 0)) != 50:
		return _fail("arena delta snapshot should preserve tile units")
	var footprint_tiles: PackedByteArray = snapshot.get("footprint_tiles", PackedByteArray())
	if footprint_tiles.size() != 2 or int(footprint_tiles[0]) != 0x7F:
		return _fail("arena delta snapshot should decode footprint tiles")
	var objective_tiles: PackedByteArray = snapshot.get("objective_tiles", PackedByteArray())
	if objective_tiles.size() != 2 or int(objective_tiles[0]) != 0x0C:
		return _fail("arena delta snapshot should decode objective tiles")
	if int(snapshot.get("objective_target_ms", 0)) != 180000:
		return _fail("arena delta snapshot should preserve the objective target")
	if int(snapshot.get("objective_team_a_ms", 0)) != 45250 or int(snapshot.get("objective_team_b_ms", 0)) != 38900:
		return _fail("arena delta snapshot should decode team objective timers")
	if players.size() != 1 or int(players[0].get("mana", 0)) != 64:
		return _fail("arena delta snapshot should decode player state")
	var equipped_trees: Array = players[0].get("equipped_skill_trees", [])
	if equipped_trees.size() != 5 or String(equipped_trees[2]) != "Rogue":
		return _fail("arena delta snapshot should preserve equipped skill tree history")
	if int(players[0].get("current_cast_slot", 0)) != 0:
		return _fail("arena delta snapshot should decode the absence of an active cast")
	if not Dictionary(snapshot.get("training_metrics", {})).is_empty():
		return _fail("match arena delta snapshots should decode without training metrics")
	return true


func _assert_decode_training_state_snapshot() -> bool:
	var payload := PackedByteArray([19])
	payload.append(2)
	payload.append(3)
	payload.append(0)
	_push_u16(payload, 900)
	_push_u16(payload, 700)
	_push_u16(payload, 50)
	_push_blob(payload, PackedByteArray([0x1F, 0x01]))
	_push_blob(payload, PackedByteArray([0x00, 0x00]))
	_push_blob(payload, PackedByteArray([0x1F, 0x01]))
	_push_blob(payload, PackedByteArray([0x1F, 0x01]))
	_push_u32(payload, 180000)
	_push_u32(payload, 0)
	_push_u32(payload, 0)
	_push_u16(payload, 0)
	_push_u16(payload, 2)
	_push_u32(payload, 91)
	_push_u32(payload, 11)
	payload.append(Protocol.TEAM_A)
	payload.append(6)
	_push_i16(payload, -120)
	_push_i16(payload, 40)
	_push_u16(payload, 28)
	_push_u16(payload, 10000)
	_push_u16(payload, 10000)
	_push_u16(payload, 0)
	_push_u32(payload, 92)
	_push_u32(payload, 11)
	payload.append(Protocol.TEAM_A)
	payload.append(7)
	_push_i16(payload, 120)
	_push_i16(payload, 40)
	_push_u16(payload, 28)
	_push_u16(payload, 500)
	_push_u16(payload, 10000)
	_push_u16(payload, 0)
	_push_u16(payload, 1)
	_push_u32(payload, 11)
	_push_string(payload, "Alice")
	payload.append(Protocol.TEAM_A)
	_push_i16(payload, -320)
	_push_i16(payload, 0)
	_push_i16(payload, 160)
	_push_i16(payload, 0)
	_push_u16(payload, 100)
	_push_u16(payload, 100)
	_push_u16(payload, 100)
	_push_u16(payload, 100)
	payload.append(1)
	payload.append(5)
	_push_u16(payload, 0)
	_push_u16(payload, 600)
	for value in [0, 0, 0, 0, 0]:
		_push_u16(payload, value)
	for value in [0, 0, 0, 0, 0]:
		_push_u16(payload, value)
	payload.append(1)
	_push_string(payload, "Warrior")
	payload.append(0)
	payload.append(0)
	payload.append(1)
	_push_string(payload, "Ranger")
	payload.append(0)
	payload.append(0)
	_push_u16(payload, 0)
	_push_u16(payload, 0)
	payload.append(0)
	_push_u16(payload, 0)
	_push_training_metrics(payload, {
		"damage_done": 420,
		"healing_done": 150,
		"elapsed_ms": 5000,
	})
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 12, 40))
	if not bool(decoded.get("ok", false)):
		return _fail("training state snapshot should decode")
	var event: Dictionary = decoded.get("event", {})
	var snapshot: Dictionary = event.get("snapshot", {})
	var deployables: Array = snapshot.get("deployables", [])
	var training_metrics: Dictionary = snapshot.get("training_metrics", {})
	if String(event.get("type", "")) != "ArenaStateSnapshot":
		return _fail("training state payload should decode through the arena snapshot event")
	if String(snapshot.get("mode", "")) != "Training":
		return _fail("training state snapshot should preserve training mode")
	if int(snapshot.get("objective_target_ms", 0)) != 180000:
		return _fail("training state snapshot should decode the shared objective target")
	if deployables.size() != 2:
		return _fail("training state snapshot should decode dummy deployables")
	if String(deployables[0].get("kind", "")) != "TrainingDummyResetFull":
		return _fail("training state snapshot should decode the reset-full dummy kind")
	if String(deployables[1].get("kind", "")) != "TrainingDummyExecute":
		return _fail("training state snapshot should decode the execute dummy kind")
	if int(training_metrics.get("damage_done", 0)) != 420 or int(training_metrics.get("healing_done", 0)) != 150:
		return _fail("training state snapshot should decode training metrics")
	if int(training_metrics.get("elapsed_ms", 0)) != 5000:
		return _fail("training state snapshot should decode elapsed training time")
	return true


func _assert_decode_arena_effect_batch() -> bool:
	var payload := PackedByteArray([21])
	_push_u16(payload, 1)
	payload.append(2)
	_push_u32(payload, 11)
	payload.append(1)
	_push_i16(payload, -640)
	_push_i16(payload, 220)
	_push_i16(payload, 640)
	_push_i16(payload, 220)
	_push_u16(payload, 28)
	_push_string(payload, "mage_t1_missile")
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 9, 21))
	if not bool(decoded.get("ok", false)):
		return _fail("arena effect batch should decode")
	var event: Dictionary = decoded.get("event", {})
	var effects: Array = event.get("effects", [])
	if String(event.get("type", "")) != "ArenaEffectBatch":
		return _fail("arena effect batch should use the ArenaEffectBatch event type")
	if effects.size() != 1 or String(effects[0].get("kind", "")) != "SkillShot":
		return _fail("arena effect batch should preserve the effect kind")
	if String(effects[0].get("audio_cue_id", "")) != "mage_t1_missile":
		return _fail("arena effect batch should decode effect audio cue ids")
	return true


func _assert_decode_round_summary() -> bool:
	var payload := PackedByteArray([22])
	payload.append(2)
	_push_u16(payload, 2)
	_push_u32(payload, 11)
	_push_string(payload, "Alice")
	payload.append(Protocol.TEAM_A)
	_push_u32(payload, 420)
	_push_u32(payload, 120)
	_push_u32(payload, 0)
	_push_u16(payload, 3)
	_push_u16(payload, 2)
	_push_u32(payload, 12)
	_push_string(payload, "Bob")
	payload.append(Protocol.TEAM_B)
	_push_u32(payload, 315)
	_push_u32(payload, 24)
	_push_u32(payload, 18)
	_push_u16(payload, 1)
	_push_u16(payload, 1)
	_push_u16(payload, 2)
	_push_u32(payload, 11)
	_push_string(payload, "Alice")
	payload.append(Protocol.TEAM_A)
	_push_u32(payload, 840)
	_push_u32(payload, 240)
	_push_u32(payload, 0)
	_push_u16(payload, 5)
	_push_u16(payload, 3)
	_push_u32(payload, 12)
	_push_string(payload, "Bob")
	payload.append(Protocol.TEAM_B)
	_push_u32(payload, 615)
	_push_u32(payload, 48)
	_push_u32(payload, 20)
	_push_u16(payload, 2)
	_push_u16(payload, 1)
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 10, 30))
	if not bool(decoded.get("ok", false)):
		return _fail("round summary should decode")
	var event: Dictionary = decoded.get("event", {})
	var summary: Dictionary = event.get("summary", {})
	var round_totals: Array = summary.get("round_totals", [])
	if String(event.get("type", "")) != "RoundSummary":
		return _fail("round summary should use the RoundSummary event type")
	if int(summary.get("round", 0)) != 2:
		return _fail("round summary should decode the round number")
	if round_totals.size() != 2 or int(round_totals[0].get("cc_hits", 0)) != 2:
		return _fail("round summary should decode combat totals")
	return true


func _assert_decode_arena_combat_text_batch() -> bool:
	var payload := PackedByteArray([24])
	_push_u16(payload, 2)
	_push_i16(payload, -220)
	_push_i16(payload, 180)
	payload.append(1)
	_push_string(payload, "82")
	_push_i16(payload, 120)
	_push_i16(payload, 64)
	payload.append(6)
	_push_string(payload, "Dispelled")
	var decoded := Protocol.decode_server_event(
		_encode_snapshot_packet(payload, Protocol.PACKET_KIND_COMBAT_TEXT_BATCH, 11, 31)
	)
	if not bool(decoded.get("ok", false)):
		return _fail("arena combat text batch should decode")
	var event: Dictionary = decoded.get("event", {})
	var entries: Array = event.get("entries", [])
	if String(event.get("type", "")) != "ArenaCombatTextBatch":
		return _fail("combat text batch should use the ArenaCombatTextBatch event type")
	if entries.size() != 2 or String(entries[1].get("style", "")) != "NegativeStatus":
		return _fail("combat text batch should decode entries and styles")
	return true


func _expect_error(result: Dictionary, expected_message: String) -> bool:
	if bool(result.get("ok", false)):
		return _fail("expected encoder to reject invalid input")
	var actual_message := String(result.get("error", ""))
	if actual_message != expected_message:
		return _fail("expected \"%s\" but received \"%s\"" % [expected_message, actual_message])
	return true


func _fail(message: String) -> bool:
	printerr(message)
	return false


func _encode_server_event_packet(payload: PackedByteArray, seq: int, sim_tick: int) -> PackedByteArray:
	var kind := int(payload[0])
	var channel_id := Protocol.CHANNEL_CONTROL
	var packet_kind := Protocol.PACKET_KIND_CONTROL_EVENT
	if kind == 19:
		channel_id = Protocol.CHANNEL_SNAPSHOT
		packet_kind = Protocol.PACKET_KIND_FULL_SNAPSHOT
	elif kind == 20:
		channel_id = Protocol.CHANNEL_SNAPSHOT
		packet_kind = Protocol.PACKET_KIND_DELTA_SNAPSHOT
	elif kind == 21:
		channel_id = Protocol.CHANNEL_SNAPSHOT
		packet_kind = Protocol.PACKET_KIND_EVENT_BATCH
	elif kind == 24:
		channel_id = Protocol.CHANNEL_SNAPSHOT
		packet_kind = Protocol.PACKET_KIND_COMBAT_TEXT_BATCH
	return _encode_snapshot_packet(payload, packet_kind, seq, sim_tick, channel_id)


func _encode_snapshot_packet(
	payload: PackedByteArray,
	packet_kind: int,
	seq: int,
	sim_tick: int,
	channel_id: int = Protocol.CHANNEL_SNAPSHOT
) -> PackedByteArray:
	var packet := PackedByteArray()
	_push_u16(packet, Protocol.PACKET_MAGIC)
	packet.append(Protocol.PROTOCOL_VERSION)
	packet.append(channel_id)
	packet.append(packet_kind)
	packet.append(0)
	_push_u16(packet, payload.size())
	_push_u32(packet, seq)
	_push_u32(packet, sim_tick)
	packet.append_array(payload)
	return packet


func _push_string(bytes: PackedByteArray, value: String) -> void:
	var utf8 := value.to_utf8_buffer()
	bytes.append(utf8.size())
	bytes.append_array(utf8)


func _push_u16(bytes: PackedByteArray, value: int) -> void:
	bytes.append(value & 0xff)
	bytes.append((value >> 8) & 0xff)


func _push_i16(bytes: PackedByteArray, value: int) -> void:
	var encoded := value & 0xffff
	bytes.append(encoded & 0xff)
	bytes.append((encoded >> 8) & 0xff)


func _push_u32(bytes: PackedByteArray, value: int) -> void:
	bytes.append(value & 0xff)
	bytes.append((value >> 8) & 0xff)
	bytes.append((value >> 16) & 0xff)
	bytes.append((value >> 24) & 0xff)


func _push_blob(bytes: PackedByteArray, value: PackedByteArray) -> void:
	_push_u16(bytes, value.size())
	bytes.append_array(value)


func _push_training_metrics(bytes: PackedByteArray, metrics: Dictionary) -> void:
	if metrics.is_empty():
		bytes.append(0)
		return
	bytes.append(1)
	_push_u32(bytes, int(metrics.get("damage_done", 0)))
	_push_u32(bytes, int(metrics.get("healing_done", 0)))
	_push_u32(bytes, int(metrics.get("elapsed_ms", 0)))
