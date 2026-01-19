//! Built-in functions for GW-BASIC

use crate::error::{Error, Result};
use crate::value::Value;

/// Math functions
pub fn abs_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(val.as_double()?.abs()))
}

pub fn int_fn(val: Value) -> Result<Value> {
    Ok(Value::Integer(val.as_double()?.floor() as i32))
}

pub fn sqr_fn(val: Value) -> Result<Value> {
    let v = val.as_double()?;
    if v < 0.0 {
        return Err(Error::RuntimeError("Square root of negative number".to_string()));
    }
    Ok(Value::Double(v.sqrt()))
}

pub fn sin_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(val.as_double()?.sin()))
}

pub fn cos_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(val.as_double()?.cos()))
}

pub fn tan_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(val.as_double()?.tan()))
}

pub fn atn_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(val.as_double()?.atan()))
}

pub fn exp_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(val.as_double()?.exp()))
}

pub fn log_fn(val: Value) -> Result<Value> {
    let v = val.as_double()?;
    if v <= 0.0 {
        return Err(Error::RuntimeError("Logarithm of non-positive number".to_string()));
    }
    Ok(Value::Double(v.ln()))
}

pub fn sgn_fn(val: Value) -> Result<Value> {
    let v = val.as_double()?;
    let sign = if v > 0.0 { 1 } else if v < 0.0 { -1 } else { 0 };
    Ok(Value::Integer(sign))
}

pub fn fix_fn(val: Value) -> Result<Value> {
    Ok(Value::Integer(val.as_double()?.trunc() as i32))
}

pub fn cint_fn(val: Value) -> Result<Value> {
    Ok(Value::Integer(val.as_double()?.round() as i32))
}

pub fn csng_fn(val: Value) -> Result<Value> {
    Ok(Value::Single(val.as_double()? as f32))
}

pub fn cdbl_fn(val: Value) -> Result<Value> {
    Ok(Value::Double(val.as_double()?))
}

/// String functions
pub fn len_fn(val: Value) -> Result<Value> {
    Ok(Value::Integer(val.as_string().len() as i32))
}

pub fn asc_fn(val: Value) -> Result<Value> {
    let s = val.as_string();
    if s.is_empty() {
        return Err(Error::RuntimeError("ASC on empty string".to_string()));
    }
    Ok(Value::Integer(s.chars().next().unwrap() as i32))
}

pub fn chr_fn(val: Value) -> Result<Value> {
    let code = val.as_integer()?;
    if code < 0 || code > 255 {
        return Err(Error::RuntimeError(format!("CHR$ code out of range: {}", code)));
    }
    Ok(Value::String((code as u8 as char).to_string()))
}

pub fn str_fn(val: Value) -> Result<Value> {
    Ok(Value::String(val.to_string()))
}

pub fn val_fn(val: Value) -> Result<Value> {
    let string = val.as_string();
    let s = string.trim();
    if let Ok(i) = s.parse::<i32>() {
        Ok(Value::Integer(i))
    } else if let Ok(f) = s.parse::<f64>() {
        Ok(Value::Double(f))
    } else {
        Ok(Value::Integer(0))
    }
}

pub fn left_fn(s: Value, n: Value) -> Result<Value> {
    let string = s.as_string();
    let count = n.as_integer()? as usize;
    Ok(Value::String(string.chars().take(count).collect()))
}

pub fn right_fn(s: Value, n: Value) -> Result<Value> {
    let string = s.as_string();
    let count = n.as_integer()? as usize;
    let chars: Vec<char> = string.chars().collect();
    let start = if count > chars.len() { 0 } else { chars.len() - count };
    Ok(Value::String(chars[start..].iter().collect()))
}

pub fn mid_fn(s: Value, start: Value, len: Option<Value>) -> Result<Value> {
    let string = s.as_string();
    let start_pos = (start.as_integer()? - 1).max(0) as usize;
    let chars: Vec<char> = string.chars().collect();
    
    if start_pos >= chars.len() {
        return Ok(Value::String(String::new()));
    }
    
    let result = if let Some(length) = len {
        let count = length.as_integer()? as usize;
        chars[start_pos..].iter().take(count).collect()
    } else {
        chars[start_pos..].iter().collect()
    };
    
    Ok(Value::String(result))
}

