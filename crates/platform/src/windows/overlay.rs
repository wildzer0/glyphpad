//! Feedback overlay (SPEC §13): a small always-on-top panel that shows the
//! stroke while drawing and the recognized gesture afterwards, and doubles
//! as the display of `notify` messages.
//!
//! It is a plain layered Win32 window on its own thread, independent of the
//! GUI, so it works while the main window is hidden. It never takes focus
//! or clicks: `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE |
//! WS_EX_TOOLWINDOW`, topmost. Transparency uses a color key for the
//! rounded corners and a global alpha for the configured opacity.

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleBitmap,
    CreateCompatibleDC, CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DT_CENTER,
    DT_END_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, DrawTextW, Ellipse,
    EndPaint, FW_NORMAL, FW_SEMIBOLD, FillRect, GetStockObject, HDC, HGDIOBJ, InvalidateRect,
    NULL_BRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, Polyline, RoundRect, SRCCOPY,
    SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, HWND_TOPMOST,
    KillTimer, LWA_ALPHA, LWA_COLORKEY, MSG, PostMessageW, RegisterClassExW, SPI_GETWORKAREA,
    SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SetLayeredWindowAttributes, SetTimer, SetWindowPos, ShowWindow, SystemParametersInfoW, WM_APP,
    WM_PAINT, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

use glyphpad_core::config::{Corner, OverlaySettings};

/// "Something changed, repaint".
const WM_REFRESH: u32 = WM_APP + 1;
const HIDE_TIMER: usize = 1;
/// Pixels never painted: magenta does not occur in the panel.
const KEY: COLORREF = rgb(255, 0, 255);
const MARGIN: i32 = 16;
const TEXT_H: i32 = 46;

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Message {
    pub title: String,
    pub detail: String,
    /// Green accent for a recognized gesture, amber otherwise.
    pub success: bool,
}

#[derive(Debug, Clone, Default)]
struct State {
    settings: OverlaySettings,
    /// Height / width of the touchpad, for the trail's frame.
    aspect: f32,
    /// Strokes in 0..1 surface coordinates.
    trail: Vec<Vec<(f32, f32)>>,
    message: Option<Message>,
    visible: bool,
    /// Hide after the configured duration (a message); a trail being drawn stays.
    timed: bool,
    /// Change of size or opacity pending.
    relayout: bool,
}

/// Handle to the overlay thread; cheap to clone.
#[derive(Clone)]
pub struct Overlay {
    state: Arc<Mutex<State>>,
    hwnd: Arc<AtomicIsize>,
}

thread_local! {
    static SHARED: RefCell<Option<Arc<Mutex<State>>>> = const { RefCell::new(None) };
}

impl Overlay {
    pub fn spawn(settings: OverlaySettings) -> Self {
        let state = Arc::new(Mutex::new(State {
            settings,
            aspect: 0.7,
            relayout: true,
            ..Default::default()
        }));
        let hwnd = Arc::new(AtomicIsize::new(0));
        let (s, h) = (state.clone(), hwnd.clone());
        std::thread::Builder::new()
            .name("overlay".into())
            .spawn(move || run(s, h))
            .expect("cannot spawn the overlay thread");
        Self { state, hwnd }
    }

    fn update(&self, f: impl FnOnce(&mut State)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
        let hwnd = self.hwnd.load(Ordering::Acquire);
        if hwnd != 0 {
            // SAFETY: posting a message with no payload to our own window.
            let _ = unsafe {
                PostMessageW(Some(HWND(hwnd as *mut _)), WM_REFRESH, WPARAM(0), LPARAM(0))
            };
        }
    }

    pub fn configure(&self, settings: OverlaySettings) {
        self.update(|s| {
            s.settings = settings;
            s.relayout = true;
            if !s.settings.enabled {
                s.visible = false;
            }
        });
    }

    pub fn set_touchpad(&self, width_mm: f32, height_mm: f32) {
        if width_mm > 0.0 {
            self.update(|s| {
                s.aspect = (height_mm / width_mm).clamp(0.3, 1.5);
                s.relayout = true;
            });
        }
    }

    /// Shows the stroke being drawn.
    pub fn trail(&self, strokes: Vec<Vec<(f32, f32)>>) {
        self.update(|s| {
            if s.settings.enabled {
                s.trail = strokes;
                s.message = Some(Message {
                    title: "Drawing…".into(),
                    detail: String::new(),
                    success: true,
                });
                s.visible = true;
                s.timed = false;
            }
        });
    }

    /// Shows a message for the configured duration.
    pub fn show(&self, message: Message) {
        self.update(|s| {
            if s.settings.enabled {
                s.message = Some(message);
                s.visible = true;
                s.timed = true;
            }
        });
    }
}

fn run(state: Arc<Mutex<State>>, hwnd_out: Arc<AtomicIsize>) {
    SHARED.with_borrow_mut(|s| *s = Some(state));
    let class = w!("GlyphPadOverlay");
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
            w!("GlyphPad overlay"),
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
                tracing::warn!("cannot create the overlay: {e}");
                return;
            }
        }
    };
    hwnd_out.store(hwnd.0 as isize, Ordering::Release);
    refresh(hwnd);
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

