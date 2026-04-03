extends RefCounted
class_name ClientStateView


static func roster_lines(roster: Dictionary) -> Array[String]:
	var lines: Array[String] = []
	var ids := roster.keys()
	ids.sort()
	for player_id in ids:
		var member: Dictionary = roster[player_id]
		lines.append("%s  |  %s  |  %s  |  %s" % [
			member.get("name", "Player %d" % int(player_id)),
			member.get("team", "Unassigned"),
			member.get("ready", "Not Ready"),
			member.get("skill", "No skill locked"),
		])
	if lines.is_empty():
		lines.append("No tracked players yet.")
	return lines


static func lobby_roster_lines(roster: Dictionary) -> Array[String]:
	var lines: Array[String] = []
	var ids := roster.keys()
	ids.sort()
	for player_id in ids:
		var member: Dictionary = roster[player_id]
		lines.append("%s  |  %s  |  %s" % [
			member.get("name", "Player %d" % int(player_id)),
			member.get("team", "Unassigned"),
			member.get("ready", "Not Ready"),
		])
	if lines.is_empty():
		lines.append("No players are currently in this lobby.")
	return lines


static func lobby_directory_lines(lobby_directory: Array[Dictionary]) -> Array[String]:
	var lines: Array[String] = []
	for lobby in lobby_directory:
		var phase: Dictionary = lobby.get("phase", {})
		var phase_name := String(phase.get("name", "Open"))
		var seconds_remaining := int(phase.get("seconds_remaining", 0))
		var phase_text := phase_name
		if phase_name == "Launch Countdown":
			phase_text = "%s (%ds)" % [phase_name, seconds_remaining]
		lines.append(
			"Lobby #%d  |  players %d  |  A %d  B %d  |  ready %d  |  %s" % [
				int(lobby.get("lobby_id", 0)),
				int(lobby.get("player_count", 0)),
				int(lobby.get("team_a_count", 0)),
				int(lobby.get("team_b_count", 0)),
				int(lobby.get("ready_count", 0)),
				phase_text,
			]
		)
	if lines.is_empty():
		lines.append("No active game lobbies.")
	return lines


static func lobby_directory_bbcode(lobby_directory: Array[Dictionary]) -> String:
	var lines: Array[String] = []
	for lobby in lobby_directory:
		var lobby_id := int(lobby.get("lobby_id", 0))
		var phase: Dictionary = lobby.get("phase", {})
		var phase_name := String(phase.get("name", "Open"))
		var seconds_remaining := int(phase.get("seconds_remaining", 0))
		var phase_text := phase_name
		if phase_name == "Launch Countdown":
			phase_text = "%s (%ds)" % [phase_name, seconds_remaining]
		var join_text := "[url=%d]Join[/url]" % lobby_id if phase_name == "Open" else "[color=#c98e78]Locked[/color]"
		lines.append(
			"%s  Lobby #%d  |  players %d  |  A %d  B %d  |  ready %d  |  %s" % [
				join_text,
				lobby_id,
				int(lobby.get("player_count", 0)),
				int(lobby.get("team_a_count", 0)),
				int(lobby.get("team_b_count", 0)),
				int(lobby.get("ready_count", 0)),
				phase_text,
			]
		)
	if lines.is_empty():
		lines.append("No active game lobbies.")
	return "\n".join(lines)


static func record_text(record: Dictionary) -> String:
	return "W-L-NC  %d-%d-%d" % [
		int(record.get("wins", 0)),
		int(record.get("losses", 0)),
		int(record.get("no_contests", 0)),
	]


static func score_text(score_a: int, score_b: int) -> String:
	return "Team A %d  :  %d Team B" % [score_a, score_b]


static func event_log_text(recent_events: Array[String]) -> String:
	return "\n".join(recent_events)


static func round_summary_text(round_summary: Dictionary, current_round: int) -> String:
	if round_summary.is_empty():
		return ""
	var round_number := int(round_summary.get("round", current_round))
	var lines: Array[String] = []
	lines.append("Round %d" % round_number)
	lines.append("This round")
	lines.append_array(_summary_block_lines(round_summary.get("round_totals", [])))
	lines.append("")
	lines.append("Running total")
	lines.append_array(_summary_block_lines(round_summary.get("running_totals", [])))
	return "\n".join(lines)


static func match_summary_text(match_summary: Dictionary) -> String:
	if match_summary.is_empty():
		return ""
	var rounds_played := int(match_summary.get("rounds_played", 0))
	var lines: Array[String] = []
	lines.append("Rounds played: %d" % rounds_played)
	lines.append_array(_summary_block_lines(match_summary.get("totals", [])))
	return "\n".join(lines)


