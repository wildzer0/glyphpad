//! The cheat sheet (SPEC §13): a panel listing the active gestures with a
//! small picture of each, shown for a few seconds. Like the feedback
//! overlay it is a layered, click-through, never-focused window on its own
//! thread, so it appears over any application and while GlyphPad's own
//! window is hidden.

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};

use glyphpad_core::preview::Pictogram;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DT_END_ELLIPSIS,
    DT_LEFT, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint,
    FW_NORMAL, FW_SEMIBOLD, FillRect, GetStockObject, HDC, HGDIOBJ, InvalidateRect, NULL_BRUSH,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, Polyline, RoundRect, SRCCOPY, SelectObject,
    SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForSystem;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, HWND_TOPMOST,
    KillTimer, LWA_ALPHA, LWA_COLORKEY, MSG, PostMessageW, RegisterClassExW, SPI_GETWORKAREA,
    SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow, SystemParametersInfoW, WM_APP,
    WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

const WM_REFRESH: u32 = WM_APP + 1;
const HIDE_TIMER: usize = 1;
/// Pixels never painted: magenta does not occur in the panel.
const KEY: COLORREF = rgb(255, 0, 255);

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    pub name: String,
    /// What to do on the touchpad, in words.
    pub trigger: String,
    /// What it does.
    pub action: String,
    pub picture: Pictogram,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sheet {
    pub title: String,
    pub rows: Vec<Row>,
}

#[derive(Default)]
struct State {
    sheet: Sheet,
    seconds: u32,
    visible: bool,
    /// Rows that did not fit.
    hidden_rows: usize,
}

#[derive(Clone)]
pub struct CheatSheet {
    state: Arc<Mutex<State>>,
    hwnd: Arc<AtomicIsize>,
}

thread_local! {
    static SHARED: RefCell<Option<Arc<Mutex<State>>>> = const { RefCell::new(None) };
}

/// Geometry in device pixels at the system's scale.
struct Layout {
    scale: f32,
    cols: usize,
    per_col: usize,
    cell_w: i32,
    cell_h: i32,
    pad: i32,
    header: i32,
    width: i32,
    height: i32,
}

fn layout(rows: usize, work: &RECT, scale: f32) -> Layout {
    let px = |v: f32| (v * scale) as i32;
    let (cell_w, cell_h, pad, header) = (px(390.0), px(74.0), px(22.0), px(50.0));
    let (work_w, work_h) = (work.right - work.left, work.bottom - work.top);
    let max_cols = ((work_w as f32 * 0.92) as i32 - 2 * pad).max(cell_w) / cell_w;
    let max_per_col = (((work_h as f32 * 0.88) as i32 - header - 2 * pad) / cell_h).max(3);
    let rows = rows.max(1);
    let cols = (rows.div_ceil(max_per_col as usize)).clamp(1, max_cols as usize);
    let per_col = rows.div_ceil(cols).min(max_per_col as usize);
    Layout {
        scale,
        cols,
        per_col,
        cell_w,
        cell_h,
        pad,
        header,
        width: cols as i32 * cell_w + 2 * pad,
        height: per_col as i32 * cell_h + header + 2 * pad,
    }
}

fn work_area() -> RECT {
    let mut work = RECT::default();
    // SAFETY: a valid out pointer for SPI_GETWORKAREA.
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut work).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    work
}

fn scale() -> f32 {
    // SAFETY: no arguments.
    (unsafe { GetDpiForSystem() }.max(96)) as f32 / 96.0
}

impl CheatSheet {
    pub fn spawn() -> Self {
        let state = Arc::new(Mutex::new(State::default()));
        let hwnd = Arc::new(AtomicIsize::new(0));
        let (s, h) = (state.clone(), hwnd.clone());
        std::thread::Builder::new()
            .name("cheat-sheet".into())
            .spawn(move || run(s, h))
            .expect("cannot spawn the cheat sheet thread");
        Self { state, hwnd }
    }

    /// Shows `sheet` for `seconds`.
    pub fn show(&self, sheet: Sheet, seconds: u64) {
        if let Ok(mut s) = self.state.lock() {
            s.sheet = sheet;
            s.seconds = seconds.clamp(1, 120) as u32;
            s.visible = true;
        }
        let hwnd = self.hwnd.load(Ordering::Acquire);
        if hwnd != 0 {
            // SAFETY: posting a message with no payload to our own window.
            let _ = unsafe {
                PostMessageW(Some(HWND(hwnd as *mut _)), WM_REFRESH, WPARAM(0), LPARAM(0))
            };
        }
    }
}

