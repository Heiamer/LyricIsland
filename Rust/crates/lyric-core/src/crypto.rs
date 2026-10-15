//! QRC / KRC decryption, ported from `Lyricify.Lyrics.Decrypter.*`.
//!
//! * KRC (Kugou): base64 → drop a 4 byte header → XOR with a fixed 16 byte key →
//!   zlib inflate → drop the UTF-8 BOM character.
//! * QRC (QQ Music): hex text → Triple-DES ECB decrypt with a fixed ASCII key →
//!   zlib inflate → strip the UTF-8 BOM.
//!
//! Triple-DES is implemented here because the original delegated to
//! `ICSharpCode.SharpZipLib` + a hand written DES helper; the port keeps the same
//! algorithm (`D(k1, E(k2, D(k3, block)))`) with no external crypto dependency.

use base64::Engine;
use flate2::read::ZlibDecoder;
use std::io::Read;

/// Decryption failure. The original threw `FormatException`/`CryptographicException`;
/// the coordinator treats every variant as "invalid payload".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecryptError {
    InvalidBase64,
    InvalidHex,
    InvalidBlockLength,
    InflateFailed,
    InvalidUtf8,
}

impl std::fmt::Display for DecryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            DecryptError::InvalidBase64 => "payload is not valid base64",
            DecryptError::InvalidHex => "payload is not valid hex",
            DecryptError::InvalidBlockLength => "cipher text is not a whole number of blocks",
            DecryptError::InflateFailed => "deflate stream could not be inflated",
            DecryptError::InvalidUtf8 => "decrypted payload is not valid text",
        };
        f.write_str(text)
    }
}

impl std::error::Error for DecryptError {}

// ---------------------------------------------------------------------------
// KRC
// ---------------------------------------------------------------------------

const KRC_DECRYPT_KEY: [u8; 16] = [
    0x40, 0x47, 0x61, 0x77, 0x5e, 0x32, 0x74, 0x47, 0x51, 0x36, 0x31, 0x2d, 0xce, 0xd2, 0x6e, 0x69,
];

/// `Lyricify.Lyrics.Decrypter.Krc.Decrypter.DecryptLyrics`.
pub fn decrypt_krc(encrypted: &str) -> Result<String, DecryptError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encrypted.trim())
        .map_err(|_| DecryptError::InvalidBase64)?;

    let mut data = decoded
        .get(4..)
        .ok_or(DecryptError::InvalidBlockLength)?
        .to_vec();

    for (index, byte) in data.iter_mut().enumerate() {
        *byte ^= KRC_DECRYPT_KEY[index % KRC_DECRYPT_KEY.len()];
    }

    let inflated = inflate(&data)?;
    let text = String::from_utf8(inflated).map_err(|_| DecryptError::InvalidUtf8)?;
    // `res[1..]` in the original: the first character is a BOM marker.
    Ok(text.chars().skip(1).collect())
}

/// Inverse of [`decrypt_krc`] — used by the port's round-trip tests and by fixtures.
pub fn encrypt_krc(plain: &str) -> String {
    let mut data: Vec<u8> = vec![0xEF, 0xBB, 0xBF];
    data.extend_from_slice(plain.as_bytes());

    let deflated = deflate(&data).unwrap_or_default();

    // 4 byte host header, then the XOR layer.
    let mut payload: Vec<u8> = vec![0x00, 0x00, 0x00, 0x00];
    for (index, byte) in deflated.iter().enumerate() {
        payload.push(byte ^ KRC_DECRYPT_KEY[index % KRC_DECRYPT_KEY.len()]);
    }
    base64::engine::general_purpose::STANDARD.encode(payload)
}

// ---------------------------------------------------------------------------
// QRC
// ---------------------------------------------------------------------------

const QRC_KEY: &[u8; 24] = b"!@#)(*$%123ZXC!@!@#)(NHL";
const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// `Lyricify.Lyrics.Decrypter.Qrc.Decrypter.DecryptLyrics`.
pub fn decrypt_qrc(encrypted_hex: &str) -> Result<String, DecryptError> {
    let cleaned: String = encrypted_hex
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect();
    let bytes = hex_to_bytes(&cleaned).ok_or(DecryptError::InvalidHex)?;
    let decrypted = triple_des_decrypt_ecb(QRC_KEY, &bytes)?;
    let inflated = inflate(&decrypted)?;

    let without_bom = if inflated.starts_with(&UTF8_BOM) {
        &inflated[UTF8_BOM.len()..]
    } else {
        &inflated[..]
    };

    String::from_utf8(without_bom.to_vec()).map_err(|_| DecryptError::InvalidUtf8)
}