static func lobby_note() -> String:
	return "Click an open lobby in the directory, enter a manual lobby ID, or start a solo training session. Use Menu for your alias, record, roster, and event views while the backend keeps directory, roster, and arena state authoritative."


static func can_join_or_create_lobby(transport_state: String, screen: String) -> bool:
	return transport_state == "open" and screen == "central"


static func can_start_training(transport_state: String, screen: String) -> bool:
	return transport_state == "open" and screen == "central"


static func can_manage_lobby(transport_state: String, screen: String, lobby_locked: bool) -> bool:
	return transport_state == "open" and screen == "lobby" and not lobby_locked


static func can_leave_lobby(transport_state: String, screen: String, lobby_locked: bool) -> bool:
	return transport_state == "open" and screen == "lobby" and not lobby_locked


static func can_choose_skill(
	transport_state: String,
	screen: String,
	training_mode: bool,
	match_phase: String,
	local_round_skill_locked: bool
) -> bool:
	if training_mode:
		return transport_state == "open" and screen == "match"
	return (
		transport_state == "open"
		and screen == "match"
		and match_phase == "skill_pick"
		and not local_round_skill_locked
	)


static func next_skill_tier_for(
	tree_name: String,
	skill_catalog: Array[Dictionary],
	local_skill_progress: Dictionary
) -> int:
	if tree_name == "":
		return 0
	if not skill_tree_names(skill_catalog, local_skill_progress).has(tree_name):
		return 0
	var current_tier := int(local_skill_progress.get(tree_name, 0))
	if current_tier >= 5:
		return 0
	return current_tier + 1


static func can_choose_skill_option(
	tree_name: String,
	tier: int,
	transport_state: String,
	screen: String,
	training_mode: bool,
	match_phase: String,
	local_round_skill_locked: bool,
	skill_catalog: Array[Dictionary],
	local_skill_progress: Dictionary
) -> bool:
	if training_mode:
		return transport_state == "open" and screen == "match" and tier >= 1 and tier <= 5
	return can_choose_skill(
		transport_state,
		screen,
		training_mode,
		match_phase,
		local_round_skill_locked
	) and tier == next_skill_tier_for(tree_name, skill_catalog, local_skill_progress)


static func skill_tree_names(
	skill_catalog: Array[Dictionary],
	local_skill_progress: Dictionary
) -> Array[String]:
	var ordered: Array[String] = []
	for entry in skill_catalog:
		var tree_name := String(entry.get("tree", ""))
		if tree_name != "" and not ordered.has(tree_name):
			ordered.append(tree_name)
	if ordered.is_empty():
		for tree_name in local_skill_progress.keys():
			ordered.append(String(tree_name))
	return ordered


static func skill_entries_for(skill_catalog: Array[Dictionary], tree_name: String) -> Array[Dictionary]:
	var entries: Array[Dictionary] = []
	for entry in skill_catalog:
		if String(entry.get("tree", "")) == tree_name:
			entries.append((entry as Dictionary).duplicate(true))
	entries.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		return int(a.get("tier", 0)) < int(b.get("tier", 0))
	)
	return entries


static func skill_name_for(skill_catalog: Array[Dictionary], tree_name: String, tier: int) -> String:
	for entry in skill_catalog:
		if String(entry.get("tree", "")) == tree_name and int(entry.get("tier", 0)) == tier:
			return String(entry.get("skill_name", "%s %d" % [tree_name, tier]))
	return "%s %d" % [tree_name, tier]


static func skill_description_for(
	skill_catalog: Array[Dictionary],
	tree_name: String,
	tier: int
) -> String:
	var entry := _skill_catalog_entry(skill_catalog, tree_name, tier)
	if entry.is_empty():
		return ""
	return String(entry.get("skill_description", ""))


static func skill_summary_for(skill_catalog: Array[Dictionary], tree_name: String, tier: int) -> String:
	var entry := _skill_catalog_entry(skill_catalog, tree_name, tier)
	if entry.is_empty():
		return ""
	return String(entry.get("skill_summary", ""))


static func skill_ui_category_for(
	skill_catalog: Array[Dictionary],
	tree_name: String,
	tier: int
) -> String:
	var entry := _skill_catalog_entry(skill_catalog, tree_name, tier)
	if entry.is_empty():
		return "neutral"
	return String(entry.get("ui_category", "neutral"))


