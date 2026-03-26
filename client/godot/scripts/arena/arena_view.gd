extends Control
class_name ArenaView

const PADDING := 22.0
const GRID_STEP_UNITS := 60.0
const PLAYER_RADIUS_UNITS := 28.0
const FOG_EDGE_EXTENSION_RATIO := 0.52
const FOG_EDGE_ALPHA_SCALE := 0.58
const DEBUG_TEXT_COLOR := Color(0.96, 0.96, 0.98, 0.96)
const DEBUG_RENDER_COLOR := Color(0.42, 0.98, 0.72, 0.92)
const DEBUG_AUTH_COLOR := Color(0.98, 0.34, 0.88, 0.92)
const DEBUG_LINK_COLOR := Color(1.0, 0.95, 0.62, 0.9)
const PLAYER_SHADOW_COLOR := Color(0.03, 0.05, 0.08, 0.26)
const PROJECTILE_SHADOW_COLOR := Color(0.03, 0.05, 0.08, 0.18)
const OBSTACLE_SHADOW_COLOR := Color(0.03, 0.04, 0.06, 0.16)
const DEPLOYABLE_SHADOW_COLOR := Color(0.03, 0.05, 0.08, 0.22)

var app_state: ClientState = null


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_PASS


func _process(_delta: float) -> void:
	queue_redraw()


func set_client_state(state: ClientState) -> void:
	app_state = state
	queue_redraw()


func has_arena_snapshot() -> bool:
	return app_state != null and app_state.arena_width > 0 and app_state.arena_height > 0


func has_mouse_in_arena() -> bool:
	return _arena_rect().has_point(get_local_mouse_position())


func mouse_world_position() -> Vector2:
	var rect := _arena_rect()
	if rect.size.x <= 0.0 or rect.size.y <= 0.0 or not has_arena_snapshot():
		return Vector2.ZERO
	var local := get_local_mouse_position()
	var normalized_x := clampf((local.x - rect.position.x) / rect.size.x, 0.0, 1.0)
	var normalized_y := clampf((local.y - rect.position.y) / rect.size.y, 0.0, 1.0)
	return Vector2(
		( normalized_x - 0.5) * float(app_state.arena_width),
		( normalized_y - 0.5) * float(app_state.arena_height)
	)


func _draw() -> void:
	var panel_rect := Rect2(Vector2.ZERO, size)
	draw_rect(panel_rect, Color8(235, 236, 239))

	if not has_arena_snapshot():
		_draw_centered_text(
			panel_rect,
			"Waiting for the authoritative arena snapshot..."
		)
		return

	var arena_rect := _arena_rect()
	draw_rect(arena_rect, Color8(232, 232, 236))
	_draw_grid(arena_rect)
	_draw_obstacles(arena_rect)
	_draw_visibility_overlay(arena_rect)
	_draw_effects(arena_rect)
	_draw_deployables(arena_rect)
	_draw_projectiles(arena_rect)
	_draw_players(arena_rect)
	_draw_debug_overlay(arena_rect)
	_draw_border(arena_rect)


func _arena_rect() -> Rect2:
	var available := size - Vector2.ONE * (PADDING * 2.0)
	if available.x <= 0.0 or available.y <= 0.0:
		return Rect2(Vector2.ZERO, Vector2.ZERO)
	if app_state == null or app_state.arena_width <= 0 or app_state.arena_height <= 0:
		return Rect2(Vector2(PADDING, PADDING), available)

	var world_size := Vector2(app_state.arena_width, app_state.arena_height)
	var scale := minf(available.x / world_size.x, available.y / world_size.y)
	var draw_size := world_size * scale
	var offset := Vector2(
		(size.x - draw_size.x) * 0.5,
		(size.y - draw_size.y) * 0.5
	)
	return Rect2(offset, draw_size)


func _draw_grid(arena_rect: Rect2) -> void:
	var step_x := arena_rect.size.x * (GRID_STEP_UNITS / float(app_state.arena_width))
	var step_y := arena_rect.size.y * (GRID_STEP_UNITS / float(app_state.arena_height))
	var grid_color := Color8(205, 206, 212)

	var x := arena_rect.position.x
	while x <= arena_rect.end.x + 0.5:
		draw_line(
			Vector2(x, arena_rect.position.y),
			Vector2(x, arena_rect.end.y),
			grid_color,
			1.0
		)
		x += step_x

	var y := arena_rect.position.y
	while y <= arena_rect.end.y + 0.5:
		draw_line(
			Vector2(arena_rect.position.x, y),
			Vector2(arena_rect.end.x, y),
			grid_color,
			1.0
		)
		y += step_y


