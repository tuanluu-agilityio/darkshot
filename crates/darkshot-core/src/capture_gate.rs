//! Cổng chụp và loại lượt chụp: luật thuần dùng chung cho mọi nền tảng.

use crate::config::HotkeyRole;

/// Cổng chụp: bảo đảm không bao giờ có hai lớp phủ và menu khay không chen vào lúc chờ chụp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureGate {
    Idle,
    /// Đã chọn "Chụp màn hình" ở menu, đang chờ menu tắt hẳn.
    PendingMenu,
    /// Đang chụp hoặc lớp phủ đang mở.
    Capturing,
    /// Cửa sổ Cài đặt đang mở.
    SettingsOpen,
    /// Như `PendingMenu`, kèm yêu cầu mở Cài đặt đang chờ.
    PendingMenuSettings,
    /// Như `Capturing`, kèm yêu cầu mở Cài đặt đang chờ.
    CapturingSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateEvent {
    MenuCapture,
    Hotkey,
    TimerFired,
    OverlayClosed,
    TrayClick,
    OpenSettings,
    SettingsClosed,
}

/// Việc bên gọi phải làm sau một sự kiện.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateAction {
    /// Không có gì đặc biệt (với `TrayClick`: mở menu như thường).
    None,
    StartTimer,
    /// Hủy bộ hẹn giờ của menu (nếu có) rồi chụp ngay.
    CaptureNow,
    /// Mở cửa sổ Cài đặt.
    OpenSettings,
    /// Đưa cửa sổ Cài đặt đang mở ra trước.
    FocusSettings,
    Ignore,
}

/// Hàm chuyển trạng thái thuần của cổng chụp.
pub fn gate_step(state: CaptureGate, ev: GateEvent) -> (CaptureGate, GateAction) {
    use CaptureGate::*;
    use GateEvent::*;
    match (state, ev) {
        (Idle, MenuCapture) => (PendingMenu, GateAction::StartTimer),
        (Idle | PendingMenu, Hotkey) | (PendingMenu, TimerFired) => {
            (Capturing, GateAction::CaptureNow)
        }
        (PendingMenuSettings, Hotkey | TimerFired) => (CapturingSettings, GateAction::CaptureNow),
        (Idle, TrayClick) => (Idle, GateAction::None),
        (Idle, OpenSettings) => (SettingsOpen, GateAction::OpenSettings),
        // Yêu cầu mở Cài đặt đến lúc đang chụp hoặc chờ menu: nhớ lại, mở sau khi lớp phủ đóng.
        (PendingMenu | PendingMenuSettings, OpenSettings) => {
            (PendingMenuSettings, GateAction::Ignore)
        }
        (Capturing | CapturingSettings, OpenSettings) => (CapturingSettings, GateAction::Ignore),
        // Cài đặt đang mở: mọi yêu cầu từ người dùng (kể cả chụp) đưa cửa sổ ra trước để nó không bị khuất.
        (SettingsOpen, TrayClick | OpenSettings | Hotkey | MenuCapture) => {
            (SettingsOpen, GateAction::FocusSettings)
        }
        (SettingsOpen, SettingsClosed) => (Idle, GateAction::None),
        (s, SettingsClosed) => (s, GateAction::None),
        (Capturing, OverlayClosed) => (Idle, GateAction::None),
        (CapturingSettings, OverlayClosed) => (SettingsOpen, GateAction::OpenSettings),
        (s, OverlayClosed) => (s, GateAction::None),
        (s, _) => (s, GateAction::Ignore),
    }
}

/// Loại của một lượt chụp: chụp vùng (lớp phủ), một trong hai kiểu toàn màn hình, hoặc lặp lại
/// lần chụp vùng trước.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureKind {
    Region,
    SaveFull,
    UploadFull,
    RepeatLast,
}

/// Loại chụp của một dòng phím tắt.
pub fn kind_of(role: HotkeyRole) -> CaptureKind {
    match role {
        HotkeyRole::Region => CaptureKind::Region,
        HotkeyRole::SaveFull => CaptureKind::SaveFull,
        HotkeyRole::UploadFull => CaptureKind::UploadFull,
        HotkeyRole::RepeatLast => CaptureKind::RepeatLast,
    }
}

/// Loại chụp sau một bước của cổng: chỉ yêu cầu được nhận (`StartTimer`, `CaptureNow`) mới đổi loại.
pub fn next_kind(current: CaptureKind, requested: CaptureKind, act: GateAction) -> CaptureKind {
    match act {
        GateAction::StartTimer | GateAction::CaptureNow => requested,
        _ => current,
    }
}

