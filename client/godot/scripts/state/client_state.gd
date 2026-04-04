extends RefCounted
class_name ClientState

const MAX_EVENT_LINES := 28
const MAX_LOCAL_COMBAT_TEXTS := 64
const COMBAT_TEXT_LIFETIME_SECONDS := 1.18
const PLAYER_RENDER_LERP_RATE := 11.0
const PROJECTILE_RENDER_LERP_RATE := 15.0
const RENDER_SNAP_DISTANCE_UNITS := 220.0
const SpellAudioRegistryScript := preload("res://scripts/content/spell_audio_registry.gd")
const ClientStateViewScript := preload("res://scripts/state/client_state_view.gd")
const GodotPerfMonitorsScript := preload("res://scripts/debug/godot_perf_monitors.gd")
const WebSocketConfigScript := preload("res://scripts/net/websocket_config.gd")

var websocket_url := WebSocketConfigScript.new().runtime_default_url()
var local_player_id := 0
var local_player_name := ""
var transport_state := "closed"
var screen := "central"
var banner_message := "Preparing your realtime session."
var phase_label := "Central Lobby"
var countdown_label := ""
var outcome_label := ""
var current_lobby_id := 0
var current_match_id := 0
var current_round := 0
var score_a := 0
var score_b := 0
var lobby_locked := false
var match_phase := "idle"
var record := {
	"wins": 0,
	"losses": 0,
	"no_contests": 0,
	"round_wins": 0,
	"round_losses": 0,
	"total_damage_done": 0,
	"total_healing_done": 0,
	"total_combat_ms": 0,
	"cc_used": 0,
	"cc_hits": 0,
	"skill_pick_counts": {},
}
var lobby_directory: Array[Dictionary] = []
var roster := {}
var recent_events: Array[String] = []
var skill_catalog: Array[Dictionary] = []
var local_skill_progress := {}
var local_skill_loadout: Array[Dictionary] = []
var local_round_skill_locked := false
var round_summary := {}
var match_summary := {}
var arena_mode := "Match"
var arena_width := 0
var arena_height := 0
var arena_tile_units := 0
var arena_obstacles: Array[Dictionary] = []
var arena_deployables: Array[Dictionary] = []
var arena_players := {}
var render_players := {}
var arena_projectiles: Array[Dictionary] = []
var render_projectiles: Array[Dictionary] = []
var arena_effects: Array[Dictionary] = []
var local_combat_texts: Array[Dictionary] = []
var footprint_tiles := PackedByteArray()
var objective_tiles := PackedByteArray()
var visible_tiles := PackedByteArray()
var explored_tiles := PackedByteArray()
var objective_target_ms := 0
var objective_team_a_ms := 0
var objective_team_b_ms := 0
var training_metrics := {}
var diagnostics := {}
var arena_render_revision := 1
var spell_audio_registry := {}


func _init() -> void:
	spell_audio_registry = SpellAudioRegistryScript.load_default_manifest()
	_reset_diagnostics()

func _is_rendered_deployable_kind(kind_name: String) -> bool:
	return kind_name != "Aura"


func prepare_for_connection(player_name: String) -> void:
	_reset_diagnostics()
	websocket_url = WebSocketConfigScript.new().runtime_default_url()
	local_player_id = 0
	local_player_name = player_name.strip_edges()
	transport_state = "connecting"
	screen = "central"
	banner_message = "Connecting as %s. Waiting for a server-assigned player ID." % local_player_name
	phase_label = "Central Lobby"
	countdown_label = ""
	outcome_label = ""
	current_lobby_id = 0
	current_match_id = 0
	current_round = 0
	score_a = 0
	score_b = 0
	lobby_locked = false
	match_phase = "idle"
	lobby_directory.clear()
	roster.clear()
	recent_events.clear()
	skill_catalog.clear()
	_reset_local_skill_progress()
	local_round_skill_locked = false
	round_summary.clear()
	match_summary.clear()
	_clear_arena_state()
	_append_event("Connecting as %s." % local_player_name)


func mark_transport_state(state_name: String) -> void:
	transport_state = state_name
	match state_name:
		"connecting":
			banner_message = "Connecting to the hosted backend."
		"open":
			banner_message = "Realtime session is open. Waiting for the server to accept the connect command."
		"closing":
			banner_message = "Realtime transport closing."
		"closed":
			banner_message = "Realtime transport closed."
	_append_event("Transport state: %s." % state_name)


func mark_transport_closed(reason: String) -> void:
	transport_state = "closed"
	var had_active_session := (
		screen != "central"
		or current_lobby_id != 0
		or current_match_id != 0
		or not roster.is_empty()
		or not arena_players.is_empty()
	)
	if had_active_session:
		_reset_to_central()
	local_player_id = 0
	banner_message = reason if reason != "" else "Realtime transport closed."
	_append_event(banner_message)


func mark_transport_error(message: String) -> void:
	banner_message = message
	_append_event("Error: %s" % message)


func announce_local(message: String) -> void:
	banner_message = message
	_append_event(message)


func apply_server_event(event: Dictionary) -> void:
	var event_type := String(event.get("type", "Unknown"))
	if _apply_session_event(event_type, event):
		return
	if _apply_lobby_event(event_type, event):
		return
	if _apply_match_flow_event(event_type, event):
		return
	if _apply_arena_event(event_type, event):
		return
	_append_event("Unhandled event: %s" % event_type)


func _apply_session_event(event_type: String, event: Dictionary) -> bool:
	match event_type:
		"Connected":
			local_player_id = int(event.get("player_id", local_player_id))
			local_player_name = String(event.get("player_name", local_player_name))
			record = event.get("record", record)
			screen = "central"
			phase_label = "Central Lobby"
			countdown_label = ""
			outcome_label = ""
			lobby_directory.clear()
			roster.clear()
			skill_catalog.clear()
			for catalog_entry in event.get("skill_catalog", []):
				skill_catalog.append((catalog_entry as Dictionary).duplicate(true))
			_reset_local_skill_progress()
			local_round_skill_locked = false
			round_summary.clear()
			match_summary.clear()
			_clear_arena_state()
			banner_message = "Connected as %s." % local_player_name
			_append_event("Connected as %s (#%d)." % [local_player_name, local_player_id])
			return true
		"ReturnedToCentralLobby":
			record = event.get("record", record)
			_reset_to_central()
			banner_message = "Returned to the central lobby."
			_append_event("Returned to the central lobby.")
			return true
		"Error":
			banner_message = String(event.get("message", "Unknown server error"))
			_append_event("Server error: %s" % banner_message)
			return true
		_:
			return false