func _draw_obstacles(arena_rect: Rect2) -> void:
	for obstacle in app_state.arena_obstacles:
		var rect := _world_rect_to_canvas(
			arena_rect,
			float(obstacle.get("center_x", 0)),
			float(obstacle.get("center_y", 0)),
			float(obstacle.get("half_width", 0)),
			float(obstacle.get("half_height", 0))
		)
		var shadow_rect := rect
		shadow_rect.position += Vector2(7.0, 9.0)
		draw_rect(shadow_rect, OBSTACLE_SHADOW_COLOR)
		var kind_name := String(obstacle.get("kind", ""))
		match kind_name:
			"Shrub":
				draw_rect(rect, Color8(185, 215, 180))
			"Pillar":
				draw_rect(rect, Color8(84, 84, 93))
				var inner: Rect2 = rect.grow(-rect.size.x * 0.24)
				draw_rect(inner, Color8(203, 217, 228))
			_:
				draw_rect(rect, Color8(140, 140, 140))


func _draw_effects(arena_rect: Rect2) -> void:
	for effect in app_state.arena_effects:
		var ttl: float = float(effect.get("ttl", 0.0))
		var ttl_max: float = maxf(0.01, float(effect.get("ttl_max", ttl)))
		var alpha: float = clampf(ttl / ttl_max, 0.15, 1.0)
		var start: Vector2 = _world_to_canvas(
			arena_rect,
			Vector2(float(effect.get("x", 0)), float(effect.get("y", 0)))
		)
		var target: Vector2 = _world_to_canvas(
			arena_rect,
			Vector2(float(effect.get("target_x", 0)), float(effect.get("target_y", 0)))
		)
		var radius: float = _world_radius_to_canvas(arena_rect, float(effect.get("radius", 0)))
		var color: Color = _effect_color(String(effect.get("kind", "")), alpha)
		match String(effect.get("kind", "")):
			"MeleeSwing":
				var direction := (target - start).normalized()
				if direction == Vector2.ZERO:
					direction = Vector2.RIGHT
				var tangent := Vector2(-direction.y, direction.x)
				var impact_radius := maxf(radius, 12.0)
				var left := target + tangent * maxf(impact_radius * 0.9, 10.0)
				var right := target - tangent * maxf(impact_radius * 0.9, 10.0)
				draw_colored_polygon(
					PackedVector2Array([start, left, target, right]),
					Color(color.r, color.g, color.b, alpha * 0.32)
				)
				draw_line(start, target, Color(color.r, color.g, color.b, alpha * 0.55), 3.0)
				draw_circle(target, impact_radius, Color(color.r, color.g, color.b, alpha * 0.16))
				draw_arc(target, impact_radius, 0.0, TAU, 24, color, 2.5)
			"SkillShot":
				draw_line(start, target, color, 4.0)
				draw_circle(target, maxf(radius, 8.0), Color(color.r, color.g, color.b, alpha * 0.18))
			"Beam":
				draw_line(start, target, color, 7.0)
				var progress := 1.0 - clampf(ttl / ttl_max, 0.0, 1.0)
				var pulse := start.lerp(target, progress)
				draw_circle(pulse, maxf(radius * 0.7, 8.0), Color(color.r, color.g, color.b, alpha * 0.85))
			"DashTrail":
				draw_line(start, target, color, 6.0)
			"Burst", "Nova":
				draw_circle(start, radius, color)
			"HitSpark":
				draw_line(start + Vector2(-radius, -radius), start + Vector2(radius, radius), color, 2.0)
				draw_line(start + Vector2(-radius, radius), start + Vector2(radius, -radius), color, 2.0)
			_:
				draw_circle(start, radius, color)


func _draw_projectiles(arena_rect: Rect2) -> void:
	for projectile in app_state.arena_projectiles_list():
		var position := _world_to_canvas(
			arena_rect,
			Vector2(float(projectile.get("x", 0)), float(projectile.get("y", 0)))
		)
		var radius := _world_radius_to_canvas(arena_rect, float(projectile.get("radius", 0)))
		var color := _effect_color(String(projectile.get("kind", "")), 0.95)
		draw_circle(
			position + Vector2(0.0, radius * 0.72),
			maxf(radius * 0.88, 4.0),
			PROJECTILE_SHADOW_COLOR
		)
		draw_circle(position, radius + 8.0, Color(color.r, color.g, color.b, 0.18))
		draw_circle(position, radius + 3.0, Color(0.08, 0.1, 0.12, 0.85))
		draw_circle(position, radius, color)
		draw_circle(position, maxf(radius * 0.42, 3.0), Color(1.0, 1.0, 1.0, 0.45))


