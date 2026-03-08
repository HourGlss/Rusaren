extends RefCounted
class_name RarenaProtocol

const PACKET_MAGIC := 0x5241
const PROTOCOL_VERSION := 1
const HEADER_LEN := 16
const MAX_PLAYER_NAME_LEN := 24
const MAX_MESSAGE_BYTES := 200

const CHANNEL_CONTROL := 0
const CHANNEL_INPUT := 1
const PACKET_KIND_CONTROL_COMMAND := 6
const PACKET_KIND_CONTROL_EVENT := 7
const PACKET_KIND_INPUT_FRAME := 16
const BUTTON_PRIMARY := 1
const BUTTON_SECONDARY := 1 << 1
const BUTTON_CAST := 1 << 2
const BUTTON_CANCEL := 1 << 3
const BUTTON_QUIT_TO_LOBBY := 1 << 4
const ALLOWED_BUTTONS_MASK := (
	BUTTON_PRIMARY
	| BUTTON_SECONDARY
	| BUTTON_CAST
	| BUTTON_CANCEL
	| BUTTON_QUIT_TO_LOBBY
)
const MIN_I16 := -32768
const MAX_I16 := 32767
const MAX_U16 := 65535
const MAX_U32 := 4294967295

const TEAM_A := 1
const TEAM_B := 2
const READY_NOT_READY := 0
const READY_READY := 1
const SKILL_TREE_WARRIOR := 1
const SKILL_TREE_ROGUE := 2
const SKILL_TREE_MAGE := 3
const SKILL_TREE_CLERIC := 4
const MATCH_OUTCOME_TEAM_A_WIN := 1
const MATCH_OUTCOME_TEAM_B_WIN := 2
const MATCH_OUTCOME_NO_CONTEST := 3


