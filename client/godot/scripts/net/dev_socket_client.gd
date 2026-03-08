extends RefCounted
class_name DevSocketClient

const Protocol := preload("res://scripts/net/protocol.gd")

signal opened
signal closed(reason: String)
signal transport_state_changed(state_name: String)
signal transport_error(message: String)
signal packet_received(decoded_event: Dictionary)

var _socket: WebSocketPeer = null
var _last_state := WebSocketPeer.STATE_CLOSED
var _next_control_seq := 1


func open(url: String) -> bool:
	close()
	_socket = WebSocketPeer.new()
	var error := _socket.connect_to_url(url)
	if error != OK:
		_socket = null
		emit_signal("transport_error", "websocket connect failed with code %d" % error)
		return false

	_last_state = _socket.get_ready_state()
	emit_signal("transport_state_changed", _state_name(_last_state))
	return true


func close() -> void:
	var should_emit := _socket != null or _last_state != WebSocketPeer.STATE_CLOSED
	if _socket != null:
		_socket.close()
		_socket = null
	_next_control_seq = 1
	_last_state = WebSocketPeer.STATE_CLOSED
	if should_emit:
		emit_signal("transport_state_changed", "closed")


func poll() -> void:
	if _socket == null:
		return

	var poll_error := _socket.poll()
	if poll_error != OK:
		emit_signal("transport_error", "websocket poll failed with code %d" % poll_error)
		close()
		emit_signal("closed", "websocket poll failed")
		return

	var current_state := _socket.get_ready_state()
	if current_state != _last_state:
		_last_state = current_state
		emit_signal("transport_state_changed", _state_name(current_state))
		if current_state == WebSocketPeer.STATE_OPEN:
			emit_signal("opened")
		elif current_state == WebSocketPeer.STATE_CLOSED:
			var reason := "websocket closed"
			if _socket.get_close_reason() != "":
				reason = _socket.get_close_reason()
			emit_signal("closed", reason)

	if current_state != WebSocketPeer.STATE_OPEN:
		return

	while _socket.get_available_packet_count() > 0:
		var packet := _socket.get_packet()
		var decoded := Protocol.decode_server_event(packet)
		if decoded.get("ok", false):
			emit_signal("packet_received", decoded)
		else:
			emit_signal("transport_error", String(decoded.get("error", "packet decode failed")))


func send_control_command(command_type: String, payload: Dictionary = {}) -> bool:
	if _socket == null or _socket.get_ready_state() != WebSocketPeer.STATE_OPEN:
		emit_signal("transport_error", "websocket is not open")
		return false

	var encoded := Protocol.encode_client_command(command_type, payload, _next_control_seq, 0)
	if not encoded.get("ok", false):
		emit_signal("transport_error", String(encoded.get("error", "command encoding failed")))
		return false

	var error := _socket.put_packet(encoded.get("packet", PackedByteArray()))
	if error != OK:
		emit_signal("transport_error", "websocket send failed with code %d" % error)
		return false

	_next_control_seq += 1
	return true


func is_open() -> bool:
	return _socket != null and _socket.get_ready_state() == WebSocketPeer.STATE_OPEN


func _state_name(state_code: int) -> String:
	match state_code:
		WebSocketPeer.STATE_CONNECTING:
			return "connecting"
		WebSocketPeer.STATE_OPEN:
			return "open"
		WebSocketPeer.STATE_CLOSING:
			return "closing"
		WebSocketPeer.STATE_CLOSED:
			return "closed"
		_:
			return "unknown"
