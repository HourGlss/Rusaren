extends SceneTree

const MainScript := preload("res://scripts/main.gd")
const GodotPerfMonitorsScript := preload("res://scripts/debug/godot_perf_monitors.gd")

const SAMPLE_FRAME_COUNT := 45
const CUSTOM_MONITORS := {
	"ui_refresh_ms": "Rarena/UIRefreshMs",
	"arena_draw_ms": "Rarena/ArenaDrawMs",
	"arena_visibility_ms": "Rarena/ArenaVisibilityMs",
	"arena_base_draw_ms": "Rarena/ArenaBaseDrawMs",
	"arena_cache_sync_ms": "Rarena/ArenaCacheSyncMs",
	"arena_cache_background_ms": "Rarena/ArenaCacheBackgroundMs",
	"arena_cache_visibility_ms": "Rarena/ArenaCacheVisibilityMs",
	"players": "Rarena/Players",
	"visible_tiles": "Rarena/VisibleTiles",
}

var _shell: Control = null


func _init() -> void:
	call_deferred("_run")


func _run() -> void:
	var success := await _collect_runtime_monitors()
	quit(0 if success else 1)


func _collect_runtime_monitors() -> bool:
	_shell = MainScript.new()
	_shell.auto_connect_enabled = false
	get_root().add_child(_shell)
	get_root().size = Vector2i(1280, 720)
	_shell.size = Vector2(1280.0, 720.0)
	await process_frame
	await process_frame

	_prime_shell_state()

	var built_in_samples := _new_sample_map(GodotPerfMonitorsScript.BUILTIN_MONITOR_ORDER)
	var custom_samples := _new_sample_map(CUSTOM_MONITORS.keys())

	for _frame_index in range(SAMPLE_FRAME_COUNT):
		_shell._refresh_ui()
		await process_frame
		_sample_builtin_monitors(built_in_samples)
		_sample_custom_monitors(custom_samples)

	var built_in_summary := _sample_map_summary(built_in_samples)
	var custom_summary := _sample_map_summary(custom_samples)
	var pre_cleanup_monitors := GodotPerfMonitorsScript.snapshot_builtin_monitors()

	var success := true
	success = _assert_metric_sampled(built_in_summary, "node_count", "built-in node count") and success
	success = _assert_metric_sampled(custom_summary, "ui_refresh_ms", "custom ui-refresh monitor") and success
	success = _assert_metric_sampled(custom_summary, "arena_draw_ms", "custom arena-draw monitor") and success
	success = _assert_metric_sampled(custom_summary, "arena_visibility_ms", "custom visibility monitor") and success
	success = _assert_metric_sampled(custom_summary, "arena_base_draw_ms", "custom arena-base-draw monitor") and success
	success = _assert_metric_sampled(custom_summary, "arena_cache_sync_ms", "custom arena-cache-sync monitor") and success
	success = _assert_metric_sampled(custom_summary, "arena_cache_background_ms", "custom arena-cache-background monitor") and success
	success = _assert_metric_sampled(custom_summary, "arena_cache_visibility_ms", "custom arena-cache-visibility monitor") and success

	_shell.queue_free()
	await process_frame
	await process_frame
	var post_cleanup_monitors := GodotPerfMonitorsScript.snapshot_builtin_monitors()

	var output_payload := {
		"generated_at_utc": Time.get_datetime_string_from_system(true, true),
		"godot_version": Engine.get_version_info(),
		"scenario": "match_combat_reference",
		"viewport": {
			"width": get_root().size.x,
			"height": get_root().size.y,
		},
		"built_in": built_in_summary,
		"custom": custom_summary,
		"pre_cleanup_builtin": pre_cleanup_monitors,
		"post_cleanup_builtin": post_cleanup_monitors,
	}
	success = _write_output(output_payload) and success
	return success


