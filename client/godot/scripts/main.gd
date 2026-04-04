extends Control

const ClientStateScript := preload("res://scripts/state/client_state.gd")
const DevSocketClientScript := preload("res://scripts/net/dev_socket_client.gd")
const GodotPerfMonitorsScript := preload("res://scripts/debug/godot_perf_monitors.gd")
const PerfClockScript := preload("res://scripts/debug/perf_clock.gd")
const MainShellFactoryScript := preload("res://scripts/main_shell_factory.gd")
const ClientStateViewScript := preload("res://scripts/state/client_state_view.gd")
const Protocol := preload("res://scripts/net/protocol.gd")
const WebSocketConfigScript := preload("res://scripts/net/websocket_config.gd")

const MENU_SECTION_NAME := "name"
const MENU_SECTION_LOADOUT := "loadout"
const MENU_SECTION_RECORD := "record"
const MENU_SECTION_ROSTER := "roster"
const MENU_SECTION_EVENTS := "events"
const MENU_SECTION_DIAGNOSTICS := "diagnostics"

const MENU_ACTION_CHANGE_NAME := 1
const MENU_ACTION_TRAINING_LOADOUT := 2
const MENU_ACTION_PLAYER_RECORD := 3
const MENU_ACTION_ROSTER_WATCH := 4
const MENU_ACTION_EVENT_FEED := 5
const MENU_ACTION_DIAGNOSTICS := 6

const AUTO_RECONNECT_DELAY_SECONDS := 2.0
const PASSIVE_UI_REFRESH_INTERVAL_SECONDS := 0.10
const RANDOM_PLAYER_NAME_LENGTH := 10
const PLAYER_NAME_ALPHABET := "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
const CUSTOM_PERF_MONITOR_IDS := [
	"Rarena/UIRefreshMs",
	"Rarena/ArenaDrawMs",
	"Rarena/ArenaVisibilityMs",
	"Rarena/ArenaBaseDrawMs",
	"Rarena/ArenaCacheSyncMs",
	"Rarena/ArenaCacheBackgroundMs",
	"Rarena/ArenaCacheVisibilityMs",
	"Rarena/Players",
	"Rarena/VisibleTiles",
]

var auto_connect_enabled := true

var app_state := ClientStateScript.new()
var transport := DevSocketClientScript.new()
var websocket_config := WebSocketConfigScript.new()

var connection_panel: PanelContainer
var player_name_input: LineEdit
var menu_button: MenuButton
var fullscreen_menu: Control
var fullscreen_menu_title: Label
var name_menu_view: VBoxContainer
var training_loadout_view: VBoxContainer
var record_view: VBoxContainer
var roster_view: VBoxContainer
var event_view: VBoxContainer
var diagnostics_view: VBoxContainer
var banner_label: Label
var status_label: Label
var record_label: Label
var identity_label: Label
var phase_label: Label
var countdown_value_label: Label
var training_metrics_label: Label
var cooldown_summary_label: Label
var score_label: Label
var outcome_label: Label
var match_summary_log: RichTextLabel
var lobby_label: Label
var lobby_note_label: Label
var team_label: Label
var lobby_roster_log: RichTextLabel
var central_panel: PanelContainer
var lobby_panel: PanelContainer
var match_panel: PanelContainer
var results_panel: PanelContainer
var central_directory_log: RichTextLabel
var roster_log: RichTextLabel
var event_log: RichTextLabel
var diagnostics_log: RichTextLabel
var join_lobby_input: LineEdit
var ready_button: Button
var leave_lobby_button: Button
var quit_results_button: Button
var create_lobby_button: Button
var join_lobby_button: Button
var start_training_button: Button
var team_a_button: Button
var team_b_button: Button
var training_loadout_button: Button
var reset_training_button: Button
var quit_arena_button: Button
var name_save_button: Button
var name_randomize_button: Button
var diagnostics_copy_button: Button
var skill_pick_panel: PanelContainer
var skill_pick_inline_host: VBoxContainer
var training_loadout_host: VBoxContainer
var skill_pick_summary_label: Label
var round_summary_log: RichTextLabel
var skill_scroll: ScrollContainer
var skill_columns: GridContainer
var combat_panel: VBoxContainer
var arena_view: ArenaView = null
var skill_buttons: Array[Button] = []
var _rendered_skill_catalog_signature := ""
var _rendered_skill_button_state_signature := ""
var _rendered_menu_popup_signature := ""
var _next_client_input_tick := 1
var _pending_primary_attack := false
var _pending_cast_slot := 0
var _last_sent_aim := Vector2i.ZERO
var _bootstrap_request: HTTPRequest
var _bootstrap_request_active := false
var _pending_bootstrap_url := ""
var _menu_section := ""
var _auto_connect_retry_seconds := -1.0
var _name_rng := RandomNumberGenerator.new()
var _passive_ui_refresh_remaining := 0.0


func _ready() -> void:
	_name_rng.randomize()
	_bootstrap_request = HTTPRequest.new()
	add_child(_bootstrap_request)
	_bootstrap_request.request_completed.connect(_on_bootstrap_request_completed)
	_build_shell()
	_bind_transport()
	_install_performance_monitors()
	_apply_player_name(_random_player_name())
	_refresh_ui()
	if auto_connect_enabled:
		_queue_auto_connect(0.0)


func _exit_tree() -> void:
	GodotPerfMonitorsScript.remove_custom_monitors(CUSTOM_PERF_MONITOR_IDS)


func _process(delta: float) -> void:
	var visuals_started_us := PerfClockScript.now_us()
	var visuals_changed := app_state.advance_visuals(delta)
	app_state.record_client_timing("advance_visuals", PerfClockScript.elapsed_us(visuals_started_us))
	transport.poll()
	if arena_view != null:
		arena_view.sync_render_state(visuals_changed)
	_tick_auto_connect(delta)
	_drive_combat_input()
	_tick_passive_ui_refresh(delta)


