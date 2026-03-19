# verus

This directory contains Verus models that specify and check the backend's protocol and ingress invariants.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `http_route_model.rs`: Verus model for HTTP route classification behavior.
- `input_stream_model.rs`: Verus model for reasoning about streamed input acceptance and ordering.
- `network_ingress_model.rs`: Verus model for ingress guard and packet acceptance rules.
- `packet_header_model.rs`: Verus model for packet-header invariants.
- `player_record_store_model.rs`: Verus model for persistent player-record storage behavior.
- `session_bootstrap_model.rs`: Verus model for session bootstrap token behavior.
- `webrtc_signaling_model.rs`: Verus model for signaling-message acceptance and rejection.
