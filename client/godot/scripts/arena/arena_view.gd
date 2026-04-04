extends Control
class_name ArenaView

const PerfClockScript := preload("res://scripts/debug/perf_clock.gd")
const PADDING := 8.0
const PLAYER_RADIUS_UNITS := 28.0
const FOG_EDGE_EXTENSION_RATIO := 0.52
const FOG_EDGE_ALPHA_SCALE := 0.58
const PLAYER_SHADOW_COLOR := Color(0.03, 0.05, 0.08, 0.26)
const PROJECTILE_SHADOW_COLOR := Color(0.03, 0.05, 0.08, 0.18)
const OBSTACLE_SHADOW_COLOR := Color(0.03, 0.04, 0.06, 0.16)
const DEPLOYABLE_SHADOW_COLOR := Color(0.03, 0.05, 0.08, 0.22)
const FRIENDLY_TEAM_COLOR := Color8(27, 58, 128)
const ENEMY_TEAM_COLOR := Color8(196, 61, 50)
const ARENA_VOID_COLOR := Color8(20, 21, 25)
const ARENA_FLOOR_COLOR := Color8(232, 232, 236)
const GRID_COLOR := Color8(205, 206, 212)

var app_state: ClientState = null
var _last_render_revision: int = -1
var _last_viewport_size: Vector2i = Vector2i.ZERO
var _background_cache_signature: String = ""
var _visibility_cache_signature: String = ""
var _background_texture: ImageTexture = null
var _visibility_texture: ImageTexture = null


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_PASS
	sync_render_state(true)

func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		sync_render_state(true)


func set_client_state(state: ClientState) -> void:
	app_state = state
	sync_render_state(true)


func sync_render_state(force: bool = false) -> void:
	var viewport_size := _viewport_size_key()
	var render_revision := app_state.current_arena_render_revision() if app_state != null else -1
	if not force and render_revision == _last_render_revision and viewport_size == _last_viewport_size:
		return
	_last_render_revision = render_revision
	_last_viewport_size = viewport_size
	_sync_cached_layers()
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
	var started_us := PerfClockScript.now_us()
	var panel_rect := Rect2(Vector2.ZERO, size)
	draw_rect(panel_rect, Color8(30, 26, 24))

	if not has_arena_snapshot():
		_draw_centered_text(
			panel_rect,
			"Waiting for the authoritative arena snapshot..."
		)
		if app_state != null:
			app_state.record_arena_draw(PerfClockScript.elapsed_us(started_us), Vector2.ZERO)
		return

	var arena_rect := _arena_rect()
	var phase_started_us := PerfClockScript.now_us()
	_draw_base_cache(arena_rect)
	_record_draw_phase("arena_draw_base", phase_started_us)
	phase_started_us = PerfClockScript.now_us()
	_draw_visibility_overlay(arena_rect)
	_record_draw_phase("arena_draw_visibility", phase_started_us)
	phase_started_us = PerfClockScript.now_us()
	_draw_effects(arena_rect)
	_record_draw_phase("arena_draw_effects", phase_started_us)
	phase_started_us = PerfClockScript.now_us()
	_draw_deployables(arena_rect)
	_record_draw_phase("arena_draw_deployables", phase_started_us)
	phase_started_us = PerfClockScript.now_us()
	_draw_projectiles(arena_rect)
	_record_draw_phase("arena_draw_projectiles", phase_started_us)
	phase_started_us = PerfClockScript.now_us()
	_draw_players(arena_rect)
	_record_draw_phase("arena_draw_players", phase_started_us)
	phase_started_us = PerfClockScript.now_us()
	_draw_local_combat_texts(arena_rect)
	_record_draw_phase("arena_draw_combat_text", phase_started_us)
	phase_started_us = PerfClockScript.now_us()
	_draw_border(arena_rect)
	_record_draw_phase("arena_draw_border", phase_started_us)
	if app_state != null:
		app_state.record_arena_draw(PerfClockScript.elapsed_us(started_us), arena_rect.size)