func _apply_lobby_event(event_type: String, event: Dictionary) -> bool:
	match event_type:
		"LobbyDirectorySnapshot":
			lobby_directory = event.get("lobbies", []).duplicate(true)
			var lobby_count := lobby_directory.size()
			banner_message = "Lobby directory updated: %d open entr%s." % [
				lobby_count,
				"ies" if lobby_count != 1 else "y",
			]
			_append_event(banner_message)
			return true
		"GameLobbyCreated":
			current_lobby_id = int(event.get("lobby_id", 0))
			screen = "lobby"
			phase_label = "Game Lobby #%d" % current_lobby_id
			lobby_locked = false
			banner_message = "Created lobby #%d." % current_lobby_id
			_append_event("Created lobby #%d." % current_lobby_id)
			return true
		"GameLobbyJoined":
			current_lobby_id = int(event.get("lobby_id", current_lobby_id))
			screen = "lobby"
			phase_label = "Game Lobby #%d" % current_lobby_id
			var joined_player_id := int(event.get("player_id", 0))
			var joined_name := local_player_name if joined_player_id == local_player_id else ""
			var joined_member := _ensure_roster_entry(joined_player_id, joined_name)
			joined_member["ready"] = "Not Ready"
			joined_member["skill"] = ""
			banner_message = "%s joined lobby #%d." % [_display_name(joined_player_id), current_lobby_id]
			_append_event("%s joined lobby #%d." % [_display_name(joined_player_id), current_lobby_id])
			return true
		"GameLobbySnapshot":
			_apply_lobby_snapshot(event)
			return true
		"GameLobbyLeft":
			var left_player_id := int(event.get("player_id", 0))
			if left_player_id == local_player_id:
				_reset_to_central()
				banner_message = "Returned to the central lobby."
			else:
				roster.erase(left_player_id)
				banner_message = "%s left lobby #%d." % [_display_name(left_player_id), current_lobby_id]
			_append_event("%s left lobby #%d." % [_display_name(left_player_id), current_lobby_id])
			return true
		"TeamSelected":
			var team_player_id := int(event.get("player_id", 0))
			var team_member := _ensure_roster_entry(team_player_id)
			team_member["team"] = String(event.get("team", "Unassigned"))
			if bool(event.get("ready_reset", false)):
				team_member["ready"] = "Not Ready"
			banner_message = "%s moved to %s." % [_display_name(team_player_id), team_member["team"]]
			_append_event("%s moved to %s." % [_display_name(team_player_id), team_member["team"]])
			return true
		"ReadyChanged":
			var ready_player_id := int(event.get("player_id", 0))
			var ready_member := _ensure_roster_entry(ready_player_id)
			ready_member["ready"] = String(event.get("ready", "Not Ready"))
			banner_message = "%s is now %s." % [_display_name(ready_player_id), ready_member["ready"]]
			_append_event("%s is now %s." % [_display_name(ready_player_id), ready_member["ready"]])
			return true
		"LaunchCountdownStarted":
			lobby_locked = true
			countdown_label = "Launch locks in %ds for %d players." % [
				int(event.get("seconds_remaining", 0)),
				int(event.get("roster_size", 0)),
			]
			banner_message = countdown_label
			_append_event(countdown_label)
			return true
		"LaunchCountdownTick":
			lobby_locked = true
			countdown_label = "Launch in %ds." % int(event.get("seconds_remaining", 0))
			banner_message = countdown_label
			_append_event(countdown_label)
			return true
		_:
			return false


func _apply_lobby_snapshot(event: Dictionary) -> void:
	current_lobby_id = int(event.get("lobby_id", current_lobby_id))
	screen = "lobby"
	roster.clear()
	var players: Array = event.get("players", [])
	for player_data in players:
		var player_id := int(player_data.get("player_id", 0))
		var member := _ensure_roster_entry(player_id, String(player_data.get("player_name", "")))
		member["name"] = String(player_data.get("player_name", member["name"]))
		member["record"] = player_data.get("record", {})
		member["team"] = String(player_data.get("team", "Unassigned"))
		member["ready"] = String(player_data.get("ready", "Not Ready"))
		member["skill"] = "Awaiting next pick"
	var phase: Dictionary = event.get("phase", {})
	var phase_name := String(phase.get("name", "Open"))
	var seconds_remaining := int(phase.get("seconds_remaining", 0))
	lobby_locked = phase_name != "Open"
	phase_label = "Game Lobby #%d" % current_lobby_id
	if lobby_locked:
		countdown_label = "Launch in %ds." % seconds_remaining
		banner_message = "Lobby #%d is locked for launch." % current_lobby_id
	else:
		countdown_label = ""
		banner_message = "Lobby #%d snapshot refreshed." % current_lobby_id
	_append_event("Lobby #%d snapshot received with %d player(s)." % [current_lobby_id, players.size()])


func _apply_match_flow_event(event_type: String, event: Dictionary) -> bool:
	match event_type:
		"MatchStarted":
			_start_match_session(event)
			return true
		"TrainingStarted":
			_start_training_session(event)
			return true
		"SkillChosen":
			_apply_skill_chosen_event(event)
			return true
		"PreCombatStarted", "CombatStarted":
			_apply_match_phase_event(event_type, event)
			return true
		"RoundWon", "RoundSummary", "MatchEnded", "MatchSummary":
			_apply_match_result_event(event_type, event)
			return true
		_:
			return false


