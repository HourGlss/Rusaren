extends Control

const ClientStateScript := preload("res://scripts/state/client_state.gd")
const DevSocketClientScript := preload("res://scripts/net/dev_socket_client.gd")
const ArenaViewScript := preload("res://scripts/arena/arena_view.gd")
const Protocol := preload("res://scripts/net/protocol.gd")
const WebSocketConfigScript := preload("res://scripts/net/websocket_config.gd")

const MENU_SECTION_NAME := "name"
const MENU_SECTION_RECORD := "record"
const MENU_SECTION_ROSTER := "roster"
const MENU_SECTION_EVENTS := "events"

const MENU_ACTION_CHANGE_NAME := 1
const MENU_ACTION_PLAYER_RECORD := 2
const MENU_ACTION_ROSTER_WATCH := 3
const MENU_ACTION_EVENT_FEED := 4

const AUTO_RECONNECT_DELAY_SECONDS := 2.0
const RANDOM_PLAYER_NAME_LENGTH := 10
const PLAYER_NAME_ALPHABET := "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"

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
var record_view: VBoxContainer
var roster_view: VBoxContainer
var event_view: VBoxContainer
var banner_label: Label
var status_label: Label
var record_label: Label
var identity_label: Label
var phase_label: Label
var countdown_value_label: Label
var combat_hint_label: Label
var cooldown_summary_label: Label
var score_label: Label
var outcome_label: Label
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
var join_lobby_input: LineEdit
var ready_button: Button
var leave_lobby_button: Button
var quit_results_button: Button
var create_lobby_button: Button
var join_lobby_button: Button
var team_a_button: Button
var team_b_button: Button
var primary_attack_button: Button
var name_save_button: Button
var name_randomize_button: Button
var skill_pick_panel: PanelContainer
var skill_pick_summary_label: Label
var skill_scroll: ScrollContainer
var skill_columns: GridContainer
var combat_panel: VBoxContainer
var arena_view = null
var skill_buttons: Array[Button] = []
var _rendered_skill_catalog_signature := ""
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


func _ready() -> void:
	_name_rng.randomize()
	_bootstrap_request = HTTPRequest.new()
	add_child(_bootstrap_request)
	_bootstrap_request.request_completed.connect(_on_bootstrap_request_completed)
	_build_shell()
	_bind_transport()
	_apply_player_name(_random_player_name())
	_refresh_ui()
	if auto_connect_enabled:
		_queue_auto_connect(0.0)


func _process(delta: float) -> void:
	app_state.advance_visuals(delta)
	transport.poll()
	_tick_auto_connect(delta)
	_drive_combat_input()
	_refresh_ui()


func _input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo and event.keycode == KEY_QUOTELEFT:
		_toggle_debug_mode()
		return

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
	root_column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root_column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root_column.add_theme_constant_override("separation", 18)
	margin.add_child(root_column)

	root_column.add_child(_build_top_bar())
	root_column.add_child(_build_body())
	add_child(_build_fullscreen_menu())


func _build_top_bar() -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 12)

	status_label = Label.new()
	status_label.add_theme_font_size_override("font_size", 14)
	status_label.add_theme_color_override("font_color", Color8(255, 214, 102))
	row.add_child(status_label)

	identity_label = Label.new()
	identity_label.add_theme_color_override("font_color", Color8(128, 201, 255))
	row.add_child(identity_label)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(spacer)

	menu_button = MenuButton.new()
	menu_button.text = "Menu"
	menu_button.custom_minimum_size = Vector2(132, 42)
	_style_clickable(menu_button, Color8(53, 73, 94))
	row.add_child(menu_button)

	var popup := menu_button.get_popup()
	popup.add_item("Change Name", MENU_ACTION_CHANGE_NAME)
	popup.add_separator()
	popup.add_item("Player Record", MENU_ACTION_PLAYER_RECORD)
	popup.add_item("Roster Watch", MENU_ACTION_ROSTER_WATCH)
	popup.add_item("Event Feed", MENU_ACTION_EVENT_FEED)
	popup.id_pressed.connect(_on_menu_option_selected)

	return row


