# corpus

This directory contains checked-in fuzz corpora grouped by target so replay coverage and smoke runs stay deterministic.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `arena_delta_snapshot_decode/`: checked-in seed inputs for the `arena_delta_snapshot_decode` fuzz target.
- `arena_delta_snapshot_roundtrip/`: checked-in seed inputs for the `arena_delta_snapshot_roundtrip` fuzz target.
- `arena_full_snapshot_decode/`: checked-in seed inputs for the `arena_full_snapshot_decode` fuzz target.
- `arena_full_snapshot_roundtrip/`: checked-in seed inputs for the `arena_full_snapshot_roundtrip` fuzz target.
- `ascii_map_parse/`: checked-in seed inputs for the `ascii_map_parse` fuzz target.
- `control_command_decode/`: checked-in seed inputs for the `control_command_decode` fuzz target.
- `control_command_roundtrip/`: checked-in seed inputs for the `control_command_roundtrip` fuzz target.
- `http_route_classification/`: checked-in seed inputs for the `http_route_classification` fuzz target.
- `input_frame_decode/`: checked-in seed inputs for the `input_frame_decode` fuzz target.
- `input_frame_roundtrip/`: checked-in seed inputs for the `input_frame_roundtrip` fuzz target.
- `observability_metrics_render/`: checked-in seed inputs for the `observability_metrics_render` fuzz target.
- `packet_header_decode/`: checked-in seed inputs for the `packet_header_decode` fuzz target.
- `player_record_store_parse/`: checked-in seed inputs for the `player_record_store_parse` fuzz target.
- `server_control_event_decode/`: checked-in seed inputs for the `server_control_event_decode` fuzz target.
- `server_control_event_roundtrip/`: checked-in seed inputs for the `server_control_event_roundtrip` fuzz target.
- `session_ingress/`: checked-in seed inputs for the `session_ingress` fuzz target.
- `session_ingress_sequence/`: checked-in seed inputs for the `session_ingress_sequence` fuzz target.
- `skill_progression/`: checked-in seed inputs for the `skill_progression` fuzz target.
- `skill_yaml_parse/`: checked-in seed inputs for the `skill_yaml_parse` fuzz target.
- `webrtc_signal_message_parse/`: checked-in seed inputs for the `webrtc_signal_message_parse` fuzz target.
- `webrtc_signal_message_roundtrip/`: checked-in seed inputs for the `webrtc_signal_message_roundtrip` fuzz target.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
