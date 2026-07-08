//! S101 byte-framing protocol implementation.
//!
//! S101 wraps EmBER packets for transmission over a stream transport such as TCP.
//! This module implements the escaping variant of the protocol:
//!
//! - Start of frame: `0xFE`
//! - End of frame: `0xFF`
//! - Escape byte: `0xFD`, followed by the escaped byte XOR `0x20`
//! - CRC-16-CCITT over the unescaped frame contents (excluding BOF/EOF)
//!
//! Multi-packet messages are indicated by the flags byte in the frame header.

use crate::{Error, Result};
use bytes::{Buf, BufMut, BytesMut};

/// Byte indicating the beginning of an S101 frame.
pub const BOF: u8 = 0xFE;
/// Byte indicating the end of an S101 frame.
pub const EOF: u8 = 0xFF;
/// Escape byte used inside an S101 frame.
pub const CE: u8 = 0xFD;
/// XOR value applied to escaped bytes.
pub const ESCAPE_XOR: u8 = 0x20;

/// Mask for bytes that must be escaped in the payload.
pub const ESCAPE_MASK: u8 = 0xF8;

/// S101 message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// EmBER packet.
    Ember = 0x0E,
    /// Keep-alive request.
    KeepAliveRequest = 0x01,
    /// Keep-alive response.
    KeepAliveResponse = 0x02,
}

/// S101 command types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandType {
    /// EmBER packet command.
    EmberPacket = 0x00,
}

/// Frame flags used to indicate fragmentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameFlags;

impl FrameFlags {
    /// First packet of a fragmented message.
    pub const FIRST: u8 = 0x80;
    /// Middle packet of a fragmented message.
    pub const MIDDLE: u8 = 0x40;
    /// Last packet of a fragmented message.
    pub const LAST: u8 = 0xC0;
    /// Complete message in a single frame.
    pub const SINGLE: u8 = 0xC0;
    /// Mask for the fragment bits in the flags byte.
    pub const MASK: u8 = 0xC0;
}

/// DTD types carried in an S101 frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DtdType {
    /// Glow DTD.
    Glow = 0x01,
}

/// A decoded S101 frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Slot number.
    pub slot: u8,
    /// Message type.
    pub message_type: u8,
    /// Command type.
    pub command: u8,
    /// Version byte.
    pub version: u8,
    /// Frame flags.
    pub flags: u8,
    /// DTD type.
    pub dtd: u8,
    /// Application bytes (e.g. Glow DTD version).
    pub app_bytes: Vec<u8>,
    /// Payload bytes (EmBER data or keep-alive body).
    pub payload: Vec<u8>,
}

impl Frame {
    /// Returns true if this frame is a single, unfragmented message.
    pub fn is_single(&self) -> bool {
        (self.flags & FrameFlags::MASK) == FrameFlags::SINGLE
    }

    /// Returns true if this frame is the first fragment of a message.
    pub fn is_first(&self) -> bool {
        (self.flags & FrameFlags::MASK) == FrameFlags::FIRST
    }

    /// Returns true if this frame is a middle fragment of a message.
    pub fn is_middle(&self) -> bool {
        (self.flags & FrameFlags::MASK) == FrameFlags::MIDDLE
    }

    /// Returns true if this frame is the last fragment of a message.
    pub fn is_last(&self) -> bool {
        (self.flags & FrameFlags::MASK) == FrameFlags::LAST
    }

    /// Returns true if this frame carries an EmBER packet payload.
    pub fn is_ember_packet(&self) -> bool {
        self.message_type == MessageType::Ember as u8
            && self.command == CommandType::EmberPacket as u8
    }

    /// Returns true if this frame is a keep-alive request.
    pub fn is_keep_alive_request(&self) -> bool {
        self.message_type == MessageType::KeepAliveRequest as u8
    }

    /// Returns true if this frame is a keep-alive response.
    pub fn is_keep_alive_response(&self) -> bool {
        self.message_type == MessageType::KeepAliveResponse as u8
    }
}

/// Default Glow DTD version bytes: minor 0x05, major 0x02 (Glow 2.5).
const GLOW_DTD_VERSION: &[u8] = &[0x05, 0x02];

