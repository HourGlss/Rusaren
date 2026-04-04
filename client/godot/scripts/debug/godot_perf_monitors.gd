extends RefCounted
class_name GodotPerfMonitors

const BUILTIN_MONITORS := {
	"fps": {
		"monitor": Performance.TIME_FPS,
		"unit": "fps",
	},
	"process_time_ms": {
		"monitor": Performance.TIME_PROCESS,
		"unit": "seconds_to_ms",
	},
	"physics_process_time_ms": {
		"monitor": Performance.TIME_PHYSICS_PROCESS,
		"unit": "seconds_to_ms",
	},
	"object_count": {
		"monitor": Performance.OBJECT_COUNT,
		"unit": "quantity",
	},
	"node_count": {
		"monitor": Performance.OBJECT_NODE_COUNT,
		"unit": "quantity",
	},
	"orphan_node_count": {
		"monitor": Performance.OBJECT_ORPHAN_NODE_COUNT,
		"unit": "quantity",
	},
	"render_total_objects_in_frame": {
		"monitor": Performance.RENDER_TOTAL_OBJECTS_IN_FRAME,
		"unit": "quantity",
	},
	"render_total_primitives_in_frame": {
		"monitor": Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME,
		"unit": "quantity",
	},
	"render_total_draw_calls_in_frame": {
		"monitor": Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME,
		"unit": "quantity",
	},
	"render_video_mem_used_mb": {
		"monitor": Performance.RENDER_VIDEO_MEM_USED,
		"unit": "bytes_to_mb",
	},
}

const BUILTIN_MONITOR_ORDER := [
	"fps",
	"process_time_ms",
	"physics_process_time_ms",
	"object_count",
	"node_count",
	"orphan_node_count",
	"render_total_objects_in_frame",
	"render_total_primitives_in_frame",
	"render_total_draw_calls_in_frame",
	"render_video_mem_used_mb",
]


static func snapshot_builtin_monitors() -> Dictionary:
	var snapshot := {}
	for metric_name in BUILTIN_MONITOR_ORDER:
		var entry: Dictionary = BUILTIN_MONITORS.get(metric_name, {})
		var raw_value := float(Performance.get_monitor(int(entry.get("monitor", 0))))
		snapshot[metric_name] = _normalize_monitor_value(raw_value, String(entry.get("unit", "quantity")))
	return snapshot


static func add_or_replace_custom_monitor(
	id: String,
	callable: Callable
) -> void:
	if Performance.has_custom_monitor(id):
		Performance.remove_custom_monitor(id)
	Performance.add_custom_monitor(id, callable)


static func remove_custom_monitors(ids: Array) -> void:
	for id in ids:
		if Performance.has_custom_monitor(id):
			Performance.remove_custom_monitor(id)


static func builtin_monitor_lines(snapshot: Dictionary) -> Array[String]:
	return [
		"",
		"Godot Monitors",
		"  fps: %.1f" % float(snapshot.get("fps", 0.0)),
		"  process_time_ms: %.3f" % float(snapshot.get("process_time_ms", 0.0)),
		"  physics_process_time_ms: %.3f" % float(snapshot.get("physics_process_time_ms", 0.0)),
		"  object_count: %d" % int(snapshot.get("object_count", 0)),
		"  node_count: %d" % int(snapshot.get("node_count", 0)),
		"  orphan_node_count: %d" % int(snapshot.get("orphan_node_count", 0)),
		"  render_total_objects_in_frame: %d" % int(snapshot.get("render_total_objects_in_frame", 0)),
		"  render_total_primitives_in_frame: %d" % int(snapshot.get("render_total_primitives_in_frame", 0)),
		"  render_total_draw_calls_in_frame: %d" % int(snapshot.get("render_total_draw_calls_in_frame", 0)),
		"  render_video_mem_used_mb: %.3f" % float(snapshot.get("render_video_mem_used_mb", 0.0)),
	]


static func _normalize_monitor_value(raw_value: float, unit: String) -> float:
	match unit:
		"seconds_to_ms":
			return raw_value * 1000.0
		"bytes_to_mb":
			return raw_value / (1024.0 * 1024.0)
		_:
			return raw_value
