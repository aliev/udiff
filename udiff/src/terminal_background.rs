//! Asking the terminal what colour its background actually is.
//!
//! `COLORFGBG` is a guess left in the environment, sometimes by a different
//! terminal and usually before the last theme change. OSC 11 asks the terminal
//! in front of the reader, right now, and most of them answer.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::time::{Duration, Instant};

/// How long a terminal is given to answer, in the tenths of a second `VTIME`
/// counts. Long enough for one that will answer, short enough that one that
/// will not costs nobody a visible pause.
const DEADLINE_TENTHS: u8 = 1;
const DEADLINE: Duration = Duration::from_millis(100);

/// Asks the terminal for its background, or returns `None` if it will not say.
///
/// Runs before the event loop takes the terminal over, so the reply cannot be
/// mistaken for input. Raw mode is needed to read it at all — a cooked
/// terminal holds the bytes until a newline that never comes — and is put back
/// however this ends.
pub(crate) fn query() -> Option<(u8, u8, u8)> {
    if !worth_asking() {
        return None;
    }
    let mut tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .ok()?;
    let fd = tty.as_raw_fd();
    let restore = raw_mode(fd)?;
    tty.write_all(b"\x1b]11;?\x1b\\").ok()?;
    tty.flush().ok()?;
    let reply = read_reply(&mut tty);
    restore();
    parse_reply(&reply?)
}

/// Whether asking can work at all.
///
/// A multiplexer needs the query wrapped in its own passthrough, and without
/// it may swallow the question or echo it back; neither is worth the risk of
/// eating a keystroke, so those are left to the environment.
fn worth_asking() -> bool {
    let multiplexed = std::env::var("TERM")
        .map(|term| term.starts_with("screen") || term.starts_with("tmux"))
        .unwrap_or(false);
    !multiplexed && std::env::var_os("TMUX").is_none() && std::env::var_os("STY").is_none()
}

/// Reads until the reply is closed or the deadline passes.
fn read_reply(tty: &mut std::fs::File) -> Option<String> {
    let started = Instant::now();
    let mut reply = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        // `VTIME` is the timeout: a read returns nothing once the terminal has
        // been quiet for it, which is the answer "this one will not say".
        match tty.read(&mut byte) {
            Ok(1) => reply.push(byte[0]),
            _ => return None,
        }
        // A terminal dribbling one byte per tick must not hold the start up
        // for as long as it likes.
        if started.elapsed() > DEADLINE * 4 {
            return None;
        }
        // BEL, or the backslash that closes ST.
        if byte[0] == 0x07 || (byte[0] == b'\\' && reply.len() > 1) {
            return String::from_utf8(reply).ok();
        }
        // A terminal that answers something else entirely must not be read
        // until the deadline one byte at a time.
        if reply.len() > 64 {
            return None;
        }
    }
}

/// Puts the terminal in raw mode and hands back the way out of it.
fn raw_mode(fd: RawFd) -> Option<impl FnOnce()> {
    // SAFETY: termios is written by tcgetattr before it is read.
    let mut original = unsafe {
        let mut termios = std::mem::zeroed::<libc::termios>();
        (libc::tcgetattr(fd, &raw mut termios) == 0).then_some(termios)?
    };
    let restore = original;
    original.c_lflag &= !(libc::ICANON | libc::ECHO);
    original.c_cc[libc::VMIN] = 0;
    original.c_cc[libc::VTIME] = DEADLINE_TENTHS;
    // SAFETY: both structs are initialised and the fd is the terminal's own.
    if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw const original) } != 0 {
        return None;
    }
    Some(move || {
        // SAFETY: restoring exactly what tcgetattr reported.
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw const restore) };
    })
}

/// Parses an OSC 11 reply into a background colour.
///
/// The shape is `ESC ] 11 ; rgb:RRRR/GGGG/BBBB` closed by BEL or ST, with two
/// to four hex digits a channel — xterm answers with four, some others with
/// two, and the value is a fraction of the channel's width either way.
pub(crate) fn parse_reply(reply: &str) -> Option<(u8, u8, u8)> {
    let body = reply.split("rgb:").nth(1)?;
    let body = body
        .split(['\u{7}', '\u{1b}'])
        .next()
        .unwrap_or(body)
        .trim_end();
    let mut channels = body.split('/').map(channel);
    let (red, green, blue) = (channels.next()??, channels.next()??, channels.next()??);
    channels.next().is_none().then_some((red, green, blue))
}

/// One channel, scaled to a byte whatever width it was written in.
fn channel(text: &str) -> Option<u8> {
    let digits = text.trim();
    if digits.is_empty() || digits.len() > 4 {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    let full = 16u32.pow(u32::try_from(digits.len()).ok()?) - 1;
    u8::try_from(value * 255 / full).ok()
}

/// Whether a background is light enough to read dark text on.
///
/// The same relative luminance the contrast rules use, against the midpoint
/// of the range rather than a colour: a background is light or it is not.
pub(crate) fn is_light((red, green, blue): (u8, u8, u8)) -> bool {
    let linear = |channel: u8| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(red) + 0.7152 * linear(green) + 0.0722 * linear(blue) > 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_is_read_whatever_width_its_channels_are_written_in() {
        assert_eq!(
            parse_reply("\u{1b}]11;rgb:ffff/ffff/ffff\u{7}"),
            Some((255, 255, 255)),
            "four digits a channel, as xterm answers"
        );
        assert_eq!(
            parse_reply("\u{1b}]11;rgb:00/00/00\u{1b}\\"),
            Some((0, 0, 0)),
            "two digits, closed by ST rather than BEL"
        );
        assert_eq!(
            parse_reply("\u{1b}]11;rgb:1e1e/1e1e/2e2e\u{7}"),
            Some((30, 30, 46))
        );
    }

    #[test]
    fn anything_that_is_not_a_reply_is_no_answer_at_all() {
        // A terminal that says nothing, echoes the question, or answers in a
        // shape we do not know must leave the next source to decide.
        for reply in [
            "",
            "\u{1b}]11;?\u{7}",
            "\u{1b}]11;rgb:ffff/ffff\u{7}",
            "\u{1b}]11;rgb:ffff/ffff/ffff/ffff\u{7}",
            "\u{1b}]11;rgb:zz/00/00\u{7}",
            "\u{1b}]11;rgb:fffff/0/0\u{7}",
            "hello",
        ] {
            assert_eq!(parse_reply(reply), None, "{reply:?} is not an answer");
        }
    }

    #[test]
    fn a_background_is_light_or_it_is_not() {
        assert!(is_light((255, 255, 255)));
        assert!(is_light((250, 245, 230)), "a warm paper background");
        assert!(!is_light((0, 0, 0)));
        assert!(!is_light((30, 30, 46)), "a dark theme's near-black");
        assert!(!is_light((60, 60, 60)), "and a middling grey reads as dark");
    }
}
