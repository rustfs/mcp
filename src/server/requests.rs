// Copyright 2024 RustFS Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::s3_client::PresignMethod;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Request payload for listing objects in a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ListObjectsRequest {
    pub bucket_name: String,
    #[serde(default)]
    #[schemars(description = "Optional prefix to filter objects")]
    pub prefix: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional character used to group keys, e.g. '/' for directory-style browsing (returns common prefixes as Directories)"
    )]
    pub delimiter: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional maximum number of keys to return per page (1-1000, default 1000)"
    )]
    pub max_keys: Option<i32>,
    #[serde(default)]
    #[schemars(
        description = "Optional token from a previous truncated response's next_continuation_token to fetch the next page"
    )]
    pub continuation_token: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional key to start listing after (ignored when a continuation token is provided)"
    )]
    pub start_after: Option<String>,
}

/// Request payload for uploading a file to S3.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UploadFileRequest {
    #[schemars(description = "Path to the local file to upload")]
    pub local_file_path: String,
    #[schemars(description = "Name of the S3 bucket to upload to")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional content type (auto-detected if not specified)")]
    pub content_type: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional storage class (STANDARD, REDUCED_REDUNDANCY, etc.)")]
    pub storage_class: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional cache control header")]
    pub cache_control: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional user metadata key-value pairs stored with the object (x-amz-meta-*)"
    )]
    pub metadata: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    #[schemars(description = "Optional server-side encryption (e.g. AES256 or aws:kms)")]
    pub server_side_encryption: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional content disposition header (e.g. attachment; filename=x)")]
    pub content_disposition: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional content encoding header (e.g. gzip)")]
    pub content_encoding: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional content language header (e.g. en-US)")]
    pub content_language: Option<String>,
}

/// Request payload for uploading inline content to S3.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UploadContentRequest {
    #[schemars(
        description = "Text content to store as the object body (creates or replaces the object)"
    )]
    pub content: String,
    #[schemars(description = "Name of the S3 bucket to upload to")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional content type (detected from the object key extension, falling back to text/plain)"
    )]
    pub content_type: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional storage class (STANDARD, REDUCED_REDUNDANCY, etc.)")]
    pub storage_class: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional cache control header")]
    pub cache_control: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional user metadata key-value pairs stored with the object (x-amz-meta-*)"
    )]
    pub metadata: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    #[schemars(description = "Optional server-side encryption (e.g. AES256 or aws:kms)")]
    pub server_side_encryption: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional content disposition header (e.g. attachment; filename=x)")]
    pub content_disposition: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional content encoding header (e.g. gzip)")]
    pub content_encoding: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional content language header (e.g. en-US)")]
    pub content_language: Option<String>,
}

/// Request payload for creating a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CreateBucketRequest {
    #[schemars(description = "Name of the S3 bucket to create")]
    pub bucket_name: String,
    #[serde(default)]
    #[schemars(
        description = "Optional: create the bucket with Object Lock enabled, which also turns on versioning. Object Lock can only be enabled when the bucket is created and cannot be turned off afterwards (default: false)"
    )]
    pub object_lock_enabled: Option<bool>,
}

/// Request payload for checking that a bucket exists.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct HeadBucketRequest {
    #[schemars(description = "Name of the S3 bucket to check")]
    pub bucket_name: String,
}

/// Request payload for reading a bucket's region.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketLocationRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for reading a bucket policy.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketPolicyRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for replacing a bucket policy.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketPolicyRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "The complete bucket policy as a JSON resource policy document (Version + Statement array). Replaces the current policy"
    )]
    pub policy: std::collections::BTreeMap<String, serde_json::Value>,
}

/// Request payload for removing a bucket policy.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketPolicyRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for reading a bucket's request payment configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketRequestPaymentRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for setting who pays for the requests to a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketRequestPaymentRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "Who pays for the requests to the bucket: \"BucketOwner\" (the default; anonymous requests stay allowed) or \"Requester\" (callers must authenticate and cover the request and data-transfer costs; anonymous access is disabled while it is set)"
    )]
    pub payer: crate::s3_client::BucketPayer,
}

/// Request payload for deleting a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketRequest {
    #[schemars(description = "Name of the S3 bucket to delete")]
    pub bucket_name: String,
}

/// Request payload for reading a bucket's versioning configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketVersioningRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for enabling or suspending bucket versioning.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketVersioningRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "Versioning state to set: 'enabled' keeps every version of every object, 'suspended' stops creating new versions while keeping the existing ones"
    )]
    pub status: crate::s3_client::BucketVersioningStatus,
}

/// Request payload for listing object versions.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ListObjectVersionsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[serde(default)]
    #[schemars(description = "Optional prefix to filter keys")]
    pub prefix: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional character used to group keys, e.g. '/' for directory-style browsing (returns common prefixes as Directories)"
    )]
    pub delimiter: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional maximum number of entries (versions + delete markers) to return per page (1-1000, default 1000)"
    )]
    pub max_keys: Option<i32>,
    #[serde(default)]
    #[schemars(
        description = "Optional key marker from a previous truncated response's next_key_marker to fetch the next page"
    )]
    pub key_marker: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID marker from a previous truncated response's next_version_id_marker (used together with key_marker)"
    )]
    pub version_id_marker: Option<String>,
}

/// Request payload for permanently deleting specific object versions.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteObjectVersionsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "Object versions to delete permanently (key + version ID pairs from list_object_versions; delete markers count as versions; up to 1000 per request)"
    )]
    pub versions: Vec<crate::s3_client::ObjectVersion>,
}

/// Request payload for reading a bucket's Object Lock configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketLockConfigurationRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for setting the default retention of an Object Lock bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketLockConfigurationRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[serde(default)]
    #[schemars(
        description = "Retention mode applied to every new object version: 'GOVERNANCE' (users with the s3:BypassGovernanceRetention permission can shorten it) or 'COMPLIANCE' (cannot be shortened or removed until it expires); omit it together with days and years to remove the default retention"
    )]
    pub mode: Option<crate::s3_client::ObjectLockRetentionMode>,
    #[serde(default)]
    #[schemars(
        description = "Retention period in days (at least 1); set exactly one of days and years"
    )]
    pub days: Option<i32>,
    #[serde(default)]
    #[schemars(
        description = "Retention period in years (at least 1); set exactly one of days and years"
    )]
    pub years: Option<i32>,
}