func _build_body() -> Control:
	var column := VBoxContainer.new()
	column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_theme_constant_override("separation", 14)

	connection_panel = _build_connection_panel()
	central_panel = _build_central_panel()
	lobby_panel = _build_lobby_panel()
	match_panel = _build_match_panel()
	results_panel = _build_results_panel()

	column.add_child(connection_panel)
	column.add_child(central_panel)
	column.add_child(lobby_panel)
	column.add_child(match_panel)
	column.add_child(results_panel)
	return column


func _build_connection_panel() -> PanelContainer:
	var panel := _make_panel(Color8(29, 42, 53), Color8(92, 120, 143))
	connection_panel = panel
	var body := panel.get_meta("body") as VBoxContainer

	var heading := Label.new()
	heading.text = "Central Actions"
	heading.add_theme_font_size_override("font_size", 19)
	heading.add_theme_color_override("font_color", Color8(244, 239, 232))
	body.add_child(heading)

	banner_label = Label.new()
	banner_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	banner_label.add_theme_color_override("font_color", Color8(214, 192, 154))
	body.add_child(banner_label)

	var note := Label.new()
	note.text = "The client now connects automatically. Use Menu to change your alias or open record, roster, and event views."
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.add_theme_color_override("font_color", Color8(184, 191, 198))
	body.add_child(note)

	var controls := HBoxContainer.new()
	controls.add_theme_constant_override("separation", 12)
	body.add_child(controls)

	var join_wrapper := VBoxContainer.new()
	join_wrapper.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	controls.add_child(join_wrapper)

	var join_label := Label.new()
	join_label.text = "Join lobby ID"
	join_label.add_theme_color_override("font_color", Color8(205, 214, 221))
	join_wrapper.add_child(join_label)

	join_lobby_input = LineEdit.new()
	join_lobby_input.placeholder_text = "Join lobby ID"
	join_wrapper.add_child(join_lobby_input)

	create_lobby_button = _action_button("Create Lobby", Color8(45, 74, 126))
	create_lobby_button.pressed.connect(_on_create_lobby_pressed)
	controls.add_child(create_lobby_button)

	join_lobby_button = _action_button("Join Lobby", Color8(102, 72, 28))
	join_lobby_button.pressed.connect(_on_join_lobby_pressed)
	controls.add_child(join_lobby_button)

	return panel


