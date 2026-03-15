extends RefCounted
class_name ClientState

const MAX_EVENT_LINES := 28
const WebSocketConfigScript := preload("res://scripts/net/websocket_config.gd")
const KNOWN_SKILL_TREES := ["Warrior", "Rogue", "Mage", "Cleric"]

var websocket_url := WebSocketConfigScript.new().runtime_default_url()
var local_player_id := 0
var local_player_name := ""
var transport_state := "closed"
var screen := "central"
var banner_message := "Connect to the Rust signaling endpoint to negotiate WebRTC gameplay channels. Browser exports default to the same-origin /ws endpoint."
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
}
var lobby_directory: Array[Dictionary] = []
var roster := {}
var recent_events: Array[String] = []
var local_skill_progress := {}
var local_round_skill_locked := false
var arena_width := 0
var arena_height := 0
var arena_obstacles: Array[Dictionary] = []
var arena_players := {}
var arena_projectiles: Array[Dictionary] = []
var arena_effects: Array[Dictionary] = []


func prepare_for_connection(url: String, player_name: String) -> void:
	websocket_url = WebSocketConfigScript.new().runtime_default_url(url)
	local_player_id = 0
	local_player_name = player_name.strip_edges()
	transport_state = "connecting"
	screen = "central"
	banner_message = "Connecting to %s as %s. Waiting for a server-assigned player ID." % [
		websocket_url,
		local_player_name,
	]
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
	_reset_local_skill_progress()
	local_round_skill_locked = false
	_clear_arena_state()
	_append_event("Connecting to %s." % websocket_url)


func mark_transport_state(state_name: String) -> void:
	transport_state = state_name
	match state_name:
		"connecting":
			banner_message = "Signaling and WebRTC negotiation are in progress."
		"open":
			banner_message = "WebRTC control channel is open. Waiting for the server to accept the connect command."
		"closing":
			banner_message = "Realtime transport closing."
		"closed":
			banner_message = "Realtime transport closed."
	_append_event("Transport state: %s." % state_name)


func mark_transport_closed(reason: String) -> void:
	transport_state = "closed"
	if reason != "":
		banner_message = reason
		_append_event(reason)


func mark_transport_error(message: String) -> void:
	banner_message = message
	_append_event("Error: %s" % message)


func announce_local(message: String) -> void:
	banner_message = message
	_append_event(message)


