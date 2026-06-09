// =============================================================================
// Shared application constants — single source of truth for values used across
// multiple layers (frontend components, backend config). Keep in sync with
// backend/src/config.rs::DEFAULT_EXECUTION_TIMEOUT_SECS.
// =============================================================================

/** Default execution timeout in seconds (1 hour). Used as fallback when no config is set. */
export const DEFAULT_EXECUTION_TIMEOUT_SECS = 3600;

/** Maximum execution timeout in minutes (7 days = 10080 min).
 * Derived from backend config::MAX_EXECUTION_TIMEOUT_SECS (604800 seconds).
 * If you change one side, update the other: MAX_EXECUTION_TIMEOUT_SECS / 60 = 10080. */
export const MAX_EXECUTION_TIMEOUT_MINUTES = 10080;
