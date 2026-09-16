# Changelog

All notable changes to this project will be documented in this file.

## 0.1.0 - 2026-09-16

First milestone release of the RustFS MCP server.

### Dependencies

- Upgrade rmcp to 3.4.0 and adapt to the renamed `ServerConfig` API (previously `ServerInfo`)
- Upgrade aws-sdk-s3 to 1.146.1 and aws-smithy-http-client to 1.4.2
- Refresh the full dependency set (tokio, clap, serde, schemars, tracing, anyhow, mime_guess)

### CI & Infrastructure

- Refresh GitHub Actions workflows (actions/checkout@v7)
- Update the Docker builder image to rust:1.98.1-bookworm

### Documentation

- Document the `create_bucket` and `delete_bucket` tools in the feature list
- Document the `--force-path-style` command-line option
- Update the minimum supported Rust version to 1.85 (edition 2024)
- Fix formatting of the `delete_bucket` tool reference and minor typos
- Add this changelog

## 0.0.8 - 2026-07-13

- Dependency refresh release.

## 0.0.7 - 2026-05-27

- Dependency refresh release.

## 0.0.6 - 2026-04-17

- Dependency refresh release.
