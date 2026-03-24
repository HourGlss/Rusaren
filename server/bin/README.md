# bin

This directory contains small executable packages that support analysis, serving, and test-data generation around the main backend.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `backend_callgraph/`: the binary crate that builds backend call-graph artifacts from workspace code.
- `dedicated_server/`: the production server executable crate that boots the realtime backend.
- `fuzz_seed_builder/`: the utility crate that writes checked-in fuzz corpus seeds for the backend.
- `live_transport_probe/`: the binary crate that runs a real bootstrap-plus-WebRTC transport probe with four headless clients.
- `scip_json_dump/`: the binary crate that exports SCIP JSON data for analysis tooling.
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
