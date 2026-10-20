use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};
use aes::Aes128;
use anyhow::{anyhow, Result};

const CORE_KEY: &[u8; 16] = b"hzHRAmso5kInbaxW";
const KEY_PREFIX: &[u8] = b"neteasecloudmusic";

#[derive(Debug)]
pub(crate) struct DecodedNcm {
    pub audio: Vec<u8>,
    pub format: &'static str,
}

pub(crate) fn decode_ncm(data: &[u8]) -> Result<DecodedNcm> {
    if data.len() < 10 || !data.starts_with(b"CTENFDAM") {
        return Err(anyhow!("不是有效的NCM文件格式"));
    }

    let mut pos = 10;
    let key_length = read_length(data, &mut pos, "密钥长度")?;
    let encrypted_key = take_section(data, &mut pos, key_length, "密钥数据")?;
    if encrypted_key.is_empty() || encrypted_key.len() % 16 != 0 {
        return Err(anyhow!("加密密钥长度无效"));
    }
    let mut decrypted_key: Vec<u8> = encrypted_key.iter().map(|byte| byte ^ 0x64).collect();
    let cipher = Aes128::new(GenericArray::from_slice(CORE_KEY));
    for chunk in decrypted_key.chunks_exact_mut(16) {
        cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
    }
    let padding = *decrypted_key.last().unwrap() as usize;
    if padding == 0
        || padding > 16
        || padding > decrypted_key.len()
        || !decrypted_key[decrypted_key.len() - padding..]
            .iter()
            .all(|&byte| byte as usize == padding)
    {
        return Err(anyhow!("密钥填充无效"));
    }
    decrypted_key.truncate(decrypted_key.len() - padding);
    if !decrypted_key.starts_with(KEY_PREFIX) || decrypted_key.len() <= KEY_PREFIX.len() {
        return Err(anyhow!("密钥解密失败"));
    }
    let key = &decrypted_key[KEY_PREFIX.len()..];

    let mut key_box = [0u8; 256];
    for (index, byte) in key_box.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let mut last_byte = 0u8;
    for index in 0..256 {
        let swap = key_box[index];
        last_byte = swap
            .wrapping_add(last_byte)
            .wrapping_add(key[index % key.len()]);
        key_box[index] = key_box[last_byte as usize];
        key_box[last_byte as usize] = swap;
    }

    let meta_length = read_length(data, &mut pos, "元数据长度")?;
    take_section(data, &mut pos, meta_length, "元数据")?;
    // CRC32 (4 bytes) and image version (1 byte) precede two distinct lengths.
    take_section(data, &mut pos, 5, "封面头")?;
    let cover_frame_length = read_length(data, &mut pos, "封面预留长度")?;
    let image_length = read_length(data, &mut pos, "封面图片长度")?;
    if image_length > cover_frame_length {
        return Err(anyhow!("封面图片长度超过预留空间"));
    }
    // The frame includes padding, even when image_length is zero.
    take_section(data, &mut pos, cover_frame_length, "封面数据")?;
    let audio = &data[pos..];
    if audio.is_empty() {
        return Err(anyhow!("没有找到音频数据"));
    }

    let audio: Vec<u8> = audio
        .iter()
        .enumerate()
        .map(|(index, &byte)| {
            let j = (index + 1) & 0xff;
            let key_byte = key_box
                [(key_box[j] as usize + key_box[(key_box[j] as usize + j) & 0xff] as usize) & 0xff];
            byte ^ key_byte
        })
        .collect();
    let format = detect_audio_format(&audio)
        .ok_or_else(|| anyhow!("无法识别解密后的音频格式，文件可能损坏或不受支持"))?;
    Ok(DecodedNcm { audio, format })
}

fn take_section<'a>(
    data: &'a [u8],
    pos: &mut usize,
    length: usize,
    label: &str,
) -> Result<&'a [u8]> {
    let end = pos
        .checked_add(length)
        .ok_or_else(|| anyhow!("{}长度溢出", label))?;
    let section = data
        .get(*pos..end)
        .ok_or_else(|| anyhow!("{}数据不完整", label))?;
    *pos = end;
    Ok(section)
}

fn read_length(data: &[u8], pos: &mut usize, label: &str) -> Result<usize> {
    let bytes = take_section(data, pos, 4, label)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize)
}

