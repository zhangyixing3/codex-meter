use crate::data::{Snapshot, Window, compact};
use chrono::{Datelike, Local, TimeZone};
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::{Arc, Mutex, mpsc::Sender},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    System::{LibraryLoader::*, Registry::*, Threading::*},
    UI::{HiDpi::*, Shell::*, WindowsAndMessaging::*},
};

mod reset_panel;

const W: i32 = 340;
const H: i32 = 648;
const TRAY: u32 = WM_APP + 1;
const BG: u32 = 0x344b40;
const CARD: u32 = 0x485e69;
const EDGE: u32 = 0x697b7e;
const TEXT: u32 = 0xf0f4f6;
const MUTED: u32 = 0xb6c4c9;
const TEAL: u32 = 0x55d4c4;
const GOLD: u32 = 0xffca6b;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn rgb(hex: u32) -> u32 {
    ((hex >> 16) & 255) | (hex & 0xff00) | ((hex & 255) << 16)
}
struct App {
    shared: Arc<Mutex<Snapshot>>,
    refresh: Sender<()>,
    pinned: bool,
    scale: f32,
    icon: HICON,
    last: Option<Snapshot>,
    last_minute: i64,
}

unsafe fn rect(dc: HDC, x: i32, y: i32, w: i32, h: i32, color: u32) {
    let brush = CreateSolidBrush(rgb(color));
    let r = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    FillRect(dc, &r, brush);
    DeleteObject(brush);
}
unsafe fn round(dc: HDC, x: i32, y: i32, w: i32, h: i32, color: u32, border: u32, r: i32) {
    let b = CreateSolidBrush(rgb(color));
    let p = CreatePen(PS_SOLID, 1, rgb(border));
    let ob = SelectObject(dc, b);
    let op = SelectObject(dc, p);
    RoundRect(dc, x, y, x + w, y + h, r, r);
    SelectObject(dc, ob);
    SelectObject(dc, op);
    DeleteObject(b);
    DeleteObject(p);
}
unsafe fn text(
    dc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    s: &str,
    size: i32,
    color: u32,
    bold: bool,
    align: u32,
) {
    let name = wide("Segoe UI");
    let font = CreateFontW(
        -size,
        0,
        0,
        0,
        if bold { 600 } else { 400 },
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        DEFAULT_PITCH as u32,
        name.as_ptr(),
    );
    let old = SelectObject(dc, font);
    SetTextColor(dc, rgb(color));
    SetBkMode(dc, TRANSPARENT as i32);
    let mut r = RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    };
    let s = wide(s);
    DrawTextW(
        dc,
        s.as_ptr(),
        -1,
        &mut r,
        DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX | align,
    );
    SelectObject(dc, old);
    DeleteObject(font);
}

fn window_name(w: Option<&Window>, fallback: &str) -> String {
    match w.and_then(|w| w.minutes) {
        Some(m) if m % 1440 == 0 => format!("{} 天剩余额度", m / 1440),
        Some(m) if m % 60 == 0 => format!("{} 小时剩余额度", m / 60),
        Some(m) => format!("{m} 分钟剩余额度"),
        None => fallback.into(),
    }
}
unsafe fn quota(dc: HDC, y: i32, window: Option<&Window>, fallback: &str, stale: bool) {
    round(dc, 20, y, 300, 86, CARD, EDGE, 12);
    text(
        dc,
        34,
        y + 8,
        180,
        20,
        &window_name(window, fallback),
        12,
        MUTED,
        false,
        DT_LEFT,
    );
    let value = window
        .map(|w| format!("{:.0}%", w.remaining))
        .unwrap_or("—".into());
    text(dc, 34, y + 28, 120, 32, &value, 26, TEXT, true, DT_LEFT);
    let reset = window
        .and_then(|w| w.resets_at)
        .and_then(|t| Local.timestamp_opt(t, 0).single())
        .map(|t| {
            if t.date_naive() == Local::now().date_naive() {
                format!("{} 重置", t.format("%H:%M"))
            } else {
                format!("{} 重置", t.format("%m/%d %H:%M"))
            }
        })
        .unwrap_or("尚无数据".into());
    text(
        dc,
        150,
        y + 30,
        156,
        28,
        &reset,
        11,
        if stale { MUTED } else { TEAL },
        true,
        DT_RIGHT,
    );
    round(dc, 34, y + 66, 272, 5, 0x687a80, 0x687a80, 4);
    if let Some(w) = window {
        let length = (272.0 * w.remaining / 100.0).round() as i32;
        for i in 0..length {
            let f = i as f64 / (length - 1).max(1) as f64;
            let c = blend(TEAL, GOLD, f);
            rect(dc, 34 + i, y + 66, 1, 5, c);
        }
    }
}
fn blend(a: u32, b: u32, t: f64) -> u32 {
    let mut c = 0;
    for shift in [0, 8, 16] {
        let x = ((a >> shift) & 255) as f64;
        let y = ((b >> shift) & 255) as f64;
        c |= ((x + (y - x) * t).round() as u32) << shift;
    }
    c
}