func _record_draw_phase(metric_name: String, started_us: int) -> void:
	if app_state != null:
		app_state.record_client_timing(metric_name, PerfClockScript.elapsed_us(started_us))


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


func _draw_base_cache(arena_rect: Rect2) -> void:
	if _background_texture != null:
		draw_texture_rect(_background_texture, arena_rect, false)
		return
	draw_rect(arena_rect, ARENA_VOID_COLOR)


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
		if kind_name == "Aura":
			continue
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
		var radius: float = _world_radius_to_canvas(arena_rect, PLAYER_RADIUS_UNITS)
		var team_border := _player_team_border_color(String(player.get("team", "")), alive)
		var outline: Color = Color.WHITE if is_local_player else Color8(28, 30, 36)

		if is_local_player:
			draw_line(canvas_pos, aim_end, Color(team_border, 0.42), 2.0)
		draw_circle(
			canvas_pos + Vector2(0.0, radius * 0.82),
			radius * 0.96,
			PLAYER_SHADOW_COLOR
		)
		_draw_player_token(player, canvas_pos, radius, team_border, outline, alive)
		_draw_status_halo(player, canvas_pos, radius, alive)

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


func _draw_border(arena_rect: Rect2) -> void:
	draw_rect(arena_rect, Color8(58, 61, 68), false, 3.0)


func _draw_visibility_overlay(arena_rect: Rect2) -> void:
	if _visibility_texture != null:
		draw_texture_rect(_visibility_texture, arena_rect, false)


func _viewport_size_key() -> Vector2i:
	return Vector2i(int(round(size.x)), int(round(size.y)))


func _sync_cached_layers() -> void:
	if app_state == null or not has_arena_snapshot():
		_clear_cached_layers()
		return
	var arena_rect := _arena_rect()
	if arena_rect.size.x <= 0.0 or arena_rect.size.y <= 0.0:
		_clear_cached_layers()
		return
	var started_us := PerfClockScript.now_us()
	_sync_background_cache(arena_rect)
	_sync_visibility_cache(arena_rect)
	if app_state != null:
		app_state.record_client_timing("arena_draw_cache_sync", PerfClockScript.elapsed_us(started_us))


func _sync_background_cache(arena_rect: Rect2) -> void:
	var signature := _background_signature(arena_rect)
	if signature == _background_cache_signature:
		return
	var started_us := PerfClockScript.now_us()
	_background_cache_signature = signature
	_background_texture = _update_cached_texture(_background_texture, _build_background_image(arena_rect))
	if app_state != null:
		app_state.record_client_timing("arena_cache_background", PerfClockScript.elapsed_us(started_us))


func _sync_visibility_cache(arena_rect: Rect2) -> void:
	var signature := _visibility_signature(arena_rect)
	if signature == _visibility_cache_signature:
		return
	var started_us := PerfClockScript.now_us()
	_visibility_cache_signature = signature
	_visibility_texture = _update_cached_texture(_visibility_texture, _build_visibility_image(arena_rect))
	if app_state != null:
		app_state.record_client_timing("arena_cache_visibility", PerfClockScript.elapsed_us(started_us))


func _clear_cached_layers() -> void:
	_background_cache_signature = ""
	_visibility_cache_signature = ""
	_background_texture = null
	_visibility_texture = null


func _background_signature(arena_rect: Rect2) -> String:
	return "%d|%d|%d|%d|%d|%d|%d" % [
		int(round(arena_rect.size.x)),
		int(round(arena_rect.size.y)),
		app_state.arena_width,
		app_state.arena_height,
		app_state.arena_tile_units,
		hash(app_state.footprint_tiles),
		hash(app_state.arena_obstacles),
	]


func _visibility_signature(arena_rect: Rect2) -> String:
	return "%d|%d|%d|%d|%d|%d|%d" % [
		int(round(arena_rect.size.x)),
		int(round(arena_rect.size.y)),
		app_state.arena_width,
		app_state.arena_height,
		app_state.arena_tile_units,
		hash(app_state.visible_tiles),
		hash(app_state.explored_tiles),
	]