/// Inverse of [`decrypt_qrc`] — used by the port's round-trip tests and by fixtures.
pub fn encrypt_qrc(plain: &str) -> Result<String, DecryptError> {
    let mut data: Vec<u8> = UTF8_BOM.to_vec();
    data.extend_from_slice(plain.as_bytes());

    let deflated = deflate(&data)?;
    let cipher = triple_des_encrypt_ecb(QRC_KEY, &pad_to_block(&deflated))?;
    Ok(cipher
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>())
}

/// Triple-DES is a block cipher, so the payload has to be padded. Trailing zero bytes
/// after the deflate stream end are ignored by the inflater, which is exactly how the
/// QQ Music container handles the tail.
fn pad_to_block(data: &[u8]) -> Vec<u8> {
    let mut padded = data.to_vec();
    while padded.len() % 8 != 0 {
        padded.push(0);
    }
    padded
}

fn deflate(data: &[u8]) -> Result<Vec<u8>, DecryptError> {
    use std::io::Write;
    let mut out = Vec::new();
    {
        let mut encoder = flate2::write::ZlibEncoder::new(&mut out, flate2::Compression::default());
        encoder
            .write_all(data)
            .map_err(|_| DecryptError::InflateFailed)?;
        encoder.finish().map_err(|_| DecryptError::InflateFailed)?;
    }
    Ok(out)
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 {
        return None;
    }
    let bytes = hex.as_bytes();
    let mut out = Vec::with_capacity(hex.len() / 2);
    for chunk in bytes.chunks(2) {
        let high = (chunk[0] as char).to_digit(16)?;
        let low = (chunk[1] as char).to_digit(16)?;
        out.push(((high << 4) | low) as u8);
    }
    Some(out)
}

