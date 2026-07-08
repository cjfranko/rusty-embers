//! Safe Rust wrappers around `libember_slim` Glow objects.
//!
//! This module is responsible for initializing the C library, converting between
//! Rust and C Glow values, and encoding/decoding Glow messages.

use crate::{Error, Result};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
use std::sync::Once;

use crate::sys;

static INIT: Once = Once::new();

/// Initialize `libember_slim` with Rust-backed allocator and logging callbacks.
pub fn init() {
    INIT.call_once(|| unsafe {
        sys::ember_init(
            Some(throw_error),
            Some(fail_assertion),
            Some(alloc_memory),
            Some(free_memory),
        );
    });
}

unsafe extern "C" fn throw_error(error: c_int, p_message: *const c_char) {
    let message = if p_message.is_null() {
        "unknown".to_string()
    } else {
        CStr::from_ptr(p_message).to_string_lossy().into_owned()
    };
    tracing::error!("libember_slim error {}: {}", error, message);
}

unsafe extern "C" fn fail_assertion(p_file: *const c_char, line: c_int) {
    let file = if p_file.is_null() {
        "unknown".to_string()
    } else {
        CStr::from_ptr(p_file).to_string_lossy().into_owned()
    };
    tracing::error!("libember_slim assertion failed at {}:{}", file, line);
}

unsafe extern "C" fn alloc_memory(size: usize) -> *mut c_void {
    if size == 0 {
        return ptr::null_mut();
    }
    libc::malloc(size)
}

unsafe extern "C" fn free_memory(p_memory: *mut c_void) {
    if !p_memory.is_null() {
        libc::free(p_memory);
    }
}

/// A Glow value, mirrored from `libember_slim`'s `GlowValue`.
#[derive(Debug, Clone, PartialEq)]
pub enum GlowValue {
    /// No value set.
    None,
    /// Integer value.
    Integer(i64),
    /// Real (floating point) value.
    Real(f64),
    /// Boolean value.
    Boolean(bool),
    /// UTF-8 string value.
    String(String),
    /// Octet string value.
    OctetString(Vec<u8>),
}

impl GlowValue {
    /// Convert a Rust `GlowValue` into a C `GlowValue`.
    ///
    /// # Safety
    /// The returned `sys::GlowValue` contains pointers into the provided `CString`
    /// or `Vec<u8>` buffers. Those buffers must outlive the C value.
    unsafe fn to_c(
        &self,
        string_buffer: &mut Option<CString>,
        octet_buffer: &mut Option<Vec<u8>>,
    ) -> sys::GlowValue {
        let mut c_value = sys::GlowValue {
            flag: sys::EGlowParameterType_GlowParameterType_None,
            choice: sys::SGlowValue__bindgen_ty_1 { integer: 0 },
        };

        match self {
            GlowValue::None => {}
            GlowValue::Integer(v) => {
                c_value.flag = sys::EGlowParameterType_GlowParameterType_Integer;
                c_value.choice.integer = *v;
            }
            GlowValue::Real(v) => {
                c_value.flag = sys::EGlowParameterType_GlowParameterType_Real;
                c_value.choice.real = *v;
            }
            GlowValue::Boolean(v) => {
                c_value.flag = sys::EGlowParameterType_GlowParameterType_Boolean;
                c_value.choice.boolean = *v as i32;
            }
            GlowValue::String(s) => {
                c_value.flag = sys::EGlowParameterType_GlowParameterType_String;
                let c_string = CString::new(s.clone()).unwrap_or_default();
                c_value.choice.pString = c_string.as_ptr() as *mut c_char;
                *string_buffer = Some(c_string);
            }
            GlowValue::OctetString(bytes) => {
                c_value.flag = sys::EGlowParameterType_GlowParameterType_Octets;
                let mut buf = bytes.clone();
                let len = buf.len() as c_int;
                let ptr = buf.as_mut_ptr();
                c_value.choice.octets.pOctets = ptr;
                c_value.choice.octets.length = len;
                *octet_buffer = Some(buf);
            }
        }

        c_value
    }