func _apply_skill_chosen_event(event: Dictionary) -> void:
	var skill_player_id := int(event.get("player_id", 0))
	var skill_member := _ensure_roster_entry(skill_player_id)
	var slot := int(event.get("slot", 0))
	var tree_name := String(event.get("tree", "Unknown"))
	var tier := int(event.get("tier", 0))
	skill_member["skill"] = ClientStateViewScript.skill_name_for(skill_catalog, tree_name, tier)
	if skill_player_id == local_player_id and tree_name != "":
		local_skill_progress[tree_name] = max(tier, int(local_skill_progress.get(tree_name, 0)))
		_remember_local_skill_choice(slot, tree_name, tier, skill_member["skill"])
		if not is_training_mode():
			local_round_skill_locked = true
	banner_message = "%s locked %s." % [_display_name(skill_player_id), skill_member["skill"]]
	_append_event("%s locked %s." % [_display_name(skill_player_id), skill_member["skill"]])


func _apply_match_phase_event(event_type: String, event: Dictionary) -> void:
	match event_type:
		"PreCombatStarted":
			match_phase = "pre_combat"
			countdown_label = "Arena unlocks in %ds." % int(event.get("seconds_remaining", 0))
		"CombatStarted":
			match_phase = "combat"
			countdown_label = "Combat is live. The shell can now send primary attacks."
		_:
			return
	banner_message = countdown_label
	_append_event(countdown_label)


func _apply_match_result_event(event_type: String, event: Dictionary) -> void:
	match event_type:
		"RoundWon":
			match_phase = "skill_pick"
			current_round = int(event.get("round", current_round))
			score_a = int(event.get("score_a", score_a))
			score_b = int(event.get("score_b", score_b))
			countdown_label = "Round %d ended. Choose the next skill when ready." % current_round
			local_round_skill_locked = false
			arena_effects.clear()
			_mark_arena_render_dirty()
			banner_message = "%s won round %d." % [
				String(event.get("winning_team", "Unknown Team")),
				current_round,
			]
			_clear_round_skills()
			_append_event("%s won round %d. Score %d-%d." % [
				String(event.get("winning_team", "Unknown Team")),
				current_round,
				score_a,
				score_b,
			])
		"RoundSummary":
			round_summary = (event.get("summary", {}) as Dictionary).duplicate(true)
			_append_event("Round %d summary received." % int(round_summary.get("round", current_round)))
		"MatchEnded":
			screen = "results"
			match_phase = "ended"
			score_a = int(event.get("score_a", score_a))
			score_b = int(event.get("score_b", score_b))
			arena_effects.clear()
			_mark_arena_render_dirty()
			outcome_label = "%s, %d-%d" % [
				String(event.get("outcome", "Unknown")),
				score_a,
				score_b,
			]
			banner_message = String(event.get("message", "Match ended."))
			_append_event(banner_message)
		"MatchSummary":
			match_summary = (event.get("summary", {}) as Dictionary).duplicate(true)
			_append_event("Match summary received.")


func _start_match_session(event: Dictionary) -> void:
	arena_mode = "Match"
	screen = "match"
	current_match_id = int(event.get("match_id", 0))
	current_round = int(event.get("round", 0))
	match_phase = "skill_pick"
	_reset_local_skill_progress()
	local_round_skill_locked = false
	round_summary.clear()
	match_summary.clear()
	_clear_arena_state()
	_mark_arena_render_dirty()
	phase_label = "Match #%d, Round %d" % [current_match_id, current_round]
	countdown_label = "Choose one skill in %ds." % int(event.get("skill_pick_seconds", 0))
	lobby_locked = false
	banner_message = "Match #%d started." % current_match_id
	_append_event("Match #%d started. Round %d skill pick is open." % [
		current_match_id,
		current_round,
	])


func _start_training_session(event: Dictionary) -> void:
	arena_mode = "Training"
	screen = "match"
	current_match_id = int(event.get("training_id", 0))
	current_round = 0
	match_phase = "combat"
	_reset_local_skill_progress()
	local_round_skill_locked = false
	round_summary.clear()
	match_summary.clear()
	_clear_arena_state()
	_mark_arena_render_dirty()
	phase_label = "Training Grounds"
	countdown_label = "Choose skills freely, then test on the dummies."
	lobby_locked = false
	banner_message = "Training session #%d started." % current_match_id
	_append_event("Training session #%d started." % current_match_id)


func _apply_arena_event(event_type: String, event: Dictionary) -> bool:
	match event_type:
		"ArenaStateSnapshot":
			_apply_full_arena_snapshot(event.get("snapshot", {}))
			return true
		"ArenaDeltaSnapshot":
			_apply_delta_arena_snapshot(event.get("snapshot", {}))
			return true
		"ArenaEffectBatch":
			for effect_data in event.get("effects", []):
				var effect: Dictionary = (effect_data as Dictionary).duplicate(true)
				var ttl: float = _effect_ttl_seconds(String(effect.get("kind", "")))
				effect["ttl"] = ttl
				effect["ttl_max"] = ttl
				arena_effects.append(effect)
			_mark_arena_render_dirty()
			return true
		"ArenaCombatTextBatch":
			for entry_data in event.get("entries", []):
				_queue_local_combat_text((entry_data as Dictionary).duplicate(true))
			_mark_arena_render_dirty()
			return true
		_:
			return false


func _apply_full_arena_snapshot(snapshot: Dictionary) -> void:
	arena_mode = String(snapshot.get("mode", arena_mode))
	_apply_arena_phase(snapshot)
	arena_width = int(snapshot.get("width", 0))
	arena_height = int(snapshot.get("height", 0))
	arena_tile_units = int(snapshot.get("tile_units", 0))
	footprint_tiles = snapshot.get("footprint_tiles", PackedByteArray())
	objective_tiles = snapshot.get("objective_tiles", PackedByteArray())
	visible_tiles = snapshot.get("visible_tiles", PackedByteArray())
	explored_tiles = snapshot.get("explored_tiles", PackedByteArray())
	objective_target_ms = int(snapshot.get("objective_target_ms", objective_target_ms))
	objective_team_a_ms = int(snapshot.get("objective_team_a_ms", objective_team_a_ms))
	objective_team_b_ms = int(snapshot.get("objective_team_b_ms", objective_team_b_ms))
	_apply_arena_common(snapshot)