func _input(event: InputEvent) -> void:
	if not app_state.can_send_combat_input():
		return

	if event is InputEventMouseButton:
		var mouse_button := event as InputEventMouseButton
		if mouse_button.button_index == MOUSE_BUTTON_LEFT and mouse_button.pressed and arena_view != null and arena_view.has_mouse_in_arena():
			_pending_primary_attack = true
			_drive_combat_input()
		return

	if event is InputEventKey and event.pressed and not event.echo:
		match event.keycode:
			KEY_1:
				_queue_combat_cast(1)
			KEY_2:
				_queue_combat_cast(2)
			KEY_3:
				_queue_combat_cast(3)
			KEY_4:
				_queue_combat_cast(4)
			KEY_5:
				_queue_combat_cast(5)
			KEY_X:
				if _pending_cast_slot > 0:
					_drive_combat_input()


func _bind_transport() -> void:
	transport.opened.connect(_on_socket_opened)
	transport.closed.connect(_on_socket_closed)
	transport.transport_state_changed.connect(_on_transport_state_changed)
	transport.transport_error.connect(_on_transport_error)
	transport.packet_received.connect(_on_packet_received)


func _build_shell() -> void:
	var refs := MainShellFactoryScript.build_shell(self, RANDOM_PLAYER_NAME_LENGTH)
	_assign_shell_refs(refs)
	central_directory_log.meta_clicked.connect(_on_lobby_directory_meta_clicked)
	menu_button.get_popup().id_pressed.connect(_on_menu_option_selected)
	arena_view.set_client_state(app_state)
	_rebuild_menu_popup()
	_rebuild_skill_buttons()


func _assign_shell_refs(refs) -> void:
	connection_panel = refs.connection_panel
	player_name_input = refs.player_name_input
	menu_button = refs.menu_button
	fullscreen_menu = refs.fullscreen_menu
	fullscreen_menu_title = refs.fullscreen_menu_title
	name_menu_view = refs.name_menu_view
	training_loadout_view = refs.training_loadout_view
	record_view = refs.record_view
	roster_view = refs.roster_view
	event_view = refs.event_view
	diagnostics_view = refs.diagnostics_view
	banner_label = refs.banner_label
	status_label = refs.status_label
	record_label = refs.record_label
	identity_label = refs.identity_label
	phase_label = refs.phase_label
	countdown_value_label = refs.countdown_value_label
	training_metrics_label = refs.training_metrics_label
	cooldown_summary_label = refs.cooldown_summary_label
	score_label = refs.score_label
	outcome_label = refs.outcome_label
	match_summary_log = refs.match_summary_log
	lobby_label = refs.lobby_label
	lobby_note_label = refs.lobby_note_label
	team_label = refs.team_label
	lobby_roster_log = refs.lobby_roster_log
	central_panel = refs.central_panel
	lobby_panel = refs.lobby_panel
	match_panel = refs.match_panel
	results_panel = refs.results_panel
	central_directory_log = refs.central_directory_log
	roster_log = refs.roster_log
	event_log = refs.event_log
	diagnostics_log = refs.diagnostics_log
	join_lobby_input = refs.join_lobby_input
	ready_button = refs.ready_button
	leave_lobby_button = refs.leave_lobby_button
	quit_results_button = refs.quit_results_button
	create_lobby_button = refs.create_lobby_button
	join_lobby_button = refs.join_lobby_button
	start_training_button = refs.start_training_button
	team_a_button = refs.team_a_button
	team_b_button = refs.team_b_button
	training_loadout_button = refs.training_loadout_button
	reset_training_button = refs.reset_training_button
	quit_arena_button = refs.quit_arena_button
	name_save_button = refs.name_save_button
	name_randomize_button = refs.name_randomize_button
	diagnostics_copy_button = refs.diagnostics_copy_button
	skill_pick_panel = refs.skill_pick_panel
	skill_pick_inline_host = refs.skill_pick_inline_host
	training_loadout_host = refs.training_loadout_host
	skill_pick_summary_label = refs.skill_pick_summary_label
	round_summary_log = refs.round_summary_log
	skill_scroll = refs.skill_scroll
	skill_columns = refs.skill_columns
	combat_panel = refs.combat_panel
	arena_view = refs.arena_view


func _style_clickable(control: Control, color: Color) -> void:
	var normal := StyleBoxFlat.new()
	normal.bg_color = color
	normal.corner_radius_top_left = 12
	normal.corner_radius_top_right = 12
	normal.corner_radius_bottom_right = 12
	normal.corner_radius_bottom_left = 12
	normal.border_color = color.lightened(0.18)
	normal.set_border_width_all(1)
	control.add_theme_stylebox_override("normal", normal)

	var hover := normal.duplicate()
	hover.bg_color = color.lightened(0.08)
	control.add_theme_stylebox_override("hover", hover)

	var disabled := normal.duplicate()
	disabled.bg_color = color.darkened(0.55)
	disabled.border_color = color.darkened(0.35)
	control.add_theme_stylebox_override("disabled", disabled)
	control.add_theme_color_override("font_color", Color8(246, 244, 240))


func _action_button(text: String, color: Color) -> Button:
	var button := Button.new()
	button.text = text
	button.custom_minimum_size = Vector2(0, 42)
	_style_clickable(button, color)
	return button


func _queue_auto_connect(delay_seconds: float) -> void:
	if not auto_connect_enabled:
		return
	_auto_connect_retry_seconds = maxf(0.0, delay_seconds)


func _tick_auto_connect(delta: float) -> void:
	if _auto_connect_retry_seconds < 0.0:
		return
	_auto_connect_retry_seconds = maxf(-1.0, _auto_connect_retry_seconds - delta)
	if _auto_connect_retry_seconds > 0.0:
		return
	_auto_connect_retry_seconds = -1.0
	_on_connect_pressed()