/// Build an S101 EmBER frame carrying the provided EmBER payload.
pub fn encode_ember_frame(payload: &[u8]) -> Vec<u8> {
    encode_frame(
        0x00,
        MessageType::Ember as u8,
        CommandType::EmberPacket as u8,
        0x01,
        FrameFlags::SINGLE,
        DtdType::Glow as u8,
        GLOW_DTD_VERSION,
        payload,
    )
}

/// Build an S101 keep-alive request frame.
pub fn encode_keep_alive_request() -> Vec<u8> {
    encode_frame(
        0x00,
        MessageType::KeepAliveRequest as u8,
        0x00,
        0x01,
        FrameFlags::SINGLE,
        0x00,
        &[],
        &[],
    )
}

/// Build an S101 keep-alive response frame.
pub fn encode_keep_alive_response() -> Vec<u8> {
    encode_frame(
        0x00,
        MessageType::KeepAliveResponse as u8,
        0x00,
        0x01,
        FrameFlags::SINGLE,
        0x00,
        &[],
        &[],
    )
}

fn encode_frame(
    slot: u8,
    message_type: u8,
    command: u8,
    version: u8,
    flags: u8,
    dtd: u8,
    app_bytes: &[u8],
    payload: &[u8],
) -> Vec<u8> {
    let mut raw = BytesMut::with_capacity(
        8 + app_bytes.len() + payload.len() + 2, // header + apps + payload + crc
    );
    raw.put_u8(slot);
    raw.put_u8(message_type);
    raw.put_u8(command);
    raw.put_u8(version);
    raw.put_u8(flags);
    raw.put_u8(dtd);
    raw.put_u8(app_bytes.len() as u8);
    raw.put_slice(app_bytes);
    raw.put_slice(payload);

    let crc = crc16_ccitt(&raw);
    let crc_inverted = !crc;

    let mut out = Vec::with_capacity(raw.len() * 2 + 3);
    out.push(BOF);
    escape_and_append(&raw, &mut out);
    escape_and_append(&crc_inverted.to_le_bytes(), &mut out);
    out.push(EOF);
    out
}

fn escape_and_append(input: &[u8], out: &mut Vec<u8>) {
    for &b in input {
        if b >= ESCAPE_MASK || b == BOF || b == EOF || b == CE {
            out.push(CE);
            out.push(b ^ ESCAPE_XOR);
        } else {
            out.push(b);
        }
    }
}