func _apply_delta_arena_snapshot(snapshot: Dictionary) -> void:
	arena_mode = String(snapshot.get("mode", arena_mode))
	_apply_arena_phase(snapshot)
	arena_tile_units = int(snapshot.get("tile_units", arena_tile_units))
	footprint_tiles = snapshot.get("footprint_tiles", footprint_tiles)
	objective_tiles = snapshot.get("objective_tiles", objective_tiles)
	visible_tiles = snapshot.get("visible_tiles", PackedByteArray())
	explored_tiles = snapshot.get("explored_tiles", PackedByteArray())
	objective_target_ms = int(snapshot.get("objective_target_ms", objective_target_ms))
	objective_team_a_ms = int(snapshot.get("objective_team_a_ms", objective_team_a_ms))
	objective_team_b_ms = int(snapshot.get("objective_team_b_ms", objective_team_b_ms))
	_apply_arena_common(snapshot)


func _apply_arena_common(snapshot: Dictionary) -> void:
	training_metrics = (snapshot.get("training_metrics", training_metrics) as Dictionary).duplicate(true)
	arena_obstacles.clear()
	for obstacle_data in snapshot.get("obstacles", []):
		arena_obstacles.append((obstacle_data as Dictionary).duplicate(true))
	arena_deployables.clear()
	for deployable_data in snapshot.get("deployables", []):
		var deployable := (deployable_data as Dictionary).duplicate(true)
		if _is_rendered_deployable_kind(String(deployable.get("kind", ""))):
			arena_deployables.append(deployable)
	_replace_arena_players(snapshot.get("players", []))
	_replace_arena_projectiles(snapshot.get("projectiles", []))
	_mark_arena_render_dirty()


func ready_button_text() -> String:
	var member := self_entry()
	if member.is_empty():
		return "Toggle Ready"
	return "Set Not Ready" if String(member.get("ready", "Not Ready")) == "Ready" else "Set Ready"


func current_team() -> String:
	var member := self_entry()
	if member.is_empty():
		return "Unassigned"
	return String(member.get("team", "Unassigned"))


func self_entry() -> Dictionary:
	if roster.has(local_player_id):
		return roster[local_player_id]
	return {}


func roster_lines() -> Array[String]:
	return ClientStateViewScript.roster_lines(roster)


func lobby_roster_lines() -> Array[String]:
	return ClientStateViewScript.lobby_roster_lines(roster)


func lobby_directory_lines() -> Array[String]:
	return ClientStateViewScript.lobby_directory_lines(lobby_directory)


func lobby_directory_bbcode() -> String:
	return ClientStateViewScript.lobby_directory_bbcode(lobby_directory)


func record_text() -> String:
	return ClientStateViewScript.record_text(record, skill_catalog)


func score_text() -> String:
	return ClientStateViewScript.score_text(score_a, score_b)


func event_log_text() -> String:
	return ClientStateViewScript.event_log_text(recent_events)


func round_summary_text() -> String:
	return ClientStateViewScript.round_summary_text(round_summary, current_round)


func match_summary_text() -> String:
	return ClientStateViewScript.match_summary_text(match_summary)


func lobby_note() -> String:
	return ClientStateViewScript.lobby_note()


func can_join_or_create_lobby() -> bool:
	return ClientStateViewScript.can_join_or_create_lobby(transport_state, screen)


func can_start_training() -> bool:
	return ClientStateViewScript.can_start_training(transport_state, screen)


func can_manage_lobby() -> bool:
	return ClientStateViewScript.can_manage_lobby(transport_state, screen, lobby_locked)


func can_leave_lobby() -> bool:
	return ClientStateViewScript.can_leave_lobby(transport_state, screen, lobby_locked)


func can_choose_skill() -> bool:
	return ClientStateViewScript.can_choose_skill(
		transport_state,
		screen,
		is_training_mode(),
		match_phase,
		local_round_skill_locked
	)


func next_skill_tier_for(tree_name: String) -> int:
	return ClientStateViewScript.next_skill_tier_for(tree_name, skill_catalog, local_skill_progress)


func can_choose_skill_option(tree_name: String, tier: int) -> bool:
	return ClientStateViewScript.can_choose_skill_option(
		tree_name,
		tier,
		transport_state,
		screen,
		is_training_mode(),
		match_phase,
		local_round_skill_locked,
		skill_catalog,
		local_skill_progress
	)


func skill_tree_names() -> Array[String]:
	return ClientStateViewScript.skill_tree_names(skill_catalog, local_skill_progress)


func skill_entries_for(tree_name: String) -> Array[Dictionary]:
	return ClientStateViewScript.skill_entries_for(skill_catalog, tree_name)


func skill_name_for(tree_name: String, tier: int) -> String:
	return ClientStateViewScript.skill_name_for(skill_catalog, tree_name, tier)


func skill_description_for(tree_name: String, tier: int) -> String:
	return ClientStateViewScript.skill_description_for(skill_catalog, tree_name, tier)


func skill_summary_for(tree_name: String, tier: int) -> String:
	return ClientStateViewScript.skill_summary_for(skill_catalog, tree_name, tier)


func skill_ui_category_for(tree_name: String, tier: int) -> String:
	return ClientStateViewScript.skill_ui_category_for(skill_catalog, tree_name, tier)


func skill_tooltip_for(tree_name: String, tier: int) -> String:
	return ClientStateViewScript.skill_tooltip_for(skill_catalog, tree_name, tier)


func skill_audio_cue_for(tree_name: String, tier: int) -> String:
	return ClientStateViewScript.skill_audio_cue_for(skill_catalog, tree_name, tier)


func skill_audio_entry_for(tree_name: String, tier: int) -> Dictionary:
	var cue_id := skill_audio_cue_for(tree_name, tier)
	if cue_id == "":
		return {}
	return SpellAudioRegistryScript.lookup(spell_audio_registry, cue_id)


func skill_catalog_signature() -> String:
	return ClientStateViewScript.skill_catalog_signature(skill_catalog)


func can_quit_results() -> bool:
	return ClientStateViewScript.can_quit_results(transport_state, screen)


func can_reset_training() -> bool:
	return ClientStateViewScript.can_reset_training(transport_state, is_training_mode())


func can_quit_arena() -> bool:
	return ClientStateViewScript.can_quit_arena(transport_state, screen, is_training_mode())


func can_send_combat_input() -> bool:
	return ClientStateViewScript.can_send_combat_input(
		transport_state,
		screen,
		is_training_mode(),
		match_phase,
		local_arena_player()
	)


