extends Node2D
class_name GameAudioRuntime

const SpellAudioRegistryScript := preload("res://scripts/content/spell_audio_registry.gd")
const PerfClockScript := preload("res://scripts/debug/perf_clock.gd")

const PLAYER_POOL_SIZE := 18
const DEFAULT_SAMPLE_RATE_HZ := 22050
const DEFAULT_DURATION_MS := 150

var app_state: ClientState = null
var _manifest: Dictionary = {}
var _players: Array[AudioStreamPlayer2D] = []
var _next_player_index := 0
var _stream_cache: Dictionary = {}
var _recent_cue_us: Dictionary = {}


func _ready() -> void:
	for _index in range(PLAYER_POOL_SIZE):
		var player := AudioStreamPlayer2D.new()
		player.bus = "Master"
		add_child(player)
		_players.append(player)


func set_client_state(state: ClientState) -> void:
	app_state = state
	_manifest = {}
	if app_state != null:
		_manifest = app_state.spell_audio_registry


func play_effect_batch(effects: Array, canvas_size: Vector2) -> void:
	if app_state == null:
		return
	var now_us := PerfClockScript.now_us()
	_prune_recent_cues(now_us)
	for raw_effect in effects:
		var effect := (raw_effect as Dictionary).duplicate(true)
		var cue_id := String(effect.get("audio_cue_id", "")).strip_edges()
		if cue_id == "":
			continue
		if _is_duplicate_effect(effect, cue_id, now_us):
			continue
		_play_cue(effect, cue_id, canvas_size)


func preview_stream_for_cue(cue_id: String) -> AudioStream:
	var spec := SpellAudioRegistryScript.lookup(_manifest, cue_id)
	return _stream_for_cue(cue_id, spec)


func _play_cue(effect: Dictionary, cue_id: String, canvas_size: Vector2) -> void:
	var spec := SpellAudioRegistryScript.lookup(_manifest, cue_id)
	var stream := _stream_for_cue(cue_id, spec)
	if stream == null:
		return
	var player := _next_player()
	player.stop()
	player.stream = stream
	player.position = _canvas_audio_position(effect, canvas_size)
	player.volume_db = _cue_volume_db(effect, spec)
	player.pitch_scale = clampf(float(spec.get("pitch_scale", 1.0)), 0.5, 2.5)
	player.play()


func _next_player() -> AudioStreamPlayer2D:
	if _players.is_empty():
		var fallback := AudioStreamPlayer2D.new()
		add_child(fallback)
		_players.append(fallback)
	var player := _players[_next_player_index]
	_next_player_index = (_next_player_index + 1) % _players.size()
	return player


func _stream_for_cue(cue_id: String, spec: Dictionary) -> AudioStream:
	if _stream_cache.has(cue_id):
		return _stream_cache[cue_id]
	var stream := _load_stream_from_spec(spec)
	if stream == null:
		stream = _build_generated_stream(_merged_spec(spec, _fallback_spec(cue_id)))
	if stream != null:
		_stream_cache[cue_id] = stream
	return stream


func _load_stream_from_spec(spec: Dictionary) -> AudioStream:
	var relative_path := String(spec.get("file", "")).strip_edges()
	if relative_path == "":
		return null
	var asset_root := String(_manifest.get("asset_root", "res://assets/audio/spells")).trim_suffix("/")
	var resource_path := "%s/%s" % [asset_root, relative_path]
	if not ResourceLoader.exists(resource_path):
		return null
	var loaded: Resource = load(resource_path)
	if loaded is AudioStream:
		return loaded as AudioStream
	return null


func _merged_spec(spec: Dictionary, fallback: Dictionary) -> Dictionary:
	var merged := fallback.duplicate(true)
	for key in spec.keys():
		merged[key] = spec[key]
	return merged


func _fallback_spec(cue_id: String) -> Dictionary:
	var cue_hash: int = abs(hash(cue_id))
	var waveform_options: Array[String] = ["sine", "triangle", "square", "saw"]
	var waveform: String = waveform_options[cue_hash % waveform_options.size()]
	var duration_ms: int = 90 + (cue_hash % 110)
	var frequency_hz: float = 170.0 + float(cue_hash % 420)
	var harmonic_hz: float = frequency_hz * (2.0 if cue_hash % 3 == 0 else 1.5)
	return {
		"waveform": waveform,
		"duration_ms": duration_ms,
		"frequency_hz": frequency_hz,
		"harmonic_hz": harmonic_hz,
		"harmonic_gain": 0.26,
		"attack_ms": 8,
		"release_ms": 44,
		"amplitude": 0.46,
		"volume_db": -11.0,
	}