fn detect_audio_format(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 4 {
        return None;
    }
    if bytes.starts_with(b"ID3") {
        return Some("mp3");
    }
    if bytes.starts_with(b"fLaC") {
        return Some("flac");
    }
    if bytes.starts_with(b"OggS") {
        return Some("ogg");
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        return Some("wav");
    }
    if bytes.len() >= 12 && bytes.get(4..8) == Some(b"ftyp") {
        return Some("m4a");
    }
    if bytes[0] == 0xff
        && bytes[1] & 0xe0 == 0xe0
        && bytes[1] & 0x18 != 0x08
        && bytes[1] & 0x06 == 0x02
        && bytes[2] & 0xf0 != 0xf0
        && bytes[2] & 0x0c != 0x0c
    {
        return Some("mp3");
    }
    None
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::Value;

    fn vectors() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/ncm-vectors.json")).unwrap()
    }

    fn vector(name: &str) -> Vec<u8> {
        hex::decode(vectors()[name].as_str().unwrap()).unwrap()
    }

    pub(crate) fn flac_audio() -> Vec<u8> {
        let mut audio = b"fLaC".to_vec();
        audio.extend((0..600).map(|index| index as u8));
        audio
    }

    pub(crate) fn fixture(audio: &[u8], capacity: u32, image_length: u32) -> Vec<u8> {
        fixture_with_key(audio, capacity, image_length, &vector("encryptedKey"))
    }

    fn fixture_with_key(audio: &[u8], capacity: u32, image_length: u32, key: &[u8]) -> Vec<u8> {
        let stream = vector("keyStream");
        let mut data = b"CTENFDAM\x01\x6d".to_vec();
        data.extend_from_slice(&(key.len() as u32).to_le_bytes());
        data.extend_from_slice(key);
        data.extend_from_slice(&0u32.to_le_bytes()); // no metadata
        data.extend_from_slice(&[0; 5]); // CRC32 and image version
        data.extend_from_slice(&capacity.to_le_bytes());
        data.extend_from_slice(&image_length.to_le_bytes());
        data.extend(vec![0xa5; capacity as usize]);
        data.extend(
            audio
                .iter()
                .enumerate()
                .map(|(index, &byte)| byte ^ stream[index % 256]),
        );
        data
    }

    #[test]
    fn decodes_padded_legacy_and_empty_cover_frames() {
        let audio = flac_audio();
        for (capacity, image_length) in [(0, 0), (23, 23), (7442, 0), (7442, 23)] {
            let decoded = decode_ncm(&fixture(&audio, capacity, image_length)).unwrap();
            assert_eq!(decoded.audio, audio);
            assert_eq!(decoded.format, "flac");
        }
    }

    #[test]
    fn identifies_supported_formats_from_audio_instead_of_metadata() {
        let cases: &[(&str, &[u8])] = &[
            ("mp3", b"ID3\x04"),
            ("mp3", &[0xff, 0xfb, 0x90, 0x64]),
            ("flac", b"fLaC"),
            ("ogg", b"OggS"),
            ("wav", b"RIFF\x00\x00\x00\x00WAVE"),
            ("m4a", b"\x00\x00\x00\x18ftypM4A "),
        ];
        for (format, header) in cases {
            let mut audio = header.to_vec();
            audio.extend_from_slice(&flac_audio()[4..]);
            let decoded = decode_ncm(&fixture(&audio, 17, 3)).unwrap();
            assert_eq!(decoded.format, *format);
            assert_eq!(decoded.audio, audio);
        }
    }

    #[test]
    fn skips_metadata_without_using_its_format_as_the_extension() {
        let mut data = fixture(&flac_audio(), 7442, 0);
        let metadata = vector("metadata");
        let offset = 14 + vector("encryptedKey").len();
        data.splice(
            offset..offset + 4,
            (metadata.len() as u32)
                .to_le_bytes()
                .into_iter()
                .chain(metadata),
        );
        let decoded = decode_ncm(&data).unwrap();
        assert_eq!(decoded.format, "flac");
        assert_eq!(decoded.audio, flac_audio());
    }

    #[test]
    fn rejects_image_larger_than_frame() {
        let error = decode_ncm(&fixture(&flac_audio(), 4, 5)).unwrap_err();
        assert!(error.to_string().contains("封面图片长度"));
    }

    #[test]
    fn rejects_sections_extending_beyond_eof() {
        let meta_offset = 14 + vector("encryptedKey").len();
        for offset in [10, meta_offset, meta_offset + 9] {
            let mut data = fixture(&flac_audio(), 0, 0);
            data[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(decode_ncm(&data).is_err());
        }
    }

    #[test]
    fn rejects_every_truncated_header() {
        let data = fixture(&flac_audio(), 0, 0);
        let audio_offset = 10 + 4 + vector("encryptedKey").len() + 4 + 13;
        for end in 0..=audio_offset {
            assert!(decode_ncm(&data[..end]).is_err(), "accepted {end} bytes");
        }
    }

    #[test]
    fn rejects_empty_misaligned_and_invalid_keys_without_panicking() {
        let mut keys = vec![Vec::new(), vector("encryptedKey")[1..].to_vec()];
        keys.extend(
            ["emptyKey", "invalidPrefixKey", "invalidPaddingKey"]
                .iter()
                .map(|name| vector(name)),
        );
        for key in keys {
            assert!(decode_ncm(&fixture_with_key(&flac_audio(), 0, 0, &key)).is_err());
        }
    }

    #[test]
    fn rejects_unknown_audio_and_reserved_mpeg_fields() {
        let headers: &[&[u8]] = &[
            &[0; 32],
            &[0xff, 0xf1, 0x50, 0x80], // AAC ADTS
            &[0xff, 0xeb, 0x90, 0x64], // reserved MPEG version
            &[0xff, 0xfb, 0xf0, 0x64], // reserved bitrate
            &[0xff, 0xfb, 0x9c, 0x64], // reserved sample rate
        ];
        for audio in headers {
            let error = decode_ncm(&fixture(audio, 0, 0)).unwrap_err();
            assert!(error.to_string().contains("无法识别"));
        }
    }

    #[test]
    fn rejects_absent_audio() {
        let error = decode_ncm(&fixture(&[], 7442, 0)).unwrap_err();
        assert!(error.to_string().contains("没有找到音频数据"));
    }
}
