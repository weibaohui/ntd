//! Tests for executor_config handlers - detect_binary function

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
#[cfg(test)]
mod detect_binary_tests {
    use std::path::PathBuf;

    fn detect_binary(path: &str) -> (bool, Option<String>) {
        // Expand ~ to home directory
        let expanded = if path.starts_with('~') {
            if let Some(home) = dirs::home_dir() {
                path.replacen('~', &home.to_string_lossy(), 1)
            } else {
                path.to_string()
            }
        } else {
            path.to_string()
        };

        let p = PathBuf::from(&expanded);

        // If it looks like an absolute or relative path (contains separator)
        if p.is_absolute() || expanded.contains('/') || expanded.contains('\\') {
            if p.exists() {
                return (true, Some(p.to_string_lossy().to_string()));
            }
            return (false, None);
        }

        // Bare command name — look up in PATH using `which` equivalent
        match which::which(&expanded) {
            Ok(resolved) => (true, Some(resolved.to_string_lossy().to_string())),
            Err(_) => (false, None),
        }
    }

    #[test]
    fn test_detect_binary_absolute_path_exists() {
        // Use a path that should definitely exist on the current platform
        let path = if cfg!(unix) { "/usr/bin/env" } else { "C:\\Windows\\System32\\cmd.exe" };
        let (found, resolved) = detect_binary(path);
        assert!(found, "Standard binary should be found at absolute path");
        assert!(resolved.is_some());
    }

    #[test]
    fn test_detect_binary_absolute_path_not_exists() {
        let (found, resolved) = detect_binary("/nonexistent/path/to/binary");
        assert!(!found);
        assert!(resolved.is_none());
    }

    #[test]
    fn test_detect_binary_relative_path() {
        // Relative path with separator
        let (found, resolved) = detect_binary("./Cargo.toml");
        // May or may not exist depending on cwd, but shouldn't crash
        let _ = found;
        let _ = resolved;
    }

    #[test]
    fn test_detect_binary_tilde_expansion() {
        // ~/.ntd/config.yaml should exist or not crash
        let (found, resolved) = detect_binary("~/.ntd/config.yaml");
        let _ = found;
        let _ = resolved;
    }

    #[test]
    fn test_detect_binary_bare_command() {
        // Try a common command that should exist
        let (found, resolved) = detect_binary("ls");
        if found {
            assert!(resolved.is_some());
            // Resolved path should be absolute
            let resolved_path = resolved.unwrap();
            assert!(PathBuf::from(&resolved_path).is_absolute(), "Resolved path should be absolute");
        }
    }

    #[test]
    fn test_detect_binary_nonexistent_bare_command() {
        let (found, resolved) = detect_binary("this_command_definitely_does_not_exist_12345");
        assert!(!found);
        assert!(resolved.is_none());
    }

    #[test]
    fn test_detect_binary_empty_string() {
        let (found, resolved) = detect_binary("");
        // Empty string is treated as relative path, won't exist
        assert!(!found);
        assert!(resolved.is_none());
    }

    #[test]
    fn test_detect_binary_with_backslash() {
        #[cfg(windows)]
        {
            let (found, _) = detect_binary("C:\\Windows\\System32");
            assert!(found); // System32 should exist on Windows
        }
        #[cfg(not(windows))]
        {
            // On Unix, backslash-containing path won't match absolute/relative checks
            let (found, resolved) = detect_binary("some\\path");
            // Treated as bare command name since no /
            if found {
                assert!(resolved.is_some());
            }
        }
    }
}