fn inflate(data: &[u8]) -> Result<Vec<u8>, DecryptError> {
    let mut out = Vec::new();
    let mut decoder = ZlibDecoder::new(data);
    decoder
        .read_to_end(&mut out)
        .map_err(|_| DecryptError::InflateFailed)?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Triple DES (EDE, ECB)
// ---------------------------------------------------------------------------

/// `D(k1, E(k2, D(k3, block)))`.
pub fn triple_des_decrypt_ecb(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptError> {
    triple_des_ecb(key, data, false)
}

/// `E(k3, D(k2, E(k1, block)))`.
pub fn triple_des_encrypt_ecb(key: &[u8], data: &[u8]) -> Result<Vec<u8>, DecryptError> {
    triple_des_ecb(key, data, true)
}

fn triple_des_ecb(key: &[u8], data: &[u8], encrypt: bool) -> Result<Vec<u8>, DecryptError> {
    if key.len() < 24 {
        return Err(DecryptError::InvalidBlockLength);
    }
    if data.len() % 8 != 0 {
        return Err(DecryptError::InvalidBlockLength);
    }

    let k1: [u8; 8] = key[0..8].try_into().expect("8 byte key");
    let k2: [u8; 8] = key[8..16].try_into().expect("8 byte key");
    let k3: [u8; 8] = key[16..24].try_into().expect("8 byte key");

    let mut out = Vec::with_capacity(data.len());
    for block in data.chunks(8) {
        let block: [u8; 8] = block.try_into().expect("8 byte block");
        let result = if encrypt {
            let first = des_encrypt_block(&k1, &block);
            let second = des_decrypt_block(&k2, &first);
            des_encrypt_block(&k3, &second)
        } else {
            let first = des_decrypt_block(&k3, &block);
            let second = des_encrypt_block(&k2, &first);
            des_decrypt_block(&k1, &second)
        };
        out.extend_from_slice(&result);
    }
    Ok(out)
}

/// Plain single DES block encryption.
pub fn des_encrypt_block(key: &[u8; 8], block: &[u8; 8]) -> [u8; 8] {
    des_process_block(key, block, true)
}

/// Plain single DES block decryption.
pub fn des_decrypt_block(key: &[u8; 8], block: &[u8; 8]) -> [u8; 8] {
    des_process_block(key, block, false)
}

fn des_process_block(key: &[u8; 8], block: &[u8; 8], encrypt: bool) -> [u8; 8] {
    let subkeys = des_subkeys(key);
    let input = u64::from_be_bytes(*block);
    let permuted = permute(input, &IP, 64);
    let mut left = (permuted >> 32) as u32;
    let mut right = (permuted & 0xFFFF_FFFF) as u32;

    let order: Vec<usize> = if encrypt {
        (0..16).collect()
    } else {
        (0..16).rev().collect()
    };

    for index in order {
        let next_left = right;
        let next_right = left ^ feistel(right, subkeys[index]);
        left = next_left;
        right = next_right;
    }

    let pre_output = ((right as u64) << 32) | (left as u64);
    let output = permute(pre_output, &FP, 64);
    output.to_be_bytes()
}

fn des_subkeys(key: &[u8; 8]) -> [u64; 16] {
    let key_bits = u64::from_be_bytes(*key);
    let permuted = permute(key_bits, &PC1, 64);
    let mut c = ((permuted >> 28) & 0x0FFF_FFFF) as u32;
    let mut d = (permuted & 0x0FFF_FFFF) as u32;

    let mut subkeys = [0u64; 16];
    for (round, shift) in SHIFTS.iter().enumerate() {
        c = ((c << shift) | (c >> (28 - shift))) & 0x0FFF_FFFF;
        d = ((d << shift) | (d >> (28 - shift))) & 0x0FFF_FFFF;
        let combined = ((c as u64) << 28) | d as u64;
        subkeys[round] = permute(combined, &PC2, 56);
    }
    subkeys
}

fn feistel(right: u32, subkey: u64) -> u32 {
    let expanded = permute(right as u64, &E, 32);
    let mixed = expanded ^ subkey;
    let mut output: u32 = 0;

    for box_index in 0..8 {
        let shift = 42 - 6 * box_index;
        let six_bits = ((mixed >> shift) & 0x3F) as usize;
        let row = ((six_bits & 0x20) >> 4) | (six_bits & 1);
        let column = (six_bits >> 1) & 0x0F;
        let value = S_BOXES[box_index][row * 16 + column] as u32;
        output = (output << 4) | value;
    }

    permute(output as u64, &P, 32) as u32
}

/// `input` bits are read from the MSB of `value`, `table` holds 1-based bit positions.
fn permute(value: u64, table: &[u8], input_bits: usize) -> u64 {
    let mut output = 0u64;
    for position in table {
        let bit_index = input_bits as u64 - *position as u64;
        let bit = (value >> bit_index) & 1;
        output = (output << 1) | bit;
    }
    output
}

const IP: [u8; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6, 64,
    56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61, 53,
    45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
];

const FP: [u8; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30,
    37, 5, 45, 13, 53, 21, 61, 29, 36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27, 34,
    2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
];

const E: [u8; 48] = [
    32, 1, 2, 3, 4, 5, 4, 5, 6, 7, 8, 9, 8, 9, 10, 11, 12, 13, 12, 13, 14, 15, 16, 17, 16, 17, 18,
    19, 20, 21, 20, 21, 22, 23, 24, 25, 24, 25, 26, 27, 28, 29, 28, 29, 30, 31, 32, 1,
];

const P: [u8; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19, 13,
    30, 6, 22, 11, 4, 25,
];

const PC1: [u8; 56] = [
    57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59, 51, 43, 35, 27, 19, 11, 3, 60,
    52, 44, 36, 63, 55, 47, 39, 31, 23, 15, 7, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29, 21,
    13, 5, 28, 20, 12, 4,
];

const PC2: [u8; 48] = [
    14, 17, 11, 24, 1, 5, 3, 28, 15, 6, 21, 10, 23, 19, 12, 4, 26, 8, 16, 7, 27, 20, 13, 2, 41, 52,
    31, 37, 47, 55, 30, 40, 51, 45, 33, 48, 44, 49, 39, 56, 34, 53, 46, 42, 50, 36, 29, 32,
];

const SHIFTS: [u32; 16] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];

const S_BOXES: [[u8; 64]; 8] = [
    [
        14, 4, 13, 1, 2, 15, 11, 8, 3, 10, 6, 12, 5, 9, 0, 7, 0, 15, 7, 4, 14, 2, 13, 1, 10, 6, 12,
        11, 9, 5, 3, 8, 4, 1, 14, 8, 13, 6, 2, 11, 15, 12, 9, 7, 3, 10, 5, 0, 15, 12, 8, 2, 4, 9, 1,
        7, 5, 11, 3, 14, 10, 0, 6, 13,
    ],
    [
        15, 1, 8, 14, 6, 11, 3, 4, 9, 7, 2, 13, 12, 0, 5, 10, 3, 13, 4, 7, 15, 2, 8, 14, 12, 0, 1,
        10, 6, 9, 11, 5, 0, 14, 7, 11, 10, 4, 13, 1, 5, 8, 12, 6, 9, 3, 2, 15, 13, 8, 10, 1, 3, 15,
        4, 2, 11, 6, 7, 12, 0, 5, 14, 9,
    ],
    [
        10, 0, 9, 14, 6, 3, 15, 5, 1, 13, 12, 7, 11, 4, 2, 8, 13, 7, 0, 9, 3, 4, 6, 10, 2, 8, 5, 14,
        12, 11, 15, 1, 13, 6, 4, 9, 8, 15, 3, 0, 11, 1, 2, 12, 5, 10, 14, 7, 1, 10, 13, 0, 6, 9, 8,
        7, 4, 15, 14, 3, 11, 5, 2, 12,
    ],
    [
        7, 13, 14, 3, 0, 6, 9, 10, 1, 2, 8, 5, 11, 12, 4, 15, 13, 8, 11, 5, 6, 15, 0, 3, 4, 7, 2, 12,
        1, 10, 14, 9, 10, 6, 9, 0, 12, 11, 7, 13, 15, 1, 3, 14, 5, 2, 8, 4, 3, 15, 0, 6, 10, 1, 13,
        8, 9, 4, 5, 11, 12, 7, 2, 14,
    ],
    [
        2, 12, 4, 1, 7, 10, 11, 6, 8, 5, 3, 15, 13, 0, 14, 9, 14, 11, 2, 12, 4, 7, 13, 1, 5, 0, 15,
        10, 3, 9, 8, 6, 4, 2, 1, 11, 10, 13, 7, 8, 15, 9, 12, 5, 6, 3, 0, 14, 11, 8, 12, 7, 1, 14, 2,
        13, 6, 15, 0, 9, 10, 4, 5, 3,
    ],
    [
        12, 1, 10, 15, 9, 2, 6, 8, 0, 13, 3, 4, 14, 7, 5, 11, 10, 15, 4, 2, 7, 12, 9, 5, 6, 1, 13,
        14, 0, 11, 3, 8, 9, 14, 15, 5, 2, 8, 12, 3, 7, 0, 4, 10, 1, 13, 11, 6, 4, 3, 2, 12, 9, 5, 15,
        10, 11, 14, 1, 7, 6, 0, 8, 13,
    ],
    [
        4, 11, 2, 14, 15, 0, 8, 13, 3, 12, 9, 7, 5, 10, 6, 1, 13, 0, 11, 7, 4, 9, 1, 10, 14, 3, 5,
        12, 2, 15, 8, 6, 1, 4, 11, 13, 12, 3, 7, 14, 10, 15, 6, 8, 0, 5, 9, 2, 6, 11, 13, 8, 1, 4,
        10, 7, 9, 5, 0, 15, 14, 2, 3, 12,
    ],
    [
        13, 2, 8, 4, 6, 15, 11, 1, 10, 9, 3, 14, 5, 0, 12, 7, 1, 15, 13, 8, 10, 3, 7, 4, 12, 5, 6,
        11, 0, 14, 9, 2, 7, 11, 4, 1, 9, 12, 14, 2, 0, 6, 10, 13, 15, 3, 5, 8, 2, 1, 14, 7, 4, 10, 8,
        13, 15, 12, 9, 0, 3, 5, 6, 11,
    ],
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn des_matches_the_classic_fips_test_vector() {
        let key = [0x13, 0x34, 0x57, 0x79, 0x9B, 0xBC, 0xDF, 0xF1];
        let plain = [0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF];
        let cipher = des_encrypt_block(&key, &plain);

        assert_eq!(
            cipher,
            [0x85, 0xE8, 0x13, 0x54, 0x0F, 0x0A, 0xB4, 0x05]
        );
        assert_eq!(des_decrypt_block(&key, &cipher), plain);
    }

    #[test]
    fn triple_des_round_trips() {
        let data = b"0123456789abcdefghijklmn".to_vec();
        let cipher = triple_des_encrypt_ecb(QRC_KEY, &data).unwrap();
        let plain = triple_des_decrypt_ecb(QRC_KEY, &cipher).unwrap();
        assert_eq!(plain, data);
        assert!(matches!(
            triple_des_decrypt_ecb(QRC_KEY, &[1, 2, 3]),
            Err(DecryptError::InvalidBlockLength)
        ));
    }

    #[test]
    fn qrc_round_trips_through_the_real_pipeline() {
        let payload = "[1000,1800]qrc(1000,300)zero(1300,0)word(1300,1500)";
        let encrypted = encrypt_qrc(payload).unwrap();
        assert_eq!(decrypt_qrc(&encrypted).unwrap(), payload);
        assert!(matches!(decrypt_qrc("zz"), Err(DecryptError::InvalidHex)));
    }

    #[test]
    fn krc_round_trips_through_the_real_pipeline() {
        let payload = "[1000,1800]<0,300,0>krc<300,0,0>zero<300,1500,0>word";
        let encrypted = encrypt_krc(payload);
        assert_eq!(decrypt_krc(&encrypted).unwrap(), payload);
        assert!(matches!(decrypt_krc("!!!!"), Err(DecryptError::InvalidBase64)));
    }
}