fn run(state: Arc<Mutex<State>>, hwnd_out: Arc<AtomicIsize>) {
    SHARED.with_borrow_mut(|s| *s = Some(state));
    let class = w!("GlyphPadCheatSheet");
    // SAFETY: plain window creation with static strings and a valid window procedure.
    let hwnd = unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let instance = module.into();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassExW(&wc);
        match CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            w!("GlyphPad cheat sheet"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(instance),
            None,
        ) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("cannot create the cheat sheet: {e}");
                return;
            }
        }
    };
    hwnd_out.store(hwnd.0 as isize, Ordering::Release);
    let mut msg = MSG::default();
    // SAFETY: standard message loop for this thread's window.
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        unsafe { DispatchMessageW(&msg) };
    }
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    SHARED.with_borrow(|s| {
        s.as_ref()
            .and_then(|s| s.lock().ok().map(|mut s| f(&mut s)))
    })
}

fn refresh(hwnd: HWND) {
    let Some((rows, seconds)) = with_state(|s| (s.sheet.rows.len(), s.seconds)) else {
        return;
    };
    let work = work_area();
    let l = layout(rows, &work, scale());
    with_state(|s| s.hidden_rows = rows.saturating_sub(l.cols * l.per_col));
    let x = work.left + ((work.right - work.left) - l.width) / 2;
    let y = work.top + ((work.bottom - work.top) - l.height) / 2;
    // SAFETY: window calls on our own window handle.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            l.width,
            l.height,
            SWP_NOACTIVATE,
        );
        let _ = SetLayeredWindowAttributes(hwnd, KEY, 245, LWA_COLORKEY | LWA_ALPHA);
        let _ = InvalidateRect(Some(hwnd), None, false);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = KillTimer(Some(hwnd), HIDE_TIMER);
        SetTimer(Some(hwnd), HIDE_TIMER, seconds * 1000, None);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let handled = std::panic::catch_unwind(|| match msg {
        WM_REFRESH => {
            refresh(hwnd);
            true
        }
        WM_TIMER if wparam.0 == HIDE_TIMER => {
            with_state(|s| s.visible = false);
            // SAFETY: our own window and timer.
            unsafe {
                let _ = KillTimer(Some(hwnd), HIDE_TIMER);
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            true
        }
        WM_PAINT => {
            paint(hwnd);
            true
        }
        _ => false,
    });
    if handled.unwrap_or(false) {
        return LRESULT(0);
    }
    // SAFETY: forwarding the original arguments.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

const BLUE: COLORREF = rgb(110, 160, 255);

fn paint(hwnd: HWND) {
    let Some((sheet, hidden)) = with_state(|s| (s.sheet.clone(), s.hidden_rows)) else {
        return;
    };
    let work = work_area();
    let l = layout(sheet.rows.len(), &work, scale());
    let px = |v: f32| (v * l.scale) as i32;
    // SAFETY: GDI objects are created, selected, restored and deleted within
    // this function; the paint struct pairs BeginPaint with EndPaint.
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc = RECT::default();
        let _ = GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w, h);
        let old_bmp = SelectObject(mem, bmp.into());
        let key = CreateSolidBrush(KEY);
        FillRect(mem, &rc, key);
        let _ = DeleteObject(key.into());

        let panel = CreateSolidBrush(rgb(24, 26, 31));
        let border = CreatePen(PS_SOLID, 1, rgb(70, 74, 84));
        let old_brush = SelectObject(mem, panel.into());
        let old_pen = SelectObject(mem, border.into());
        let _ = RoundRect(mem, 0, 0, w, h, px(22.0), px(22.0));

        SetBkMode(mem, TRANSPARENT);
        let title_font = font(px(20.0), FW_SEMIBOLD.0 as i32);
        let name_font = font(px(15.0), FW_SEMIBOLD.0 as i32);
        let note_font = font(px(12.5), FW_NORMAL.0 as i32);
        SetTextColor(mem, rgb(235, 237, 242));
        let mut title_rc = RECT {
            left: l.pad,
            top: l.pad - px(4.0),
            right: w - l.pad,
            bottom: l.pad + px(30.0),
        };
        draw_text(mem, title_font, &sheet.title, &mut title_rc);

        let frame_pen = CreatePen(PS_SOLID, 1, rgb(86, 90, 102));
        let line_pen = CreatePen(PS_SOLID, px(2.5).max(2), BLUE);
        let dot_brush = CreateSolidBrush(BLUE);
        let pic_w = px(78.0);
        let pic_h = px(54.0);
        for (i, row) in sheet.rows.iter().take(l.cols * l.per_col).enumerate() {
            let (col, r) = (i / l.per_col, i % l.per_col);
            let x = l.pad + col as i32 * l.cell_w;
            let y = l.pad + l.header + r as i32 * l.cell_h;
            // The picture sits on a small touchpad.
            let pad_rc = RECT {
                left: x,
                top: y + (l.cell_h - pic_h) / 2,
                right: x + pic_w,
                bottom: y + (l.cell_h - pic_h) / 2 + pic_h,
            };
            SelectObject(mem, frame_pen.into());
            SelectObject(mem, GetStockObject(NULL_BRUSH));
            let _ = RoundRect(
                mem,
                pad_rc.left,
                pad_rc.top,
                pad_rc.right,
                pad_rc.bottom,
                px(8.0),
                px(8.0),
            );
            let to_px = |(fx, fy): (f32, f32)| POINT {
                x: pad_rc.left + (fx.clamp(0.0, 1.0) * (pic_w - 1) as f32) as i32,
                y: pad_rc.top + (fy.clamp(0.0, 1.0) * (pic_h - 1) as f32) as i32,
            };
            SelectObject(mem, line_pen.into());
            for line in &row.picture.lines {
                let pts: Vec<POINT> = line.iter().map(|&p| to_px(p)).collect();
                if pts.len() >= 2 {
                    let _ = Polyline(mem, &pts);
                }
            }
            let r_dot = px(4.5);
            for &(dx, dy, ring) in &row.picture.dots {
                let c = to_px((dx, dy));
                if ring {
                    SelectObject(mem, GetStockObject(NULL_BRUSH));
                } else {
                    SelectObject(mem, dot_brush.into());
                }
                let _ = Ellipse(mem, c.x - r_dot, c.y - r_dot, c.x + r_dot, c.y + r_dot);
            }
            // Name above, trigger and action below.
            let text_left = x + pic_w + px(14.0);
            let text_right = x + l.cell_w - px(14.0);
            SetTextColor(mem, rgb(235, 237, 242));
            let mut name_rc = RECT {
                left: text_left,
                top: y + px(10.0),
                right: text_right,
                bottom: y + px(34.0),
            };
            draw_text(mem, name_font, &row.name, &mut name_rc);
            SetTextColor(mem, rgb(160, 166, 180));
            let detail = format!("{}  →  {}", row.trigger, row.action);
            let mut detail_rc = RECT {
                left: text_left,
                top: y + px(36.0),
                right: text_right,
                bottom: y + px(60.0),
            };
            draw_text(mem, note_font, &detail, &mut detail_rc);
        }
        if hidden > 0 {
            SetTextColor(mem, rgb(160, 166, 180));
            let mut more = RECT {
                left: w / 2,
                top: l.pad - px(2.0),
                right: w - l.pad,
                bottom: l.pad + px(28.0),
            };
            let text = format!("+{hidden} more");
            let mut wide: Vec<u16> = text.encode_utf16().collect();
            let old = SelectObject(mem, note_font);
            DrawTextW(
                mem,
                &mut wide,
                &mut more,
                windows::Win32::Graphics::Gdi::DT_RIGHT | DT_SINGLELINE | DT_VCENTER,
            );
            SelectObject(mem, old);
        }

        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old_pen);
        SelectObject(mem, old_brush);
        SelectObject(mem, old_bmp);
        for o in [
            title_font,
            name_font,
            note_font,
            frame_pen.into(),
            line_pen.into(),
            dot_brush.into(),
            border.into(),
            panel.into(),
            bmp.into(),
        ] {
            let _ = DeleteObject(o);
        }
        let _ = DeleteDC(mem);
        let _ = EndPaint(hwnd, &ps);
    }
}

