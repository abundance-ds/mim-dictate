use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct PermissionStatus {
    pub microphone: String,
    pub accessibility: bool,
    pub input_monitoring: bool,
}

#[cfg(target_os = "macos")]
extern "C" {
    fn mim_mic_permission_status() -> i32;
    fn mim_request_mic_permission(callback: extern "C" fn(i32));
    fn mim_accessibility_status() -> i32;
    fn mim_request_accessibility_permission() -> i32;
    fn mim_input_monitoring_status() -> i32;
    fn mim_request_input_monitoring_permission() -> i32;
}

#[cfg(target_os = "macos")]
pub fn check() -> PermissionStatus {
    let mic_status = unsafe { mim_mic_permission_status() };
    let accessibility = unsafe { mim_accessibility_status() } != 0;
    let input_monitoring = unsafe { mim_input_monitoring_status() } != 0;

    PermissionStatus {
        microphone: match mic_status {
            0 => "not_determined",
            1 => "restricted",
            2 => "denied",
            3 => "authorized",
            _ => "unknown",
        }
        .to_string(),
        accessibility,
        input_monitoring,
    }
}

#[cfg(target_os = "macos")]
pub fn request_mic() {
    extern "C" fn noop(_: i32) {}
    unsafe { mim_request_mic_permission(noop) };
}

#[cfg(target_os = "macos")]
pub fn request_accessibility() -> PermissionStatus {
    unsafe { mim_request_accessibility_permission() };
    check()
}

#[cfg(target_os = "macos")]
pub fn request_input_monitoring() -> PermissionStatus {
    unsafe {
        mim_request_input_monitoring_permission();
    }
    check()
}

#[cfg(target_os = "macos")]
pub fn request_keyboard() -> PermissionStatus {
    unsafe {
        mim_request_accessibility_permission();
        mim_request_input_monitoring_permission();
    }
    check()
}

#[cfg(target_os = "macos")]
pub fn open_settings(pane: &str) {
    let url = match pane {
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        "input_monitoring" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
        }
        _ => return,
    };
    let _ = std::process::Command::new("open").arg(url).spawn();
}

#[cfg(not(target_os = "macos"))]
pub fn check() -> PermissionStatus {
    PermissionStatus {
        microphone: "authorized".to_string(),
        accessibility: true,
        input_monitoring: true,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn request_mic() {}

#[cfg(not(target_os = "macos"))]
pub fn request_accessibility() -> PermissionStatus {
    check()
}

#[cfg(not(target_os = "macos"))]
pub fn request_input_monitoring() -> PermissionStatus {
    check()
}

#[cfg(not(target_os = "macos"))]
pub fn request_keyboard() -> PermissionStatus {
    check()
}

#[cfg(not(target_os = "macos"))]
pub fn open_settings(_pane: &str) {}