pub unsafe fn paint(dc: HDC, s: &Snapshot, pinned: bool) {
    // Reference palette: soft grey header, slate-blue middle, deep green footer.
    for y in 0..H {
        let color = if y < 130 {
            blend(0x85898b, 0x536b78, y as f64 / 130.0)
        } else if y < 320 {
            blend(0x536b78, 0x40585e, (y - 130) as f64 / 190.0)
        } else {
            blend(0x40585e, BG, (y - 320) as f64 / (H - 320) as f64)
        };
        rect(dc, 0, y, W, 1, color);
    }
    text(dc, 22, 17, 190, 16, "AGENT STATUS", 10, TEAL, true, DT_LEFT);
    text(dc, 20, 36, 170, 37, "Codex", 29, TEXT, true, DT_LEFT);
    text(
        dc,
        226,
        13,
        28,
        25,
        if pinned { "◆" } else { "◇" },
        15,
        if pinned { TEAL } else { MUTED },
        true,
        DT_CENTER,
    );
    text(dc, 260, 13, 25, 25, "−", 18, MUTED, false, DT_CENTER);
    text(dc, 292, 13, 28, 25, "×", 19, MUTED, false, DT_CENTER);
    let stale = s.error.is_some()
        || s.quota_at
            .is_none_or(|t| Local::now().timestamp() - t > 600);
    let status = if s.demo {
        "DEMO"
    } else if s.refreshing {
        "SYNC"
    } else if stale {
        "STALE"
    } else {
        "LIVE"
    };
    round(dc, 258, 47, 62, 22, 0x6e9091, 0x82b5b2, 20);
    text(dc, 258, 47, 62, 22, status, 10, TEAL, true, DT_CENTER);
    rect(dc, 20, 81, 300, 1, EDGE);
    quota(dc, 94, s.primary.as_ref(), "短周期额度", stale);
    quota(dc, 190, s.secondary.as_ref(), "长周期额度", stale);

    round(dc, 20, 288, 300, 62, 0x45645f, 0x62817b, 10);
    text(
        dc,
        34,
        294,
        138,
        23,
        "使用限额重置",
        12,
        TEXT,
        true,
        DT_LEFT,
    );
    let count = s
        .reset_cards
        .as_ref()
        .map(|r| {
            format!(
                "可用 {} 次{}",
                r.available_count,
                if stale { " · 缓存" } else { "" }
            )
        })
        .unwrap_or("信息未返回".into());
    text(dc, 172, 294, 134, 23, &count, 12, TEAL, true, DT_RIGHT);
    let expiry = s
        .reset_cards
        .as_ref()
        .and_then(|r| r.cards.as_ref())
        .and_then(|rows| {
            rows.iter()
                .filter(|r| r.status.as_deref() == Some("available"))
                .filter_map(|r| r.expires_at)
                .min()
        })
        .and_then(|t| Local.timestamp_opt(t, 0).single());
    let hint = if let Some(t) = expiry {
        format!("最近到期 {}", t.format("%m/%d %H:%M"))
    } else if s
        .reset_cards
        .as_ref()
        .is_some_and(|r| r.available_count == 0)
    {
        "暂无可用重置卡".into()
    } else {
        "到期时间未返回".into()
    };
    text(dc, 34, 320, 190, 21, &hint, 10, MUTED, false, DT_LEFT);
    text(dc, 226, 320, 80, 21, "查看详情 ›", 10, TEAL, true, DT_RIGHT);

    text(dc, 20, 370, 130, 22, "Token 日历", 13, TEXT, true, DT_LEFT);
    let today = Local::now().date_naive();
    let start = today - chrono::Duration::days(29);
    let range = format!(
        "近30天  {} – {}",
        start.format("%m/%d"),
        today.format("%m/%d")
    );
    text(dc, 134, 370, 186, 22, &range, 11, MUTED, false, DT_RIGHT);
    let max = s
        .days
        .as_ref()
        .map(|days| {
            days.range(start..=today)
                .map(|(_, v)| *v)
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
        .max(1);
    for i in 0..35 {
        let x = 20 + (i % 7) * 43;
        let y = 405 + (i / 7) * 23;
        if i < 30 {
            let date = start + chrono::Duration::days(i as i64);
            let n = s.days.as_ref().and_then(|days| days.get(&date)).copied();
            let color = match n {
                None => 0x4f6265,
                Some(0) => 0x556d6b,
                Some(n) => {
                    let level = (n as f64 / max as f64).sqrt();
                    if level > 0.85 {
                        blend(0x64b7a4, GOLD, (level - 0.85) / 0.15)
                    } else {
                        blend(0x4e716c, TEAL, level * 0.8)
                    }
                }
            };
            round(
                dc,
                x,
                y,
                40,
                20,
                color,
                if date == today { GOLD } else { color },
                5,
            );
            text(
                dc,
                x,
                y,
                40,
                20,
                &date.day().to_string(),
                9,
                if n.unwrap_or(0) > max / 2 { BG } else { TEXT },
                false,
                DT_CENTER,
            );
        }
    }
    text(
        dc,
        20,
        518,
        218,
        18,
        if s.usage_error.is_some() {
            "Token 更新失败 · 保留旧数据"
        } else {
            "灰色 = 无数据 · 日期按服务端返回"
        },
        10,
        MUTED,
        false,
        DT_LEFT,
    );
    text(dc, 236, 518, 84, 18, "少 → 多", 10, TEAL, false, DT_RIGHT);
    for (x, label, value) in [
        (20, "累计 Token", compact(s.lifetime)),
        (175, "今日 Token", compact(s.today())),
    ] {
        round(dc, x, 545, 145, 63, 0x435d51, 0x5d7468, 10);
        text(dc, x + 13, 552, 119, 20, label, 11, MUTED, false, DT_LEFT);
        text(dc, x + 13, 573, 119, 27, &value, 23, TEXT, true, DT_LEFT);
    }
    let footer = if s.refreshing {
        "正在获取最新额度和 Token…".into()
    } else if s.error.is_some() {
        "刷新失败 · 点击这里查看详情".into()
    } else if s.demo {
        s.notice
            .clone()
            .unwrap_or("演示数据 · 不代表账户额度".into())
    } else if let Some(t) = s.quota_at {
        let time = Local
            .timestamp_opt(t, 0)
            .single()
            .map(|t| t.format("%H:%M:%S").to_string())
            .unwrap_or_default();
        format!(
            "已刷新 {time} · {}",
            if s.usage_error.is_some() {
                "Token 失败"
            } else {
                s.plan.as_deref().unwrap_or("Codex")
            }
        )
    } else {
        "正在读取账户数据…".into()
    };
    text(
        dc,
        20,
        614,
        228,
        24,
        &footer,
        10,
        if stale { GOLD } else { MUTED },
        false,
        DT_LEFT,
    );
    round(dc, 252, 611, 68, 30, 0x4b6866, 0x6c9790, 10);
    text(
        dc,
        252,
        611,
        68,
        30,
        if s.refreshing {
            "刷新中…"
        } else {
            "刷新 ↻"
        },
        11,
        if s.refreshing { MUTED } else { TEAL },
        true,
        DT_CENTER,
    );
}

unsafe fn request_refresh(hwnd: HWND) {
    if let Some(a) = state(hwnd) {
        crate::data::request_refresh(&mut a.shared.lock().unwrap(), &a.refresh);
    }
    InvalidateRect(hwnd, null(), 0);
    UpdateWindow(hwnd);
}

unsafe fn show_reset_details(hwnd: HWND) {
    let shared = state(hwnd).map(|a| a.shared.clone());
    if let Some(shared) = shared {
        reset_panel::show(hwnd, shared);
    }
}

unsafe fn state<'a>(hwnd: HWND) -> Option<&'a mut App> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App).as_mut()
}
unsafe fn tray(hwnd: HWND, icon: HICON, action: u32) {
    let mut n: NOTIFYICONDATAW = zeroed();
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    n.uCallbackMessage = TRAY;
    n.hIcon = icon;
    let tip = wide("Codex Meter · 双击显示 / 右键菜单");
    n.szTip[..tip.len()].copy_from_slice(&tip);
    Shell_NotifyIconW(action, &n);
}
unsafe fn toggle_pin(hwnd: HWND) {
    let pinned = if let Some(a) = state(hwnd) {
        a.pinned = !a.pinned;
        a.pinned
    } else {
        return;
    };
    // SetWindowPos can re-enter the window procedure; don't hold an App borrow across it.
    SetWindowPos(
        hwnd,
        if pinned { HWND_TOPMOST } else { HWND_NOTOPMOST },
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
    InvalidateRect(hwnd, null(), 0);
}

unsafe fn trim_hidden() {
    // Release resident pages while in the tray. This does not change committed memory.
    // Trim once per hide/completed refresh, not periodically, to avoid page churn.
    SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
}
unsafe fn hide(hwnd: HWND) {
    ShowWindow(hwnd, SW_HIDE);
    trim_hidden();
}
fn startup_name() -> Vec<u16> {
    wide("CodexMeter")
}
unsafe fn startup_enabled() -> bool {
    let mut key = null_mut();
    let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    if RegOpenKeyExW(
        HKEY_CURRENT_USER,
        path.as_ptr(),
        0,
        KEY_QUERY_VALUE,
        &mut key,
    ) != 0
    {
        return false;
    }
    let ok = RegQueryValueExW(
        key,
        startup_name().as_ptr(),
        null(),
        null_mut(),
        null_mut(),
        null_mut(),
    ) == 0;
    RegCloseKey(key);
    ok
}
unsafe fn set_startup(enable: bool) -> bool {
    let mut key = null_mut();
    let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    if RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_SET_VALUE, &mut key) != 0 {
        return false;
    }
    let ok = if enable {
        if let Ok(exe) = std::env::current_exe() {
            let value = wide(&format!("\"{}\" --hidden", exe.display()));
            RegSetValueExW(
                key,
                startup_name().as_ptr(),
                0,
                REG_SZ,
                value.as_ptr() as *const u8,
                (value.len() * 2) as u32,
            ) == 0
        } else {
            false
        }
    } else {
        RegDeleteValueW(key, startup_name().as_ptr()) == 0
    };
    RegCloseKey(key);
    ok
}
unsafe fn menu(hwnd: HWND) {
    let m = CreatePopupMenu();
    let pinned = state(hwnd).is_some_and(|a| a.pinned);
    for (id, label, checked) in [
        (1, "显示窗口", false),
        (2, "立即刷新", false),
        (3, "窗口置顶", pinned),
        (4, "开机启动", startup_enabled()),
        (5, "打开设置目录", false),
        (6, "退出", false),
    ] {
        AppendMenuW(
            m,
            MF_STRING | if checked { MF_CHECKED } else { 0 },
            id,
            wide(label).as_ptr(),
        );
    }
    let mut p = POINT { x: 0, y: 0 };
    GetCursorPos(&mut p);
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(
        m,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        p.x,
        p.y,
        0,
        hwnd,
        null(),
    );
    DestroyMenu(m);
    command(hwnd, cmd);
    PostMessageW(hwnd, WM_NULL, 0, 0);
}

