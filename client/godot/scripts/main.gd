extends Control

const ClientStateScript := preload("res://scripts/state/client_state.gd")
const DevSocketClientScript := preload("res://scripts/net/dev_socket_client.gd")

var app_state := ClientStateScript.new()
var transport := DevSocketClientScript.new()

var connect_button: Button
var disconnect_button: Button
var ws_url_input: LineEdit
var player_name_input: LineEdit
var banner_label: Label
var status_label: Label
var record_label: Label
var identity_label: Label
var phase_label: Label
var countdown_value_label: Label
var score_label: Label
var outcome_label: Label
var lobby_label: Label
var lobby_note_label: Label
var team_label: Label
var central_panel: PanelContainer
var lobby_panel: PanelContainer
var match_panel: PanelContainer
var results_panel: PanelContainer
var central_directory_log: RichTextLabel
var roster_log: RichTextLabel
var event_log: RichTextLabel
var join_lobby_input: LineEdit
var ready_button: Button
var leave_lobby_button: Button
var quit_results_button: Button
var create_lobby_button: Button
var join_lobby_button: Button
var team_a_button: Button
var team_b_button: Button
var primary_attack_button: Button
var skill_buttons: Array[Button] = []
var _next_client_input_tick := 1


func _ready() -> void:
	_build_shell()
	_bind_transport()
	_refresh_ui()


func _process(_delta: float) -> void:
	transport.poll()


func _bind_transport() -> void:
	transport.opened.connect(_on_socket_opened)
	transport.closed.connect(_on_socket_closed)
	transport.transport_state_changed.connect(_on_transport_state_changed)
	transport.transport_error.connect(_on_transport_error)
	transport.packet_received.connect(_on_packet_received)


func _build_shell() -> void:
	var base := ColorRect.new()
	base.color = Color8(10, 18, 24)
	base.anchor_right = 1.0
	base.anchor_bottom = 1.0
	base.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(base)

	var glow_top := ColorRect.new()
	glow_top.color = Color8(189, 105, 57, 32)
	glow_top.anchor_right = 1.0
	glow_top.anchor_bottom = 0.35
	glow_top.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(glow_top)

	var glow_side := ColorRect.new()
	glow_side.color = Color8(40, 132, 163, 28)
	glow_side.anchor_left = 0.66
	glow_side.anchor_right = 1.0
	glow_side.anchor_bottom = 1.0
	glow_side.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(glow_side)

	var margin := MarginContainer.new()
	margin.anchor_right = 1.0
	margin.anchor_bottom = 1.0
	margin.add_theme_constant_override("margin_left", 28)
	margin.add_theme_constant_override("margin_top", 24)
	margin.add_theme_constant_override("margin_right", 28)
	margin.add_theme_constant_override("margin_bottom", 24)
	add_child(margin)

	var root_column := VBoxContainer.new()
	root_column.add_theme_constant_override("separation", 18)
	margin.add_child(root_column)

	root_column.add_child(_build_header())
	root_column.add_child(_build_connection_panel())
	root_column.add_child(_build_body())


func _build_header() -> Control:
	var wrapper := VBoxContainer.new()
	wrapper.add_theme_constant_override("separation", 8)

	var title := Label.new()
	title.text = "Rusaren Control Shell"
	title.add_theme_font_size_override("font_size", 34)
	title.add_theme_color_override("font_color", Color8(240, 232, 219))
	wrapper.add_child(title)

	var subtitle := Label.new()
	subtitle.text = "Godot web shell wired to the current websocket dev adapter. Gameplay rendering stays placeholder while the backend packet surface stabilizes."
	subtitle.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	subtitle.add_theme_color_override("font_color", Color8(184, 191, 198))
	wrapper.add_child(subtitle)

	var badge_row := HBoxContainer.new()
	badge_row.add_theme_constant_override("separation", 12)
	wrapper.add_child(badge_row)

	status_label = Label.new()
	status_label.add_theme_font_size_override("font_size", 14)
	status_label.add_theme_color_override("font_color", Color8(255, 214, 102))
	badge_row.add_child(status_label)

	identity_label = Label.new()
	identity_label.add_theme_color_override("font_color", Color8(128, 201, 255))
	badge_row.add_child(identity_label)

	return wrapper


