#![allow(dead_code)]

use std::{fs::create_dir_all, path::{Path, PathBuf}, time::Duration};

use anyhow::{Context, Result};
use image::RgbImage;
use mupdf::{Document, Matrix, MetadataName};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use crate::utils::*;

#[derive(Debug, Clone)]
pub enum ScanProgress {
    Found(PathBuf),
    Processing(PathBuf),
    Extracted(String, PdfMetadata),
    DuplicateDetected(PathBuf, PathBuf),
    Error(PathBuf, String),
    Complete(Vec<PdfMetadata>, Duration),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PdfMetadata {
    pub hash: String,
    pub partial_hash: String,
    pub path: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
    pub page_count: u32,
    pub cover_path: Option<String>,
    pub file_size: u64,
}

pub struct PdfCache {
    pool: Pool<SqliteConnectionManager>,
    // conn: Connection,
    cache_dir: PathBuf,
}

impl PdfCache {
    pub fn new() -> Result<Self> {
        let cache_dir = dirs::home_dir().unwrap().join(".shelf");
        
        create_dir_all(&cache_dir)?;
        create_dir_all(cache_dir.join("covers"))?;
        
        let db_path = cache_dir.join("pdf_cache.db");
        let manager = SqliteConnectionManager::file(&db_path);
        let pool = Pool::new(manager)?;

        {
            let conn = pool.get()?;
            conn.execute(
                "CREATE TABLE IF NOT EXISTS pdf_metadata (
                    hash TEXT PRIMARY KEY,
                    partial_hash TEXT NOT NULL,
                    path TEXT NOT NULL,
                    title TEXT,
                    author TEXT,
                    subject TEXT,
                    keywords TEXT,
                    creator TEXT,
                    producer TEXT,
                    creation_date TEXT,
                    modification_date TEXT,
                    page_count INTEGER NOT NULL,
                    cover_path TEXT,
                    file_size INTEGER NOT NULL,
                    last_seen INTEGER NOT NULL
                )",
                [],
            )?;
            
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_partial_hash ON pdf_metadata(partial_hash)",
                [],
            )?;
            
            conn.execute(
                "CREATE INDEX IF NOT EXISTS idx_path ON pdf_metadata(path)",
                [],
            )?;
        }
        
        Ok(Self { pool, cache_dir })
    }
    
    pub fn get_by_partial_hash(&self, partial_hash: &str, file_size: u64) -> Result<Vec<PdfMetadata>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM pdf_metadata WHERE partial_hash = ?1 AND file_size = ?2"
        )?;
        
        let results = stmt.query_map(params![partial_hash, file_size], |row| {
            Ok(PdfMetadata {
                hash: row.get(0)?,
                partial_hash: row.get(1)?,
                path: row.get(2)?,
                title: row.get(3)?,
                author: row.get(4)?,
                subject: row.get(5)?,
                keywords: row.get(6)?,
                creator: row.get(7)?,
                producer: row.get(8)?,
                creation_date: row.get(9)?,
                modification_date: row.get(10)?,
                page_count: row.get(11)?,
                cover_path: row.get(12)?,
                file_size: row.get(13)?,
            })
        })?;
        
        results.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
    
    pub fn get_metadata(&self, hash: &str) -> Result<Option<PdfMetadata>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT * FROM pdf_metadata WHERE hash = ?1"
        )?;
        
        let result = stmt.query_row(params![hash], |row| {
            Ok(PdfMetadata {
                hash: row.get(0)?,
                partial_hash: row.get(1)?,
                path: row.get(2)?,
                title: row.get(3)?,
                author: row.get(4)?,
                subject: row.get(5)?,
                keywords: row.get(6)?,
                creator: row.get(7)?,
                producer: row.get(8)?,
                creation_date: row.get(9)?,
                modification_date: row.get(10)?,
                page_count: row.get(11)?,
                cover_path: row.get(12)?,
                file_size: row.get(13)?,
            })
        });
        
        match result {
            Ok(metadata) => Ok(Some(metadata)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    
    pub fn store_metadata(&self, metadata: &PdfMetadata) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT OR REPLACE INTO pdf_metadata 
            (hash, partial_hash, path, title, author, subject, keywords, creator, producer, 
             creation_date, modification_date, page_count, cover_path, file_size, last_seen)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                metadata.hash,
                metadata.partial_hash,
                metadata.path,
                metadata.title,
                metadata.author,
                metadata.subject,
                metadata.keywords,
                metadata.creator,
                metadata.producer,
                metadata.creation_date,
                metadata.modification_date,
                metadata.page_count,
                metadata.cover_path,
                metadata.file_size,
                now,
            ],
        )?;
        
        Ok(())
    }
}