func _prime_shell_state() -> void:
	_shell.app_state.mark_transport_state("open")
	_shell.app_state.local_player_id = 11
	_shell.app_state.local_player_name = "Alice"
	_shell.app_state.apply_server_event({
		"type": "Connected",
		"player_id": 11,
		"player_name": "Alice",
		"record": {
			"wins": 0,
			"losses": 0,
			"no_contests": 0,
		},
		"skill_catalog": [
			{
				"tree": "Mage",
				"tier": 1,
				"skill_id": "mage_t1_missile",
				"skill_name": "Magic Missile",
				"skill_description": "Fast projectile damage.",
				"skill_summary": "CD 0.7s | Cast instant | Mana 16\nProjectile: range 1500, radius 16, speed 310\nEffect: 10 damage",
				"ui_category": "damage",
			},
			{
				"tree": "Mage",
				"tier": 2,
				"skill_id": "mage_t2_ice_lance",
				"skill_name": "Ice Lance",
				"skill_description": "Burst damage with chill.",
				"skill_summary": "CD 2.0s | Cast instant | Mana 30\nBurst: cast range 250, radius 86\nEffect: 14 damage\nStatus: Chill 20 for 2s (max 2)",
				"ui_category": "control",
			},
		],
	})
	_shell.app_state.apply_server_event({
		"type": "MatchStarted",
		"match_id": 1,
		"round": 1,
		"skill_pick_seconds": 25,
	})
	_shell.app_state.apply_server_event({
		"type": "SkillChosen",
		"player_id": 11,
		"tree": "Mage",
		"tier": 1,
	})
	_shell.app_state.local_round_skill_locked = false
	_shell.app_state.apply_server_event({
		"type": "RoundWon",
		"round": 1,
		"winning_team": "Team A",
		"score_a": 1,
		"score_b": 0,
	})
	_shell.app_state.apply_server_event({
		"type": "SkillChosen",
		"player_id": 11,
		"tree": "Mage",
		"tier": 2,
	})
	_shell.app_state.apply_server_event({
		"type": "CombatStarted",
	})
	_shell.app_state.apply_server_event({
		"type": "ArenaStateSnapshot",
		"snapshot": {
			"mode": "Match",
			"phase": "Combat",
			"width": 1750,
			"height": 950,
			"tile_units": 50,
			"footprint_tiles": _full_mask(35, 19),
			"visible_tiles": _rect_mask(35, 19, Rect2i(0, 0, 16, 19)),
			"explored_tiles": _full_mask(35, 19),
			"obstacles": [
				{
					"kind": "Pillar",
					"center_x": -225,
					"center_y": -125,
					"half_width": 50,
					"half_height": 50,
				},
				{
					"kind": "Shrub",
					"center_x": -225,
					"center_y": -125,
					"half_width": 92,
					"half_height": 92,
				},
				{
					"kind": "Pillar",
					"center_x": 225,
					"center_y": 125,
					"half_width": 50,
					"half_height": 50,
				},
			],
			"players": [
				{
					"player_id": 11,
					"player_name": "Alice",
					"team": "Team A",
					"x": -600,
					"y": 180,
					"aim_x": 220,
					"aim_y": -40,
					"hit_points": 100,
					"max_hit_points": 100,
					"mana": 82,
					"max_mana": 100,
					"alive": true,
					"unlocked_skill_slots": 2,
					"primary_cooldown_remaining_ms": 150,
					"primary_cooldown_total_ms": 600,
					"slot_cooldown_remaining_ms": [0, 250, 0, 0, 0],
					"slot_cooldown_total_ms": [800, 900, 0, 0, 0],
					"equipped_skill_trees": ["Mage", "Mage", "", "", ""],
					"current_cast_slot": 2,
					"current_cast_remaining_ms": 220,
					"current_cast_total_ms": 350,
					"active_statuses": [
						{
							"kind": "Shield",
							"remaining_duration_ms": 800,
						},
					],
				},
				{
					"player_id": 22,
					"player_name": "Bob",
					"team": "Team B",
					"x": 250,
					"y": 40,
					"aim_x": -180,
					"aim_y": 30,
					"hit_points": 84,
					"max_hit_points": 100,
					"mana": 64,
					"max_mana": 100,
					"alive": true,
					"unlocked_skill_slots": 2,
					"primary_cooldown_remaining_ms": 0,
					"primary_cooldown_total_ms": 600,
					"slot_cooldown_remaining_ms": [0, 0, 0, 0, 0],
					"slot_cooldown_total_ms": [800, 900, 0, 0, 0],
					"equipped_skill_trees": ["Mage", "Mage", "", "", ""],
					"current_cast_slot": 0,
					"current_cast_remaining_ms": 0,
					"current_cast_total_ms": 0,
					"active_statuses": [],
				},
			],
			"projectiles": [
				{
					"owner": 11,
					"slot": 1,
					"kind": "SkillShot",
					"x": -260,
					"y": 160,
					"radius": 18,
				},
			],
			"deployables": [
				{
					"id": 91,
					"owner": 11,
					"team": "Team A",
					"kind": "Ward",
					"x": -360,
					"y": 120,
					"radius": 60,
					"hit_points": 120,
					"max_hit_points": 120,
					"remaining_ms": 2000,
				},
			],
		},
	})
	_shell.app_state.apply_server_event({
		"type": "ArenaEffectBatch",
		"effects": [
			{
				"kind": "SkillShot",
				"owner": 11,
				"slot": 1,
				"x": -600,
				"y": 180,
				"target_x": -260,
				"target_y": 160,
				"radius": 18,
			},
		],
	})
	_shell._refresh_ui()