func _draw_deployables(arena_rect: Rect2) -> void:
	var font := ThemeDB.fallback_font
	for deployable in app_state.arena_deployables_list():
		var position := _world_to_canvas(
			arena_rect,
			Vector2(float(deployable.get("x", 0)), float(deployable.get("y", 0)))
		)
		var radius := _world_radius_to_canvas(arena_rect, float(deployable.get("radius", 0)))
		var team_color := _team_color(String(deployable.get("team", "")), true)
		var kind_name := String(deployable.get("kind", ""))
		draw_circle(
			position + Vector2(0.0, radius * 0.76),
			maxf(radius * 0.94, 5.0),
			DEPLOYABLE_SHADOW_COLOR
		)
		match kind_name:
			"Barrier":
				var rect := Rect2(position - Vector2(radius, radius), Vector2(radius * 2.0, radius * 2.0))
				draw_rect(rect, Color(team_color.r, team_color.g, team_color.b, 0.26))
				draw_rect(rect.grow(-maxf(radius * 0.18, 3.0)), Color(0.18, 0.2, 0.24, 0.9))
				draw_rect(rect, team_color, false, 2.0)
			"Ward":
				draw_circle(position, radius + 5.0, Color(team_color.r, team_color.g, team_color.b, 0.16))
				draw_circle(position, radius, Color(0.14, 0.16, 0.2, 0.92))
				draw_arc(position, radius, 0.0, TAU, 24, team_color, 2.0)
				draw_circle(position, maxf(radius * 0.34, 3.0), team_color)
			"Trap":
				draw_circle(position, radius, Color(0.18, 0.13, 0.14, 0.96))
				draw_arc(position, radius, 0.0, TAU, 20, team_color, 2.0)
				draw_line(position + Vector2(-radius * 0.6, -radius * 0.6), position + Vector2(radius * 0.6, radius * 0.6), team_color, 2.0)
				draw_line(position + Vector2(-radius * 0.6, radius * 0.6), position + Vector2(radius * 0.6, -radius * 0.6), team_color, 2.0)
			"Aura":
				draw_circle(position, radius, Color(team_color.r, team_color.g, team_color.b, 0.12))
				draw_arc(position, radius, 0.0, TAU, 32, team_color, 2.0)
				draw_circle(position, maxf(radius * 0.32, 4.0), Color(team_color.r, team_color.g, team_color.b, 0.75))
			_:
				draw_circle(position, radius + 5.0, Color(team_color.r, team_color.g, team_color.b, 0.14))
				draw_circle(position, radius, Color(0.18, 0.2, 0.24, 0.94))
				draw_circle(position, maxf(radius * 0.48, 4.0), team_color)

		var hp_width := radius * 1.8
		var hp_origin := position + Vector2(-hp_width * 0.5, radius + 8.0)
		var hp_ratio := 0.0
		var max_hp := maxf(1.0, float(deployable.get("max_hit_points", 1)))
		hp_ratio = clampf(float(deployable.get("hit_points", 0)) / max_hp, 0.0, 1.0)
		draw_rect(Rect2(hp_origin, Vector2(hp_width, 4.0)), Color8(65, 34, 34))
		draw_rect(Rect2(hp_origin, Vector2(hp_width * hp_ratio, 4.0)), Color8(86, 198, 125))

		if font != null:
			draw_string(
				font,
				position + Vector2(-radius * 0.9, -radius - 8.0),
				kind_name,
				HORIZONTAL_ALIGNMENT_LEFT,
				-1.0,
				12,
				Color8(34, 38, 46)
			)


