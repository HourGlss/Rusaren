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
	success = _assert_decode_arena_state_snapshot() and success
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


func _assert_decode_arena_state_snapshot() -> bool:
	var payload := PackedByteArray([19])
	_push_u16(payload, 1800)
	_push_u16(payload, 1200)
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
	payload.append(1)
	payload.append(3)
	var decoded := Protocol.decode_server_event(_encode_server_event_packet(payload, 8, 21))
	if not bool(decoded.get("ok", false)):
		return _fail("arena state snapshot should decode")
	var event: Dictionary = decoded.get("event", {})
	var snapshot: Dictionary = event.get("snapshot", {})
	var players: Array = snapshot.get("players", [])
	if String(event.get("type", "")) != "ArenaStateSnapshot":
		return _fail("arena state snapshot should use the ArenaStateSnapshot event type")
	if players.size() != 1:
		return _fail("arena state snapshot should decode one player")
	if int(players[0].get("unlocked_skill_slots", 0)) != 3:
		return _fail("arena state snapshot should preserve unlocked combat slots")
	return true


func _assert_decode_arena_effect_batch() -> bool:
	var payload := PackedByteArray([20])
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
	var packet := PackedByteArray()
	_push_u16(packet, Protocol.PACKET_MAGIC)
	packet.append(Protocol.PROTOCOL_VERSION)
	packet.append(Protocol.CHANNEL_CONTROL)
	packet.append(Protocol.PACKET_KIND_CONTROL_EVENT)
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
