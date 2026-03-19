# tests

This directory contains headless Godot checks for protocol behavior, shell layout, exports, and transport assumptions.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `protocol_checks.gd`: Headless Godot test script that exercises packet and protocol assumptions.
- `protocol_checks.gd.uid`: Godot UID sidecar for `protocol_checks`. It preserves a stable resource identifier for the neighboring script.
- `shell_layout_checks.gd`: Headless Godot test script that verifies the shell's layout and flow transitions.
- `shell_layout_checks.gd.uid`: Godot UID sidecar for `shell_layout_checks`. It preserves a stable resource identifier for the neighboring script.
- `web_export_checks.gd`: Headless Godot test script that validates the browser export assumptions.
- `web_export_checks.gd.uid`: Godot UID sidecar for `web_export_checks`. It preserves a stable resource identifier for the neighboring script.
- `webrtc_transport_checks.gd.uid`: Godot UID sidecar that reserves the stable resource identifier for the transport check script.