class ByteCursor:
	extends RefCounted

	var payload: PackedByteArray
	var kind: String
	var index := 0
	var error_message := ""

	func _init(bytes: PackedByteArray, kind_name: String, start_index: int = 0) -> void:
		payload = bytes
		kind = kind_name
		index = start_index

	func has_error() -> bool:
		return error_message != ""

	func _ensure_available(needed: int) -> bool:
		if index + needed > payload.size():
			error_message = "%s payload expected at least %d bytes but received %d" % [
				kind,
				index + needed,
				payload.size(),
			]
			return false
		return true

	func read_u8() -> Variant:
		if not _ensure_available(1):
			return null
		var value := int(payload[index])
		index += 1
		return value

	func read_bool() -> Variant:
		var raw = read_u8()
		if has_error():
			return null
		if raw == 0:
			return false
		if raw == 1:
			return true
		error_message = "encoded boolean %d is invalid" % raw
		return null

	func read_u16() -> Variant:
		if not _ensure_available(2):
			return null
		var value := int(payload[index]) | (int(payload[index + 1]) << 8)
		index += 2
		return value

	func read_u32() -> Variant:
		if not _ensure_available(4):
			return null
		var value := int(payload[index])
		value |= int(payload[index + 1]) << 8
		value |= int(payload[index + 2]) << 16
		value |= int(payload[index + 3]) << 24
		index += 4
		return value

	func read_player_id() -> Variant:
		var raw = read_u32()
		if has_error():
			return null
		if raw <= 0:
			error_message = "encoded player id %d is invalid" % raw
			return null
		return raw

	func read_lobby_id() -> Variant:
		var raw = read_u32()
		if has_error():
			return null
		if raw <= 0:
			error_message = "encoded lobby id %d is invalid" % raw
			return null
		return raw

	func read_match_id() -> Variant:
		var raw = read_u32()
		if has_error():
			return null
		if raw <= 0:
			error_message = "encoded match id %d is invalid" % raw
			return null
		return raw

	func read_round() -> Variant:
		var raw = read_u8()
		if has_error():
			return null
		if raw <= 0 or raw > 5:
			error_message = "encoded round %d is invalid" % raw
			return null
		return raw

	func read_string(field: String, max_len: int) -> Variant:
		var length = read_u8()
		if has_error():
			return null
		if length > max_len:
			error_message = "%s length %d exceeds maximum %d" % [field, length, max_len]
			return null
		if not _ensure_available(length):
			return null

		var bytes := PackedByteArray()
		bytes.resize(length)
		for offset in range(length):
			bytes[offset] = payload[index + offset]
		index += length
		return bytes.get_string_from_utf8()

	func read_record() -> Variant:
		var wins = read_u16()
		var losses = read_u16()
		var no_contests = read_u16()
		if has_error():
			return null
		return {
			"wins": wins,
			"losses": losses,
			"no_contests": no_contests,
		}

	func read_team_label() -> Variant:
		var raw = read_u8()
		if has_error():
			return null
		match raw:
			TEAM_A:
				return "Team A"
			TEAM_B:
				return "Team B"
			_:
				error_message = "encoded team %d is invalid" % raw
				return null

	func read_optional_team_label() -> Variant:
		var raw = read_u8()
		if has_error():
			return null
		match raw:
			0:
				return "Unassigned"
			TEAM_A:
				return "Team A"
			TEAM_B:
				return "Team B"
			_:
				error_message = "encoded optional team %d is invalid" % raw
				return null

	func read_ready_label() -> Variant:
		var raw = read_u8()
		if has_error():
			return null
		match raw:
			READY_NOT_READY:
				return "Not Ready"
			READY_READY:
				return "Ready"
			_:
				error_message = "encoded ready state %d is invalid" % raw
				return null

	func read_skill_tree_name() -> Variant:
		var raw = read_u8()
		if has_error():
			return null
		match raw:
			SKILL_TREE_WARRIOR:
				return "Warrior"
			SKILL_TREE_ROGUE:
				return "Rogue"
			SKILL_TREE_MAGE:
				return "Mage"
			SKILL_TREE_CLERIC:
				return "Cleric"
			_:
				error_message = "encoded skill tree %d is invalid" % raw
				return null

	func read_match_outcome_name() -> Variant:
		var raw = read_u8()
		if has_error():
			return null
		match raw:
			MATCH_OUTCOME_TEAM_A_WIN:
				return "Team A Win"
			MATCH_OUTCOME_TEAM_B_WIN:
				return "Team B Win"
			MATCH_OUTCOME_NO_CONTEST:
				return "No Contest"
			_:
				error_message = "encoded match outcome %d is invalid" % raw
				return null

	func read_lobby_phase() -> Variant:
		var raw = read_u8()
		if has_error():
			return null
		match raw:
			0:
				return {
					"name": "Open",
					"seconds_remaining": 0,
				}
			1:
				var seconds_remaining = read_u8()
				if has_error():
					return null
				return {
					"name": "Launch Countdown",
					"seconds_remaining": seconds_remaining,
				}
			_:
				error_message = "encoded lobby phase %d is invalid" % raw
				return null

	func finish() -> Dictionary:
		if has_error():
			return {
				"ok": false,
				"error": error_message,
			}
		if index != payload.size():
			return {
				"ok": false,
				"error": "%s payload contained %d unexpected trailing bytes" % [
					kind,
					payload.size() - index,
				],
			}
		return {"ok": true}