func _build_background_image(arena_rect: Rect2) -> Image:
	var image_size := _arena_image_size(arena_rect)
	var image := _create_rgba_image(image_size)
	image.fill(ARENA_VOID_COLOR)
	_paint_floor_runs(image, image_size)
	_paint_grid(image, image_size)
	_paint_obstacles(image, image_size)
	return image


func _build_visibility_image(arena_rect: Rect2) -> Image:
	var image_size := _arena_image_size(arena_rect)
	var image := _create_rgba_image(image_size)
	image.fill(Color(0.0, 0.0, 0.0, 0.0))
	var tile_width := app_state.arena_tile_width()
	var tile_height := app_state.arena_tile_height()
	if tile_width <= 0 or tile_height <= 0 or app_state.arena_tile_units <= 0:
		return image
	for row in range(tile_height):
		var run_start := -1
		var run_explored := false
		for column in range(tile_width + 1):
			var in_fog := (
				column < tile_width
				and app_state.is_tile_in_footprint(column, row)
				and not app_state.is_tile_visible(column, row)
			)
			var explored := in_fog and app_state.is_tile_explored(column, row)
			if in_fog and run_start < 0:
				run_start = column
				run_explored = explored
			elif in_fog and explored != run_explored:
				image.fill_rect(
					_tile_run_image_rect(image_size, run_start, row, column - 1, row),
					_fog_fill_color(run_explored)
				)
				run_start = column
				run_explored = explored
			elif not in_fog and run_start >= 0:
				image.fill_rect(
					_tile_run_image_rect(image_size, run_start, row, column - 1, row),
					_fog_fill_color(run_explored)
				)
				run_start = -1
	return image


func _paint_floor_runs(image: Image, image_size: Vector2i) -> void:
	var tile_width := app_state.arena_tile_width()
	var tile_height := app_state.arena_tile_height()
	if tile_width <= 0 or tile_height <= 0 or app_state.arena_tile_units <= 0:
		image.fill(ARENA_FLOOR_COLOR)
		return
	for row in range(tile_height):
		var run_start := -1
		for column in range(tile_width + 1):
			var in_footprint := column < tile_width and app_state.is_tile_in_footprint(column, row)
			if in_footprint and run_start < 0:
				run_start = column
			elif not in_footprint and run_start >= 0:
				image.fill_rect(
					_tile_run_image_rect(image_size, run_start, row, column - 1, row),
					ARENA_FLOOR_COLOR
				)
				run_start = -1


func _paint_grid(image: Image, image_size: Vector2i) -> void:
	var tile_width := app_state.arena_tile_width()
	var tile_height := app_state.arena_tile_height()
	if tile_width <= 0 or tile_height <= 0 or app_state.arena_tile_units <= 0:
		return
	for column in range(tile_width + 1):
		var x := mini(image_size.x - 1, int(round(float(column) * float(image_size.x) / float(tile_width))))
		image.fill_rect(Rect2i(x, 0, 1, image_size.y), GRID_COLOR)
	for row in range(tile_height + 1):
		var y := mini(image_size.y - 1, int(round(float(row) * float(image_size.y) / float(tile_height))))
		image.fill_rect(Rect2i(0, y, image_size.x, 1), GRID_COLOR)


