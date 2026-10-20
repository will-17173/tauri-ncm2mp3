use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Emitter;
mod ncm;

// Windows编码处理函数
fn safe_path_to_string(path: &Path) -> String {
    #[cfg(windows)]
    {
        // Windows平台特殊处理
        use std::os::windows::ffi::OsStrExt;
        if let Some(os_str) = path.as_os_str().to_str() {
            os_str.to_string()
        } else {
            // 如果无法直接转换，使用UTF-16处理
            let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
            String::from_utf16_lossy(&wide)
        }
    }
    #[cfg(not(windows))]
    {
        path.to_string_lossy().to_string()
    }
}

fn safe_filename_to_string(path: &Path) -> String {
    if let Some(filename) = path.file_name() {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            if let Some(s) = filename.to_str() {
                s.to_string()
            } else {
                let wide: Vec<u16> = filename.encode_wide().collect();
                String::from_utf16_lossy(&wide)
            }
        }
        #[cfg(not(windows))]
        {
            filename.to_string_lossy().to_string()
        }
    } else {
        "unknown".to_string()
    }
}

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

    let file_paths: Vec<String> = ncm_files.iter().map(|p| safe_path_to_string(p)).collect();

    Ok(file_paths)
}

fn collect_ncm_files_recursive(dir: &Path, ncm_files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // 递归搜索子文件夹
                collect_ncm_files_recursive(&path, ncm_files);
            } else if path.extension().map(|s| s.to_string_lossy().to_lowercase())
                == Some("ncm".to_string())
            {
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
async fn convert_ncm_folder(
    folder_path: String,
    window: tauri::Window,
) -> Result<Vec<ConversionResult>, String> {
    let path = Path::new(&folder_path);
    let mut results = Vec::new();
    let mut ncm_files = Vec::new();

    collect_ncm_files(path, &mut ncm_files);

    let total = ncm_files.len();

    for (index, file_path) in ncm_files.iter().enumerate() {
        let progress = ConversionProgress {
            total,
            processed: index,
            current_file: safe_filename_to_string(file_path),
            status: "正在转换".to_string(),
        };

        let _ = window.emit("conversion-progress", &progress);

        let result = match convert_single_ncm(&safe_path_to_string(file_path)).await {
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
            } else if path.extension().map(|s| s.to_string_lossy().to_lowercase())
                == Some("ncm".to_string())
            {
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

    let decoded = ncm::decode_ncm(&fs::read(path)?)?;
    let output_path = path.with_extension(decoded.format);
    fs::write(&output_path, &decoded.audio)?;

    Ok(safe_path_to_string(&output_path))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_directory() -> PathBuf {
        static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "ncm2mp3-test-{}-{unique}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn writes_actual_extension_and_preserves_other_formats() {
        let dir = test_directory();
        let source = dir.join("回归样例.ncm");
        let existing_mp3 = dir.join("回归样例.mp3");
        fs::write(&existing_mp3, b"existing MP3").unwrap();
        let audio = ncm::tests::flac_audio();
        fs::write(&source, ncm::tests::fixture(&audio, 7442, 0)).unwrap();

        let output = convert_single_ncm(source.to_str().unwrap()).await.unwrap();
        assert_eq!(Path::new(&output), source.with_extension("flac"));
        assert_eq!(fs::read(&output).unwrap(), audio);
        assert_eq!(fs::read(&existing_mp3).unwrap(), b"existing MP3");

        let mp3_source = dir.join("MP3样例.NCM");
        let mp3_audio = b"ID3\x04test payload";
        fs::write(&mp3_source, ncm::tests::fixture(mp3_audio, 23, 23)).unwrap();
        let output = convert_single_ncm(mp3_source.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(Path::new(&output), mp3_source.with_extension("mp3"));
        assert_eq!(fs::read(output).unwrap(), mp3_audio);
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn invalid_audio_does_not_create_an_output_file() {
        let dir = test_directory();
        let source = dir.join("invalid.ncm");
        fs::write(&source, ncm::tests::fixture(&[0; 32], 0, 0)).unwrap();
        assert!(convert_single_ncm(source.to_str().unwrap()).await.is_err());
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    #[ignore = "Set NCM_SAMPLE_PATH to validate a local NCM file without adding music to the repository"]
    async fn convert_local_sample() {
        let original = PathBuf::from(
            std::env::var_os("NCM_SAMPLE_PATH").expect("NCM_SAMPLE_PATH is required"),
        );
        let dir = test_directory();
        let source = dir.join(original.file_name().unwrap());
        fs::copy(&original, &source).unwrap();
        let output = convert_single_ncm(source.to_str().unwrap()).await.unwrap();
        println!("Validated local sample output: {output}");
        assert!(fs::metadata(output).unwrap().len() > 0);
    }
}