func _draw_players(arena_rect: Rect2) -> void:
	var font := ThemeDB.fallback_font
	for player in app_state.arena_players_list():
		var is_local_player := int(player.get("player_id", 0)) == app_state.local_player_id
		var canvas_pos := _world_to_canvas(
			arena_rect,
			Vector2(float(player.get("x", 0)), float(player.get("y", 0)))
		)
		var aim_end := _world_to_canvas(
			arena_rect,
			Vector2(
				float(player.get("x", 0)) + float(player.get("aim_x", 0)),
				float(player.get("y", 0)) + float(player.get("aim_y", 0))
			)
		)
		var alive := bool(player.get("alive", false))
		var body_color: Color = _team_color(String(player.get("team", "")), alive)
		var radius: float = _world_radius_to_canvas(arena_rect, PLAYER_RADIUS_UNITS)
		var outline: Color = Color.WHITE if is_local_player else Color8(34, 34, 42)

		if is_local_player:
			draw_line(canvas_pos, aim_end, Color(body_color, 0.35), 2.0)
		draw_circle(
			canvas_pos + Vector2(0.0, radius * 0.82),
			radius * 0.96,
			PLAYER_SHADOW_COLOR
		)
		draw_circle(canvas_pos, radius + 9.0, Color(body_color.r, body_color.g, body_color.b, 0.14))
		draw_circle(canvas_pos, radius + 4.0, outline)
		draw_circle(canvas_pos, radius, body_color)

		var hp_ratio: float = 0.0
		var max_hp: float = maxf(1.0, float(player.get("max_hit_points", 1)))
		hp_ratio = clampf(float(player.get("hit_points", 0)) / max_hp, 0.0, 1.0)
		var hp_width: float = radius * 2.2
		var hp_origin: Vector2 = canvas_pos + Vector2(-hp_width * 0.5, radius + 10.0)
		draw_rect(Rect2(hp_origin, Vector2(hp_width, 5.0)), Color8(65, 34, 34))
		draw_rect(Rect2(hp_origin, Vector2(hp_width * hp_ratio, 5.0)), Color8(86, 198, 125))
		var max_mana: float = maxf(1.0, float(player.get("max_mana", 1)))
		var mana_ratio: float = clampf(float(player.get("mana", 0)) / max_mana, 0.0, 1.0)
		var mana_origin: Vector2 = hp_origin + Vector2(0.0, 8.0)
		draw_rect(Rect2(mana_origin, Vector2(hp_width, 4.0)), Color8(28, 44, 78))
		draw_rect(Rect2(mana_origin, Vector2(hp_width * mana_ratio, 4.0)), Color8(89, 163, 255))
		_draw_cast_bar(arena_rect, player, canvas_pos, radius)

		if font != null:
			var name_label := _player_name_label(player)
			var name_position := canvas_pos + Vector2(-radius * 0.85, -radius - 12.0)
			draw_string(
				font,
				name_position,
				name_label,
				HORIZONTAL_ALIGNMENT_LEFT,
				-1.0,
				14,
				Color8(26, 28, 34)
			)
			var resource_label := _player_resource_label(player)
			if resource_label != "":
				draw_string(
					font,
					canvas_pos + Vector2(-radius * 0.85, -radius - 28.0),
					resource_label,
					HORIZONTAL_ALIGNMENT_LEFT,
					-1.0,
					13,
					Color8(12, 18, 28)
				)
			var status_tokens: Array[String] = []
			for status in player.get("active_statuses", []):
				var status_data := status as Dictionary
				status_tokens.append("%s x%d" % [
					String(status_data.get("kind", "")),
					int(status_data.get("stacks", 0)),
				])
			if not status_tokens.is_empty():
				var status_y_offset := -44.0 if resource_label != "" else -28.0
				draw_string(
					font,
					canvas_pos + Vector2(-radius * 0.85, -radius + status_y_offset),
					" ".join(status_tokens),
					HORIZONTAL_ALIGNMENT_LEFT,
					-1.0,
					12,
					Color8(52, 66, 78)
				)


func _draw_border(arena_rect: Rect2) -> void:
	draw_rect(arena_rect, Color8(58, 61, 68), false, 3.0)


func _draw_visibility_overlay(arena_rect: Rect2) -> void:
	var tile_width := app_state.arena_tile_width()
	var tile_height := app_state.arena_tile_height()
	if tile_width <= 0 or tile_height <= 0 or app_state.arena_tile_units <= 0:
		return
	var half_tile := float(app_state.arena_tile_units) * 0.5
	for row in range(tile_height):
		for column in range(tile_width):
			if app_state.is_tile_visible(column, row):
				continue
			var center_x := -float(app_state.arena_width) * 0.5 + float(column * app_state.arena_tile_units) + half_tile
			var center_y := -float(app_state.arena_height) * 0.5 + float(row * app_state.arena_tile_units) + half_tile
			var rect := _world_rect_to_canvas(
				arena_rect,
				center_x,
				center_y,
				half_tile,
				half_tile
			)
			var fog_color := (
				Color(0.06, 0.08, 0.11, 0.56)
				if app_state.is_tile_explored(column, row)
				else Color(0.03, 0.04, 0.05, 0.96)
			)
			draw_rect(rect, fog_color)
	_draw_fog_soft_edges(arena_rect, tile_width, tile_height, half_tile)


