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

use aws_sdk_s3::operation::put_object::builders::PutObjectFluentBuilder;

use super::types::UploadFileOptions;

/// Apply the object-level upload options — storage class, standard content
/// headers, server-side encryption, and user metadata — to a `PutObject`
/// request.
pub(super) fn apply_put_object_options(
    mut request: PutObjectFluentBuilder,
    options: &UploadFileOptions,
) -> PutObjectFluentBuilder {
    if let Some(storage_class) = &options.storage_class {
        request = request.storage_class(storage_class.as_str().into());
    }

    if let Some(cache_control) = &options.cache_control {
        request = request.cache_control(cache_control);
    }

    if let Some(content_disposition) = &options.content_disposition {
        request = request.content_disposition(content_disposition);
    }

    if let Some(content_encoding) = &options.content_encoding {
        request = request.content_encoding(content_encoding);
    }

    if let Some(content_language) = &options.content_language {
        request = request.content_language(content_language);
    }

    if let Some(sse) = &options.server_side_encryption {
        request = request.server_side_encryption(sse.as_str().into());
    }

    if let Some(metadata_map) = &options.metadata {
        for (key, value) in metadata_map {
            request = request.metadata(key, value);
        }
    }

    request
}
