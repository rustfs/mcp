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

use super::S3Client;
use super::types::DetectedFileType;

impl S3Client {
    /// Classify an object's content by content type, magic bytes, and
    /// UTF-8 heuristics.
    pub(super) fn detect_file_type(
        content_type: Option<&str>,
        content_bytes: &[u8],
    ) -> DetectedFileType {
        if let Some(ct) = content_type {
            let ct_lower = ct.to_lowercase();

            if ct_lower.starts_with("text/")
                || ct_lower == "application/json"
                || ct_lower == "application/xml"
                || ct_lower == "application/yaml"
                || ct_lower == "application/javascript"
                || ct_lower == "application/x-yaml"
                || ct_lower == "application/x-sh"
                || ct_lower == "application/x-shellscript"
                || ct_lower.contains("script")
                || ct_lower.contains("xml")
                || ct_lower.contains("json")
            {
                return DetectedFileType::Text;
            }

            return DetectedFileType::NonText(ct.to_string());
        }

        if content_bytes.len() >= 4 {
            match &content_bytes[0..4] {
                // PNG: 89 50 4E 47
                [0x89, 0x50, 0x4E, 0x47] => {
                    return DetectedFileType::NonText("image/png".to_string());
                }
                // JPEG: FF D8 FF
                [0xFF, 0xD8, 0xFF, _] => {
                    return DetectedFileType::NonText("image/jpeg".to_string());
                }
                // GIF: 47 49 46 38
                [0x47, 0x49, 0x46, 0x38] => {
                    return DetectedFileType::NonText("image/gif".to_string());
                }
                // BMP: 42 4D
                [0x42, 0x4D, _, _] => return DetectedFileType::NonText("image/bmp".to_string()),
                // RIFF container (WebP/WAV)
                [0x52, 0x49, 0x46, 0x46] if content_bytes.len() >= 12 => {
                    if &content_bytes[8..12] == b"WEBP" {
                        return DetectedFileType::NonText("image/webp".to_string());
                    } else if &content_bytes[8..12] == b"WAVE" {
                        return DetectedFileType::NonText("audio/wav".to_string());
                    }
                    return DetectedFileType::NonText("application/octet-stream".to_string());
                }
                _ => {}
            }
        }

        // 3. Check if content is valid UTF-8 text as fallback
        if std::str::from_utf8(content_bytes).is_ok() {
            // Additional heuristics for text detection
            let non_printable_count = content_bytes
                .iter()
                .filter(|&&b| b < 0x20 && b != 0x09 && b != 0x0A && b != 0x0D) // Control chars except tab, LF, CR
                .count();
            let total_chars = content_bytes.len();

            // If less than 5% are non-printable control characters, consider it text
            if total_chars > 0 && (non_printable_count as f64 / total_chars as f64) < 0.05 {
                return DetectedFileType::Text;
            }
        }

        // Default to non-text binary
        DetectedFileType::NonText("application/octet-stream".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_file_type_text_content_type() {
        let test_cases = vec![
            ("text/plain", "Hello world"),
            ("text/html", "<html></html>"),
            ("application/json", r#"{"key": "value"}"#),
            ("application/xml", "<xml></xml>"),
            ("application/yaml", "key: value"),
            ("application/javascript", "console.log('hello');"),
        ];

        for (content_type, content) in test_cases {
            let result = S3Client::detect_file_type(Some(content_type), content.as_bytes());
            match result {
                DetectedFileType::Text => {}
                _ => panic!("Expected Text for content type {content_type}"),
            }
        }
    }

    #[test]
    fn test_detect_file_type_non_text_content_type() {
        // Test various non-text content types
        let test_cases = vec![
            ("image/png", "image/png"),
            ("image/jpeg", "image/jpeg"),
            ("audio/mp3", "audio/mp3"),
            ("video/mp4", "video/mp4"),
            ("application/pdf", "application/pdf"),
        ];

        for (content_type, expected_mime) in test_cases {
            let result = S3Client::detect_file_type(Some(content_type), b"some content");
            match result {
                DetectedFileType::NonText(mime_type) => {
                    assert_eq!(mime_type, expected_mime);
                }
                _ => panic!("Expected NonText for content type {content_type}"),
            }
        }
    }

    #[test]
    fn test_detect_file_type_magic_bytes_simplified() {
        // Test magic bytes detection (now all return NonText)
        let test_cases = vec![
            // PNG magic bytes: 89 50 4E 47
            (
                vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
                "image/png",
            ),
            // JPEG magic bytes: FF D8 FF
            (vec![0xFF, 0xD8, 0xFF, 0xE0], "image/jpeg"),
            // GIF magic bytes: 47 49 46 38
            (vec![0x47, 0x49, 0x46, 0x38, 0x37, 0x61], "image/gif"),
        ];

        for (content, expected_mime) in test_cases {
            let result = S3Client::detect_file_type(None, &content);
            match result {
                DetectedFileType::NonText(mime_type) => {
                    assert_eq!(mime_type, expected_mime);
                }
                _ => panic!("Expected NonText for magic bytes: {content:?}"),
            }
        }
    }

    #[test]
    fn test_detect_file_type_webp_magic_bytes() {
        // WebP has more complex magic bytes: RIFF....WEBP
        let mut webp_content = vec![0x52, 0x49, 0x46, 0x46]; // RIFF
        webp_content.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Size (4 bytes)
        webp_content.extend_from_slice(b"WEBP"); // WEBP signature

        let result = S3Client::detect_file_type(None, &webp_content);
        match result {
            DetectedFileType::NonText(mime_type) => {
                assert_eq!(mime_type, "image/webp");
            }
            _ => panic!("Expected WebP NonText detection"),
        }
    }

    #[test]
    fn test_detect_file_type_wav_magic_bytes() {
        // WAV has magic bytes: RIFF....WAVE
        let mut wav_content = vec![0x52, 0x49, 0x46, 0x46]; // RIFF
        wav_content.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // Size (4 bytes)
        wav_content.extend_from_slice(b"WAVE"); // WAVE signature

        let result = S3Client::detect_file_type(None, &wav_content);
        match result {
            DetectedFileType::NonText(mime_type) => {
                assert_eq!(mime_type, "audio/wav");
            }
            _ => panic!("Expected WAV NonText detection"),
        }
    }

    #[test]
    fn test_detect_file_type_utf8_text() {
        // Test UTF-8 text detection
        let utf8_content = "Hello, World! 🌍".as_bytes();
        let result = S3Client::detect_file_type(None, utf8_content);
        match result {
            DetectedFileType::Text => {}
            _ => panic!("Expected Text for UTF-8 content"),
        }

        // Test ASCII text
        let ascii_content = b"Hello, world! This is ASCII text.";
        let result = S3Client::detect_file_type(None, ascii_content);
        match result {
            DetectedFileType::Text => {}
            _ => panic!("Expected Text for ASCII content"),
        }
    }

    #[test]
    fn test_detect_file_type_binary() {
        // Test binary content that should not be detected as text
        let binary_content = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD, 0xFC];
        let result = S3Client::detect_file_type(None, &binary_content);
        match result {
            DetectedFileType::NonText(mime_type) => {
                assert_eq!(mime_type, "application/octet-stream");
            }
            _ => panic!("Expected NonText for binary content"),
        }
    }

    #[test]
    fn test_detect_file_type_priority() {
        // Content-Type should take priority over magic bytes
        let png_magic_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

        // Even with PNG magic bytes, text content-type should win
        let result = S3Client::detect_file_type(Some("text/plain"), &png_magic_bytes);
        match result {
            DetectedFileType::Text => {}
            _ => panic!("Expected Text due to content-type priority"),
        }
    }
}
