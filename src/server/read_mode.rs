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

use crate::s3_client::{DetectedFileType, GetObjectOptions};
use tracing::{debug, error};

use super::RustfsMcpServer;
use super::requests::GetObjectRequest;

impl RustfsMcpServer {
    pub(super) async fn handle_read_mode(&self, req: GetObjectRequest) -> String {
        let options = GetObjectOptions {
            version_id: req.version_id.clone(),
            range: req.range.clone(),
            if_modified_since: req.if_modified_since.clone(),
            if_unmodified_since: req.if_unmodified_since.clone(),
            max_content_size: Some(req.max_content_size),
        };

        match self
            .s3_client
            .get_object(&req.bucket_name, &req.object_key, options)
            .await
        {
            Ok(result) => {
                debug!(
                    "Successfully retrieved object s3://{}/{} ({} bytes)",
                    req.bucket_name, req.object_key, result.content_length
                );

                match result.detected_type {
                    DetectedFileType::Text => {
                        if let Some(ref text_content) = result.text_content {
                            let mut result_text = format!(
                                "✅ **Text file content retrieved!**\n\n\
                                 **S3 Location:** s3://{}/{}\n",
                                result.bucket, result.key
                            );
                            if let Some(ref range) = req.range {
                                result_text
                                    .push_str(&format!("**Range:** {range} (partial read)\n"));
                            }
                            result_text.push_str(&format!(
                                "**File Size:** {} bytes\n\
                                 **Content Type:** {}\n\n\
                                 **Content:**\n```\n{}\n```",
                                result.content_length, result.content_type, text_content
                            ));
                            result_text
                        } else {
                            format!(
                                "⚠️ **Text file detected but content could not be decoded!**\n\n\
                                 **S3 Location:** s3://{}/{}\n\
                                 **File Size:** {} bytes\n\
                                 **Content Type:** {}\n\n\
                                 **Note:** Could not decode file as UTF-8 text. \
                                 Try using download mode instead.",
                                result.bucket,
                                result.key,
                                result.content_length,
                                result.content_type
                            )
                        }
                    }
                    DetectedFileType::NonText(ref mime_type) => {
                        let file_category = if mime_type.starts_with("image/") {
                            "Image"
                        } else if mime_type.starts_with("audio/") {
                            "Audio"
                        } else if mime_type.starts_with("video/") {
                            "Video"
                        } else {
                            "Binary"
                        };

                        format!(
                            "⚠️ **Non-text file detected!**\n\n\
                             **S3 Location:** s3://{}/{}\n\
                             **File Type:** {} ({})\n\
                             **File Size:** {} bytes ({:.2} MB)\n\n\
                             **Note:** This file type cannot be displayed as text.\n\
                             Please use download mode to save it to a local file:\n\n\
                             ```json\n{{\n  \"mode\": \"download\",\n  \"local_path\": \"/path/to/save/file\"\n}}\n```",
                            result.bucket,
                            result.key,
                            file_category,
                            mime_type,
                            result.content_length,
                            result.content_length as f64 / 1_048_576.0
                        )
                    }
                }
            }
            Err(e) => {
                error!(
                    "Failed to read object s3://{}/{}: {:?}",
                    req.bucket_name, req.object_key, e
                );
                self.format_error_message(&req, e)
            }
        }
    }
}
