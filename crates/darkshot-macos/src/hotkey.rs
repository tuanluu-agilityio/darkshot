//! Phím tắt toàn cục bằng Carbon (`RegisterEventHotKey`): lớp FFI mỏng, khai báo `extern "C"`
//! tối thiểu theo `HIToolbox/CarbonEvents.h` và `CarbonEventsCore.h` (SDK macOS 15), không thêm
//! crate.
//!
//! Carbon đã cũ nhưng vẫn là cách duy nhất bắt phím tắt toàn cục mà không cần quyền Accessibility
//! hay Input Monitoring và không cần cửa sổ (`NSEvent.addGlobalMonitor…` cần quyền và không nuốt
//! phím). Sự kiện `kEventHotKeyPressed` đến bộ xử lý cài trên đích sự kiện của ứng dụng, chạy trên
//! luồng chính trong vòng lặp của `NSApplication`.
//!
//! Mọi trạng thái (bộ xử lý đã cài, các `EventHotKeyRef` đang giữ) chỉ sống trên luồng chính
//! (`thread_local`). Mỗi lần `apply` hủy mọi phím cũ rồi đăng ký lại, nên không bao giờ rò
//! `EventHotKeyRef`.

use crate::keymap::{roles_to_register, to_carbon};
use darkshot_core::config::{HotkeyRole, Hotkeys};
use darkshot_core::hotkey_keys::{id_of, role_of};
use std::cell::{Cell, RefCell};
use std::ffi::c_void;

/// `OSStatus` (`MacTypes.h`: `SInt32`).
type OSStatus = i32;
/// `OSType` / `FourCharCode` (`UInt32`).
type OSType = u32;
/// Các kiểu con trỏ mờ của Carbon (`struct Opaque…*`).
type EventRef = *mut c_void;
type EventTargetRef = *mut c_void;
type EventHandlerRef = *mut c_void;
type EventHandlerCallRef = *mut c_void;
type EventHotKeyRef = *mut c_void;
/// `EventHandlerUPP` = `EventHandlerProcPtr` trên Mac OS X (`NewEventHandlerUPP` chỉ ép kiểu).
type EventHandlerUpp = unsafe extern "C" fn(EventHandlerCallRef, EventRef, *mut c_void) -> OSStatus;

/// `struct EventHotKeyID { OSType signature; UInt32 id; }` (`CarbonEvents.h`).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EventHotKeyId {
    signature: OSType,
    id: u32,
}

/// `struct EventTypeSpec { OSType eventClass; UInt32 eventKind; }` (`CarbonEventsCore.h`).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct EventTypeSpec {
    event_class: OSType,
    event_kind: u32,
}

const fn four_cc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const NO_ERR: OSStatus = 0;
/// `eventNotHandledErr`.
const EVENT_NOT_HANDLED_ERR: OSStatus = -9874;
/// `kEventClassKeyboard` = 'keyb'.
const EVENT_CLASS_KEYBOARD: OSType = four_cc(b"keyb");
/// `kEventHotKeyPressed` = 5.
const EVENT_HOT_KEY_PRESSED: u32 = 5;
/// `kEventParamDirectObject` = '----'.
const EVENT_PARAM_DIRECT_OBJECT: OSType = four_cc(b"----");
/// `typeEventHotKeyID` = 'hkid'.
const TYPE_EVENT_HOT_KEY_ID: OSType = four_cc(b"hkid");
/// `kEventHotKeyExclusive`: ứng dụng khác đã giữ độc quyền tổ hợp này thì đăng ký thất bại
/// (`eventHotKeyExistsErr`), để báo "phím bị chiếm" như Windows.
const EVENT_HOT_KEY_EXCLUSIVE: u32 = 1 << 0;
/// Chữ ký của darkshot trong `EventHotKeyID` (để bỏ qua phím tắt không phải của mình).
const SIGNATURE: OSType = four_cc(b"dksh");

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn InstallEventHandler(
        in_target: EventTargetRef,
        in_handler: EventHandlerUpp,
        in_num_types: usize,
        in_list: *const EventTypeSpec,
        in_user_data: *mut c_void,
        out_ref: *mut EventHandlerRef,
    ) -> OSStatus;
    fn RegisterEventHotKey(
        in_hot_key_code: u32,
        in_hot_key_modifiers: u32,
        in_hot_key_id: EventHotKeyId,
        in_target: EventTargetRef,
        in_options: u32,
        out_ref: *mut EventHotKeyRef,
    ) -> OSStatus;
    fn UnregisterEventHotKey(in_hot_key: EventHotKeyRef) -> OSStatus;
    fn GetEventParameter(
        in_event: EventRef,
        in_name: OSType,
        in_desired_type: OSType,
        out_actual_type: *mut OSType,
        in_buffer_size: usize,
        out_actual_size: *mut usize,
        out_data: *mut c_void,
    ) -> OSStatus;
}