func can_use_combat_slot(slot: int) -> bool:
	return ClientStateViewScript.can_use_combat_slot(
		slot,
		transport_state,
		screen,
		is_training_mode(),
		match_phase,
		local_arena_player()
	)


func can_use_primary_attack() -> bool:
	return ClientStateViewScript.can_use_primary_attack(
		transport_state,
		screen,
		is_training_mode(),
		match_phase,
		local_arena_player()
	)


func local_arena_player() -> Dictionary:
	if arena_players.has(local_player_id):
		return arena_players[local_player_id]
	return {}


func arena_projectiles_list() -> Array[Dictionary]:
	var projectiles: Array[Dictionary] = []
	for projectile in render_projectiles:
		projectiles.append((projectile as Dictionary).duplicate(true))
	return projectiles


func arena_deployables_list() -> Array[Dictionary]:
	var deployables: Array[Dictionary] = []
	for deployable in arena_deployables:
		var copy := (deployable as Dictionary).duplicate(true)
		if _is_rendered_deployable_kind(String(copy.get("kind", ""))):
			deployables.append(copy)
	return deployables


func authoritative_arena_projectiles_list() -> Array[Dictionary]:
	var projectiles: Array[Dictionary] = []
	for projectile in arena_projectiles:
		projectiles.append((projectile as Dictionary).duplicate(true))
	return projectiles


func local_skill_name_for_slot(slot: int) -> String:
	return ClientStateViewScript.local_skill_name_for_slot(local_skill_loadout, slot)


func player_skill_name_for_slot(player_id: int, slot: int) -> String:
	return ClientStateViewScript.player_skill_name_for_slot(
		local_player_id,
		local_skill_loadout,
		roster,
		player_id,
		slot
	)


func cooldown_summary_text() -> String:
	return ClientStateViewScript.cooldown_summary_text(
		local_arena_player(),
		local_skill_loadout
	)


func arena_players_list() -> Array[Dictionary]:
	var ids := render_players.keys()
	ids.sort()
	var players: Array[Dictionary] = []
	for player_id in ids:
		players.append((render_players[player_id] as Dictionary).duplicate(true))
	return players


func local_combat_text_entries() -> Array[Dictionary]:
	var entries: Array[Dictionary] = []
	for entry in local_combat_texts:
		entries.append((entry as Dictionary).duplicate(true))
	return entries


func authoritative_arena_players_list() -> Array[Dictionary]:
	var ids := arena_players.keys()
	ids.sort()
	var players: Array[Dictionary] = []
	for player_id in ids:
		players.append((arena_players[player_id] as Dictionary).duplicate(true))
	return players


func authoritative_arena_player(player_id: int) -> Dictionary:
	if arena_players.has(player_id):
		return (arena_players[player_id] as Dictionary).duplicate(true)
	return {}


func rendered_arena_player(player_id: int) -> Dictionary:
	if render_players.has(player_id):
		return (render_players[player_id] as Dictionary).duplicate(true)
	return {}


func arena_tile_width() -> int:
	if arena_tile_units <= 0:
		return 0
	return int(arena_width / arena_tile_units)


func arena_tile_height() -> int:
	if arena_tile_units <= 0:
		return 0
	return int(arena_height / arena_tile_units)


func current_arena_render_revision() -> int:
	return arena_render_revision


func is_tile_in_footprint(column: int, row: int) -> bool:
	if footprint_tiles.is_empty():
		return true
	return _mask_has_tile(footprint_tiles, column, row)


func is_tile_visible(column: int, row: int) -> bool:
	return _mask_has_tile(visible_tiles, column, row)


func is_tile_explored(column: int, row: int) -> bool:
	return _mask_has_tile(explored_tiles, column, row)


func is_objective_tile(column: int, row: int) -> bool:
	return _mask_has_tile(objective_tiles, column, row)


func advance_visuals(delta: float) -> bool:
	var changed := false
	for index in range(arena_effects.size() - 1, -1, -1):
		var effect: Dictionary = arena_effects[index]
		var previous_ttl: float = float(effect.get("ttl", 0.0))
		var ttl: float = maxf(0.0, previous_ttl - delta)
		effect["ttl"] = ttl
		if not is_equal_approx(ttl, previous_ttl):
			changed = true
		if ttl <= 0.0:
			arena_effects.remove_at(index)
			changed = true
	for index in range(local_combat_texts.size() - 1, -1, -1):
		var entry: Dictionary = local_combat_texts[index]
		var previous_ttl := float(entry.get("ttl", 0.0))
		var ttl := maxf(0.0, previous_ttl - delta)
		entry["ttl"] = ttl
		if not is_equal_approx(ttl, previous_ttl):
			changed = true
		if ttl <= 0.0:
			local_combat_texts.remove_at(index)
			changed = true
	changed = _smooth_render_state(delta) or changed
	if changed:
		_mark_arena_render_dirty()
	_refresh_diagnostics_counts()
	return changed


func record_client_timing(metric_name: String, micros: int) -> void:
	var timings: Dictionary = diagnostics.get("timings", {})
	var bucket: Dictionary = timings.get(metric_name, _new_timing_bucket())
	_record_timing_bucket(bucket, micros)
	timings[metric_name] = bucket
	diagnostics["timings"] = timings


func record_inbound_packet(
	header: Dictionary,
	event_type: String,
	packet_size: int,
	decode_us: int,
	apply_us: int
) -> void:
	var packet_stats: Dictionary = diagnostics.get("packets", {})
	packet_stats["bytes_in_total"] = int(packet_stats.get("bytes_in_total", 0)) + packet_size
	packet_stats["last_packet_bytes"] = packet_size
	packet_stats["last_event_type"] = event_type
	packet_stats["last_sim_tick"] = int(header.get("sim_tick", 0))
	packet_stats["last_packet_kind"] = int(header.get("packet_kind", -1))
	match event_type:
		"ArenaStateSnapshot":
			packet_stats["full_snapshots"] = int(packet_stats.get("full_snapshots", 0)) + 1
		"ArenaDeltaSnapshot":
			packet_stats["delta_snapshots"] = int(packet_stats.get("delta_snapshots", 0)) + 1
		"ArenaEffectBatch":
			packet_stats["effect_batches"] = int(packet_stats.get("effect_batches", 0)) + 1
		"ArenaCombatTextBatch":
			packet_stats["combat_text_batches"] = int(packet_stats.get("combat_text_batches", 0)) + 1
		_:
			packet_stats["control_events"] = int(packet_stats.get("control_events", 0)) + 1
	diagnostics["packets"] = packet_stats
	record_client_timing("packet_decode", decode_us)
	record_client_timing("snapshot_apply", apply_us)
	_refresh_diagnostics_counts()