/// Có liệt kê cửa sổ (gợi ý khung) trước khi chụp không, theo loại chụp `kind` (sau khi đã đổi phím
/// lặp lại chưa có lần chụp trước thành `Region`) và ô gợi ý cửa sổ `snap_windows`.
pub fn enumerates_windows(kind: CaptureKind, snap_windows: bool) -> bool {
    snap_windows && matches!(kind, CaptureKind::Region | CaptureKind::RepeatLast)
}

#[cfg(test)]
mod tests {
    use super::*;
    use CaptureGate::*;
    use GateEvent::*;

    #[test]
    fn capture_gate_transitions() {
        assert_eq!(
            gate_step(Idle, MenuCapture),
            (PendingMenu, GateAction::StartTimer)
        );
        assert_eq!(
            gate_step(PendingMenu, TrayClick),
            (PendingMenu, GateAction::Ignore)
        );
        assert_eq!(
            gate_step(PendingMenu, Hotkey),
            (Capturing, GateAction::CaptureNow)
        );
        assert_eq!(
            gate_step(PendingMenu, TimerFired),
            (Capturing, GateAction::CaptureNow)
        );
        assert_eq!(
            gate_step(Capturing, Hotkey),
            (Capturing, GateAction::Ignore)
        );
        assert_eq!(
            gate_step(Capturing, TimerFired),
            (Capturing, GateAction::Ignore)
        );
        assert_eq!(
            gate_step(Capturing, OverlayClosed),
            (Idle, GateAction::None)
        );
        // Thêm: phím tắt khi rảnh chụp ngay; bấm khay khi rảnh mở menu như thường;
        // menu và khay khi đang chụp bị bỏ qua; hẹn giờ cũ khi rảnh bị bỏ qua.
        assert_eq!(gate_step(Idle, Hotkey), (Capturing, GateAction::CaptureNow));
        assert_eq!(gate_step(Idle, TrayClick), (Idle, GateAction::None));
        assert_eq!(
            gate_step(Capturing, TrayClick),
            (Capturing, GateAction::Ignore)
        );
        assert_eq!(
            gate_step(Capturing, MenuCapture),
            (Capturing, GateAction::Ignore)
        );
        assert_eq!(gate_step(Idle, TimerFired), (Idle, GateAction::Ignore));
        assert_eq!(
            gate_step(PendingMenu, MenuCapture),
            (PendingMenu, GateAction::Ignore)
        );
    }

    #[test]
    fn settings_gate_transitions() {
        assert_eq!(
            gate_step(Idle, OpenSettings),
            (SettingsOpen, GateAction::OpenSettings)
        );
        // Đang mở Cài đặt: bấm khay hoặc mở lại thì đưa cửa sổ ra trước, vẫn SettingsOpen.
        for ev in [TrayClick, OpenSettings] {
            assert_eq!(
                gate_step(SettingsOpen, ev),
                (SettingsOpen, GateAction::FocusSettings),
                "{ev:?}"
            );
        }
        // Yêu cầu chụp không chụp nhưng đưa cửa sổ Cài đặt ra trước để nó không bị khuất.
        for ev in [MenuCapture, Hotkey] {
            assert_eq!(
                gate_step(SettingsOpen, ev),
                (SettingsOpen, GateAction::FocusSettings),
                "{ev:?}"
            );
        }
        // Hẹn giờ cũ của menu thì chỉ bị bỏ qua.
        assert_eq!(
            gate_step(SettingsOpen, TimerFired),
            (SettingsOpen, GateAction::Ignore)
        );
        assert_eq!(
            gate_step(SettingsOpen, OverlayClosed),
            (SettingsOpen, GateAction::None)
        );
        assert_eq!(
            gate_step(SettingsOpen, SettingsClosed),
            (Idle, GateAction::None)
        );
        // Đóng Cài đặt khi không ở trạng thái đó: không đổi.
        assert_eq!(gate_step(Idle, SettingsClosed), (Idle, GateAction::None));
        assert_eq!(
            gate_step(Capturing, SettingsClosed),
            (Capturing, GateAction::None)
        );
    }

