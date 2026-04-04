extends RefCounted
class_name MainShellFactory

const ArenaViewScript := preload("res://scripts/arena/arena_view.gd")


class ShellRefs:
	extends RefCounted

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


static func build_shell(owner: Control, random_player_name_length: int) -> ShellRefs:
	var refs := ShellRefs.new()

	var base := ColorRect.new()
	base.color = Color8(10, 18, 24)
	base.anchor_right = 1.0
	base.anchor_bottom = 1.0
	base.mouse_filter = Control.MOUSE_FILTER_IGNORE
	owner.add_child(base)

	var glow_top := ColorRect.new()
	glow_top.color = Color8(189, 105, 57, 32)
	glow_top.anchor_right = 1.0
	glow_top.anchor_bottom = 0.35
	glow_top.mouse_filter = Control.MOUSE_FILTER_IGNORE
	owner.add_child(glow_top)

	var glow_side := ColorRect.new()
	glow_side.color = Color8(40, 132, 163, 28)
	glow_side.anchor_left = 0.66
	glow_side.anchor_right = 1.0
	glow_side.anchor_bottom = 1.0
	glow_side.mouse_filter = Control.MOUSE_FILTER_IGNORE
	owner.add_child(glow_side)

	var margin := MarginContainer.new()
	margin.anchor_right = 1.0
	margin.anchor_bottom = 1.0
	margin.add_theme_constant_override("margin_left", 28)
	margin.add_theme_constant_override("margin_top", 24)
	margin.add_theme_constant_override("margin_right", 28)
	margin.add_theme_constant_override("margin_bottom", 24)
	owner.add_child(margin)

	var root_column := VBoxContainer.new()
	root_column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root_column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root_column.add_theme_constant_override("separation", 18)
	margin.add_child(root_column)

	root_column.add_child(_build_top_bar(refs))
	root_column.add_child(_build_body(owner, refs, random_player_name_length))
	refs.fullscreen_menu = _build_fullscreen_menu(owner, refs, random_player_name_length)
	owner.add_child(refs.fullscreen_menu)
	return refs


static func _build_top_bar(refs: ShellRefs) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 12)

	refs.status_label = Label.new()
	refs.status_label.add_theme_font_size_override("font_size", 14)
	refs.status_label.add_theme_color_override("font_color", Color8(255, 214, 102))
	row.add_child(refs.status_label)

	refs.identity_label = Label.new()
	refs.identity_label.add_theme_color_override("font_color", Color8(128, 201, 255))
	row.add_child(refs.identity_label)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	row.add_child(spacer)

	refs.menu_button = MenuButton.new()
	refs.menu_button.text = "Menu"
	refs.menu_button.custom_minimum_size = Vector2(132, 42)
	_style_clickable(refs.menu_button, Color8(53, 73, 94))
	row.add_child(refs.menu_button)

	return row


static func _build_body(
	owner: Control,
	refs: ShellRefs,
	random_player_name_length: int
) -> Control:
	var column := VBoxContainer.new()
	column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	column.size_flags_vertical = Control.SIZE_EXPAND_FILL
	column.add_theme_constant_override("separation", 14)

	refs.connection_panel = _build_connection_panel(owner, refs, random_player_name_length)
	refs.central_panel = _build_central_panel(refs)
	refs.lobby_panel = _build_lobby_panel(owner, refs)
	refs.match_panel = _build_match_panel(owner, refs)
	refs.results_panel = _build_results_panel(owner, refs)

	column.add_child(refs.connection_panel)
	column.add_child(refs.central_panel)
	column.add_child(refs.lobby_panel)
	column.add_child(refs.match_panel)
	column.add_child(refs.results_panel)
	return column