func _build_central_panel() -> PanelContainer:
	var panel := _make_panel(Color8(32, 45, 58), Color8(75, 111, 138))
	var body := panel.get_meta("body") as VBoxContainer

	var title := Label.new()
	title.text = "Central Lobby"
	title.add_theme_font_size_override("font_size", 22)
	title.add_theme_color_override("font_color", Color8(247, 241, 233))
	body.add_child(title)

	var summary := Label.new()
	summary.text = "Create a game lobby or click one from the directory below to join it. The browser shell stays same-origin, auto-connects, and hands live gameplay to WebRTC once signaling finishes."
	summary.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	summary.add_theme_color_override("font_color", Color8(187, 196, 203))
	body.add_child(summary)

	var list_title := Label.new()
	list_title.text = "Active game lobbies"
	list_title.add_theme_color_override("font_color", Color8(244, 233, 216))
	body.add_child(list_title)

	central_directory_log = RichTextLabel.new()
	central_directory_log.bbcode_enabled = true
	central_directory_log.fit_content = true
	central_directory_log.meta_underlined = true
	central_directory_log.scroll_active = true
	central_directory_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	central_directory_log.custom_minimum_size = Vector2(0, 220)
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

	var roster_title := Label.new()
	roster_title.text = "Lobby roster"
	roster_title.add_theme_color_override("font_color", Color8(228, 240, 228))
	body.add_child(roster_title)

	lobby_roster_log = RichTextLabel.new()
	lobby_roster_log.fit_content = true
	lobby_roster_log.scroll_active = false
	lobby_roster_log.custom_minimum_size = Vector2(0, 120)
	lobby_roster_log.add_theme_color_override("default_color", Color8(221, 234, 223))
	body.add_child(lobby_roster_log)

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

	skill_pick_panel = _make_panel(Color8(60, 47, 38), Color8(191, 135, 88))
	var skill_pick_body := skill_pick_panel.get_meta("body") as VBoxContainer

	var skill_pick_title := Label.new()
	skill_pick_title.text = "Skill picks"
	skill_pick_title.add_theme_color_override("font_color", Color8(252, 235, 209))
	skill_pick_title.add_theme_font_size_override("font_size", 18)
	skill_pick_body.add_child(skill_pick_title)

	skill_pick_summary_label = Label.new()
	skill_pick_summary_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	skill_pick_summary_label.add_theme_color_override("font_color", Color8(222, 210, 192))
	skill_pick_body.add_child(skill_pick_summary_label)

	skill_scroll = ScrollContainer.new()
	skill_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	skill_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	skill_scroll.custom_minimum_size = Vector2(0, 360)
	skill_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	skill_pick_body.add_child(skill_scroll)

	skill_columns = GridContainer.new()
	skill_columns.columns = 3
	skill_columns.add_theme_constant_override("separation", 10)
	skill_columns.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	skill_scroll.add_child(skill_columns)
	_rebuild_skill_buttons()

	body.add_child(skill_pick_panel)

	combat_panel = VBoxContainer.new()
	combat_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	combat_panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	combat_panel.add_theme_constant_override("separation", 10)
	body.add_child(combat_panel)

	var placeholder := Label.new()
	placeholder.text = "The first arena slice is live here: a mostly empty map, central shrub-encased pillars, authoritative snapshots, WASD movement, mouse aim, left-click melee, and authored combat skills on 1-5."
	placeholder.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	placeholder.add_theme_color_override("font_color", Color8(179, 180, 174))
	combat_panel.add_child(placeholder)

	arena_view = ArenaViewScript.new()
	arena_view.custom_minimum_size = Vector2(0, 460)
	arena_view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	arena_view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	arena_view.set_client_state(app_state)
	combat_panel.add_child(arena_view)

	combat_hint_label = Label.new()
	combat_hint_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	combat_hint_label.add_theme_color_override("font_color", Color8(214, 218, 208))
	combat_panel.add_child(combat_hint_label)

	cooldown_summary_label = Label.new()
	cooldown_summary_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	cooldown_summary_label.add_theme_color_override("font_color", Color8(183, 204, 214))
	combat_panel.add_child(cooldown_summary_label)

	var combat_title := Label.new()
	combat_title.text = "Combat controls"
	combat_title.add_theme_color_override("font_color", Color8(244, 233, 216))
	combat_panel.add_child(combat_title)

	var combat_row := HBoxContainer.new()
	combat_row.add_theme_constant_override("separation", 10)
	combat_panel.add_child(combat_row)

	primary_attack_button = _action_button("Primary Attack", Color8(120, 78, 34))
	primary_attack_button.pressed.connect(_on_primary_attack_pressed)
	combat_row.add_child(primary_attack_button)

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