func _paint_obstacles(image: Image, image_size: Vector2i) -> void:
	for obstacle in app_state.arena_obstacles:
		var rect := _world_rect_to_image(
			image_size,
			float(obstacle.get("center_x", 0)),
			float(obstacle.get("center_y", 0)),
			float(obstacle.get("half_width", 0)),
			float(obstacle.get("half_height", 0))
		)
		if rect.size.x <= 0 or rect.size.y <= 0:
			continue
		var kind_name := String(obstacle.get("kind", ""))
		match kind_name:
			"Shrub":
				image.fill_rect(rect, Color8(185, 215, 180))
			"Pillar":
				image.fill_rect(rect, Color8(84, 84, 93))
				var inset_x := maxi(1, int(round(float(rect.size.x) * 0.24)))
				var inset_y := maxi(1, int(round(float(rect.size.y) * 0.24)))
				var inner := rect.grow_individual(-inset_x, -inset_y, -inset_x, -inset_y)
				if inner.size.x > 0 and inner.size.y > 0:
					image.fill_rect(inner, Color8(203, 217, 228))
			_:
				image.fill_rect(rect, Color8(140, 140, 140))


func _arena_image_size(arena_rect: Rect2) -> Vector2i:
	return Vector2i(
		maxi(1, int(round(arena_rect.size.x))),
		maxi(1, int(round(arena_rect.size.y)))
	)


func _tile_run_image_rect(
	image_size: Vector2i,
	start_column: int,
	start_row: int,
	end_column: int,
	end_row: int
) -> Rect2i:
	var tile_width := app_state.arena_tile_width()
	var tile_height := app_state.arena_tile_height()
	var x0 := int(floor(float(start_column) * float(image_size.x) / float(tile_width)))
	var x1 := int(ceil(float(end_column + 1) * float(image_size.x) / float(tile_width)))
	var y0 := int(floor(float(start_row) * float(image_size.y) / float(tile_height)))
	var y1 := int(ceil(float(end_row + 1) * float(image_size.y) / float(tile_height)))
	var rect := Rect2i(
		clampi(x0, 0, image_size.x),
		clampi(y0, 0, image_size.y),
		maxi(1, x1 - x0),
		maxi(1, y1 - y0)
	)
	return rect.intersection(Rect2i(Vector2i.ZERO, image_size))


func _world_rect_to_image(
	image_size: Vector2i,
	center_x: float,
	center_y: float,
	half_width: float,
	half_height: float
) -> Rect2i:
	var center := _world_to_image(image_size, Vector2(center_x, center_y))
	var scale_x := float(image_size.x) / float(app_state.arena_width)
	var scale_y := float(image_size.y) / float(app_state.arena_height)
	var draw_width := maxi(1, int(round(half_width * 2.0 * scale_x)))
	var draw_height := maxi(1, int(round(half_height * 2.0 * scale_y)))
	var rect := Rect2i(
		int(round(center.x - float(draw_width) * 0.5)),
		int(round(center.y - float(draw_height) * 0.5)),
		draw_width,
		draw_height
	)
	return rect.intersection(Rect2i(Vector2i.ZERO, image_size))


func _world_to_image(image_size: Vector2i, world_point: Vector2) -> Vector2:
	var normalized := Vector2(
		(world_point.x / float(app_state.arena_width)) + 0.5,
		(world_point.y / float(app_state.arena_height)) + 0.5
	)
	return Vector2(
		normalized.x * float(image_size.x),
		normalized.y * float(image_size.y)
	)


func _update_cached_texture(texture: ImageTexture, image: Image) -> ImageTexture:
	if texture != null \
		and texture.get_width() == image.get_width() \
		and texture.get_height() == image.get_height():
		texture.update(image)
		return texture
	return ImageTexture.create_from_image(image)


func _create_rgba_image(image_size: Vector2i) -> Image:
	return Image.create(image_size.x, image_size.y, false, Image.FORMAT_RGBA8)


func _has_fog_edge(column: int, row: int, direction: Vector2i) -> bool:
	if app_state == null or not app_state.is_tile_in_footprint(column, row) or app_state.is_tile_visible(column, row):
		return false
	return app_state.is_tile_visible(column + direction.x, row + direction.y)


func _fog_fill_color(explored: bool) -> Color:
	return (
		Color(0.06, 0.08, 0.11, 0.56)
		if explored
		else Color(0.03, 0.04, 0.05, 0.96)
	)