func record_arena_draw(draw_micros: int, arena_pixel_size: Vector2) -> void:
	record_client_timing("arena_draw", draw_micros)
	var render_stats: Dictionary = diagnostics.get("render", {})
	render_stats["arena_pixels_w"] = int(round(arena_pixel_size.x))
	render_stats["arena_pixels_h"] = int(round(arena_pixel_size.y))
	diagnostics["render"] = render_stats


func record_godot_monitor_snapshot(snapshot: Dictionary) -> void:
	diagnostics["godot_builtin_monitors"] = snapshot.duplicate(true)


func diagnostics_snapshot() -> Dictionary:
	var snapshot := diagnostics.duplicate(true)
	var timings: Dictionary = diagnostics.get("timings", {})
	var timing_snapshots := {}
	for metric_name in timings.keys():
		timing_snapshots[metric_name] = _timing_bucket_snapshot(timings.get(metric_name, {}))
	snapshot["timings"] = timing_snapshots
	return snapshot


func timing_bucket_snapshot(metric_name: String) -> Dictionary:
	var timings: Dictionary = diagnostics.get("timings", {})
	return _timing_bucket_snapshot(timings.get(metric_name, {}))


func timing_bucket_last_us(metric_name: String) -> int:
	var timings: Dictionary = diagnostics.get("timings", {})
	var bucket: Dictionary = timings.get(metric_name, {})
	return int(bucket.get("last_us", 0))


func diagnostics_text(transport_snapshot: Dictionary) -> String:
	return ClientStateViewScript.diagnostics_text(
		diagnostics,
		transport_snapshot,
		transport_state,
		screen,
		arena_mode,
		match_phase,
		local_player_id,
		current_match_id,
		current_round,
		arena_width,
		arena_height,
		arena_tile_units,
		objective_team_a_ms,
		objective_team_b_ms,
		objective_target_ms,
		is_training_mode(),
		training_metrics
	)


func _apply_arena_phase(snapshot: Dictionary) -> void:
	var phase_name := String(snapshot.get("phase", ""))
	if phase_name.is_empty():
		return
	if String(snapshot.get("mode", arena_mode)) == "Training":
		match_phase = "combat"
		phase_label = "Training Grounds"
		countdown_label = "Training live"
		return
	var seconds_remaining := int(snapshot.get("phase_seconds_remaining", 0))
	match phase_name:
		"SkillPick":
			match_phase = "skill_pick"
			countdown_label = "Skill Pick: %ds" % seconds_remaining
		"PreCombat":
			match_phase = "pre_combat"
			countdown_label = "Pre-Combat: %ds" % seconds_remaining
		"Combat":
			match_phase = "combat"
			countdown_label = "Combat live"
		"MatchEnd":
			match_phase = "ended"


func _reset_to_central() -> void:
	screen = "central"
	phase_label = "Central Lobby"
	countdown_label = ""
	outcome_label = ""
	current_lobby_id = 0
	current_match_id = 0
	current_round = 0
	score_a = 0
	score_b = 0
	lobby_locked = false
	match_phase = "idle"
	lobby_directory.clear()
	roster.clear()
	_reset_local_skill_progress()
	local_round_skill_locked = false
	round_summary.clear()
	match_summary.clear()
	_clear_arena_state()


func _ensure_roster_entry(player_id: int, suggested_name: String = "") -> Dictionary:
	if not roster.has(player_id):
		roster[player_id] = {
			"id": player_id,
			"name": suggested_name if suggested_name != "" else "Player %d" % player_id,
			"team": "Unassigned",
			"ready": "Not Ready",
			"skill": "No skill locked",
		}
	return roster[player_id]


func _display_name(player_id: int) -> String:
	if player_id == local_player_id and local_player_name != "":
		return local_player_name
	if roster.has(player_id):
		var member: Dictionary = roster[player_id]
		return String(member.get("name", "Player %d" % player_id))
	return "Player %d" % player_id


func _append_event(line: String) -> void:
	recent_events.append(line)
	while recent_events.size() > MAX_EVENT_LINES:
		recent_events.remove_at(0)


func _queue_local_combat_text(entry: Dictionary) -> void:
	var copy := entry.duplicate(true)
	copy["ttl"] = COMBAT_TEXT_LIFETIME_SECONDS
	copy["ttl_max"] = COMBAT_TEXT_LIFETIME_SECONDS
	var basis := "%s|%s|%d|%d|%d" % [
		String(copy.get("text", "")),
		String(copy.get("style", "")),
		int(copy.get("x", 0)),
		int(copy.get("y", 0)),
		local_combat_texts.size(),
	]
	copy["jitter_x"] = float((basis.hash() % 19) - 9)
	local_combat_texts.append(copy)
	while local_combat_texts.size() > MAX_LOCAL_COMBAT_TEXTS:
		local_combat_texts.remove_at(0)


func _clear_round_skills() -> void:
	for player_id in roster.keys():
		var member: Dictionary = roster[player_id]
		member["skill"] = "Awaiting next pick"


func _reset_local_skill_progress() -> void:
	local_skill_progress.clear()
	local_skill_loadout.clear()
	for tree_name in ClientStateViewScript.skill_tree_names(skill_catalog, local_skill_progress):
		local_skill_progress[tree_name] = 0