func _cancel_bootstrap_request() -> void:
	if _bootstrap_request_active:
		_bootstrap_request.cancel_request()
		_bootstrap_request_active = false
	_pending_bootstrap_url = ""


func _random_player_name() -> String:
	var result := ""
	for _index in range(RANDOM_PLAYER_NAME_LENGTH):
		var char_index := _name_rng.randi_range(0, PLAYER_NAME_ALPHABET.length() - 1)
		result += PLAYER_NAME_ALPHABET.substr(char_index, 1)
	return result


func _sanitize_player_name(raw_name: String) -> String:
	var sanitized := ""
	for index in range(raw_name.length()):
		var code := raw_name.unicode_at(index)
		var is_upper := code >= 65 and code <= 90
		var is_lower := code >= 97 and code <= 122
		if is_upper or is_lower:
			sanitized += raw_name.substr(index, 1)
		if sanitized.length() >= RANDOM_PLAYER_NAME_LENGTH:
			break
	return sanitized


func _apply_player_name(raw_name: String) -> String:
	var sanitized := _sanitize_player_name(raw_name)
	if sanitized == "":
		sanitized = _random_player_name()
	if player_name_input != null:
		player_name_input.text = sanitized
	return sanitized


func _current_requested_player_name() -> String:
	return _apply_player_name("" if player_name_input == null else player_name_input.text)


func _rebuild_menu_popup() -> void:
	if menu_button == null:
		return

	var popup := menu_button.get_popup()
	popup.clear()
	popup.add_item("Change Name", MENU_ACTION_CHANGE_NAME)
	if app_state.screen == "match" and app_state.is_training_mode():
		popup.add_item("Training Loadout", MENU_ACTION_TRAINING_LOADOUT)
	popup.add_separator()
	popup.add_item("Player Record", MENU_ACTION_PLAYER_RECORD)
	popup.add_item("Roster Watch", MENU_ACTION_ROSTER_WATCH)
	popup.add_item("Event Feed", MENU_ACTION_EVENT_FEED)
	popup.add_item("Diagnostics", MENU_ACTION_DIAGNOSTICS)
	_rendered_menu_popup_signature = _menu_popup_signature()


func _move_skill_catalog_to(host: Control) -> void:
	if skill_pick_panel == null or host == null:
		return
	if skill_pick_panel.get_parent() == host:
		return

	var current_parent := skill_pick_panel.get_parent()
	if current_parent != null:
		current_parent.remove_child(skill_pick_panel)
	host.add_child(skill_pick_panel)


func _open_fullscreen_menu(section: String) -> void:
	_menu_section = section
	fullscreen_menu.visible = true
	name_menu_view.visible = section == MENU_SECTION_NAME
	training_loadout_view.visible = section == MENU_SECTION_LOADOUT
	record_view.visible = section == MENU_SECTION_RECORD
	roster_view.visible = section == MENU_SECTION_ROSTER
	event_view.visible = section == MENU_SECTION_EVENTS
	diagnostics_view.visible = section == MENU_SECTION_DIAGNOSTICS

	match section:
		MENU_SECTION_NAME:
			fullscreen_menu_title.text = "Change Name"
			player_name_input.grab_focus()
			player_name_input.select_all()
		MENU_SECTION_LOADOUT:
			fullscreen_menu_title.text = "Training Loadout"
		MENU_SECTION_RECORD:
			fullscreen_menu_title.text = "Player Record"
		MENU_SECTION_ROSTER:
			fullscreen_menu_title.text = "Roster Watch"
		MENU_SECTION_EVENTS:
			fullscreen_menu_title.text = "Event Feed"
		MENU_SECTION_DIAGNOSTICS:
			fullscreen_menu_title.text = "Diagnostics"
		_:
			fullscreen_menu_title.text = "Menu"

	_refresh_ui()


func _close_fullscreen_menu() -> void:
	_menu_section = ""
	fullscreen_menu.visible = false
	_refresh_ui()


func _on_menu_option_selected(menu_id: int) -> void:
	match menu_id:
		MENU_ACTION_CHANGE_NAME:
			_open_fullscreen_menu(MENU_SECTION_NAME)
		MENU_ACTION_TRAINING_LOADOUT:
			_open_fullscreen_menu(MENU_SECTION_LOADOUT)
		MENU_ACTION_PLAYER_RECORD:
			_open_fullscreen_menu(MENU_SECTION_RECORD)
		MENU_ACTION_ROSTER_WATCH:
			_open_fullscreen_menu(MENU_SECTION_ROSTER)
		MENU_ACTION_EVENT_FEED:
			_open_fullscreen_menu(MENU_SECTION_EVENTS)
		MENU_ACTION_DIAGNOSTICS:
			_open_fullscreen_menu(MENU_SECTION_DIAGNOSTICS)


func _on_name_submitted(_text: String) -> void:
	_on_save_name_pressed()


func _on_randomize_name_pressed() -> void:
	_apply_player_name(_random_player_name())
	_refresh_ui()


func _on_save_name_pressed() -> void:
	var previous_name := _current_requested_player_name()
	var new_name := _apply_player_name(player_name_input.text)
	_close_fullscreen_menu()

	if new_name == previous_name and (
		transport.is_open()
		or _bootstrap_request_active
		or app_state.transport_state == "connecting"
		or app_state.transport_state == "bootstrapping"
	):
		app_state.announce_local("Alias remains %s." % new_name)
		_refresh_ui()
		return

	if transport.is_open() or _bootstrap_request_active or app_state.transport_state == "connecting" or app_state.transport_state == "bootstrapping":
		_cancel_bootstrap_request()
		transport.close()
		app_state.mark_transport_closed("Refreshing realtime session for alias %s." % new_name)
		_queue_auto_connect(0.1)
	else:
		app_state.announce_local("Alias set to %s." % new_name)
		_queue_auto_connect(0.0)

	_refresh_ui()