pub fn space_fn(n: Value) -> Result<Value> {
    let count = n.as_integer()?;
    if count < 0 {
        return Err(Error::RuntimeError("SPACE$ count cannot be negative".to_string()));
    }
    Ok(Value::String(" ".repeat(count as usize)))
}

pub fn string_fn(n: Value, ch: Value) -> Result<Value> {
    let count = n.as_integer()?;
    if count < 0 {
        return Err(Error::RuntimeError("STRING$ count cannot be negative".to_string()));
    }
    
    let char_code = if ch.is_string() {
        let s = ch.as_string();
        if s.is_empty() {
            return Err(Error::RuntimeError("STRING$ character cannot be empty".to_string()));
        }
        s.chars().next().unwrap()
    } else {
        let code = ch.as_integer()?;
        if code < 0 || code > 255 {
            return Err(Error::RuntimeError("STRING$ code out of range".to_string()));
        }
        code as u8 as char
    };
    
    Ok(Value::String(char_code.to_string().repeat(count as usize)))
}

pub fn instr_fn(start: Option<Value>, haystack: Value, needle: Value) -> Result<Value> {
    let start_pos = if let Some(s) = start {
        (s.as_integer()? - 1).max(0) as usize
    } else {
        0
    };
    
    let hay = haystack.as_string();
    let need = needle.as_string();
    
    if start_pos >= hay.len() {
        return Ok(Value::Integer(0));
    }
    
    if let Some(pos) = hay[start_pos..].find(&need) {
        Ok(Value::Integer((start_pos + pos + 1) as i32))
    } else {
        Ok(Value::Integer(0))
    }
}

pub fn hex_fn(val: Value) -> Result<Value> {
    Ok(Value::String(format!("{:X}", val.as_integer()?)))
}

pub fn oct_fn(val: Value) -> Result<Value> {
    Ok(Value::String(format!("{:o}", val.as_integer()?)))
}

/// Conversion functions
pub fn peek_fn(_addr: Value) -> Result<Value> {
    // Simulated - returns 0
    Ok(Value::Integer(0))
}

pub fn inp_fn(_port: Value) -> Result<Value> {
    // Simulated - returns 0
    Ok(Value::Integer(0))
}

/// System functions
pub fn rnd_fn(seed: Option<Value>) -> Result<Value> {
    use std::cell::RefCell;
    thread_local! {
        static RNG_STATE: RefCell<u64> = RefCell::new(12345);
    }
    
    RNG_STATE.with(|state| {
        let mut s = state.borrow_mut();
        
        if let Some(seed_val) = seed {
            let sv = seed_val.as_double()?;
            if sv < 0.0 {
                *s = (sv.abs() * 1000000.0) as u64;
            } else if sv == 0.0 {
                // Return last random number (simplified)
                return Ok(Value::Single((*s % 1000) as f32 / 1000.0));
            }
        }
        
        // Simple LCG
        *s = (*s * 1103515245 + 12345) & 0x7fffffff;
        Ok(Value::Single((*s % 1000000) as f32 / 1000000.0))
    })
}

pub fn timer_fn() -> Result<Value> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap();
    let seconds_since_midnight = (now.as_secs() % 86400) as f32;
    Ok(Value::Single(seconds_since_midnight))
}

/// Additional string functions
pub fn lcase_fn(val: Value) -> Result<Value> {
    Ok(Value::String(val.as_string().to_lowercase()))
}

pub fn ucase_fn(val: Value) -> Result<Value> {
    Ok(Value::String(val.as_string().to_uppercase()))
}

pub fn input_fn(n: Value, file_num: Option<Value>) -> Result<Value> {
    let count = n.as_integer()? as usize;
    if file_num.is_some() {
        // File input - simulated
        Ok(Value::String(" ".repeat(count)))
    } else {
        // Console input - try to read from stdin
        use std::io::{self, Read};
        let mut buffer = vec![0u8; count];
        match io::stdin().read_exact(&mut buffer) {
            Ok(_) => Ok(Value::String(String::from_utf8_lossy(&buffer).to_string())),
            Err(_) => {
                // Non-interactive mode - return empty/space string
                // This allows graphics programs to run without hanging
                Ok(Value::String(" ".repeat(count)))
            }
        }
    }
}