func _draw_centered_text(rect: Rect2, text: String) -> void:
	var font := ThemeDB.fallback_font
	if font == null:
		return
	var size_px := 18
	var text_width := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, size_px).x
	var baseline := rect.position + Vector2((rect.size.x - text_width) * 0.5, rect.size.y * 0.5)
	draw_string(font, baseline, text, HORIZONTAL_ALIGNMENT_LEFT, -1.0, size_px, Color8(58, 61, 68))


func _draw_player_token(
	player: Dictionary,
	canvas_pos: Vector2,
	radius: float,
	team_border: Color,
	outline: Color,
	alive: bool
) -> void:
	draw_circle(
		canvas_pos,
		radius + 8.0,
		Color(team_border.r, team_border.g, team_border.b, 0.13)
	)
	draw_circle(canvas_pos, radius + 5.0, team_border)
	draw_circle(canvas_pos, radius + 2.3, Color(0.08, 0.09, 0.11, 0.96))
	var tree_names: Array = player.get("equipped_skill_trees", [])
	for slot in range(5, 0, -1):
		var band_radius := radius * (float(slot) / 5.0)
		var tree_name := ""
		if tree_names.size() >= slot:
			tree_name = String(tree_names[slot - 1])
		var band_color := _skill_tree_color(tree_name, alive)
		draw_circle(canvas_pos, band_radius, band_color)
	draw_circle(canvas_pos, maxf(radius * 0.18, 4.0), Color(1.0, 1.0, 1.0, 0.15))
	draw_arc(canvas_pos, radius + 1.6, 0.0, TAU, 28, outline, 1.4, true)


func _draw_status_halo(player: Dictionary, canvas_pos: Vector2, radius: float, alive: bool) -> void:
	var statuses: Array = player.get("active_statuses", [])
	if statuses.is_empty():
		return
	var positives: Array[Dictionary] = []
	var negatives: Array[Dictionary] = []
	for status_variant in statuses:
		var status := (status_variant as Dictionary).duplicate(true)
		if _is_positive_status(String(status.get("kind", ""))):
			positives.append(status)
		else:
			negatives.append(status)
	positives.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return int(a.get("remaining_ms", 0)) > int(b.get("remaining_ms", 0))
	)
	negatives.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return int(a.get("remaining_ms", 0)) > int(b.get("remaining_ms", 0))
	)
	var halo_radius := radius + 11.0
	var halo_width := maxf(radius * 0.16, 3.0)
	_draw_status_arc_stack(canvas_pos, halo_radius, halo_width, positives, -PI / 3.0, PI / 3.0, alive)
	_draw_status_arc_stack(canvas_pos, halo_radius, halo_width, negatives, PI * 2.0 / 3.0, PI * 4.0 / 3.0, alive)


func _draw_status_arc_stack(
	canvas_pos: Vector2,
	halo_radius: float,
	halo_width: float,
	statuses: Array[Dictionary],
	start_angle: float,
	end_angle: float,
	alive: bool
) -> void:
	if statuses.is_empty():
		return
	var span := (end_angle - start_angle) / float(statuses.size())
	for index in range(statuses.size()):
		var status: Dictionary = statuses[index]
		var from := start_angle + float(index) * span
		var to := from + span
		var color := _status_color(String(status.get("kind", "")), alive)
		draw_arc(canvas_pos, halo_radius, from, to, 10, color, halo_width, true)