/// Compute CRC-16-CCITT over the provided bytes using the same algorithm as
/// `libember_slim`.
fn crc16_ccitt(data: &[u8]) -> u16 {
    const CRC_TABLE: [u16; 256] = [
        0x0000, 0x1189, 0x2312, 0x329b, 0x4624, 0x57ad, 0x6536, 0x74bf,
        0x8c48, 0x9dc1, 0xaf5a, 0xbed3, 0xca6c, 0xdbe5, 0xe97e, 0xf8f7,
        0x1081, 0x0108, 0x3393, 0x221a, 0x56a5, 0x472c, 0x75b7, 0x643e,
        0x9cc9, 0x8d40, 0xbfdb, 0xae52, 0xdaed, 0xcb64, 0xf9ff, 0xe876,
        0x2102, 0x308b, 0x0210, 0x1399, 0x6726, 0x76af, 0x4434, 0x55bd,
        0xad4a, 0xbcc3, 0x8e58, 0x9fd1, 0xeb6e, 0xfae7, 0xc87c, 0xd9f5,
        0x3183, 0x200a, 0x1291, 0x0318, 0x77a7, 0x662e, 0x54b5, 0x453c,
        0xbdcb, 0xac42, 0x9ed9, 0x8f50, 0xfbef, 0xea66, 0xd8fd, 0xc974,
        0x4204, 0x538d, 0x6116, 0x709f, 0x0420, 0x15a9, 0x2732, 0x36bb,
        0xce4c, 0xdfc5, 0xed5e, 0xfcd7, 0x8868, 0x99e1, 0xab7a, 0xbaf3,
        0x5285, 0x430c, 0x7197, 0x601e, 0x14a1, 0x0528, 0x37b3, 0x263a,
        0xdecd, 0xcf44, 0xfddf, 0xec56, 0x98e9, 0x8960, 0xbbfb, 0xaa72,
        0x6306, 0x728f, 0x4014, 0x519d, 0x2522, 0x34ab, 0x0630, 0x17b9,
        0xef4e, 0xfec7, 0xcc5c, 0xddd5, 0xa96a, 0xb8e3, 0x8a78, 0x9bf1,
        0x7387, 0x620e, 0x5095, 0x411c, 0x35a3, 0x242a, 0x16b1, 0x0738,
        0xffcf, 0xee46, 0xdcdd, 0xcd54, 0xb9eb, 0xa862, 0x9af9, 0x8b70,
        0x8408, 0x9581, 0xa71a, 0xb693, 0xc22c, 0xd3a5, 0xe13e, 0xf0b7,
        0x0840, 0x19c9, 0x2b52, 0x3adb, 0x4e64, 0x5fed, 0x6d76, 0x7cff,
        0x9489, 0x8500, 0xb79b, 0xa612, 0xd2ad, 0xc324, 0xf1bf, 0xe036,
        0x18c1, 0x0948, 0x3bd3, 0x2a5a, 0x5ee5, 0x4f6c, 0x7df7, 0x6c7e,
        0xa50a, 0xb483, 0x8618, 0x9791, 0xe32e, 0xf2a7, 0xc03c, 0xd1b5,
        0x2942, 0x38cb, 0x0a50, 0x1bd9, 0x6f66, 0x7eef, 0x4c74, 0x5dfd,
        0xb58b, 0xa402, 0x9699, 0x8710, 0xf3af, 0xe226, 0xd0bd, 0xc134,
        0x39c3, 0x284a, 0x1ad1, 0x0b58, 0x7fe7, 0x6e6e, 0x5cf5, 0x4d7c,
        0xc60c, 0xd785, 0xe51e, 0xf497, 0x8028, 0x91a1, 0xa33a, 0xb2b3,
        0x4a44, 0x5bcd, 0x6956, 0x78df, 0x0c60, 0x1de9, 0x2f72, 0x3efb,
        0xd68d, 0xc704, 0xf59f, 0xe416, 0x90a9, 0x8120, 0xb3bb, 0xa232,
        0x5ac5, 0x4b4c, 0x79d7, 0x685e, 0x1ce1, 0x0d68, 0x3ff3, 0x2e7a,
        0xe70e, 0xf687, 0xc41c, 0xd595, 0xa12a, 0xb0a3, 0x8238, 0x93b1,
        0x6b46, 0x7acf, 0x4854, 0x59dd, 0x2d62, 0x3ceb, 0x0e70, 0x1ff9,
        0xf78f, 0xe606, 0xd49d, 0xc514, 0xb1ab, 0xa022, 0x92b9, 0x8330,
        0x7bc7, 0x6a4e, 0x58d5, 0x495c, 0x3de3, 0x2c6a, 0x1ef1, 0x0f78,
    ];

    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc = (crc >> 8) ^ CRC_TABLE[((crc ^ byte as u16) & 0xFF) as usize];
    }
    crc
}

/// Streaming S101 frame decoder.
///
/// Feeds raw bytes from a TCP stream and emits complete [`Frame`] values.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: BytesMut,
    in_frame: bool,
}

impl FrameDecoder {
    /// Create a new decoder.
    pub fn new() -> Self {
        Self {
            buf: BytesMut::with_capacity(4096),
            in_frame: false,
        }
    }