static func _build_connection_panel(
	owner: Control,
	refs: ShellRefs,
	random_player_name_length: int
) -> PanelContainer:
	var panel := _make_panel(Color8(29, 42, 53), Color8(92, 120, 143))
	var body := panel.get_meta("body") as VBoxContainer

	var heading := Label.new()
	heading.text = "Central Actions"
	heading.add_theme_font_size_override("font_size", 19)
	heading.add_theme_color_override("font_color", Color8(244, 239, 232))
	body.add_child(heading)

	refs.banner_label = Label.new()
	refs.banner_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	refs.banner_label.add_theme_color_override("font_color", Color8(214, 192, 154))
	body.add_child(refs.banner_label)

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

	refs.join_lobby_input = LineEdit.new()
	refs.join_lobby_input.placeholder_text = "Join lobby ID"
	join_wrapper.add_child(refs.join_lobby_input)

	refs.create_lobby_button = _action_button("Create Lobby", Color8(45, 74, 126))
	refs.create_lobby_button.pressed.connect(Callable(owner, "_on_create_lobby_pressed"))
	controls.add_child(refs.create_lobby_button)

	refs.join_lobby_button = _action_button("Join Lobby", Color8(102, 72, 28))
	refs.join_lobby_button.pressed.connect(Callable(owner, "_on_join_lobby_pressed"))
	controls.add_child(refs.join_lobby_button)

	refs.start_training_button = _action_button("Start Training", Color8(62, 90, 50))
	refs.start_training_button.pressed.connect(Callable(owner, "_on_start_training_pressed"))
	controls.add_child(refs.start_training_button)

	return panel


static func _build_central_panel(refs: ShellRefs) -> PanelContainer:
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

	refs.central_directory_log = RichTextLabel.new()
	refs.central_directory_log.bbcode_enabled = true
	refs.central_directory_log.fit_content = true
	refs.central_directory_log.meta_underlined = true
	refs.central_directory_log.scroll_active = true
	refs.central_directory_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	refs.central_directory_log.custom_minimum_size = Vector2(0, 220)
	refs.central_directory_log.add_theme_color_override("default_color", Color8(222, 230, 236))
	body.add_child(refs.central_directory_log)

	return panel


static func _build_lobby_panel(owner: Control, refs: ShellRefs) -> PanelContainer:
	var panel := _make_panel(Color8(33, 50, 44), Color8(86, 144, 124))
	var body := panel.get_meta("body") as VBoxContainer

	var title := Label.new()
	title.text = "Game Lobby"
	title.add_theme_font_size_override("font_size", 22)
	title.add_theme_color_override("font_color", Color8(243, 247, 243))
	body.add_child(title)

	refs.lobby_label = Label.new()
	refs.lobby_label.add_theme_font_size_override("font_size", 18)
	refs.lobby_label.add_theme_color_override("font_color", Color8(155, 230, 189))
	body.add_child(refs.lobby_label)

	refs.lobby_note_label = Label.new()
	refs.lobby_note_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	refs.lobby_note_label.add_theme_color_override("font_color", Color8(190, 203, 196))
	body.add_child(refs.lobby_note_label)

	refs.team_label = Label.new()
	refs.team_label.add_theme_color_override("font_color", Color8(240, 241, 220))
	body.add_child(refs.team_label)

	var roster_title := Label.new()
	roster_title.text = "Lobby roster"
	roster_title.add_theme_color_override("font_color", Color8(228, 240, 228))
	body.add_child(roster_title)

	refs.lobby_roster_log = RichTextLabel.new()
	refs.lobby_roster_log.fit_content = true
	refs.lobby_roster_log.scroll_active = false
	refs.lobby_roster_log.custom_minimum_size = Vector2(0, 120)
	refs.lobby_roster_log.add_theme_color_override("default_color", Color8(221, 234, 223))
	body.add_child(refs.lobby_roster_log)

	var team_row := HBoxContainer.new()
	team_row.add_theme_constant_override("separation", 10)
	body.add_child(team_row)

	refs.team_a_button = _action_button("Join Team A", Color8(35, 82, 138))
	refs.team_a_button.pressed.connect(Callable(owner, "_on_team_pressed").bind("Team A"))
	team_row.add_child(refs.team_a_button)

	refs.team_b_button = _action_button("Join Team B", Color8(138, 56, 35))
	refs.team_b_button.pressed.connect(Callable(owner, "_on_team_pressed").bind("Team B"))
	team_row.add_child(refs.team_b_button)

	var lobby_action_row := HBoxContainer.new()
	lobby_action_row.add_theme_constant_override("separation", 10)
	body.add_child(lobby_action_row)

	refs.ready_button = _action_button("Set Ready", Color8(28, 102, 82))
	refs.ready_button.pressed.connect(Callable(owner, "_on_ready_pressed"))
	lobby_action_row.add_child(refs.ready_button)

	refs.leave_lobby_button = _action_button("Leave Lobby", Color8(102, 57, 28))
	refs.leave_lobby_button.pressed.connect(Callable(owner, "_on_leave_lobby_pressed"))
	lobby_action_row.add_child(refs.leave_lobby_button)

	return panel


