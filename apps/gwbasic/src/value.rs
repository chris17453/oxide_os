//! Value types for the GW-BASIC interpreter

use crate::error::{Error, Result};
use std::fmt;

/// Represents a value in GW-BASIC
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Integer value
    Integer(i32),
    
    /// Single-precision floating point
    Single(f32),
    
    /// Double-precision floating point
    Double(f64),
    
    /// String value
    String(String),
    
    /// Nil/Empty value
    Nil,
}

impl Value {
    /// Convert value to integer
    pub fn as_integer(&self) -> Result<i32> {
        match self {
            Value::Integer(i) => Ok(*i),
            Value::Single(f) => Ok(*f as i32),
            Value::Double(d) => Ok(*d as i32),
            Value::String(s) => s.parse::<i32>()
                .map_err(|_| Error::TypeError(format!("Cannot convert '{}' to integer", s))),
            Value::Nil => Ok(0),
        }
    }

    /// Convert value to double
    pub fn as_double(&self) -> Result<f64> {
        match self {
            Value::Integer(i) => Ok(*i as f64),
            Value::Single(f) => Ok(*f as f64),
            Value::Double(d) => Ok(*d),
            Value::String(s) => s.parse::<f64>()
                .map_err(|_| Error::TypeError(format!("Cannot convert '{}' to double", s))),
            Value::Nil => Ok(0.0),
        }
    }

    /// Convert value to string
    pub fn as_string(&self) -> String {
        match self {
            Value::Integer(i) => i.to_string(),
            Value::Single(f) => f.to_string(),
            Value::Double(d) => d.to_string(),
            Value::String(s) => s.clone(),
            Value::Nil => String::new(),
        }
    }

    /// Check if value is numeric
    pub fn is_numeric(&self) -> bool {
        matches!(self, Value::Integer(_) | Value::Single(_) | Value::Double(_))
    }

    /// Check if value is string
    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }
    
    /// Convert value to string with Result
    pub fn as_string_result(&self) -> Result<String> {
        match self {
            Value::String(s) => Ok(s.clone()),
            _ => Err(Error::TypeError("Expected string value".to_string())),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Integer(i) => write!(f, "{}", i),
            Value::Single(s) => write!(f, "{}", s),
            Value::Double(d) => write!(f, "{}", d),
            Value::String(s) => write!(f, "{}", s),
            Value::Nil => write!(f, ""),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integer_value() {
        let val = Value::Integer(42);
        assert_eq!(val.as_integer().unwrap(), 42);
        assert_eq!(val.as_double().unwrap(), 42.0);
        assert_eq!(val.as_string(), "42");
        assert!(val.is_numeric());
        assert!(!val.is_string());
    }

    #[test]
    fn test_string_value() {
        let val = Value::String("Hello".to_string());
        assert_eq!(val.as_string(), "Hello");
        assert!(!val.is_numeric());
        assert!(val.is_string());
    }

    #[test]
    fn test_value_display() {
        let val = Value::Integer(123);
        assert_eq!(val.to_string(), "123");
    }

    #[test]
    fn test_nil_value() {
        let val = Value::Nil;
        assert_eq!(val.as_integer().unwrap(), 0);
        assert_eq!(val.as_double().unwrap(), 0.0);
        assert_eq!(val.as_string(), "");
    }
}