func _on_connect_pressed() -> void:
	if _bootstrap_request_active or transport.is_open():
		return
	if app_state.transport_state == "connecting" or app_state.transport_state == "bootstrapping":
		return

	var player_name := _current_requested_player_name()
	_next_client_input_tick = 1
	_pending_primary_attack = false
	_pending_cast_slot = 0
	_last_sent_aim = Vector2i.ZERO
	app_state.prepare_for_connection(player_name)
	var bootstrap_url := websocket_config.bootstrap_url(app_state.websocket_url)
	if bootstrap_url == "":
		app_state.mark_transport_closed("Unable to prepare the session bootstrap request.")
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
		_refresh_ui()
		return

	_cancel_bootstrap_request()
	_pending_bootstrap_url = app_state.websocket_url
	var request_error := _bootstrap_request.request(bootstrap_url)
	if request_error != OK:
		_pending_bootstrap_url = ""
		app_state.mark_transport_closed("Unable to request a session token.")
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
		_refresh_ui()
		return

	_bootstrap_request_active = true
	app_state.mark_transport_state("bootstrapping")
	app_state.announce_local("Requested a short-lived session token.")
	_refresh_ui()


func _on_create_lobby_pressed() -> void:
	if transport.send_control_command("CreateGameLobby"):
		app_state.announce_local("Create lobby command sent.")
		_refresh_ui()


func _on_start_training_pressed() -> void:
	if transport.send_control_command("StartTraining"):
		app_state.announce_local("Requested a solo training session.")
		_refresh_ui()


func _on_join_lobby_pressed() -> void:
	var lobby_id := int(join_lobby_input.text.strip_edges())
	_try_join_lobby(lobby_id)


func _on_lobby_directory_meta_clicked(meta: Variant) -> void:
	var lobby_id := int(meta)
	join_lobby_input.text = str(lobby_id)
	_try_join_lobby(lobby_id)


func _try_join_lobby(lobby_id: int) -> void:
	if lobby_id <= 0:
		app_state.mark_transport_error("Lobby ID must be a positive integer.")
		_refresh_ui()
		return
	if transport.send_control_command("JoinGameLobby", {"lobby_id": lobby_id}):
		app_state.announce_local("Join lobby #%d command sent." % lobby_id)
		_refresh_ui()


func _on_team_pressed(team_name: String) -> void:
	if transport.send_control_command("SelectTeam", {"team": team_name}):
		app_state.announce_local("Requested move to %s." % team_name)
		_refresh_ui()


func _on_ready_pressed() -> void:
	var should_ready := app_state.ready_button_text() == "Set Ready"
	if transport.send_control_command("SetReady", {"ready": should_ready}):
		app_state.announce_local("Requested ready state change.")
		_refresh_ui()


func _on_leave_lobby_pressed() -> void:
	if transport.send_control_command("LeaveGameLobby"):
		app_state.announce_local("Requested leave from the current game lobby.")
		_refresh_ui()


func _on_skill_pressed(tree_name: String, tier: int) -> void:
	if transport.send_control_command("ChooseSkill", {"tree": tree_name, "tier": tier}):
		app_state.announce_local("Requested %s %d." % [tree_name, tier])
		_refresh_ui()


func _on_quit_results_pressed() -> void:
	if transport.send_control_command("QuitToCentralLobby"):
		app_state.announce_local("Requested return to the central lobby.")
		_refresh_ui()


func _on_reset_training_pressed() -> void:
	if transport.send_control_command("ResetTrainingSession"):
		app_state.announce_local("Requested a training reset.")
		_refresh_ui()


func _on_quit_arena_pressed() -> void:
	if transport.send_control_command("QuitToCentralLobby"):
		app_state.announce_local("Requested return to the central lobby.")
		_refresh_ui()


func _on_training_loadout_pressed() -> void:
	_open_fullscreen_menu(MENU_SECTION_LOADOUT)


func _on_transport_state_changed(state_name: String) -> void:
	app_state.mark_transport_state(state_name)
	_refresh_ui()


func _on_transport_error(message: String) -> void:
	app_state.mark_transport_error(message)
	if auto_connect_enabled and not transport.is_open() and not _bootstrap_request_active:
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
	_refresh_ui()


func _on_socket_opened() -> void:
	var player_name := _current_requested_player_name()
	if not transport.send_control_command("Connect", {
		"player_name": player_name,
	}):
		app_state.mark_transport_closed("The initial connect command could not be sent.")
		transport.close()
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
	_refresh_ui()


func _on_socket_closed(reason: String) -> void:
	app_state.mark_transport_closed(reason)
	if auto_connect_enabled:
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
	_refresh_ui()


func _on_bootstrap_request_completed(
	result: int,
	response_code: int,
	_headers: PackedStringArray,
	body: PackedByteArray
) -> void:
	_bootstrap_request_active = false
	var expected_signal_url := _pending_bootstrap_url
	_pending_bootstrap_url = ""
	if expected_signal_url == "":
		return

	if result != HTTPRequest.RESULT_SUCCESS:
		app_state.mark_transport_closed("The session bootstrap request failed.")
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
		_refresh_ui()
		return

	var payload_text := body.get_string_from_utf8()
	var payload: Variant = JSON.parse_string(payload_text)
	if response_code != 200:
		var error_message := "The session bootstrap request was rejected."
		if typeof(payload) == TYPE_DICTIONARY:
			error_message = String((payload as Dictionary).get("error", error_message))
		app_state.mark_transport_closed(error_message)
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
		_refresh_ui()
		return

	if typeof(payload) != TYPE_DICTIONARY:
		app_state.mark_transport_closed("The session bootstrap response was not valid JSON.")
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
		_refresh_ui()
		return

	var token := String((payload as Dictionary).get("token", ""))
	if token.strip_edges() == "":
		app_state.mark_transport_closed("The session bootstrap response did not contain a token.")
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
		_refresh_ui()
		return

	var tokenized_url := websocket_config.append_session_token(expected_signal_url, token)
	if not transport.open(tokenized_url):
		app_state.mark_transport_closed("Unable to start the realtime connection.")
		_queue_auto_connect(AUTO_RECONNECT_DELAY_SECONDS)
		_refresh_ui()