func _build_connection_panel() -> Control:
	var panel := _make_panel(Color8(29, 42, 53), Color8(92, 120, 143))
	var body := panel.get_meta("body") as VBoxContainer

	var heading := Label.new()
	heading.text = "Dev adapter"
	heading.add_theme_font_size_override("font_size", 19)
	heading.add_theme_color_override("font_color", Color8(244, 239, 232))
	body.add_child(heading)

	banner_label = Label.new()
	banner_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	banner_label.add_theme_color_override("font_color", Color8(214, 192, 154))
	body.add_child(banner_label)

	var grid := GridContainer.new()
	grid.columns = 3
	grid.add_theme_constant_override("h_separation", 14)
	grid.add_theme_constant_override("v_separation", 10)
	body.add_child(grid)

	ws_url_input = _labeled_line_edit(grid, "WebSocket URL", app_state.websocket_url)
	player_name_input = _labeled_line_edit(grid, "Player name", "Alice")
	join_lobby_input = _labeled_line_edit(grid, "Join lobby ID", "")

	var button_row := HBoxContainer.new()
	button_row.add_theme_constant_override("separation", 10)
	body.add_child(button_row)

	connect_button = _action_button("Connect", Color8(28, 102, 82))
	connect_button.pressed.connect(_on_connect_pressed)
	button_row.add_child(connect_button)

	disconnect_button = _action_button("Disconnect", Color8(116, 47, 47))
	disconnect_button.pressed.connect(_on_disconnect_pressed)
	button_row.add_child(disconnect_button)

	create_lobby_button = _action_button("Create Lobby", Color8(45, 74, 126))
	create_lobby_button.pressed.connect(_on_create_lobby_pressed)
	button_row.add_child(create_lobby_button)

	join_lobby_button = _action_button("Join Lobby", Color8(102, 72, 28))
	join_lobby_button.pressed.connect(_on_join_lobby_pressed)
	button_row.add_child(join_lobby_button)

	return panel


func _build_body() -> Control:
	var split := HSplitContainer.new()
	split.size_flags_vertical = Control.SIZE_EXPAND_FILL
	split.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	split.split_offset = 840

	var left_column := VBoxContainer.new()
	left_column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	left_column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	left_column.add_theme_constant_override("separation", 14)
	split.add_child(left_column)

	central_panel = _build_central_panel()
	lobby_panel = _build_lobby_panel()
	match_panel = _build_match_panel()
	results_panel = _build_results_panel()
	left_column.add_child(central_panel)
	left_column.add_child(lobby_panel)
	left_column.add_child(match_panel)
	left_column.add_child(results_panel)

	var right_column := VBoxContainer.new()
	right_column.custom_minimum_size = Vector2(360, 0)
	right_column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	right_column.add_theme_constant_override("separation", 14)
	split.add_child(right_column)

	right_column.add_child(_build_record_panel())
	right_column.add_child(_build_roster_panel())
	right_column.add_child(_build_event_panel())
	return split


func _build_central_panel() -> PanelContainer:
	var panel := _make_panel(Color8(32, 45, 58), Color8(75, 111, 138))
	var body := panel.get_meta("body") as VBoxContainer

	var title := Label.new()
	title.text = "Central Lobby"
	title.add_theme_font_size_override("font_size", 22)
	title.add_theme_color_override("font_color", Color8(247, 241, 233))
	body.add_child(title)

	var summary := Label.new()
	summary.text = "Create a game lobby or click one from the directory below to join it. Browser exports default to the same-origin /ws endpoint when you leave the URL field on its default value."
	summary.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	summary.add_theme_color_override("font_color", Color8(187, 196, 203))
	body.add_child(summary)

	var prompt := Label.new()
	prompt.text = "Connection, identity, and join controls live in the dev adapter panel above."
	prompt.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	prompt.add_theme_color_override("font_color", Color8(164, 178, 188))
	body.add_child(prompt)

	var list_title := Label.new()
	list_title.text = "Active game lobbies"
	list_title.add_theme_color_override("font_color", Color8(244, 233, 216))
	body.add_child(list_title)

	central_directory_log = RichTextLabel.new()
	central_directory_log.bbcode_enabled = true
	central_directory_log.fit_content = true
	central_directory_log.meta_underlined = true
	central_directory_log.scroll_active = true
	central_directory_log.custom_minimum_size = Vector2(0, 150)
	central_directory_log.add_theme_color_override("default_color", Color8(222, 230, 236))
	central_directory_log.meta_clicked.connect(_on_lobby_directory_meta_clicked)
	body.add_child(central_directory_log)

	return panel