/// Request payload for reading an object version's retention.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetObjectRetentionRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to read the retention of a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for setting an object version's retention.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutObjectRetentionRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[schemars(
        description = "Retention mode: 'GOVERNANCE' (users with the s3:BypassGovernanceRetention permission can shorten it) or 'COMPLIANCE' (cannot be shortened or removed until it expires)"
    )]
    pub mode: crate::s3_client::ObjectLockRetentionMode,
    #[schemars(
        description = "Moment until which the object version stays retained, as an ISO 8601 UTC timestamp, e.g. \"2030-01-01T00:00:00Z\"; must lie in the future"
    )]
    pub retain_until: String,
    #[serde(default)]
    #[schemars(
        description = "Optional: allow shortening an existing GOVERNANCE retention; requires the s3:BypassGovernanceRetention permission (default: false)"
    )]
    pub bypass_governance_retention: bool,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to set the retention of a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for reading an object version's legal hold.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetObjectLegalHoldRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to read the legal hold of a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for setting an object version's legal hold.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutObjectLegalHoldRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[schemars(
        description = "Legal hold status to set: 'ON' stops the version from being deleted or overwritten until the hold is removed, 'OFF' removes the hold"
    )]
    pub status: crate::s3_client::ObjectLockLegalHoldStatus,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to set the legal hold of a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for reading/downloading an object.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetObjectRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional version ID for versioned objects")]
    pub version_id: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional byte range to read, e.g. 'bytes=0-1023' (partial read, supported in both modes)"
    )]
    pub range: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional ISO 8601 timestamp; the read succeeds only if the object was modified after this time, otherwise it fails with a not-modified error"
    )]
    pub if_modified_since: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional ISO 8601 timestamp; the read succeeds only if the object was not modified after this time, otherwise it fails with a precondition error"
    )]
    pub if_unmodified_since: Option<String>,
    #[serde(default = "default_operation_mode")]
    #[schemars(
        description = "Operation mode: read (return content) or download (save to local file)"
    )]
    pub mode: GetObjectMode,
    #[serde(default)]
    #[schemars(description = "Local file path for download mode (required when mode is download)")]
    pub local_path: Option<String>,
    #[serde(default = "default_max_content_size")]
    #[schemars(description = "Maximum content size to read in bytes for read mode (default: 1MB)")]
    pub max_content_size: usize,
}

#[derive(Serialize, Deserialize, JsonSchema, Debug, Clone, PartialEq)]
pub enum GetObjectMode {
    /// Read object content directly and return it in response text.
    #[serde(rename = "read")]
    Read,
    /// Download object content to a local file path.
    #[serde(rename = "download")]
    Download,
}

fn default_operation_mode() -> GetObjectMode {
    GetObjectMode::Read
}
fn default_max_content_size() -> usize {
    1024 * 1024
}

/// Request payload for reading a bucket's logging configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketLoggingRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for enabling server access logging of a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketLoggingRequest {
    #[schemars(description = "Name of the S3 bucket whose requests are recorded")]
    pub bucket_name: String,
    #[schemars(
        description = "Bucket the access logs are written to, e.g. 'my-logs'; it should exist and allow the logging service to write to it"
    )]
    pub target_bucket: String,
    #[serde(default)]
    #[schemars(
        description = "Optional prefix of the keys the logs are stored under, e.g. 'logs/' (default: the root of the target bucket)"
    )]
    pub target_prefix: Option<String>,
}

/// Request payload for disabling server access logging of a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketLoggingRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for reading a bucket's default encryption configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketEncryptionRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for setting the default encryption of a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketEncryptionRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "Server-side encryption algorithm for new objects: 'AES256' (SSE-S3), 'aws:kms' (SSE-KMS), or 'aws:kms:dsse' (dual-layer server-side encryption with KMS); exact spelling, case-sensitive"
    )]
    pub sse_algorithm: String,
    #[serde(default)]
    #[schemars(
        description = "Optional KMS key ID, ARN, or alias; only used with the 'aws:kms' and 'aws:kms:dsse' algorithms"
    )]
    pub kms_key_id: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional Bucket Key setting, which reduces KMS request costs; only supported with the 'aws:kms' algorithm (S3 does not apply Bucket Keys to 'AES256' or 'aws:kms:dsse')"
    )]
    pub bucket_key_enabled: Option<bool>,
}

/// Request payload for removing the default encryption of a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketEncryptionRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for deleting a single object.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteObjectRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional version ID for versioned objects")]
    pub version_id: Option<String>,
}

/// Request payload for reading a bucket's access control policy.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketAclRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for applying a canned ACL to a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketAclRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "Canned ACL to apply: 'private' (only the owner has access), 'public-read' (everyone may read), 'public-read-write' (everyone may read and write), or 'authenticated-read' (any authenticated account may read)"
    )]
    pub acl: crate::s3_client::CannedAcl,
}

/// Request payload for reading an object version's access control policy.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetObjectAclRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to read the ACL of a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for applying a canned ACL to an object version.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutObjectAclRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[schemars(
        description = "Canned ACL to apply: 'private' (only the owner has access), 'public-read' (everyone may read), 'public-read-write' (everyone may read and write), 'authenticated-read' (any authenticated account may read), 'bucket-owner-read' (the bucket owner may read), or 'bucket-owner-full-control' (the bucket owner gets full control)"
    )]
    pub acl: crate::s3_client::CannedAcl,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to set the ACL of a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for batch-deleting objects.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteObjectsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "Object keys to delete permanently (up to 1000 keys per request)")]
    pub object_keys: Vec<String>,
}

/// Request payload for reading a bucket's notification configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketNotificationRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for replacing a bucket's notification configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketNotificationRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "The complete notification configuration; it replaces the current one. Each rule names a destination (lambda, queue, or topic), the ARN to deliver to, at least one S3 event, and an optional key prefix/suffix filter"
    )]
    pub rules: Vec<crate::s3_client::NotificationRule>,
}

/// Request payload for removing a bucket's notification configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketNotificationRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for server-side object copy.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CopyObjectRequest {
    #[schemars(description = "Name of the source S3 bucket")]
    pub source_bucket: String,
    #[schemars(description = "Source S3 object key")]
    pub source_key: String,
    #[schemars(description = "Name of the destination S3 bucket")]
    pub destination_bucket: String,
    #[schemars(description = "Destination S3 object key")]
    pub destination_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional content type override (applied with REPLACE metadata directive)"
    )]
    pub content_type: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional storage class override (STANDARD, REDUCED_REDUNDANCY, etc.)"
    )]
    pub storage_class: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional cache control header override")]
    pub cache_control: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID of the source object; copies that specific version instead of the latest one (find version IDs with list_object_versions)"
    )]
    pub source_version_id: Option<String>,
}

/// Request payload for reading a bucket's website configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketWebsiteRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for replacing a bucket's website configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketWebsiteRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[serde(default)]
    #[schemars(
        description = "Suffix of the key served when a request ends with '/', e.g. 'index.html'; required unless redirect_all_requests_to is set"
    )]
    pub index_document_suffix: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional key of the document served for errors, e.g. 'errors/404.html'"
    )]
    pub error_document_key: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional host that every request is redirected to instead of serving documents, as {\"host_name\": \"www.example.com\", \"protocol\": \"HTTPS\"}; cannot be combined with the other settings"
    )]
    pub redirect_all_requests_to: Option<crate::s3_client::RedirectAllRequestsTo>,
    #[serde(default)]
    #[schemars(
        description = "Optional rules that redirect requests whose key starts with a given prefix, each as {\"key_prefix_equals\": \"docs/\", \"replace_key_prefix_with\": \"documents/\"}"
    )]
    pub routing_rules: Vec<crate::s3_client::RoutingRule>,
}