static func skill_tooltip_for(skill_catalog: Array[Dictionary], tree_name: String, tier: int) -> String:
	var description := skill_description_for(skill_catalog, tree_name, tier)
	var summary := skill_summary_for(skill_catalog, tree_name, tier)
	var parts: Array[String] = []
	if description != "":
		parts.append(description)
	if summary != "":
		if not parts.is_empty():
			parts.append("")
		parts.append(summary)
	return "\n".join(parts)


static func skill_catalog_signature(skill_catalog: Array[Dictionary]) -> String:
	var parts: Array[String] = []
	for entry in skill_catalog:
		parts.append("%s:%d:%s:%s:%s:%s:%s" % [
			String(entry.get("tree", "")),
			int(entry.get("tier", 0)),
			String(entry.get("skill_id", "")),
			String(entry.get("skill_name", "")),
			String(entry.get("skill_description", "")),
			String(entry.get("skill_summary", "")),
			String(entry.get("ui_category", "")),
		])
	return "|".join(parts)


static func can_quit_results(transport_state: String, screen: String) -> bool:
	return transport_state == "open" and screen == "results"


static func can_reset_training(transport_state: String, training_mode: bool) -> bool:
	return transport_state == "open" and training_mode


static func can_quit_arena(transport_state: String, screen: String, training_mode: bool) -> bool:
	return transport_state == "open" and (screen == "results" or training_mode)


static func can_send_combat_input(
	transport_state: String,
	screen: String,
	training_mode: bool,
	match_phase: String,
	player: Dictionary
) -> bool:
	return (
		transport_state == "open"
		and screen == "match"
		and (match_phase == "combat" or training_mode)
		and not player.is_empty()
		and bool(player.get("alive", false))
	)


static func can_use_combat_slot(
	slot: int,
	transport_state: String,
	screen: String,
	training_mode: bool,
	match_phase: String,
	player: Dictionary
) -> bool:
	if slot < 1 or slot > 5:
		return false
	var cooldowns: Array = player.get("slot_cooldown_remaining_ms", [])
	var remaining := int(cooldowns[slot - 1]) if cooldowns.size() >= slot else 0
	return (
		can_send_combat_input(transport_state, screen, training_mode, match_phase, player)
		and int(player.get("current_cast_slot", 0)) == 0
		and slot <= int(player.get("unlocked_skill_slots", 0))
		and remaining <= 0
	)


static func can_use_primary_attack(
	transport_state: String,
	screen: String,
	training_mode: bool,
	match_phase: String,
	player: Dictionary
) -> bool:
	return (
		can_send_combat_input(transport_state, screen, training_mode, match_phase, player)
		and int(player.get("current_cast_slot", 0)) == 0
		and int(player.get("primary_cooldown_remaining_ms", 0)) <= 0
	)


static func local_skill_name_for_slot(local_skill_loadout: Array[Dictionary], slot: int) -> String:
	if slot <= 0:
		return "Awaiting pick"
	if slot <= local_skill_loadout.size():
		return String(local_skill_loadout[slot - 1].get("skill_name", "Awaiting pick"))
	return "Awaiting pick"


static func player_skill_name_for_slot(
	local_player_id: int,
	local_skill_loadout: Array[Dictionary],
	roster: Dictionary,
	player_id: int,
	slot: int
) -> String:
	if slot <= 0:
		return "Melee"
	if player_id == local_player_id:
		return local_skill_name_for_slot(local_skill_loadout, slot)
	if roster.has(player_id):
		var member: Dictionary = roster[player_id]
		var skill_name := String(member.get("skill", ""))
		if skill_name != "" and skill_name != "Awaiting next pick" and skill_name != "No skill locked":
			return skill_name
	return "Slot %d" % slot


static func cooldown_summary_text(
	local_player: Dictionary,
	local_skill_loadout: Array[Dictionary]
) -> String:
	if local_player.is_empty():
		return "Cooldowns: waiting for a local combat snapshot."

	var labels: Array[String] = []
	var cast_label := _cast_cooldown_label(local_player, local_skill_loadout)
	if cast_label != "":
		labels.append(cast_label)
	labels.append(_primary_cooldown_label(local_player))
	labels.append_array(_ability_cooldown_labels(local_player, local_skill_loadout))
	return "Cooldowns: %s" % "  |  ".join(labels)


