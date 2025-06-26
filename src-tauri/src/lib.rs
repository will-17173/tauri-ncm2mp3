use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use aes::Aes128;
use aes::cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray};

#[derive(Debug, Serialize, Deserialize)]
pub struct ConversionResult {
    pub success: bool,
    pub message: String,
    pub output_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConversionProgress {
    pub total: usize,
    pub processed: usize,
    pub current_file: String,
    pub status: String,
}

#[tauri::command]
async fn is_directory(path: String) -> Result<bool, String> {
    let path = Path::new(&path);
    Ok(path.is_dir())
}

#[tauri::command]
async fn find_ncm_files(folder_path: String) -> Result<Vec<String>, String> {
    let path = Path::new(&folder_path);
    let mut ncm_files = Vec::new();
    
    if !path.exists() {
        return Err("文件夹不存在".to_string());
    }
    
    if !path.is_dir() {
        return Err("路径不是文件夹".to_string());
    }
    
    collect_ncm_files_recursive(path, &mut ncm_files);
    
    let file_paths: Vec<String> = ncm_files
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    
    Ok(file_paths)
}

fn collect_ncm_files_recursive(dir: &Path, ncm_files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // 递归搜索子文件夹
                collect_ncm_files_recursive(&path, ncm_files);
            } else if path.extension().map(|s| s.to_string_lossy().to_lowercase()) == Some("ncm".to_string()) {
                ncm_files.push(path);
            }
        }
    }
}

#[tauri::command]
async fn convert_ncm_file(file_path: String) -> Result<ConversionResult, String> {
    match convert_single_ncm(&file_path).await {
        Ok(output_path) => Ok(ConversionResult {
            success: true,
            message: "转换成功".to_string(),
            output_path: Some(output_path),
        }),
        Err(e) => Ok(ConversionResult {
            success: false,
            message: format!("转换失败: {}", e),
            output_path: None,
        }),
    }
}

#[tauri::command]
async fn convert_ncm_folder(folder_path: String, window: tauri::Window) -> Result<Vec<ConversionResult>, String> {
    let path = Path::new(&folder_path);
    let mut results = Vec::new();
    let mut ncm_files = Vec::new();
    
    collect_ncm_files(path, &mut ncm_files);
    
    let total = ncm_files.len();
    
    for (index, file_path) in ncm_files.iter().enumerate() {
        let progress = ConversionProgress {
            total,
            processed: index,
            current_file: file_path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            status: "正在转换".to_string(),
        };
        
        let _ = window.emit("conversion-progress", &progress);
        
        let result = match convert_single_ncm(&file_path.to_string_lossy()).await {
            Ok(output_path) => ConversionResult {
                success: true,
                message: "转换成功".to_string(),
                output_path: Some(output_path),
            },
            Err(e) => ConversionResult {
                success: false,
                message: format!("转换失败: {}", e),
                output_path: None,
            },
        };
        
        results.push(result);
    }
    
    let final_progress = ConversionProgress {
        total,
        processed: total,
        current_file: "".to_string(),
        status: "转换完成".to_string(),
    };
    let _ = window.emit("conversion-progress", &final_progress);
    
    Ok(results)
}

fn collect_ncm_files(dir: &Path, ncm_files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_ncm_files(&path, ncm_files);
            } else if path.extension().map(|s| s.to_string_lossy().to_lowercase()) == Some("ncm".to_string()) {
                ncm_files.push(path);
            }
        }
    }
}