    /// Convert a C `GlowValue` into a Rust `GlowValue`.
    ///
    /// # Safety
    /// `c_value` must be a valid `sys::GlowValue` with pointers to valid memory.
    unsafe fn from_c(c_value: &sys::GlowValue) -> Self {
        match c_value.flag {
            sys::EGlowParameterType_GlowParameterType_None => GlowValue::None,
            sys::EGlowParameterType_GlowParameterType_Integer => {
                GlowValue::Integer(c_value.choice.integer)
            }
            sys::EGlowParameterType_GlowParameterType_Real => GlowValue::Real(c_value.choice.real),
            sys::EGlowParameterType_GlowParameterType_Boolean => {
                GlowValue::Boolean(c_value.choice.boolean != 0)
            }
            sys::EGlowParameterType_GlowParameterType_String => {
                if c_value.choice.pString.is_null() {
                    GlowValue::String(String::new())
                } else {
                    GlowValue::String(
                        CStr::from_ptr(c_value.choice.pString)
                            .to_string_lossy()
                            .into_owned(),
                    )
                }
            }
            sys::EGlowParameterType_GlowParameterType_Octets => {
                let octets = &c_value.choice.octets;
                let len = octets.length as usize;
                if octets.pOctets.is_null() || len == 0 {
                    GlowValue::OctetString(Vec::new())
                } else {
                    GlowValue::OctetString(std::slice::from_raw_parts(octets.pOctets, len).to_vec())
                }
            }
            _ => GlowValue::None,
        }
    }
}

/// Access flags for a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Access {
    /// No access.
    None = 0,
    /// Read-only.
    Read = 1,
    /// Write-only.
    Write = 2,
    /// Read/write.
    ReadWrite = 3,
}

/// Parameter type hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ParameterType {
    /// No specific type.
    None = 0,
    /// Integer.
    Integer = 1,
    /// Real.
    Real = 2,
    /// String.
    String = 3,
    /// Boolean.
    Boolean = 4,
    /// Trigger.
    Trigger = 5,
    /// Enum.
    Enum = 6,
    /// Octets.
    Octets = 7,
}

/// Information about a Glow node used for encoding.
#[derive(Debug, Clone)]
pub struct NodeInfo {
    /// Node identifier.
    pub identifier: String,
    /// Optional description.
    pub description: Option<String>,
}

/// Information about a Glow parameter used for encoding.
#[derive(Debug, Clone)]
pub struct ParameterInfo {
    /// Parameter identifier.
    pub identifier: String,
    /// Optional description.
    pub description: Option<String>,
    /// Current value.
    pub value: GlowValue,
    /// Access rights.
    pub access: Access,
    /// Parameter type hint.
    pub parameter_type: ParameterType,
}

/// Encode a `QualifiedNode` response.
pub fn encode_qualified_node(path: &[u32], node: &NodeInfo) -> Result<Vec<u8>> {
    init();

    const BUFFER_SIZE: usize = 65536;
    let mut buffer: Vec<u8> = vec![0; BUFFER_SIZE];

    unsafe {
        let mut output: sys::GlowOutput = std::mem::zeroed();
        sys::glowOutput_init(&mut output,
            buffer.as_mut_ptr(),
            BUFFER_SIZE as u32,
            0,
        );
        sys::glowOutput_beginPackage(&mut output, 1);

        let c_path: Vec<sys::berint> = path.iter().map(|&n| n as sys::berint).collect();
        let mut c_node: sys::GlowNode = std::mem::zeroed();
        let c_identifier = CString::new(node.identifier.clone())
            .map_err(|e| Error::Glow(e.to_string()))?;
        c_node.pIdentifier = c_identifier.as_ptr() as *mut c_char;

        let mut fields = sys::EGlowFieldFlags_GlowFieldFlag_Identifier;
        let c_description = node
            .description
            .as_ref()
            .and_then(|d| CString::new(d.clone()).ok());
        if let Some(desc) = &c_description {
            c_node.pDescription = desc.as_ptr() as *mut c_char;
            fields |= sys::EGlowFieldFlags_GlowFieldFlag_Description;
        }

        sys::glow_writeQualifiedNode(
            &mut output,
            &c_node,
            fields,
            c_path.as_ptr(),
            c_path.len() as c_int,
        );

        let len = sys::glowOutput_finishPackage(&mut output) as usize;
        buffer.truncate(len);
    }

    Ok(buffer)
}

