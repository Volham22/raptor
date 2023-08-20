#[derive(Debug)]
pub struct WindowUpdate(pub u32);

impl WindowUpdate {
    pub fn from_bytes(value: &[u8], length: usize) -> Result<Self, &'static str> {
        if length < 4 {
            return Err("Not enought data in buffer");
        }

        Ok(Self(u32::from_be_bytes([
            value[0], value[1], value[2], value[3],
        ])))
    }
}