static func _cast_cooldown_label(
	local_player: Dictionary,
	local_skill_loadout: Array[Dictionary]
) -> String:
	var current_cast_slot := int(local_player.get("current_cast_slot", 0))
	if current_cast_slot <= 0:
		return ""
	return "Casting %s %d/%dms" % [
		local_skill_name_for_slot(local_skill_loadout, current_cast_slot),
		int(local_player.get("current_cast_remaining_ms", 0)),
		int(local_player.get("current_cast_total_ms", 0)),
	]


static func _primary_cooldown_label(local_player: Dictionary) -> String:
	return "Melee %s" % _cooldown_token(
		int(local_player.get("primary_cooldown_remaining_ms", 0)),
		int(local_player.get("primary_cooldown_total_ms", 0))
	)


static func _ability_cooldown_labels(
	local_player: Dictionary,
	local_skill_loadout: Array[Dictionary]
) -> Array[String]:
	var labels: Array[String] = []
	var unlocked_slots := int(local_player.get("unlocked_skill_slots", 0))
	var remaining_list: Array = local_player.get("slot_cooldown_remaining_ms", [])
	var total_list: Array = local_player.get("slot_cooldown_total_ms", [])
	for slot in range(1, 6):
		labels.append(_slot_cooldown_label(
			local_skill_loadout,
			unlocked_slots,
			remaining_list,
			total_list,
			slot
		))
	return labels


static func _slot_cooldown_label(
	local_skill_loadout: Array[Dictionary],
	unlocked_slots: int,
	remaining_list: Array,
	total_list: Array,
	slot: int
) -> String:
	var skill_name := local_skill_name_for_slot(local_skill_loadout, slot)
	var remaining := int(remaining_list[slot - 1]) if remaining_list.size() >= slot else 0
	var total := int(total_list[slot - 1]) if total_list.size() >= slot else 0
	if slot > unlocked_slots:
		return "%d %s locked" % [slot, skill_name]
	return "%d %s %s" % [
		slot,
		skill_name,
		_cooldown_token(remaining, total),
	]


static func diagnostics_text(
	diagnostics: Dictionary,
	transport_snapshot: Dictionary,
	transport_state: String,
	screen: String,
	arena_mode: String,
	match_phase: String,
	local_player_id: int,
	current_match_id: int,
	current_round: int,
	arena_width: int,
	arena_height: int,
	arena_tile_units: int,
	training_mode: bool,
	training_metrics: Dictionary
) -> String:
	var lines: Array[String] = []
	lines.append_array(_diagnostic_header_lines(
		transport_state,
		screen,
		arena_mode,
		match_phase,
		local_player_id,
		current_match_id,
		current_round
	))
	lines.append_array(_diagnostic_timing_lines(diagnostics))
	lines.append_array(_diagnostic_packet_lines(diagnostics))
	lines.append_array(_diagnostic_scene_lines(diagnostics))
	lines.append_array(_diagnostic_tile_lines(diagnostics, arena_width, arena_height, arena_tile_units))
	lines.append_array(_diagnostic_render_lines(diagnostics))
	lines.append_array(_diagnostic_transport_lines(transport_snapshot, diagnostics))
	lines.append_array(_diagnostic_training_lines(training_mode, training_metrics))
	return "\n".join(lines)


static func training_metrics_text(training_metrics: Dictionary) -> String:
	if training_metrics.is_empty():
		return ""
	var elapsed_ms := int(training_metrics.get("elapsed_ms", 0))
	var elapsed_seconds := maxf(float(elapsed_ms) / 1000.0, 0.001)
	var damage_done := int(training_metrics.get("damage_done", 0))
	var healing_done := int(training_metrics.get("healing_done", 0))
	return "Training metrics: dmg %d  |  heal %d  |  DPS %.1f  |  HPS %.1f  |  time %.1fs" % [
		damage_done,
		healing_done,
		float(damage_done) / elapsed_seconds,
		float(healing_done) / elapsed_seconds,
		elapsed_seconds,
	]


static func _skill_catalog_entry(
	skill_catalog: Array[Dictionary],
	tree_name: String,
	tier: int
) -> Dictionary:
	for entry in skill_catalog:
		if String(entry.get("tree", "")) == tree_name and int(entry.get("tier", 0)) == tier:
			return (entry as Dictionary).duplicate(true)
	return {}