/// Encode a `QualifiedParameter` response.
pub fn encode_qualified_parameter(path: &[u32], param: &ParameterInfo) -> Result<Vec<u8>> {
    init();

    const BUFFER_SIZE: usize = 65536;
    let mut buffer: Vec<u8> = vec![0; BUFFER_SIZE];

    unsafe {
        let mut output: sys::GlowOutput = std::mem::zeroed();
        sys::glowOutput_init(&mut output,
            buffer.as_mut_ptr(),
            BUFFER_SIZE as u32,
            0,
        );
        sys::glowOutput_beginPackage(&mut output, 1);

        let c_path: Vec<sys::berint> = path.iter().map(|&n| n as sys::berint).collect();
        let mut c_param: sys::GlowParameter = std::mem::zeroed();

        let c_identifier = CString::new(param.identifier.clone())
            .map_err(|e| Error::Glow(e.to_string()))?;
        c_param.pIdentifier = c_identifier.as_ptr() as *mut c_char;

        let mut fields = sys::EGlowFieldFlags_GlowFieldFlag_Identifier;

        let mut string_buffer: Option<CString> = None;
        let mut octet_buffer: Option<Vec<u8>> = None;
        c_param.value = param.value.to_c(&mut string_buffer, &mut octet_buffer);
        if param.value != GlowValue::None {
            fields |= sys::EGlowFieldFlags_GlowFieldFlag_Value;
        }

        c_param.access = param.access as i32;
        fields |= sys::EGlowFieldFlags_GlowFieldFlag_Access;

        c_param.type_ = param.parameter_type as i32;
        fields |= sys::EGlowFieldFlags_GlowFieldFlag_Type;

        sys::glow_writeQualifiedParameter(
            &mut output,
            &c_param,
            fields,
            c_path.as_ptr(),
            c_path.len() as c_int,
        );

        let len = sys::glowOutput_finishPackage(&mut output) as usize;
        buffer.truncate(len);
    }

    Ok(buffer)
}

/// A decoded Glow command.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Get directory of the element at the given path.
    GetDirectory {
        /// Path to the element.
        path: Vec<u32>,
        /// Optional field mask.
        dir_field_mask: Option<i32>,
    },
    /// Subscribe to value changes at the given path.
    Subscribe {
        /// Path to the parameter.
        path: Vec<u32>,
    },
    /// Unsubscribe from value changes at the given path.
    Unsubscribe {
        /// Path to the parameter.
        path: Vec<u32>,
    },
    /// Set the value of a parameter.
    SetValue {
        /// Path to the parameter.
        path: Vec<u32>,
        /// New value.
        value: GlowValue,
    },
    /// Invoke a function.
    Invoke {
        /// Path to the function.
        path: Vec<u32>,
        /// Invocation id.
        invocation_id: i32,
    },
    /// Other/unknown command.
    Other,
}

/// Encode a `GetDirectory` command for the element at `path`.
pub fn encode_get_directory_command(path: &[u32]) -> Result<Vec<u8>> {
    init();

    const BUFFER_SIZE: usize = 4096;
    let mut buffer: Vec<u8> = vec![0; BUFFER_SIZE];

    unsafe {
        let mut output: sys::GlowOutput = std::mem::zeroed();
        sys::glowOutput_init(&mut output,
            buffer.as_mut_ptr(),
            BUFFER_SIZE as u32,
            0,
        );
        sys::glowOutput_beginPackage(&mut output, 1);

        let c_path: Vec<sys::berint> = path.iter().map(|&n| n as sys::berint).collect();
        let mut c_command: sys::GlowCommand = std::mem::zeroed();
        c_command.number = sys::EGlowCommandType_GlowCommandType_GetDirectory;
        c_command.options.dirFieldMask = sys::EGlowFieldFlags_GlowFieldFlag_All;

        sys::glow_writeQualifiedCommand(
            &mut output,
            &c_command,
            c_path.as_ptr(),
            c_path.len() as c_int,
            sys::EGlowElementType_GlowElementType_Node,
        );

        let len = sys::glowOutput_finishPackage(&mut output) as usize;
        buffer.truncate(len);
    }

    Ok(buffer)
}

use std::sync::Mutex;

