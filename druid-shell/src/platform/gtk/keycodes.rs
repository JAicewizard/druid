// Copyright 2019 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! GTK code handling.

use gdk::keys::constants::*;
use gdk::keys::Key as GDKKey;

use crate::keyboard_types::{Code,Key, Location};

pub type RawKey = gdk::keys::Key;

#[allow(clippy::just_underscores_and_digits, non_upper_case_globals)]
pub fn raw_key_to_key(raw: &RawKey) -> Option<Key> {
    Some(match raw.clone() {
        Escape => Key::Escape,
        BackSpace => Key::Backspace,
        Tab | ISO_Left_Tab => Key::Tab,
        Return => Key::Enter,
        Control_L | Control_R => Key::Control,
        Alt_L | Alt_R => Key::Alt,
        Shift_L | Shift_R => Key::Shift,
        // TODO: investigate mapping. Map Meta_[LR]?
        Super_L | Super_R => Key::Meta,
        Caps_Lock => Key::CapsLock,
        F1 => Key::F1,
        F2 => Key::F2,
        F3 => Key::F3,
        F4 => Key::F4,
        F5 => Key::F5,
        F6 => Key::F6,
        F7 => Key::F7,
        F8 => Key::F8,
        F9 => Key::F9,
        F10 => Key::F10,
        F11 => Key::F11,
        F12 => Key::F12,

        Print => Key::PrintScreen,
        Scroll_Lock => Key::ScrollLock,
        // Pause/Break not audio.
        Pause => Key::Pause,

        Insert => Key::Insert,
        Delete => Key::Delete,
        Home => Key::Home,
        End => Key::End,
        Page_Up => Key::PageUp,
        Page_Down => Key::PageDown,
        Num_Lock => Key::NumLock,

        Up => Key::ArrowUp,
        Down => Key::ArrowDown,
        Left => Key::ArrowLeft,
        Right => Key::ArrowRight,
        Clear => Key::Clear,

        Menu => Key::ContextMenu,
        WakeUp => Key::WakeUp,
        Launch0 => Key::LaunchApplication1,
        Launch1 => Key::LaunchApplication2,
        ISO_Level3_Shift => Key::AltGraph,

        KP_Begin => Key::Clear,
        KP_Delete => Key::Delete,
        KP_Down => Key::ArrowDown,
        KP_End => Key::End,
        KP_Enter => Key::Enter,
        KP_F1 => Key::F1,
        KP_F2 => Key::F2,
        KP_F3 => Key::F3,
        KP_F4 => Key::F4,
        KP_Home => Key::Home,
        KP_Insert => Key::Insert,
        KP_Left => Key::ArrowLeft,
        KP_Page_Down => Key::PageDown,
        KP_Page_Up => Key::PageUp,
        KP_Right => Key::ArrowRight,
        // KP_Separator? What does it map to?
        KP_Tab => Key::Tab,
        KP_Up => Key::ArrowUp,
        // TODO: more mappings (media etc)
        _ => return None,
    })
}

#[allow(clippy::just_underscores_and_digits, non_upper_case_globals)]
pub fn raw_key_to_location(raw: &RawKey) -> Location {
    match raw.clone() {
        Control_L | Shift_L | Alt_L | Super_L | Meta_L => Location::Left,
        Control_R | Shift_R | Alt_R | Super_R | Meta_R => Location::Right,
        KP_0 | KP_1 | KP_2 | KP_3 | KP_4 | KP_5 | KP_6 | KP_7 | KP_8 | KP_9 | KP_Add | KP_Begin
        | KP_Decimal | KP_Delete | KP_Divide | KP_Down | KP_End | KP_Enter | KP_Equal | KP_F1
        | KP_F2 | KP_F3 | KP_F4 | KP_Home | KP_Insert | KP_Left | KP_Multiply | KP_Page_Down
        | KP_Page_Up | KP_Right | KP_Separator | KP_Space | KP_Subtract | KP_Tab | KP_Up => {
            Location::Numpad
        }
        _ => Location::Standard,
    }
}

