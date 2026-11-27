[![RustFS](https://rustfs.com/images/rustfs-github.png)](https://rustfs.com)

# RustFS MCP Server - Model Context Protocol

<p align="center">
  <strong>High-performance MCP server providing S3-compatible object storage operations for AI/LLM integration</strong>
</p>

<p align="center">
  <a href="https://github.com/rustfs/mcp/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/rustfs/mcp/actions/workflows/ci.yml/badge.svg" /></a>
  <a href="https://crates.io/crates/rustfs-mcp"><img alt="crates.io" src="https://img.shields.io/crates/v/rustfs-mcp.svg" /></a>
  <a href="https://docs.rs/rustfs-mcp"><img alt="docs.rs" src="https://docs.rs/rustfs-mcp/badge.svg" /></a>
  <a href="./LICENSE"><img alt="License" src="https://img.shields.io/crates/l/rustfs-mcp" /></a>
  <a href="https://crates.io/crates/rustfs-mcp"><img alt="Downloads" src="https://img.shields.io/crates/d/rustfs-mcp" /></a>
</p>
<p align="center">
  <a href="https://docs.rustfs.com/">📖 Documentation</a>
  <a href="https://github.com/rustfs/rustfs/issues">🐛 Bug Reports</a>
  <a href="https://github.com/rustfs/rustfs/discussions">💬 Discussions</a>
</p>

---

## 📖 Overview

**RustFS MCP Server** is a high-performance [Model Context Protocol (MCP)](https://spec.modelcontextprotocol.org) server that provides AI/LLM tools with seamless access to S3-compatible object storage operations. Built with Rust for maximum performance and safety, it enables AI assistants like Claude Desktop to interact with cloud storage through a standardized protocol.

### What is MCP?

The Model Context Protocol is an open standard that enables secure, controlled connections between AI applications and external systems. This server acts as a bridge between AI tools and S3-compatible storage services, providing structured access to file operations while maintaining security and observability.

## ✨ Features

### Supported S3 Operations

- **List Buckets**: List all accessible S3 buckets
- **List Objects**: Browse bucket contents with optional prefix filtering
- **Bucket Lifecycle**: Inspect, set, and remove rules that expire objects, change their storage class, or clean up old versions
- **Bucket Replication**: Inspect, set, and remove rules that copy object versions to another bucket, selected by prefix and tags
- **Bucket Logging**: Inspect, enable, and disable server access logging of a bucket
- **Upload Files**: Upload local files with automatic MIME type detection and cache control
- **Upload Content**: Write text, JSON, or code straight to S3 without creating a local file
- **Get Objects**: Retrieve objects from S3 storage with read or download modes
- **Bucket Encryption**: Inspect, set, and remove the default server-side encryption of a bucket
- **Create Bucket**: Create a new S3 bucket with the specified name
- **Bucket CORS**: Inspect, set, and remove the cross-origin resource sharing rules of a bucket
- **Bucket Notifications**: Inspect, set, and remove the rules that deliver object events to Lambda functions, queues, topics, or webhook targets
- **Delete Bucket**: Delete an existing, empty S3 bucket
- **Bucket Inspection & Policy**: Check bucket access and region, and read, set, and remove bucket policies
- **Request Payment**: Inspect and change who pays for the requests to a bucket — the bucket owner or the requester
- **Delete Objects**: Permanently delete a single object or batch-delete up to 1,000 objects
- **ACLs**: Read the access control list of buckets and object versions, and apply canned ACLs, confirmed by reading the ACL back
- **Tagging**: Read, set, and remove bucket tags and object tags
- **Copy Objects**: Copy objects server-side within or across buckets
- **Head Objects**: Inspect object metadata without downloading the content
- **Object Attributes**: Read the ETag, recorded checksums, and parts of an object version without downloading it
- **Versioning**: Enable, suspend, and inspect bucket versioning; list, delete, and copy object versions
- **Object Lock**: Create lock-enabled buckets, set a default retention period, and read, set, and remove per-version retention periods and legal holds
- **Multipart Upload**: Upload large files in parts with initiate / upload_part / complete / abort / list_parts tools
- **Multipart Upload Management**: List in-progress multipart uploads and build parts by copying existing objects (list_multipart_uploads / upload_part_copy)
- **Static Website**: Inspect, set, and remove the website configuration of a bucket (index document, error document, redirects)

## 🔧 Installation

### Prerequisites

- Rust 1.85+ (for building from source, Rust edition 2024)
- AWS credentials configured (via environment variables, AWS CLI, or IAM roles)
- Access to S3-compatible storage service

### Build from Source

```bash
# Clone the repository
git clone https://github.com/rustfs/mcp.git
cd mcp

# Build the MCP server
cargo build --release -p rustfs-mcp

# The binary will be available at
./target/release/rustfs-mcp
```

### Optional: Enable io-uring

`io-uring` is enabled by default in this crate. Tokio requires unstable cfg for this mode:

```bash
RUSTFLAGS="--cfg tokio_unstable" cargo build --release -p rustfs-mcp
```

To explicitly disable it:

```bash
cargo build --release -p rustfs-mcp --no-default-features
```

## ⚙️ Configuration

### Environment Variables

```bash
# AWS Credentials (required)
export AWS_ACCESS_KEY_ID=your_access_key
export AWS_SECRET_ACCESS_KEY=your_secret_key
export AWS_REGION=us-east-1  # Optional, defaults to us-east-1

# Optional: Custom S3 endpoint (for MinIO, etc.)
export AWS_ENDPOINT_URL=http://localhost:9000

# Logging level (optional)
export RUST_LOG=info
```

### Command Line Options

```bash
rustfs-mcp --help
```

The server supports various command-line options for customizing behavior:

- `--access-key-id`: AWS Access Key ID for S3 authentication
- `--secret-access-key`: AWS Secret Access Key for S3 authentication
- `--region`: AWS region to use for S3 operations (default: us-east-1)
- `--endpoint-url`: Custom S3 endpoint URL (for MinIO, LocalStack, etc.)
- `--force-path-style`: Force path-style S3 addressing (automatically enabled for custom endpoints)
- `--log-level`: Log level configuration (default: rustfs_mcp_server=info)

## 🚀 Usage

### Starting the Server

```bash
# Start the MCP server
rustfs-mcp

# Or with custom options
rustfs-mcp --log-level debug --region us-west-2
```

### Integration with chat client

#### Option 1: Using Command Line Arguments

```json
{
  "mcpServers": {
    "rustfs-mcp": {
      "command": "/path/to/rustfs-mcp",
      "args": [
        "--endpoint-url", "your_url",
        "--access-key-id", "your_access_key",
        "--secret-access-key", "your_secret_key",
        "--region", "us-west-2",
        "--log-level", "info"
      ]
    }
  }
}
```

#### Option 2: Using Environment Variables

```json
{
  "mcpServers": {
    "rustfs-mcp": {
      "command": "/path/to/rustfs-mcp",
      "env": {
        "AWS_ENDPOINT_URL": "your_url",
        "AWS_ACCESS_KEY_ID": "your_access_key",
        "AWS_SECRET_ACCESS_KEY": "your_secret_key",
        "AWS_REGION": "us-east-1"
      }
    }
  }
}
```

### Using MCP with Docker

#### Docker image build

Using MCP with docker will simplify the usage of rustfs mcp. Building the docker image with below command:

```
docker build -f Dockerfile -t rustfs/rustfs-mcp ../../
```

Alternatively, if you want to build the image from the rustfs codebase root directory, run the command:

```
docker build -f crates/mcp/Dockerfile -t rustfs/rustfs-mcp .
```

#### IDE Configuration

Adding the following content in IDE MCP settings:

```
{
  "mcpServers": {
    "rustfs-mcp": {
      "command": "docker",
      "args": [
        "run",
        "--rm",
        "-i",
        "-e",
        "AWS_ACCESS_KEY_ID",
        "-e",
        "AWS_SECRET_ACCESS_KEY",
        "-e",
        "AWS_REGION",
        "-e",
        "AWS_ENDPOINT_URL",
        "rustfs/rustfs-mcp"
      ],
      "env": {
        "AWS_ACCESS_KEY_ID": "rustfs_access_key",
        "AWS_SECRET_ACCESS_KEY": "rustfs_secret_key",
        "AWS_REGION": "cn-east-1",
        "AWS_ENDPOINT_URL": "rustfs_instance_url"
      }
    }
  }
}
```

If success, MCP configure page will show the [available tools](#️-available-tools).

## 🛠️ Available Tools

The MCP server exposes the following tools that AI assistants can use:

Every tool carries MCP tool annotations that clients can use to decide which calls need confirmation. Tools that only read (`list_*`, `head_*`) set `readOnlyHint`. Tools that overwrite or delete data set `destructiveHint`, and tools that can be repeated safely set `idempotentHint`. Upload, copy, and delete tools do not claim idempotency: with bucket versioning enabled, a repeated call adds another object version or delete marker. `get_object` is deliberately not marked read-only because its download mode writes to the local file system.

### `list_buckets`

List all S3 buckets accessible with the configured credentials.

**Parameters:** None

### `get_bucket_lifecycle`

Get the lifecycle rules of a bucket: when objects expire, move to another storage class, or get cleaned up. Returns a readable summary and the rules as JSON, which can be edited and sent back through `put_bucket_lifecycle`.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_lifecycle`

Set the lifecycle rules of a bucket, replacing the current configuration.

**WARNING: Expiration actions permanently delete objects — review the rules before applying them!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `rules` (array): The complete lifecycle configuration (1-1000 rules). Each rule has:
  - `id` (string, optional): Unique rule identifier (max 255 characters)
  - `status` (string): `"enabled"` or `"disabled"`
  - `prefix` (string, optional): Key prefix the rule applies to; omit it to match the whole bucket
  - `tags` (object, optional): Tags the object must carry (all of them)
  - `object_size_greater_than` / `object_size_less_than` (number, optional): Size filter in bytes
  - `expiration` (object, optional): Exactly one of `days`, `date` (ISO 8601), or `expired_object_delete_marker` (true)
  - `transitions` (array, optional): Each with a `storage_class` and exactly one of `days` or `date`
  - `noncurrent_version_transitions` (array, optional): Each with `days` (at least 1), `storage_class`, and an optional `newer_noncurrent_versions`
  - `noncurrent_version_expiration` (object, optional): `days` (at least 1), `newer_noncurrent_versions`, or both
  - `abort_incomplete_multipart_upload_after_days` (number, optional): Discard incomplete multipart uploads after this many days

  Every rule needs at least one action.

### `delete_bucket_lifecycle`

Remove every lifecycle rule from a bucket. Objects are then kept until they are deleted by hand.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `get_bucket_replication`

Get the replication configuration of a bucket: the role used for copying — an IAM role on AWS S3, the ARN of a replication target configured on the server on MinIO and RustFS — and the rules deciding which object versions are copied to which destination bucket. Returns a readable summary and the rules as JSON, which can be edited and sent back through `put_bucket_replication`. Settings the tool does not model (destination account, access control translation, replication metrics, replication time control, encryption configuration, source selection criteria) are listed as a warning, and `put_bucket_replication` refuses to replace such a configuration, because its request would drop them. A bucket without replication is reported as such instead of as a failure (AWS S3, MinIO, and RustFS answer `ReplicationConfigurationNotFoundError`).

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_replication`

Set the replication rules of a bucket, replacing the current configuration. Rules copy new object versions to another bucket, selected by prefix and tags.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket to replicate from
- `role` (string): ARN of an IAM role on AWS S3, e.g. `arn:aws:iam::123456789012:role/replicator`; on MinIO and RustFS the ARN of a replication target configured on the server, e.g. `arn:rustfs:replication::<deployment-id>:<bucket>` — both answer `InvalidRequest` for anything else
- `rules` (array): The complete replication configuration (1-1000 rules). Each rule has:
  - `id` (string, optional): Unique rule identifier (max 255 characters)
  - `status` (string): `"enabled"` or `"disabled"`
  - `priority` (number, optional): Precedence among rules whose filters overlap; a rule without one counts as 0, and priorities must be unique (MinIO and RustFS refuse the configuration otherwise)
  - `prefix` (string, optional): Key prefix the rule applies to; omit it to match the whole bucket
  - `tags` (object, optional): Tags the object must carry (all of them)
  - `delete_marker_replication` (boolean, optional): Whether deleting a source object also places a delete marker on the copy; MinIO and RustFS refuse it on a rule that selects by a single tag
  - `existing_object_replication` (boolean, optional): Whether object versions that already exist are copied once when the rule is created
  - `destination` (object): `bucket` (a name or an ARN such as `arn:aws:s3:::backup` — AWS S3 requires the ARN form; it must differ from the source bucket) and an optional `storage_class` (AWS S3 storage classes; RustFS keeps `STANDARD` copies only and refuses any other class)

  The bucket needs versioning enabled (`put_bucket_versioning`), which MinIO and RustFS check before they accept replication, and rules without `existing_object_replication` only copy object versions created after the rule. RustFS also refuses the settings listed under `get_bucket_replication`.

### `delete_bucket_replication`

Remove every replication rule written through S3 from a bucket, then read the configuration back and report what the storage backend kept. On AWS S3 and MinIO nothing is left: new object versions are then kept in this bucket only, and copies that already reached the destination buckets stay there. On RustFS the rules a site replication manages survive the request and stay in effect, and the tool reports them instead of claiming that the bucket stopped replicating.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `get_bucket_logging`

Get the server access logging configuration of a bucket: the log target bucket requests are recorded to, and the key prefix they are stored under. Settings the tool does not model (log-object access grants, a partitioned log-object key format) are reported as a warning rather than failing the read.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_logging`

Enable server access logging for a bucket: its requests are then recorded as objects in a log target bucket. The request replaces the whole configuration, so a bucket whose current configuration holds settings the tool does not model (log-object access grants, a partitioned log-object key format) is refused — manage such a bucket with the storage backend's own tooling, or remove its configuration first (`delete_bucket_logging`).

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket whose requests are recorded
- `target_bucket` (string): Bucket the access logs are written to; it should exist and allow the logging service to write to it
- `target_prefix` (string, optional): Prefix of the keys the logs are stored under, e.g. `logs/` (default: the root of the target bucket)

On AWS, the target bucket needs a bucket policy that lets the S3 logging service write to it (`put_bucket_policy`). Storage backends differ in whether they deliver the logs: RustFS 1.0.6 stores the configuration but does not write log objects.

### `delete_bucket_logging`

Disable server access logging for a bucket. Logs that were already written stay in the log target bucket.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

The configuration is cleared with an empty `put_bucket_logging` request instead of S3's own DELETE operation, which some S3-compatible servers do not support.

### `list_objects`

List objects in an S3 bucket with optional prefix filtering, directory-style browsing, and pagination.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `prefix` (string, optional): Prefix to filter objects
- `delimiter` (string, optional): Character to group keys by (e.g. `/` for directory browsing; matching prefixes are returned as Directories)
- `max_keys` (number, optional): Maximum keys per page, 1-1000 (default: 1000)
- `continuation_token` (string, optional): Token from a truncated response to fetch the next page
- `start_after` (string, optional): Key to start listing after (ignored when a continuation token is provided)

### `get_bucket_versioning`

Get the versioning configuration of a bucket: `Enabled`, `Suspended`, or never enabled.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_versioning`

Enable or suspend versioning for a bucket. Suspending keeps the existing versions; it only stops new ones from being created.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `status` (string): Versioning state to set — `"enabled"` or `"suspended"`

### `list_object_versions`

List every version and delete marker of the objects in a bucket, newest first per key. Returns the version IDs used by `get_object`, `copy_object`, and `delete_object_versions`.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `prefix` (string, optional): Prefix to filter keys
- `delimiter` (string, optional): Character to group keys by (e.g. `/` for directory browsing; matching prefixes are returned as Directories)
- `max_keys` (number, optional): Maximum entries (versions + delete markers) per page, 1-1000 (default: 1000)
- `key_marker` (string, optional): Key marker from a truncated response to fetch the next page
- `version_id_marker` (string, optional): Version ID marker from a truncated response (used together with `key_marker`)

### `delete_object_versions`

Permanently delete specific object versions or delete markers (up to 1,000 per request) in a single batch call, reporting per-version failures.

**WARNING: This operation cannot be undone!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `versions` (array): Object versions to delete, each with a `key` and a `version_id` from `list_object_versions` (max 1000)

### `get_bucket_lock_configuration`

Get the Object Lock configuration of a bucket: whether Object Lock is enabled and which retention period new object versions receive by default. A bucket without Object Lock is reported as not enabled rather than as a failure: AWS S3, MinIO, and RustFS all answer its absence with `ObjectLockConfigurationNotFoundError`, and the tool first confirms that the bucket exists, because some storage backends give the same answer for a bucket that does not exist.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_lock_configuration`

Set or remove the default retention period that every new object version of a bucket receives. The bucket must have Object Lock enabled, which the tool verifies before sending the update — on AWS, a request for a bucket without it would irreversibly enable Object Lock and permanently enable versioning (`create_bucket` with `object_lock_enabled` turns it on).

**WARNING: Retained versions cannot be deleted or overwritten until their retention expires — a COMPLIANCE retention cannot be shortened or removed at all!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `mode` (string, optional): Retention mode — `"GOVERNANCE"` (users with `s3:BypassGovernanceRetention` can shorten it) or `"COMPLIANCE"` (cannot be shortened or removed until it expires)
- `days` (number, optional): Retention period in days (at least 1); set exactly one of `days` and `years`
- `years` (number, optional): Retention period in years (at least 1); set exactly one of `days` and `years`

To set a default retention, pass `mode` together with exactly one of `days` and `years`. Omit all three parameters to remove the default retention again: Object Lock stays enabled for the bucket, and versions that already carry a retention period keep it.

### `get_object_retention`

Get the retention period of an object version: the mode and the moment until which the version cannot be deleted or overwritten. "Nothing applies" is reported as such rather than as a failure, whichever way the storage backend says it: a version without a retention period (`NoSuchObjectLockConfiguration` on AWS S3, MinIO, and RustFS, or an empty answer on older RustFS), or a bucket with Object Lock disabled (`InvalidRequest` on all three, each with its own wording). For the latter the tool reads the bucket's Object Lock configuration to confirm it, and the note of the read says whether the version or the bucket is the reason. A missing object, version, or bucket stays an error.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `version_id` (string, optional): Version ID to read the retention of a specific version (default: the current version)

### `put_object_retention`

Set the retention period of an object version until a given moment. Until then the version cannot be deleted or overwritten.

**WARNING: A COMPLIANCE retention cannot be shortened or removed until it expires — not even by the root account!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `mode` (string): Retention mode — `"GOVERNANCE"` (users with `s3:BypassGovernanceRetention` can shorten it) or `"COMPLIANCE"` (cannot be shortened or removed until it expires)
- `retain_until` (string): Moment until which the version stays retained, as an ISO 8601 UTC timestamp (e.g. `2030-01-01T00:00:00Z`); must lie in the future
- `bypass_governance_retention` (boolean, optional): Allow shortening an existing GOVERNANCE retention; requires the `s3:BypassGovernanceRetention` permission (default: false)
- `version_id` (string, optional): Version ID to set the retention of a specific version (default: the current version)

The bucket needs Object Lock enabled (`create_bucket` with `object_lock_enabled`), and the retention belongs to a single object version: overwriting the object creates a new version with its own retention.

### `get_object_legal_hold`

Get the legal hold status of an object version. A version without a legal hold, or a bucket with Object Lock disabled, is reported as no legal hold rather than as a failure, and the note of the read says which of the two it is (see `get_object_retention` for how the backends answer each case). An explicit `OFF` stays distinguishable from no legal hold.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `version_id` (string, optional): Version ID to read the legal hold of a specific version (default: the current version)

### `put_object_legal_hold`

Set or remove the legal hold of an object version. While the hold is on, the version cannot be deleted or overwritten.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `status` (string): Legal hold status — `"ON"` or `"OFF"`
- `version_id` (string, optional): Version ID to set the legal hold of a specific version (default: the current version)

The bucket needs Object Lock enabled (`create_bucket` with `object_lock_enabled`), and the hold belongs to a single object version.

### `upload_file`

Upload a local file to S3 with automatic MIME type detection.

**Parameters:**

- `local_file_path` (string): Path to the local file
- `bucket_name` (string): Target S3 bucket
- `object_key` (string): S3 object key (destination path)
- `content_type` (string, optional): Content type (auto-detected if not provided)
- `storage_class` (string, optional): S3 storage class
- `cache_control` (string, optional): Cache control header
- `metadata` (object, optional): User metadata key-value pairs (x-amz-meta-*)
- `server_side_encryption` (string, optional): Server-side encryption (e.g. AES256, aws:kms)
- `content_disposition` (string, optional): Content disposition header
- `content_encoding` (string, optional): Content encoding header (e.g. gzip)
- `content_language` (string, optional): Content language header (e.g. en-US)

### `upload_content`

Upload text content held in memory — notes, JSON, code — straight to S3 without creating a local file. Creates or replaces the object. The content type is detected from the object key's extension and falls back to `text/plain`.

**Parameters:**

- `content` (string): Text to store as the object body (up to 5 MB)
- `bucket_name` (string): Target S3 bucket
- `object_key` (string): S3 object key (destination path)
- `content_type` (string, optional): Content type (detected from the object key extension, falling back to `text/plain`)
- `storage_class` (string, optional): S3 storage class
- `cache_control` (string, optional): Cache control header
- `metadata` (object, optional): User metadata key-value pairs (x-amz-meta-*)
- `server_side_encryption` (string, optional): Server-side encryption (e.g. AES256, aws:kms)
- `content_disposition` (string, optional): Content disposition header
- `content_encoding` (string, optional): Content encoding header (e.g. gzip)
- `content_language` (string, optional): Content language header (e.g. en-US)

Use `upload_file` for content larger than 5 MB or for binary data.

### `get_object`

Retrieve an object from S3 with two operation modes: read content directly or download to a file. Supports byte-range partial reads and conditional reads.

**Parameters:**

- `bucket_name` (string): Source S3 bucket
- `object_key` (string): S3 object key
- `version_id` (string, optional): Version ID for versioned objects
- `range` (string, optional): Byte range to read, e.g. `bytes=0-1023` (partial read, both modes)
- `if_modified_since` (string, optional): ISO 8601 timestamp; succeeds only if the object was modified after this time
- `if_unmodified_since` (string, optional): ISO 8601 timestamp; succeeds only if the object was not modified after this time
- `mode` (string, optional): Operation mode - "read" (default) returns content directly, "download" saves to local file
- `local_path` (string, optional): Local file path (required when mode is "download")
- `max_content_size` (number, optional): Maximum content size in bytes for read mode (default: 1MB)

### `create_bucket`

Create a new S3 bucket with the specified name, optionally with Object Lock enabled.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket to create
- `object_lock_enabled` (boolean, optional): Create the bucket with Object Lock enabled, which also turns on versioning. Object Lock can only be enabled when the bucket is created and cannot be turned off afterwards (default: false)

### `get_bucket_encryption`

Get the default server-side encryption configuration of a bucket: the SSE algorithm, the KMS key, and the Bucket Key setting of each rule.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_encryption`

Set the default server-side encryption of a bucket by replacing its configuration with a single rule. The rule applies to new objects only; existing objects are not re-encrypted.

**WARNING: This replaces the existing encryption configuration; the previous rule is not kept!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `sse_algorithm` (string): Server-side encryption algorithm - `"AES256"` (SSE-S3), `"aws:kms"` (SSE-KMS), or `"aws:kms:dsse"` (exact spelling, case-sensitive)
- `kms_key_id` (string, optional): KMS key ID, ARN, or alias (only used with `aws:kms` and `aws:kms:dsse`)
- `bucket_key_enabled` (boolean, optional): Bucket Key setting, which reduces KMS request costs (only supported with `aws:kms`; S3 does not apply Bucket Keys to `AES256` or `aws:kms:dsse`)

### `delete_bucket_encryption`

Remove the default server-side encryption configuration of a bucket. Subsequent writes follow the backend's default encryption behavior — on Amazon S3, removing the configuration resets the default to SSE-S3.

**WARNING: This operation cannot be undone!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `delete_bucket`

Delete an existing S3 bucket with the specified name. If the bucket is not empty, the deletion will fail — delete all objects (and their versions) in the bucket before calling this tool.

**WARNING: This operation will permanently delete the bucket!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket to delete

### `head_bucket`

Check that a bucket exists and is accessible with the configured credentials. Reports the bucket's region when the storage backend provides it.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket to check

### `get_bucket_location`

Get the region a bucket lives in.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `get_bucket_policy`

Get the resource policy of a bucket as a JSON document describing who may access the bucket and its objects.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_policy`

Set the resource policy of a bucket from a JSON policy document, replacing the current policy.

**WARNING: A policy can expose a bucket or its objects to other accounts or to everyone — review it before applying!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `policy` (object): The complete bucket policy as a JSON resource policy document (`Version` + `Statement` array), passed as a JSON object rather than an escaped string

### `delete_bucket_policy`

Remove the resource policy from a bucket. Access then falls back to the bucket's remaining access controls such as bucket or object ACLs; public ACL grants are not revoked.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `get_bucket_request_payment`

Get who pays for the requests to a bucket: the bucket owner (the S3 default, reported even when the backend leaves the setting unset) or the requester.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_request_payment`

Set who pays for the requests to a bucket.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `payer` (string): `"BucketOwner"` (the default; anonymous requests stay allowed) or `"Requester"` (callers must authenticate and cover the request and data-transfer costs; anonymous access to the bucket is disabled while requester pays is set)

### `delete_object`

Permanently delete a single object from an S3 bucket, optionally a specific version for versioned objects.

**WARNING: This operation cannot be undone!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key to delete
- `version_id` (string, optional): Version ID for versioned objects

### `delete_objects`

Permanently delete multiple objects (up to 1,000 per request) from an S3 bucket in a single batch call, reporting per-key failures.

**WARNING: This operation cannot be undone!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_keys` (string[]): Object keys to delete (max 1000)

### `get_bucket_acl`

Get the access control list (ACL) of a bucket: its owner and the permissions granted to accounts, everyone, or authenticated users.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_acl`

Apply a canned ACL to a bucket, replacing its whole ACL, then read the ACL back to confirm that the storage backend applied it.

**WARNING: `public-read` and `public-read-write` make the bucket readable — or writable — by everyone on the internet!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `acl` (string): Canned ACL to apply — `"private"`, `"public-read"`, `"public-read-write"`, or `"authenticated-read"`

### `get_object_acl`

Get the access control list (ACL) of an object version: its owner and the permissions granted to accounts, everyone, or authenticated users.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `version_id` (string, optional): Version ID to read the ACL of a specific version (default: the current version)

### `put_object_acl`

Apply a canned ACL to an object version, replacing its whole ACL, then read the ACL back to confirm that the storage backend applied it.

**WARNING: `public-read` and `public-read-write` make the object readable — or writable — by everyone on the internet!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `acl` (string): Canned ACL to apply — `"private"`, `"public-read"`, `"public-read-write"`, `"authenticated-read"`, `"bucket-owner-read"`, or `"bucket-owner-full-control"`
- `version_id` (string, optional): Version ID to set the ACL of a specific version (default: the current version)

Some S3-compatible servers accept ACL requests without applying them, and access that should have been revoked may linger. `put_bucket_acl` and `put_object_acl` therefore read the ACL back afterwards and report an unconfirmed result unless the policy matches the ACL's full expectation: the owner's FULL_CONTROL, exactly the ACL's group grants, and nothing extra. The two bucket-owner ACLs are additionally verified against the bucket owner's canonical ID (read from the bucket) and stay unconfirmed when the backend does not report it. RustFS 1.0.6 is such a server: it accepts every ACL request and reports ACLs without naming who receives their permissions. Manage public access there with a bucket policy (`put_bucket_policy`) instead.

### `copy_object`

Copy an object server-side within the same bucket or across buckets, without downloading or uploading data through the client.

**Parameters:**

- `source_bucket` (string): Name of the source S3 bucket
- `source_key` (string): Source S3 object key
- `destination_bucket` (string): Name of the destination S3 bucket
- `destination_key` (string): Destination S3 object key
- `content_type` (string, optional): Content type override (source headers and user metadata are preserved unless overridden)
- `storage_class` (string, optional): S3 storage class override (does not affect metadata)
- `cache_control` (string, optional): Cache control header override (source headers and user metadata are preserved unless overridden)
- `source_version_id` (string, optional): Version ID of the source object; copies that specific version instead of the latest one (find version IDs with `list_object_versions`)

### `head_object`

Retrieve metadata (size, ETag, content type, storage class, version, user metadata, Object Lock retention and legal hold) for an S3 object without downloading its content.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `version_id` (string, optional): Version ID for versioned objects

### `get_object_attributes`

Get the attributes of an object version without downloading its content: ETag, the checksums it was stored with, the parts it was assembled from, storage class, and size.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `version_id` (string, optional): Version ID to read the attributes of a specific version (default: the current version)

Reports the checksums the storage backend recorded for the object (uploads through this server send a CRC32 checksum by default; a `COMPOSITE` value combines the part checksums and validates the whole object) and the part details it reports, with the size of every part. Part details follow S3's pagination markers, so objects with more than 1,000 parts report every part; buckets without them may report none even for multipart objects.

### `get_object_tagging`

Get the tags (key-value pairs) of an object, or of a specific version of it.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `version_id` (string, optional): Version ID to read the tags of a specific version (default: the current version)

### `put_object_tagging`

Set the tags of an object, or of a specific version of it. The given tags replace every existing tag of that object version.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `tags` (object): Tags to attach — key-value pairs, at most 10; keys up to 128 and values up to 256 characters
- `version_id` (string, optional): Version ID to tag a specific version (default: the current version)

### `delete_object_tagging`

Remove every tag from an object, or from a specific version of it.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `version_id` (string, optional): Version ID to untag a specific version (default: the current version)

### `get_bucket_tagging`

Get the tags (key-value pairs) of a bucket.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_tagging`

Set the tags of a bucket. The given tags replace every existing tag of the bucket.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `tags` (object): Tags to attach — key-value pairs, at most 50; keys up to 128 and values up to 256 characters

### `delete_bucket_tagging`

Remove every tag from a bucket.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `presigned_url`

Generate a time-limited presigned URL for an S3 object, allowing download or upload without embedding AWS credentials in the client.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `method` (string, optional): Operation the URL grants - "get" (download, default) or "put" (upload)
- `expires_in_secs` (number, optional): URL validity in seconds, 1-604800 (default: 3600)

**WARNING: Anyone holding the URL can perform the granted operation until it expires — share it carefully!**

### `get_bucket_cors`

Get the cross-origin resource sharing (CORS) rules of a bucket: the origins, HTTP methods, and headers that browsers on other origins may use. A bucket without a CORS configuration is reported as having no rules.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_cors`

Replace the CORS configuration of a bucket with the given rules. The rules replace the whole configuration, so send every rule that should apply.

**WARNING: This operation replaces the existing CORS configuration!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `rules` (array): Complete CORS configuration that replaces the current one (at least one rule; use `delete_bucket_cors` to remove the configuration). Each rule is an object with:
  - `id` (string, optional): Unique identifier of the rule (up to 255 characters)
  - `allowed_methods` (string[]): HTTP methods the rule allows; each must be exactly `GET`, `PUT`, `POST`, `DELETE`, or `HEAD` (case-sensitive)
  - `allowed_origins` (string[]): Origins allowed to call the bucket, e.g. `https://app.example.com` or `*`
  - `allowed_headers` (string[], optional): Request headers allowed in preflight requests, e.g. `Content-Type` or `*`
  - `expose_headers` (string[], optional): Response headers browsers may read, e.g. `ETag`
  - `max_age_seconds` (number, optional): Seconds browsers may cache preflight responses, 0-86400

### `delete_bucket_cors`

Remove the CORS configuration of a bucket. Afterwards browsers on other origins are no longer allowed to make cross-origin requests to the bucket.

**WARNING: This operation removes the CORS configuration — cross-origin browser requests to the bucket stop working until a new one is set!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `get_bucket_notification`

Get the notification configuration of a bucket: which object events are delivered to which Lambda functions, queues, or topics. Returns a readable summary and the rules as JSON, which can be edited and sent back through `put_bucket_notification`. Refuses buckets that also have an EventBridge notification configuration, which this tool cannot show or edit (putting rules back over one would disable EventBridge).

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_notification`

Replace the notification configuration of a bucket with the given rules. The rules replace the whole configuration, so send every rule that should apply.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `rules` (array): Complete notification configuration that replaces the current one (at least one rule; use `delete_bucket_notification` to remove the configuration). Each rule is an object with:
  - `destination` (string): Where the events are delivered — `"lambda"`, `"queue"`, or `"topic"`
  - `arn` (string): ARN of the destination: on AWS the ARN of the Lambda function, SQS queue, or SNS topic; on RustFS the ARN of a configured target in the form `arn:rustfs:sqs::<id>:<name>` (e.g. `arn:rustfs:sqs::primary:webhook`)
  - `events` (string[]): S3 events that trigger a notification, e.g. `s3:ObjectCreated:*` or `s3:ObjectRemoved:Delete` (at least one)
  - `prefix` (string, optional): Only keys that start with this prefix trigger the rule. Given literally, e.g. `reports+2026/`; the tool encodes it for the wire, where S3 reads `+` as a space
  - `suffix` (string, optional): Only keys that end with this suffix trigger the rule. Given literally and encoded like the prefix
  - `id` (string, optional): Unique identifier of the rule (up to 255 characters)

RustFS accepts notification rules only after at least one notification target is configured on the server, for example a webhook target defined through its environment.

### `delete_bucket_notification`

Remove every notification rule from a bucket. Object events of the bucket are then no longer delivered anywhere.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

The configuration is cleared with an empty `put_bucket_notification_configuration` request instead of S3's own DELETE operation, which some S3-compatible servers do not support.

### `initiate_multipart_upload`

Start a multipart upload for a large file and get an upload ID for the subsequent part tools.

**Parameters:**

- `bucket_name` (string): Target S3 bucket
- `object_key` (string): S3 object key (destination path)
- `content_type` (string, optional): Content type of the final object
- `storage_class` (string, optional): S3 storage class
- `cache_control` (string, optional): Cache control header
- `metadata` (object, optional): User metadata key-value pairs (x-amz-meta-*)

### `upload_part`

Upload a local file as a single multipart part (part numbers 1-10000). Each part (except the last) must be at least 5 MB.

**Parameters:**

- `local_file_path` (string): Path to the local part file (the whole file is sent as one part)
- `bucket_name` (string): Target S3 bucket
- `object_key` (string): S3 object key of the final object
- `upload_id` (string): Upload ID from initiate_multipart_upload
- `part_number` (number): Part number, 1-10000

### `complete_multipart_upload`

Assemble the uploaded parts into the final object. Parts are sorted by part number automatically.

**Parameters:**

- `bucket_name` (string): Target S3 bucket
- `object_key` (string): S3 object key of the final object
- `upload_id` (string): Upload ID from initiate_multipart_upload
- `parts` (array): Part number + ETag pairs collected from upload_part / list_parts

### `abort_multipart_upload`

Abort a multipart upload and permanently discard all parts uploaded so far.

**WARNING: This operation cannot be undone!**

**Parameters:**

- `bucket_name` (string): Target S3 bucket
- `object_key` (string): S3 object key of the final object
- `upload_id` (string): Upload ID from initiate_multipart_upload

### `list_parts`

List the parts uploaded so far for a multipart upload, including each part's ETag for complete_multipart_upload.

**Parameters:**

- `bucket_name` (string): Target S3 bucket
- `object_key` (string): S3 object key of the final object
- `upload_id` (string): Upload ID from initiate_multipart_upload

### `list_multipart_uploads`

List the in-progress multipart uploads of a bucket — the uploads started with `initiate_multipart_upload` that are not yet completed or aborted — with optional prefix filtering, directory-style browsing, and pagination. Each entry carries the upload ID used by `upload_part`, `upload_part_copy`, `list_parts`, `complete_multipart_upload`, and `abort_multipart_upload`. Unfinished uploads keep consuming storage: cancel the ones you no longer need with `abort_multipart_upload`.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `prefix` (string, optional): Prefix to filter the keys of in-progress uploads
- `delimiter` (string, optional): Character to group keys by (e.g. `/` for directory browsing; matching prefixes are returned as Directories)
- `max_uploads` (number, optional): Maximum uploads per page, 1-1000 (default: 1000)
- `key_marker` (string, optional): Key marker from a truncated response to fetch the next page
- `upload_id_marker` (string, optional): Upload ID marker from a truncated response (used together with `key_marker`)

### `upload_part_copy`

Upload one part of a multipart upload by copying an existing object server-side, without downloading it. The part is either the whole source object or an inclusive byte range of it (`range` as `"<start>-<end>"`), which lets the agent assemble a large object from data already in S3. Record the returned ETag for `complete_multipart_upload`. Each part except the last must cover at least 5 MB.

**Parameters:**

- `bucket_name` (string): Target S3 bucket
- `object_key` (string): S3 object key of the final object
- `upload_id` (string): Upload ID from initiate_multipart_upload
- `part_number` (number): Part number, 1-10000
- `source_bucket` (string): Name of the source S3 bucket
- `source_key` (string): Key of the source object to copy
- `source_version_id` (string, optional): Version ID of the source object; copies that specific version instead of the latest one
- `range` (string, optional): Byte range of the source object to copy as `"<start>-<end>"` (zero-based, inclusive byte offsets, e.g. `0-5242879` for the first 5 MB); omit it to copy the whole source object

### `get_bucket_website`

Get the website configuration of a bucket: the index document served for a request ending with `/`, the error document, a redirect of all requests, and routing rules. Returns a readable summary and the configuration as JSON, which can be edited and sent back through `put_bucket_website`.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

### `put_bucket_website`

Set the website configuration of a bucket, replacing the current one. Visitors can only be served objects that a bucket policy lets everyone read (`put_bucket_policy`).

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `index_document_suffix` (string, optional): Suffix of the key served when a request ends with `/`, e.g. `index.html`; required unless `redirect_all_requests_to` is set
- `error_document_key` (string, optional): Key of the document served for errors, e.g. `errors/404.html`
- `redirect_all_requests_to` (object, optional): Host that every request is redirected to instead of serving documents, as `{"host_name": "www.example.com", "protocol": "HTTPS"}`; cannot be combined with the other settings
- `routing_rules` (array, optional): Rules that redirect requests whose key starts with a given prefix, each as `{"key_prefix_equals": "docs/", "replace_key_prefix_with": "documents/"}`. `key_prefix_equals` may be empty to match every key (e.g. prepending `archive/` to all keys); only `replace_key_prefix_with` must be set

The storage backend serves the website on its website endpoint. RustFS 1.0.6 stores the configuration but does not serve website documents on its S3 endpoint.

### `delete_bucket_website`

Delete the website configuration of a bucket. Its objects are then no longer served as a website; the objects themselves stay in place.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket

## Architecture

The MCP server is built with a modular architecture:

```
rustfs-mcp/
├── src/
│   ├── main.rs              # Entry point, CLI parsing, and server initialization
│   ├── lib.rs               # Library exports and public API
│   ├── config.rs            # Configuration management and CLI options
│   ├── server/              # MCP layer
│   │   ├── mod.rs           # Tool registration (rmcp tool_router) and tool handlers
│   │   ├── requests.rs      # Tool request payload types (JSON schemas)
│   │   ├── read_mode.rs     # get_object read-mode handling
│   │   ├── download_mode.rs # get_object download-mode handling
│   │   ├── format_error.rs  # get_object error responses
│   │   ├── lifecycle.rs     # Readable lifecycle-rule summaries
│   │   ├── object_lock.rs   # Notes of the Object Lock reads that found nothing
│   │   ├── replication.rs   # Readable replication-rule summaries
│   │   └── shell_quote.rs   # Shell-quoted example commands
│   └── s3_client/           # S3 layer: one module per S3 operation
│       ├── mod.rs           # Client construction and operation re-exports
│       ├── fake_s3.rs       # Tests only: an in-process S3 endpoint with scripted answers
│       ├── bucket_admin/    # head_bucket, location, policies, request payment
│       ├── acl/             # Bucket and object ACLs
│       ├── cors/            # Bucket CORS configuration
│       ├── encryption/      # Default server-side encryption
│       ├── lifecycle/       # Typed lifecycle rules and validation
│       ├── logging/         # Server access logging
│       ├── multipart_uploads/ # Multipart management operations
│       ├── notification/    # Event notification configurations
│       ├── object_lock/     # Object Lock configuration, retention, legal hold
│       ├── replication/     # Typed replication rules and validation
│       ├── tagging/         # Bucket and object tag sets
│       ├── versioning/      # Versioning state and object versions
│       ├── website/         # Static website configuration
│       └── …                # Per-operation files (get_object, upload_file, presigned_url, …)
└── Cargo.toml               # Dependencies, metadata, and binary configuration
```

## 📄 License

Licensed under the [Apache License, Version 2.0](./LICENSE) — see the [LICENSE](./LICENSE) file for the full text.

Copyright 2024-2026 RustFS, Inc.

**RustFS** is a trademark of RustFS, Inc. All other trademarks are the property of their respective owners.
