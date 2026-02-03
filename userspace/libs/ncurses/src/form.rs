//! # Form Library
//!
//! -- IronGhost: Form system - data entry forms

use crate::{Result, WINDOW};

pub struct Form;
pub struct Field;

pub fn new_form(_fields: &[*mut Field]) -> *mut Form {
    core::ptr::null_mut()
}

pub fn free_form(_form: *mut Form) -> Result<()> {
    Ok(())
}

pub fn post_form(_form: *mut Form) -> Result<()> {
    Ok(())
}

pub fn unpost_form(_form: *mut Form) -> Result<()> {
    Ok(())
}

pub fn form_driver(_form: *mut Form, _ch: i32) -> Result<()> {
    Ok(())
}