func _draw_fog_soft_edges(
	arena_rect: Rect2,
	tile_width: int,
	tile_height: int,
	half_tile: float
) -> void:
	for column in range(tile_width):
		var row := 0
		while row < tile_height:
			if _has_fog_edge(column, row, Vector2i.LEFT):
				var explored := app_state.is_tile_explored(column, row)
				var start_row := row
				while row < tile_height \
					and _has_fog_edge(column, row, Vector2i.LEFT) \
					and app_state.is_tile_explored(column, row) == explored:
					row += 1
				_draw_fog_edge_run(arena_rect, column, start_row, column, row - 1, half_tile, Vector2i.LEFT, explored)
				continue
			row += 1

	for column in range(tile_width):
		var row := 0
		while row < tile_height:
			if _has_fog_edge(column, row, Vector2i.RIGHT):
				var explored := app_state.is_tile_explored(column, row)
				var start_row := row
				while row < tile_height \
					and _has_fog_edge(column, row, Vector2i.RIGHT) \
					and app_state.is_tile_explored(column, row) == explored:
					row += 1
				_draw_fog_edge_run(arena_rect, column, start_row, column, row - 1, half_tile, Vector2i.RIGHT, explored)
				continue
			row += 1

	for row in range(tile_height):
		var column := 0
		while column < tile_width:
			if _has_fog_edge(column, row, Vector2i.UP):
				var explored := app_state.is_tile_explored(column, row)
				var start_column := column
				while column < tile_width \
					and _has_fog_edge(column, row, Vector2i.UP) \
					and app_state.is_tile_explored(column, row) == explored:
					column += 1
				_draw_fog_edge_run(arena_rect, start_column, row, column - 1, row, half_tile, Vector2i.UP, explored)
				continue
			column += 1

	for row in range(tile_height):
		var column := 0
		while column < tile_width:
			if _has_fog_edge(column, row, Vector2i.DOWN):
				var explored := app_state.is_tile_explored(column, row)
				var start_column := column
				while column < tile_width \
					and _has_fog_edge(column, row, Vector2i.DOWN) \
					and app_state.is_tile_explored(column, row) == explored:
					column += 1
				_draw_fog_edge_run(arena_rect, start_column, row, column - 1, row, half_tile, Vector2i.DOWN, explored)
				continue
			column += 1


func _has_fog_edge(column: int, row: int, direction: Vector2i) -> bool:
	if app_state == null or app_state.is_tile_visible(column, row):
		return false
	return app_state.is_tile_visible(column + direction.x, row + direction.y)


func _draw_fog_edge_run(
	arena_rect: Rect2,
	start_column: int,
	start_row: int,
	end_column: int,
	end_row: int,
	half_tile: float,
	direction: Vector2i,
	explored: bool
) -> void:
	var start_center := _tile_center_world(start_column, start_row, half_tile)
	var end_center := _tile_center_world(end_column, end_row, half_tile)
	var start_rect := _world_rect_to_canvas(
		arena_rect,
		start_center.x,
		start_center.y,
		half_tile,
		half_tile
	)
	var end_rect := _world_rect_to_canvas(
		arena_rect,
		end_center.x,
		end_center.y,
		half_tile,
		half_tile
	)
	var fog_color := (
		Color(0.06, 0.08, 0.11, 0.56)
		if explored
		else Color(0.03, 0.04, 0.05, 0.96)
	)
	var edge_color := Color(
		fog_color.r,
		fog_color.g,
		fog_color.b,
		fog_color.a * FOG_EDGE_ALPHA_SCALE
	)
	var transparent := Color(edge_color.r, edge_color.g, edge_color.b, 0.0)
	var extension_x := start_rect.size.x * FOG_EDGE_EXTENSION_RATIO
	var extension_y := start_rect.size.y * FOG_EDGE_EXTENSION_RATIO

	if direction == Vector2i.LEFT:
		draw_polygon(
			PackedVector2Array([
				Vector2(start_rect.position.x - extension_x, start_rect.position.y),
				Vector2(start_rect.position.x, start_rect.position.y),
				Vector2(end_rect.position.x, end_rect.end.y),
				Vector2(end_rect.position.x - extension_x, end_rect.end.y),
			]),
			PackedColorArray([transparent, edge_color, edge_color, transparent])
		)
	elif direction == Vector2i.RIGHT:
		draw_polygon(
			PackedVector2Array([
				Vector2(start_rect.end.x, start_rect.position.y),
				Vector2(start_rect.end.x + extension_x, start_rect.position.y),
				Vector2(end_rect.end.x + extension_x, end_rect.end.y),
				Vector2(end_rect.end.x, end_rect.end.y),
			]),
			PackedColorArray([edge_color, transparent, transparent, edge_color])
		)
	elif direction == Vector2i.UP:
		draw_polygon(
			PackedVector2Array([
				Vector2(start_rect.position.x, start_rect.position.y - extension_y),
				Vector2(end_rect.end.x, end_rect.position.y - extension_y),
				Vector2(end_rect.end.x, end_rect.position.y),
				Vector2(start_rect.position.x, start_rect.position.y),
			]),
			PackedColorArray([transparent, transparent, edge_color, edge_color])
		)
	elif direction == Vector2i.DOWN:
		draw_polygon(
			PackedVector2Array([
				Vector2(start_rect.position.x, start_rect.end.y),
				Vector2(end_rect.end.x, end_rect.end.y),
				Vector2(end_rect.end.x, end_rect.end.y + extension_y),
				Vector2(start_rect.position.x, start_rect.end.y + extension_y),
			]),
			PackedColorArray([edge_color, edge_color, transparent, transparent])
		)