/// Decode a Glow payload (EmBER bytes without S101 framing) into a sequence of commands.
pub fn decode_glow_payload(payload: &[u8]) -> Result<Vec<Command>> {
    init();

    let commands: Box<Mutex<Vec<Command>>> = Box::new(Mutex::new(Vec::new()));
    let commands_ptr: *mut Mutex<Vec<Command>> = Box::into_raw(commands);

    unsafe {
        let mut reader: sys::NonFramingGlowReader = std::mem::zeroed();

        sys::nonFramingGlowReader_init(
            &mut reader,
            Some(on_node),
            Some(on_parameter),
            Some(on_command),
            Some(on_stream_entry),
            commands_ptr as *mut c_void,
        );

        let base = ptr::addr_of_mut!(reader.base);
        sys::emberAsyncReader_readBytes(base, payload.as_ptr(), payload.len() as c_int);

        sys::nonFramingGlowReader_free(&mut reader);
    }

    // Reclaim the boxed mutex and extract the commands.
    let commands = unsafe { Box::from_raw(commands_ptr) };
    commands
        .into_inner()
        .map_err(|_| Error::Glow("command mutex poisoned".into()))
}

unsafe extern "C" fn on_node(
    _p_node: *const sys::GlowNode,
    _fields: sys::EGlowFieldFlags,
    _p_path: *const sys::berint,
    _path_length: c_int,
    _state: *mut c_void,
) {
    // Directory responses contain nodes; we are interested in commands here.
}

unsafe extern "C" fn on_parameter(
    p_param: *const sys::GlowParameter,
    fields: sys::EGlowFieldFlags,
    p_path: *const sys::berint,
    path_length: c_int,
    state: *mut c_void,
) {
    let commands = &*(state as *const Mutex<Vec<Command>>);
    let path = slice_to_path(p_path, path_length);

    if fields & sys::EGlowFieldFlags_GlowFieldFlag_Value != 0 {
        let value = GlowValue::from_c(&(*p_param).value);
        commands.lock().unwrap().push(Command::SetValue { path, value });
    }
}

unsafe extern "C" fn on_command(
    p_command: *const sys::GlowCommand,
    p_path: *const sys::berint,
    path_length: c_int,
    state: *mut c_void,
) {
    let commands = &*(state as *const Mutex<Vec<Command>>);
    let path = slice_to_path(p_path, path_length);
    let cmd_type = (*p_command).number;
    let dir_field_mask = (*p_command).options.dirFieldMask;

    let command = match cmd_type {
        sys::EGlowCommandType_GlowCommandType_GetDirectory => Command::GetDirectory {
            path,
            dir_field_mask: if dir_field_mask != 0 {
                Some(dir_field_mask)
            } else {
                None
            },
        },
        sys::EGlowCommandType_GlowCommandType_Subscribe => Command::Subscribe { path },
        sys::EGlowCommandType_GlowCommandType_Unsubscribe => Command::Unsubscribe { path },
        _ => Command::Other,
    };

    commands.lock().unwrap().push(command);
}

unsafe extern "C" fn on_stream_entry(
    _p_entry: *const sys::GlowStreamEntry,
    _state: *mut c_void,
) {
    // Streams are not handled in the first milestone.
}

unsafe fn slice_to_path(p_path: *const sys::berint, path_length: c_int) -> Vec<u32> {
    if p_path.is_null() || path_length <= 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(p_path, path_length as usize)
            .iter()
            .map(|&n| n as u32)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_qualified_node_frame() {
        let node = NodeInfo {
            identifier: "Callie".to_string(),
            description: None,
        };
        let encoded = encode_qualified_node(&[1], &node).unwrap();
        assert!(!encoded.is_empty());
        // The output includes S101 framing; at minimum it should start with BOF.
        assert_eq!(encoded[0], crate::s101::BOF);
    }

    #[test]
    fn encode_qualified_parameter_frame() {
        let param = ParameterInfo {
            identifier: "Trigger".to_string(),
            description: None,
            value: GlowValue::Boolean(false),
            access: Access::ReadWrite,
            parameter_type: ParameterType::Boolean,
        };
        let encoded = encode_qualified_parameter(&[1, 2], &param).unwrap();
        assert!(!encoded.is_empty());
        assert_eq!(encoded[0], crate::s101::BOF);
    }
}