    #[test]
    fn settings_request_during_capture_is_remembered() {
        let (s, a) = gate_step(Capturing, OpenSettings);
        assert_eq!(a, GateAction::Ignore);
        // Hai yêu cầu chờ chỉ mở một lần.
        let (s, a) = gate_step(s, OpenSettings);
        assert_eq!(a, GateAction::Ignore);
        // Phím chụp trong lúc đó vẫn bị bỏ qua.
        assert_eq!(gate_step(s, Hotkey).1, GateAction::Ignore);
        assert_eq!(
            gate_step(s, OverlayClosed),
            (SettingsOpen, GateAction::OpenSettings)
        );
        // Không có yêu cầu chờ: về Idle như cũ.
        assert_eq!(
            gate_step(Capturing, OverlayClosed),
            (Idle, GateAction::None)
        );
    }

    #[test]
    fn enumerates_windows_for_region_and_repeat_fallback() {
        use CaptureKind::*;
        // Lặp lại cũng liệt kê: vùng trước có thể đã mất và lớp phủ dự phòng cần gợi ý cửa sổ.
        assert!(enumerates_windows(Region, true));
        assert!(enumerates_windows(RepeatLast, true));
        for k in [Region, RepeatLast, SaveFull, UploadFull] {
            assert!(!enumerates_windows(k, false), "{k:?}");
        }
        assert!(!enumerates_windows(SaveFull, true));
        assert!(!enumerates_windows(UploadFull, true));
    }

    #[test]
    fn kinds_of_roles() {
        assert_eq!(kind_of(HotkeyRole::Region), CaptureKind::Region);
        assert_eq!(kind_of(HotkeyRole::SaveFull), CaptureKind::SaveFull);
        assert_eq!(kind_of(HotkeyRole::UploadFull), CaptureKind::UploadFull);
        assert_eq!(kind_of(HotkeyRole::RepeatLast), CaptureKind::RepeatLast);
    }

    #[test]
    fn capture_kind_follows_the_accepted_request() {
        use CaptureKind::*;
        let step = |state, ev, cur, req| {
            let (s, a) = gate_step(state, ev);
            (s, a, next_kind(cur, req, a))
        };
        // rảnh, bấm phím lưu toàn màn hình: chụp ngay loại đó
        assert_eq!(
            step(Idle, Hotkey, Region, SaveFull),
            (Capturing, GateAction::CaptureNow, SaveFull)
        );
        // menu đang mở: chờ, loại được nhớ; hết giờ không đổi loại
        let (s, a, k) = step(Idle, MenuCapture, Region, UploadFull);
        assert_eq!((s, a, k), (PendingMenu, GateAction::StartTimer, UploadFull));
        assert_eq!(
            gate_step(s, TimerFired),
            (Capturing, GateAction::CaptureNow)
        );
        // đang chờ menu (Region) mà bấm phím loại khác: chụp ngay một lần, loại của phím vừa bấm
        assert_eq!(
            step(PendingMenu, Hotkey, Region, SaveFull),
            (Capturing, GateAction::CaptureNow, SaveFull)
        );
        // lớp phủ đang mở: bỏ qua, loại không đổi
        assert_eq!(
            step(Capturing, Hotkey, Region, SaveFull),
            (Capturing, GateAction::Ignore, Region)
        );
        assert_eq!(step(Capturing, MenuCapture, Region, UploadFull).2, Region);
        // Tùy chọn đang mở: đưa ra trước, loại không đổi
        assert_eq!(
            step(SettingsOpen, Hotkey, Region, UploadFull),
            (SettingsOpen, GateAction::FocusSettings, Region)
        );
    }

    #[test]
    fn settings_request_during_pending_menu_is_remembered() {
        let (s, a) = gate_step(PendingMenu, OpenSettings);
        assert_eq!(a, GateAction::Ignore);
        let (s, a) = gate_step(s, OpenSettings);
        assert_eq!(a, GateAction::Ignore);
        // Hẹn giờ nổ: vẫn chụp trước, yêu cầu còn chờ.
        let (s, a) = gate_step(s, TimerFired);
        assert_eq!(a, GateAction::CaptureNow);
        assert_eq!(
            gate_step(s, OverlayClosed),
            (SettingsOpen, GateAction::OpenSettings)
        );
        // Phím chụp thay cho hẹn giờ cũng vậy.
        let (s, _) = gate_step(PendingMenu, OpenSettings);
        let (s, a) = gate_step(s, Hotkey);
        assert_eq!(a, GateAction::CaptureNow);
        assert_eq!(
            gate_step(s, OverlayClosed),
            (SettingsOpen, GateAction::OpenSettings)
        );
    }
}