unsafe fn command(hwnd: HWND, cmd: i32) {
    match cmd {
        1 => {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
        2 => {
            request_refresh(hwnd);
        }
        3 => toggle_pin(hwnd),
        4 => {
            if !set_startup(!startup_enabled()) {
                MessageBoxW(
                    hwnd,
                    wide("无法修改开机启动设置").as_ptr(),
                    wide("Codex Meter").as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
        5 => {
            let dir = crate::data::data_dir();
            let _ = std::fs::create_dir_all(&dir);
            ShellExecuteW(
                hwnd,
                wide("open").as_ptr(),
                wide(&dir.to_string_lossy()).as_ptr(),
                null(),
                null(),
                SW_SHOWNORMAL,
            );
        }
        6 => {
            DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lp as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            1
        }
        WM_ERASEBKGND => 1,
        WM_COMMAND => {
            command(hwnd, (wp & 0xffff) as i32);
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut r: RECT = zeroed();
            GetClientRect(hwnd, &mut r);
            let mem = CreateCompatibleDC(dc);
            let bmp = CreateCompatibleBitmap(dc, r.right.max(1), r.bottom.max(1));
            let old = SelectObject(mem, bmp);
            SetMapMode(mem, MM_ANISOTROPIC);
            SetWindowExtEx(mem, W, H, null_mut());
            SetViewportExtEx(mem, r.right, r.bottom, null_mut());
            if let Some(a) = state(hwnd) {
                paint(mem, &a.shared.lock().unwrap(), a.pinned);
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
            if let Some(a) = state(hwnd) {
                let snapshot = a.shared.lock().unwrap().clone();
                let minute = Local::now().timestamp() / 60;
                let changed = a.last.as_ref() != Some(&snapshot);
                if changed || minute != a.last_minute {
                    let refreshing = snapshot.refreshing;
                    a.last = Some(snapshot);
                    a.last_minute = minute;
                    if IsWindowVisible(hwnd) != 0 {
                        InvalidateRect(hwnd, null(), 0);
                    } else if changed && !refreshing {
                        trim_hidden();
                    }
                }
            }
            0
        }
        WM_NCHITTEST => {
            let mut point = POINT {
                x: (lp as i16) as i32,
                y: ((lp >> 16) as i16) as i32,
            };
            ScreenToClient(hwnd, &mut point);
            let scale = state(hwnd).map(|a| a.scale).unwrap_or(1.0);
            if point.y < (80.0 * scale) as i32 && point.x < (220.0 * scale) as i32 {
                HTCAPTION as isize
            } else {
                HTCLIENT as isize
            }
        }
        WM_LBUTTONUP => {
            let mut client: RECT = zeroed();
            GetClientRect(hwnd, &mut client);
            let x = (lp as i16) as i32 * W / client.right.max(1);
            let y = ((lp >> 16) as i16) as i32 * H / client.bottom.max(1);
            if (10..40).contains(&y) {
                if x >= 258 {
                    hide(hwnd);
                } else if x >= 224 {
                    toggle_pin(hwnd);
                }
            } else if (288..350).contains(&y) && (20..320).contains(&x) {
                show_reset_details(hwnd);
            } else if (611..641).contains(&y) && (252..320).contains(&x) {
                request_refresh(hwnd);
            } else if y >= 608 && x < 248 {
                let error = state(hwnd).and_then(|a| a.shared.lock().unwrap().error.clone());
                if let Some(error) = error {
                    MessageBoxW(
                        hwnd,
                        wide(&error).as_ptr(),
                        wide("刷新失败").as_ptr(),
                        MB_OK | MB_ICONINFORMATION,
                    );
                }
            }
            0
        }
        WM_RBUTTONUP => {
            menu(hwnd);
            0
        }
        TRAY => {
            match lp as u32 {
                WM_LBUTTONDBLCLK => {
                    ShowWindow(hwnd, SW_SHOW);
                    SetForegroundWindow(hwnd);
                }
                WM_RBUTTONUP => menu(hwnd),
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            hide(hwnd);
            0
        }
        WM_SYSCOMMAND if wp & 0xfff0 == SC_MINIMIZE as usize => {
            hide(hwnd);
            0
        }
        WM_DPICHANGED => {
            let scale = if let Some(a) = state(hwnd) {
                a.scale = (wp & 0xffff) as f32 / 96.0;
                a.scale
            } else {
                1.0
            };
            let r = &*(lp as *const RECT);
            SetWindowPos(
                hwnd,
                null_mut(),
                r.left,
                r.top,
                (W as f32 * scale) as i32,
                (H as f32 * scale) as i32,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, 1);
            if let Some(a) = state(hwnd) {
                tray(hwnd, a.icon, NIM_DELETE);
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn make_icon() -> HICON {
    // Tiny native gauge icon, no file or runtime image decoder required.
    let mut and = [0xffu8; 32];
    let mut xor = [0u8; 16 * 16 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let d = (x as f32 - 7.5).hypot(y as f32 - 7.5);
            if (4.5..=7.0).contains(&d) || ((7..=8).contains(&x) && (5..=9).contains(&y)) {
                and[y * 2 + x / 8] &= !(0x80 >> (x % 8));
                let i = (y * 16 + x) * 4;
                xor[i] = 0xc8;
                xor[i + 1] = 0xd8;
                xor[i + 2] = 0x59;
                xor[i + 3] = 255;
            }
        }
    }
    CreateIcon(
        GetModuleHandleW(null()),
        16,
        16,
        1,
        32,
        and.as_ptr(),
        xor.as_ptr(),
    )
}

pub unsafe fn run(shared: Arc<Mutex<Snapshot>>, refresh: Sender<()>, hidden: bool) {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let instance = GetModuleHandleW(null());
    let class = wide("CodexMeterWindow");
    let icon = make_icon();
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        hCursor: LoadCursorW(null_mut(), IDC_ARROW),
        hIcon: icon,
        ..zeroed()
    };
    RegisterClassW(&wc);
    let mut area: RECT = zeroed();
    SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut area as *mut _ as *mut c_void, 0);
    let scale = (GetDpiForSystem() as f32 / 96.0)
        .min((area.bottom - area.top - 48) as f32 / H as f32)
        .max(0.75);
    let mut app = Box::new(App {
        shared,
        refresh,
        pinned: false,
        scale,
        icon,
        last: None,
        last_minute: 0,
    });
    let width = (W as f32 * scale) as i32;
    let height = (H as f32 * scale) as i32;
    let hwnd = CreateWindowExW(
        WS_EX_APPWINDOW,
        class.as_ptr(),
        wide("Codex Meter").as_ptr(),
        WS_POPUP | WS_MINIMIZEBOX | WS_SYSMENU,
        area.right - width - 24,
        area.top + 24,
        width,
        height,
        null_mut(),
        null_mut(),
        instance,
        &mut *app as *mut _ as *const c_void,
    );
    if hwnd.is_null() {
        MessageBoxW(
            null_mut(),
            wide("无法创建窗口").as_ptr(),
            wide("Codex Meter").as_ptr(),
            MB_OK | MB_ICONERROR,
        );
        return;
    }
    let corner: u32 = 2;
    DwmSetWindowAttribute(hwnd, 33, &corner as *const _ as *const c_void, 4);
    tray(hwnd, icon, NIM_ADD);
    SetTimer(hwnd, 1, 1000, None);
    // Explorer restart removes tray icons; recreate ours when the taskbar returns.
    let taskbar = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
    if !hidden {
        ShowWindow(hwnd, SW_SHOW);
    } else {
        trim_hidden();
    }
    let mut msg: MSG = zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        if msg.message == taskbar {
            tray(hwnd, icon, NIM_ADD);
        }
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    DestroyIcon(icon);
}

pub struct Instance(HANDLE);
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                CloseHandle(self.0);
            }
        }
    }
}
pub unsafe fn single_instance() -> Option<Instance> {
    let mutex = CreateMutexW(null(), 0, wide("Local\\CodexMeter.Singleton").as_ptr());
    if !mutex.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
        let existing = FindWindowW(wide("CodexMeterWindow").as_ptr(), null());
        if !existing.is_null() {
            ShowWindow(existing, SW_SHOW);
            SetForegroundWindow(existing);
        }
        CloseHandle(mutex);
        return None;
    }
    Some(Instance(mutex))
}

pub unsafe fn preview(s: &Snapshot, path: &str) -> std::io::Result<()> {
    render_preview(s, path, W, H, |dc, s| paint(dc, s, false))
}

pub unsafe fn preview_reset(s: &Snapshot, path: &str) -> std::io::Result<()> {
    let layout = reset_panel::layout(s);
    render_preview(s, path, layout.width, layout.height, |dc, s| {
        reset_panel::paint(dc, s, layout)
    })
}

unsafe fn render_preview(
    s: &Snapshot,
    path: &str,
    width: i32,
    height: i32,
    draw: impl FnOnce(HDC, &Snapshot),
) -> std::io::Result<()> {
    // Offscreen rendering uses exactly the native window's paint routine.
    let dc = CreateCompatibleDC(null_mut());
    let mut info: BITMAPINFO = zeroed();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    let mut bits = null_mut();
    let bmp = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bmp.is_null() {
        DeleteDC(dc);
        return Err(std::io::Error::other("CreateDIBSection failed"));
    }
    let old = SelectObject(dc, bmp);
    draw(dc, s);
    GdiFlush();
    let size = (width * height * 4) as usize;
    let mut bytes = Vec::with_capacity(size + 54);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&((size + 54) as u32).to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(std::slice::from_raw_parts(
        &info.bmiHeader as *const _ as *const u8,
        40,
    ));
    bytes.extend_from_slice(std::slice::from_raw_parts(bits as *const u8, size));
    let result = std::fs::write(path, bytes);
    SelectObject(dc, old);
    DeleteObject(bmp);
    DeleteDC(dc);
    result
}
