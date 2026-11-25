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

use crate::s3_client::GetObjectOptions;
use tracing::{error, info};

use super::RustfsMcpServer;
use super::requests::GetObjectRequest;

impl RustfsMcpServer {
    pub(super) async fn handle_download_mode(&self, req: GetObjectRequest) -> String {
        let local_path = match req.local_path {
            Some(ref path) => path,
            None => {
                return "❌ **Error:** local_path is required when using download mode.\n\n\
                        **Example:**\n```json\n{\n  \"mode\": \"download\",\n  \"local_path\": \"/path/to/save/file.ext\"\n}\n```"
                    .to_string();
            }
        };

        let options = GetObjectOptions {
            version_id: req.version_id.clone(),
            range: req.range.clone(),
            if_modified_since: req.if_modified_since.clone(),
            if_unmodified_since: req.if_unmodified_since.clone(),
            ..GetObjectOptions::default()
        };

        match self
            .s3_client
            .download_object_to_file(&req.bucket_name, &req.object_key, local_path, options)
            .await
        {
            Ok((bytes_downloaded, absolute_path)) => {
                info!(
                    "Successfully downloaded object s3://{}/{} to {} ({} bytes)",
                    req.bucket_name, req.object_key, absolute_path, bytes_downloaded
                );

                let mut result_text = format!(
                    "✅ **File downloaded successfully!**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Local Path (requested):** {}\n\
                     **Absolute Path:** {}\n",
                    req.bucket_name, req.object_key, local_path, absolute_path
                );
                if let Some(ref range) = req.range {
                    result_text.push_str(&format!("**Range:** {range} (partial read)\n"));
                }
                result_text.push_str(&format!(
                    "**File Size:** {} bytes ({:.2} MB)\n\n\
                     **✨ File saved successfully!** You can now access it at:\n\
                     `{}`",
                    bytes_downloaded,
                    bytes_downloaded as f64 / 1_048_576.0,
                    absolute_path
                ));
                result_text
            }
            Err(e) => {
                error!(
                    "Failed to download object s3://{}/{} to {}: {:?}",
                    req.bucket_name, req.object_key, local_path, e
                );

                format!(
                    "❌ **Failed to download file from S3**\n\n\
                     **S3 Location:** s3://{}/{}\n\
                     **Local Path:** {}\n\
                     **Error:** {}\n\n\
                     **Possible causes:**\n\
                     • Object does not exist in the specified bucket\n\
                     • AWS credentials lack permissions to read this object\n\
                     • Cannot write to the specified local path\n\
                     • Insufficient disk space\n\
                     • Network connectivity issues\n\n\
                     **Troubleshooting steps:**\n\
                     1. Verify the object exists using list_objects\n\
                     2. Check your AWS credentials and permissions\n\
                     3. Ensure the local directory exists and is writable\n\
                     4. Check available disk space",
                    req.bucket_name, req.object_key, local_path, e
                )
            }
        }
    }
}