/// Request payload for deleting a bucket's website configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketWebsiteRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for reading a bucket's lifecycle configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketLifecycleRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for replacing a bucket's lifecycle configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketLifecycleRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "The complete lifecycle configuration (1-1000 rules); it replaces the current configuration. Every rule needs a status and at least one action"
    )]
    pub rules: Vec<crate::s3_client::LifecycleRule>,
}

/// Request payload for removing a bucket's lifecycle configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketLifecycleRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for reading a bucket's replication configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketReplicationRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for replacing a bucket's replication configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketReplicationRequest {
    #[schemars(description = "Name of the S3 bucket to replicate from")]
    pub bucket_name: String,
    #[schemars(
        description = "Who copies, as an ARN: an IAM role on AWS S3, e.g. \"arn:aws:iam::123456789012:role/replicator\"; on MinIO and RustFS the ARN of a replication target configured on the server, e.g. \"arn:rustfs:replication::<deployment-id>:<bucket>\""
    )]
    pub role: String,
    #[schemars(
        description = "The complete replication configuration (1-1000 rules); it replaces the current configuration. Every rule needs a status and a destination bucket that differs from this bucket, and the bucket needs versioning enabled. Buckets whose current configuration holds settings this tool does not model are refused (get_bucket_replication lists them)"
    )]
    pub rules: Vec<crate::s3_client::ReplicationRule>,
}

/// Request payload for removing a bucket's replication configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketReplicationRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for reading the attributes of an object version.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetObjectAttributesRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to read the attributes of a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for retrieving object metadata.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct HeadObjectRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional version ID for versioned objects")]
    pub version_id: Option<String>,
}

/// Request payload for reading an object's tags.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetObjectTaggingRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to read the tags of a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for replacing an object's tags.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutObjectTaggingRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[schemars(
        description = "Tags to attach (key-value pairs, at most 10; keys up to 128 and values up to 256 characters). Replaces every existing tag of the object"
    )]
    pub tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to tag a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for removing an object's tags.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteObjectTaggingRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID to untag a specific version (default: the current version)"
    )]
    pub version_id: Option<String>,
}

/// Request payload for reading a bucket's tags.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketTaggingRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for replacing a bucket's tags.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketTaggingRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "Tags to attach (key-value pairs, at most 50; keys up to 128 and values up to 256 characters). Replaces every existing tag of the bucket"
    )]
    pub tags: std::collections::HashMap<String, String>,
}

/// Request payload for removing a bucket's tags.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketTaggingRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for generating a presigned URL.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PresignUrlRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (path/filename in the bucket)")]
    pub object_key: String,
    #[serde(default = "default_presign_method")]
    #[schemars(description = "Operation the URL grants: get (download) or put (upload)")]
    pub method: PresignMethod,
    #[serde(default = "default_presign_expiry")]
    #[schemars(
        description = "URL validity in seconds, clamped to 1-604800 (7 days maximum; default 3600)"
    )]
    pub expires_in_secs: u64,
}

fn default_presign_method() -> PresignMethod {
    PresignMethod::Get
}
fn default_presign_expiry() -> u64 {
    3600
}

/// Request payload for reading a bucket's CORS configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct GetBucketCorsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for replacing a bucket's CORS configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct PutBucketCorsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(
        description = "Complete CORS configuration that replaces the current one: the rules deciding which origins, HTTP methods, and headers browsers may use (send every rule that should apply, at least one; use delete_bucket_cors to remove the configuration)"
    )]
    pub rules: Vec<crate::s3_client::CorsRule>,
}

/// Request payload for deleting a bucket's CORS configuration.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketCorsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
}

/// Request payload for initiating a multipart upload.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct InitiateMultipartUploadRequest {
    #[schemars(description = "Name of the S3 bucket to upload to")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key (destination path)")]
    pub object_key: String,
    #[serde(default)]
    #[schemars(description = "Optional content type of the final object")]
    pub content_type: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional storage class (STANDARD, REDUCED_REDUNDANCY, etc.)")]
    pub storage_class: Option<String>,
    #[serde(default)]
    #[schemars(description = "Optional cache control header")]
    pub cache_control: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional user metadata key-value pairs stored with the final object (x-amz-meta-*)"
    )]
    pub metadata: Option<std::collections::HashMap<String, String>>,
}

/// A part recorded for multipart upload completion.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CompletedPartInput {
    #[schemars(description = "Part number (1-10000)")]
    pub part_number: i32,
    #[schemars(description = "ETag returned by upload_part for this part")]
    pub etag: String,
}

/// Request payload for uploading one multipart part from a local file.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UploadPartRequest {
    #[schemars(
        description = "Path to the local file to upload as this part (the whole file is sent as one part)"
    )]
    pub local_file_path: String,
    #[schemars(description = "Name of the S3 bucket to upload to")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key of the final object")]
    pub object_key: String,
    #[schemars(description = "Upload ID returned by initiate_multipart_upload")]
    pub upload_id: String,
    #[schemars(description = "Part number (1-10000)")]
    pub part_number: i32,
}

/// Request payload for completing a multipart upload.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CompleteMultipartUploadRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key of the final object")]
    pub object_key: String,
    #[schemars(description = "Upload ID returned by initiate_multipart_upload")]
    pub upload_id: String,
    #[schemars(
        description = "Parts to assemble, in any order (sorted by part number automatically)"
    )]
    pub parts: Vec<CompletedPartInput>,
}

/// Request payload for aborting a multipart upload.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct AbortMultipartUploadRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key of the final object")]
    pub object_key: String,
    #[schemars(description = "Upload ID returned by initiate_multipart_upload")]
    pub upload_id: String,
}

/// Request payload for listing uploaded parts of a multipart upload.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ListPartsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key of the final object")]
    pub object_key: String,
    #[schemars(description = "Upload ID returned by initiate_multipart_upload")]
    pub upload_id: String,
}

/// Request payload for listing the in-progress multipart uploads of a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ListMultipartUploadsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[serde(default)]
    #[schemars(description = "Optional prefix to filter the keys of in-progress uploads")]
    pub prefix: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional character used to group keys, e.g. '/' for directory-style browsing (returns common prefixes as Directories)"
    )]
    pub delimiter: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional maximum number of uploads to return per page (1-1000, default 1000)"
    )]
    pub max_uploads: Option<i32>,
    #[serde(default)]
    #[schemars(
        description = "Optional key marker from a previous truncated response's next_key_marker to fetch the next page"
    )]
    pub key_marker: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional upload ID marker from a previous truncated response's next_upload_id_marker (used together with key_marker)"
    )]
    pub upload_id_marker: Option<String>,
}

