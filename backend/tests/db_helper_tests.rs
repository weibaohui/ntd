//! Tests for database helper functions

// 测试代码允许 unwrap/expect/panic 等写法以简化断言逻辑，统一放宽以下 clippy 检查
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::useless_vec, clippy::redundant_pattern_matching, clippy::redundant_clone, clippy::len_zero, clippy::bool_assert_comparison, clippy::unnecessary_get_then_check, clippy::doc_lazy_continuation, clippy::clone_on_copy, clippy::print_stdout, clippy::needless_pass_by_value, clippy::sliced_string_as_bytes, clippy::manual_map, clippy::collapsible_match, clippy::question_mark)]
#[cfg(test)]
mod unique_constraint_tests {
    fn is_unique_constraint_error(err: &sea_orm::DbErr) -> bool {
        let err_str = format!("{:?}", err);
        err_str.contains("UNIQUE constraint failed")
    }

    #[test]
    fn test_is_unique_constraint_error_with_unique_constraint() {
        let err = sea_orm::DbErr::Query(sea_orm::RuntimeErr::Internal("UNIQUE constraint failed: project_directories.path".to_string()));
        assert!(is_unique_constraint_error(&err));
    }

    #[test]
    fn test_is_unique_constraint_error_without_unique() {
        let err = sea_orm::DbErr::Query(sea_orm::RuntimeErr::Internal("Foreign key constraint failed".to_string()));
        assert!(!is_unique_constraint_error(&err));
    }

    #[test]
    fn test_is_unique_constraint_error_record_not_found() {
        let err = sea_orm::DbErr::RecordNotFound("Record not found".to_string());
        assert!(!is_unique_constraint_error(&err));
    }
}