/// # Safety
/// Plain GDI call; the caller deletes the returned font.
unsafe fn font(height: i32, weight: i32) -> HGDIOBJ {
    // SAFETY: static face name.
    unsafe {
        CreateFontW(
            -height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            w!("Segoe UI"),
        )
        .into()
    }
}

/// # Safety
/// `hdc` must be a valid device context and `font` a valid font.
unsafe fn draw_text(hdc: HDC, font: HGDIOBJ, text: &str, rc: &mut RECT) {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return;
    }
    // SAFETY: per the function contract; the previous font is restored.
    unsafe {
        let old = SelectObject(hdc, font);
        DrawTextW(
            hdc,
            &mut wide,
            rc,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );
        SelectObject(hdc, old);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> RECT {
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        }
    }

    #[test]
    fn few_rows_use_one_column_and_many_use_more() {
        let one = layout(6, &work(), 1.0);
        assert_eq!((one.cols, one.per_col), (1, 6));
        let many = layout(40, &work(), 1.0);
        assert!(many.cols >= 2 && many.cols * many.per_col >= 40.min(many.cols * many.per_col));
        assert!(many.height <= 1040);
        assert!(many.width <= 1920);
    }

    #[test]
    fn a_huge_list_is_cut_to_what_fits() {
        let l = layout(500, &work(), 1.0);
        assert!(l.cols * l.per_col < 500);
        assert!(l.width <= 1920 && l.height <= 1040);
    }
}
