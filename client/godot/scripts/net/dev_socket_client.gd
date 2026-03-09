extends RefCounted
class_name DevSocketClient

const Protocol := preload("res://scripts/net/protocol.gd")

signal opened
signal closed(reason: String)
signal transport_state_changed(state_name: String)
signal transport_error(message: String)
signal packet_received(decoded_event: Dictionary)

var _signal_socket: WebSocketPeer = null
var _peer: WebRTCPeerConnection = null
var _control_channel: WebRTCDataChannel = null
var _input_channel: WebRTCDataChannel = null
var _snapshot_channel: WebRTCDataChannel = null
var _transport_state := "closed"
var _next_control_seq := 1
var _next_input_seq := 1
var _opened_emitted := false
var _signal_url := ""
var _closing_requested := false


func open(url: String) -> bool:
	close()
	_signal_socket = WebSocketPeer.new()
	_signal_url = url.strip_edges()
	if _signal_url == "":
		emit_signal("transport_error", "signaling url must not be empty")
		_signal_socket = null
		return false

	var error := _signal_socket.connect_to_url(_signal_url)
	if error != OK:
		_signal_socket = null
		emit_signal("transport_error", "signaling websocket connect failed with code %d" % error)
		return false

	_closing_requested = false
	_opened_emitted = false
	_set_transport_state("connecting")
	return true


func close() -> void:
	_closing_requested = true
	if _signal_socket != null and _signal_socket.get_ready_state() == WebSocketPeer.STATE_OPEN:
		_send_signal_message({"type": "bye"})
		_signal_socket.close()
	_signal_socket = null
	if _peer != null:
		_peer.close()
	_peer = null
	_control_channel = null
	_input_channel = null
	_snapshot_channel = null
	_next_control_seq = 1
	_next_input_seq = 1
	_opened_emitted = false
	_set_transport_state("closed")


func poll() -> void:
	_poll_signal_socket()
	_poll_webrtc_peer()
	_poll_data_channel(_control_channel)
	_poll_data_channel(_snapshot_channel)

	if _control_channel != null and _control_channel.get_ready_state() == WebRTCDataChannel.STATE_OPEN:
		if not _opened_emitted:
			_opened_emitted = true
			_set_transport_state("open")
			emit_signal("opened")


func send_control_command(command_type: String, payload: Dictionary = {}) -> bool:
	if not _channel_is_open(_control_channel):
		emit_signal("transport_error", "control data channel is not open")
		return false

	var encoded := Protocol.encode_client_command(command_type, payload, _next_control_seq, 0)
	if not encoded.get("ok", false):
		emit_signal("transport_error", String(encoded.get("error", "command encoding failed")))
		return false

	var error := _control_channel.put_packet(encoded.get("packet", PackedByteArray()))
	if error != OK:
		emit_signal("transport_error", "control data channel send failed with code %d" % error)
		return false

	_next_control_seq += 1
	return true


func send_input_frame(payload: Dictionary = {}, sim_tick: int = 0) -> bool:
	if not _channel_is_open(_input_channel):
		emit_signal("transport_error", "input data channel is not open")
		return false

	var encoded := Protocol.encode_input_frame(payload, _next_input_seq, sim_tick)
	if not encoded.get("ok", false):
		emit_signal("transport_error", String(encoded.get("error", "input encoding failed")))
		return false

	var error := _input_channel.put_packet(encoded.get("packet", PackedByteArray()))
	if error != OK:
		emit_signal("transport_error", "input data channel send failed with code %d" % error)
		return false

	_next_input_seq += 1
	return true


func is_open() -> bool:
	return _channel_is_open(_control_channel)


func _poll_signal_socket() -> void:
	if _signal_socket == null:
		return

	_signal_socket.poll()
	var current_state := _signal_socket.get_ready_state()
	match current_state:
		WebSocketPeer.STATE_CONNECTING:
			_set_transport_state("connecting")
		WebSocketPeer.STATE_CLOSING:
			_set_transport_state("closing")
		WebSocketPeer.STATE_OPEN:
			if _transport_state == "closed":
				_set_transport_state("connecting")
		WebSocketPeer.STATE_CLOSED:
			if not _closing_requested:
				var close_reason := _signal_socket.get_close_reason()
				var failure_reason := "signaling websocket closed"
				if close_reason != "":
					failure_reason = close_reason
				elif _signal_socket.get_close_code() != -1:
					failure_reason = "signaling websocket closed with code %d" % _signal_socket.get_close_code()
				_fail_transport(failure_reason)
			return

	while _signal_socket.get_available_packet_count() > 0:
		var packet := _signal_socket.get_packet()
		if not _signal_socket.was_string_packet():
			_fail_transport("binary websocket messages are not accepted on /ws")
			return
		_handle_signal_message(packet.get_string_from_utf8())
		if _signal_socket == null:
			return