func _on_packet_received(decoded_event: Dictionary) -> void:
	var event_data: Dictionary = decoded_event.get("event", {})
	var apply_started_us := PerfClockScript.now_us()
	app_state.apply_server_event(event_data)
	var header: Dictionary = decoded_event.get("header", {})
	var packet_size := int(decoded_event.get("raw_packet_bytes", Protocol.HEADER_LEN + int(header.get("payload_len", 0))))
	app_state.record_inbound_packet(
		header,
		String(event_data.get("type", "Unknown")),
		packet_size,
		int(decoded_event.get("decode_us", 0)),
		PerfClockScript.elapsed_us(apply_started_us)
	)
	_refresh_ui()
	if arena_view != null:
		arena_view.sync_render_state()


func _on_copy_diagnostics_pressed() -> void:
	var diagnostics_text := app_state.diagnostics_text(transport.telemetry_snapshot())
	DisplayServer.clipboard_set(diagnostics_text)
	app_state.announce_local("Copied the structured diagnostics report to the clipboard.")
	_refresh_ui()

func _refresh_ui() -> void:
	var started_us := PerfClockScript.now_us()
	_refresh_ui_impl()
	app_state.record_client_timing("ui_refresh", PerfClockScript.elapsed_us(started_us))
	_passive_ui_refresh_remaining = PASSIVE_UI_REFRESH_INTERVAL_SECONDS


func _refresh_ui_impl() -> void:
	var diagnostics_panel_visible := fullscreen_menu.visible and _menu_section == MENU_SECTION_DIAGNOSTICS
	_refresh_ui_catalog()
	var view_state := _refresh_ui_labels()
	_refresh_ui_logs(view_state)
	_refresh_ui_diagnostics(diagnostics_panel_visible)
	_refresh_ui_buttons(bool(view_state.get("is_training", false)))
	_refresh_ui_visibility(view_state)


func _refresh_ui_catalog() -> void:
	var phase_started_us := PerfClockScript.now_us()
	var skill_catalog_signature := app_state.skill_catalog_signature()
	if skill_catalog_signature != _rendered_skill_catalog_signature:
		_rebuild_skill_buttons()
	var menu_popup_signature := _menu_popup_signature()
	if menu_popup_signature != _rendered_menu_popup_signature:
		_rebuild_menu_popup()
	app_state.record_client_timing("ui_refresh_catalog", PerfClockScript.elapsed_us(phase_started_us))


func _refresh_ui_labels() -> Dictionary:
	var phase_started_us := PerfClockScript.now_us()
	var is_training := app_state.is_training_mode()
	var show_skill_pick := app_state.screen == "match" and app_state.match_phase == "skill_pick"
	var show_training_loadout_menu := _sync_training_loadout_menu(is_training)
	status_label.text = "Transport: %s" % app_state.transport_state.capitalize()
	identity_label.text = "Identity: %s" % _identity_text()
	banner_label.text = app_state.banner_message
	record_label.text = app_state.record_text()
	lobby_label.text = _lobby_label_text()
	lobby_note_label.text = app_state.lobby_note()
	team_label.text = "Current team: %s" % app_state.current_team()
	score_label.text = _match_header_text(is_training)
	countdown_value_label.text = _combat_countdown_text()
	_refresh_skill_pick_summary(is_training, show_skill_pick)
	phase_label.text = _combat_state_heading()
	cooldown_summary_label.text = app_state.cooldown_summary_text()
	training_metrics_label.text = app_state.training_metrics_text()
	outcome_label.text = _results_banner_text()
	lobby_roster_log.text = "\n".join(app_state.lobby_roster_lines())
	app_state.record_client_timing("ui_refresh_labels", PerfClockScript.elapsed_us(phase_started_us))
	return {
		"is_training": is_training,
		"show_skill_pick": show_skill_pick,
		"show_training_loadout_menu": show_training_loadout_menu,
		"round_summary_text": app_state.round_summary_text(),
		"match_summary_text": app_state.match_summary_text(),
	}


func _identity_text() -> String:
	if app_state.local_player_id > 0 and app_state.local_player_name != "":
		return "%s (#%d)" % [app_state.local_player_name, app_state.local_player_id]
	return _current_requested_player_name()


func _lobby_label_text() -> String:
	if app_state.current_lobby_id > 0:
		return "Lobby ID: %d" % app_state.current_lobby_id
	return "Lobby ID: not assigned yet"


func _results_banner_text() -> String:
	if app_state.outcome_label == "":
		return app_state.banner_message
	return "%s\n%s" % [app_state.outcome_label, app_state.banner_message]


func _sync_training_loadout_menu(is_training: bool) -> bool:
	if _menu_section == MENU_SECTION_LOADOUT and not (app_state.screen == "match" and is_training):
		_menu_section = ""
		fullscreen_menu.visible = false
	var show_training_loadout_menu := (
		app_state.screen == "match"
		and is_training
		and fullscreen_menu.visible
		and _menu_section == MENU_SECTION_LOADOUT
	)
	_move_skill_catalog_to(training_loadout_host if show_training_loadout_menu else skill_pick_inline_host)
	return show_training_loadout_menu


