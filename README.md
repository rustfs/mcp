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
- **Upload Files**: Upload local files with automatic MIME type detection and cache control
- **Upload Content**: Write text, JSON, or code straight to S3 without creating a local file
- **Get Objects**: Retrieve objects from S3 storage with read or download modes
- **Create Bucket**: Create a new S3 bucket with the specified name
- **Delete Bucket**: Delete an existing, empty S3 bucket
- **Delete Objects**: Permanently delete a single object or batch-delete up to 1,000 objects
- **Tagging**: Read, set, and remove bucket tags and object tags
- **Copy Objects**: Copy objects server-side within or across buckets
- **Head Objects**: Inspect object metadata without downloading the content
- **Versioning**: Enable, suspend, and inspect bucket versioning; list, delete, and copy object versions
- **Multipart Upload**: Upload large files in parts with initiate / upload_part / complete / abort / list_parts tools

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

Create a new S3 bucket with the specified name.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket to create

### `delete_bucket`

Delete an existing S3 bucket with the specified name. If the bucket is not empty, the deletion will fail — delete all objects (and their versions) in the bucket before calling this tool.

**WARNING: This operation will permanently delete the bucket!**

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket to delete

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

Retrieve metadata (size, ETag, content type, storage class, version, user metadata) for an S3 object without downloading its content.

**Parameters:**

- `bucket_name` (string): Name of the S3 bucket
- `object_key` (string): S3 object key
- `version_id` (string, optional): Version ID for versioned objects

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

## Architecture

The MCP server is built with a modular architecture:

```
rustfs-mcp/
├── src/
│   ├── main.rs          # Entry point, CLI parsing, and server initialization
│   ├── server.rs        # MCP server implementation and tool handlers
│   ├── s3_client.rs     # S3 client wrapper with async operations
│   ├── config.rs        # Configuration management and CLI options
│   └── lib.rs           # Library exports and public API
└── Cargo.toml           # Dependencies, metadata, and binary configuration
```
