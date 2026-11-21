//! Giờ địa phương qua `libc::localtime_r`. Không panic: thất bại thì trả 1970-01-01 00:00:00.

use darkshot_core::naming::LocalTime;

/// Giá trị dự phòng khi không đọc được giờ hệ thống.
pub const EPOCH: LocalTime = LocalTime {
    year: 1970,
    month: 1,
    day: 1,
    hour: 0,
    minute: 0,
    second: 0,
};

/// Đổi các trường của `struct tm` (năm tính từ 1900, tháng 0–11) sang `LocalTime`; trường ngoài
/// khoảng hợp lệ thì trả `EPOCH`. Giây nhuận (60) được kẹp về 59.
pub fn from_tm_fields(
    year_since_1900: i32,
    month0: i32,
    mday: i32,
    hour: i32,
    min: i32,
    sec: i32,
) -> LocalTime {
    let _ = (year_since_1900, month0, mday, hour, min, sec);
    todo!()
}

/// Giờ địa phương hiện tại.
pub fn local_now() -> LocalTime {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_tm_fields() {
        assert_eq!(
            from_tm_fields(126, 10, 21, 23, 59, 58),
            LocalTime {
                year: 2026,
                month: 11,
                day: 21,
                hour: 23,
                minute: 59,
                second: 58,
            }
        );
    }

    #[test]
    fn leap_second_is_clamped() {
        assert_eq!(from_tm_fields(126, 11, 31, 23, 59, 60).second, 59);
    }

    #[test]
    fn out_of_range_fields_fall_back_to_epoch() {
        assert_eq!(from_tm_fields(126, 12, 1, 0, 0, 0), EPOCH);
        assert_eq!(from_tm_fields(126, -1, 1, 0, 0, 0), EPOCH);
        assert_eq!(from_tm_fields(126, 0, 0, 0, 0, 0), EPOCH);
        assert_eq!(from_tm_fields(126, 0, 32, 0, 0, 0), EPOCH);
        assert_eq!(from_tm_fields(126, 0, 1, 24, 0, 0), EPOCH);
        assert_eq!(from_tm_fields(126, 0, 1, 0, 60, 0), EPOCH);
        assert_eq!(from_tm_fields(126, 0, 1, 0, 0, 61), EPOCH);
        assert_eq!(from_tm_fields(-2000, 0, 1, 0, 0, 0), EPOCH);
        assert_eq!(from_tm_fields(i32::MAX, 0, 1, 0, 0, 0), EPOCH);
    }

    #[test]
    fn local_now_fields_are_in_range() {
        let t = local_now();
        assert!(t.year >= 2026, "{t:?}");
        assert!((1..=12).contains(&t.month), "{t:?}");
        assert!((1..=31).contains(&t.day), "{t:?}");
        assert!(t.hour <= 23, "{t:?}");
        assert!(t.minute <= 59, "{t:?}");
        assert!(t.second <= 59, "{t:?}");
    }
}