/// Conversion functions
pub fn cvi_fn(val: Value) -> Result<Value> {
    let s = val.as_string();
    if s.len() < 2 {
        return Err(Error::RuntimeError("CVI requires 2-byte string".to_string()));
    }
    let bytes = s.as_bytes();
    let n = i16::from_le_bytes([bytes[0], bytes[1]]) as i32;
    Ok(Value::Integer(n))
}

pub fn cvs_fn(val: Value) -> Result<Value> {
    let s = val.as_string();
    if s.len() < 4 {
        return Err(Error::RuntimeError("CVS requires 4-byte string".to_string()));
    }
    let bytes = s.as_bytes();
    let n = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Ok(Value::Single(n))
}

pub fn cvd_fn(val: Value) -> Result<Value> {
    let s = val.as_string();
    if s.len() < 8 {
        return Err(Error::RuntimeError("CVD requires 8-byte string".to_string()));
    }
    let bytes = s.as_bytes();
    let n = f64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5], bytes[6], bytes[7],
    ]);
    Ok(Value::Double(n))
}

pub fn mki_fn(val: Value) -> Result<Value> {
    let n = val.as_integer()? as i16;
    let bytes = n.to_le_bytes();
    Ok(Value::String(String::from_utf8_lossy(&bytes).to_string()))
}

pub fn mks_fn(val: Value) -> Result<Value> {
    let n = val.as_double()? as f32;
    let bytes = n.to_le_bytes();
    Ok(Value::String(String::from_utf8_lossy(&bytes).to_string()))
}

pub fn mkd_fn(val: Value) -> Result<Value> {
    let n = val.as_double()?;
    let bytes = n.to_le_bytes();
    Ok(Value::String(String::from_utf8_lossy(&bytes).to_string()))
}

/// System functions
pub fn fre_fn(_val: Value) -> Result<Value> {
    // Simulated - return large number for free memory
    Ok(Value::Integer(65000))
}

pub fn varptr_fn(_var_name: Value) -> Result<Value> {
    // Simulated - return dummy pointer
    Ok(Value::Integer(0))
}

pub fn inkey_fn() -> Result<Value> {
    // Simulated - would check keyboard without waiting
    Ok(Value::String(String::new()))
}

pub fn date_fn() -> Result<Value> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap();
    let days_since_epoch = now.as_secs() / 86400;
    // Simple date format MM-DD-YYYY (simplified)
    Ok(Value::String(format!("{:02}-{:02}-{:04}", 
        (days_since_epoch % 365) / 30 + 1,
        (days_since_epoch % 365) % 30 + 1,
        1970 + days_since_epoch / 365)))
}

pub fn time_fn() -> Result<Value> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap();
    let seconds = now.as_secs() % 86400;
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    Ok(Value::String(format!("{:02}:{:02}:{:02}", hours, minutes, secs)))
}

pub fn pos_fn(_dummy: Value) -> Result<Value> {
    // Return current cursor column (simulated)
    Ok(Value::Integer(1))
}

pub fn csrlin_fn() -> Result<Value> {
    // Return current cursor row (simulated)
    Ok(Value::Integer(1))
}

/// File functions (placeholders - would need FileManager reference)
pub fn eof_fn(file_num: Value) -> Result<Value> {
    let _num = file_num.as_integer()?;
    // Simplified - would check actual file EOF
    Ok(Value::Integer(0))
}

pub fn loc_fn(file_num: Value) -> Result<Value> {
    let _num = file_num.as_integer()?;
    // Return file position
    Ok(Value::Integer(0))
}

pub fn lof_fn(file_num: Value) -> Result<Value> {
    let _num = file_num.as_integer()?;
    // Return file length
    Ok(Value::Integer(0))
}

/// Screen functions
pub fn point_fn(x: Value, y: Value) -> Result<Value> {
    let _x = x.as_integer()?;
    let _y = y.as_integer()?;
    // Return pixel color at position (simulated)
    Ok(Value::Integer(0))
}

pub fn screen_fn(row: Value, col: Value, color_num: Option<Value>) -> Result<Value> {
    let _r = row.as_integer()?;
    let _c = col.as_integer()?;
    if let Some(_cn) = color_num {
        // Return color at position
        Ok(Value::Integer(0))
    } else {
        // Return character at position
        Ok(Value::Integer(32)) // Space
    }
}

/// Error handling functions
pub fn erl_fn() -> Result<Value> {
    // Return line number where error occurred (simulated)
    // In real implementation, would track from error handler
    Ok(Value::Integer(0))
}

