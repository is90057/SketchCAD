use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique persistent identifier for any element in the CAD/BIM model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ElementId(pub Uuid);

impl ElementId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_u128(v: u128) -> Self {
        Self(Uuid::from_u128(v))
    }

    /// Converts to standard 22-character IFC GUID representation (base64 compressed uuid).
    pub fn to_ifc_guid(&self) -> String {
        // IFC uses a custom 64-character alphabet for 22-character GUIDs
        const IFC_CHARS: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";
        let bytes = self.0.as_bytes();
        let mut result = String::with_capacity(22);

        // Compress 16 bytes into 22 characters (6 bits per char)
        let mut num = 0u32;
        let mut n_bits = 0;

        for &b in bytes {
            num = (num << 8) | (b as u32);
            n_bits += 8;
            while n_bits >= 6 {
                n_bits -= 6;
                let idx = ((num >> n_bits) & 0x3F) as usize;
                result.push(IFC_CHARS[idx] as char);
            }
        }
        if n_bits > 0 {
            let idx = ((num << (6 - n_bits)) & 0x3F) as usize;
            result.push(IFC_CHARS[idx] as char);
        }

        result
    }

    /// Decodes a 22-character IFC GUID back into an ElementId (UUID).
    pub fn from_ifc_guid(guid: &str) -> Option<Self> {
        if guid.len() != 22 {
            return None;
        }
        const IFC_CHARS: &[u8; 64] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";
        let mut lookup = [255u8; 256];
        for (i, &b) in IFC_CHARS.iter().enumerate() {
            lookup[b as usize] = i as u8;
        }

        let mut num = 0u64;
        let mut n_bits = 0;
        let mut bytes = [0u8; 16];
        let mut byte_idx = 0;

        for &b in guid.as_bytes() {
            let val = lookup[b as usize];
            if val == 255 {
                return None;
            }
            num = (num << 6) | (val as u64);
            n_bits += 6;
            while n_bits >= 8 {
                n_bits -= 8;
                if byte_idx < 16 {
                    bytes[byte_idx] = ((num >> n_bits) & 0xFF) as u8;
                    byte_idx += 1;
                }
            }
        }

        Some(Self(Uuid::from_bytes(bytes)))
    }
}

impl Default for ElementId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ElementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ifc_guid_roundtrip() {
        for _ in 0..100 {
            let id = ElementId::new();
            let ifc_guid = id.to_ifc_guid();
            assert_eq!(ifc_guid.len(), 22);
            let restored = ElementId::from_ifc_guid(&ifc_guid).expect("Valid IFC GUID decode");
            assert_eq!(id, restored);
        }
    }
}
