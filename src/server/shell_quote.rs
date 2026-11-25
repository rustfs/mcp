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

/// Wrap a value in POSIX single quotes so it is safe to embed in the shell
/// examples we print. An apostrophe inside `'...'` would terminate the
/// quoted string, so it is replaced with the `'\''` escape sequence.
pub(super) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_single_quote() {
        assert_eq!(shell_single_quote("plain.txt"), "'plain.txt'");
        assert_eq!(shell_single_quote("it's.png"), "'it'\\''s.png'");
        assert_eq!(
            shell_single_quote("a'$(printf injected)'.txt"),
            "'a'\\''$(printf injected)'\\''.txt'"
        );
    }
}
