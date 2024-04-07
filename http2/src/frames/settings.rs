use tracing::debug;

use crate::{server::ConnectionStream, utils};

use super::{
    errors::{FrameError, FrameResult},
    Frame,
};

#[repr(u16)]
#[derive(Debug, PartialEq)]
pub(crate) enum SettingType {
    HeaderTableSize = 1,
    EnablePush = 2,
    MaxConcurrentStreams = 3,
    InitialWindowSize = 4,
    MaxFrameSize = 5,
    MaxHeaderListSize = 6,
}

impl TryFrom<u16> for SettingType {
    type Error = FrameError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::HeaderTableSize),
            2 => Ok(Self::EnablePush),
            3 => Ok(Self::MaxConcurrentStreams),
            4 => Ok(Self::InitialWindowSize),
            5 => Ok(Self::MaxFrameSize),
            6 => Ok(Self::MaxHeaderListSize),
            _ => Err(FrameError::UnknownSettingIdentifier(value)),
        }
    }
}

pub(crate) type Setting = (SettingType, u32);

#[derive(Debug)]
pub(crate) struct Settings {
    is_ack: bool,
    settings: Vec<Setting>,
}

impl Settings {
    pub async fn receive_from_frame(
        frame: &Frame,
        stream: &mut ConnectionStream,
    ) -> FrameResult<Self> {
        let settings_payload = utils::receive_n_bytes(stream, frame.length as usize)
            .await
            .map_err(FrameError::IOError)?;

        Self::from_payload_bytes(frame, &settings_payload)
    }

    pub(self) fn from_payload_bytes(frame: &Frame, payload_bytes: &[u8]) -> FrameResult<Self> {
        let mut settings = Vec::with_capacity(frame.length as usize / 6);

        if frame.stream_id != 0 {
            return Err(FrameError::SettingNotStreamZero);
        }

        if frame.length % 6 != 0 {
            return Err(FrameError::BadSettingsLength(frame.length));
        }

        for setting_bytes in
            payload_bytes.chunks(std::mem::size_of::<u16>() + std::mem::size_of::<u32>())
        {
            let Ok(kind) = SettingType::try_from(u16::from_be_bytes(
                TryInto::<[u8; 2]>::try_into(&setting_bytes[..2]).expect("unreachable"),
            )) else {
                debug!("Unknown setting id. Ignoring");
                continue;
            };

            let value = u32::from_be_bytes(
                TryInto::<[u8; 4]>::try_into(&setting_bytes[2..]).expect("unreachable"),
            );

            settings.push((kind, value));
        }

        Ok(Settings {
            is_ack: frame.flags == 0x01,
            settings,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::frames::{
        errors::FrameError, Frame, FrameType, SettingType, Settings, FRAME_HEADER_SIZE,
    };

    #[test]
    fn valid_setting_parse() {
        const FRAME: Frame = Frame {
            length: 18,
            frame_type: FrameType::Settings,
            flags: 0,
            stream_id: 0,
        };
        const FRAME_BYTES: &[u8; 27] = include_bytes!("../../tests/data/settings_frame1.raw");
        let expected_settings = [
            (SettingType::MaxConcurrentStreams, 100),
            (SettingType::InitialWindowSize, 1073741824),
            (SettingType::EnablePush, 0),
        ];
        let result = Settings::from_payload_bytes(&FRAME, &FRAME_BYTES[FRAME_HEADER_SIZE..])
            .expect("Should succeed");

        assert!(!result.is_ack);
        assert_eq!(result.settings, expected_settings);
    }

    #[test]
    fn parse_setting_ack() {
        const FRAME: Frame = Frame {
            length: 18,
            frame_type: FrameType::Settings,
            flags: 0,
            stream_id: 0,
        };
        const FRAME_BYTES: &[u8; 9] = include_bytes!("../../tests/data/settings_ack.raw");
        let result = Settings::from_payload_bytes(&FRAME, &FRAME_BYTES[FRAME_HEADER_SIZE..])
            .expect("Should succeed");

        assert!(!result.is_ack);
        assert!(result.settings.is_empty());
    }

    #[test]
    fn setting_stream_not_zero() {
        const FRAME: Frame = Frame {
            length: 18,
            frame_type: FrameType::Settings,
            flags: 0,
            stream_id: 42,
        };

        let result = Settings::from_payload_bytes(&FRAME, &[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            FrameError::SettingNotStreamZero => (),
            _ => panic!("Incorrect error"),
        }
    }

    #[test]
    fn settings_incorrect_length() {
        const FRAME: Frame = Frame {
            length: 25,
            frame_type: FrameType::Settings,
            flags: 0,
            stream_id: 0,
        };

        let result = Settings::from_payload_bytes(&FRAME, &[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            FrameError::BadSettingsLength(25) => (),
            _ => panic!("Incorrect error"),
        }
    }
}
