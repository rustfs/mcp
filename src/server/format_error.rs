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

use super::RustfsMcpServer;
use super::requests::GetObjectRequest;

impl RustfsMcpServer {
    pub(super) fn format_error_message(
        &self,
        req: &GetObjectRequest,
        error: anyhow::Error,
    ) -> String {
        format!(
            "❌ **Failed to get object from S3 bucket '{}'**\n\n\
             **Object Key:** {}\n\
             **Mode:** {:?}\n\
             **Error:** {}\n\n\
             **Possible causes:**\n\
             • Object does not exist in the specified bucket\n\
             • AWS credentials lack permissions to read this object\n\
             • Network connectivity issues\n\
             • Object key contains invalid characters\n\
             • Bucket does not exist or is not accessible\n\
             • Object is in a different AWS region\n\
             • Version ID is invalid (for versioned objects)\n\
             • Range is invalid or outside the object bounds\n\
             • Conditional read failed: the object's modified state does not match if_modified_since / if_unmodified_since\n\n\
             **Troubleshooting steps:**\n\
             1. Verify the object exists using list_objects\n\
             2. Check your AWS credentials and permissions\n\
             3. Ensure the bucket name and object key are correct\n\
             4. Try with a different object to test connectivity\n\
             5. Check if the bucket has versioning enabled",
            req.bucket_name, req.object_key, req.mode, error
        )
    }
}
