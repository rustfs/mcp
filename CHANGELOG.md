# Changelog

All notable changes to this project will be documented in this file.

## 0.2.0 - 2026-11-23

Feature release expanding the MCP tool surface with full object CRUD support.

### Added

- `delete_object` tool: permanently delete a single object, optionally by version ID
- `delete_objects` tool: batch-delete up to 1,000 objects in one request, with per-key error reporting
- `copy_object` tool: server-side copy within or across buckets, with optional content type, storage class, and cache control overrides
- `head_object` tool: retrieve object metadata (size, ETag, content type, storage class, version, user metadata) without downloading the content
- GitHub Release automation: the Publish Crate workflow now creates a GitHub Release (notes extracted from this changelog) after a successful crates.io publish on `v*` tags

### Changed

- Renamed the request structs `CreateBucketReqeust` → `CreateBucketRequest` and `DeleteBucketReqeust` → `DeleteBucketRequest` (typo fix in the library's public API)

### Dependencies

- Upgrade rmcp to 3.11.0 and aws-sdk-s3 to 1.161.0
- Refresh the full dependency set (aws-smithy-http-client, tokio, clap, serde, schemars, tracing, anyhow)

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