/// Compute partial hash from:
/// - First 64KB of file
/// - Last 64KB of file
/// - File size
/// This is ~1000x faster than full hash for large files

pub fn extract_pdf_metadata(
    path: &Path,
    cache: &PdfCache,
    tx: &async_channel::Sender<ScanProgress>,
) -> Result<PdfMetadata> {
    // Step 1: Compute fast partial hash
    let (partial_hash, file_size) = compute_partial_hash(path)?;
    
    // Step 2: Check cache for matches with same partial hash and size
    let cached_matches = cache.get_by_partial_hash(&partial_hash, file_size)?;
    
    // Step 3: Handle cache hits
    if !cached_matches.is_empty() {
        let first_hit = cached_matches[0].clone();
        // Check if any cached entry has matching full hash
        if cached_matches.len() > 1 {
            let full_hash = compute_full_hash(path)?;
            for cached in cached_matches {
                if cached.hash == full_hash {
                    // Exact match found - update path if changed
                    if cached.path != path.to_string_lossy() {
                        let _ = tx.send_blocking(ScanProgress::DuplicateDetected(
                            PathBuf::from(&cached.path),
                            path.to_path_buf(),
                        ));
                    }
                    
                    // Return cached metadata with updated path
                    let mut updated = cached.clone();
                    updated.path = path.to_string_lossy().to_string();
                    cache.store_metadata(&updated)?;
                    return Ok(updated);
                }
            }
        } else {
            return Ok(first_hit); 
        }
    }
    
    println!("New file detected - {}", path.display());
    // Step 4: No cache hit - extract metadata from PDF
    let document = Document::open(path).unwrap();
    let page_count = document.page_count().unwrap() as u32;
    // let format = document.metadata(MetadataName::Format).ok();
    // let encryption = document.metadata(MetadataName::Encryption).ok();
    let author = document.metadata(MetadataName::Author).ok();
    let title = document.metadata(MetadataName::Title).ok();
    let producer = document.metadata(MetadataName::Producer).ok();
    let creator = document.metadata(MetadataName::Creator).ok();
    let creation_date = document.metadata(MetadataName::CreationDate).ok();
    let modification_date = document.metadata(MetadataName::ModDate).ok();
    let subject = document.metadata(MetadataName::Subject).ok();
    let keywords = document.metadata(MetadataName::Keywords).ok();
    
    // Compute full hash now (we need it for unique identification)
    let full_hash = compute_full_hash(path)?;
    
    // Step 5: Extract cover image
    let cover_path = if page_count > 0 {
        let page = document.load_page(0)?;
        
        // Calculate scale from DPI (default PDF is 72 DPI)
        let scale = 1.0;
        let matrix = Matrix::new_scale(scale, scale);
        
        // Render page to pixmap
        let pixmap = page.to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false, true)?;
        
        // Convert to image and save
        let width = pixmap.width() as u32;
        let height = pixmap.height() as u32;
        let samples = pixmap.samples();
        
        let image = RgbImage::from_raw(width, height, samples.to_vec())
            .context("Failed to create image from pixmap")?;
         
        let cover_filename = format!("{}.jpg", &full_hash[..16]);
        let cover_full_path = cache.cache_dir.join("covers").join(&cover_filename);
        
        image.save(&cover_full_path)?;
        Some(cover_filename)
    } else {
        None
    };
    
    let metadata = PdfMetadata {
        hash: full_hash,
        partial_hash,
        path: path.to_string_lossy().to_string(),
        title,
        author,
        subject,
        keywords,
        creator,
        producer,
        creation_date,
        modification_date,
        page_count,
        cover_path,
        file_size,
    };
    
    // Step 6: Store in cache
    println!("storing cache");
    cache.store_metadata(&metadata)?;
    
    Ok(metadata)
}


