extends SceneTree

const Protocol := preload("res://scripts/net/protocol.gd")


func _init() -> void:
	var success := true
	success = _assert_valid_connect_command() and success
	success = _assert_connect_rejects_empty_name() and success
	success = _assert_valid_primary_input() and success
	success = _assert_move_axis_rejection() and success
	success = _assert_missing_cast_context_rejection() and success
	success = _assert_unexpected_context_rejection() and success
	success = _assert_aim_range_rejection() and success
	success = _assert_choose_skill_uses_tree_strings() and success
	success = _assert_decode_connected_with_skill_catalog() and success
	success = _assert_decode_arena_state_snapshot() and success
	success = _assert_decode_arena_delta_snapshot() and success
	success = _assert_decode_arena_effect_batch() and success
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
	if int(payload[0]) != 7:
		return _fail("choose-skill command should use kind 7")
	if int(payload[1]) != 5:
		return _fail("choose-skill command should prefix the tree string length")
	return true


func _assert_decode_connected_with_skill_catalog() -> bool:
	var payload := PackedByteArray([1])
	_push_u32(payload, 11)
	_push_string(payload, "Alice")
	_push_u16(payload, 1)
	_push_u16(payload, 2)
	_push_u16(payload, 3)
	_push_u16(payload, 2)
	_push_string(payload, "Mage")
	payload.append(1)
	_push_string(payload, "mage_t1_missile")
	_push_string(payload, "Magic Missile")
	_push_string(payload, "Cleric")
	payload.append(1)
	_push_string(payload, "cleric_t1_minor_heal")
	_push_string(payload, "Minor Heal")
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 7, 0))
	if not bool(decoded.get("ok", false)):
		return _fail("connected event with skill catalog should decode")
	var event: Dictionary = decoded.get("event", {})
	var skill_catalog: Array = event.get("skill_catalog", [])
	if String(event.get("type", "")) != "Connected":
		return _fail("connected payload should decode as a Connected event")
	if skill_catalog.size() != 2:
		return _fail("connected event should decode the skill catalog entries")
	if String(skill_catalog[0].get("skill_name", "")) != "Magic Missile":
		return _fail("connected event should preserve catalog skill names")
	return true


func _assert_decode_arena_state_snapshot() -> bool:
	var payload := PackedByteArray([19])
	payload.append(3)
	payload.append(0)
	_push_u16(payload, 1800)
	_push_u16(payload, 1200)
	_push_u16(payload, 50)
	_push_u16(payload, 2)
	payload.append(0x3F)
	payload.append(0x03)
	_push_u16(payload, 2)
	payload.append(0xFF)
	payload.append(0x0F)
	_push_u16(payload, 1)
	payload.append(1)
	_push_i16(payload, -220)
	_push_i16(payload, -150)
	_push_u16(payload, 70)
	_push_u16(payload, 70)
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
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 8, 21))
	if not bool(decoded.get("ok", false)):
		return _fail("arena state snapshot should decode")
	var event: Dictionary = decoded.get("event", {})
	var snapshot: Dictionary = event.get("snapshot", {})
	var players: Array = snapshot.get("players", [])
	var projectiles: Array = snapshot.get("projectiles", [])
	if String(event.get("type", "")) != "ArenaStateSnapshot":
		return _fail("arena state snapshot should use the ArenaStateSnapshot event type")
	if players.size() != 1:
		return _fail("arena state snapshot should decode one player")
	if String(snapshot.get("phase", "")) != "Combat":
		return _fail("arena state snapshot should decode the arena phase")
	if int(snapshot.get("tile_units", 0)) != 50:
		return _fail("arena state snapshot should decode tile units")
	var visible_tiles: PackedByteArray = snapshot.get("visible_tiles", PackedByteArray())
	if visible_tiles.size() != 2 or int(visible_tiles[0]) != 0x3F:
		return _fail("arena state snapshot should decode visible tile masks")
	if int(players[0].get("unlocked_skill_slots", 0)) != 3:
		return _fail("arena state snapshot should preserve unlocked combat slots")
	if int(players[0].get("mana", 0)) != 72:
		return _fail("arena state snapshot should decode mana state")
	var statuses: Array = players[0].get("active_statuses", [])
	if statuses.size() != 1 or String(statuses[0].get("kind", "")) != "Poison":
		return _fail("arena state snapshot should decode active statuses")
	if int(players[0].get("primary_cooldown_remaining_ms", 0)) != 180:
		return _fail("arena state snapshot should decode primary cooldown state")
	if int(players[0].get("current_cast_slot", 0)) != 2:
		return _fail("arena state snapshot should decode active cast slots")
	if int(players[0].get("current_cast_remaining_ms", 0)) != 400:
		return _fail("arena state snapshot should decode cast remaining state")
	if projectiles.size() != 1 or String(projectiles[0].get("kind", "")) != "SkillShot":
		return _fail("arena state snapshot should decode projectile state")
	return true


func _assert_decode_arena_delta_snapshot() -> bool:
	var payload := PackedByteArray([20])
	payload.append(3)
	payload.append(0)
	_push_u16(payload, 50)
	_push_u16(payload, 2)
	payload.append(0x3F)
	payload.append(0x03)
	_push_u16(payload, 2)
	payload.append(0xFF)
	payload.append(0x0F)
	_push_u16(payload, 1)
	payload.append(2)
	_push_i16(payload, -220)
	_push_i16(payload, -150)
	_push_u16(payload, 92)
	_push_u16(payload, 92)
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
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 9, 22))
	if not bool(decoded.get("ok", false)):
		return _fail("arena delta snapshot should decode")
	var event: Dictionary = decoded.get("event", {})
	if String(event.get("type", "")) != "ArenaDeltaSnapshot":
		return _fail("arena delta snapshot should use the ArenaDeltaSnapshot event type")
	var snapshot: Dictionary = event.get("snapshot", {})
	var players: Array = snapshot.get("players", [])
	if String(snapshot.get("phase", "")) != "Combat":
		return _fail("arena delta snapshot should preserve phase")
	if int(snapshot.get("tile_units", 0)) != 50:
		return _fail("arena delta snapshot should preserve tile units")
	if players.size() != 1 or int(players[0].get("mana", 0)) != 64:
		return _fail("arena delta snapshot should decode player state")
	if int(players[0].get("current_cast_slot", 0)) != 0:
		return _fail("arena delta snapshot should decode the absence of an active cast")
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
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 9, 21))
	if not bool(decoded.get("ok", false)):
		return _fail("arena effect batch should decode")
	var event: Dictionary = decoded.get("event", {})
	var effects: Array = event.get("effects", [])
	if String(event.get("type", "")) != "ArenaEffectBatch":
		return _fail("arena effect batch should use the ArenaEffectBatch event type")
	if effects.size() != 1 or String(effects[0].get("kind", "")) != "SkillShot":
		return _fail("arena effect batch should preserve the effect kind")
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
