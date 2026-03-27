# nexus

Peer-to-peer file synchronization. Like Syncthing, but built from scratch to understand how it actually works.

## Why?

Central servers are single points of failure, privacy risks, and monthly bills. Your files should sync directly between your devices, encrypted end-to-end, without asking anyone's permission.

## Features

- **Content-Addressed Storage**: Files stored by SHA-256 hash. Same content = same blocks = automatic deduplication.
- **Block-Level Sync**: Only transfer changed blocks, not entire files. Edit a 1GB video? Send just the changed chunks.
- **End-to-End Encryption**: TLS 1.3 with device certificates. Not even we can read your files (there is no "we").
- **NAT Traversal**: STUN for address discovery, UDP hole punching, relay fallback. Works behind most firewalls.
- **Conflict Resolution**: Last-writer-wins with vector clocks. Conflicts create `.conflict` files for manual resolution.

## Quick Start

```bash
# Initialize your device
nexus init

# Add a folder to sync
nexus add ~/Documents

# Share with another device
nexus device id  # prints your device ID

# On the other device
nexus init
nexus device add XXXXX-XXXXX-XXXXX  # the ID from above
nexus add ~/Documents

# Start syncing
nexus serve
```

## How It Works

### Chunking

Files are split into variable-size chunks (4KB-64KB) using content-defined boundaries. This means:
- Inserting data in the middle of a file only affects nearby chunks
- Similar files share most of their chunks
- Deduplication happens automatically

### Sync Protocol

1. Devices exchange folder indexes (file paths, sizes, block hashes)
2. Each device identifies which blocks it needs
3. Missing blocks are requested from peers who have them
4. Blocks are verified by hash before being written
5. File is reconstructed from blocks

### Security

- Each device has an Ed25519 identity key
- Device IDs are the public key hash
- All connections authenticated via TLS 1.3
- Session keys derived via X25519 ECDH
- Data encrypted with ChaCha20-Poly1305

## CLI Reference

```
nexus init                    Initialize device
nexus add <path>              Add folder to sync
nexus remove <folder-id>      Remove folder
nexus device list             List known devices
nexus device add <id>         Add peer device
nexus device remove <id>      Remove device
nexus device id               Show this device's ID
nexus status                  Show sync status
nexus sync                    Force sync now
nexus serve                   Run sync daemon
```

## Configuration

Config lives in `~/.config/nexus/config.toml`:

```toml
[device]
name = "my-laptop"

[[folders]]
id = "default"
path = "/home/user/Sync"
ignore = [".git", "node_modules", "*.tmp"]

[[devices]]
id = "XXXXXXX"
name = "my-phone"
```

## Building

```bash
cargo build --release
```

## License

MIT

---

*Sync locally. Think globally. Trust nobody's servers.*
