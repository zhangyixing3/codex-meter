use super::*;
use std::cell::Cell;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;

pub(super) const WIDTH: i32 = 408;
const CLASS: &str = "CodexMeterResetPanel";
thread_local! {static OPEN_PANEL:Cell<HWND>=const {Cell::new(null_mut())};}
struct Panel {
    shared: Arc<Mutex<Snapshot>>,
    layout: Layout,
    last: Option<Snapshot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Layout {
    pub width: i32,
    pub height: i32,
    rows: usize,
}

pub(super) fn layout(s: &Snapshot) -> Layout {
    let count = s
        .reset_cards
        .as_ref()
        .and_then(|r| r.cards.as_ref())
        .map_or(0, Vec::len)
        .max(1);
    Layout {
        width: WIDTH,
        height: 184 + count as i32 * 94,
        rows: count,
    }
}

pub(super) unsafe fn paint(dc: HDC, s: &Snapshot, layout: Layout) {
    let w = layout.width;
    let h = layout.height;
    for y in 0..h {
        rect(
            dc,
            0,
            y,
            w,
            1,
            blend(0x657c82, 0x344b40, y as f64 / h as f64),
        );
    }
    text(
        dc,
        24,
        19,
        280,
        18,
        "ACCOUNT / RESETS",
        10,
        TEAL,
        true,
        DT_LEFT,
    );
    text(dc, 22, 43, 270, 38, "重置卡", 25, TEXT, true, DT_LEFT);
    text(dc, w - 52, 15, 30, 28, "×", 21, MUTED, false, DT_CENTER);
    let available = s
        .reset_cards
        .as_ref()
        .map(|r| format!("可用 {} 次", r.available_count))
        .unwrap_or("未返回".into());
    round(dc, w - 122, 50, 98, 27, 0x507e72, 0x7fa99a, 24);
    text(
        dc,
        w - 122,
        50,
        98,
        27,
        &available,
        12,
        TEAL,
        true,
        DT_CENTER,
    );
    let zone = Local::now().format("%:z");
    text(
        dc,
        24,
        87,
        w - 48,
        20,
        &format!("按到期时间排序 · 本地时间 UTC{zone}"),
        11,
        MUTED,
        false,
        DT_LEFT,
    );
    rect(dc, 24, 116, w - 48, 1, 0x6b8280);
    let cards = s.reset_cards.as_ref().and_then(|r| r.cards.as_ref());
    if let Some(cards) = cards.filter(|r| !r.is_empty()) {
        let mut rows: Vec<_> = cards.iter().collect();
        rows.sort_by_key(|r| r.expires_at.unwrap_or(i64::MAX));
        for (i, card) in rows.iter().enumerate() {
            let base = 0;
            let y = 132 + i as i32 * 94;
            round(dc, base + 22, y, WIDTH - 44, 82, 0x465f5a, 0x69847a, 12);
            let title = match card.title.as_deref() {
                Some("Full reset (Weekly + 5 hr)") => "完全重置（每周 + 5 小时）",
                Some("Rate-limit reset") | None => "Codex 额度重置",
                Some(t) => t,
            };
            text(
                dc,
                base + 38,
                y + 9,
                246,
                24,
                title,
                14,
                TEXT,
                true,
                DT_LEFT,
            );
            let (status, color) = match card.status.as_deref() {
                Some("available") => ("可用", TEAL),
                Some("redeeming") => ("处理中", GOLD),
                Some("redeemed") => ("已使用", MUTED),
                _ => ("未知", MUTED),
            };
            round(
                dc,
                base + WIDTH - 102,
                y + 12,
                64,
                23,
                0x3f6259,
                0x6b8b7c,
                20,
            );
            text(
                dc,
                base + WIDTH - 102,
                y + 12,
                64,
                23,
                status,
                10,
                color,
                true,
                DT_CENTER,
            );
            let expiry = card
                .expires_at
                .and_then(|t| Local.timestamp_opt(t, 0).single());
            let date = expiry
                .map(|t| format!("{} 到期", t.format("%m/%d  %H:%M")))
                .unwrap_or("到期时间未提供".into());
            text(
                dc,
                base + 38,
                y + 40,
                210,
                25,
                &date,
                12,
                MUTED,
                false,
                DT_LEFT,
            );
            let countdown = card
                .expires_at
                .map(|t| {
                    let left = t - Local::now().timestamp();
                    if left <= 0 {
                        "已到期".into()
                    } else if left < 86400 {
                        "24小时内到期".into()
                    } else {
                        format!("{} 天后到期", (left + 86399) / 86400)
                    }
                })
                .unwrap_or_default();
            text(
                dc,
                base + 239,
                y + 40,
                WIDTH - 277,
                25,
                &countdown,
                11,
                if i == 0 { GOLD } else { MUTED },
                false,
                DT_RIGHT,
            );
        }
    } else {
        round(dc, 22, 132, WIDTH - 44, 82, 0x465f5a, 0x69847a, 12);
        let message = if s
            .reset_cards
            .as_ref()
            .is_some_and(|r| r.available_count == 0)
        {
            "暂无可用重置卡"
        } else {
            "服务端尚未返回单张卡详情"
        };
        text(
            dc,
            38,
            149,
            WIDTH - 76,
            25,
            message,
            14,
            TEXT,
            true,
            DT_LEFT,
        );
        text(
            dc,
            38,
            180,
            WIDTH - 76,
            20,
            "刷新后会自动同步账户信息",
            11,
            MUTED,
            false,
            DT_LEFT,
        );
    }
    let returned = cards.map_or(0, Vec::len);
    let note = if s.demo {
        "演示数据".into()
    } else if s.error.is_some() {
        "缓存数据 · 请刷新确认".into()
    } else if s
        .reset_cards
        .as_ref()
        .is_some_and(|r| r.available_count != returned as u64)
    {
        format!("已返回 {} 张详情", returned)
    } else {
        format!("全部 {} 张 · 自动同步", returned)
    };
    text(dc, 24, h - 42, 240, 26, &note, 10, MUTED, false, DT_LEFT);
    round(dc, w - 108, h - 46, 84, 32, 0x56746b, 0x81a195, 10);
    text(
        dc,
        w - 108,
        h - 46,
        84,
        32,
        "关闭",
        12,
        TEXT,
        true,
        DT_CENTER,
    );
}

unsafe fn panel<'a>(hwnd: HWND) -> Option<&'a mut Panel> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Panel).as_mut()
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            SetWindowLongPtrW(
                hwnd,
                GWLP_USERDATA,
                (*(lp as *const CREATESTRUCTW)).lpCreateParams as isize,
            );
            1
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut r: RECT = zeroed();
            GetClientRect(hwnd, &mut r);
            let mem = CreateCompatibleDC(dc);
            let bmp = CreateCompatibleBitmap(dc, r.right.max(1), r.bottom.max(1));
            let old = SelectObject(mem, bmp);
            if let Some(p) = panel(hwnd) {
                SetMapMode(mem, MM_ANISOTROPIC);
                SetWindowExtEx(mem, p.layout.width, p.layout.height, null_mut());
                SetViewportExtEx(mem, r.right, r.bottom, null_mut());
                paint(mem, &p.shared.lock().unwrap(), p.layout);
            }
            SetMapMode(mem, MM_TEXT);
            BitBlt(dc, 0, 0, r.right, r.bottom, mem, 0, 0, SRCCOPY);
            SelectObject(mem, old);
            DeleteObject(bmp);
            DeleteDC(mem);
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER => {
            let mut new_layout = None;
            let changed = if let Some(p) = panel(hwnd) {
                let s = p.shared.lock().unwrap().clone();
                if p.last.as_ref() != Some(&s) {
                    let next = layout(&s);
                    if next != p.layout {
                        p.layout = next;
                        new_layout = Some(next);
                    }
                    p.last = Some(s);
                    true
                } else {
                    false
                }
            } else {
                false
            };
            if let Some(next) = new_layout {
                resize_to_layout(hwnd, next);
            }
            if changed {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_NCHITTEST => {
            let mut r: RECT = zeroed();
            GetClientRect(hwnd, &mut r);
            let mut pt = POINT {
                x: (lp as i16) as i32,
                y: ((lp >> 16) as i16) as i32,
            };
            ScreenToClient(hwnd, &mut pt);
            let h = panel(hwnd).map_or(1, |p| p.layout.height);
            if pt.y * h / r.bottom.max(1) < 84 && pt.x * WIDTH / r.right.max(1) < WIDTH - 65 {
                HTCAPTION as isize
            } else {
                HTCLIENT as isize
            }
        }
        WM_LBUTTONUP => {
            let mut r: RECT = zeroed();
            GetClientRect(hwnd, &mut r);
            let h = panel(hwnd).map_or(1, |p| p.layout.height);
            let x = (lp as i16) as i32 * WIDTH / r.right.max(1);
            let y = ((lp >> 16) as i16) as i32 * h / r.bottom.max(1);
            if (x > WIDTH - 65 && y < 45) || (x > WIDTH - 108 && y > h - 50) {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_MOUSEWHEEL => 0,
        WM_KEYDOWN if wp == 0x1b => {
            DestroyWindow(hwnd);
            0
        }
        WM_DPICHANGED => {
            let r = &*(lp as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_NCDESTROY => {
            OPEN_PANEL.with(|open| {
                if open.get() == hwnd {
                    open.set(null_mut());
                }
            });
            KillTimer(hwnd, 1);
            let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Panel;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            if !p.is_null() {
                drop(Box::from_raw(p));
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

pub(super) unsafe fn show(owner: HWND, shared: Arc<Mutex<Snapshot>>) {
    let class = wide(CLASS);
    let existing = OPEN_PANEL.with(Cell::get);
    if !existing.is_null() && IsWindow(existing) != 0 {
        ShowWindow(existing, SW_SHOW);
        SetForegroundWindow(existing);
        return;
    }
    let instance = GetModuleHandleW(null());
    let wc = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        ..zeroed()
    };
    if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
        eprintln!("Reset panel class registration failed: {}", GetLastError());
        return;
    }
    let layout = layout(&shared.lock().unwrap());
    let h = layout.height;
    let mut work: RECT = zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as *mut c_void, 0);
    let mut owner_rect: RECT = zeroed();
    GetWindowRect(owner, &mut owner_rect);
    let scale = (GetDpiForWindow(owner) as f32 / 96.0)
        .min((work.bottom - work.top - 48) as f32 / h as f32)
        .max(0.1);
    let w = (WIDTH as f32 * scale) as i32;
    let ph = (h as f32 * scale) as i32;
    let x = if owner_rect.right + w + 16 < work.right {
        owner_rect.right + 16
    } else {
        (owner_rect.left - w - 16).max(work.left + 16)
    };
    let y = owner_rect
        .top
        .clamp(work.top + 16, (work.bottom - ph - 16).max(work.top + 16));
    let p = Box::into_raw(Box::new(Panel {
        shared,
        layout,
        last: None,
    }));
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class.as_ptr(),
        wide("重置卡详情").as_ptr(),
        WS_POPUP,
        x,
        y,
        w,
        ph,
        owner,
        null_mut(),
        instance,
        p as *const c_void,
    );
    if hwnd.is_null() {
        eprintln!("Reset panel creation failed: {}", GetLastError());
        return;
    }
    OPEN_PANEL.with(|open| open.set(hwnd));
    let corner: u32 = 2;
    SetWindowTextW(hwnd, wide("重置卡详情").as_ptr());
    DwmSetWindowAttribute(hwnd, 33, &corner as *const _ as *const c_void, 4);
    SetTimer(hwnd, 1, 1000, None);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    SetFocus(hwnd);
}

unsafe fn resize_to_layout(hwnd: HWND, layout: Layout) {
    let mut work: RECT = zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as *mut c_void, 0);
    let scale = (GetDpiForWindow(hwnd) as f32 / 96.0)
        .min((work.bottom - work.top - 48) as f32 / layout.height as f32)
        .max(0.1);
    let width = (layout.width as f32 * scale) as i32;
    let height = (layout.height as f32 * scale) as i32;
    let mut r: RECT = zeroed();
    GetWindowRect(hwnd, &mut r);
    SetWindowPos(
        hwnd,
        null_mut(),
        r.left.clamp(
            work.left + 16,
            (work.right - width - 16).max(work.left + 16),
        ),
        r.top.clamp(
            work.top + 16,
            (work.bottom - height - 16).max(work.top + 16),
        ),
        width,
        height,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn more_than_five_cards_still_use_one_column_without_a_limit() {
        let mut s = Snapshot::demo();
        let summary = s.reset_cards.as_mut().unwrap();
        let card = summary.cards.as_ref().unwrap()[0].clone();
        summary.cards = Some(vec![card; 8]);
        summary.available_count = 8;
        let l = layout(&s);
        assert_eq!(l.width, WIDTH);
        assert_eq!(l.rows, 8);
        assert_eq!(l.height, 184 + 8 * 94);
    }
}