/// Request payload for uploading one multipart part by copying an existing object.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct UploadPartCopyRequest {
    #[schemars(description = "Name of the S3 bucket of the multipart upload")]
    pub bucket_name: String,
    #[schemars(description = "S3 object key of the final object")]
    pub object_key: String,
    #[schemars(description = "Upload ID returned by initiate_multipart_upload")]
    pub upload_id: String,
    #[schemars(description = "Part number (1-10000)")]
    pub part_number: i32,
    #[schemars(description = "Name of the bucket holding the source object")]
    pub source_bucket: String,
    #[schemars(description = "Source S3 object key")]
    pub source_key: String,
    #[serde(default)]
    #[schemars(
        description = "Optional version ID of the source object; copies that specific version instead of the latest one"
    )]
    pub source_version_id: Option<String>,
    #[serde(default)]
    #[schemars(
        description = "Optional byte range of the source object copied as this part, as \"<start>-<end>\" (zero-based, inclusive byte offsets), e.g. \"0-5242879\" for the first 5 MB (5,242,880 bytes); when omitted the whole source object becomes the part"
    )]
    pub range: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_object_request_defaults() {
        let request = GetObjectRequest {
            bucket_name: "test-bucket".to_string(),
            object_key: "test-key".to_string(),
            version_id: None,
            range: None,
            if_modified_since: None,
            if_unmodified_since: None,
            mode: default_operation_mode(),
            local_path: None,
            max_content_size: default_max_content_size(),
        };

        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_key, "test-key");
        assert!(request.version_id.is_none());
        assert!(request.range.is_none());
        assert!(request.if_modified_since.is_none());
        assert!(request.if_unmodified_since.is_none());
        assert_eq!(request.mode, GetObjectMode::Read);
        assert!(request.local_path.is_none());
        assert_eq!(request.max_content_size, 1024 * 1024);
    }

    #[test]
    fn test_get_object_request_serialization() {
        let request = GetObjectRequest {
            bucket_name: "test-bucket".to_string(),
            object_key: "test-key".to_string(),
            version_id: Some("version123".to_string()),
            range: Some("bytes=0-1023".to_string()),
            if_modified_since: Some("2026-11-24T00:00:00Z".to_string()),
            if_unmodified_since: None,
            mode: GetObjectMode::Download,
            local_path: Some("/path/to/file".to_string()),
            max_content_size: 2048,
        };

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: GetObjectRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(request.bucket_name, deserialized.bucket_name);
        assert_eq!(request.object_key, deserialized.object_key);
        assert_eq!(request.version_id, deserialized.version_id);
        assert_eq!(request.range, deserialized.range);
        assert_eq!(request.if_modified_since, deserialized.if_modified_since);
        assert_eq!(request.mode, deserialized.mode);
        assert_eq!(request.local_path, deserialized.local_path);
        assert_eq!(request.max_content_size, deserialized.max_content_size);
    }

    #[test]
    fn test_get_object_request_serde_with_defaults() {
        let json = r#"{
            "bucket_name": "test-bucket",
            "object_key": "test-key"
        }"#;

        let request: GetObjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_key, "test-key");
        assert!(request.version_id.is_none());
        assert!(request.range.is_none());
        assert!(request.if_modified_since.is_none());
        assert!(request.if_unmodified_since.is_none());
        assert_eq!(request.mode, GetObjectMode::Read);
        assert!(request.local_path.is_none());
        assert_eq!(request.max_content_size, 1024 * 1024);
    }

    #[test]
    fn test_default_functions() {
        assert_eq!(default_operation_mode(), GetObjectMode::Read);
        assert_eq!(default_max_content_size(), 1024 * 1024);
    }

    #[test]
    fn test_get_object_mode_serialization() {
        let read_mode = GetObjectMode::Read;
        let download_mode = GetObjectMode::Download;

        let read_json = serde_json::to_string(&read_mode).unwrap();
        let download_json = serde_json::to_string(&download_mode).unwrap();

        assert_eq!(read_json, r#""read""#);
        assert_eq!(download_json, r#""download""#);

        let read_mode_deser: GetObjectMode = serde_json::from_str(r#""read""#).unwrap();
        let download_mode_deser: GetObjectMode = serde_json::from_str(r#""download""#).unwrap();

        assert_eq!(read_mode_deser, GetObjectMode::Read);
        assert_eq!(download_mode_deser, GetObjectMode::Download);
    }

    #[test]
    fn test_bucket_encryption_request_deserialization() {
        let get: GetBucketEncryptionRequest =
            serde_json::from_str(r#"{"bucket_name": "test-bucket"}"#).unwrap();
        assert_eq!(get.bucket_name, "test-bucket");

        let minimal: PutBucketEncryptionRequest =
            serde_json::from_str(r#"{"bucket_name": "test-bucket", "sse_algorithm": "AES256"}"#)
                .unwrap();
        assert_eq!(minimal.bucket_name, "test-bucket");
        assert_eq!(minimal.sse_algorithm, "AES256");
        assert!(minimal.kms_key_id.is_none());
        assert!(minimal.bucket_key_enabled.is_none());

        let full: PutBucketEncryptionRequest = serde_json::from_str(
            r#"{
            "bucket_name": "test-bucket",
            "sse_algorithm": "aws:kms",
            "kms_key_id": "arn:aws:kms:us-east-1:111122223333:key/abc",
            "bucket_key_enabled": true
        }"#,
        )
        .unwrap();
        assert_eq!(full.sse_algorithm, "aws:kms");
        assert_eq!(
            full.kms_key_id,
            Some("arn:aws:kms:us-east-1:111122223333:key/abc".to_string())
        );
        assert_eq!(full.bucket_key_enabled, Some(true));

        let delete: DeleteBucketEncryptionRequest =
            serde_json::from_str(r#"{"bucket_name": "test-bucket"}"#).unwrap();
        assert_eq!(delete.bucket_name, "test-bucket");
    }

    #[test]
    fn test_put_bucket_encryption_request_requires_algorithm() {
        // An explicit false is kept apart from an unset Bucket Key.
        let disabled: PutBucketEncryptionRequest = serde_json::from_str(
            r#"{"bucket_name": "test-bucket", "sse_algorithm": "AES256", "bucket_key_enabled": false}"#,
        )
        .unwrap();
        assert_eq!(disabled.bucket_key_enabled, Some(false));

        assert!(
            serde_json::from_str::<PutBucketEncryptionRequest>(r#"{"bucket_name": "test-bucket"}"#)
                .is_err()
        );
    }

    #[test]
    fn test_bucket_creation() {
        let request: CreateBucketRequest =
            serde_json::from_str(r#"{"bucket_name": "test-bucket"}"#).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        // Object Lock stays off unless it is asked for.
        assert_eq!(request.object_lock_enabled, None);

        let request: CreateBucketRequest =
            serde_json::from_str(r#"{"bucket_name": "test-bucket", "object_lock_enabled": true}"#)
                .unwrap();
        assert_eq!(request.object_lock_enabled, Some(true));
    }

    #[test]
    fn test_bucket_deletion() {
        let request = DeleteBucketRequest {
            bucket_name: "test-bucket".to_string(),
        };
        assert_eq!(request.bucket_name, "test-bucket");
    }

    #[test]
    fn test_bucket_policy_request_deserialization() {
        let request: PutBucketPolicyRequest = serde_json::from_str(
            r#"{
            "bucket_name": "public-bucket",
            "policy": {
                "Version": "2012-10-17",
                "Statement": [
                    {
                        "Sid": "PublicRead",
                        "Effect": "Allow",
                        "Principal": "*",
                        "Action": ["s3:GetObject"],
                        "Resource": ["arn:aws:s3:::public-bucket/*"]
                    }
                ]
            }
        }"#,
        )
        .unwrap();
        assert_eq!(request.bucket_name, "public-bucket");
        let serialized = serde_json::to_string(&request.policy).unwrap();
        assert!(serialized.contains(r#""Effect":"Allow""#));
        assert!(serialized.contains(r#""Principal":"*""#));
        assert!(serialized.contains(r#""Action":["s3:GetObject"]"#));

        // A policy that is not a JSON object is rejected by the schema.
        assert!(
            serde_json::from_str::<PutBucketPolicyRequest>(
                r#"{"bucket_name": "b", "policy": "not an object"}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<PutBucketPolicyRequest>(r#"{"bucket_name": "b"}"#).is_err());
    }

    #[test]
    fn test_bucket_inspection_request_deserialization() {
        let head: HeadBucketRequest = serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        let location: GetBucketLocationRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        let get: GetBucketPolicyRequest = serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        let delete: DeleteBucketPolicyRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();

        for bucket_name in [
            head.bucket_name,
            location.bucket_name,
            get.bucket_name,
            delete.bucket_name,
        ] {
            assert_eq!(bucket_name, "b");
        }
    }

    #[test]
    fn test_list_objects_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket"}"#;
        let request: ListObjectsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert!(request.prefix.is_none());
        assert!(request.delimiter.is_none());
        assert!(request.max_keys.is_none());
        assert!(request.continuation_token.is_none());
        assert!(request.start_after.is_none());

        let full: ListObjectsRequest = serde_json::from_str(
            r#"{
            "bucket_name": "test-bucket",
            "prefix": "docs/",
            "delimiter": "/",
            "max_keys": 50,
            "continuation_token": "tok==",
            "start_after": "docs/a.txt"
        }"#,
        )
        .unwrap();
        assert_eq!(full.prefix, Some("docs/".to_string()));
        assert_eq!(full.delimiter, Some("/".to_string()));
        assert_eq!(full.max_keys, Some(50));
        assert_eq!(full.continuation_token, Some("tok==".to_string()));
        assert_eq!(full.start_after, Some("docs/a.txt".to_string()));
    }

    #[test]
    fn test_delete_object_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_key": "a/b.txt"}"#;
        let request: DeleteObjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_key, "a/b.txt");
        assert!(request.version_id.is_none());

        let with_version: DeleteObjectRequest = serde_json::from_str(
            r#"{"bucket_name": "test-bucket", "object_key": "a/b.txt", "version_id": "v1"}"#,
        )
        .unwrap();
        assert_eq!(with_version.version_id, Some("v1".to_string()));
    }

    #[test]
    fn test_object_lock_request_deserialization() {
        let lock_config: PutBucketLockConfigurationRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "mode": "COMPLIANCE", "years": 7}"#)
                .unwrap();
        assert_eq!(lock_config.bucket_name, "b");
        assert_eq!(
            lock_config.mode,
            Some(crate::s3_client::ObjectLockRetentionMode::Compliance)
        );
        assert_eq!(lock_config.days, None);
        assert_eq!(lock_config.years, Some(7));

        // Leaving the three fields out asks to remove the default retention.
        let removal: PutBucketLockConfigurationRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(removal.mode, None);
        assert_eq!(removal.days, None);
        assert_eq!(removal.years, None);

        let retention: PutObjectRetentionRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "object_key": "k",
            "mode": "GOVERNANCE",
            "retain_until": "2030-01-01T00:00:00Z",
            "version_id": "v1"
        }"#,
        )
        .unwrap();
        assert_eq!(retention.object_key, "k");
        assert_eq!(
            retention.mode,
            crate::s3_client::ObjectLockRetentionMode::Governance
        );
        assert_eq!(retention.retain_until, "2030-01-01T00:00:00Z");
        assert_eq!(retention.version_id.as_deref(), Some("v1"));
        assert!(!retention.bypass_governance_retention);

        let hold: PutObjectLegalHoldRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k", "status": "ON"}"#)
                .unwrap();
        assert_eq!(hold.status, crate::s3_client::ObjectLockLegalHoldStatus::On);
        assert!(hold.version_id.is_none());

        let read: GetObjectRetentionRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k"}"#).unwrap();
        assert!(read.version_id.is_none());
    }

    #[test]
    fn test_object_lock_requests_reject_unknown_enum_values() {
        // S3 compares the mode and status exactly, so they must not be
        // silently coerced before reaching the storage backend.
        let error = serde_json::from_str::<PutBucketLockConfigurationRequest>(
            r#"{"bucket_name": "b", "mode": "governance", "days": 1}"#,
        )
        .err()
        .expect("a lower-case mode must be rejected");
        assert!(error.to_string().contains("unknown variant"), "{error}");

        let error = serde_json::from_str::<PutObjectLegalHoldRequest>(
            r#"{"bucket_name": "b", "object_key": "k", "status": "on"}"#,
        )
        .err()
        .expect("a lower-case status must be rejected");
        assert!(error.to_string().contains("unknown variant"), "{error}");
    }
    #[test]
    fn test_delete_objects_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_keys": ["a.txt", "b/c.txt"]}"#;
        let request: DeleteObjectsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_keys.len(), 2);
        assert_eq!(request.object_keys[1], "b/c.txt");
    }

    #[test]
    fn test_bucket_notification_request_deserialization() {
        let put: PutBucketNotificationRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "rules": [
                {
                    "destination": "queue",
                    "arn": "arn:rustfs:sqs::primary:webhook",
                    "events": ["s3:ObjectCreated:*"],
                    "prefix": "images/",
                    "suffix": ".jpg",
                    "id": "image-uploads"
                },
                {
                    "destination": "topic",
                    "arn": "arn:aws:sns:us-east-1:111122223333:my-topic",
                    "events": ["s3:ObjectRemoved:Delete"]
                }
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(put.bucket_name, "b");
        assert_eq!(put.rules.len(), 2);
        assert_eq!(
            put.rules[0].destination,
            crate::s3_client::NotificationDestination::Queue
        );
        assert_eq!(put.rules[0].prefix.as_deref(), Some("images/"));
        assert_eq!(put.rules[0].id.as_deref(), Some("image-uploads"));
        // The optional fields default to None.
        assert_eq!(
            put.rules[1].destination,
            crate::s3_client::NotificationDestination::Topic
        );
        assert_eq!(
            put.rules[1].events,
            vec!["s3:ObjectRemoved:Delete".to_string()]
        );
        assert!(put.rules[1].prefix.is_none());
        assert!(put.rules[1].suffix.is_none());
        assert!(put.rules[1].id.is_none());

        let get: GetBucketNotificationRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(get.bucket_name, "b");
        let delete: DeleteBucketNotificationRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(delete.bucket_name, "b");
    }
    #[test]
    fn test_bucket_lifecycle_request_deserialization() {
        let put: PutBucketLifecycleRequest = serde_json::from_str(
            r#"{
            "bucket_name": "logs",
            "rules": [
                {
                    "id": "expire-logs",
                    "status": "enabled",
                    "prefix": "logs/",
                    "expiration": {"days": 30},
                    "abort_incomplete_multipart_upload_after_days": 7
                }
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(put.bucket_name, "logs");
        assert_eq!(put.rules.len(), 1);
        assert_eq!(put.rules[0].id.as_deref(), Some("expire-logs"));
        assert_eq!(
            put.rules[0]
                .expiration
                .as_ref()
                .and_then(|expiration| expiration.days),
            Some(30)
        );
        assert_eq!(put.rules[0].transitions.len(), 0);

        // An unknown status is rejected.
        assert!(
            serde_json::from_str::<PutBucketLifecycleRequest>(
                r#"{"bucket_name": "b", "rules": [{"status": "on", "expiration": {"days": 30}}]}"#
            )
            .is_err()
        );

        let get: GetBucketLifecycleRequest =
            serde_json::from_str(r#"{"bucket_name": "logs"}"#).unwrap();
        let delete: DeleteBucketLifecycleRequest =
            serde_json::from_str(r#"{"bucket_name": "logs"}"#).unwrap();
        assert_eq!(get.bucket_name, "logs");
        assert_eq!(delete.bucket_name, "logs");
    }

    #[test]
    fn test_acl_request_deserialization() {
        let bucket: PutBucketAclRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "acl": "public-read"}"#).unwrap();
        assert_eq!(bucket.bucket_name, "b");
        assert_eq!(bucket.acl, crate::s3_client::CannedAcl::PublicRead);

        let object: PutObjectAclRequest = serde_json::from_str(
            r#"{"bucket_name": "b", "object_key": "k", "acl": "bucket-owner-read", "version_id": "v1"}"#,
        )
        .unwrap();
        assert_eq!(object.object_key, "k");
        assert_eq!(object.acl, crate::s3_client::CannedAcl::BucketOwnerRead);
        assert_eq!(object.version_id.as_deref(), Some("v1"));

        let get: GetObjectAclRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k"}"#).unwrap();
        assert!(get.version_id.is_none());

        // A canned ACL outside the six S3 defines is rejected up front.
        let error = serde_json::from_str::<PutBucketAclRequest>(
            r#"{"bucket_name": "b", "acl": "everyone"}"#,
        )
        .err()
        .expect("an unknown canned ACL must be rejected");
        assert!(error.to_string().contains("unknown variant"), "{error}");
    }
    #[test]
    fn test_copy_object_request_deserialization() {
        let json = r#"{
            "source_bucket": "src-bucket",
            "source_key": "docs/report.pdf",
            "destination_bucket": "dst-bucket",
            "destination_key": "archive/report-2026.pdf"
        }"#;
        let request: CopyObjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.source_bucket, "src-bucket");
        assert_eq!(request.destination_key, "archive/report-2026.pdf");
        assert!(request.content_type.is_none());
        assert!(request.storage_class.is_none());
        assert!(request.cache_control.is_none());
        assert!(request.source_version_id.is_none());

        let with_overrides: CopyObjectRequest = serde_json::from_str(
            r#"{
            "source_bucket": "src-bucket",
            "source_key": "a.txt",
            "destination_bucket": "dst-bucket",
            "destination_key": "b.txt",
            "content_type": "text/plain",
            "storage_class": "REDUCED_REDUNDANCY",
            "cache_control": "max-age=3600"
        }"#,
        )
        .unwrap();
        assert_eq!(with_overrides.content_type, Some("text/plain".to_string()));
        assert_eq!(
            with_overrides.storage_class,
            Some("REDUCED_REDUNDANCY".to_string())
        );

        let from_version: CopyObjectRequest = serde_json::from_str(
            r#"{
            "source_bucket": "src-bucket",
            "source_key": "a.txt",
            "destination_bucket": "dst-bucket",
            "destination_key": "b.txt",
            "source_version_id": "v1"
        }"#,
        )
        .unwrap();
        assert_eq!(from_version.source_version_id, Some("v1".to_string()));
    }

    #[test]
    fn test_versioning_request_deserialization() {
        let get: GetBucketVersioningRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(get.bucket_name, "b");

        let put: PutBucketVersioningRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "status": "suspended"}"#).unwrap();
        assert_eq!(
            put.status,
            crate::s3_client::BucketVersioningStatus::Suspended
        );
        assert!(
            serde_json::from_str::<PutBucketVersioningRequest>(
                r#"{"bucket_name": "b", "status": "on"}"#
            )
            .is_err()
        );

        let minimal: ListObjectVersionsRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert!(minimal.prefix.is_none());
        assert!(minimal.delimiter.is_none());
        assert!(minimal.max_keys.is_none());
        assert!(minimal.key_marker.is_none());
        assert!(minimal.version_id_marker.is_none());

        let paged: ListObjectVersionsRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "prefix": "logs/",
            "delimiter": "/",
            "max_keys": 20,
            "key_marker": "logs/a.log",
            "version_id_marker": "v3"
        }"#,
        )
        .unwrap();
        assert_eq!(paged.prefix, Some("logs/".to_string()));
        assert_eq!(paged.delimiter, Some("/".to_string()));
        assert_eq!(paged.max_keys, Some(20));
        assert_eq!(paged.key_marker, Some("logs/a.log".to_string()));
        assert_eq!(paged.version_id_marker, Some("v3".to_string()));

        let delete: DeleteObjectVersionsRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "versions": [
                {"key": "a.txt", "version_id": "v2"},
                {"key": "a.txt", "version_id": "v1"}
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(delete.versions.len(), 2);
        assert_eq!(delete.versions[0].key, "a.txt");
        assert_eq!(delete.versions[1].version_id, "v1");
    }

    #[test]
    fn test_object_attributes_request_deserialization() {
        let minimal: GetObjectAttributesRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k"}"#).unwrap();
        assert_eq!(minimal.object_key, "k");
        assert!(minimal.version_id.is_none());

        let versioned: GetObjectAttributesRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k", "version_id": "v1"}"#)
                .unwrap();
        assert_eq!(versioned.version_id.as_deref(), Some("v1"));
    }
    #[test]
    fn test_head_object_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_key": "video.mp4"}"#;
        let request: HeadObjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.object_key, "video.mp4");
        assert!(request.version_id.is_none());
    }

    #[test]
    fn test_bucket_logging_request_deserialization() {
        let put: PutBucketLoggingRequest = serde_json::from_str(
            r#"{"bucket_name": "b", "target_bucket": "my-logs", "target_prefix": "logs/"}"#,
        )
        .unwrap();
        assert_eq!(put.bucket_name, "b");
        assert_eq!(put.target_bucket, "my-logs");
        assert_eq!(put.target_prefix.as_deref(), Some("logs/"));

        let put: PutBucketLoggingRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "target_bucket": "my-logs"}"#).unwrap();
        assert!(put.target_prefix.is_none());

        let get: GetBucketLoggingRequest = serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(get.bucket_name, "b");
        let delete: DeleteBucketLoggingRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(delete.bucket_name, "b");
    }
    #[test]
    fn test_object_tagging_request_deserialization() {
        let get: GetObjectTaggingRequest = serde_json::from_str(
            r#"{"bucket_name": "b", "object_key": "a.txt", "version_id": "v1"}"#,
        )
        .unwrap();
        assert_eq!(get.object_key, "a.txt");
        assert_eq!(get.version_id, Some("v1".to_string()));

        let put: PutObjectTaggingRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "object_key": "a.txt",
            "tags": {"team": "storage", "reviewed": ""}
        }"#,
        )
        .unwrap();
        assert_eq!(put.tags.get("team"), Some(&"storage".to_string()));
        assert_eq!(put.tags.get("reviewed"), Some(&String::new()));
        assert!(put.version_id.is_none());

        let delete: DeleteObjectTaggingRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "a.txt"}"#).unwrap();
        assert!(delete.version_id.is_none());

        // Tags are mandatory on a put request.
        assert!(
            serde_json::from_str::<PutObjectTaggingRequest>(
                r#"{"bucket_name": "b", "object_key": "a.txt"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn test_bucket_tagging_request_deserialization() {
        let get: GetBucketTaggingRequest = serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(get.bucket_name, "b");

        let put: PutBucketTaggingRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "tags": {"cost-center": "42"}}"#).unwrap();
        assert_eq!(put.tags.get("cost-center"), Some(&"42".to_string()));

        let delete: DeleteBucketTaggingRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(delete.bucket_name, "b");
    }

    #[test]
    fn test_bucket_website_request_deserialization() {
        let put: PutBucketWebsiteRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "index_document_suffix": "index.html",
            "error_document_key": "errors/404.html",
            "routing_rules": [
                {"key_prefix_equals": "docs/", "replace_key_prefix_with": "documents/"}
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(put.bucket_name, "b");
        assert_eq!(put.index_document_suffix.as_deref(), Some("index.html"));
        assert_eq!(put.routing_rules.len(), 1);
        assert!(
            put.redirect_all_requests_to.is_none(),
            "documents and a redirect cannot be asked for together"
        );

        let redirect: PutBucketWebsiteRequest = serde_json::from_str(
            r#"{"bucket_name": "b", "redirect_all_requests_to": {"host_name": "www.example.com", "protocol": "HTTPS"}}"#,
        )
        .unwrap();
        let redirect = redirect.redirect_all_requests_to.unwrap();
        assert_eq!(redirect.host_name, "www.example.com");
        assert_eq!(
            redirect.protocol,
            Some(crate::s3_client::WebsiteProtocol::Https)
        );

        let get: GetBucketWebsiteRequest = serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(get.bucket_name, "b");
        let delete: DeleteBucketWebsiteRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(delete.bucket_name, "b");
    }
    #[test]
    fn test_presign_url_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_key": "a/b.txt"}"#;
        let request: PresignUrlRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.method, PresignMethod::Get);
        assert_eq!(request.expires_in_secs, 3600);

        let full: PresignUrlRequest = serde_json::from_str(
            r#"{
            "bucket_name": "test-bucket",
            "object_key": "a/b.txt",
            "method": "put",
            "expires_in_secs": 86400
        }"#,
        )
        .unwrap();
        assert_eq!(full.method, PresignMethod::Put);
        assert_eq!(full.expires_in_secs, 86400);
    }

    #[test]
    fn test_presign_method_serialization() {
        let get_json = serde_json::to_string(&PresignMethod::Get).unwrap();
        let put_json = serde_json::to_string(&PresignMethod::Put).unwrap();

        assert_eq!(get_json, r#""get""#);
        assert_eq!(put_json, r#""put""#);

        let get: PresignMethod = serde_json::from_str(r#""get""#).unwrap();
        let put: PresignMethod = serde_json::from_str(r#""put""#).unwrap();

        assert_eq!(get, PresignMethod::Get);
        assert_eq!(put, PresignMethod::Put);
    }

    #[test]
    fn test_bucket_cors_put_request_deserialization() {
        let full: PutBucketCorsRequest = serde_json::from_str(
            r#"{
            "bucket_name": "test-bucket",
            "rules": [
                {
                    "id": "app-uploads",
                    "allowed_methods": ["GET", "PUT", "POST"],
                    "allowed_origins": ["https://app.example.com"],
                    "allowed_headers": ["Content-Type", "X-Amz-Date"],
                    "expose_headers": ["ETag"],
                    "max_age_seconds": 3000
                },
                {
                    "id": null,
                    "allowed_methods": ["GET"],
                    "allowed_origins": ["*"],
                    "allowed_headers": [],
                    "expose_headers": [],
                    "max_age_seconds": null
                }
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(full.bucket_name, "test-bucket");
        assert_eq!(full.rules.len(), 2);

        let first = &full.rules[0];
        assert_eq!(first.id.as_deref(), Some("app-uploads"));
        assert_eq!(
            first.allowed_methods,
            vec!["GET".to_string(), "PUT".to_string(), "POST".to_string()]
        );
        assert_eq!(
            first.allowed_origins,
            vec!["https://app.example.com".to_string()]
        );
        assert_eq!(
            first.allowed_headers,
            vec!["Content-Type".to_string(), "X-Amz-Date".to_string()]
        );
        assert_eq!(first.expose_headers, vec!["ETag".to_string()]);
        assert_eq!(first.max_age_seconds, Some(3000));

        let second = &full.rules[1];
        assert_eq!(second.id, None);
        assert_eq!(second.allowed_methods, vec!["GET".to_string()]);
        assert_eq!(second.allowed_origins, vec!["*".to_string()]);
        assert!(second.allowed_headers.is_empty());
        assert!(second.expose_headers.is_empty());
        assert_eq!(second.max_age_seconds, None);

        // allowed_headers and expose_headers are optional and default to empty.
        let minimal: PutBucketCorsRequest = serde_json::from_str(
            r#"{
            "bucket_name": "test-bucket",
            "rules": [
                {"allowed_methods": ["GET", "HEAD"], "allowed_origins": ["https://app.example.com"]}
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(minimal.rules.len(), 1);
        assert_eq!(minimal.rules[0].id, None);
        assert_eq!(
            minimal.rules[0].allowed_methods,
            vec!["GET".to_string(), "HEAD".to_string()]
        );
        assert!(minimal.rules[0].allowed_headers.is_empty());
        assert!(minimal.rules[0].expose_headers.is_empty());
        assert_eq!(minimal.rules[0].max_age_seconds, None);

        // allowed_methods and allowed_origins are mandatory for every rule.
        assert!(
            serde_json::from_str::<PutBucketCorsRequest>(
                r#"{"bucket_name": "test-bucket", "rules": [{"allowed_origins": ["*"]}]}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<PutBucketCorsRequest>(
                r#"{"bucket_name": "test-bucket", "rules": [{"allowed_methods": ["GET"]}]}"#
            )
            .is_err()
        );
    }

    #[test]
    fn test_bucket_cors_get_delete_request_deserialization() {
        let get: GetBucketCorsRequest =
            serde_json::from_str(r#"{"bucket_name": "test-bucket"}"#).unwrap();
        assert_eq!(get.bucket_name, "test-bucket");

        let delete: DeleteBucketCorsRequest =
            serde_json::from_str(r#"{"bucket_name": "test-bucket"}"#).unwrap();
        assert_eq!(delete.bucket_name, "test-bucket");
    }

    #[test]
    fn test_multipart_request_deserialization() {
        let init: InitiateMultipartUploadRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k"}"#).unwrap();
        assert!(init.content_type.is_none());
        assert!(init.metadata.is_none());

        let part: UploadPartRequest = serde_json::from_str(
            r#"{
            "local_file_path": "/tmp/part-1",
            "bucket_name": "b",
            "object_key": "k",
            "upload_id": "abc",
            "part_number": 1
        }"#,
        )
        .unwrap();
        assert_eq!(part.part_number, 1);
        assert_eq!(part.upload_id, "abc");

        let complete: CompleteMultipartUploadRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "object_key": "k",
            "upload_id": "abc",
            "parts": [
                {"part_number": 2, "etag": "\"e2\""},
                {"part_number": 1, "etag": "\"e1\""}
            ]
        }"#,
        )
        .unwrap();
        assert_eq!(complete.parts.len(), 2);
        assert_eq!(complete.parts[1].part_number, 1);

        let abort: AbortMultipartUploadRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k", "upload_id": "abc"}"#)
                .unwrap();
        assert_eq!(abort.upload_id, "abc");

        let list: ListPartsRequest =
            serde_json::from_str(r#"{"bucket_name": "b", "object_key": "k", "upload_id": "abc"}"#)
                .unwrap();
        assert_eq!(list.object_key, "k");
    }

    #[test]
    fn test_list_multipart_uploads_request_deserialization() {
        let minimal: ListMultipartUploadsRequest =
            serde_json::from_str(r#"{"bucket_name": "b"}"#).unwrap();
        assert_eq!(minimal.bucket_name, "b");
        assert!(minimal.prefix.is_none());
        assert!(minimal.delimiter.is_none());
        assert!(minimal.max_uploads.is_none());
        assert!(minimal.key_marker.is_none());
        assert!(minimal.upload_id_marker.is_none());

        let full: ListMultipartUploadsRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "prefix": "videos/",
            "delimiter": "/",
            "max_uploads": 20,
            "key_marker": "videos/b.mp4",
            "upload_id_marker": "upl-2"
        }"#,
        )
        .unwrap();
        assert_eq!(full.bucket_name, "b");
        assert_eq!(full.prefix, Some("videos/".to_string()));
        assert_eq!(full.delimiter, Some("/".to_string()));
        assert_eq!(full.max_uploads, Some(20));
        assert_eq!(full.key_marker, Some("videos/b.mp4".to_string()));
        assert_eq!(full.upload_id_marker, Some("upl-2".to_string()));
    }

    #[test]
    fn test_upload_part_copy_request_deserialization() {
        let full: UploadPartCopyRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "object_key": "assembled.bin",
            "upload_id": "abc",
            "part_number": 1,
            "source_bucket": "src",
            "source_key": "source.bin",
            "source_version_id": "v1",
            "range": "0-6291455"
        }"#,
        )
        .unwrap();
        assert_eq!(full.bucket_name, "b");
        assert_eq!(full.object_key, "assembled.bin");
        assert_eq!(full.upload_id, "abc");
        assert_eq!(full.part_number, 1);
        assert_eq!(full.source_bucket, "src");
        assert_eq!(full.source_key, "source.bin");
        assert_eq!(full.source_version_id, Some("v1".to_string()));
        assert_eq!(full.range, Some("0-6291455".to_string()));

        let minimal: UploadPartCopyRequest = serde_json::from_str(
            r#"{
            "bucket_name": "b",
            "object_key": "assembled.bin",
            "upload_id": "abc",
            "part_number": 2,
            "source_bucket": "src",
            "source_key": "source.bin"
        }"#,
        )
        .unwrap();
        assert_eq!(minimal.part_number, 2);
        assert!(minimal.source_version_id.is_none());
        assert!(minimal.range.is_none());
    }

    #[test]
    fn test_upload_file_request_metadata_deserialization() {
        let json = r#"{
            "local_file_path": "/tmp/file.bin",
            "bucket_name": "b",
            "object_key": "k",
            "metadata": {"owner": "rustfs"},
            "server_side_encryption": "AES256",
            "content_disposition": "attachment",
            "content_encoding": "gzip",
            "content_language": "en-US"
        }"#;
        let request: UploadFileRequest = serde_json::from_str(json).unwrap();
        assert_eq!(
            request.metadata.as_ref().unwrap().get("owner"),
            Some(&"rustfs".to_string())
        );
        assert_eq!(request.server_side_encryption, Some("AES256".to_string()));
        assert_eq!(request.content_encoding, Some("gzip".to_string()));
    }

    #[test]
    fn test_upload_content_request_deserialization() {
        let json = r#"{
            "content": "{\"total\": 3}\n",
            "bucket_name": "test-bucket",
            "object_key": "reports/today.json",
            "content_type": "application/json",
            "metadata": {"source": "agent"}
        }"#;
        let request: UploadContentRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.content, "{\"total\": 3}\n");
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_key, "reports/today.json");
        assert_eq!(request.content_type, Some("application/json".to_string()));
        assert_eq!(
            request.metadata.as_ref().and_then(|m| m.get("source")),
            Some(&"agent".to_string())
        );
        assert!(request.storage_class.is_none());
        assert!(request.cache_control.is_none());
        assert!(request.server_side_encryption.is_none());

        let minimal: UploadContentRequest =
            serde_json::from_str(r#"{"content": "hi", "bucket_name": "b", "object_key": "k"}"#)
                .unwrap();
        assert_eq!(minimal.content, "hi");
        assert!(minimal.content_type.is_none());
        assert!(minimal.metadata.is_none());
    }
}
