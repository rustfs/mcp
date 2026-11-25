# Changelog

All notable changes to this project will be documented in this file.

## 0.6.0 - 2026-11-25

Multipart upload workflow for large files.

### Added

- `initiate_multipart_upload` tool: start a multipart upload and get an upload ID, with optional content type, storage class, cache control, and user metadata
- `upload_part` tool: upload a local file as a single part (part numbers 1-10000) and record its ETag
- `complete_multipart_upload` tool: assemble the uploaded parts (part number + ETag pairs, sorted automatically)
- `abort_multipart_upload` tool: cancel a multipart upload and discard its parts
- `list_parts` tool: inspect uploaded parts and their ETags
- `upload_file` tool new options: `metadata` (user metadata), `server_side_encryption`, `content_disposition`, `content_encoding`, `content_language`

## 0.5.0 - 2026-11-24

Presigned URLs for credential-free temporary access.

### Added

- `presigned_url` tool: generate time-limited SigV4 presigned URLs for objects, with a `method` parameter granting `get` (download) or `put` (upload) access and `expires_in_secs` validity (clamped to 1-604800 seconds, i.e. the 7-day SigV4 maximum; default 3600)
- Output includes a ready-to-run `curl` example and a sharing caution; presigned URLs are never logged

## 0.4.0 - 2026-11-24

Range and conditional reads for `get_object`.

### Added

- `get_object` tool `range` parameter for partial reads (e.g. `bytes=0-1023`), supported in both read and download modes; partial reads are labeled in the output
- `get_object` tool conditional read parameters `if_modified_since` and `if_unmodified_since` (ISO 8601 timestamps)
- Error guidance for out-of-bounds ranges and failed conditional reads

### Fixed

- `if_unmodified_since` is now sent to S3: the option existed in the data layer but was never applied to read or download requests, so the precondition was silently ignored

## 0.3.0 - 2026-11-24

Pagination and directory browsing for object listings.

### Added

- `list_objects` tool pagination parameters: `max_keys` (1-1000 per page), `continuation_token` (resume from a truncated response), and `start_after` (begin listing after a key)
- `list_objects` tool `delimiter` parameter for directory-style browsing; matching common prefixes are reported as Directories
- Truncated responses now include the continuation token with usage guidance for fetching the next page

## 0.2.0 - 2026-11-23

Feature release expanding the MCP tool surface with full object CRUD support.

### Added

- `delete_object` tool: permanently delete a single object, optionally by version ID
- `delete_objects` tool: batch-delete up to 1,000 objects in one request, with per-key error reporting
- `copy_object` tool: server-side copy within or across buckets, with optional content type, storage class, and cache control overrides (source headers and user metadata are preserved unless overridden; storage class changes do not affect metadata)
- `head_object` tool: retrieve object metadata (size, ETag, content type, storage class, version, user metadata) without downloading the content
- GitHub Release automation: the Publish Crate workflow now creates a GitHub Release (notes extracted from this changelog) after a successful crates.io publish on `v*` tags

### Changed

- Renamed the request structs `CreateBucketReqeust` → `CreateBucketRequest` and `DeleteBucketReqeust` → `DeleteBucketRequest` (typo fix in the library's public API)

### Documentation

- Refresh the README badges: point the CI badge at this repository and add crates.io version, docs.rs, license, and download badges

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