func _tile_center_world(column: int, row: int, half_tile: float) -> Vector2:
	return Vector2(
		-float(app_state.arena_width) * 0.5 + float(column * app_state.arena_tile_units) + half_tile,
		-float(app_state.arena_height) * 0.5 + float(row * app_state.arena_tile_units) + half_tile
	)


func _draw_centered_text(rect: Rect2, text: String) -> void:
	var font := ThemeDB.fallback_font
	if font == null:
		return
	var size_px := 18
	var text_width := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, size_px).x
	var baseline := rect.position + Vector2((rect.size.x - text_width) * 0.5, rect.size.y * 0.5)
	draw_string(font, baseline, text, HORIZONTAL_ALIGNMENT_LEFT, -1.0, size_px, Color8(58, 61, 68))


func _draw_debug_overlay(arena_rect: Rect2) -> void:
	if app_state == null or not app_state.debug_overlay_enabled():
		return
	_draw_debug_summary(arena_rect)
	if app_state.debug_shows_render():
		_draw_render_debug_overlay(arena_rect)
	if app_state.debug_shows_auth():
		_draw_auth_debug_overlay(arena_rect)


func _draw_debug_summary(arena_rect: Rect2) -> void:
	var font := ThemeDB.fallback_font
	if font == null:
		return
	var lines := app_state.debug_summary_lines()
	var y := arena_rect.position.y + 18.0
	for line in lines:
		draw_string(
			font,
			Vector2(arena_rect.position.x + 16.0, y),
			line,
			HORIZONTAL_ALIGNMENT_LEFT,
			-1.0,
			14,
			DEBUG_TEXT_COLOR
		)
		y += 16.0


func _draw_render_debug_overlay(arena_rect: Rect2) -> void:
	var font := ThemeDB.fallback_font
	for player in app_state.arena_players_list():
		var player_id := int(player.get("player_id", 0))
		var position := _world_to_canvas(
			arena_rect,
			Vector2(float(player.get("x", 0)), float(player.get("y", 0)))
		)
		var radius := _world_radius_to_canvas(arena_rect, PLAYER_RADIUS_UNITS)
		draw_arc(position, radius + 12.0, 0.0, TAU, 24, DEBUG_RENDER_COLOR, 1.5)
		if font != null:
			var label := "R (%d,%d)" % [int(round(float(player.get("x", 0)))), int(round(float(player.get("y", 0))))]
			var last_spell := app_state.debug_last_spell_for_player(player_id)
			if last_spell != "":
				label = "%s %s" % [label, last_spell]
			var current_cast_slot := int(player.get("current_cast_slot", 0))
			if current_cast_slot > 0:
				label = "%s cast=%s" % [label, _cast_label_for_player(player)]
			draw_string(
				font,
				position + Vector2(radius + 8.0, -radius - 4.0),
				label,
				HORIZONTAL_ALIGNMENT_LEFT,
				-1.0,
				12,
				DEBUG_RENDER_COLOR
			)

	for projectile in app_state.arena_projectiles_list():
		var position := _world_to_canvas(
			arena_rect,
			Vector2(float(projectile.get("x", 0)), float(projectile.get("y", 0)))
		)
		var radius := _world_radius_to_canvas(arena_rect, float(projectile.get("radius", 0)))
		draw_arc(position, radius + 5.0, 0.0, TAU, 20, DEBUG_RENDER_COLOR, 1.5)
		if font != null:
			draw_string(
				font,
				position + Vector2(radius + 6.0, -4.0),
				app_state.debug_label_for_projectile(projectile),
				HORIZONTAL_ALIGNMENT_LEFT,
				-1.0,
				11,
				DEBUG_RENDER_COLOR
			)


