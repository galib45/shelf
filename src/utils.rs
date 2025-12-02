#![allow(dead_code)]

use std::{
    fs::{read_dir, File}, 
    io::{Read, Seek, SeekFrom}, path::{Path, PathBuf}
};
use anyhow::Result;
use blake3::Hasher;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::pdf::ScanProgress;

pub fn scan_pdfs_rayon(dir: &PathBuf, tx: async_channel::Sender<ScanProgress>) -> Vec<PathBuf> {
    let mut pdfs = Vec::new();
    let mut subdirs = Vec::new();
    let entries = read_dir(&dir).unwrap();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) {
            pdfs.push(path.clone());
            let _ = tx.send_blocking(ScanProgress::Found(path));
        } else if path.is_dir() {
            subdirs.push(path);
        }
    }

    // Process subdirectories recursively in parallel
    let sub_pdfs: Vec<PathBuf> = subdirs
        .par_iter()
        .flat_map(|subdir| scan_pdfs_rayon(subdir, tx.clone()))
        .collect();

    pdfs.extend(sub_pdfs);
    pdfs
}

pub fn compute_partial_hash(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path)?;
    let file_size = file.metadata()?.len();
    let mut hasher = Hasher::new();
    
    // Hash file size first (important discriminator)
    hasher.update(&file_size.to_le_bytes());
    
    // Hash first 64KB
    let mut buffer = vec![0u8; 65536];
    let n = file.read(&mut buffer)?;
    hasher.update(&buffer[..n]);
    
    // Hash last 64KB (if file is large enough)
    if file_size > 65536 {
        file.seek(SeekFrom::End(-65536))?;
        let n = file.read(&mut buffer)?;
        hasher.update(&buffer[..n]);
    }
    
    Ok((hasher.finalize().to_hex().to_string(), file_size))
}

/// Compute full file hash (only when needed for duplicate detection)
pub fn compute_full_hash(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Hasher::new();
    let mut buffer = vec![0; 65536];
    
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    
    Ok(hasher.finalize().to_hex().to_string())
}

fn human_readable_file_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