func apply_server_event(event: Dictionary) -> void:
	var event_type := String(event.get("type", "Unknown"))

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
			_reset_local_skill_progress()
			local_round_skill_locked = false
			_clear_arena_state()
			banner_message = "Connected as %s." % local_player_name
			_append_event("Connected as %s (#%d)." % [local_player_name, local_player_id])
		"LobbyDirectorySnapshot":
			lobby_directory = event.get("lobbies", []).duplicate(true)
			var lobby_count := lobby_directory.size()
			banner_message = "Lobby directory updated: %d open entr%s." % [
				lobby_count,
				"ies" if lobby_count != 1 else "y",
			]
			_append_event(banner_message)
		"GameLobbyCreated":
			current_lobby_id = int(event.get("lobby_id", 0))
			screen = "lobby"
			phase_label = "Game Lobby #%d" % current_lobby_id
			lobby_locked = false
			banner_message = "Created lobby #%d." % current_lobby_id
			_append_event("Created lobby #%d." % current_lobby_id)
		"GameLobbyJoined":
			current_lobby_id = int(event.get("lobby_id", current_lobby_id))
			screen = "lobby"
			phase_label = "Game Lobby #%d" % current_lobby_id
			var joined_player_id := int(event.get("player_id", 0))
			var joined_name := local_player_name if joined_player_id == local_player_id else ""
			var member := _ensure_roster_entry(joined_player_id, joined_name)
			member["ready"] = "Not Ready"
			member["skill"] = ""
			banner_message = "%s joined lobby #%d." % [_display_name(joined_player_id), current_lobby_id]
			_append_event("%s joined lobby #%d." % [_display_name(joined_player_id), current_lobby_id])
		"GameLobbySnapshot":
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
		"GameLobbyLeft":
			var left_player_id := int(event.get("player_id", 0))
			if left_player_id == local_player_id:
				_reset_to_central()
				banner_message = "Returned to the central lobby."
			else:
				roster.erase(left_player_id)
				banner_message = "%s left lobby #%d." % [_display_name(left_player_id), current_lobby_id]
			_append_event("%s left lobby #%d." % [_display_name(left_player_id), current_lobby_id])
		"TeamSelected":
			var team_player_id := int(event.get("player_id", 0))
			var member := _ensure_roster_entry(team_player_id)
			member["team"] = String(event.get("team", "Unassigned"))
			if bool(event.get("ready_reset", false)):
				member["ready"] = "Not Ready"
			banner_message = "%s moved to %s." % [_display_name(team_player_id), member["team"]]
			_append_event("%s moved to %s." % [_display_name(team_player_id), member["team"]])
		"ReadyChanged":
			var ready_player_id := int(event.get("player_id", 0))
			var ready_member := _ensure_roster_entry(ready_player_id)
			ready_member["ready"] = String(event.get("ready", "Not Ready"))
			banner_message = "%s is now %s." % [_display_name(ready_player_id), ready_member["ready"]]
			_append_event("%s is now %s." % [_display_name(ready_player_id), ready_member["ready"]])
		"LaunchCountdownStarted":
			lobby_locked = true
			countdown_label = "Launch locks in %ds for %d players." % [
				int(event.get("seconds_remaining", 0)),
				int(event.get("roster_size", 0)),
			]
			banner_message = countdown_label
			_append_event(countdown_label)
		"LaunchCountdownTick":
			lobby_locked = true
			countdown_label = "Launch in %ds." % int(event.get("seconds_remaining", 0))
			banner_message = countdown_label
			_append_event(countdown_label)
		"MatchStarted":
			screen = "match"
			current_match_id = int(event.get("match_id", 0))
			current_round = int(event.get("round", 0))
			match_phase = "skill_pick"
			_reset_local_skill_progress()
			local_round_skill_locked = false
			_clear_arena_state()
			phase_label = "Match #%d, Round %d" % [current_match_id, current_round]
			countdown_label = "Choose one skill in %ds." % int(event.get("skill_pick_seconds", 0))
			lobby_locked = false
			banner_message = "Match #%d started." % current_match_id
			_append_event("Match #%d started. Round %d skill pick is open." % [
				current_match_id,
				current_round,
			])
		"SkillChosen":
			var skill_player_id := int(event.get("player_id", 0))
			var skill_member := _ensure_roster_entry(skill_player_id)
			var tree_name := String(event.get("tree", "Unknown"))
			var tier := int(event.get("tier", 0))
			skill_member["skill"] = "%s %d" % [tree_name, tier]
			if skill_player_id == local_player_id and KNOWN_SKILL_TREES.has(tree_name):
				local_skill_progress[tree_name] = tier
				local_round_skill_locked = true
			banner_message = "%s locked %s." % [_display_name(skill_player_id), skill_member["skill"]]
			_append_event("%s locked %s." % [_display_name(skill_player_id), skill_member["skill"]])
		"PreCombatStarted":
			match_phase = "pre_combat"
			countdown_label = "Arena unlocks in %ds." % int(event.get("seconds_remaining", 0))
			banner_message = countdown_label
			_append_event(countdown_label)
		"CombatStarted":
			match_phase = "combat"
			countdown_label = "Combat is live. The shell can now send placeholder primary attacks."
			banner_message = countdown_label
			_append_event(countdown_label)
		"RoundWon":
			match_phase = "skill_pick"
			current_round = int(event.get("round", current_round))
			score_a = int(event.get("score_a", score_a))
			score_b = int(event.get("score_b", score_b))
			countdown_label = "Round %d ended. Choose the next skill when ready." % current_round
			local_round_skill_locked = false
			arena_effects.clear()
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
		"MatchEnded":
			screen = "results"
			match_phase = "ended"
			score_a = int(event.get("score_a", score_a))
			score_b = int(event.get("score_b", score_b))
			arena_effects.clear()
			outcome_label = "%s, %d-%d" % [
				String(event.get("outcome", "Unknown")),
				score_a,
				score_b,
			]
			banner_message = String(event.get("message", "Match ended."))
			_append_event(banner_message)
		"ReturnedToCentralLobby":
			record = event.get("record", record)
			_reset_to_central()
			banner_message = "Returned to the central lobby."
			_append_event("Returned to the central lobby.")
		"Error":
			banner_message = String(event.get("message", "Unknown server error"))
			_append_event("Server error: %s" % banner_message)
		"ArenaStateSnapshot":
			var snapshot: Dictionary = event.get("snapshot", {})
			_apply_arena_phase(snapshot)
			arena_width = int(snapshot.get("width", 0))
			arena_height = int(snapshot.get("height", 0))
			arena_obstacles.clear()
			for obstacle_data in snapshot.get("obstacles", []):
				arena_obstacles.append((obstacle_data as Dictionary).duplicate(true))
			arena_players.clear()
			for player_data in snapshot.get("players", []):
				var player_id := int(player_data.get("player_id", 0))
				arena_players[player_id] = player_data.duplicate(true)
			arena_projectiles.clear()
			for projectile_data in snapshot.get("projectiles", []):
				arena_projectiles.append((projectile_data as Dictionary).duplicate(true))
		"ArenaDeltaSnapshot":
			var delta_snapshot: Dictionary = event.get("snapshot", {})
			_apply_arena_phase(delta_snapshot)
			arena_players.clear()
			for player_delta in delta_snapshot.get("players", []):
				var delta_player_id := int(player_delta.get("player_id", 0))
				arena_players[delta_player_id] = player_delta.duplicate(true)
			arena_projectiles.clear()
			for projectile_delta in delta_snapshot.get("projectiles", []):
				arena_projectiles.append((projectile_delta as Dictionary).duplicate(true))
		"ArenaEffectBatch":
			for effect_data in event.get("effects", []):
				var effect: Dictionary = (effect_data as Dictionary).duplicate(true)
				var ttl: float = _effect_ttl_seconds(String(effect.get("kind", "")))
				effect["ttl"] = ttl
				effect["ttl_max"] = ttl
				arena_effects.append(effect)
		_:
			_append_event("Unhandled event: %s" % event_type)


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