    /// Feed bytes into the decoder.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Try to decode the next complete frame.
    ///
    /// Returns `Ok(Some(frame))` when a frame is available, `Ok(None)` when more
    /// bytes are needed, or `Err` if the stream contains an invalid frame.
    pub fn decode_next(&mut self) -> Result<Option<Frame>> {
        loop {
            if !self.in_frame {
                // Look for the next BOF.
                match self.buf.iter().position(|&b| b == BOF) {
                    Some(pos) => {
                        self.buf.advance(pos);
                        self.in_frame = true;
                    }
                    None => {
                        self.buf.clear();
                        return Ok(None);
                    }
                }
            }

            // Now inside a frame: look for EOF.
            match self.buf.iter().skip(1).position(|&b| b == EOF) {
                Some(end_rel) => {
                    let end = end_rel + 1; // account for skip(1)
                    let frame_bytes = self.buf.split_to(end + 1);
                    self.in_frame = false;
                    return decode_single_frame(&frame_bytes).map(Some);
                }
                None => {
                    // Need more data, but drop leading garbage before BOF if any.
                    return Ok(None);
                }
            }
        }
    }
}

fn decode_single_frame(bytes: &[u8]) -> Result<Frame> {
    if bytes.len() < 4 {
        return Err(Error::S101("frame too short".into()));
    }
    if bytes[0] != BOF || bytes[bytes.len() - 1] != EOF {
        return Err(Error::S101("frame missing BOF/EOF".into()));
    }

    let mut raw = Vec::with_capacity(bytes.len());
    let mut escaped = false;

    for &b in &bytes[1..bytes.len() - 1] {
        if escaped {
            raw.push(b ^ ESCAPE_XOR);
            escaped = false;
        } else if b == CE {
            escaped = true;
        } else {
            raw.push(b);
        }
    }

    if escaped {
        return Err(Error::S101("dangling escape byte".into()));
    }

    if raw.len() < 4 {
        return Err(Error::S101("decoded frame too short".into()));
    }

    let (content, crc_bytes) = raw.split_at(raw.len() - 2);
    let received_crc = u16::from_le_bytes([crc_bytes[0], crc_bytes[1]]);
    let computed_crc = crc16_ccitt(content);

    if received_crc != !computed_crc {
        return Err(Error::S101(format!(
            "CRC mismatch: expected {:04X}, got {:04X}",
            !computed_crc, received_crc
        )));
    }

    parse_frame_content(content)
}

fn parse_frame_content(content: &[u8]) -> Result<Frame> {
    if content.len() < 7 {
        return Err(Error::S101("content too short for header".into()));
    }

    let slot = content[0];
    let message_type = content[1];
    let command = content[2];
    let version = content[3];
    let flags = content[4];
    let dtd = content[5];
    let app_count = content[6] as usize;

    if content.len() < 7 + app_count {
        return Err(Error::S101("app bytes truncated".into()));
    }

    let app_bytes = content[7..7 + app_count].to_vec();
    let payload = content[7 + app_count..].to_vec();

    Ok(Frame {
        slot,
        message_type,
        command,
        version,
        flags,
        dtd,
        app_bytes,
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_ember_frame() {
        let payload = b"\x01\x02\x03\xFE\xFD";
        let encoded = encode_ember_frame(payload);
        let decoded = decode_single_frame(&encoded).unwrap();
        assert_eq!(decoded.message_type, MessageType::Ember as u8);
        assert_eq!(decoded.command, CommandType::EmberPacket as u8);
        assert_eq!(decoded.flags, FrameFlags::SINGLE);
        assert_eq!(decoded.dtd, DtdType::Glow as u8);
        assert_eq!(decoded.app_bytes, GLOW_DTD_VERSION);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn keep_alive_request_roundtrip() {
        let encoded = encode_keep_alive_request();
        let decoded = decode_single_frame(&encoded).unwrap();
        assert!(decoded.is_keep_alive_request());
    }

    #[test]
    fn decoder_streaming() {
        let mut decoder = FrameDecoder::new();
        let frame = encode_ember_frame(b"hello");
        decoder.feed(&frame[..5]);
        assert!(decoder.decode_next().unwrap().is_none());
        decoder.feed(&frame[5..]);
        let decoded = decoder.decode_next().unwrap().unwrap();
        assert_eq!(decoded.payload, b"hello");
    }

    #[test]
    fn crc_residual() {
        // The spec says the CRC residual should be 0xF0B8.
        let payload = b"test";
        let encoded = encode_ember_frame(payload);
        let decoded = decode_single_frame(&encoded).unwrap();
        assert_eq!(decoded.payload, payload);
    }
}