static func encode_client_command(
	command_type: String,
	payload: Dictionary,
	seq: int,
	sim_tick: int = 0
) -> Dictionary:
	var body := PackedByteArray()

	match command_type:
		"Connect":
			var player_id := int(payload.get("player_id", 0))
			var player_name := String(payload.get("player_name", "")).strip_edges()
			if player_id <= 0:
				return _error("player id must be a positive integer")
			if player_name.is_empty():
				return _error("player name must not be empty")
			if player_name.length() > MAX_PLAYER_NAME_LEN:
				return _error("player name length %d exceeds maximum %d" % [
					player_name.length(),
					MAX_PLAYER_NAME_LEN,
				])
			for index in range(player_name.length()):
				var code := player_name.unicode_at(index)
				var ascii_alphanumeric := (
					(code >= 48 and code <= 57)
					or (code >= 65 and code <= 90)
					or (code >= 97 and code <= 122)
				)
				if not ascii_alphanumeric and code != 95 and code != 45:
					return _error("player name contains unsupported characters")
			body.append(1)
			_push_u32(body, player_id)
			_push_string(body, player_name)
		"CreateGameLobby":
			body.append(2)
		"JoinGameLobby":
			var lobby_id := int(payload.get("lobby_id", 0))
			if lobby_id <= 0:
				return _error("lobby id must be a positive integer")
			body.append(3)
			_push_u32(body, lobby_id)
		"LeaveGameLobby":
			body.append(4)
		"SelectTeam":
			var team_name := String(payload.get("team", ""))
			var team_code := _team_code(team_name)
			if team_code == -1:
				return _error("team must be Team A or Team B")
			body.append(5)
			body.append(team_code)
		"SetReady":
			body.append(6)
			body.append(READY_READY if bool(payload.get("ready", false)) else READY_NOT_READY)
		"ChooseSkill":
			var tree_name := String(payload.get("tree", ""))
			var tier := int(payload.get("tier", 0))
			var tree_code := _skill_tree_code(tree_name)
			if tree_code == -1:
				return _error("skill tree must be Warrior, Rogue, Mage, or Cleric")
			if tier < 1 or tier > 5:
				return _error("skill tier must be between 1 and 5")
			body.append(7)
			body.append(tree_code)
			body.append(tier)
		"QuitToCentralLobby":
			body.append(8)
		_:
			return _error("unsupported client control command %s" % command_type)

	return _encode_packet(CHANNEL_CONTROL, PACKET_KIND_CONTROL_COMMAND, 0, body, seq, sim_tick)


static func encode_input_frame(payload: Dictionary, seq: int, sim_tick: int = 0) -> Dictionary:
	var client_input_tick_result := _checked_range(
		payload.get("client_input_tick", 0),
		"client_input_tick",
		0,
		MAX_U32
	)
	if not client_input_tick_result.get("ok", false):
		return client_input_tick_result
	var move_horizontal_result := _checked_range(
		payload.get("move_horizontal_q", 0),
		"move_horizontal_q",
		-1,
		1
	)
	if not move_horizontal_result.get("ok", false):
		return move_horizontal_result
	var move_vertical_result := _checked_range(
		payload.get("move_vertical_q", 0),
		"move_vertical_q",
		-1,
		1
	)
	if not move_vertical_result.get("ok", false):
		return move_vertical_result
	var aim_horizontal_result := _checked_range(
		payload.get("aim_horizontal_q", 0),
		"aim_horizontal_q",
		MIN_I16,
		MAX_I16
	)
	if not aim_horizontal_result.get("ok", false):
		return aim_horizontal_result
	var aim_vertical_result := _checked_range(
		payload.get("aim_vertical_q", 0),
		"aim_vertical_q",
		MIN_I16,
		MAX_I16
	)
	if not aim_vertical_result.get("ok", false):
		return aim_vertical_result
	var ability_result := _checked_range(
		payload.get("ability_or_context", 0),
		"ability_or_context",
		0,
		MAX_U16
	)
	if not ability_result.get("ok", false):
		return ability_result

	var buttons := 0
	if bool(payload.get("primary", false)):
		buttons |= BUTTON_PRIMARY
	if bool(payload.get("secondary", false)):
		buttons |= BUTTON_SECONDARY
	if bool(payload.get("cast", false)):
		buttons |= BUTTON_CAST
	if bool(payload.get("cancel", false)):
		buttons |= BUTTON_CANCEL
	if bool(payload.get("quit_to_lobby", false)):
		buttons |= BUTTON_QUIT_TO_LOBBY
	if buttons & ~ALLOWED_BUTTONS_MASK != 0:
		return _error("input frame contains unsupported button bits")

	var ability_or_context := int(ability_result.get("value", 0))
	var cast_requested := buttons & BUTTON_CAST != 0
	match [cast_requested, ability_or_context]:
		[true, 0]:
			return _error("cast input requires a non-zero ability_or_context")
		[false, _]:
			if ability_or_context != 0:
				return _error("non-cast input must not provide ability_or_context")

	var body := PackedByteArray()
	_push_u32(body, int(client_input_tick_result.get("value", 0)))
	_push_i16(body, int(move_horizontal_result.get("value", 0)))
	_push_i16(body, int(move_vertical_result.get("value", 0)))
	_push_i16(body, int(aim_horizontal_result.get("value", 0)))
	_push_i16(body, int(aim_vertical_result.get("value", 0)))
	_push_u16(body, buttons)
	_push_u16(body, ability_or_context)
	return _encode_packet(CHANNEL_INPUT, PACKET_KIND_INPUT_FRAME, 0, body, seq, sim_tick)


