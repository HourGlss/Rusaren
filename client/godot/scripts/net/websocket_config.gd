extends RefCounted
class_name WebSocketConfig

const DEFAULT_LOCAL_URL := "ws://127.0.0.1:3000/ws"


func runtime_default_url(configured_url: String = DEFAULT_LOCAL_URL) -> String:
	return derive_url(
		configured_url,
		_browser_origin(),
		_should_prefer_browser_origin(configured_url)
	)


func derive_url(
	configured_url: String,
	browser_origin: String,
	prefer_browser_origin: bool
) -> String:
	var trimmed_url := configured_url.strip_edges()
	var trimmed_origin := browser_origin.strip_edges()
	if prefer_browser_origin and trimmed_origin != "":
		var same_origin_url := _origin_to_websocket(trimmed_origin)
		if same_origin_url != "":
			return "%s/ws" % same_origin_url

	if trimmed_url != "":
		return trimmed_url

	return DEFAULT_LOCAL_URL


func _should_prefer_browser_origin(configured_url: String) -> bool:
	var trimmed_url := configured_url.strip_edges()
	return trimmed_url == "" or trimmed_url == DEFAULT_LOCAL_URL


func _browser_origin() -> String:
	if not OS.has_feature("web"):
		return ""

	var origin: Variant = JavaScriptBridge.eval("window.location.origin", true)
	if origin == null:
		return ""

	return String(origin).strip_edges()


func _origin_to_websocket(origin: String) -> String:
	var trimmed_origin := origin.strip_edges()
	if trimmed_origin.ends_with("/"):
		trimmed_origin = trimmed_origin.left(trimmed_origin.length() - 1)

	if trimmed_origin.begins_with("https://"):
		return "wss://%s" % trimmed_origin.trim_prefix("https://")
	if trimmed_origin.begins_with("http://"):
		return "ws://%s" % trimmed_origin.trim_prefix("http://")
	if trimmed_origin.begins_with("wss://") or trimmed_origin.begins_with("ws://"):
		return trimmed_origin

	return ""