func _build_lobby_panel() -> PanelContainer:
	var panel := _make_panel(Color8(33, 50, 44), Color8(86, 144, 124))
	var body := panel.get_meta("body") as VBoxContainer

	var title := Label.new()
	title.text = "Game Lobby"
	title.add_theme_font_size_override("font_size", 22)
	title.add_theme_color_override("font_color", Color8(243, 247, 243))
	body.add_child(title)

	lobby_label = Label.new()
	lobby_label.add_theme_font_size_override("font_size", 18)
	lobby_label.add_theme_color_override("font_color", Color8(155, 230, 189))
	body.add_child(lobby_label)

	lobby_note_label = Label.new()
	lobby_note_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	lobby_note_label.add_theme_color_override("font_color", Color8(190, 203, 196))
	body.add_child(lobby_note_label)

	team_label = Label.new()
	team_label.add_theme_color_override("font_color", Color8(240, 241, 220))
	body.add_child(team_label)

	var team_row := HBoxContainer.new()
	team_row.add_theme_constant_override("separation", 10)
	body.add_child(team_row)

	team_a_button = _action_button("Join Team A", Color8(35, 82, 138))
	team_a_button.pressed.connect(_on_team_pressed.bind("Team A"))
	team_row.add_child(team_a_button)

	team_b_button = _action_button("Join Team B", Color8(138, 56, 35))
	team_b_button.pressed.connect(_on_team_pressed.bind("Team B"))
	team_row.add_child(team_b_button)

	var lobby_action_row := HBoxContainer.new()
	lobby_action_row.add_theme_constant_override("separation", 10)
	body.add_child(lobby_action_row)

	ready_button = _action_button("Set Ready", Color8(28, 102, 82))
	ready_button.pressed.connect(_on_ready_pressed)
	lobby_action_row.add_child(ready_button)

	leave_lobby_button = _action_button("Leave Lobby", Color8(102, 57, 28))
	leave_lobby_button.pressed.connect(_on_leave_lobby_pressed)
	lobby_action_row.add_child(leave_lobby_button)

	return panel