static func _build_match_panel(owner: Control, refs: ShellRefs) -> PanelContainer:
	var panel := _make_panel(Color8(52, 39, 33), Color8(162, 112, 73))
	panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var body := panel.get_meta("body") as VBoxContainer
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	body.add_theme_constant_override("separation", 8)

	_build_match_header(refs, body)
	_build_skill_pick_host(refs, body)
	_build_combat_body(owner, refs, body)
	return panel


static func _build_match_header(refs: ShellRefs, body: VBoxContainer) -> void:
	refs.score_label = Label.new()
	refs.score_label.add_theme_font_size_override("font_size", 18)
	refs.score_label.add_theme_color_override("font_color", Color8(240, 241, 220))
	refs.score_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	refs.score_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	body.add_child(refs.score_label)


static func _build_skill_pick_host(refs: ShellRefs, body: VBoxContainer) -> void:
	refs.skill_pick_panel = _build_skill_pick_panel(refs)
	refs.skill_pick_inline_host = VBoxContainer.new()
	refs.skill_pick_inline_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	refs.skill_pick_inline_host.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	body.add_child(refs.skill_pick_inline_host)
	refs.skill_pick_inline_host.add_child(refs.skill_pick_panel)


static func _build_skill_pick_panel(refs: ShellRefs) -> PanelContainer:
	var panel := _make_panel(Color8(60, 47, 38), Color8(191, 135, 88))
	var skill_pick_body := panel.get_meta("body") as VBoxContainer

	var skill_pick_title := Label.new()
	skill_pick_title.text = "Skill picks"
	skill_pick_title.add_theme_color_override("font_color", Color8(252, 235, 209))
	skill_pick_title.add_theme_font_size_override("font_size", 18)
	skill_pick_body.add_child(skill_pick_title)

	refs.skill_pick_summary_label = Label.new()
	refs.skill_pick_summary_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	refs.skill_pick_summary_label.add_theme_color_override("font_color", Color8(222, 210, 192))
	skill_pick_body.add_child(refs.skill_pick_summary_label)

	var round_summary_title := Label.new()
	round_summary_title.text = "Round Summary"
	round_summary_title.add_theme_color_override("font_color", Color8(248, 224, 196))
	skill_pick_body.add_child(round_summary_title)

	refs.round_summary_log = RichTextLabel.new()
	refs.round_summary_log.fit_content = true
	refs.round_summary_log.scroll_active = false
	refs.round_summary_log.custom_minimum_size = Vector2(0, 136)
	refs.round_summary_log.add_theme_color_override("default_color", Color8(232, 224, 216))
	skill_pick_body.add_child(refs.round_summary_log)

	refs.skill_scroll = ScrollContainer.new()
	refs.skill_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	refs.skill_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_AUTO
	refs.skill_scroll.custom_minimum_size = Vector2(0, 360)
	refs.skill_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	skill_pick_body.add_child(refs.skill_scroll)

	refs.skill_columns = GridContainer.new()
	refs.skill_columns.columns = 3
	refs.skill_columns.add_theme_constant_override("separation", 10)
	refs.skill_columns.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	refs.skill_scroll.add_child(refs.skill_columns)
	return panel