async fn convert_single_ncm(file_path: &str) -> Result<String> {
    let path = Path::new(file_path);
    if !path.exists() {
        return Err(anyhow!("文件不存在"));
    }
    
    if path.extension().map(|s| s.to_string_lossy().to_lowercase()) != Some("ncm".to_string()) {
        return Err(anyhow!("不是NCM文件"));
    }
    
    let file_data = fs::read(path)?;
    
    if file_data.len() < 10 {
        return Err(anyhow!("文件太小，不是有效的NCM文件"));
    }
    
    // 检查NCM文件头 - "CTENFDAM"
    if &file_data[0..8] != b"CTENFDAM" {
        return Err(anyhow!("不是有效的NCM文件格式"));
    }
    
    let mut pos = 10; // 跳过文件头和版本号
    
    // 1. 读取并解密密钥数据
    if pos + 4 > file_data.len() {
        return Err(anyhow!("文件数据不完整"));
    }
    let key_length = u32::from_le_bytes([
        file_data[pos], file_data[pos + 1], 
        file_data[pos + 2], file_data[pos + 3]
    ]) as usize;
    pos += 4;
    
    if pos + key_length > file_data.len() {
        return Err(anyhow!("密钥数据不完整"));
    }
    let encrypted_key = &file_data[pos..pos + key_length];
    pos += key_length;
    
    // 使用0x64异或解密密钥数据
    let mut key_data_array = Vec::new();
    for &byte in encrypted_key {
        key_data_array.push(byte ^ 0x64);
    }
    
    // 使用AES-128-ECB解密密钥
    let core_key = hex::decode("687A4852416D736F356B496E62617857").unwrap();
    let cipher = Aes128::new(GenericArray::from_slice(&core_key));
    
    // 去掉PKCS7填充
    let mut decrypted_key = key_data_array.clone();
    while decrypted_key.len() % 16 != 0 {
        decrypted_key.push(0);
    }
    
    for chunk in decrypted_key.chunks_mut(16) {
        if chunk.len() == 16 {
            let mut block = GenericArray::clone_from_slice(chunk);
            cipher.decrypt_block(&mut block);
            chunk.copy_from_slice(&block);
        }
    }
    
    // 去掉PKCS7填充
    if !decrypted_key.is_empty() {
        let padding_len = decrypted_key[decrypted_key.len() - 1] as usize;
        if padding_len <= 16 && padding_len <= decrypted_key.len() {
            decrypted_key.truncate(decrypted_key.len() - padding_len);
        }
    }
    
    // 跳过前17个字节（"neteasecloudmusic"）
    if decrypted_key.len() < 17 {
        return Err(anyhow!("解密后的密钥长度不足"));
    }
    let key_data = &decrypted_key[17..];
    
    // 2. 生成密钥盒（RC4密钥调度算法）
    let mut key_box = [0u8; 256];
    for i in 0..256 {
        key_box[i] = i as u8;
    }
    
    let mut last_byte = 0u8;
    let mut key_offset = 0;
    for i in 0..256 {
        let swap = key_box[i];
        last_byte = (swap as usize + last_byte as usize + key_data[key_offset] as usize) as u8;
        key_offset += 1;
        if key_offset >= key_data.len() {
            key_offset = 0;
        }
        key_box[i] = key_box[last_byte as usize];
        key_box[last_byte as usize] = swap;
    }
    
    // 3. 跳过meta数据
    if pos + 4 > file_data.len() {
        return Err(anyhow!("元数据长度信息缺失"));
    }
    let meta_length = u32::from_le_bytes([
        file_data[pos], file_data[pos + 1], 
        file_data[pos + 2], file_data[pos + 3]
    ]) as usize;
    pos += 4;
    pos += meta_length;
    
    // 4. 跳过CRC32（4字节）和间隔（5字节）
    pos += 9;
    
    // 5. 跳过专辑图片
    if pos + 4 > file_data.len() {
        return Err(anyhow!("图片长度信息缺失"));
    }
    let image_length = u32::from_le_bytes([
        file_data[pos], file_data[pos + 1], 
        file_data[pos + 2], file_data[pos + 3]
    ]) as usize;
    pos += 4;
    pos += image_length;
    
    // 6. 解密音频数据
    if pos >= file_data.len() {
        return Err(anyhow!("没有找到音频数据"));
    }
    
    let audio_data = &file_data[pos..];
    let mut decrypted_audio = Vec::with_capacity(audio_data.len());
    
    // 按照参考算法解密音频数据
    for (i, &byte) in audio_data.iter().enumerate() {
        let j = (i + 1) & 0xff;
        let key_byte = key_box[(key_box[j] as usize + key_box[(key_box[j] as usize + j) & 0xff] as usize) & 0xff];
        decrypted_audio.push(byte ^ key_byte);
    }
    
    let output_path = path.with_extension("mp3");
    fs::write(&output_path, &decrypted_audio)?;
    
    Ok(output_path.to_string_lossy().to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            convert_ncm_file, 
            convert_ncm_folder, 
            is_directory, 
            find_ncm_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