func _build_fullscreen_menu() -> Control:
	var overlay := Control.new()
	overlay.anchor_right = 1.0
	overlay.anchor_bottom = 1.0
	overlay.visible = false
	overlay.mouse_filter = Control.MOUSE_FILTER_STOP
	fullscreen_menu = overlay

	var shade := ColorRect.new()
	shade.color = Color8(6, 10, 14, 210)
	shade.anchor_right = 1.0
	shade.anchor_bottom = 1.0
	shade.mouse_filter = Control.MOUSE_FILTER_STOP
	overlay.add_child(shade)

	var margin := MarginContainer.new()
	margin.anchor_right = 1.0
	margin.anchor_bottom = 1.0
	margin.add_theme_constant_override("margin_left", 24)
	margin.add_theme_constant_override("margin_top", 24)
	margin.add_theme_constant_override("margin_right", 24)
	margin.add_theme_constant_override("margin_bottom", 24)
	overlay.add_child(margin)

	var panel := PanelContainer.new()
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var style := StyleBoxFlat.new()
	style.bg_color = Color8(18, 28, 36)
	style.border_color = Color8(87, 119, 143)
	style.set_border_width_all(2)
	style.corner_radius_top_left = 20
	style.corner_radius_top_right = 20
	style.corner_radius_bottom_right = 20
	style.corner_radius_bottom_left = 20
	style.content_margin_left = 22
	style.content_margin_top = 22
	style.content_margin_right = 22
	style.content_margin_bottom = 22
	panel.add_theme_stylebox_override("panel", style)
	margin.add_child(panel)

	var body := VBoxContainer.new()
	body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	body.add_theme_constant_override("separation", 16)
	panel.add_child(body)

	var top_row := HBoxContainer.new()
	top_row.add_theme_constant_override("separation", 12)
	body.add_child(top_row)

	fullscreen_menu_title = Label.new()
	fullscreen_menu_title.add_theme_font_size_override("font_size", 28)
	fullscreen_menu_title.add_theme_color_override("font_color", Color8(244, 239, 232))
	top_row.add_child(fullscreen_menu_title)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	top_row.add_child(spacer)

	var close_button := _action_button("Close", Color8(72, 61, 81))
	close_button.pressed.connect(_close_fullscreen_menu)
	top_row.add_child(close_button)

	name_menu_view = _build_name_menu_view()
	record_view = _build_record_view()
	roster_view = _build_roster_view()
	event_view = _build_event_view()
	body.add_child(name_menu_view)
	body.add_child(record_view)
	body.add_child(roster_view)
	body.add_child(event_view)

	return overlay


func _build_name_menu_view() -> VBoxContainer:
	var view := VBoxContainer.new()
	view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_theme_constant_override("separation", 14)

	var note := Label.new()
	note.text = "Your alias is client-side and saving it refreshes the realtime session. Only A-Z and a-z are kept. Blank names reroll automatically."
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.add_theme_color_override("font_color", Color8(188, 200, 210))
	view.add_child(note)

	var input_label := Label.new()
	input_label.text = "Player alias"
	input_label.add_theme_color_override("font_color", Color8(226, 232, 236))
	view.add_child(input_label)

	player_name_input = LineEdit.new()
	player_name_input.max_length = RANDOM_PLAYER_NAME_LENGTH
	player_name_input.placeholder_text = "Exactly 10 letters is recommended"
	player_name_input.custom_minimum_size = Vector2(0, 42)
	player_name_input.text_submitted.connect(_on_name_submitted)
	view.add_child(player_name_input)

	var action_row := HBoxContainer.new()
	action_row.add_theme_constant_override("separation", 12)
	view.add_child(action_row)

	name_save_button = _action_button("Save Name", Color8(36, 108, 85))
	name_save_button.pressed.connect(_on_save_name_pressed)
	action_row.add_child(name_save_button)

	name_randomize_button = _action_button("Randomize", Color8(74, 86, 116))
	name_randomize_button.pressed.connect(_on_randomize_name_pressed)
	action_row.add_child(name_randomize_button)

	return view


func _build_record_view() -> VBoxContainer:
	var view := VBoxContainer.new()
	view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_theme_constant_override("separation", 12)

	var info := Label.new()
	info.text = "The server remains authoritative. W-L-NC updates only when the backend sends Connected or ReturnedToCentralLobby."
	info.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	info.add_theme_color_override("font_color", Color8(180, 188, 194))
	view.add_child(info)

	record_label = Label.new()
	record_label.add_theme_font_size_override("font_size", 34)
	record_label.add_theme_color_override("font_color", Color8(255, 217, 122))
	view.add_child(record_label)

	return view


