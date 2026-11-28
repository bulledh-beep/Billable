//! Computer use for Mac chats (PROTOCOL.md, "Computer"): the `computer_*`
//! tools on the `billable` server. Billable itself takes the screenshots and
//! moves the mouse and keys, so it works the same for Claude Code and Codex,
//! and only Billable needs Screen Recording and Accessibility.
//!
//! The engine decides who may act (one chat at a time, and only in apps the
//! person allowed for that chat); this module does the acting. Coordinates
//! are in the last screenshot's pixels, which are scaled from the main
//! display's points by a fixed rule, so a click lands where the agent saw.

use std::time::Duration;

use serde_json::{json, Value};

/// Screenshots are at most this many pixels on their long side.
pub const SHOT_LONG_SIDE: f64 = 1280.0;
/// Tool names start with this on the `billable` server.
pub const PREFIX: &str = "computer_";

/// What a tool needs the person to have allowed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Needs {
    /// Seeing the screen.
    Screen,
    /// Acting in this app (the frontmost one, or the one being opened).
    App(String),
    /// Acting in whatever app is in front (asked of the system first).
    FrontApp,
    Nothing,
}

/// What each tool needs.
pub fn needs(tool: &str, args: &Value) -> Needs {
    match tool {
        "computer_screenshot" | "computer_cursor_position" => Needs::Screen,
        "computer_open_app" => match args.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => Needs::App(n.to_string()),
            None => Needs::Nothing,
        },
        "computer_wait" => Needs::Nothing,
        _ => Needs::FrontApp,
    }
}

/// The tools, for `tools/list`.
pub fn tools() -> Vec<Value> {
    let point = |what: &str| {
        json!({
            "x": {"type": "integer", "description": format!("{what}: pixels from the left of the latest screenshot")},
            "y": {"type": "integer", "description": format!("{what}: pixels from the top of the latest screenshot")}
        })
    };
    let with = |mut base: Value, extra: Value| {
        for (k, v) in extra.as_object().into_iter().flatten() {
            base[k] = v.clone();
        }
        base
    };
    let mods = json!({"modifiers": {"type": "array", "items": {"type": "string", "enum": ["cmd", "shift", "option", "control"]}, "description": "Keys held during the click."}});
    vec![
        json!({
            "name": "computer_screenshot",
            "description": "See the Mac's main screen. Take one before acting and after each step to check what happened. Coordinates for the other computer tools are in this image's pixels. Use the computer tools only when the person asks you to use an app or their screen.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "computer_click",
            "description": "Click at a point.",
            "inputSchema": {"type": "object", "properties": with(with(point("Where"), json!({
                "button": {"type": "string", "enum": ["left", "right", "middle"], "description": "Default left."},
                "count": {"type": "integer", "enum": [1, 2, 3], "description": "2 for a double click."}
            })), mods), "required": ["x", "y"]}
        }),
        json!({
            "name": "computer_move",
            "description": "Move the pointer (to hover).",
            "inputSchema": {"type": "object", "properties": point("Where"), "required": ["x", "y"]}
        }),
        json!({
            "name": "computer_drag",
            "description": "Press at one point, drag to another, and let go.",
            "inputSchema": {"type": "object", "properties": {
                "fromX": {"type": "integer"}, "fromY": {"type": "integer"}, "toX": {"type": "integer"}, "toY": {"type": "integer"}
            }, "required": ["fromX", "fromY", "toX", "toY"]}
        }),
        json!({
            "name": "computer_scroll",
            "description": "Scroll at a point.",
            "inputSchema": {"type": "object", "properties": with(point("Where"), json!({
                "direction": {"type": "string", "enum": ["up", "down", "left", "right"]},
                "amount": {"type": "integer", "description": "Lines, default 3."}
            })), "required": ["x", "y", "direction"]}
        }),
        json!({
            "name": "computer_type",
            "description": "Type text into whatever has focus.",
            "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}
        }),
        json!({
            "name": "computer_key",
            "description": "Press a key or shortcut, e.g. \"return\", \"escape\", \"cmd+s\", \"cmd+shift+4\", \"space\", \"left\".",
            "inputSchema": {"type": "object", "properties": {"keys": {"type": "string"}}, "required": ["keys"]}
        }),
        json!({
            "name": "computer_open_app",
            "description": "Open an app by name (or bring it to the front), e.g. \"DaVinci Resolve\", \"Safari\".",
            "inputSchema": {"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}
        }),
        json!({
            "name": "computer_cursor_position",
            "description": "Where the pointer is, in screenshot pixels.",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "computer_wait",
            "description": "Wait for something to load, up to 10 seconds.",
            "inputSchema": {"type": "object", "properties": {"seconds": {"type": "number"}}}
        }),
    ]
}

/// What Billable is allowed by macOS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    pub supported: bool,
    pub screen: bool,
    pub control: bool,
}