func _build_match_panel() -> PanelContainer:
	var panel := _make_panel(Color8(52, 39, 33), Color8(162, 112, 73))
	var body := panel.get_meta("body") as VBoxContainer

	var title := Label.new()
	title.text = "Match Shell"
	title.add_theme_font_size_override("font_size", 22)
	title.add_theme_color_override("font_color", Color8(248, 236, 224))
	body.add_child(title)

	phase_label = Label.new()
	phase_label.add_theme_font_size_override("font_size", 18)
	phase_label.add_theme_color_override("font_color", Color8(255, 216, 156))
	body.add_child(phase_label)

	score_label = Label.new()
	score_label.add_theme_font_size_override("font_size", 16)
	score_label.add_theme_color_override("font_color", Color8(240, 241, 220))
	body.add_child(score_label)

	countdown_value_label = Label.new()
	countdown_value_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	countdown_value_label.add_theme_color_override("font_color", Color8(203, 197, 192))
	body.add_child(countdown_value_label)

	var placeholder := Label.new()
	placeholder.text = "Combat presentation stays intentionally thin in this slice. This shell now sends real input-frame packets, but the current backend slice still resolves combat with a placeholder one-hit primary attack."
	placeholder.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	placeholder.add_theme_color_override("font_color", Color8(179, 180, 174))
	body.add_child(placeholder)

	var combat_title := Label.new()
	combat_title.text = "Combat controls"
	combat_title.add_theme_color_override("font_color", Color8(244, 233, 216))
	body.add_child(combat_title)

	var combat_row := HBoxContainer.new()
	combat_row.add_theme_constant_override("separation", 10)
	body.add_child(combat_row)

	primary_attack_button = _action_button("Primary Attack", Color8(120, 78, 34))
	primary_attack_button.pressed.connect(_on_primary_attack_pressed)
	combat_row.add_child(primary_attack_button)

	var skill_title := Label.new()
	skill_title.text = "Skill picks"
	skill_title.add_theme_color_override("font_color", Color8(244, 233, 216))
	body.add_child(skill_title)

	var skill_columns := HBoxContainer.new()
	skill_columns.add_theme_constant_override("separation", 10)
	body.add_child(skill_columns)

	for tree_name in ["Warrior", "Rogue", "Mage", "Cleric"]:
		var column := VBoxContainer.new()
		column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		column.add_theme_constant_override("separation", 6)
		skill_columns.add_child(column)

		var tree_label := Label.new()
		tree_label.text = tree_name
		tree_label.add_theme_color_override("font_color", Color8(255, 208, 138))
		column.add_child(tree_label)

		for tier in range(1, 6):
			var button := _action_button("Tier %d" % tier, Color8(74, 61, 52))
			button.set_meta("tree_name", tree_name)
			button.set_meta("tier", tier)
			button.pressed.connect(_on_skill_pressed.bind(tree_name, tier))
			column.add_child(button)
			skill_buttons.append(button)

	return panel


func _build_results_panel() -> PanelContainer:
	var panel := _make_panel(Color8(47, 30, 45), Color8(128, 73, 141))
	var body := panel.get_meta("body") as VBoxContainer

	var title := Label.new()
	title.text = "Results"
	title.add_theme_font_size_override("font_size", 22)
	title.add_theme_color_override("font_color", Color8(245, 233, 244))
	body.add_child(title)

	outcome_label = Label.new()
	outcome_label.add_theme_font_size_override("font_size", 20)
	outcome_label.add_theme_color_override("font_color", Color8(244, 192, 236))
	outcome_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	body.add_child(outcome_label)

	quit_results_button = _action_button("Quit To Central Lobby", Color8(95, 61, 104))
	quit_results_button.pressed.connect(_on_quit_results_pressed)
	body.add_child(quit_results_button)

	return panel


func _build_record_panel() -> PanelContainer:
	var panel := _make_panel(Color8(24, 34, 44), Color8(78, 108, 128))
	var body := panel.get_meta("body") as VBoxContainer

	var title := Label.new()
	title.text = "Player Record"
	title.add_theme_font_size_override("font_size", 20)
	title.add_theme_color_override("font_color", Color8(246, 242, 236))
	body.add_child(title)

	var info := Label.new()
	info.text = "The server remains authoritative. W-L-NC updates only when the backend sends Connected or ReturnedToCentralLobby."
	info.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	info.add_theme_color_override("font_color", Color8(180, 188, 194))
	body.add_child(info)

	record_label = Label.new()
	record_label.add_theme_font_size_override("font_size", 26)
	record_label.add_theme_color_override("font_color", Color8(255, 217, 122))
	body.add_child(record_label)

	return panel


func _build_roster_panel() -> PanelContainer:
	var panel := _make_panel(Color8(25, 44, 41), Color8(74, 135, 124))
	var body := panel.get_meta("body") as VBoxContainer
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL

	var title := Label.new()
	title.text = "Roster Watch"
	title.add_theme_font_size_override("font_size", 20)
	title.add_theme_color_override("font_color", Color8(242, 247, 244))
	body.add_child(title)

	var note := Label.new()
	note.text = "Authoritative roster built from the current backend snapshot plus live updates."
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.add_theme_color_override("font_color", Color8(179, 199, 192))
	body.add_child(note)

	roster_log = RichTextLabel.new()
	roster_log.fit_content = true
	roster_log.scroll_active = true
	roster_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	roster_log.add_theme_color_override("default_color", Color8(226, 237, 233))
	body.add_child(roster_log)

	return panel