func _refresh_skill_pick_summary(is_training: bool, show_skill_pick: bool) -> void:
	if is_training:
		skill_pick_summary_label.text = "Training loadout is live. Click any tier 1-5 skill to replace that slot immediately."
	elif app_state.can_choose_skill():
		skill_pick_summary_label.text = "Choose one legal tier now. Only the next tier in a started tree, or tier 1 in an unstarted tree, is enabled."
	elif show_skill_pick:
		skill_pick_summary_label.text = "Your pick is locked. Waiting for the round to leave the skill-pick phase."
	else:
		skill_pick_summary_label.text = "Skill picks appear here at the start of each round."


func _refresh_ui_logs(view_state: Dictionary) -> void:
	var phase_started_us := PerfClockScript.now_us()
	var round_summary_text := String(view_state.get("round_summary_text", ""))
	var match_summary_text := String(view_state.get("match_summary_text", ""))
	round_summary_log.text = round_summary_text
	match_summary_log.text = match_summary_text
	central_directory_log.text = app_state.lobby_directory_bbcode()
	roster_log.text = "\n".join(app_state.roster_lines())
	event_log.text = app_state.event_log_text()
	app_state.record_client_timing("ui_refresh_logs", PerfClockScript.elapsed_us(phase_started_us))


func _refresh_ui_diagnostics(diagnostics_panel_visible: bool) -> void:
	var phase_started_us := PerfClockScript.now_us()
	if diagnostics_log != null and diagnostics_panel_visible:
		diagnostics_log.text = app_state.diagnostics_text(transport.telemetry_snapshot())
	app_state.record_client_timing("ui_refresh_diagnostics", PerfClockScript.elapsed_us(phase_started_us))


func _refresh_ui_buttons(is_training: bool) -> void:
	var phase_started_us := PerfClockScript.now_us()
	create_lobby_button.disabled = not app_state.can_join_or_create_lobby()
	join_lobby_button.disabled = not app_state.can_join_or_create_lobby()
	start_training_button.disabled = not app_state.can_start_training()
	team_a_button.disabled = not app_state.can_manage_lobby()
	team_b_button.disabled = not app_state.can_manage_lobby()
	ready_button.disabled = not app_state.can_manage_lobby()
	ready_button.text = app_state.ready_button_text()
	leave_lobby_button.disabled = not app_state.can_leave_lobby()
	quit_results_button.disabled = not app_state.can_quit_results()
	training_loadout_button.disabled = not is_training
	reset_training_button.disabled = not app_state.can_reset_training()
	quit_arena_button.disabled = not app_state.can_quit_arena()
	name_save_button.disabled = false
	name_randomize_button.disabled = false

	var skill_button_state_signature := _skill_button_state_signature(is_training)
	if skill_button_state_signature != _rendered_skill_button_state_signature:
		_refresh_skill_buttons(is_training)
	app_state.record_client_timing("ui_refresh_buttons", PerfClockScript.elapsed_us(phase_started_us))


func _refresh_ui_visibility(view_state: Dictionary) -> void:
	var phase_started_us := PerfClockScript.now_us()
	var is_training := bool(view_state.get("is_training", false))
	var show_skill_pick := bool(view_state.get("show_skill_pick", false))
	var show_training_loadout_menu := bool(view_state.get("show_training_loadout_menu", false))
	var round_summary_text := String(view_state.get("round_summary_text", ""))
	var match_summary_text := String(view_state.get("match_summary_text", ""))
	connection_panel.visible = app_state.screen == "central"
	central_panel.visible = app_state.screen == "central"
	lobby_panel.visible = app_state.screen == "lobby"
	match_panel.visible = app_state.screen == "match"
	results_panel.visible = app_state.screen == "results"
	skill_pick_panel.visible = app_state.screen == "match" and (show_skill_pick or show_training_loadout_menu)
	skill_pick_inline_host.visible = app_state.screen == "match" and show_skill_pick and not show_training_loadout_menu
	combat_panel.visible = app_state.screen == "match" and (is_training or not show_skill_pick)
	phase_label.visible = phase_label.text != ""
	countdown_value_label.visible = countdown_value_label.text != ""
	round_summary_log.visible = round_summary_text != ""
	match_summary_log.visible = match_summary_text != ""
	training_metrics_label.visible = is_training
	training_loadout_button.visible = is_training
	reset_training_button.visible = is_training
	quit_arena_button.visible = is_training
	app_state.record_client_timing("ui_refresh_visibility", PerfClockScript.elapsed_us(phase_started_us))