impl Access {
    pub fn to_json(self) -> Value {
        json!({ "supported": self.supported, "screen": self.screen, "control": self.control })
    }
}

/// The screenshot's size for a display of `w`×`h` points.
pub fn shot_size(w: f64, h: f64) -> (u32, u32) {
    let scale = (SHOT_LONG_SIDE / w.max(h)).min(1.0);
    ((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32)
}

/// A screenshot pixel → display points.
pub fn to_points(x: f64, y: f64, display: (f64, f64)) -> (f64, f64) {
    let (sw, sh) = shot_size(display.0, display.1);
    (x * display.0 / sw as f64, y * display.1 / sh as f64)
}

/// Display points → a screenshot pixel.
pub fn to_pixels(x: f64, y: f64, display: (f64, f64)) -> (i64, i64) {
    let (sw, sh) = shot_size(display.0, display.1);
    ((x * sw as f64 / display.0).round() as i64, (y * sh as f64 / display.1).round() as i64)
}

/// A shortcut like `cmd+shift+s` → (modifier names, key name).
pub fn parse_keys(keys: &str) -> Result<(Vec<&'static str>, String), String> {
    let mut mods = Vec::new();
    let parts: Vec<String> = keys.split('+').map(|p| p.trim().to_ascii_lowercase()).filter(|p| !p.is_empty()).collect();
    let Some((key, rest)) = parts.split_last() else { return Err("Say which key.".into()) };
    for m in rest {
        mods.push(match m.as_str() {
            "cmd" | "command" | "meta" | "super" => "cmd",
            "shift" => "shift",
            "option" | "opt" | "alt" => "option",
            "ctrl" | "control" => "control",
            other => return Err(format!("{other} isn't a modifier (use cmd, shift, option or control).")),
        });
    }
    if keycode(key).is_none() {
        return Err(format!("Billable doesn't know the key {key}."));
    }
    Ok((mods, key.clone()))
}

/// macOS virtual key codes (US layout).
pub fn keycode(key: &str) -> Option<u16> {
    let k = match key {
        "a" => 0, "s" => 1, "d" => 2, "f" => 3, "h" => 4, "g" => 5, "z" => 6, "x" => 7, "c" => 8, "v" => 9,
        "b" => 11, "q" => 12, "w" => 13, "e" => 14, "r" => 15, "y" => 16, "t" => 17, "1" => 18, "2" => 19,
        "3" => 20, "4" => 21, "6" => 22, "5" => 23, "=" | "equal" => 24, "9" => 25, "7" => 26, "-" | "minus" => 27,
        "8" => 28, "0" => 29, "]" => 30, "o" => 31, "u" => 32, "[" => 33, "i" => 34, "p" => 35,
        "return" | "enter" => 36, "l" => 37, "j" => 38, "'" => 39, "k" => 40, ";" => 41, "\\" => 42, "," | "comma" => 43,
        "/" | "slash" => 44, "n" => 45, "m" => 46, "." | "period" => 47, "tab" => 48, "space" => 49, "`" => 50,
        "backspace" | "delete" => 51, "escape" | "esc" => 53, "f17" => 64, "f18" => 79, "f19" => 80, "f20" => 90,
        "f5" => 96, "f6" => 97, "f7" => 98, "f3" => 99, "f8" => 100, "f9" => 101, "f11" => 103, "f13" => 105,
        "f16" => 106, "f14" => 107, "f10" => 109, "f12" => 111, "f15" => 113, "help" => 114, "home" => 115,
        "pageup" | "page_up" => 116, "forwarddelete" | "forward_delete" | "del" => 117, "f4" => 118, "end" => 119,
        "f2" => 120, "pagedown" | "page_down" => 121, "f1" => 122, "left" => 123, "right" => 124, "down" => 125, "up" => 126,
        _ => return None,
    };
    Some(k)
}

fn int(args: &Value, k: &str) -> Result<f64, String> {
    args.get(k).and_then(Value::as_f64).ok_or_else(|| format!("Give {k}."))
}

/// Run a tool. The answer is MCP content: text, or an image and a line.
pub fn perform(tool: &str, args: &Value) -> Result<Vec<Value>, String> {
    let text = |s: String| Ok(vec![json!({"type": "text", "text": s})]);
    match tool {
        "computer_wait" => {
            let secs = args.get("seconds").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.1, 10.0);
            std::thread::sleep(Duration::from_secs_f64(secs));
            text(format!("Waited {secs:.1}s."))
        }
        "computer_key" => {
            let (mods, key) = parse_keys(args.get("keys").and_then(Value::as_str).unwrap_or(""))?;
            mac::key(&mods, &key)?;
            text("Pressed.".into())
        }
        "computer_type" => {
            let t = args.get("text").and_then(Value::as_str).ok_or("Give the text.")?;
            mac::type_text(t)?;
            text("Typed.".into())
        }
        "computer_screenshot" => mac::screenshot(),
        "computer_cursor_position" => {
            let (x, y) = mac::cursor()?;
            text(json!({"x": x, "y": y}).to_string())
        }
        "computer_click" => {
            let button = args.get("button").and_then(Value::as_str).unwrap_or("left");
            let count = args.get("count").and_then(Value::as_u64).unwrap_or(1).clamp(1, 3) as i64;
            let mods: Vec<&str> = args.get("modifiers").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
            mac::click(int(args, "x")?, int(args, "y")?, button, count, &mods)?;
            text("Clicked.".into())
        }
        "computer_move" => {
            mac::move_to(int(args, "x")?, int(args, "y")?)?;
            text("Moved.".into())
        }
        "computer_drag" => {
            mac::drag((int(args, "fromX")?, int(args, "fromY")?), (int(args, "toX")?, int(args, "toY")?))?;
            text("Dragged.".into())
        }
        "computer_scroll" => {
            let amount = args.get("amount").and_then(Value::as_i64).unwrap_or(3).clamp(1, 50) as i32;
            let (dy, dx) = match args.get("direction").and_then(Value::as_str).unwrap_or("down") {
                "up" => (amount, 0),
                "down" => (-amount, 0),
                "left" => (0, amount),
                "right" => (0, -amount),
                _ => return Err("Scroll up, down, left or right.".into()),
            };
            mac::scroll(int(args, "x")?, int(args, "y")?, dy, dx)?;
            text("Scrolled.".into())
        }
        "computer_open_app" => {
            let name = args.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).ok_or("Name the app.")?;
            mac::open_app(name)?;
            text(format!("{name} is open."))
        }
        other => Err(format!("Billable has no tool called {other}.")),
    }
}

