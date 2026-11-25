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

use serde::{Deserialize, Serialize};

/// Basic S3 bucket information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketInfo {
    pub name: String,
    pub creation_date: Option<String>,
}

/// Basic S3 object metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfo {
    pub key: String,
    pub size: Option<i64>,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    pub storage_class: Option<String>,
}

/// Options for `list_objects_v2`.
#[derive(Debug, Clone, Default)]
pub struct ListObjectsOptions {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_keys: Option<i32>,
    pub continuation_token: Option<String>,
    pub start_after: Option<String>,
}

/// Result payload for `list_objects_v2`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListObjectsResult {
    pub objects: Vec<ObjectInfo>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
    pub max_keys: Option<i32>,
    pub key_count: i32,
}

/// Options for uploading a local file to S3.
#[derive(Debug, Clone, Default)]
pub struct UploadFileOptions {
    pub content_type: Option<String>,
    pub metadata: Option<std::collections::HashMap<String, String>>,
    pub storage_class: Option<String>,
    pub server_side_encryption: Option<String>,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub content_encoding: Option<String>,
    pub content_language: Option<String>,
}

/// Options for reading/downloading an object from S3.
#[derive(Debug, Clone, Default)]
pub struct GetObjectOptions {
    pub version_id: Option<String>,
    pub range: Option<String>,
    pub if_modified_since: Option<String>,
    pub if_unmodified_since: Option<String>,
    pub max_content_size: Option<usize>,
}

/// File type classification used by get-object operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DetectedFileType {
    Text,
    NonText(String), // mime type for non-text files
}

/// Result payload for object retrieval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetObjectResult {
    pub bucket: String,
    pub key: String,
    pub content_type: String,
    pub content_length: u64,
    pub last_modified: Option<String>,
    pub etag: Option<String>,
    pub version_id: Option<String>,
    pub detected_type: DetectedFileType,
    pub content: Option<Vec<u8>>,     // Raw content bytes
    pub text_content: Option<String>, // UTF-8 decoded content for text files
}

/// Result payload for file upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResult {
    pub bucket: String,
    pub key: String,
    pub etag: String,
    pub location: String,
    pub version_id: Option<String>,
    pub file_size: u64,
    pub content_type: String,
    pub upload_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_info_serialization() {
        let bucket = BucketInfo {
            name: "test-bucket".to_string(),
            creation_date: Some("2024-01-01T00:00:00Z".to_string()),
        };

        let json = serde_json::to_string(&bucket).unwrap();
        let deserialized: BucketInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(bucket.name, deserialized.name);
        assert_eq!(bucket.creation_date, deserialized.creation_date);
    }

    #[test]
    fn test_get_object_options_default() {
        let options = GetObjectOptions::default();
        assert!(options.version_id.is_none());
        assert!(options.range.is_none());
        assert!(options.if_modified_since.is_none());
        assert!(options.if_unmodified_since.is_none());
        assert!(options.max_content_size.is_none());
    }

    #[test]
    fn test_list_objects_options_default() {
        let options = ListObjectsOptions::default();
        assert!(options.prefix.is_none());
        assert!(options.delimiter.is_none());
        assert!(options.max_keys.is_none());
        assert!(options.continuation_token.is_none());
        assert!(options.start_after.is_none());
    }

    #[test]
    fn test_detected_file_type_serialization() {
        let test_cases = vec![
            DetectedFileType::Text,
            DetectedFileType::NonText("image/png".to_string()),
            DetectedFileType::NonText("audio/mpeg".to_string()),
            DetectedFileType::NonText("application/octet-stream".to_string()),
        ];

        for file_type in test_cases {
            let json = serde_json::to_string(&file_type).unwrap();
            let deserialized: DetectedFileType = serde_json::from_str(&json).unwrap();

            match (&file_type, &deserialized) {
                (DetectedFileType::Text, DetectedFileType::Text) => {}
                (DetectedFileType::NonText(a), DetectedFileType::NonText(b)) => assert_eq!(a, b),
                _ => panic!("Serialization/deserialization mismatch"),
            }
        }
    }
}