func _build_roster_view() -> VBoxContainer:
	var view := VBoxContainer.new()
	view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_theme_constant_override("separation", 12)

	var note := Label.new()
	note.text = "Authoritative roster built from the current backend snapshot plus live updates."
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.add_theme_color_override("font_color", Color8(179, 199, 192))
	view.add_child(note)

	roster_log = RichTextLabel.new()
	roster_log.fit_content = false
	roster_log.scroll_active = true
	roster_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	roster_log.add_theme_color_override("default_color", Color8(226, 237, 233))
	view.add_child(roster_log)

	return view


func _build_event_view() -> VBoxContainer:
	var view := VBoxContainer.new()
	view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_theme_constant_override("separation", 12)

	var note := Label.new()
	note.text = "Recent shell-visible events from authoritative snapshots, lobby flow, and local transport transitions."
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.add_theme_color_override("font_color", Color8(188, 190, 205))
	view.add_child(note)

	event_log = RichTextLabel.new()
	event_log.fit_content = false
	event_log.scroll_active = true
	event_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	event_log.add_theme_color_override("default_color", Color8(212, 214, 226))
	view.add_child(event_log)

	return view


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
	body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_theme_constant_override("separation", 10)
	panel.add_child(body)
	panel.set_meta("body", body)
	return panel


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


func _open_fullscreen_menu(section: String) -> void:
	_menu_section = section
	fullscreen_menu.visible = true
	name_menu_view.visible = section == MENU_SECTION_NAME
	record_view.visible = section == MENU_SECTION_RECORD
	roster_view.visible = section == MENU_SECTION_ROSTER
	event_view.visible = section == MENU_SECTION_EVENTS

	match section:
		MENU_SECTION_NAME:
			fullscreen_menu_title.text = "Change Name"
			player_name_input.grab_focus()
			player_name_input.select_all()
		MENU_SECTION_RECORD:
			fullscreen_menu_title.text = "Player Record"
		MENU_SECTION_ROSTER:
			fullscreen_menu_title.text = "Roster Watch"
		MENU_SECTION_EVENTS:
			fullscreen_menu_title.text = "Event Feed"
		_:
			fullscreen_menu_title.text = "Menu"


func _close_fullscreen_menu() -> void:
	_menu_section = ""
	fullscreen_menu.visible = false


func _on_menu_option_selected(menu_id: int) -> void:
	match menu_id:
		MENU_ACTION_CHANGE_NAME:
			_open_fullscreen_menu(MENU_SECTION_NAME)
		MENU_ACTION_PLAYER_RECORD:
			_open_fullscreen_menu(MENU_SECTION_RECORD)
		MENU_ACTION_ROSTER_WATCH:
			_open_fullscreen_menu(MENU_SECTION_ROSTER)
		MENU_ACTION_EVENT_FEED:
			_open_fullscreen_menu(MENU_SECTION_EVENTS)


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
	_pending_primary_attack = true
	_drive_combat_input()


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
	app_state.apply_server_event(event_data)
	if String(event_data.get("type", "")) == "Connected" and app_state.debug_overlay_enabled():
		_sync_debug_mode_with_backend()
	_refresh_ui()