/// Applies the shared state: size, position, opacity, visibility.
fn refresh(hwnd: HWND) {
    let Some((visible, relayout, settings, aspect, timed)) = with_state(|s| {
        let r = (s.visible, s.relayout, s.settings.clone(), s.aspect, s.timed);
        s.relayout = false;
        r
    }) else {
        return;
    };
    // SAFETY: window calls on our own window handle.
    unsafe {
        if relayout {
            let width = settings.width_px as i32;
            let height = (width as f32 * aspect) as i32 + TEXT_H + MARGIN;
            let mut work = RECT::default();
            let _ = SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some((&raw mut work).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
            let (x, y) = match settings.position {
                Corner::TopLeft => (work.left + MARGIN, work.top + MARGIN),
                Corner::TopRight => (work.right - width - MARGIN, work.top + MARGIN),
                Corner::BottomLeft => (work.left + MARGIN, work.bottom - height - MARGIN),
                Corner::BottomRight => (work.right - width - MARGIN, work.bottom - height - MARGIN),
            };
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE,
            );
            let alpha = (settings.opacity.clamp(0.05, 1.0) * 255.0) as u8;
            let _ = SetLayeredWindowAttributes(hwnd, KEY, alpha, LWA_COLORKEY | LWA_ALPHA);
        }
        let _ = InvalidateRect(Some(hwnd), None, false);
        if visible {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            // While drawing the panel stays; afterwards it hides after the duration.
            let _ = KillTimer(Some(hwnd), HIDE_TIMER);
            if timed {
                SetTimer(Some(hwnd), HIDE_TIMER, settings.duration_ms as u32, None);
            }
        } else {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let handled = std::panic::catch_unwind(|| match msg {
        WM_REFRESH => {
            refresh(hwnd);
            true
        }
        WM_TIMER if wparam.0 == HIDE_TIMER => {
            with_state(|s| {
                s.visible = false;
                s.timed = false;
                s.trail.clear();
                s.message = None;
            });
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

/// Double-buffered GDI painting of the panel.
fn paint(hwnd: HWND) {
    let Some((trail, message)) = with_state(|s| {
        // A finished drawing keeps its trail until the next message hides it.
        let trail = s.trail.clone();
        (trail, s.message.clone())
    }) else {
        return;
    };
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

        // Panel.
        let panel = CreateSolidBrush(rgb(30, 31, 36));
        let border = CreatePen(PS_SOLID, 1, rgb(70, 72, 80));
        let old_brush = SelectObject(mem, panel.into());
        let old_pen = SelectObject(mem, border.into());
        let _ = RoundRect(mem, 0, 0, w, h, 18, 18);

        // Touchpad frame.
        let pad = RECT {
            left: MARGIN / 2 + 4,
            top: MARGIN / 2 + 4,
            right: w - MARGIN / 2 - 4,
            bottom: h - TEXT_H,
        };
        let frame_pen = CreatePen(PS_SOLID, 1, rgb(90, 92, 102));
        SelectObject(mem, frame_pen.into());
        SelectObject(mem, GetStockObject(NULL_BRUSH));
        let _ = RoundRect(mem, pad.left, pad.top, pad.right, pad.bottom, 10, 10);

        // Trail: one color per stroke, start marked with a dot.
        let colors = [
            rgb(90, 170, 255),
            rgb(255, 170, 70),
            rgb(120, 220, 140),
            rgb(230, 110, 200),
        ];
        let (pw, ph) = ((pad.right - pad.left) as f32, (pad.bottom - pad.top) as f32);
        let to_px = |(x, y): (f32, f32)| windows::Win32::Foundation::POINT {
            x: pad.left + (x.clamp(0.0, 1.0) * pw) as i32,
            y: pad.top + (y.clamp(0.0, 1.0) * ph) as i32,
        };
        let mut pens: Vec<HGDIOBJ> = Vec::new();
        for (i, stroke) in trail.iter().enumerate() {
            let pen = CreatePen(PS_SOLID, 3, colors[i % colors.len()]);
            pens.push(pen.into());
            SelectObject(mem, pen.into());
            let pts: Vec<_> = stroke.iter().map(|&p| to_px(p)).collect();
            if pts.len() >= 2 {
                let _ = Polyline(mem, &pts);
            }
            if let Some(p) = pts.first() {
                let dot = CreateSolidBrush(colors[i % colors.len()]);
                SelectObject(mem, dot.into());
                let _ = Ellipse(mem, p.x - 4, p.y - 4, p.x + 5, p.y + 5);
                SelectObject(mem, GetStockObject(NULL_BRUSH));
                let _ = DeleteObject(dot.into());
            }
        }

        if let Some(m) = message {
            SetBkMode(mem, TRANSPARENT);
            let title_font = font(17, FW_SEMIBOLD.0 as i32);
            let detail_font = font(13, FW_NORMAL.0 as i32);
            let mut title_rc = RECT {
                left: 8,
                top: h - TEXT_H,
                right: w - 8,
                bottom: h - TEXT_H / 2 + 2,
            };
            let mut detail_rc = RECT {
                left: 8,
                top: h - TEXT_H / 2,
                right: w - 8,
                bottom: h - 4,
            };
            SetTextColor(
                mem,
                if m.success {
                    rgb(140, 230, 160)
                } else {
                    rgb(255, 196, 90)
                },
            );
            draw_text(mem, title_font, &m.title, &mut title_rc);
            SetTextColor(mem, rgb(200, 202, 210));
            draw_text(mem, detail_font, &m.detail, &mut detail_rc);
            let _ = DeleteObject(title_font);
            let _ = DeleteObject(detail_font);
        }

        let _ = BitBlt(hdc, 0, 0, w, h, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old_pen);
        SelectObject(mem, old_brush);
        SelectObject(mem, old_bmp);
        for p in pens {
            let _ = DeleteObject(p);
        }
        let _ = DeleteObject(frame_pen.into());
        let _ = DeleteObject(border.into());
        let _ = DeleteObject(panel.into());
        let _ = DeleteObject(bmp.into());
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
            DT_CENTER | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );
        SelectObject(hdc, old);
    }
}