static func _build_combat_body(owner: Control, refs: ShellRefs, body: VBoxContainer) -> void:
	refs.combat_panel = VBoxContainer.new()
	refs.combat_panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	refs.combat_panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	refs.combat_panel.add_theme_constant_override("separation", 6)
	body.add_child(refs.combat_panel)

	refs.phase_label = Label.new()
	refs.phase_label.add_theme_font_size_override("font_size", 20)
	refs.phase_label.add_theme_color_override("font_color", Color8(255, 216, 156))
	refs.phase_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	refs.combat_panel.add_child(refs.phase_label)

	refs.countdown_value_label = Label.new()
	refs.countdown_value_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	refs.countdown_value_label.add_theme_color_override("font_color", Color8(203, 197, 192))
	refs.countdown_value_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	refs.combat_panel.add_child(refs.countdown_value_label)

	refs.arena_view = ArenaViewScript.new()
	refs.arena_view.custom_minimum_size = Vector2.ZERO
	refs.arena_view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	refs.arena_view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	refs.arena_view.size_flags_stretch_ratio = 1.0
	refs.combat_panel.add_child(refs.arena_view)

	refs.cooldown_summary_label = Label.new()
	refs.cooldown_summary_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	refs.cooldown_summary_label.add_theme_color_override("font_color", Color8(183, 204, 214))
	refs.combat_panel.add_child(refs.cooldown_summary_label)

	refs.training_metrics_label = Label.new()
	refs.training_metrics_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	refs.training_metrics_label.add_theme_color_override("font_color", Color8(164, 232, 191))
	refs.combat_panel.add_child(refs.training_metrics_label)

	refs.combat_panel.add_child(_build_combat_action_row(owner, refs))


static func _build_combat_action_row(owner: Control, refs: ShellRefs) -> HBoxContainer:
	var combat_row := HBoxContainer.new()
	combat_row.add_theme_constant_override("separation", 10)

	refs.training_loadout_button = _action_button("Class Loadout", Color8(74, 86, 116))
	refs.training_loadout_button.pressed.connect(Callable(owner, "_on_training_loadout_pressed"))
	combat_row.add_child(refs.training_loadout_button)

	refs.reset_training_button = _action_button("Reset Training", Color8(56, 97, 76))
	refs.reset_training_button.pressed.connect(Callable(owner, "_on_reset_training_pressed"))
	combat_row.add_child(refs.reset_training_button)

	refs.quit_arena_button = _action_button("Back To Lobby Select", Color8(102, 57, 28))
	refs.quit_arena_button.pressed.connect(Callable(owner, "_on_quit_arena_pressed"))
	combat_row.add_child(refs.quit_arena_button)

	return combat_row


static func _build_results_panel(owner: Control, refs: ShellRefs) -> PanelContainer:
	var panel := _make_panel(Color8(47, 30, 45), Color8(128, 73, 141))
	var body := panel.get_meta("body") as VBoxContainer

	var title := Label.new()
	title.text = "Results"
	title.add_theme_font_size_override("font_size", 22)
	title.add_theme_color_override("font_color", Color8(245, 233, 244))
	body.add_child(title)

	refs.outcome_label = Label.new()
	refs.outcome_label.add_theme_font_size_override("font_size", 20)
	refs.outcome_label.add_theme_color_override("font_color", Color8(244, 192, 236))
	refs.outcome_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	body.add_child(refs.outcome_label)

	refs.match_summary_log = RichTextLabel.new()
	refs.match_summary_log.fit_content = true
	refs.match_summary_log.scroll_active = true
	refs.match_summary_log.custom_minimum_size = Vector2(0, 220)
	refs.match_summary_log.add_theme_color_override("default_color", Color8(236, 225, 238))
	body.add_child(refs.match_summary_log)

	refs.quit_results_button = _action_button("Quit To Central Lobby", Color8(95, 61, 104))
	refs.quit_results_button.pressed.connect(Callable(owner, "_on_quit_results_pressed"))
	body.add_child(refs.quit_results_button)

	return panel