thread_local! {
    /// Hàm nhận phím (đặt một lần bằng `init`).
    static CALLBACK: Cell<Option<fn(HotkeyRole)>> = const { Cell::new(None) };
    /// Đã cài bộ xử lý `kEventHotKeyPressed` trên đích sự kiện của ứng dụng.
    static HANDLER_INSTALLED: Cell<bool> = const { Cell::new(false) };
    /// Các phím đang đăng ký; hủy hết ở `unregister_all`.
    static REGISTERED: RefCell<Vec<(HotkeyRole, EventHotKeyRef)>> = const { RefCell::new(Vec::new()) };
}

/// Vai trò ứng với `EventHotKeyID` của một sự kiện; phím không mang chữ ký darkshot là `None`.
fn role_from_id(signature: OSType, id: u32) -> Option<HotkeyRole> {
    if signature != SIGNATURE {
        return None;
    }
    i32::try_from(id).ok().and_then(role_of)
}

/// Đặt hàm nhận phím (chạy trên luồng chính, bên trong bộ xử lý sự kiện Carbon: nên làm ít và
/// không chặn). Gọi trước `apply`.
pub fn init(callback: fn(HotkeyRole)) {
    CALLBACK.with(|c| c.set(Some(callback)));
}

/// Hủy mọi phím đã đăng ký rồi đăng ký các dòng đang bật mà bản Mac có làm
/// (`keymap::roles_to_register`). Trả về các dòng đang bật mà không đăng ký được (phím bị chiếm,
/// phím chưa hỗ trợ trên Mac, hoặc không cài được bộ xử lý), để báo bằng một thông báo.
/// Chỉ gọi trên luồng chính.
pub fn apply(keys: &Hotkeys) -> Vec<HotkeyRole> {
    unregister_all();
    let roles = roles_to_register(keys);
    if roles.is_empty() {
        return roles;
    }
    if !ensure_handler() {
        eprintln!("darkshot: cannot install the hotkey handler");
        return roles;
    }
    roles.into_iter().filter(|r| !register(*r, keys)).collect()
}

/// Hủy mọi phím đã đăng ký (gọi trước khi đăng ký lại và khi thoát).
pub fn unregister_all() {
    let refs = REGISTERED
        .try_with(|r| std::mem::take(&mut *r.borrow_mut()))
        .unwrap_or_default();
    for (_, hk) in refs {
        // SAFETY: `hk` là `EventHotKeyRef` khác null do `RegisterEventHotKey` trả về và chưa hủy
        // (lấy ra khỏi danh sách nên mỗi ref chỉ hủy một lần).
        let status = unsafe { UnregisterEventHotKey(hk) };
        if status != NO_ERR {
            eprintln!("darkshot: UnregisterEventHotKey failed ({status})");
        }
    }
}

/// Đăng ký một dòng; `false` khi không được.
fn register(role: HotkeyRole, keys: &Hotkeys) -> bool {
    let Some((code, mods)) = to_carbon(&keys.get(role).key) else {
        return false;
    };
    let Ok(id) = u32::try_from(id_of(role)) else {
        return false;
    };
    let mut out: EventHotKeyRef = std::ptr::null_mut();
    // SAFETY: các tham số là giá trị thuần; `GetApplicationEventTarget` trả đích sự kiện của
    // ứng dụng (sống suốt tiến trình); `out` là chỗ ghi hợp lệ trên ngăn xếp. Gọi trên luồng chính
    // (Carbon "not thread safe").
    let status = unsafe {
        RegisterEventHotKey(
            code,
            mods,
            EventHotKeyId {
                signature: SIGNATURE,
                id,
            },
            GetApplicationEventTarget(),
            EVENT_HOT_KEY_EXCLUSIVE,
            &mut out,
        )
    };
    if status != NO_ERR || out.is_null() {
        return false;
    }
    REGISTERED.with(|r| r.borrow_mut().push((role, out)));
    true
}