func _rebuild_skill_buttons() -> void:
	_rendered_skill_catalog_signature = app_state.skill_catalog_signature()
	_rendered_skill_button_state_signature = ""
	if skill_columns == null:
		return

	for child in skill_columns.get_children():
		child.queue_free()
	skill_buttons.clear()

	for tree_name in app_state.skill_tree_names():
		var column_panel := PanelContainer.new()
		var column_style := StyleBoxFlat.new()
		column_style.bg_color = Color8(46, 36, 30)
		column_style.border_color = Color8(129, 97, 69)
		column_style.set_border_width_all(1)
		column_style.corner_radius_top_left = 12
		column_style.corner_radius_top_right = 12
		column_style.corner_radius_bottom_right = 12
		column_style.corner_radius_bottom_left = 12
		column_style.content_margin_left = 12
		column_style.content_margin_top = 12
		column_style.content_margin_right = 12
		column_style.content_margin_bottom = 12
		column_panel.add_theme_stylebox_override("panel", column_style)
		column_panel.custom_minimum_size = Vector2(240, 0)
		column_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		skill_columns.add_child(column_panel)

		var column := VBoxContainer.new()
		column.custom_minimum_size = Vector2(220, 0)
		column.add_theme_constant_override("separation", 6)
		column_panel.add_child(column)

		var tree_label := Label.new()
		tree_label.text = tree_name
		tree_label.add_theme_color_override("font_color", Color8(255, 208, 138))
		column.add_child(tree_label)

		var entries := app_state.skill_entries_for(tree_name)
		if entries.is_empty():
			for tier in range(1, 6):
				entries.append({
					"tree": tree_name,
					"tier": tier,
					"skill_name": "%s %d" % [tree_name, tier],
				})

		for entry in entries:
			var tier := int(entry.get("tier", 0))
			var skill_name := String(entry.get("skill_name", "%s %d" % [tree_name, tier]))
			var button := _action_button("%d. %s" % [tier, skill_name], Color8(74, 61, 52))
			var tooltip_text := app_state.skill_tooltip_for(tree_name, tier)
			var category_color := _skill_button_font_color(app_state.skill_ui_category_for(tree_name, tier))
			button.set_meta("tree_name", tree_name)
			button.set_meta("tier", tier)
			button.set_meta("base_tooltip_text", tooltip_text)
			button.pressed.connect(_on_skill_pressed.bind(tree_name, tier))
			_apply_skill_button_font_color(button, category_color)
			button.tooltip_text = tooltip_text
			column.add_child(button)
			skill_buttons.append(button)


func _refresh_skill_buttons(is_training: bool) -> void:
	var can_choose_skill := app_state.can_choose_skill()
	for button in skill_buttons:
		var tree_name := String(button.get_meta("tree_name", ""))
		var tier := int(button.get_meta("tier", 0))
		var base_tooltip_text := String(button.get_meta("base_tooltip_text", ""))
		button.disabled = not app_state.can_choose_skill_option(tree_name, tier)
		if is_training:
			button.tooltip_text = _append_tooltip_note(
				base_tooltip_text,
				"Training equip: replace slot %d immediately." % tier
			)
		elif can_choose_skill:
			var next_tier := app_state.next_skill_tier_for(tree_name)
			if next_tier == 0:
				button.tooltip_text = _append_tooltip_note(
					base_tooltip_text,
					"%s is fully unlocked." % tree_name
				)
			elif tier == next_tier:
				button.tooltip_text = _append_tooltip_note(base_tooltip_text, "Available now.")
			else:
				button.tooltip_text = _append_tooltip_note(
					base_tooltip_text,
					"Only tier %d is currently available for %s." % [next_tier, tree_name]
				)
		else:
			button.tooltip_text = _append_tooltip_note(
				base_tooltip_text,
				"Skill selection is only available during your active skill-pick window."
			)
	_rendered_skill_button_state_signature = _skill_button_state_signature(is_training)


func _menu_popup_signature() -> String:
	return "%s|%s" % [
		app_state.screen,
		str(app_state.is_training_mode()),
	]


func _skill_button_state_signature(is_training: bool) -> String:
	var next_tiers: Array[String] = []
	for tree_name in app_state.skill_tree_names():
		next_tiers.append("%s:%d" % [tree_name, app_state.next_skill_tier_for(tree_name)])
	return "%s|%s|%s|%s|%s|%s" % [
		app_state.transport_state,
		app_state.screen,
		app_state.match_phase,
		str(is_training),
		str(app_state.local_round_skill_locked),
		"|".join(next_tiers),
	]


func _tick_passive_ui_refresh(delta: float) -> void:
	if not _needs_passive_ui_refresh():
		_passive_ui_refresh_remaining = 0.0
		return
	_passive_ui_refresh_remaining -= delta
	if _passive_ui_refresh_remaining > 0.0:
		return
	app_state.record_godot_monitor_snapshot(GodotPerfMonitorsScript.snapshot_builtin_monitors())
	_refresh_ui()


func _needs_passive_ui_refresh() -> bool:
	return (
		app_state.screen == "match"
		or fullscreen_menu.visible
		or _bootstrap_request_active
	)


func _match_header_text(_is_training: bool) -> String:
	var round_number: int = maxi(1, app_state.current_round)
	var header := "Round %d, Team A %d : %d Team B" % [
		round_number,
		app_state.score_a,
		app_state.score_b,
	]
	if not app_state.is_training_mode() and app_state.objective_target_ms > 0:
		header += "\n%s" % ClientStateViewScript.objective_control_text(
			app_state.objective_team_a_ms,
			app_state.objective_team_b_ms,
			app_state.objective_target_ms
		)
	return header


func _combat_state_heading() -> String:
	match app_state.match_phase:
		"combat":
			return "Combat Live"
		"skill_pick":
			return "Skill Pick"
		"pre_combat":
			return "Arena Unlocks"
		"ended":
			return "Round Complete"
		_:
			return ""


func _combat_countdown_text() -> String:
	var raw := app_state.countdown_label.strip_edges()
	if raw == "":
		return ""
	match app_state.match_phase:
		"combat":
			return ""
		"skill_pick":
			return raw.replace("Skill Pick: ", "")
		"pre_combat":
			return raw.replace("Pre-Combat: ", "")
		_:
			return raw


func _append_tooltip_note(base_text: String, note: String) -> String:
	var parts: Array[String] = []
	if base_text != "":
		parts.append(base_text)
	if note != "":
		if not parts.is_empty():
			parts.append("")
		parts.append(note)
	return "\n".join(parts)