static func decode_server_event(packet: PackedByteArray) -> Dictionary:
	var decoded := decode_packet(packet)
	if not decoded.get("ok", false):
		return decoded

	var header: Dictionary = decoded.get("header", {})
	if int(header.get("channel_id", -1)) != CHANNEL_CONTROL or int(header.get("packet_kind", -1)) != PACKET_KIND_CONTROL_EVENT:
		return _error("expected Control/ControlEvent but received %s/%s" % [
			str(header.get("channel_id", -1)),
			str(header.get("packet_kind", -1)),
		])

	var payload: PackedByteArray = decoded.get("payload", PackedByteArray())
	if payload.is_empty():
		return _error("ServerControlEvent payload expected at least 1 bytes but received 0")

	var kind := int(payload[0])
	var cursor := ByteCursor.new(payload, "ServerControlEvent", 1)
	var event := {}

	match kind:
		1:
			var player_id = cursor.read_player_id()
			var player_name = cursor.read_string("player_name", MAX_PLAYER_NAME_LEN)
			var record = cursor.read_record()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "Connected",
				"player_id": player_id,
				"player_name": player_name,
				"record": record,
			}
		2:
			var lobby_id = cursor.read_lobby_id()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "GameLobbyCreated",
				"lobby_id": lobby_id,
			}
		3:
			var joined_lobby_id = cursor.read_lobby_id()
			var joined_player_id = cursor.read_player_id()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "GameLobbyJoined",
				"lobby_id": joined_lobby_id,
				"player_id": joined_player_id,
			}
		4:
			var left_lobby_id = cursor.read_lobby_id()
			var left_player_id = cursor.read_player_id()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "GameLobbyLeft",
				"lobby_id": left_lobby_id,
				"player_id": left_player_id,
			}
		5:
			var selected_player_id = cursor.read_player_id()
			var team = cursor.read_team_label()
			var ready_reset = cursor.read_bool()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "TeamSelected",
				"player_id": selected_player_id,
				"team": team,
				"ready_reset": ready_reset,
			}
		6:
			var ready_player_id = cursor.read_player_id()
			var ready_label = cursor.read_ready_label()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "ReadyChanged",
				"player_id": ready_player_id,
				"ready": ready_label,
			}
		7:
			var countdown_lobby_id = cursor.read_lobby_id()
			var countdown_seconds = cursor.read_u8()
			var roster_size = cursor.read_u16()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "LaunchCountdownStarted",
				"lobby_id": countdown_lobby_id,
				"seconds_remaining": countdown_seconds,
				"roster_size": roster_size,
			}
		8:
			var tick_lobby_id = cursor.read_lobby_id()
			var tick_seconds = cursor.read_u8()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "LaunchCountdownTick",
				"lobby_id": tick_lobby_id,
				"seconds_remaining": tick_seconds,
			}
		9:
			var match_id = cursor.read_match_id()
			var round = cursor.read_round()
			var skill_pick_seconds = cursor.read_u8()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "MatchStarted",
				"match_id": match_id,
				"round": round,
				"skill_pick_seconds": skill_pick_seconds,
			}
		10:
			var skill_player_id = cursor.read_player_id()
			var tree_name = cursor.read_skill_tree_name()
			var tier = cursor.read_u8()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "SkillChosen",
				"player_id": skill_player_id,
				"tree": tree_name,
				"tier": tier,
			}
		11:
			var pre_combat_seconds = cursor.read_u8()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "PreCombatStarted",
				"seconds_remaining": pre_combat_seconds,
			}
		12:
			event = {"type": "CombatStarted"}
		13:
			var won_round = cursor.read_round()
			var winning_team = cursor.read_team_label()
			var score_a = cursor.read_u8()
			var score_b = cursor.read_u8()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "RoundWon",
				"round": won_round,
				"winning_team": winning_team,
				"score_a": score_a,
				"score_b": score_b,
			}
		14:
			var outcome = cursor.read_match_outcome_name()
			var end_score_a = cursor.read_u8()
			var end_score_b = cursor.read_u8()
			var message = cursor.read_string("message", MAX_MESSAGE_BYTES)
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "MatchEnded",
				"outcome": outcome,
				"score_a": end_score_a,
				"score_b": end_score_b,
				"message": message,
			}
		15:
			var returned_record = cursor.read_record()
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "ReturnedToCentralLobby",
				"record": returned_record,
			}
		16:
			var error_message = cursor.read_string("message", MAX_MESSAGE_BYTES)
			if cursor.has_error():
				return _error(cursor.error_message)
			event = {
				"type": "Error",
				"message": error_message,
			}
		17:
			var lobby_count = cursor.read_u16()
			if cursor.has_error():
				return _error(cursor.error_message)
			var lobbies: Array[Dictionary] = []
			for _lobby_index in range(int(lobby_count)):
				var lobby_id = cursor.read_lobby_id()
				var player_count = cursor.read_u16()
				var team_a_count = cursor.read_u16()
				var team_b_count = cursor.read_u16()
				var ready_count = cursor.read_u16()
				var phase = cursor.read_lobby_phase()
				if cursor.has_error():
					return _error(cursor.error_message)
				lobbies.append({
					"lobby_id": lobby_id,
					"player_count": player_count,
					"team_a_count": team_a_count,
					"team_b_count": team_b_count,
					"ready_count": ready_count,
					"phase": phase,
				})
			event = {
				"type": "LobbyDirectorySnapshot",
				"lobbies": lobbies,
			}
		18:
			var lobby_id = cursor.read_lobby_id()
			var phase = cursor.read_lobby_phase()
			var player_count = cursor.read_u16()
			if cursor.has_error():
				return _error(cursor.error_message)
			var players: Array[Dictionary] = []
			for _player_index in range(int(player_count)):
				var player_id = cursor.read_player_id()
				var player_name = cursor.read_string("player_name", MAX_PLAYER_NAME_LEN)
				var record = cursor.read_record()
				var team = cursor.read_optional_team_label()
				var ready = cursor.read_ready_label()
				if cursor.has_error():
					return _error(cursor.error_message)
				players.append({
					"player_id": player_id,
					"player_name": player_name,
					"record": record,
					"team": team,
					"ready": ready,
				})
			event = {
				"type": "GameLobbySnapshot",
				"lobby_id": lobby_id,
				"phase": phase,
				"players": players,
			}
		_:
			return _error("unknown server event %d" % kind)

	var finished := cursor.finish()
	if not finished.get("ok", false):
		return finished

	return {
		"ok": true,
		"header": header,
		"event": event,
	}