func _build_event_panel() -> PanelContainer:
	var panel := _make_panel(Color8(28, 28, 35), Color8(95, 95, 121))
	var body := panel.get_meta("body") as VBoxContainer
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL

	var title := Label.new()
	title.text = "Event Feed"
	title.add_theme_font_size_override("font_size", 20)
	title.add_theme_color_override("font_color", Color8(243, 243, 248))
	body.add_child(title)

	event_log = RichTextLabel.new()
	event_log.fit_content = true
	event_log.scroll_active = true
	event_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	event_log.add_theme_color_override("default_color", Color8(212, 214, 226))
	body.add_child(event_log)

	return panel


func _make_panel(background: Color, border: Color) -> PanelContainer:
	var panel := PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var style := StyleBoxFlat.new()
	style.bg_color = background
	style.border_color = border
	style.set_border_width_all(2)
	style.corner_radius_top_left = 18
	style.corner_radius_top_right = 18
	style.corner_radius_bottom_right = 18
	style.corner_radius_bottom_left = 18
	style.content_margin_left = 16
	style.content_margin_top = 16
	style.content_margin_right = 16
	style.content_margin_bottom = 16
	panel.add_theme_stylebox_override("panel", style)

	var body := VBoxContainer.new()
	body.add_theme_constant_override("separation", 10)
	panel.add_child(body)
	panel.set_meta("body", body)
	return panel


func _action_button(text: String, color: Color) -> Button:
	var button := Button.new()
	button.text = text
	button.custom_minimum_size = Vector2(0, 42)
	var normal := StyleBoxFlat.new()
	normal.bg_color = color
	normal.corner_radius_top_left = 12
	normal.corner_radius_top_right = 12
	normal.corner_radius_bottom_right = 12
	normal.corner_radius_bottom_left = 12
	normal.border_color = color.lightened(0.18)
	normal.set_border_width_all(1)
	button.add_theme_stylebox_override("normal", normal)

	var hover := normal.duplicate()
	hover.bg_color = color.lightened(0.08)
	button.add_theme_stylebox_override("hover", hover)

	var disabled := normal.duplicate()
	disabled.bg_color = color.darkened(0.55)
	disabled.border_color = color.darkened(0.35)
	button.add_theme_stylebox_override("disabled", disabled)
	button.add_theme_color_override("font_color", Color8(246, 244, 240))
	return button


func _labeled_line_edit(parent: Control, label_text: String, default_value: String) -> LineEdit:
	var wrapper := VBoxContainer.new()
	wrapper.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	parent.add_child(wrapper)

	var label := Label.new()
	label.text = label_text
	label.add_theme_color_override("font_color", Color8(205, 214, 221))
	wrapper.add_child(label)

	var input := LineEdit.new()
	input.text = default_value
	input.placeholder_text = label_text
	wrapper.add_child(input)
	return input


func _on_connect_pressed() -> void:
	var url := ws_url_input.text.strip_edges()
	var player_name := player_name_input.text.strip_edges()
	_next_client_input_tick = 1
	app_state.prepare_for_connection(url, player_name)
	ws_url_input.text = app_state.websocket_url
	_refresh_ui()
	if not transport.open(app_state.websocket_url):
		app_state.mark_transport_error("Unable to start the websocket connection.")
		_refresh_ui()


func _on_disconnect_pressed() -> void:
	_next_client_input_tick = 1
	transport.close()
	app_state.mark_transport_closed("Disconnected by the local client.")
	_refresh_ui()


func _on_create_lobby_pressed() -> void:
	if transport.send_control_command("CreateGameLobby"):
		app_state.announce_local("Create lobby command sent.")
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