func _install_performance_monitors() -> void:
	GodotPerfMonitorsScript.add_or_replace_custom_monitor(
		"Rarena/UIRefreshMs",
		Callable(self, "_custom_monitor_ui_refresh_seconds")
	)
	GodotPerfMonitorsScript.add_or_replace_custom_monitor(
		"Rarena/ArenaDrawMs",
		Callable(self, "_custom_monitor_arena_draw_seconds")
	)
	GodotPerfMonitorsScript.add_or_replace_custom_monitor(
		"Rarena/ArenaVisibilityMs",
		Callable(self, "_custom_monitor_arena_visibility_seconds")
	)
	GodotPerfMonitorsScript.add_or_replace_custom_monitor(
		"Rarena/ArenaBaseDrawMs",
		Callable(self, "_custom_monitor_arena_base_draw_seconds")
	)
	GodotPerfMonitorsScript.add_or_replace_custom_monitor(
		"Rarena/ArenaCacheSyncMs",
		Callable(self, "_custom_monitor_arena_cache_sync_seconds")
	)
	GodotPerfMonitorsScript.add_or_replace_custom_monitor(
		"Rarena/ArenaCacheBackgroundMs",
		Callable(self, "_custom_monitor_arena_cache_background_seconds")
	)
	GodotPerfMonitorsScript.add_or_replace_custom_monitor(
		"Rarena/ArenaCacheVisibilityMs",
		Callable(self, "_custom_monitor_arena_cache_visibility_seconds")
	)
	GodotPerfMonitorsScript.add_or_replace_custom_monitor(
		"Rarena/Players",
		Callable(self, "_custom_monitor_player_count")
	)
	GodotPerfMonitorsScript.add_or_replace_custom_monitor(
		"Rarena/VisibleTiles",
		Callable(self, "_custom_monitor_visible_tile_count")
	)


func _custom_monitor_ui_refresh_seconds() -> float:
	return _timing_bucket_seconds("ui_refresh")


func _custom_monitor_arena_draw_seconds() -> float:
	return _timing_bucket_seconds("arena_draw")


func _custom_monitor_arena_visibility_seconds() -> float:
	return _timing_bucket_seconds("arena_draw_visibility")


func _custom_monitor_arena_base_draw_seconds() -> float:
	return _timing_bucket_seconds("arena_draw_base")


func _custom_monitor_arena_cache_sync_seconds() -> float:
	return _timing_bucket_seconds("arena_draw_cache_sync")


func _custom_monitor_arena_cache_background_seconds() -> float:
	return _timing_bucket_seconds("arena_cache_background")


func _custom_monitor_arena_cache_visibility_seconds() -> float:
	return _timing_bucket_seconds("arena_cache_visibility")


func _custom_monitor_player_count() -> int:
	return int(app_state.diagnostics_snapshot().get("objects", {}).get("players", 0))


func _custom_monitor_visible_tile_count() -> int:
	return int(app_state.diagnostics_snapshot().get("tiles", {}).get("visible", 0))


func _timing_bucket_seconds(metric_name: String) -> float:
	return float(app_state.timing_bucket_last_us(metric_name)) / 1000000.0


func _apply_skill_button_font_color(button: Button, color: Color) -> void:
	button.add_theme_color_override("font_color", color)
	button.add_theme_color_override("font_hover_color", color.lightened(0.08))
	button.add_theme_color_override("font_pressed_color", color)
	button.add_theme_color_override("font_focus_color", color)
	button.add_theme_color_override("font_disabled_color", color.darkened(0.45))


func _skill_button_font_color(category: String) -> Color:
	match category:
		"heal":
			return Color8(120, 228, 138)
		"dot":
			return Color8(188, 120, 255)
		"control":
			return Color8(120, 182, 255)
		"buff":
			return Color8(122, 236, 230)
		"mobility":
			return Color8(255, 194, 112)
		"utility":
			return Color8(244, 214, 133)
		"damage":
			return Color8(255, 156, 124)
		_:
			return Color8(246, 244, 240)


func _queue_combat_cast(slot: int) -> void:
	if not app_state.can_use_combat_slot(slot):
		app_state.mark_transport_error("Skill slot %d is not currently usable." % slot)
		_refresh_ui()
		return
	_pending_cast_slot = slot
	_drive_combat_input()


func _drive_combat_input() -> void:
	if not app_state.can_send_combat_input():
		_pending_primary_attack = false
		_pending_cast_slot = 0
		return

	var move_x := int(Input.is_key_pressed(KEY_D)) - int(Input.is_key_pressed(KEY_A))
	var move_y := int(Input.is_key_pressed(KEY_S)) - int(Input.is_key_pressed(KEY_W))
	var aim := _current_aim_vector()
	var aim_changed := aim != _last_sent_aim
	var should_send := (
		move_x != 0
		or move_y != 0
		or aim_changed
		or _pending_primary_attack
		or _pending_cast_slot > 0
	)
	if not should_send:
		return

	var payload := {
		"client_input_tick": _next_client_input_tick,
		"move_horizontal_q": move_x,
		"move_vertical_q": move_y,
		"aim_horizontal_q": aim.x,
		"aim_vertical_q": aim.y,
		"primary": _pending_primary_attack,
	}
	if _pending_cast_slot > 0:
		payload["cast"] = true
		payload["ability_or_context"] = _pending_cast_slot
		payload["self_cast"] = Input.is_key_pressed(KEY_X)

	if transport.send_input_frame(payload):
		_next_client_input_tick += 1
		_last_sent_aim = aim
		_pending_primary_attack = false
		_pending_cast_slot = 0


func _current_aim_vector() -> Vector2i:
	var player := app_state.local_arena_player()
	if player.is_empty() or arena_view == null or not arena_view.has_arena_snapshot():
		return Vector2i.ZERO

	var world_mouse: Vector2 = arena_view.mouse_world_position() as Vector2
	var delta_x := int(round(world_mouse.x - float(player.get("x", 0))))
	var delta_y := int(round(world_mouse.y - float(player.get("y", 0))))
	return Vector2i(
		clampi(delta_x, Protocol.MIN_I16, Protocol.MAX_I16),
		clampi(delta_y, Protocol.MIN_I16, Protocol.MAX_I16)
	)
