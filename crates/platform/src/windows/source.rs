//! Raw Input touch source: a hidden window receives `WM_INPUT` for every
//! Precision Touchpad, in the background, and turns reports into frames.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use crossbeam_channel::Sender;
use glyphpad_core::{DeviceEvent, DeviceId, DeviceInfo, SourceError, TouchSource};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWHID, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT,
    RIDEV_DEVNOTIFY, RIDEV_INPUTSINK, RIM_TYPEHID, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GIDC_ARRIVAL, GIDC_REMOVAL,
    GetMessageW, HWND_MESSAGE, MSG, PostQuitMessage, RegisterClassExW, WINDOW_EX_STYLE, WM_INPUT,
    WM_INPUT_DEVICE_CHANGE, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::w;

use super::device::{Touchpad, touchpads};
use crate::ptp::{FrameAssembler, ScanClock, usage};

/// Which kind of hidden window receives Raw Input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowKind {
    /// A message-only window (`HWND_MESSAGE`): never visible, not enumerable.
    #[default]
    MessageOnly,
    /// A top-level window that is never shown, in case message-only windows
    /// do not receive background input on some system (SPEC §4).
    HiddenTopLevel,
}

/// Reads every Precision Touchpad through Raw Input (SPEC §4).
#[derive(Debug, Default)]
pub struct RawInputSource {
    window: WindowKind,
}

impl RawInputSource {
    pub fn new(window: WindowKind) -> Self {
        Self { window }
    }
}

impl TouchSource for RawInputSource {
    fn devices(&self) -> Vec<DeviceInfo> {
        touchpads()
            .into_iter()
            .enumerate()
            .map(|(i, (_, pad))| pad.info(DeviceId(i as u32 + 1)))
            .collect()
    }

    fn run(self, tx: Sender<DeviceEvent>) -> Result<(), SourceError> {
        let hwnd = create_window(self.window)?;
        let result = run_on_window(hwnd, tx);
        // SAFETY: the window was created on this thread and is destroyed once.
        let _ = unsafe { DestroyWindow(hwnd) };
        STATE.with_borrow_mut(|s| *s = None);
        result
    }
}

fn run_on_window(hwnd: HWND, tx: Sender<DeviceEvent>) -> Result<(), SourceError> {
    STATE.with_borrow_mut(|s| {
        *s = Some(State {
            tx,
            devices: HashMap::new(),
            next_id: 1,
            epoch: Instant::now(),
            buf: Vec::new(),
            disconnected: false,
        })
    });
    // Devices already present. WM_INPUT_DEVICE_CHANGE also reports them on
    // registration on current Windows versions; duplicates are ignored.
    for (handle, pad) in touchpads() {
        with_state(|s| s.add(handle, pad));
    }

    let device = RAWINPUTDEVICE {
        usUsagePage: usage::PAGE_DIGITIZER,
        usUsage: usage::TOUCH_PAD,
        // INPUTSINK: receive input while another application is in the
        // foreground. DEVNOTIFY: receive WM_INPUT_DEVICE_CHANGE for hot-plug.
        dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
        hwndTarget: hwnd,
    };
    // SAFETY: one valid RAWINPUTDEVICE targeting a window owned by this thread.
    unsafe { RegisterRawInputDevices(&[device], std::mem::size_of::<RAWINPUTDEVICE>() as u32)? };
    tracing::info!("listening for Precision Touchpad input");

    let mut msg = MSG::default();
    loop {
        // SAFETY: `msg` is a valid out pointer; messages for any window of this thread.
        let r = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if r.0 == 0 || r.0 == -1 {
            break; // WM_QUIT, or an error on an invalid parameter.
        }
        // SAFETY: `msg` was filled by GetMessageW. No TranslateMessage: no keyboard input here.
        unsafe { DispatchMessageW(&msg) };
    }
    Ok(())
}

fn create_window(kind: WindowKind) -> Result<HWND, SourceError> {
    let class = w!("GlyphPadRawInput");
    // SAFETY: plain Win32 window creation with static strings and a valid window procedure.
    unsafe {
        let instance = GetModuleHandleW(None)?.into();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        };
        // Fails harmlessly with ERROR_CLASS_ALREADY_EXISTS when the source is restarted.
        RegisterClassExW(&wc);
        let (ex_style, parent) = match kind {
            WindowKind::MessageOnly => (WINDOW_EX_STYLE(0), Some(HWND_MESSAGE)),
            // Tool window: no taskbar button even if it were ever shown.
            WindowKind::HiddenTopLevel => (WS_EX_TOOLWINDOW, None),
        };
        let hwnd = CreateWindowExW(
            ex_style,
            class,
            w!("GlyphPad"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            parent,
            None,
            Some(instance),
            None,
        )?;
        Ok(hwnd)
    }
}

struct Device {
    id: DeviceId,
    pad: Touchpad,
    assembler: FrameAssembler,
    clock: ScanClock,
}

struct State {
    tx: Sender<DeviceEvent>,
    devices: HashMap<usize, Device>,
    next_id: u32,
    epoch: Instant,
    /// Reused WM_INPUT buffer, `u64` for RAWINPUT's alignment.
    buf: Vec<u64>,
    disconnected: bool,
}

