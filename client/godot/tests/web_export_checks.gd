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


func _expect_equal(actual: String, expected: String, context: String) -> bool:
	if actual != expected:
		return _fail("%s: expected \"%s\" but received \"%s\"" % [context, expected, actual])
	return true


func _fail(message: String) -> bool:
	printerr(message)
	return false
