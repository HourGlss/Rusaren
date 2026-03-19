# fuzz_targets

This directory contains cargo-fuzz entrypoints for each parser, codec, and ingress boundary under test.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `arena_delta_snapshot_decode.rs`: cargo-fuzz entrypoint for the `arena_delta_snapshot_decode` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `arena_delta_snapshot_roundtrip.rs`: cargo-fuzz entrypoint for the `arena_delta_snapshot_roundtrip` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `arena_full_snapshot_decode.rs`: cargo-fuzz entrypoint for the `arena_full_snapshot_decode` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `arena_full_snapshot_roundtrip.rs`: cargo-fuzz entrypoint for the `arena_full_snapshot_roundtrip` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `ascii_map_parse.rs`: cargo-fuzz entrypoint for the `ascii_map_parse` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `control_command_decode.rs`: cargo-fuzz entrypoint for the `control_command_decode` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `control_command_roundtrip.rs`: cargo-fuzz entrypoint for the `control_command_roundtrip` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `http_route_classification.rs`: cargo-fuzz entrypoint for the `http_route_classification` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `input_frame_decode.rs`: cargo-fuzz entrypoint for the `input_frame_decode` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `input_frame_roundtrip.rs`: cargo-fuzz entrypoint for the `input_frame_roundtrip` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `observability_metrics_render.rs`: cargo-fuzz entrypoint for the `observability_metrics_render` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `packet_header_decode.rs`: cargo-fuzz entrypoint for the `packet_header_decode` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `player_record_store_parse.rs`: cargo-fuzz entrypoint for the `player_record_store_parse` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `server_control_event_decode.rs`: cargo-fuzz entrypoint for the `server_control_event_decode` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `server_control_event_roundtrip.rs`: cargo-fuzz entrypoint for the `server_control_event_roundtrip` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `session_ingress.rs`: cargo-fuzz entrypoint for the `session_ingress` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `session_ingress_sequence.rs`: cargo-fuzz entrypoint for the `session_ingress_sequence` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `skill_progression.rs`: cargo-fuzz entrypoint for the `skill_progression` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `skill_yaml_parse.rs`: cargo-fuzz entrypoint for the `skill_yaml_parse` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `webrtc_signal_message_parse.rs`: cargo-fuzz entrypoint for the `webrtc_signal_message_parse` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
- `webrtc_signal_message_roundtrip.rs`: cargo-fuzz entrypoint for the `webrtc_signal_message_roundtrip` target. It feeds generated inputs into the targeted parser, codec, or ingress boundary.