thread_local! {
    // The window procedure has no context argument; the source owns the
    // thread, so per-thread state is the simplest safe way to reach it.
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with_state(f: impl FnOnce(&mut State)) {
    STATE.with_borrow_mut(|s| {
        if let Some(s) = s.as_mut() {
            f(s);
        }
    });
}

fn key(h: HANDLE) -> usize {
    h.0 as usize
}

impl State {
    fn send(&mut self, event: DeviceEvent) {
        if !self.disconnected && self.tx.send(event).is_err() {
            // Nobody is listening any more: end the message loop.
            self.disconnected = true;
            // SAFETY: called on the thread that runs the message loop.
            unsafe { PostQuitMessage(0) };
        }
    }

    fn add(&mut self, handle: HANDLE, pad: Touchpad) {
        if self.devices.contains_key(&key(handle)) {
            return;
        }
        let id = DeviceId(self.next_id);
        self.next_id += 1;
        let info = pad.info(id);
        tracing::info!(
            name = %info.name,
            "touchpad connected: {:.1}×{:.1} mm, {} contacts ({:?}), {} finger slots per report",
            info.width_mm,
            info.height_mm,
            info.max_contacts,
            pad.max_contacts_source,
            pad.layout.fingers.len()
        );
        self.devices.insert(
            key(handle),
            Device {
                id,
                pad,
                assembler: FrameAssembler::default(),
                clock: ScanClock::default(),
            },
        );
        self.send(DeviceEvent::Connected(info));
    }

    fn remove(&mut self, handle: HANDLE) {
        if let Some(dev) = self.devices.remove(&key(handle)) {
            tracing::info!(name = %dev.pad.name, "touchpad disconnected");
            self.send(DeviceEvent::Disconnected { device: dev.id });
        }
    }

    fn input(&mut self, lparam: LPARAM) {
        let mut buf = std::mem::take(&mut self.buf);
        self.input_into(&mut buf, lparam);
        self.buf = buf;
    }

    fn input_into(&mut self, buf: &mut Vec<u64>, lparam: LPARAM) {
        let handle = HRAWINPUT(lparam.0 as *mut _);
        let header_size = std::mem::size_of::<RAWINPUTHEADER>() as u32;
        let mut size = 0u32;
        // SAFETY: a null buffer queries the size of this WM_INPUT's data.
        if unsafe { GetRawInputData(handle, RID_INPUT, None, &mut size, header_size) } != 0 {
            return;
        }
        buf.resize((size as usize).div_ceil(8), 0);
        // SAFETY: `buf` holds at least `size` bytes.
        let read = unsafe {
            GetRawInputData(
                handle,
                RID_INPUT,
                Some(buf.as_mut_ptr().cast()),
                &mut size,
                header_size,
            )
        };
        if read == u32::MAX || (read as usize) < std::mem::size_of::<RAWINPUTHEADER>() {
            return;
        }
        let bytes = &mut bytemuck_bytes(buf)[..read as usize];

        // SAFETY: the buffer starts with a RAWINPUT filled by GetRawInputData and is 8-byte aligned.
        let raw = unsafe { &*(bytes.as_ptr() as *const RAWINPUT) };
        if raw.header.dwType != RIM_TYPEHID.0 {
            return;
        }
        let device_handle = raw.header.hDevice;
        // SAFETY: `dwType == RIM_TYPEHID` selects the `hid` variant.
        let (report_size, count) = unsafe { (raw.data.hid.dwSizeHid, raw.data.hid.dwCount) };

        if !self.devices.contains_key(&key(device_handle)) {
            // Input before (or without) an arrival notification.
            match Touchpad::open(device_handle) {
                Some(pad) => self.add(device_handle, pad),
                None => return,
            }
        }

        // `dwCount` reports of `dwSizeHid` bytes each follow `bRawData`'s offset.
        let offset = std::mem::offset_of!(RAWINPUT, data) + std::mem::offset_of!(RAWHID, bRawData);
        let total = report_size as usize * count as usize;
        let Some(reports) = bytes.get_mut(offset..offset + total) else {
            tracing::warn!("truncated WM_INPUT: {count} × {report_size} B in {read} B");
            return;
        };
        if report_size == 0 {
            return;
        }

        let host_us = self.epoch.elapsed().as_micros() as u64;
        let mut frames = Vec::new();
        if let Some(dev) = self.devices.get_mut(&key(device_handle)) {
            for report in reports.chunks_exact_mut(report_size as usize) {
                match dev.pad.layout.parse(&dev.pad.preparsed, report) {
                    Ok(Some(raw)) => {
                        if let Some(frame) = dev.assembler.push(raw) {
                            let ts = dev.clock.timestamp(frame.scan_time, host_us);
                            frames.push(dev.pad.geometry.frame(&frame, ts));
                        }
                    }
                    Ok(None) => {}
                    Err(e) => tracing::debug!("unparsable report: {e}"),
                }
            }
            let device = dev.id;
            for frame in frames {
                self.send(DeviceEvent::Frame { device, frame });
            }
        }
    }
}

/// Views a `u64` buffer as bytes.
fn bytemuck_bytes(buf: &mut [u64]) -> &mut [u8] {
    // SAFETY: u8 has no alignment or validity requirements and the length covers exactly the buffer.
    unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr().cast(), std::mem::size_of_val(buf)) }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Unwinding out of an `extern "system"` callback aborts the process, so
    // contain panics here and keep the message loop alive.
    let handled = std::panic::catch_unwind(|| match msg {
        WM_INPUT => with_state(|s| s.input(lparam)),
        WM_INPUT_DEVICE_CHANGE => {
            let device = HANDLE(lparam.0 as *mut _);
            match wparam.0 as u32 {
                GIDC_ARRIVAL => {
                    if let Some(pad) = Touchpad::open(device) {
                        with_state(|s| s.add(device, pad));
                    }
                }
                GIDC_REMOVAL => with_state(|s| s.remove(device)),
                _ => {}
            }
        }
        _ => {}
    });
    if handled.is_err() {
        tracing::error!("panic while handling window message {msg:#x}");
    }
    // DefWindowProc must also see WM_INPUT so the system can free the input data.
    // SAFETY: forwarding the original arguments of this window procedure call.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