func _draw_auth_debug_overlay(arena_rect: Rect2) -> void:
	var font := ThemeDB.fallback_font
	for player in app_state.authoritative_arena_players_list():
		var player_id := int(player.get("player_id", 0))
		var auth_position := _world_to_canvas(
			arena_rect,
			Vector2(float(player.get("x", 0)), float(player.get("y", 0)))
		)
		var radius := _world_radius_to_canvas(arena_rect, PLAYER_RADIUS_UNITS)
		draw_arc(auth_position, radius + 18.0, 0.0, TAU, 28, DEBUG_AUTH_COLOR, 2.0)

		var movement_vector := app_state.debug_movement_vector_for(player_id)
		if movement_vector != Vector2.ZERO:
			draw_line(
				auth_position,
				auth_position + _world_vector_to_canvas(arena_rect, movement_vector),
				DEBUG_AUTH_COLOR,
				2.0
			)

		if app_state.debug_mode == "both":
			var render_player := app_state.rendered_arena_player(player_id)
			if not render_player.is_empty():
				var render_position := _world_to_canvas(
					arena_rect,
					Vector2(float(render_player.get("x", 0)), float(render_player.get("y", 0)))
				)
				draw_line(render_position, auth_position, DEBUG_LINK_COLOR, 1.5)

		if font != null:
			var label := "A (%d,%d) hp=%d mana=%d" % [
				int(player.get("x", 0)),
				int(player.get("y", 0)),
				int(player.get("hit_points", 0)),
				int(player.get("mana", 0)),
			]
			var current_cast_slot := int(player.get("current_cast_slot", 0))
			if current_cast_slot > 0:
				label = "%s cast=%s" % [label, _cast_label_for_player(player)]
			draw_string(
				font,
				auth_position + Vector2(radius + 8.0, radius + 14.0),
				label,
				HORIZONTAL_ALIGNMENT_LEFT,
				-1.0,
				12,
				DEBUG_AUTH_COLOR
			)

	for projectile in app_state.authoritative_arena_projectiles_list():
		var position := _world_to_canvas(
			arena_rect,
			Vector2(float(projectile.get("x", 0)), float(projectile.get("y", 0)))
		)
		var radius := _world_radius_to_canvas(arena_rect, float(projectile.get("radius", 0)))
		draw_arc(position, radius + 10.0, 0.0, TAU, 20, DEBUG_AUTH_COLOR, 1.6)
		if font != null:
			var auth_label := "%s r=%d" % [
				app_state.debug_label_for_projectile(projectile),
				int(projectile.get("radius", 0)),
			]
			draw_string(
				font,
				position + Vector2(radius + 6.0, 14.0),
				auth_label,
				HORIZONTAL_ALIGNMENT_LEFT,
				-1.0,
				11,
				DEBUG_AUTH_COLOR
			)


func _player_name_label(player: Dictionary) -> String:
	return String(player.get("player_name", "Player"))


func _player_resource_label(player: Dictionary) -> String:
	if app_state == null:
		return ""
	if int(player.get("player_id", 0)) != app_state.local_player_id:
		return ""
	return "HP %d  Mana %d" % [
		int(player.get("hit_points", 0)),
		int(player.get("mana", 0)),
	]


func _cast_label_for_player(player: Dictionary) -> String:
	var slot := int(player.get("current_cast_slot", 0))
	if slot <= 0:
		return ""
	if app_state == null:
		return "Slot %d" % slot
	return app_state.player_skill_name_for_slot(int(player.get("player_id", 0)), slot)


func _cast_bar_origin(player: Dictionary, canvas_pos: Vector2, radius: float, bar_width: float) -> Vector2:
	var line_count := 1
	if _player_resource_label(player) != "":
		line_count += 1
	if not Array(player.get("active_statuses", [])).is_empty():
		line_count += 1
	var top_offset := radius + 44.0 + float(max(0, line_count - 1)) * 16.0
	return canvas_pos + Vector2(-bar_width * 0.5, -top_offset)