pub use mac::{access, front_app, request_control, request_screen};

#[cfg(target_os = "macos")]
mod mac {
    use std::{ffi::c_void, path::PathBuf, process::Command, time::Duration};

    use base64::Engine as _;
    use serde_json::{json, Value};

    use super::{shot_size, to_pixels, to_points, Access};
    use crate::process::run_with_timeout;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }
    type Ref = *mut c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGMainDisplayID() -> u32;
        fn CGDisplayBounds(display: u32) -> CGRect;
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
        fn CGEventCreate(source: Ref) -> Ref;
        fn CGEventGetLocation(event: Ref) -> CGPoint;
        fn CGEventCreateMouseEvent(source: Ref, kind: u32, at: CGPoint, button: u32) -> Ref;
        fn CGEventCreateKeyboardEvent(source: Ref, key: u16, down: bool) -> Ref;
        fn CGEventKeyboardSetUnicodeString(event: Ref, length: usize, chars: *const u16);
        fn CGEventCreateScrollWheelEvent(source: Ref, units: u32, count: u32, wheel1: i32, ...) -> Ref;
        fn CGEventSetIntegerValueField(event: Ref, field: u32, value: i64);
        fn CGEventSetFlags(event: Ref, flags: u64);
        fn CGEventPost(tap: u32, event: Ref);
    }
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> u8;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(r: *const c_void);
    }

    const HID: u32 = 0;
    const MOUSE_MOVED: u32 = 5;
    const LEFT_DRAGGED: u32 = 6;
    const CLICK_STATE: u32 = 1;
    const SCROLL_LINES: u32 = 1;

    fn post(event: Ref) -> Result<(), String> {
        if event.is_null() {
            return Err("macOS didn't make the event.".into());
        }
        unsafe {
            CGEventPost(HID, event);
            CFRelease(event);
        }
        Ok(())
    }

    fn flags(mods: &[&str]) -> u64 {
        mods.iter().fold(0, |f, m| {
            f | match *m {
                "shift" => 0x20000,
                "control" => 0x40000,
                "option" => 0x80000,
                "cmd" => 0x100000,
                _ => 0,
            }
        })
    }

    fn display() -> (f64, f64) {
        let r = unsafe { CGDisplayBounds(CGMainDisplayID()) };
        (r.size.width, r.size.height)
    }

    fn point(x: f64, y: f64) -> CGPoint {
        let d = display();
        let (px, py) = to_points(x.clamp(0.0, 100_000.0), y.clamp(0.0, 100_000.0), d);
        CGPoint { x: px.min(d.0 - 1.0), y: py.min(d.1 - 1.0) }
    }

    fn need_control() -> Result<(), String> {
        if unsafe { AXIsProcessTrusted() } != 0 {
            Ok(())
        } else {
            Err("Billable needs Accessibility to click and type. Turn it on in Agents → Tools.".into())
        }
    }

    pub fn access() -> Access {
        Access { supported: true, screen: unsafe { CGPreflightScreenCaptureAccess() }, control: unsafe { AXIsProcessTrusted() } != 0 }
    }

    /// Ask macOS for Screen Recording (its own prompt, once), then open its
    /// settings pane.
    pub fn request_screen() {
        unsafe {
            CGRequestScreenCaptureAccess();
        }
        let _ = Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture").spawn();
    }

    pub fn request_control() {
        let _ = Command::new("/usr/bin/open").arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility").spawn();
    }

    /// The app in front, by its name.
    pub fn front_app() -> Option<String> {
        let front = run_with_timeout(Command::new("/usr/bin/lsappinfo").arg("front"), Duration::from_secs(2))?;
        let asn = front.stdout.trim().to_string();
        if asn.is_empty() {
            return None;
        }
        let info = run_with_timeout(Command::new("/usr/bin/lsappinfo").args(["info", "-only", "name", &asn]), Duration::from_secs(2))?;
        // "LSDisplayName"="Safari"
        let v = info.stdout.split_once('=')?.1.trim().trim_matches('"').to_string();
        (!v.is_empty()).then_some(v)
    }

    pub fn screenshot() -> Result<Vec<Value>, String> {
        if !unsafe { CGPreflightScreenCaptureAccess() } {
            return Err("Billable needs Screen Recording to see the screen. Turn it on in Agents → Tools.".into());
        }
        let path: PathBuf = std::env::temp_dir().join(format!("billable-shot-{}.jpg", crate::util::new_id("s")));
        let p = path.to_string_lossy().to_string();
        let shot = run_with_timeout(Command::new("/usr/sbin/screencapture").args(["-x", "-C", "-m", "-t", "jpg", &p]), Duration::from_secs(10));
        if shot.is_none_or(|o| o.status != Some(0)) || !path.exists() {
            let _ = std::fs::remove_file(&path);
            return Err("macOS didn't take the screenshot.".into());
        }
        let d = display();
        let (w, h) = shot_size(d.0, d.1);
        let _ = run_with_timeout(Command::new("/usr/bin/sips").args(["-z", &h.to_string(), &w.to_string(), &p]), Duration::from_secs(10));
        let bytes = std::fs::read(&path).map_err(|e| e.to_string());
        let _ = std::fs::remove_file(&path);
        let data = base64::engine::general_purpose::STANDARD.encode(bytes?);
        let app = front_app().map(|a| format!(" {a} is in front.")).unwrap_or_default();
        Ok(vec![json!({"type": "image", "data": data, "mimeType": "image/jpeg"}), json!({"type": "text", "text": format!("Screen {w}×{h}.{app}")})])
    }

    pub fn cursor() -> Result<(i64, i64), String> {
        let at = unsafe {
            let e = CGEventCreate(std::ptr::null_mut());
            if e.is_null() {
                return Err("macOS didn't say.".into());
            }
            let at = CGEventGetLocation(e);
            CFRelease(e);
            at
        };
        Ok(to_pixels(at.x, at.y, display()))
    }

    fn mouse(kind: u32, at: CGPoint, button: u32, clicks: i64, mods: u64) -> Result<(), String> {
        unsafe {
            let e = CGEventCreateMouseEvent(std::ptr::null_mut(), kind, at, button);
            if e.is_null() {
                return Err("macOS didn't make the event.".into());
            }
            if clicks > 0 {
                CGEventSetIntegerValueField(e, CLICK_STATE, clicks);
            }
            if mods != 0 {
                CGEventSetFlags(e, mods);
            }
            post(e)
        }
    }

    pub fn move_to(x: f64, y: f64) -> Result<(), String> {
        need_control()?;
        mouse(MOUSE_MOVED, point(x, y), 0, 0, 0)
    }

    pub fn click(x: f64, y: f64, button: &str, count: i64, mods: &[&str]) -> Result<(), String> {
        need_control()?;
        let at = point(x, y);
        let (down, up, b) = match button {
            "right" => (3, 4, 1),
            "middle" => (25, 26, 2),
            _ => (1, 2, 0),
        };
        let f = flags(mods);
        mouse(MOUSE_MOVED, at, 0, 0, 0)?;
        std::thread::sleep(Duration::from_millis(30));
        for n in 1..=count {
            mouse(down, at, b, n, f)?;
            mouse(up, at, b, n, f)?;
            std::thread::sleep(Duration::from_millis(40));
        }
        Ok(())
    }

    pub fn drag(from: (f64, f64), to: (f64, f64)) -> Result<(), String> {
        need_control()?;
        let (a, b) = (point(from.0, from.1), point(to.0, to.1));
        mouse(MOUSE_MOVED, a, 0, 0, 0)?;
        mouse(1, a, 0, 1, 0)?;
        for i in 1..=20 {
            let t = i as f64 / 20.0;
            mouse(LEFT_DRAGGED, CGPoint { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t }, 0, 0, 0)?;
            std::thread::sleep(Duration::from_millis(12));
        }
        mouse(2, b, 0, 1, 0)
    }

    pub fn scroll(x: f64, y: f64, dy: i32, dx: i32) -> Result<(), String> {
        need_control()?;
        mouse(MOUSE_MOVED, point(x, y), 0, 0, 0)?;
        post(unsafe { CGEventCreateScrollWheelEvent(std::ptr::null_mut(), SCROLL_LINES, 2, dy, dx) })
    }

    fn key_event(code: u16, down: bool, mods: u64, chars: Option<&[u16]>) -> Result<(), String> {
        unsafe {
            let e = CGEventCreateKeyboardEvent(std::ptr::null_mut(), code, down);
            if e.is_null() {
                return Err("macOS didn't make the event.".into());
            }
            if let Some(c) = chars {
                CGEventKeyboardSetUnicodeString(e, c.len(), c.as_ptr());
            }
            CGEventSetFlags(e, mods);
            post(e)
        }
    }

    pub fn key(mods: &[&str], key: &str) -> Result<(), String> {
        need_control()?;
        let code = super::keycode(key).ok_or("Unknown key.")?;
        let f = flags(mods);
        key_event(code, true, f, None)?;
        key_event(code, false, f, None)
    }

    pub fn type_text(text: &str) -> Result<(), String> {
        need_control()?;
        for (i, line) in text.split('\n').enumerate() {
            if i > 0 {
                key_event(36, true, 0, None)?;
                key_event(36, false, 0, None)?;
            }
            let units: Vec<u16> = line.encode_utf16().collect();
            for chunk in units.chunks(16) {
                key_event(0, true, 0, Some(chunk))?;
                key_event(0, false, 0, Some(chunk))?;
                std::thread::sleep(Duration::from_millis(8));
            }
        }
        Ok(())
    }

    pub fn open_app(name: &str) -> Result<(), String> {
        let out = run_with_timeout(Command::new("/usr/bin/open").args(["-a", name]), Duration::from_secs(20)).ok_or("It took too long to open.")?;
        if out.status != Some(0) {
            return Err(format!("This Mac has no app called {name}."));
        }
        std::thread::sleep(Duration::from_millis(1200));
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
mod mac {
    use serde_json::Value;

    use super::Access;

    const NO: &str = "Computer use needs a Mac.";

    pub fn access() -> Access {
        Access { supported: false, screen: false, control: false }
    }
    pub fn request_screen() {}
    pub fn request_control() {}
    pub fn front_app() -> Option<String> {
        None
    }
    pub fn screenshot() -> Result<Vec<Value>, String> {
        Err(NO.into())
    }
    pub fn cursor() -> Result<(i64, i64), String> {
        Err(NO.into())
    }
    pub fn move_to(_: f64, _: f64) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn click(_: f64, _: f64, _: &str, _: i64, _: &[&str]) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn drag(_: (f64, f64), _: (f64, f64)) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn scroll(_: f64, _: f64, _: i32, _: i32) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn key(_: &[&str], _: &str) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn type_text(_: &str) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn open_app(_: &str) -> Result<(), String> {
        Err(NO.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_pixels_map_to_points_and_back() {
        // A 14" MacBook Pro: 1512×982 points.
        let d = (1512.0, 982.0);
        assert_eq!(shot_size(d.0, d.1), (1280, 831));
        let (x, y) = to_points(640.0, 415.5, d);
        assert!((x - 756.0).abs() < 0.5 && (y - 491.0).abs() < 0.6);
        assert_eq!(to_pixels(756.0, 491.0, d), (640, 416));
        // A small screen is never scaled up.
        assert_eq!(shot_size(1024.0, 768.0), (1024, 768));
    }

    #[test]
    fn shortcuts() {
        assert_eq!(parse_keys("cmd+shift+s").unwrap(), (vec!["cmd", "shift"], "s".to_string()));
        assert_eq!(parse_keys("Return").unwrap(), (vec![], "return".to_string()));
        assert_eq!(parse_keys("ctrl + alt + delete").unwrap().0, vec!["control", "option"]);
        assert!(parse_keys("hyper+s").is_err());
        assert!(parse_keys("cmd+nope").is_err());
        assert!(parse_keys("").is_err());
    }

    #[test]
    fn what_each_tool_needs() {
        assert_eq!(needs("computer_screenshot", &json!({})), Needs::Screen);
        assert_eq!(needs("computer_open_app", &json!({"name": " Safari "})), Needs::App("Safari".into()));
        assert_eq!(needs("computer_click", &json!({"x": 1, "y": 2})), Needs::FrontApp);
        assert_eq!(needs("computer_wait", &json!({})), Needs::Nothing);
        let names: Vec<String> = tools().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
        assert!(names.iter().all(|n| n.starts_with(PREFIX)));
        assert_eq!(names.len(), 10);
    }

    #[test]
    fn waits_are_capped() {
        let t = std::time::Instant::now();
        perform("computer_wait", &json!({"seconds": 0.1})).unwrap();
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