pub fn err_fn() -> Result<Value> {
    // Return error code (simulated)
    // In real implementation, would return last error code
    Ok(Value::Integer(0))
}

pub fn erdev_fn() -> Result<Value> {
    // Return device error code (simulated)
    Ok(Value::Integer(0))
}

pub fn erdev_string_fn() -> Result<Value> {
    // Return device error string (simulated)
    Ok(Value::String(String::new()))
}

/// Environment and system functions
pub fn environ_fn(val: Value) -> Result<Value> {
    // Get environment variable
    let var_name = if let Ok(name) = val.as_string_result() {
        name
    } else {
        // If numeric, get by index (not commonly used)
        return Ok(Value::String(String::new()));
    };
    
    match std::env::var(var_name) {
        Ok(value) => Ok(Value::String(value)),
        Err(_) => Ok(Value::String(String::new())),
    }
}

/// I/O control functions
pub fn ioctl_fn(file_num: Value) -> Result<Value> {
    let _num = file_num.as_integer()?;
    // Return IOCTL string (simulated)
    Ok(Value::String(String::new()))
}

/// Joystick functions
pub fn stick_fn(val: Value) -> Result<Value> {
    let _n = val.as_integer()?;
    // Return joystick coordinate (simulated - no actual hardware)
    Ok(Value::Integer(0))
}

pub fn strig_fn(val: Value) -> Result<Value> {
    let _n = val.as_integer()?;
    // Return joystick trigger state (simulated - no actual hardware)
    Ok(Value::Integer(0))
}

/// File I/O functions
pub fn fileattr_fn(filenum: Value, attribute: Value) -> Result<Value> {
    let _fnum = filenum.as_integer()?;
    let _attr = attribute.as_integer()?;
    // Return file attribute (simulated)
    // attribute: 1=mode, 2=handle
    Ok(Value::Integer(0))
}

pub fn ioctl_string_fn(filenum: Value) -> Result<Value> {
    let _fnum = filenum.as_integer()?;
    // Return IOCTL control string (simulated)
    Ok(Value::String(String::new()))
}

/// Machine language function call
pub fn usr_fn(_index: Option<Value>, arg: Value) -> Result<Value> {
    // Validate the argument can be converted to double (required by GW-BASIC spec)
    // The value is discarded since we only simulate the call and return 0
    let _ = arg.as_double()?;
    // USR function call (simulated - machine language calls not supported)
    // In real GW-BASIC, this would call a user-defined machine language routine
    // at the address specified by index and pass the argument
    Ok(Value::Integer(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_math_functions() {
        assert_eq!(abs_fn(Value::Integer(-5)).unwrap().as_integer().unwrap(), 5);
        assert_eq!(int_fn(Value::Double(3.7)).unwrap().as_integer().unwrap(), 3);
        assert!((sqr_fn(Value::Integer(16)).unwrap().as_double().unwrap() - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_string_functions() {
        assert_eq!(len_fn(Value::String("Hello".to_string())).unwrap().as_integer().unwrap(), 5);
        assert_eq!(asc_fn(Value::String("A".to_string())).unwrap().as_integer().unwrap(), 65);
        assert_eq!(chr_fn(Value::Integer(65)).unwrap().as_string(), "A");
    }

    #[test]
    fn test_left_right_mid() {
        let s = Value::String("HELLO".to_string());
        assert_eq!(left_fn(s.clone(), Value::Integer(2)).unwrap().as_string(), "HE");
        assert_eq!(right_fn(s.clone(), Value::Integer(2)).unwrap().as_string(), "LO");
        assert_eq!(mid_fn(s, Value::Integer(2), Some(Value::Integer(3))).unwrap().as_string(), "ELL");
    }
    
    #[test]
    fn test_case_functions() {
        assert_eq!(lcase_fn(Value::String("HELLO".to_string())).unwrap().as_string(), "hello");
        assert_eq!(ucase_fn(Value::String("hello".to_string())).unwrap().as_string(), "HELLO");
    }
    
    #[test]
    fn test_usr_function() {
        // Test USR without index
        assert_eq!(usr_fn(None, Value::Integer(100)).unwrap().as_integer().unwrap(), 0);
        // Test USR with index
        assert_eq!(usr_fn(Some(Value::Integer(5)), Value::Double(3.14)).unwrap().as_integer().unwrap(), 0);
    }
}