static func _summary_block_lines(entries: Array) -> Array[String]:
	var normalized: Array[Dictionary] = []
	for entry in entries:
		normalized.append((entry as Dictionary).duplicate(true))
	normalized.sort_custom(func(a: Dictionary, b: Dictionary) -> bool:
		var team_a := String(a.get("team", ""))
		var team_b := String(b.get("team", ""))
		if team_a != team_b:
			return team_a < team_b
		var damage_a := int(a.get("damage_done", 0))
		var damage_b := int(b.get("damage_done", 0))
		if damage_a != damage_b:
			return damage_a > damage_b
		return String(a.get("player_name", "")) < String(b.get("player_name", ""))
	)
	var lines: Array[String] = []
	for entry in normalized:
		lines.append("%s  |  %s  |  dmg %d  |  heal+ %d  |  heal- %d  |  cc %d/%d" % [
			String(entry.get("player_name", "Unknown")),
			String(entry.get("team", "Unknown")),
			int(entry.get("damage_done", 0)),
			int(entry.get("healing_to_allies", 0)),
			int(entry.get("healing_to_enemies", 0)),
			int(entry.get("cc_used", 0)),
			int(entry.get("cc_hits", 0)),
		])
	if lines.is_empty():
		lines.append("No combat events recorded yet.")
	return lines


static func _diagnostic_header_lines(
	transport_state: String,
	screen: String,
	arena_mode: String,
	match_phase: String,
	local_player_id: int,
	current_match_id: int,
	current_round: int
) -> Array[String]:
	return [
		"Client Diagnostics",
		"  transport_state: %s" % transport_state,
		"  screen: %s" % screen,
		"  arena_mode: %s" % arena_mode,
		"  match_phase: %s" % match_phase,
		"  local_player_id: %d" % local_player_id,
		"  current_match_id: %d" % current_match_id,
		"  current_round: %d" % current_round,
	]


static func _diagnostic_timing_lines(diagnostics: Dictionary) -> Array[String]:
	var lines: Array[String] = ["", "Timing"]
	var timings: Dictionary = diagnostics.get("timings", {})
	for metric_name in [
		"ui_refresh",
		"ui_refresh_catalog",
		"ui_refresh_labels",
		"ui_refresh_logs",
		"ui_refresh_diagnostics",
		"ui_refresh_buttons",
		"ui_refresh_visibility",
		"advance_visuals",
		"packet_decode",
		"snapshot_apply",
		"arena_draw",
		"arena_draw_cache_sync",
		"arena_draw_base",
		"arena_draw_floor",
		"arena_draw_grid",
		"arena_draw_obstacles",
		"arena_draw_visibility",
		"arena_draw_effects",
		"arena_draw_deployables",
		"arena_draw_projectiles",
		"arena_draw_players",
		"arena_draw_combat_text",
		"arena_draw_border",
		"arena_cache_background",
		"arena_cache_visibility",
	]:
		lines.append("  %s_ms: %s" % [metric_name, _timing_bucket_text(timings.get(metric_name, {}))])
	return lines


static func _diagnostic_packet_lines(diagnostics: Dictionary) -> Array[String]:
	var packet_stats: Dictionary = diagnostics.get("packets", {})
	return [
		"",
		"Packet Flow",
		"  control_events: %d" % int(packet_stats.get("control_events", 0)),
		"  full_snapshots: %d" % int(packet_stats.get("full_snapshots", 0)),
		"  delta_snapshots: %d" % int(packet_stats.get("delta_snapshots", 0)),
		"  effect_batches: %d" % int(packet_stats.get("effect_batches", 0)),
		"  combat_text_batches: %d" % int(packet_stats.get("combat_text_batches", 0)),
		"  bytes_in_total: %d" % int(packet_stats.get("bytes_in_total", 0)),
		"  last_packet_bytes: %d" % int(packet_stats.get("last_packet_bytes", 0)),
		"  last_event_type: %s" % String(packet_stats.get("last_event_type", "")),
		"  last_sim_tick: %d" % int(packet_stats.get("last_sim_tick", 0)),
	]


static func _diagnostic_scene_lines(diagnostics: Dictionary) -> Array[String]:
	var object_stats: Dictionary = diagnostics.get("objects", {})
	return [
		"",
		"Scene Counts",
		"  players: %d" % int(object_stats.get("players", 0)),
		"  projectiles: %d" % int(object_stats.get("projectiles", 0)),
		"  deployables: %d" % int(object_stats.get("deployables", 0)),
		"  effects: %d" % int(object_stats.get("effects", 0)),
		"  local_combat_texts: %d" % int(object_stats.get("combat_texts", 0)),
	]