func _new_sample_map(metric_names: Array) -> Dictionary:
	var samples := {}
	for metric_name in metric_names:
		samples[String(metric_name)] = []
	return samples


func _sample_builtin_monitors(samples: Dictionary) -> void:
	var snapshot := GodotPerfMonitorsScript.snapshot_builtin_monitors()
	_shell.app_state.record_godot_monitor_snapshot(snapshot)
	for metric_name in GodotPerfMonitorsScript.BUILTIN_MONITOR_ORDER:
		var values: Array = samples.get(metric_name, [])
		values.append(float(snapshot.get(metric_name, 0.0)))
		samples[metric_name] = values


func _sample_custom_monitors(samples: Dictionary) -> void:
	for metric_name in CUSTOM_MONITORS.keys():
		var monitor_id := String(CUSTOM_MONITORS[metric_name])
		var values: Array = samples.get(metric_name, [])
		var raw_value := float(Performance.get_custom_monitor(monitor_id))
		if metric_name.ends_with("_ms"):
			values.append(raw_value * 1000.0)
		else:
			values.append(raw_value)
		samples[metric_name] = values


func _sample_map_summary(samples: Dictionary) -> Dictionary:
	var summary := {}
	for metric_name in samples.keys():
		summary[metric_name] = _series_summary(samples[metric_name])
	return summary


func _series_summary(series: Array) -> Dictionary:
	if series.is_empty():
		return {
			"count": 0,
			"last": 0.0,
			"avg": 0.0,
			"min": 0.0,
			"p95": 0.0,
			"max": 0.0,
		}

	var sorted := series.duplicate()
	sorted.sort()
	var total := 0.0
	for value in series:
		total += float(value)
	var p95_index := mini(sorted.size() - 1, int(floor(float(sorted.size() - 1) * 0.95)))
	return {
		"count": series.size(),
		"last": float(series[series.size() - 1]),
		"avg": total / float(series.size()),
		"min": float(sorted[0]),
		"p95": float(sorted[p95_index]),
		"max": float(sorted[sorted.size() - 1]),
	}


func _assert_metric_sampled(summary: Dictionary, metric_name: String, label: String) -> bool:
	var metric: Dictionary = summary.get(metric_name, {})
	if int(metric.get("count", 0)) <= 0:
		return _fail("%s should produce at least one sample" % label)
	return true


func _write_output(payload: Dictionary) -> bool:
	var output_path := OS.get_environment("RARENA_FRONTEND_MONITOR_OUTPUT").strip_edges()
	if output_path == "":
		output_path = ProjectSettings.globalize_path("user://runtime_monitors.json")
	output_path = output_path.replace("\\", "/")
	var parent_dir := output_path.get_base_dir()
	if parent_dir != "" and not DirAccess.dir_exists_absolute(parent_dir):
		var mkdir_error := DirAccess.make_dir_recursive_absolute(parent_dir)
		if mkdir_error != OK:
			return _fail("failed to create runtime monitor output directory: %s" % output_path)
	var output_file := FileAccess.open(output_path, FileAccess.WRITE)
	if output_file == null:
		return _fail("failed to open runtime monitor output file: %s" % output_path)
	output_file.store_string(JSON.stringify(payload, "\t"))
	output_file.close()
	return true


func _full_mask(tile_width: int, tile_height: int) -> PackedByteArray:
	return _mask_for_tiles(tile_width, tile_height, Rect2i(0, 0, tile_width, tile_height))


func _rect_mask(tile_width: int, tile_height: int, rect: Rect2i) -> PackedByteArray:
	return _mask_for_tiles(tile_width, tile_height, rect)


func _mask_for_tiles(tile_width: int, tile_height: int, rect: Rect2i) -> PackedByteArray:
	var mask := PackedByteArray()
	var bit_count := tile_width * tile_height
	mask.resize(int(ceili(float(bit_count) / 8.0)))
	for row in range(rect.position.y, rect.position.y + rect.size.y):
		for column in range(rect.position.x, rect.position.x + rect.size.x):
			var index := row * tile_width + column
			var byte_index := int(index / 8)
			var bit_index := int(index % 8)
			mask[byte_index] = int(mask[byte_index]) | (1 << bit_index)
	return mask


func _fail(message: String) -> bool:
	printerr(message)
	return false
