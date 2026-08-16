use std::{thread, time::Duration};

#[cfg(not(target_os = "macos"))]
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

#[derive(Debug, Clone, Serialize)]
pub struct PasteOutcome {
    pub pasted: bool,
    pub restored_text_clipboard: bool,
    pub message: String,
}

pub fn paste_text(
    app: &AppHandle,
    text: &str,
    target_pid: Option<i32>,
) -> anyhow::Result<PasteOutcome> {
    let clipboard = app.clipboard();
    let original_text = clipboard.read_text().ok();
    clipboard.write_text(text.to_string())?;

    if let (Some(target), Some(current)) = (target_pid, frontmost_app_pid()) {
        if current != target {
            return Ok(PasteOutcome {
                pasted: false,
                restored_text_clipboard: false,
                message: "Focus changed. Copied - paste with Cmd+V.".to_string(),
            });
        }
    }

    let paste_result = simulate_cmd_v();
    if let Err(error) = paste_result {
        return Ok(PasteOutcome {
            pasted: false,
            restored_text_clipboard: false,
            message: format!("Copied. Paste manually with Cmd+V. ({error})"),
        });
    }

    thread::sleep(Duration::from_millis(300));

    if let Some(original) = original_text {
        clipboard.write_text(original)?;
        Ok(PasteOutcome {
            pasted: true,
            restored_text_clipboard: true,
            message: "Pasted".to_string(),
        })
    } else {
        Ok(PasteOutcome {
            pasted: true,
            restored_text_clipboard: false,
            message: "Pasted".to_string(),
        })
    }
}

#[cfg(target_os = "macos")]
pub fn frontmost_app_pid() -> Option<i32> {
    use objc2_app_kit::NSWorkspace;

    NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .map(|app| app.processIdentifier())
}

#[cfg(not(target_os = "macos"))]
pub fn frontmost_app_pid() -> Option<i32> {
    None
}

fn simulate_cmd_v() -> anyhow::Result<()> {
    if !accessibility_is_trusted() {
        let status = crate::permissions::request_accessibility();
        if !status.accessibility {
            return Err(anyhow::anyhow!(
                "Accessibility permission is not enabled for keyboard automation"
            ));
        }
    }

    simulate_paste_shortcut()
}

#[cfg(target_os = "macos")]
fn simulate_paste_shortcut() -> anyhow::Result<()> {
    const ANSI_V_KEY_CODE: u16 = 0x09;
    const COMMAND_FLAG: u64 = 0x0010_0000;

    unsafe {
        post_key_event(ANSI_V_KEY_CODE, true, COMMAND_FLAG)?;
        post_key_event(ANSI_V_KEY_CODE, false, COMMAND_FLAG)?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn post_key_event(key_code: u16, key_down: bool, flags: u64) -> anyhow::Result<()> {
    use std::{ffi::c_void, ptr};

    const CG_HID_EVENT_TAP: u32 = 0;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: *mut c_void,
            virtual_key: u16,
            key_down: bool,
        ) -> *mut c_void;
        fn CGEventSetFlags(event: *mut c_void, flags: u64);
        fn CGEventPost(tap: u32, event: *mut c_void);
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }

    let event = unsafe { CGEventCreateKeyboardEvent(ptr::null_mut(), key_code, key_down) };
    if event.is_null() {
        return Err(anyhow::anyhow!("Could not create paste keyboard event"));
    }

    unsafe {
        CGEventSetFlags(event, flags);
        CGEventPost(CG_HID_EVENT_TAP, event);
        CFRelease(event.cast());
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn simulate_paste_shortcut() -> anyhow::Result<()> {
    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|error| anyhow::anyhow!("Could not initialize keyboard automation: {error}"))?;

    enigo
        .key(Key::Meta, Press)
        .map_err(|error| anyhow::anyhow!("Could not press Command: {error}"))?;
    let click_result = enigo
        .key(Key::Unicode('v'), Click)
        .map_err(|error| anyhow::anyhow!("Could not send V: {error}"));
    let release_result = enigo
        .key(Key::Meta, Release)
        .map_err(|error| anyhow::anyhow!("Could not release Command: {error}"));

    click_result?;
    release_result?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn accessibility_is_trusted() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> std::os::raw::c_uchar;
    }

    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(not(target_os = "macos"))]
fn accessibility_is_trusted() -> bool {
    true
}