func _poll_webrtc_peer() -> void:
	if _peer == null:
		return

	var error := _peer.poll()
	if error != OK:
		_fail_transport("WebRTC poll failed with code %d" % error)
		return

	if _control_channel != null and _control_channel.get_ready_state() == WebRTCDataChannel.STATE_CLOSED and not _closing_requested:
		_fail_transport("the WebRTC control data channel closed")


func _poll_data_channel(data_channel: WebRTCDataChannel) -> void:
	if data_channel == null or data_channel.get_ready_state() != WebRTCDataChannel.STATE_OPEN:
		return

	while data_channel.get_available_packet_count() > 0:
		var packet := data_channel.get_packet()
		var decoded := Protocol.decode_server_event(packet)
		if decoded.get("ok", false):
			emit_signal("packet_received", decoded)
		else:
			emit_signal("transport_error", String(decoded.get("error", "packet decode failed")))


func _handle_signal_message(text: String) -> void:
	var parsed: Variant = JSON.parse_string(text)
	if typeof(parsed) != TYPE_DICTIONARY:
		_fail_transport("signaling websocket returned invalid json")
		return

	var message := parsed as Dictionary
	var message_type := String(message.get("type", ""))
	match message_type:
		"hello":
			_handle_hello_message(message)
		"session_description":
			_handle_session_description(message)
		"ice_candidate":
			_handle_ice_candidate(message)
		"error":
			var error_message := String(message.get("message", "unknown signaling error"))
			emit_signal("transport_error", error_message)
			_fail_transport(error_message)
		_:
			_fail_transport("unsupported signaling message type %s" % message_type)


func _handle_hello_message(message: Dictionary) -> void:
	if _peer != null:
		_fail_transport("received duplicate WebRTC hello message")
		return

	var protocol_version := int(message.get("protocol_version", -1))
	if protocol_version != Protocol.PROTOCOL_VERSION:
		_fail_transport("server protocol version %d does not match client version %d" % [
			protocol_version,
			Protocol.PROTOCOL_VERSION,
		])
		return

	var channels: Dictionary = message.get("channels", {})
	if not _validate_channels(channels):
		return

	var peer := WebRTCPeerConnection.new()
	peer.session_description_created.connect(_on_session_description_created)
	peer.ice_candidate_created.connect(_on_ice_candidate_created)
	peer.data_channel_received.connect(_on_data_channel_received)

	var init_config := {
		"iceServers": _ice_servers_to_godot_config(message.get("ice_servers", [])),
	}
	var init_error := peer.initialize(init_config)
	if init_error != OK:
		if not OS.has_feature("web"):
			_fail_transport("This native Godot runtime does not have a WebRTC extension configured. Use the browser client for local play or install the webrtc-native extension for editor/native testing.")
			return

		_fail_transport("WebRTC peer initialization failed with code %d" % init_error)
		return

	var control_channel = peer.create_data_channel("control", {
		"id": Protocol.CHANNEL_CONTROL,
		"negotiated": true,
		"ordered": true,
	})
	var input_channel = peer.create_data_channel("input", {
		"id": Protocol.CHANNEL_INPUT,
		"negotiated": true,
		"ordered": false,
		"maxRetransmits": 0,
	})
	var snapshot_channel = peer.create_data_channel("snapshot", {
		"id": Protocol.CHANNEL_SNAPSHOT,
		"negotiated": true,
		"ordered": false,
		"maxRetransmits": 0,
	})
	if control_channel == null or input_channel == null or snapshot_channel == null:
		if not OS.has_feature("web"):
			_fail_transport("This native Godot runtime cannot create WebRTC data channels without the webrtc-native extension. Use the browser client for local play or install the extension for native testing.")
			return

		_fail_transport("failed to create negotiated WebRTC data channels")
		return

	_configure_data_channel(control_channel)
	_configure_data_channel(input_channel)
	_configure_data_channel(snapshot_channel)

	_peer = peer
	_control_channel = control_channel
	_input_channel = input_channel
	_snapshot_channel = snapshot_channel

	var offer_error := _peer.create_offer()
	if offer_error != OK:
		_fail_transport("WebRTC offer creation failed with code %d" % offer_error)
		return
	_set_transport_state("connecting")