func _build_generated_stream(spec: Dictionary) -> AudioStreamWAV:
	var sample_rate_hz := maxi(8000, int(spec.get("sample_rate_hz", DEFAULT_SAMPLE_RATE_HZ)))
	var duration_ms := maxi(35, int(spec.get("duration_ms", DEFAULT_DURATION_MS)))
	var sample_count := maxi(1, int(round((float(sample_rate_hz) * float(duration_ms)) / 1000.0)))
	var waveform := String(spec.get("waveform", "sine"))
	var frequency_hz := maxf(40.0, float(spec.get("frequency_hz", 220.0)))
	var harmonic_hz := maxf(0.0, float(spec.get("harmonic_hz", 0.0)))
	var harmonic_gain := clampf(float(spec.get("harmonic_gain", 0.0)), 0.0, 1.0)
	var amplitude := clampf(float(spec.get("amplitude", 0.45)), 0.05, 1.0)
	var attack_ms := clampf(float(spec.get("attack_ms", 6.0)), 0.0, float(duration_ms))
	var release_ms := clampf(float(spec.get("release_ms", 36.0)), 0.0, float(duration_ms))
	var data := PackedByteArray()
	data.resize(sample_count * 2)

	# Generate a small deterministic PCM clip so every cue can be played even before real assets arrive.
	for sample_index in range(sample_count):
		var time_seconds := float(sample_index) / float(sample_rate_hz)
		var envelope := _envelope_value(sample_index, sample_count, attack_ms, release_ms, sample_rate_hz)
		var sample := _waveform_value(waveform, frequency_hz, time_seconds)
		if harmonic_hz > 0.0 and harmonic_gain > 0.0:
			sample = sample * (1.0 - harmonic_gain) + _waveform_value("sine", harmonic_hz, time_seconds) * harmonic_gain
		sample *= envelope * amplitude
		var sample_i16 := int(round(clampf(sample, -1.0, 1.0) * 32767.0))
		data[sample_index * 2] = sample_i16 & 0xFF
		data[sample_index * 2 + 1] = (sample_i16 >> 8) & 0xFF

	var stream := AudioStreamWAV.new()
	stream.format = AudioStreamWAV.FORMAT_16_BITS
	stream.mix_rate = sample_rate_hz
	stream.stereo = false
	stream.data = data
	return stream


func _envelope_value(
	sample_index: int,
	sample_count: int,
	attack_ms: float,
	release_ms: float,
	sample_rate_hz: int
) -> float:
	var attack_samples := maxi(0, int(round((attack_ms / 1000.0) * float(sample_rate_hz))))
	var release_samples := maxi(0, int(round((release_ms / 1000.0) * float(sample_rate_hz))))
	var value := 1.0
	if attack_samples > 0 and sample_index < attack_samples:
		value *= float(sample_index) / float(attack_samples)
	if release_samples > 0:
		var release_start := maxi(0, sample_count - release_samples)
		if sample_index >= release_start:
			var release_progress := float(sample_count - sample_index) / float(maxi(1, release_samples))
			value *= clampf(release_progress, 0.0, 1.0)
	return value


func _waveform_value(kind: String, frequency_hz: float, time_seconds: float) -> float:
	var phase := fmod(time_seconds * frequency_hz, 1.0)
	match kind:
		"square":
			return 1.0 if phase < 0.5 else -1.0
		"triangle":
			return 1.0 - absf(phase * 4.0 - 2.0)
		"saw":
			return phase * 2.0 - 1.0
		"noise":
			return sin((time_seconds * frequency_hz * 13.0) + float(abs(hash("%0.4f" % time_seconds)) % 29))
		_:
			return sin(time_seconds * TAU * frequency_hz)


func _canvas_audio_position(effect: Dictionary, canvas_size: Vector2) -> Vector2:
	var center := canvas_size * 0.5
	if app_state == null:
		return center
	var listener := app_state.local_arena_player()
	if listener.is_empty():
		return center
	var arena_width_units := maxf(1.0, float(app_state.arena_width))
	var arena_height_units := maxf(1.0, float(app_state.arena_height))
	var delta_x := float(effect.get("x", 0)) - float(listener.get("x", 0))
	var delta_y := float(effect.get("y", 0)) - float(listener.get("y", 0))
	var normalized_x := clampf(delta_x / (arena_width_units * 0.5), -1.0, 1.0)
	var normalized_y := clampf(delta_y / (arena_height_units * 0.5), -1.0, 1.0)
	return center + Vector2(normalized_x * center.x * 0.72, normalized_y * center.y * 0.72)


func _cue_volume_db(effect: Dictionary, spec: Dictionary) -> float:
	var base_volume_db := float(spec.get("volume_db", -11.0))
	if app_state == null:
		return base_volume_db
	var listener := app_state.local_arena_player()
	if listener.is_empty():
		return base_volume_db
	var delta_x := float(effect.get("x", 0)) - float(listener.get("x", 0))
	var delta_y := float(effect.get("y", 0)) - float(listener.get("y", 0))
	var distance_units := Vector2(delta_x, delta_y).length()
	var audible_radius_units := maxf(1.0, float(effect.get("radius", 1)))
	var distance_ratio := clampf(distance_units / audible_radius_units, 0.0, 1.0)
	return base_volume_db - (distance_ratio * 14.0)


func _is_duplicate_effect(effect: Dictionary, cue_id: String, now_us: int) -> bool:
	var dedupe_window_us := 120000
	var kind_name := String(effect.get("kind", ""))
	if kind_name == "Footstep" or kind_name == "BrushRustle" or kind_name == "StealthFootstep":
		dedupe_window_us = 60000
	var dedupe_key := "%s|%s|%s|%s" % [
		cue_id,
		String(effect.get("owner", 0)),
		String(effect.get("slot", 0)),
		kind_name,
	]
	var previous_us := int(_recent_cue_us.get(dedupe_key, 0))
	if now_us - previous_us < dedupe_window_us:
		return true
	_recent_cue_us[dedupe_key] = now_us
	return false


func _prune_recent_cues(now_us: int) -> void:
	var expired: Array[String] = []
	for key in _recent_cue_us.keys():
		if now_us - int(_recent_cue_us.get(key, 0)) > 1500000:
			expired.append(String(key))
	for key in expired:
		_recent_cue_us.erase(key)