#[allow(non_upper_case_globals)]
pub fn key_to_raw_key(src: &Key) -> Option<RawKey> {
    Some(match src {
        Key::Escape => Escape,
        Key::Backspace => BackSpace,

        Key::Tab => Tab,
        Key::Enter => Return,

        // Give "left" variants
        Key::Control => Control_L,
        Key::Alt => Alt_L,
        Key::Shift => Shift_L,
        Key::Meta => Super_L,

        Key::CapsLock => Caps_Lock,
        Key::F1 => F1,
        Key::F2 => F2,
        Key::F3 => F3,
        Key::F4 => F4,
        Key::F5 => F5,
        Key::F6 => F6,
        Key::F7 => F7,
        Key::F8 => F8,
        Key::F9 => F9,
        Key::F10 => F10,
        Key::F11 => F11,
        Key::F12 => F12,

        Key::PrintScreen => Print,
        Key::ScrollLock => Scroll_Lock,
        // Pause/Break not audio.
        Key::Pause => Pause,

        Key::Insert => Insert,
        Key::Delete => Delete,
        Key::Home => Home,
        Key::End => End,
        Key::PageUp => Page_Up,
        Key::PageDown => Page_Down,

        Key::NumLock => Num_Lock,

        Key::ArrowUp => Up,
        Key::ArrowDown => Down,
        Key::ArrowLeft => Left,
        Key::ArrowRight => Right,

        Key::ContextMenu => Menu,
        Key::WakeUp => WakeUp,
        Key::LaunchApplication1 => Launch0,
        Key::LaunchApplication2 => Launch1,
        Key::AltGraph => ISO_Level3_Shift,
        // TODO: probably more
        _ => return None,
    })
}