static func decode_packet(packet: PackedByteArray) -> Dictionary:
	if packet.size() < HEADER_LEN:
		return _error("packet length %d is below the minimum header length %d" % [
			packet.size(),
			HEADER_LEN,
		])

	var magic := int(packet[0]) | (int(packet[1]) << 8)
	if magic != PACKET_MAGIC:
		return _error("packet magic 0x%04x does not match expected 0x%04x" % [
			magic,
			PACKET_MAGIC,
		])

	var version := int(packet[2])
	if version != PROTOCOL_VERSION:
		return _error("protocol version %d does not match expected version %d" % [
			version,
			PROTOCOL_VERSION,
		])

	var channel_id := int(packet[3])
	var packet_kind := int(packet[4])
	var flags := int(packet[5])
	var payload_len := int(packet[6]) | (int(packet[7]) << 8)
	var seq := _read_u32_at(packet, 8)
	var sim_tick := _read_u32_at(packet, 12)
	var actual_payload_len := packet.size() - HEADER_LEN
	if actual_payload_len != payload_len:
		return _error("payload length declared %d but actual bytes were %d" % [
			payload_len,
			actual_payload_len,
		])

	var payload := PackedByteArray()
	payload.resize(payload_len)
	for index in range(payload_len):
		payload[index] = packet[HEADER_LEN + index]

	return {
		"ok": true,
		"header": {
			"version": version,
			"channel_id": channel_id,
			"packet_kind": packet_kind,
			"flags": flags,
			"payload_len": payload_len,
			"seq": seq,
			"sim_tick": sim_tick,
		},
		"payload": payload,
	}