static func _diagnostic_tile_lines(
	diagnostics: Dictionary,
	arena_width: int,
	arena_height: int,
	arena_tile_units: int
) -> Array[String]:
	var tile_stats: Dictionary = diagnostics.get("tiles", {})
	return [
		"",
		"Arena Footprint",
		"  arena_width_units: %d" % arena_width,
		"  arena_height_units: %d" % arena_height,
		"  tile_units: %d" % arena_tile_units,
		"  footprint_tiles: %d" % int(tile_stats.get("footprint", 0)),
		"  visible_tiles: %d" % int(tile_stats.get("visible", 0)),
		"  explored_tiles: %d" % int(tile_stats.get("explored", 0)),
	]


static func _diagnostic_render_lines(diagnostics: Dictionary) -> Array[String]:
	var render_stats: Dictionary = diagnostics.get("render", {})
	return [
		"",
		"Render Surface",
		"  arena_pixels_w: %d" % int(render_stats.get("arena_pixels_w", 0)),
		"  arena_pixels_h: %d" % int(render_stats.get("arena_pixels_h", 0)),
	]


static func _diagnostic_transport_lines(
	transport_snapshot: Dictionary,
	diagnostics: Dictionary
) -> Array[String]:
	return [
		"",
		"Transport",
		"  signal_socket_state: %s" % String(transport_snapshot.get("signal_socket_state", "")),
		"  control_channel_state: %s" % String(transport_snapshot.get("control_channel_state", "")),
		"  input_channel_state: %s" % String(transport_snapshot.get("input_channel_state", "")),
		"  snapshot_channel_state: %s" % String(transport_snapshot.get("snapshot_channel_state", "")),
		"  signal_messages_in: %d" % int(transport_snapshot.get("signal_messages_in", 0)),
		"  signal_messages_out: %d" % int(transport_snapshot.get("signal_messages_out", 0)),
		"  signal_bytes_in: %d" % int(transport_snapshot.get("signal_bytes_in", 0)),
		"  signal_bytes_out: %d" % int(transport_snapshot.get("signal_bytes_out", 0)),
		"  control_packets_in: %d" % int(transport_snapshot.get("control_packets_in", 0)),
		"  control_bytes_in: %d" % int(transport_snapshot.get("control_bytes_in", 0)),
		"  snapshot_packets_in: %d" % int(transport_snapshot.get("snapshot_packets_in", 0)),
		"  snapshot_bytes_in: %d" % int(transport_snapshot.get("snapshot_bytes_in", 0)),
		"  control_packets_out: %d" % int(transport_snapshot.get("control_packets_out", 0)),
		"  control_bytes_out: %d" % int(transport_snapshot.get("control_bytes_out", 0)),
		"  input_packets_out: %d" % int(transport_snapshot.get("input_packets_out", 0)),
		"  input_bytes_out: %d" % int(transport_snapshot.get("input_bytes_out", 0)),
		"  transport_decode_ms: %s" % _timing_bucket_text(transport_snapshot.get("decode_timing", {})),
		"  last_signal_close_code: %d" % int(transport_snapshot.get("last_signal_close_code", -1)),
		"  last_signal_close_reason: %s" % String(transport_snapshot.get("last_signal_close_reason", "")),
	]


static func _diagnostic_training_lines(
	training_mode: bool,
	training_metrics: Dictionary
) -> Array[String]:
	if not training_mode or training_metrics.is_empty():
		return []
	return [
		"",
		"Training Metrics",
		"  damage_done: %d" % int(training_metrics.get("damage_done", 0)),
		"  healing_done: %d" % int(training_metrics.get("healing_done", 0)),
		"  elapsed_ms: %d" % int(training_metrics.get("elapsed_ms", 0)),
	]


static func _timing_bucket_text(bucket: Dictionary) -> String:
	if bucket.is_empty():
		return "last=0.000 avg=0.000 p50=0.000 p95=0.000 max=0.000 count=0"
	return "last=%0.3f avg=%0.3f p50=%0.3f p95=%0.3f max=%0.3f count=%d" % [
		float(bucket.get("last_ms", 0.0)),
		float(bucket.get("avg_ms", 0.0)),
		float(bucket.get("p50_ms", 0.0)),
		float(bucket.get("p95_ms", 0.0)),
		float(bucket.get("max_ms", 0.0)),
		int(bucket.get("count", 0)),
	]


static func _cooldown_token(remaining_ms: int, total_ms: int) -> String:
	if remaining_ms <= 0:
		return "ready"
	if total_ms <= 0:
		return "%dms" % remaining_ms
	return "%d/%dms" % [remaining_ms, total_ms]
