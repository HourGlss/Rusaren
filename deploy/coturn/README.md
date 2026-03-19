# coturn

This directory contains TURN server configuration shared by the hosted deployment path.
Use the structure notes below to find the right file or subfolder quickly.

## Structure
- `README.md`: This guide documents the folder structure and explains what the checked-in files and subfolders are for.
- `turnserver.conf`: Base coturn configuration shared by the hosted deployment path. Runtime secrets and host-specific flags are injected by Docker Compose.