static func _build_fullscreen_menu(
	owner: Control,
	refs: ShellRefs,
	random_player_name_length: int
) -> Control:
	var overlay := Control.new()
	overlay.anchor_right = 1.0
	overlay.anchor_bottom = 1.0
	overlay.visible = false
	overlay.mouse_filter = Control.MOUSE_FILTER_STOP

	var body := _build_fullscreen_menu_shell(overlay)
	_build_fullscreen_menu_header(owner, refs, body)
	_attach_fullscreen_menu_views(owner, refs, body, random_player_name_length)
	return overlay


static func _build_fullscreen_menu_shell(overlay: Control) -> VBoxContainer:
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
	return body


static func _build_fullscreen_menu_header(
	owner: Control,
	refs: ShellRefs,
	body: VBoxContainer
) -> void:
	var top_row := HBoxContainer.new()
	top_row.add_theme_constant_override("separation", 12)
	body.add_child(top_row)

	refs.fullscreen_menu_title = Label.new()
	refs.fullscreen_menu_title.add_theme_font_size_override("font_size", 28)
	refs.fullscreen_menu_title.add_theme_color_override("font_color", Color8(244, 239, 232))
	top_row.add_child(refs.fullscreen_menu_title)

	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	top_row.add_child(spacer)

	var close_button := _action_button("Close", Color8(72, 61, 81))
	close_button.pressed.connect(Callable(owner, "_close_fullscreen_menu"))
	top_row.add_child(close_button)


static func _attach_fullscreen_menu_views(
	owner: Control,
	refs: ShellRefs,
	body: VBoxContainer,
	random_player_name_length: int
) -> void:
	refs.name_menu_view = _build_name_menu_view(owner, refs, random_player_name_length)
	refs.training_loadout_view = _build_training_loadout_view(refs)
	refs.record_view = _build_record_view(refs)
	refs.roster_view = _build_roster_view(refs)
	refs.event_view = _build_event_view(refs)
	refs.diagnostics_view = _build_diagnostics_view(owner, refs)
	body.add_child(refs.name_menu_view)
	body.add_child(refs.training_loadout_view)
	body.add_child(refs.record_view)
	body.add_child(refs.roster_view)
	body.add_child(refs.event_view)
	body.add_child(refs.diagnostics_view)


static func _build_name_menu_view(
	owner: Control,
	refs: ShellRefs,
	random_player_name_length: int
) -> VBoxContainer:
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

	refs.player_name_input = LineEdit.new()
	refs.player_name_input.max_length = random_player_name_length
	refs.player_name_input.placeholder_text = "Exactly 10 letters is recommended"
	refs.player_name_input.custom_minimum_size = Vector2(0, 42)
	refs.player_name_input.text_submitted.connect(Callable(owner, "_on_name_submitted"))
	view.add_child(refs.player_name_input)

	var action_row := HBoxContainer.new()
	action_row.add_theme_constant_override("separation", 12)
	view.add_child(action_row)

	refs.name_save_button = _action_button("Save Name", Color8(36, 108, 85))
	refs.name_save_button.pressed.connect(Callable(owner, "_on_save_name_pressed"))
	action_row.add_child(refs.name_save_button)

	refs.name_randomize_button = _action_button("Randomize", Color8(74, 86, 116))
	refs.name_randomize_button.pressed.connect(Callable(owner, "_on_randomize_name_pressed"))
	action_row.add_child(refs.name_randomize_button)

	return view


static func _build_training_loadout_view(refs: ShellRefs) -> VBoxContainer:
	var view := VBoxContainer.new()
	view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_theme_constant_override("separation", 14)

	var note := Label.new()
	note.text = "Training loadout swaps are immediate. Pick any tier 1-5 skill to replace that slot without leaving the arena."
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.add_theme_color_override("font_color", Color8(188, 200, 210))
	view.add_child(note)

	refs.training_loadout_host = VBoxContainer.new()
	refs.training_loadout_host.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	refs.training_loadout_host.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_child(refs.training_loadout_host)

	return view