func lobby_directory_lines() -> Array[String]:
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


func lobby_directory_bbcode() -> String:
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


func record_text() -> String:
	return "W-L-NC  %d-%d-%d" % [
		int(record.get("wins", 0)),
		int(record.get("losses", 0)),
		int(record.get("no_contests", 0)),
	]


func score_text() -> String:
	return "Team A %d  :  %d Team B" % [score_a, score_b]


func event_log_text() -> String:
	return "\n".join(recent_events)


func lobby_note() -> String:
	return "Click an open lobby in the directory or enter a manual lobby ID. The backend sends authoritative directory, roster, and arena snapshots so late joiners land on current state."


func can_join_or_create_lobby() -> bool:
	return transport_state == "open" and screen == "central"


func can_manage_lobby() -> bool:
	return transport_state == "open" and screen == "lobby" and not lobby_locked


func can_leave_lobby() -> bool:
	return transport_state == "open" and screen == "lobby" and not lobby_locked


func can_choose_skill() -> bool:
	return (
		transport_state == "open"
		and screen == "match"
		and match_phase == "skill_pick"
		and not local_round_skill_locked
	)


func next_skill_tier_for(tree_name: String) -> int:
	if not KNOWN_SKILL_TREES.has(tree_name):
		return 0
	var current_tier := int(local_skill_progress.get(tree_name, 0))
	if current_tier >= 5:
		return 0
	return current_tier + 1


