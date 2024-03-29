use std::mem::size_of;

use bytes::BufMut;
use tracing::warn;

use crate::http2::response::ResponseSerialize;

use super::{FrameError, MIN_FRAME_SIZE};

const TUPLE_LENGTH: usize = size_of::<u16>() + size_of::<u32>();

#[repr(u16)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SettingKind {
    HeaderTableSize = 1,
    EnablePush = 2,
    MaxConcurrentStreams = 3,
    InitialWindowSize = 4,
    MaxFrameSize = 5,
    MaxHeaderListSize = 6,
}

impl TryFrom<u16> for SettingKind {
    type Error = &'static str;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(SettingKind::HeaderTableSize),
            2 => Ok(SettingKind::EnablePush),
            3 => Ok(SettingKind::MaxConcurrentStreams),
            4 => Ok(SettingKind::InitialWindowSize),
            5 => Ok(SettingKind::MaxFrameSize),
            6 => Ok(SettingKind::MaxHeaderListSize),
            _ => Err("Unknown parameter number"),
        }
    }
}

pub type Setting = (SettingKind, u32);

#[derive(Clone, Debug)]
pub struct Settings {
    is_ack: bool,
    flags: Vec<Setting>,
}

/// Get server's default settings
impl Default for Settings {
    fn default() -> Self {
        Self {
            is_ack: false,
            flags: vec![
                (SettingKind::MaxConcurrentStreams, 1000),
                (SettingKind::MaxHeaderListSize, 100),
            ],
        }
    }
}

impl Settings {
    pub fn from_bytes(value: &[u8], length: usize) -> Result<Self, FrameError> {
        // A setting is 16 bits identifier + 32 bits value so 6 bytes
        if length % 6 != 0 {
            return Err(FrameError::SettingsFrameSize(length));
        }

        if value.len() < length {
            return Err(FrameError::BadFrameSize(length));
        }

        let mut flags: Vec<Setting> = Vec::new();
        for i in (0..length).step_by(6) {
            let identifier = u16::from_be_bytes([value[i], value[i + 1]]);

            let setting_value =
                u32::from_be_bytes([value[i + 2], value[i + 3], value[i + 4], value[i + 5]]);

            if let Ok(sid) = SettingKind::try_from(identifier) {
                flags.push((sid, setting_value));
            } else {
                warn!("Unknown setting identifier: {}. Ignoring...", identifier);
            }
        }

        Ok(Self {
            flags,
            is_ack: false,
        })
    }

    pub fn new_ack() -> Self {
        Self {
            is_ack: true,
            flags: Vec::new(),
        }
    }

    pub fn get_max_frame_size(&self) -> u32 {
        self.flags
            .iter()
            .find(|f| f.0 == SettingKind::MaxFrameSize)
            .map(|f| f.1) // map to value
            .unwrap_or(MIN_FRAME_SIZE)
    }

    pub fn get_initial_window_size(&self) -> Option<u32> {
        self.flags
            .iter()
            .rfind(|f| f.0 == SettingKind::InitialWindowSize)
            .map(|f| f.1)
    }

    pub fn is_push_enabled(&self) -> Option<u32> {
        self.flags
            .iter()
            .find(|f| f.0 == SettingKind::EnablePush)
            .map(|f| f.1)
    }
}

impl ResponseSerialize for Settings {
    fn serialize_response(&self, _: Option<&mut hpack::Encoder>) -> Vec<u8> {
        let mut result = Vec::new();
        if self.is_ack {
            return result;
        }

        for (kind, value) in &self.flags {
            // Setting id
            result.put_slice(&(*kind as u16).to_be_bytes());
            // value
            result.put_slice(&value.to_be_bytes())
        }

        result
    }

    fn compute_frame_length(&self, _: Option<&mut hpack::Encoder>) -> u32 {
        (TUPLE_LENGTH * self.flags.len()) as u32
    }

    fn get_flags(&self) -> u8 {
        if self.is_ack {
            0x01
        } else {
            0x00
        }
    }
}