func _clear_arena_state() -> void:
	arena_mode = "Match"
	arena_width = 0
	arena_height = 0
	arena_tile_units = 0
	arena_obstacles.clear()
	arena_deployables.clear()
	arena_players.clear()
	render_players.clear()
	arena_projectiles.clear()
	render_projectiles.clear()
	arena_effects.clear()
	local_combat_texts.clear()
	footprint_tiles = PackedByteArray()
	objective_tiles = PackedByteArray()
	visible_tiles = PackedByteArray()
	explored_tiles = PackedByteArray()
	objective_target_ms = 0
	objective_team_a_ms = 0
	objective_team_b_ms = 0
	training_metrics.clear()
	_mark_arena_render_dirty()
	_refresh_diagnostics_counts()


func _remember_local_skill_choice(slot: int, tree_name: String, tier: int, skill_name: String) -> void:
	var normalized_slot := maxi(1, slot)
	for index in range(local_skill_loadout.size()):
		var choice: Dictionary = local_skill_loadout[index]
		if int(choice.get("slot", 0)) == normalized_slot:
			local_skill_loadout[index] = {
				"slot": normalized_slot,
				"tree": tree_name,
				"tier": tier,
				"skill_name": skill_name,
			}
			return
	local_skill_loadout.append({
		"slot": normalized_slot,
		"tree": tree_name,
		"tier": tier,
		"skill_name": skill_name,
	})
	local_skill_loadout.sort_custom(func(left: Dictionary, right: Dictionary) -> bool:
		return int(left.get("slot", 0)) < int(right.get("slot", 0))
	)


func _replace_arena_players(player_entries: Array) -> void:
	var previous_render := render_players
	arena_players.clear()
	render_players = {}
	for player_variant in player_entries:
		var raw_player := (player_variant as Dictionary).duplicate(true)
		var player_id := int(raw_player.get("player_id", 0))
		arena_players[player_id] = raw_player

		var render_player: Dictionary = raw_player.duplicate(true)
		if previous_render.has(player_id):
			var previous_player: Dictionary = previous_render[player_id]
			render_player["x"] = float(previous_player.get("x", raw_player.get("x", 0)))
			render_player["y"] = float(previous_player.get("y", raw_player.get("y", 0)))
			render_player["aim_x"] = float(previous_player.get("aim_x", raw_player.get("aim_x", 0)))
			render_player["aim_y"] = float(previous_player.get("aim_y", raw_player.get("aim_y", 0)))
		render_players[player_id] = render_player


func _replace_arena_projectiles(projectile_entries: Array) -> void:
	var previous_render := render_projectiles.duplicate(true)
	arena_projectiles.clear()
	render_projectiles.clear()
	for index in range(projectile_entries.size()):
		var raw_projectile := (projectile_entries[index] as Dictionary).duplicate(true)
		arena_projectiles.append(raw_projectile)

		var render_projectile: Dictionary = raw_projectile.duplicate(true)
		if index < previous_render.size():
			var previous_projectile: Dictionary = previous_render[index]
			var same_identity := (
				int(previous_projectile.get("owner", -1)) == int(raw_projectile.get("owner", -2))
				and int(previous_projectile.get("slot", -1)) == int(raw_projectile.get("slot", -2))
				and String(previous_projectile.get("kind", "")) == String(raw_projectile.get("kind", ""))
			)
			if same_identity:
				render_projectile["x"] = float(previous_projectile.get("x", raw_projectile.get("x", 0)))
				render_projectile["y"] = float(previous_projectile.get("y", raw_projectile.get("y", 0)))
		render_projectiles.append(render_projectile)


func _smooth_render_state(delta: float) -> bool:
	var changed := false
	var player_step := clampf(delta * PLAYER_RENDER_LERP_RATE, 0.0, 1.0)
	for player_id in render_players.keys():
		if not arena_players.has(player_id):
			continue
		var render_player: Dictionary = render_players[player_id]
		var target_player: Dictionary = arena_players[player_id]
		var next_x := _smooth_axis(
			float(render_player.get("x", target_player.get("x", 0))),
			float(target_player.get("x", 0)),
			player_step
		)
		var next_y := _smooth_axis(
			float(render_player.get("y", target_player.get("y", 0))),
			float(target_player.get("y", 0)),
			player_step
		)
		var next_aim_x := _smooth_axis(
			float(render_player.get("aim_x", target_player.get("aim_x", 0))),
			float(target_player.get("aim_x", 0)),
			player_step
		)
		var next_aim_y := _smooth_axis(
			float(render_player.get("aim_y", target_player.get("aim_y", 0))),
			float(target_player.get("aim_y", 0)),
			player_step
		)
		if not is_equal_approx(float(render_player.get("x", next_x)), next_x) \
			or not is_equal_approx(float(render_player.get("y", next_y)), next_y) \
			or not is_equal_approx(float(render_player.get("aim_x", next_aim_x)), next_aim_x) \
			or not is_equal_approx(float(render_player.get("aim_y", next_aim_y)), next_aim_y):
			changed = true
		render_player["x"] = next_x
		render_player["y"] = next_y
		render_player["aim_x"] = next_aim_x
		render_player["aim_y"] = next_aim_y

	var projectile_step := clampf(delta * PROJECTILE_RENDER_LERP_RATE, 0.0, 1.0)
	for index in range(min(render_projectiles.size(), arena_projectiles.size())):
		var render_projectile: Dictionary = render_projectiles[index]
		var target_projectile: Dictionary = arena_projectiles[index]
		var next_projectile_x := _smooth_axis(
			float(render_projectile.get("x", target_projectile.get("x", 0))),
			float(target_projectile.get("x", 0)),
			projectile_step
		)
		var next_projectile_y := _smooth_axis(
			float(render_projectile.get("y", target_projectile.get("y", 0))),
			float(target_projectile.get("y", 0)),
			projectile_step
		)
		if not is_equal_approx(float(render_projectile.get("x", next_projectile_x)), next_projectile_x) \
			or not is_equal_approx(float(render_projectile.get("y", next_projectile_y)), next_projectile_y):
			changed = true
		render_projectile["x"] = next_projectile_x
		render_projectile["y"] = next_projectile_y
	return changed


func _smooth_axis(current: float, target: float, step: float) -> float:
	if absf(target - current) > RENDER_SNAP_DISTANCE_UNITS:
		return target
	return lerpf(current, target, step)