static func _build_record_view(refs: ShellRefs) -> VBoxContainer:
	var view := VBoxContainer.new()
	view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_theme_constant_override("separation", 12)

	var info := Label.new()
	info.text = "The server remains authoritative. Match record, round record, long-run combat rates, CC accuracy, and per-skill pick counts refresh when the backend sends Connected or ReturnedToCentralLobby."
	info.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	info.add_theme_color_override("font_color", Color8(180, 188, 194))
	view.add_child(info)

	var scroll := ScrollContainer.new()
	scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_child(scroll)

	refs.record_label = Label.new()
	refs.record_label.add_theme_font_size_override("font_size", 24)
	refs.record_label.add_theme_color_override("font_color", Color8(255, 217, 122))
	refs.record_label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	refs.record_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	refs.record_label.size_flags_vertical = Control.SIZE_EXPAND_FILL
	refs.record_label.vertical_alignment = VERTICAL_ALIGNMENT_TOP
	scroll.add_child(refs.record_label)

	return view


static func _build_roster_view(refs: ShellRefs) -> VBoxContainer:
	var view := VBoxContainer.new()
	view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_theme_constant_override("separation", 12)

	var note := Label.new()
	note.text = "Authoritative roster built from the current backend snapshot plus live updates."
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.add_theme_color_override("font_color", Color8(179, 199, 192))
	view.add_child(note)

	refs.roster_log = RichTextLabel.new()
	refs.roster_log.fit_content = false
	refs.roster_log.scroll_active = true
	refs.roster_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	refs.roster_log.add_theme_color_override("default_color", Color8(226, 237, 233))
	view.add_child(refs.roster_log)

	return view


static func _build_event_view(refs: ShellRefs) -> VBoxContainer:
	var view := VBoxContainer.new()
	view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_theme_constant_override("separation", 12)

	var note := Label.new()
	note.text = "Recent shell-visible events from authoritative snapshots, lobby flow, and local transport transitions."
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.add_theme_color_override("font_color", Color8(188, 190, 205))
	view.add_child(note)

	refs.event_log = RichTextLabel.new()
	refs.event_log.fit_content = false
	refs.event_log.scroll_active = true
	refs.event_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	refs.event_log.add_theme_color_override("default_color", Color8(212, 214, 226))
	view.add_child(refs.event_log)

	return view


static func _build_diagnostics_view(owner: Control, refs: ShellRefs) -> VBoxContainer:
	var view := VBoxContainer.new()
	view.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	view.size_flags_vertical = Control.SIZE_EXPAND_FILL
	view.add_theme_constant_override("separation", 12)

	var note := Label.new()
	note.text = "Structured client-side diagnostics for frame timing, packet flow, object counts, and transport state. Pair this with deploy/useful_log_collect.py on the host."
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.add_theme_color_override("font_color", Color8(188, 200, 210))
	view.add_child(note)

	refs.diagnostics_copy_button = _action_button("Copy Diagnostics", Color8(52, 94, 120))
	refs.diagnostics_copy_button.pressed.connect(Callable(owner, "_on_copy_diagnostics_pressed"))
	view.add_child(refs.diagnostics_copy_button)

	refs.diagnostics_log = RichTextLabel.new()
	refs.diagnostics_log.fit_content = false
	refs.diagnostics_log.scroll_active = true
	refs.diagnostics_log.size_flags_vertical = Control.SIZE_EXPAND_FILL
	refs.diagnostics_log.add_theme_color_override("default_color", Color8(222, 231, 237))
	view.add_child(refs.diagnostics_log)

	return view


static func _make_panel(background: Color, border: Color) -> PanelContainer:
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


static func _style_clickable(control: Control, color: Color) -> void:
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


static func _action_button(text: String, color: Color) -> Button:
	var button := Button.new()
	button.text = text
	button.custom_minimum_size = Vector2(0, 42)
	_style_clickable(button, color)
	return button