func _on_primary_attack_pressed() -> void:
	var payload := {
		"client_input_tick": _next_client_input_tick,
		"move_horizontal_q": 0,
		"move_vertical_q": 0,
		"aim_horizontal_q": 0,
		"aim_vertical_q": 0,
		"primary": true,
	}
	if transport.send_input_frame(payload):
		_next_client_input_tick += 1
		app_state.announce_local("Requested primary attack.")
		_refresh_ui()


func _on_transport_state_changed(state_name: String) -> void:
	app_state.mark_transport_state(state_name)
	_refresh_ui()


func _on_transport_error(message: String) -> void:
	app_state.mark_transport_error(message)
	_refresh_ui()


func _on_socket_opened() -> void:
	var player_name := player_name_input.text.strip_edges()
	if not transport.send_control_command("Connect", {
		"player_name": player_name,
	}):
		app_state.mark_transport_error("The initial connect command could not be sent.")
	_refresh_ui()


func _on_socket_closed(reason: String) -> void:
	app_state.mark_transport_closed(reason)
	_refresh_ui()


func _on_packet_received(decoded_event: Dictionary) -> void:
	app_state.apply_server_event(decoded_event.get("event", {}))
	_refresh_ui()


func _refresh_ui() -> void:
	var identity_text := "Not connected"
	if app_state.local_player_id > 0 and app_state.local_player_name != "":
		identity_text = "%s (#%d)" % [app_state.local_player_name, app_state.local_player_id]
	var lobby_text := "Lobby ID: not assigned yet"
	if app_state.current_lobby_id > 0:
		lobby_text = "Lobby ID: %d" % app_state.current_lobby_id
	var result_text := app_state.banner_message
	if app_state.outcome_label != "":
		result_text = "%s\n%s" % [app_state.outcome_label, app_state.banner_message]

	status_label.text = "Transport: %s" % app_state.transport_state.capitalize()
	identity_label.text = "Identity: %s" % identity_text
	banner_label.text = app_state.banner_message
	record_label.text = app_state.record_text()
	lobby_label.text = lobby_text
	lobby_note_label.text = app_state.lobby_note()
	team_label.text = "Current team: %s" % app_state.current_team()
	phase_label.text = app_state.phase_label
	score_label.text = app_state.score_text()
	countdown_value_label.text = app_state.countdown_label
	outcome_label.text = result_text
	central_directory_log.text = app_state.lobby_directory_bbcode()
	roster_log.text = "\n".join(app_state.roster_lines())
	event_log.text = app_state.event_log_text()

	connect_button.disabled = app_state.transport_state == "connecting" or transport.is_open()
	disconnect_button.disabled = not transport.is_open() and app_state.transport_state != "connecting"
	create_lobby_button.disabled = not app_state.can_join_or_create_lobby()
	join_lobby_button.disabled = not app_state.can_join_or_create_lobby()
	team_a_button.disabled = not app_state.can_manage_lobby()
	team_b_button.disabled = not app_state.can_manage_lobby()
	ready_button.disabled = not app_state.can_manage_lobby()
	ready_button.text = app_state.ready_button_text()
	leave_lobby_button.disabled = not app_state.can_leave_lobby()
	quit_results_button.disabled = not app_state.can_quit_results()
	primary_attack_button.disabled = not app_state.can_send_combat_input()
	for button in skill_buttons:
		var tree_name := String(button.get_meta("tree_name", ""))
		var tier := int(button.get_meta("tier", 0))
		var selectable := app_state.can_choose_skill_option(tree_name, tier)
		button.disabled = not selectable
		if app_state.can_choose_skill():
			var next_tier := app_state.next_skill_tier_for(tree_name)
			if next_tier == 0:
				button.tooltip_text = "%s is fully unlocked." % tree_name
			elif tier == next_tier:
				button.tooltip_text = "Select %s tier %d." % [tree_name, tier]
			else:
				button.tooltip_text = "Only tier %d is currently available for %s." % [next_tier, tree_name]
		else:
			button.tooltip_text = "Skill selection is only available during your active skill-pick window."

	central_panel.visible = app_state.screen == "central"
	lobby_panel.visible = app_state.screen == "lobby"
	match_panel.visible = app_state.screen == "match"
	results_panel.visible = app_state.screen == "results"