/// Map hardware keycode to code.
///
/// In theory, the hardware keycode is device dependent, but in
/// practice it's probably pretty reliable.
///
/// The logic is based on NativeKeyToDOMCodeName.h in Mozilla.
pub fn hardware_keycode_to_code(hw_keycode: &GDKKey) -> Code {
    use gdk::keys::constants::*;
    match hw_keycode.clone() {
        Escape => Code::Escape,
        _1 => Code::Digit1,
        _2 => Code::Digit2,
        _3 => Code::Digit3,
        _4 => Code::Digit4,
        _5 => Code::Digit5,
        _6 => Code::Digit6,
        _7 => Code::Digit7,
        _8 => Code::Digit8,
        _9 => Code::Digit9,
        _0 => Code::Digit0,
        minus => Code::Minus,
        equal => Code::Equal,
        BackSpace => Code::Backspace,
        Tab => Code::Tab,
        q|Q => Code::KeyQ,
        w|W => Code::KeyW,
        e|E => Code::KeyE,
        r|R => Code::KeyR,
        t|T => Code::KeyT,
        y|Y => Code::KeyY,
        u|U => Code::KeyU,
        i|I => Code::KeyI,
        o|O => Code::KeyO,
        p|P => Code::KeyP,
        bracketleft => Code::BracketLeft,
        bracketright => Code::BracketRight,
        _3270_Enter => Code::Enter,
        Control_L => Code::ControlLeft,
        a|A => Code::KeyA,
        s|S => Code::KeyS,
        d|D => Code::KeyD,
        f|F => Code::KeyF,
        g|G => Code::KeyG,
        h|H => Code::KeyH,
        j|J => Code::KeyJ,
        k|K => Code::KeyK,
        l|L => Code::KeyL,
        semicolon => Code::Semicolon,
        // 0x0030 => Code::Quote,
        // 0x0031 => Code::Backquote,
        Shift_L => Code::ShiftLeft,
        backslash => Code::Backslash,
        z|Z => Code::KeyZ,
        x|X => Code::KeyX,
        c|C => Code::KeyC,
        v|V => Code::KeyV,
        b|B => Code::KeyB,
        n|N => Code::KeyN,
        m|M => Code::KeyM,
        comma => Code::Comma,
        period => Code::Period,
        slash => Code::Slash,
        Shift_R => Code::ShiftRight,
        KP_Multiply => Code::NumpadMultiply,
        Alt_L => Code::AltLeft,
        space => Code::Space,
        Caps_Lock => Code::CapsLock,
        F1 => Code::F1,
        F2 => Code::F2,
        F3 => Code::F3,
        F4 => Code::F4,
        F5 => Code::F5,
        F6 => Code::F6,
        F7 => Code::F7,
        F8 => Code::F8,
        F9 => Code::F9,
        F10 => Code::F10,
        Num_Lock => Code::NumLock,
        Scroll_Lock => Code::ScrollLock,
        KP_7 => Code::Numpad7,
        KP_8 => Code::Numpad8,
        KP_9 => Code::Numpad9,
        KP_Subtract => Code::NumpadSubtract,
        KP_4 => Code::Numpad4,
        KP_5 => Code::Numpad5,
        KP_6 => Code::Numpad6,
        KP_Add => Code::NumpadAdd,
        KP_1 => Code::Numpad1,
        KP_2 => Code::Numpad2,
        KP_3 => Code::Numpad3,
        KP_0 => Code::Numpad0,
        KP_Decimal => Code::NumpadDecimal,
        // 0x005E => Code::IntlBackslash,
        F11 => Code::F11,
        F12 => Code::F12,
        // 0x0061 => Code::IntlRo,
        // 0x0064 => Code::Convert,
        // 0x0065 => Code::KanaMode,
        // 0x0066 => Code::NonConvert,
        KP_Enter => Code::NumpadEnter,
        Control_R => Code::ControlRight,
        KP_Divide => Code::NumpadDivide,
        _3270_PrintScreen => Code::PrintScreen,
        Alt_R => Code::AltRight,
        Home => Code::Home,
        uparrow => Code::ArrowUp,
        Page_Up => Code::PageUp,
        leftarrow => Code::ArrowLeft,
        rightarrow => Code::ArrowRight,
        End => Code::End,
        downarrow => Code::ArrowDown,
        Page_Down => Code::PageDown,
        Insert => Code::Insert,
        Delete => Code::Delete,
        AudioMute => Code::AudioVolumeMute,
        AudioLowerVolume => Code::AudioVolumeDown,
        AudioRaiseVolume => Code::AudioVolumeUp,
        KP_Equal => Code::NumpadEqual,
        Pause => Code::Pause,
        // 0x0081 => Code::NumpadComma,
        // 0x0082 => Code::Lang1,
        // 0x0083 => Code::Lang2,
        yen => Code::IntlYen,
        Meta_L => Code::MetaLeft,
        Meta_R => Code::MetaRight,
        // 0x0087 => Code::ContextMenu,
        // 0x0088 => Code::BrowserStop,
        Redo => Code::Again,
        // 0x008A => Code::Props,
        Undo => Code::Undo,
        Select => Code::Select,
        Copy => Code::Copy,
        Open => Code::Open,
        Paste => Code::Paste,
        Find => Code::Find,
        Cut => Code::Cut,
        Help => Code::Help,
        // 0x0094 => Code::LaunchApp2,
        WakeUp => Code::WakeUp,
        // 0x0098 => Code::LaunchApp1,
        // key to right of volume controls on T430s produces 0x9C
        // but no documentation of what it should map to :/
        // 0x00A3 => Code::LaunchMail,
        // 0x00A4 => Code::BrowserFavorites,
        // 0x00A6 => Code::BrowserBack,
        // 0x00A7 => Code::BrowserForward,
        Eject => Code::Eject,
        AudioNext => Code::MediaTrackNext,
        AudioPause => Code::MediaPlayPause,
        AudioPrev => Code::MediaTrackPrevious,
        AudioStop => Code::MediaStop,
        // 0x00B3 => Code::MediaSelect,
        // 0x00B4 => Code::BrowserHome,
        // 0x00B5 => Code::BrowserRefresh,
        // 0x00E1 => Code::BrowserSearch,
        _ => Code::Unidentified,
    }
}
