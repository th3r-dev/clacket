//! Force Touch trackpad tick. macOS only; a no-op elsewhere.

#[cfg(target_os = "macos")]
pub const SUPPORTED: bool = true;
#[cfg(not(target_os = "macos"))]
pub const SUPPORTED: bool = false;

/// macOS may ignore this while another app is focused. That is an OS rule, so it has to be
/// checked on real hardware; see README "Haptic tick".
#[cfg(target_os = "macos")]
pub fn tick() {
    use objc2_app_kit::{NSHapticFeedbackManager, NSHapticFeedbackPattern, NSHapticFeedbackPerformanceTime, NSHapticFeedbackPerformer};
    NSHapticFeedbackManager::defaultPerformer()
        .performFeedbackPattern_performanceTime(NSHapticFeedbackPattern::Generic, NSHapticFeedbackPerformanceTime::Now);
}

#[cfg(not(target_os = "macos"))]
pub fn tick() {}
