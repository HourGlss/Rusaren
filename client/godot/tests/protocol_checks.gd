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
