# Fuzz Targets

This package contains the first `cargo-fuzz` targets for the backend network boundary.

Current targets:
- `packet_header_decode`
- `control_command_decode`
- `input_frame_decode`
- `session_ingress`

Use:

```powershell
cd server
./scripts/install-tools.ps1 -IncludeNightly -IncludeFuzzTools
./scripts/quality.ps1 fuzz
```

To run one target interactively:

```powershell
cd server
rustup run nightly cargo fuzz run packet_header_decode
```
