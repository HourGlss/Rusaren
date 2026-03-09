# Fuzz Targets

This package contains the `cargo-fuzz` targets for the backend ingress boundary.

Current targets:
- `packet_header_decode`
- `control_command_decode`
- `input_frame_decode`
- `session_ingress`
- `server_control_event_decode`
- `webrtc_signal_message_parse`
- `control_command_roundtrip`
- `input_frame_roundtrip`
- `webrtc_signal_message_roundtrip`

Use:

```powershell
cd server
./scripts/install-tools.ps1 -IncludeNightly -IncludeFuzzTools
./scripts/quality.ps1 fuzz
```

Notes:
- `./scripts/quality.ps1 fuzz` is the repo-standard ingress fuzz smoke task.
- On native Windows/MSVC, that task stays build/smoke oriented because live `cargo fuzz run` is not dependable for this repo on this host.
- The real bounded live fuzz campaigns run in Linux CI through `./scripts/quality.ps1 fuzz-live`, which writes discovered corpora under `server/target/fuzz-generated-corpus/`.

To run one target interactively:

```powershell
cd server
rustup run nightly cargo fuzz run packet_header_decode fuzz/corpus/packet_header_decode -- -max_total_time=60
```
