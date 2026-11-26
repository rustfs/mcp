# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

### Added

- ACL tools: `get_bucket_acl` / `put_bucket_acl` and `get_object_acl` / `put_object_acl` read the access control policy of a bucket or object version and apply canned ACLs (`private`, `public-read`, `public-read-write`, `authenticated-read`, and for objects `bucket-owner-read` and `bucket-owner-full-control`), where the read-back must match the ACL's full expected policy — the owner's FULL_CONTROL, exactly the ACL's group grants, nothing extra — and the bucket-owner ACLs are additionally verified against the bucket owner's canonical ID, staying unconfirmed when the backend does not report it
- `put_bucket_acl` and `put_object_acl` read the ACL back after setting it and report an unconfirmed result when the storage backend does not show the requested access, instead of claiming success on servers that accept ACL requests without applying them (RustFS 1.0.6 does)
- Bucket notification tools: `get_bucket_notification` (readable summary plus JSON that round-trips back into `put_bucket_notification`), `put_bucket_notification` (rules that name a Lambda function, queue, or topic by ARN, the S3 events, and an optional key prefix/suffix filter, validated locally first: event names must have S3's shape and key filters are given literally and encoded for the wire, where a literal `+` travels as `%2B`), and `delete_bucket_notification`
- `get_bucket_notification` refuses buckets that also have an EventBridge notification configuration instead of showing rules whose round-trip would disable EventBridge
- `delete_bucket_notification` clears the configuration with an empty `put_bucket_notification_configuration` request rather than S3's own DELETE operation, which some S3-compatible servers do not support (RustFS 1.0.6 answers it by deleting the bucket)
- `get_object_attributes` tool: read the ETag, the checksums recorded for the object, the part details reported for it, storage class, and size of an object version without downloading its content; the parts list follows S3's pagination markers (pinned to the version the first page read, so concurrent overwrites cannot mix versions into one answer) and objects with more than 1,000 parts report every part
- Static website tools: `get_bucket_website` (readable summary plus JSON that round-trips back into `put_bucket_website`), `put_bucket_website` (index document, error document, a redirect of all requests, and routing rules, validated locally first — an empty `key_prefix_equals` is accepted as the match-all prefix S3 defines), and `delete_bucket_website`
- Server access logging tools: `get_bucket_logging`, `put_bucket_logging` (log target bucket and optional key prefix, validated locally first), and `delete_bucket_logging`
- `get_bucket_logging` reports settings it does not model (log-object access grants, a partitioned log-object key format) as a warning instead of failing the whole read, and accepts the default `SimplePrefix` naming; `put_bucket_logging` refuses buckets whose current configuration carries such settings, since its request would drop them
- `delete_bucket_logging` clears the configuration with an empty `put_bucket_logging` request rather than S3's own DELETE operation, which some S3-compatible servers do not support (RustFS 1.0.6 answers it by deleting the bucket)
- Object Lock tools: `create_bucket` new `object_lock_enabled` option creates the bucket with Object Lock (and versioning) on; `get_bucket_lock_configuration` / `put_bucket_lock_configuration` read, set, and remove the default retention period (`GOVERNANCE` / `COMPLIANCE`, in days or years), with `put_bucket_lock_configuration` verifying that Object Lock is enabled before the update (on AWS the request would irreversibly enable Object Lock and permanently enable versioning for a bucket without it); `get_object_retention` / `put_object_retention` read and set the retention period of one object version, optionally shortening a GOVERNANCE retention with a bypass; `get_object_legal_hold` / `put_object_legal_hold` read and set the legal hold of one object version
- `head_object` reports the retention mode, retain-until date, and legal hold status of a version when the storage backend sends them
- Unset states are reported as such instead of as failures: a bucket without Object Lock, and versions without a retention period or legal hold, whether the storage backend answers with S3's `NoSuchObjectLockConfiguration` error or with an empty setting

### Documentation

- Add a License section to the README: the Apache License 2.0 with a link to the LICENSE file, the copyright notice, and the trademark note

## 0.8.0 - 2026-11-25

Eight tool modules round out the S3 surface, and every tool now carries MCP annotations clients can use to decide which calls need confirmation.

### Added

- `upload_content` tool: store text held in memory - notes, JSON, code - as an object without creating a local file; content type is detected from the object key extension (falling back to text/plain), all `upload_file` options are supported, and inline payloads are capped at 5 MB
- Versioning tools: `get_bucket_versioning` and `put_bucket_versioning` inspect and change a bucket's versioning state (enabled / suspended); `list_object_versions` lists every version and delete marker newest first per key, with prefix/delimiter filtering and key/version marker pagination; `delete_object_versions` permanently deletes up to 1,000 specific versions per batch with per-version error reporting
- `copy_object` tool new `source_version_id` parameter: copy a specific version, e.g. to roll back an object by copying an older version over the current key; metadata overrides read the source version's headers, and the copy-source header URL-encodes the version ID
- Tagging tools: `get_object_tagging` / `put_object_tagging` / `delete_object_tagging` and `get_bucket_tagging` / `put_bucket_tagging` / `delete_bucket_tagging` manage tag sets per object version and per bucket; tag sets are validated locally (count, key/value lengths in characters) and output is sorted by key
- Bucket inspection and policy tools: `head_bucket` (existence and accessibility, distinguishing a missing bucket from a denied one), `get_bucket_location`, and `get_bucket_policy` / `put_bucket_policy` / `delete_bucket_policy` with the policy passed as a JSON object and pretty-printed when read back
- Lifecycle tools: `get_bucket_lifecycle` (readable summary plus JSON that round-trips back into `put_bucket_lifecycle`), `put_bucket_lifecycle` (typed rules for expiration, storage-class transitions, noncurrent version actions, incomplete multipart cleanup, and prefix/tag/size filters, validated locally with clear per-rule messages), and `delete_bucket_lifecycle`
- Bucket CORS tools: `get_bucket_cors` / `put_bucket_cors` / `delete_bucket_cors` with typed rules validated locally (allowed methods, origins, max age range)
- Bucket encryption tools: `get_bucket_encryption` / `put_bucket_encryption` / `delete_bucket_encryption` for the default server-side encryption (`AES256`, `aws:kms`, `aws:kms:dsse` with optional KMS key and Bucket Key), validated locally
- Multipart management tools: `list_multipart_uploads` (in-progress uploads with prefix/delimiter filtering and marker pagination) and `upload_part_copy` (build a part by copying an object or byte range server-side, optionally from a specific version, with local range validation and percent-encoded copy-source headers)
- MCP tool annotations on every tool: `readOnlyHint` for listing/inspecting tools, `destructiveHint` for overwriting and deleting tools, `idempotentHint` where repeating a call converges; `presigned_url` and `get_object` are deliberately not read-only (credential-bearing URL minting / local file writes)

### Changed

- `serde_json` moved from dev-dependencies to dependencies (policy documents and lifecycle rules render as JSON)

### Fixed

- Reading the tags or policy of a never-configured bucket or object is reported as an empty/unset state instead of an error, with bucket existence confirmed first where S3 implementations reuse the same error code for missing buckets

## 0.7.0 - 2026-11-25

Internal restructuring with an unchanged public API.

### Changed

- Split `s3_client.rs` and `server.rs` into per-method module files (`src/s3_client/`, `src/server/`); external paths `rustfs_mcp::s3_client::*` and `rustfs_mcp::server::*` are unchanged

### Dependencies

- Refresh transitive dependencies (aws-smithy-runtime-api, aws-smithy-types, clap 4.6.10, crc32fast, h2, hyper 1.13.0); no incompatible upgrades were available
- Update the Docker builder image to rust:1.100.0-bookworm

## 0.6.0 - 2026-11-25

Multipart upload workflow for large files.

### Added

- `initiate_multipart_upload` tool: start a multipart upload and get an upload ID, with optional content type, storage class, cache control, and user metadata
- `upload_part` tool: upload a local file as a single part (part numbers 1-10000) and record its ETag
- `complete_multipart_upload` tool: assemble the uploaded parts (part number + ETag pairs, sorted automatically)
- `abort_multipart_upload` tool: cancel a multipart upload and discard its parts
- `list_parts` tool: inspect uploaded parts and their ETags
- `upload_file` tool new options: `metadata` (user metadata), `server_side_encryption`, `content_disposition`, `content_encoding`, `content_language`

### Fixed

- `upload_part` and `upload_file` stream files from disk instead of buffering them fully in memory, so multi-GiB parts no longer exhaust server memory
- `list_parts` follows S3 pagination markers, so uploads with more than 1,000 parts report the complete inventory instead of only the first page
- `initiate_multipart_upload` applies all requested upload options, including server-side encryption and content headers

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