func _handle_session_description(message: Dictionary) -> void:
	if _peer == null:
		_fail_transport("received a session description before WebRTC initialization")
		return

	var description: Dictionary = message.get("description", {})
	var sdp_type := String(description.get("type", ""))
	var sdp := String(description.get("sdp", ""))
	if sdp_type != "answer" or sdp == "":
		_fail_transport("server session descriptions must be non-empty answers")
		return

	var error := _peer.set_remote_description(sdp_type, sdp)
	if error != OK:
		_fail_transport("failed to apply the remote WebRTC answer: %d" % error)


func _handle_ice_candidate(message: Dictionary) -> void:
	if _peer == null:
		_fail_transport("received an ICE candidate before WebRTC initialization")
		return

	var candidate: Dictionary = message.get("candidate", {})
	var candidate_name := String(candidate.get("candidate", ""))
	if candidate_name == "":
		_fail_transport("server ICE candidates must include candidate text")
		return

	var sdp_mid := String(candidate.get("sdp_mid", ""))
	var sdp_mline_index := int(candidate.get("sdp_mline_index", -1))
	var error := _peer.add_ice_candidate(sdp_mid, sdp_mline_index, candidate_name)
	if error != OK:
		_fail_transport("failed to add a remote ICE candidate: %d" % error)


func _on_session_description_created(sdp_type: String, sdp: String) -> void:
	if _peer == null:
		return

	var error := _peer.set_local_description(sdp_type, sdp)
	if error != OK:
		_fail_transport("failed to apply the local WebRTC %s: %d" % [sdp_type, error])
		return

	_send_signal_message({
		"type": "session_description",
		"description": {
			"type": sdp_type,
			"sdp": sdp,
		},
	})


func _on_ice_candidate_created(media: String, index: int, name: String) -> void:
	if name == "":
		return
	_send_signal_message({
		"type": "ice_candidate",
		"candidate": {
			"candidate": name,
			"sdp_mid": media if media != "" else null,
			"sdp_mline_index": index if index >= 0 else null,
		},
	})


func _on_data_channel_received(channel: WebRTCDataChannel) -> void:
	if channel == null:
		return
	emit_signal("transport_error", "received unexpected non-negotiated data channel %s" % channel.get_label())


func _send_signal_message(message: Dictionary) -> void:
	if _signal_socket == null or _signal_socket.get_ready_state() != WebSocketPeer.STATE_OPEN:
		return

	var json := JSON.stringify(message)
	var error := _signal_socket.send_text(json)
	if error != OK:
		_fail_transport("signaling websocket send failed with code %d" % error)


func _channel_is_open(channel: WebRTCDataChannel) -> bool:
	return channel != null and channel.get_ready_state() == WebRTCDataChannel.STATE_OPEN


func _configure_data_channel(channel: WebRTCDataChannel) -> void:
	channel.write_mode = WebRTCDataChannel.WRITE_MODE_BINARY


func _validate_channels(channels: Dictionary) -> bool:
	if int(channels.get("control", -1)) != Protocol.CHANNEL_CONTROL:
		_fail_transport("server control channel id does not match the client protocol")
		return false
	if int(channels.get("input", -1)) != Protocol.CHANNEL_INPUT:
		_fail_transport("server input channel id does not match the client protocol")
		return false
	if int(channels.get("snapshot", -1)) != Protocol.CHANNEL_SNAPSHOT:
		_fail_transport("server snapshot channel id does not match the client protocol")
		return false
	return true


func _ice_servers_to_godot_config(raw_servers: Variant) -> Array:
	var servers: Array = []
	if typeof(raw_servers) != TYPE_ARRAY:
		return servers

	for server_value in raw_servers:
		if typeof(server_value) != TYPE_DICTIONARY:
			continue
		var server := server_value as Dictionary
		var urls: Array = []
		for url_value in server.get("urls", []):
			var url := String(url_value).strip_edges()
			if url != "":
				urls.append(url)
		if urls.is_empty():
			continue
		var normalized := {"urls": urls}
		var username := String(server.get("username", "")).strip_edges()
		var credential := String(server.get("credential", "")).strip_edges()
		if username != "":
			normalized["username"] = username
		if credential != "":
			normalized["credential"] = credential
		servers.append(normalized)
	return servers


func _fail_transport(reason: String) -> void:
	if reason != "":
		emit_signal("transport_error", reason)
	if _peer != null:
		_peer.close()
	_peer = null
	if _signal_socket != null:
		_signal_socket.close()
	_signal_socket = null
	_control_channel = null
	_input_channel = null
	_snapshot_channel = null
	_next_control_seq = 1
	_next_input_seq = 1
	_opened_emitted = false
	_set_transport_state("closed")
	emit_signal("closed", reason)


func _set_transport_state(state_name: String) -> void:
	if _transport_state == state_name:
		return
	_transport_state = state_name
	emit_signal("transport_state_changed", state_name)