func _draw_cast_bar(arena_rect: Rect2, player: Dictionary, canvas_pos: Vector2, radius: float) -> void:
	var slot := int(player.get("current_cast_slot", 0))
	if slot <= 0:
		return
	var total_ms := maxf(1.0, float(player.get("current_cast_total_ms", 0)))
	var remaining_ms := clampf(float(player.get("current_cast_remaining_ms", 0)), 0.0, total_ms)
	var progress := clampf(1.0 - (remaining_ms / total_ms), 0.0, 1.0)
	var bar_width := radius * 2.8
	var bar_height := 8.0
	var bar_origin := _cast_bar_origin(player, canvas_pos, radius, bar_width)
	var outer_rect := Rect2(bar_origin + Vector2(-1.0, -1.0), Vector2(bar_width + 2.0, bar_height + 2.0))
	draw_rect(outer_rect, Color(0.02, 0.03, 0.04, 0.92))
	draw_rect(Rect2(bar_origin, Vector2(bar_width, bar_height)), Color(0.11, 0.13, 0.15, 0.92))
	draw_rect(
		Rect2(bar_origin, Vector2(bar_width * progress, bar_height)),
		Color(0.98, 0.84, 0.36, 0.98)
	)
	var font := ThemeDB.fallback_font
	if font != null:
		var label := "%s  %dms" % [_cast_label_for_player(player), int(remaining_ms)]
		draw_string(
			font,
			bar_origin + Vector2(1.0, -5.0),
			label,
			HORIZONTAL_ALIGNMENT_LEFT,
			-1.0,
			12,
			Color(0.02, 0.03, 0.04, 0.88)
		)
		draw_string(
			font,
			bar_origin + Vector2(0.0, -6.0),
			label,
			HORIZONTAL_ALIGNMENT_LEFT,
			-1.0,
			12,
			Color8(242, 240, 230)
		)


func _world_to_canvas(arena_rect: Rect2, world_point: Vector2) -> Vector2:
	var normalized := Vector2(
		(world_point.x / float(app_state.arena_width)) + 0.5,
		(world_point.y / float(app_state.arena_height)) + 0.5
	)
	return arena_rect.position + Vector2(
		normalized.x * arena_rect.size.x,
		normalized.y * arena_rect.size.y
	)


func _world_vector_to_canvas(arena_rect: Rect2, world_vector: Vector2) -> Vector2:
	return Vector2(
		world_vector.x * (arena_rect.size.x / float(app_state.arena_width)),
		world_vector.y * (arena_rect.size.y / float(app_state.arena_height))
	)


func _world_rect_to_canvas(
	arena_rect: Rect2,
	center_x: float,
	center_y: float,
	half_width: float,
	half_height: float
) -> Rect2:
	var center := _world_to_canvas(arena_rect, Vector2(center_x, center_y))
	var size_units := Vector2(half_width * 2.0, half_height * 2.0)
	var scale := Vector2(
		arena_rect.size.x / float(app_state.arena_width),
		arena_rect.size.y / float(app_state.arena_height)
	)
	var draw_size := Vector2(size_units.x * scale.x, size_units.y * scale.y)
	return Rect2(center - draw_size * 0.5, draw_size)


func _world_radius_to_canvas(arena_rect: Rect2, radius_units: float) -> float:
	var scale_x := arena_rect.size.x / float(app_state.arena_width)
	var scale_y := arena_rect.size.y / float(app_state.arena_height)
	return radius_units * minf(scale_x, scale_y)


func _team_color(team_name: String, alive: bool) -> Color:
	var color := Color8(237, 103, 69) if team_name == "Team A" else Color8(96, 197, 224)
	return color if alive else color.darkened(0.45)


func _effect_color(kind_name: String, alpha: float) -> Color:
	match kind_name:
		"MeleeSwing":
			return Color(0.95, 0.77, 0.46, alpha)
		"SkillShot":
			return Color(0.92, 0.52, 0.41, alpha)
		"DashTrail":
			return Color(0.44, 0.85, 0.89, alpha)
		"Burst":
			return Color(0.95, 0.59, 0.48, alpha)
		"Nova":
			return Color(0.60, 0.81, 0.98, alpha)
		"Beam":
			return Color(0.76, 0.93, 0.96, alpha)
		"HitSpark":
			return Color(1.0, 0.95, 0.78, alpha)
		_:
			return Color(0.85, 0.85, 0.85, alpha)