func _mask_has_tile(mask: PackedByteArray, column: int, row: int) -> bool:
	var tile_width := arena_tile_width()
	var tile_height := arena_tile_height()
	if tile_width <= 0 or tile_height <= 0:
		return true
	if column < 0 or row < 0 or column >= tile_width or row >= tile_height:
		return false
	var index := row * tile_width + column
	var byte_index := int(index / 8)
	var bit_index := int(index % 8)
	if byte_index < 0 or byte_index >= mask.size():
		return false
	return (int(mask[byte_index]) & (1 << bit_index)) != 0


func _reset_diagnostics() -> void:
	diagnostics = {
		"timings": {
			"ui_refresh": _new_timing_bucket(),
			"ui_refresh_catalog": _new_timing_bucket(),
			"ui_refresh_labels": _new_timing_bucket(),
			"ui_refresh_logs": _new_timing_bucket(),
			"ui_refresh_diagnostics": _new_timing_bucket(),
			"ui_refresh_buttons": _new_timing_bucket(),
			"ui_refresh_visibility": _new_timing_bucket(),
			"advance_visuals": _new_timing_bucket(),
			"packet_decode": _new_timing_bucket(),
			"snapshot_apply": _new_timing_bucket(),
			"arena_draw": _new_timing_bucket(),
			"arena_draw_cache_sync": _new_timing_bucket(),
			"arena_draw_base": _new_timing_bucket(),
			"arena_draw_floor": _new_timing_bucket(),
			"arena_draw_grid": _new_timing_bucket(),
			"arena_draw_obstacles": _new_timing_bucket(),
			"arena_draw_visibility": _new_timing_bucket(),
			"arena_draw_effects": _new_timing_bucket(),
			"arena_draw_deployables": _new_timing_bucket(),
			"arena_draw_projectiles": _new_timing_bucket(),
			"arena_draw_players": _new_timing_bucket(),
			"arena_draw_combat_text": _new_timing_bucket(),
			"arena_draw_border": _new_timing_bucket(),
			"arena_cache_background": _new_timing_bucket(),
			"arena_cache_visibility": _new_timing_bucket(),
		},
		"packets": {
			"control_events": 0,
			"full_snapshots": 0,
			"delta_snapshots": 0,
			"effect_batches": 0,
			"combat_text_batches": 0,
			"bytes_in_total": 0,
			"last_packet_bytes": 0,
			"last_event_type": "",
			"last_sim_tick": 0,
			"last_packet_kind": -1,
		},
		"objects": {
			"players": 0,
			"projectiles": 0,
			"deployables": 0,
			"effects": 0,
			"combat_texts": 0,
		},
		"tiles": {
			"footprint": 0,
			"objective": 0,
			"visible": 0,
			"explored": 0,
		},
		"render": {
			"arena_pixels_w": 0,
			"arena_pixels_h": 0,
		},
		"godot_builtin_monitors": GodotPerfMonitorsScript.snapshot_builtin_monitors(),
	}


func _mark_arena_render_dirty() -> void:
	arena_render_revision += 1


func _new_timing_bucket() -> Dictionary:
	return {
		"count": 0,
		"last_us": 0,
		"max_us": 0,
		"total_us": 0,
		"recent_us": [],
	}


func _record_timing_bucket(bucket: Dictionary, micros: int) -> void:
	var recent: Array = bucket.get("recent_us", [])
	recent.append(micros)
	while recent.size() > 120:
		recent.remove_at(0)
	bucket["recent_us"] = recent
	bucket["count"] = int(bucket.get("count", 0)) + 1
	bucket["last_us"] = micros
	bucket["max_us"] = maxi(int(bucket.get("max_us", 0)), micros)
	bucket["total_us"] = int(bucket.get("total_us", 0)) + micros


func _timing_bucket_snapshot(bucket_value: Variant) -> Dictionary:
	var bucket := bucket_value as Dictionary
	var count := int(bucket.get("count", 0))
	var recent: Array = (bucket.get("recent_us", []) as Array).duplicate()
	recent.sort()
	return {
		"count": count,
		"last_ms": float(bucket.get("last_us", 0)) / 1000.0,
		"avg_ms": (float(bucket.get("total_us", 0)) / float(maxi(1, count))) / 1000.0,
		"max_ms": float(bucket.get("max_us", 0)) / 1000.0,
		"p50_ms": _percentile_ms(recent, 0.50),
		"p95_ms": _percentile_ms(recent, 0.95),
	}


func _percentile_ms(sorted_values: Array, quantile: float) -> float:
	if sorted_values.is_empty():
		return 0.0
	var index := int(round((sorted_values.size() - 1) * quantile))
	index = clampi(index, 0, sorted_values.size() - 1)
	return float(sorted_values[index]) / 1000.0


func _refresh_diagnostics_counts() -> void:
	var object_stats: Dictionary = diagnostics.get("objects", {})
	object_stats["players"] = arena_players.size()
	object_stats["projectiles"] = arena_projectiles.size()
	object_stats["deployables"] = arena_deployables.size()
	object_stats["effects"] = arena_effects.size()
	object_stats["combat_texts"] = local_combat_texts.size()
	diagnostics["objects"] = object_stats

	var tile_stats: Dictionary = diagnostics.get("tiles", {})
	tile_stats["footprint"] = _mask_set_bit_count(footprint_tiles)
	tile_stats["objective"] = _mask_set_bit_count(objective_tiles)
	tile_stats["visible"] = _mask_set_bit_count(visible_tiles)
	tile_stats["explored"] = _mask_set_bit_count(explored_tiles)
	diagnostics["tiles"] = tile_stats


func _mask_set_bit_count(mask: PackedByteArray) -> int:
	var total := 0
	for byte_value in mask:
		var value := int(byte_value)
		while value != 0:
			total += value & 1
			value >>= 1
	return total


func is_training_mode() -> bool:
	return screen == "match" and arena_mode == "Training"


func training_metrics_text() -> String:
	if not is_training_mode():
		return ""
	return ClientStateViewScript.training_metrics_text(training_metrics)


func _effect_ttl_seconds(kind_name: String) -> float:
	match kind_name:
		"MeleeSwing":
			return 0.18
		"DashTrail":
			return 0.24
		"HitSpark":
			return 0.16
		"Beam":
			return 0.22
		_:
			return 0.35
