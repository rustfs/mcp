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

/// Request payload for deleting a bucket.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteBucketRequest {
    #[schemars(description = "Name of the S3 bucket to delete")]
    pub bucket_name: String,
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

/// Request payload for batch-deleting objects.
#[derive(Serialize, Deserialize, JsonSchema)]
pub struct DeleteObjectsRequest {
    #[schemars(description = "Name of the S3 bucket")]
    pub bucket_name: String,
    #[schemars(description = "Object keys to delete permanently (up to 1000 keys per request)")]
    pub object_keys: Vec<String>,
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
    fn test_bucket_creation() {
        let request = CreateBucketRequest {
            bucket_name: "test-bucket".to_string(),
        };
        assert_eq!(request.bucket_name, "test-bucket");
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
    fn test_delete_objects_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_keys": ["a.txt", "b/c.txt"]}"#;
        let request: DeleteObjectsRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.bucket_name, "test-bucket");
        assert_eq!(request.object_keys.len(), 2);
        assert_eq!(request.object_keys[1], "b/c.txt");
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
    }

    #[test]
    fn test_head_object_request_deserialization() {
        let json = r#"{"bucket_name": "test-bucket", "object_key": "video.mp4"}"#;
        let request: HeadObjectRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.object_key, "video.mp4");
        assert!(request.version_id.is_none());
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