/// Cài bộ xử lý `kEventHotKeyPressed` một lần cho cả tiến trình (không gỡ: sống tới khi thoát).
fn ensure_handler() -> bool {
    if HANDLER_INSTALLED.with(|h| h.get()) {
        return true;
    }
    let spec = EventTypeSpec {
        event_class: EVENT_CLASS_KEYBOARD,
        event_kind: EVENT_HOT_KEY_PRESSED,
    };
    // SAFETY: `on_hotkey_event` đúng nguyên mẫu `EventHandlerProcPtr`; `spec` là mảng một phần tử
    // hợp lệ trong lúc gọi (Carbon chép danh sách); không dùng `inUserData`; `outRef` có thể null.
    let status = unsafe {
        InstallEventHandler(
            GetApplicationEventTarget(),
            on_hotkey_event,
            1,
            &spec,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    let ok = status == NO_ERR;
    HANDLER_INSTALLED.with(|h| h.set(ok));
    ok
}

/// Bộ xử lý sự kiện Carbon. Không để panic vượt ranh giới C: mọi việc nằm trong `catch_unwind`,
/// lỗi thì trả `eventNotHandledErr` để sự kiện đi tiếp như thường.
unsafe extern "C" fn on_hotkey_event(
    _call: EventHandlerCallRef,
    event: EventRef,
    _user: *mut c_void,
) -> OSStatus {
    std::panic::catch_unwind(|| handle_event(event)).unwrap_or(EVENT_NOT_HANDLED_ERR)
}

fn handle_event(event: EventRef) -> OSStatus {
    if event.is_null() {
        return EVENT_NOT_HANDLED_ERR;
    }
    let mut id = EventHotKeyId {
        signature: 0,
        id: 0,
    };
    // SAFETY: `event` là `EventRef` Carbon truyền cho bộ xử lý, còn sống trong lúc xử lý;
    // `id` là bộ đệm đúng cỡ `EventHotKeyID` cho kiểu `typeEventHotKeyID`; hai con trỏ ra tùy chọn
    // để null.
    let status = unsafe {
        GetEventParameter(
            event,
            EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            std::ptr::null_mut(),
            std::mem::size_of::<EventHotKeyId>(),
            std::ptr::null_mut(),
            (&mut id as *mut EventHotKeyId).cast(),
        )
    };
    if status != NO_ERR {
        return EVENT_NOT_HANDLED_ERR;
    }
    let Some(role) = role_from_id(id.signature, id.id) else {
        return EVENT_NOT_HANDLED_ERR;
    };
    match CALLBACK.try_with(|c| c.get()).ok().flatten() {
        Some(cb) => {
            cb(role);
            NO_ERR
        }
        None => EVENT_NOT_HANDLED_ERR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::mac_default_hotkeys;
    use darkshot_core::config::{Hotkey, HotkeySlot};

    #[test]
    fn carbon_structs_match_c_layout() {
        // Hai `UInt32` liền nhau, không đệm (truyền theo giá trị cho `RegisterEventHotKey`).
        assert_eq!(std::mem::size_of::<EventHotKeyId>(), 8);
        assert_eq!(std::mem::align_of::<EventHotKeyId>(), 4);
        assert_eq!(std::mem::size_of::<EventTypeSpec>(), 8);
        assert_eq!(EVENT_CLASS_KEYBOARD, 0x6B65_7962);
        assert_eq!(EVENT_PARAM_DIRECT_OBJECT, 0x2D2D_2D2D);
        assert_eq!(TYPE_EVENT_HOT_KEY_ID, 0x686B_6964);
    }

    #[test]
    fn hotkey_ids_decode_to_roles_only_with_our_signature() {
        for r in HotkeyRole::ALL {
            let id = u32::try_from(id_of(r)).expect("positive id");
            assert_eq!(role_from_id(SIGNATURE, id), Some(r));
            // Phím của ai khác (chữ ký khác) bị bỏ qua.
            assert_eq!(role_from_id(four_cc(b"abcd"), id), None);
        }
        assert_eq!(role_from_id(SIGNATURE, 0), None);
        assert_eq!(role_from_id(SIGNATURE, 99), None);
        assert_eq!(role_from_id(SIGNATURE, u32::MAX), None);
    }

    /// Chạy tay (đăng ký phím toàn cục thật, không bấm phím):
    /// `cargo test -p darkshot-macos --bin darkshot-macos -- --ignored --test-threads=1`.
    #[test]
    #[ignore = "đăng ký phím tắt toàn cục thật; chạy tay"]
    fn register_and_unregister_a_test_hotkey() {
        // Tổ hợp hiếm: Control+Option+Shift+Command+F19, để không đụng phím của ai.
        let mut keys = mac_default_hotkeys();
        keys.region = HotkeySlot {
            enabled: true,
            key: Hotkey {
                ctrl: true,
                alt: true,
                shift: true,
                win: true,
                vk: 0x82,
            },
        };
        init(|_| {});
        assert_eq!(apply(&keys), Vec::<HotkeyRole>::new());
        assert_eq!(REGISTERED.with(|r| r.borrow().len()), 1);
        // Đăng ký lại cùng tổ hợp: hủy cái cũ trước nên vẫn được.
        assert_eq!(apply(&keys), Vec::<HotkeyRole>::new());
        unregister_all();
        assert_eq!(REGISTERED.with(|r| r.borrow().len()), 0);
    }
}