func _refresh_ui() -> void:
	var skill_catalog_signature := app_state.skill_catalog_signature()
	if skill_catalog_signature != _rendered_skill_catalog_signature:
		_rebuild_skill_buttons()

	var requested_name := _current_requested_player_name()
	var identity_text := requested_name
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
	lobby_roster_log.text = "\n".join(app_state.lobby_roster_lines())
	phase_label.text = app_state.phase_label
	score_label.text = app_state.score_text()
	countdown_value_label.text = app_state.countdown_label
	var local_player := app_state.local_arena_player()
	var unlocked_slots := int(local_player.get("unlocked_skill_slots", 0))
	var alive_state := "alive" if bool(local_player.get("alive", false)) else "down"
	var show_skill_pick := app_state.screen == "match" and app_state.match_phase == "skill_pick"
	if app_state.can_choose_skill():
		skill_pick_summary_label.text = "Choose one legal tier now. Only the next tier in a started tree, or tier 1 in an unstarted tree, is enabled."
	elif show_skill_pick:
		skill_pick_summary_label.text = "Your pick is locked. Waiting for the round to leave the skill-pick phase."
	else:
		skill_pick_summary_label.text = "Skill picks appear here at the start of each round."
	combat_hint_label.text = "WASD move, aim with the mouse, left click for melee, and use 1-5 for combat skills. Unlocked slots: %d. Local state: %s." % [unlocked_slots, alive_state]
	cooldown_summary_label.text = app_state.cooldown_summary_text()
	outcome_label.text = result_text
	central_directory_log.text = app_state.lobby_directory_bbcode()
	roster_log.text = "\n".join(app_state.roster_lines())
	event_log.text = app_state.event_log_text()

	create_lobby_button.disabled = not app_state.can_join_or_create_lobby()
	join_lobby_button.disabled = not app_state.can_join_or_create_lobby()
	team_a_button.disabled = not app_state.can_manage_lobby()
	team_b_button.disabled = not app_state.can_manage_lobby()
	ready_button.disabled = not app_state.can_manage_lobby()
	ready_button.text = app_state.ready_button_text()
	leave_lobby_button.disabled = not app_state.can_leave_lobby()
	quit_results_button.disabled = not app_state.can_quit_results()
	primary_attack_button.disabled = not app_state.can_use_primary_attack()
	primary_attack_button.text = "Primary Attack" if app_state.can_use_primary_attack() else "Primary Cooling"
	name_save_button.disabled = false
	name_randomize_button.disabled = false

	for button in skill_buttons:
		var tree_name := String(button.get_meta("tree_name", ""))
		var tier := int(button.get_meta("tier", 0))
		var selectable := app_state.can_choose_skill_option(tree_name, tier)
		button.disabled = not selectable
		button.text = "%d. %s" % [tier, app_state.skill_name_for(tree_name, tier)]
		if app_state.can_choose_skill():
			var next_tier := app_state.next_skill_tier_for(tree_name)
			if next_tier == 0:
				button.tooltip_text = "%s is fully unlocked." % tree_name
			elif tier == next_tier:
				button.tooltip_text = "Select %s for %s tier %d." % [
					app_state.skill_name_for(tree_name, tier),
					tree_name,
					tier,
				]
			else:
				button.tooltip_text = "Only tier %d is currently available for %s." % [next_tier, tree_name]
		else:
			button.tooltip_text = "Skill selection is only available during your active skill-pick window."

	connection_panel.visible = app_state.screen == "central"
	central_panel.visible = app_state.screen == "central"
	lobby_panel.visible = app_state.screen == "lobby"
	match_panel.visible = app_state.screen == "match"
	results_panel.visible = app_state.screen == "results"
	skill_pick_panel.visible = show_skill_pick
	combat_panel.visible = app_state.screen == "match" and not show_skill_pick


func _rebuild_skill_buttons() -> void:
	_rendered_skill_catalog_signature = app_state.skill_catalog_signature()
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
			button.set_meta("tree_name", tree_name)
			button.set_meta("tier", tier)
			button.pressed.connect(_on_skill_pressed.bind(tree_name, tier))
			column.add_child(button)
			skill_buttons.append(button)


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

	if transport.send_input_frame(payload):
		app_state.mark_debug_input(move_x, move_y, aim, _pending_primary_attack, _pending_cast_slot)
		_next_client_input_tick += 1
		_last_sent_aim = aim
		_pending_primary_attack = false
		_pending_cast_slot = 0


func _toggle_debug_mode() -> void:
	app_state.cycle_debug_mode()
	_sync_debug_mode_with_backend()
	_refresh_ui()


func _sync_debug_mode_with_backend() -> void:
	if not transport.is_open() or app_state.local_player_id <= 0:
		return
	if not transport.send_control_command("SetDebugMode", {"mode": app_state.debug_mode}):
		app_state.mark_transport_error("Failed to update backend debug mode.")


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