func _draw_local_combat_texts(arena_rect: Rect2) -> void:
	if app_state == null:
		return
	var font := ThemeDB.fallback_font
	if font == null:
		return
	for entry in app_state.local_combat_text_entries():
		var ttl_max := maxf(0.01, float(entry.get("ttl_max", 0.01)))
		var ttl := clampf(float(entry.get("ttl", 0.0)), 0.0, ttl_max)
		var life_ratio := clampf(1.0 - (ttl / ttl_max), 0.0, 1.0)
		var position := _world_to_canvas(
			arena_rect,
			Vector2(float(entry.get("x", 0)), float(entry.get("y", 0)))
		)
		position += Vector2(float(entry.get("jitter_x", 0.0)), -28.0 - life_ratio * 26.0)
		var text := String(entry.get("text", ""))
		var color := _combat_text_color(String(entry.get("style", "")))
		color.a = clampf(ttl / ttl_max, 0.1, 1.0)
		draw_string(
			font,
			position + Vector2(1.0, 1.0),
			text,
			HORIZONTAL_ALIGNMENT_LEFT,
			-1.0,
			16,
			Color(0.03, 0.03, 0.04, color.a * 0.8)
		)
		draw_string(
			font,
			position,
			text,
			HORIZONTAL_ALIGNMENT_LEFT,
			-1.0,
			16,
			color
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


func _friendly_team_name() -> String:
	if app_state == null:
		return ""
	var local_player := app_state.local_arena_player()
	if not local_player.is_empty():
		return String(local_player.get("team", ""))
	var self_entry := app_state.self_entry()
	if not self_entry.is_empty():
		return String(self_entry.get("team", ""))
	return ""


func _player_team_border_color(team_name: String, alive: bool) -> Color:
	var color := FRIENDLY_TEAM_COLOR if team_name == _friendly_team_name() else ENEMY_TEAM_COLOR
	return color if alive else color.darkened(0.45)


func _team_color(team_name: String, alive: bool) -> Color:
	var color := Color8(237, 103, 69) if team_name == "Team A" else Color8(96, 197, 224)
	return color if alive else color.darkened(0.45)


func _skill_tree_color(tree_name: String, alive: bool) -> Color:
	var normalized := tree_name.strip_edges().to_lower()
	var color := Color8(0, 0, 0)
	match normalized:
		"warrior":
			color = Color8(199, 156, 110)
		"mage":
			color = Color8(105, 204, 240)
		"rogue":
			color = Color8(255, 245, 105)
		"paladin":
			color = Color8(245, 140, 186)
		"druid":
			color = Color8(255, 125, 10)
		"ranger":
			color = Color8(171, 212, 115)
		"cleric":
			color = Color8(255, 255, 255)
		"bard":
			color = Color8(140, 59, 255)
		"necromancer":
			color = Color8(107, 0, 79)
		_:
			color = Color8(0, 0, 0)
	return color if alive else color.darkened(0.5)


func _is_positive_status(status_kind: String) -> bool:
	return status_kind in ["Hot", "Haste", "Shield", "Stealth"]


func _status_color(status_kind: String, alive: bool) -> Color:
	var color := Color8(196, 196, 196)
	match status_kind:
		"Poison":
			color = Color8(88, 192, 74)
		"Hot":
			color = Color8(91, 224, 160)
		"Chill":
			color = Color8(104, 198, 255)
		"Root":
			color = Color8(168, 142, 74)
		"Haste":
			color = Color8(255, 214, 99)
		"Silence":
			color = Color8(129, 116, 209)
		"Stun":
			color = Color8(255, 170, 64)
		"Sleep":
			color = Color8(176, 149, 255)
		"Shield":
			color = Color8(147, 202, 255)
		"Stealth":
			color = Color8(112, 120, 132)
		"Reveal":
			color = Color8(255, 94, 166)
		"Fear":
			color = Color8(198, 58, 74)
		_:
			color = Color8(196, 196, 196)
	return color if alive else color.darkened(0.48)


func _combat_text_color(style_name: String) -> Color:
	match style_name:
		"DamageOutgoing":
			return Color8(255, 220, 117)
		"DamageIncoming":
			return Color8(255, 122, 122)
		"HealOutgoing":
			return Color8(138, 255, 168)
		"HealIncoming":
			return Color8(112, 242, 146)
		"PositiveStatus":
			return Color8(143, 224, 255)
		"NegativeStatus":
			return Color8(255, 150, 213)
		_:
			return Color8(224, 231, 240)


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