func can_choose_skill_option(tree_name: String, tier: int) -> bool:
	return can_choose_skill() and tier == next_skill_tier_for(tree_name)


func can_quit_results() -> bool:
	return transport_state == "open" and screen == "results"


func can_send_combat_input() -> bool:
	var player := local_arena_player()
	return (
		transport_state == "open"
		and screen == "match"
		and match_phase == "combat"
		and not player.is_empty()
		and bool(player.get("alive", false))
	)


func can_use_combat_slot(slot: int) -> bool:
	if slot < 1 or slot > 5:
		return false
	var player := local_arena_player()
	var cooldowns: Array = player.get("slot_cooldown_remaining_ms", [])
	var remaining := int(cooldowns[slot - 1]) if cooldowns.size() >= slot else 0
	return (
		can_send_combat_input()
		and slot <= int(player.get("unlocked_skill_slots", 0))
		and remaining <= 0
	)


func can_use_primary_attack() -> bool:
	var player := local_arena_player()
	return can_send_combat_input() and int(player.get("primary_cooldown_remaining_ms", 0)) <= 0


func local_arena_player() -> Dictionary:
	if arena_players.has(local_player_id):
		return arena_players[local_player_id]
	return {}


func arena_projectiles_list() -> Array[Dictionary]:
	var projectiles: Array[Dictionary] = []
	for projectile in arena_projectiles:
		projectiles.append((projectile as Dictionary).duplicate(true))
	return projectiles


func cooldown_summary_text() -> String:
	var player := local_arena_player()
	if player.is_empty():
		return "Cooldowns: waiting for a local combat snapshot."

	var labels: Array[String] = []
	labels.append("Melee %s" % _cooldown_token(
		int(player.get("primary_cooldown_remaining_ms", 0)),
		int(player.get("primary_cooldown_total_ms", 0))
	))
	var remaining_list: Array = player.get("slot_cooldown_remaining_ms", [])
	var total_list: Array = player.get("slot_cooldown_total_ms", [])
	for slot in range(1, 6):
		var remaining := int(remaining_list[slot - 1]) if remaining_list.size() >= slot else 0
		var total := int(total_list[slot - 1]) if total_list.size() >= slot else 0
		labels.append("%d %s" % [slot, _cooldown_token(remaining, total)])
	return "Cooldowns: %s" % "  |  ".join(labels)


func arena_players_list() -> Array[Dictionary]:
	var ids := arena_players.keys()
	ids.sort()
	var players: Array[Dictionary] = []
	for player_id in ids:
		players.append((arena_players[player_id] as Dictionary).duplicate(true))
	return players


func advance_visuals(delta: float) -> void:
	for index in range(arena_effects.size() - 1, -1, -1):
		var effect: Dictionary = arena_effects[index]
		var ttl: float = maxf(0.0, float(effect.get("ttl", 0.0)) - delta)
		effect["ttl"] = ttl
		if ttl <= 0.0:
			arena_effects.remove_at(index)


func _apply_arena_phase(snapshot: Dictionary) -> void:
	var phase_name := String(snapshot.get("phase", ""))
	if phase_name.is_empty():
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


func _clear_round_skills() -> void:
	for player_id in roster.keys():
		var member: Dictionary = roster[player_id]
		member["skill"] = "Awaiting next pick"


func _reset_local_skill_progress() -> void:
	local_skill_progress = {
		"Warrior": 0,
		"Rogue": 0,
		"Mage": 0,
		"Cleric": 0,
	}


func _clear_arena_state() -> void:
	arena_width = 0
	arena_height = 0
	arena_obstacles.clear()
	arena_players.clear()
	arena_projectiles.clear()
	arena_effects.clear()


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


func _cooldown_token(remaining_ms: int, total_ms: int) -> String:
	if total_ms <= 0:
		return "-"
	if remaining_ms <= 0:
		return "ready"
	return "%.1fs" % (float(remaining_ms) / 1000.0)