static func _encode_packet(
	channel_id: int,
	packet_kind: int,
	flags: int,
	payload: PackedByteArray,
	seq: int,
	sim_tick: int
) -> Dictionary:
	if payload.size() > 65535:
		return _error("payload length %d exceeds maximum encodable 65535" % payload.size())

	var packet := PackedByteArray()
	_push_u16(packet, PACKET_MAGIC)
	packet.append(PROTOCOL_VERSION)
	packet.append(channel_id)
	packet.append(packet_kind)
	packet.append(flags & 0xff)
	_push_u16(packet, payload.size())
	_push_u32(packet, seq)
	_push_u32(packet, sim_tick)
	packet.append_array(payload)
	return {
		"ok": true,
		"packet": packet,
	}


static func _push_string(bytes: PackedByteArray, value: String) -> void:
	var utf8 := value.to_utf8_buffer()
	bytes.append(utf8.size())
	bytes.append_array(utf8)


static func _push_u16(bytes: PackedByteArray, value: int) -> void:
	bytes.append(value & 0xff)
	bytes.append((value >> 8) & 0xff)


static func _push_i16(bytes: PackedByteArray, value: int) -> void:
	var encoded := value & 0xffff
	bytes.append(encoded & 0xff)
	bytes.append((encoded >> 8) & 0xff)


static func _push_u32(bytes: PackedByteArray, value: int) -> void:
	bytes.append(value & 0xff)
	bytes.append((value >> 8) & 0xff)
	bytes.append((value >> 16) & 0xff)
	bytes.append((value >> 24) & 0xff)


static func _read_u32_at(bytes: PackedByteArray, offset: int) -> int:
	return int(bytes[offset]) | (int(bytes[offset + 1]) << 8) | (int(bytes[offset + 2]) << 16) | (int(bytes[offset + 3]) << 24)


static func _team_code(team_name: String) -> int:
	match team_name:
		"Team A":
			return TEAM_A
		"Team B":
			return TEAM_B
		_:
			return -1


static func _skill_tree_code(tree_name: String) -> int:
	match tree_name:
		"Warrior":
			return SKILL_TREE_WARRIOR
		"Rogue":
			return SKILL_TREE_ROGUE
		"Mage":
			return SKILL_TREE_MAGE
		"Cleric":
			return SKILL_TREE_CLERIC
		_:
			return -1


static func _error(message: String) -> Dictionary:
	return {
		"ok": false,
		"error": message,
	}


static func _checked_range(raw: Variant, field: String, minimum: int, maximum: int) -> Dictionary:
	if typeof(raw) != TYPE_INT:
		return _error("%s must be an integer" % field)
	var value := int(raw)
	if value < minimum or value > maximum:
		return _error("%s=%d is outside the allowed range %d..=%d" % [
			field,
			value,
			minimum,
			maximum,
		])
	return {
		"ok": true,
		"value": value,
	}
