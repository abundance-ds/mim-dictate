//! Keeps the process out of App Nap while transcription runs. macOS throttles
//! background CPU work in hidden menu-bar apps, which can stretch a 3-second
//! transcription into 30+ seconds until the user clicks the tray icon.

#[cfg(target_os = "macos")]
pub struct ActivityGuard {
    process_info: objc2::rc::Retained<objc2_foundation::NSProcessInfo>,
    activity: objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2::runtime::NSObjectProtocol>>,
}

#[cfg(target_os = "macos")]
impl ActivityGuard {
    pub fn begin(reason: &str) -> Self {
        use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

        let process_info = NSProcessInfo::processInfo();
        let activity = process_info.beginActivityWithOptions_reason(
            NSActivityOptions::UserInitiated,
            &NSString::from_str(reason),
        );
        Self {
            process_info,
            activity,
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for ActivityGuard {
    fn drop(&mut self) {
        unsafe { self.process_info.endActivity(&self.activity) };
    }
}

#[cfg(not(target_os = "macos"))]
pub struct ActivityGuard;

#[cfg(not(target_os = "macos"))]
impl ActivityGuard {
    pub fn begin(_reason: &str) -> Self {
        Self
    }
}
