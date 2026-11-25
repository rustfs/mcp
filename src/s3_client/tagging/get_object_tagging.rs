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

use anyhow::{Context, Result};
use aws_sdk_s3::error::ProvideErrorMetadata;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::debug;

use super::super::S3Client;
use super::{UNTAGGED_ERROR_CODE, tag_set_from_sdk};

/// Result payload for reading an object version's tag set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectTags {
    pub bucket: String,
    pub key: String,
    /// Version whose tags were read; the current version when unset.
    pub version_id: Option<String>,
    /// Empty when the object version has no tags; sorted by key.
    pub tags: BTreeMap<String, String>,
}

impl S3Client {
    /// Read the tag set of an object, or of one specific version of it.
    pub async fn get_object_tagging(
        &self,
        bucket_name: &str,
        object_key: &str,
        version_id: Option<&str>,
    ) -> Result<ObjectTags> {
        debug!(
            "Fetching tags for object: s3://{}/{}",
            bucket_name, object_key
        );

        let mut request = self
            .client
            .get_object_tagging()
            .bucket(bucket_name)
            .key(object_key);

        if let Some(version_id) = version_id {
            request = request.version_id(version_id);
        }

        let response = match request.send().await {
            Ok(response) => response,
            // An object that was never tagged answers with a dedicated error;
            // report that as an empty tag set instead of a failure.
            Err(err)
                if err.as_service_error().and_then(|e| e.code()) == Some(UNTAGGED_ERROR_CODE) =>
            {
                return Ok(ObjectTags {
                    bucket: bucket_name.to_string(),
                    key: object_key.to_string(),
                    version_id: version_id.map(|v| v.to_string()),
                    tags: BTreeMap::new(),
                });
            }
            Err(err) => {
                return Err(err).context(format!(
                    "Failed to get the tags of s3://{bucket_name}/{object_key}"
                ));
            }
        };

        Ok(ObjectTags {
            bucket: bucket_name.to_string(),
            key: object_key.to_string(),
            // Prefer the version S3 actually read: it is the current one
            // when no version was requested.
            version_id: response
                .version_id()
                .map(|v| v.to_string())
                .or_else(|| version_id.map(|v| v.to_string())),
            tags: tag_set_from_sdk(response.tag_set()),
        })
    }
}
