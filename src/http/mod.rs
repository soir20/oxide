use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::path::{Component, PathBuf};

use axum::{Router, serve};
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::routing::get;
use miniz_oxide::deflate::compress_to_vec_zlib;
use serde::Deserialize;
use tokio::fs::{create_dir_all, OpenOptions, read, read_dir, remove_dir_all, write};
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const MAGIC: u32 = 0xa1b2c3d4;
const ZLIB_COMPRESSION_LEVEL: u8 = 6;
const COMPRESSED_EXTENSION: &str = "z";

#[derive(Deserialize)]
struct ManifestConfig {
    name: String,
    path: PathBuf
}

struct Manifest {
    name: OsString,
    prefix: PathBuf
}

async fn read_manifests_config(config_dir: &std::path::Path) -> io::Result<Vec<Manifest>> {
    let manifests_data = read(config_dir.join("manifests.json")).await?;
    let manifests: Vec<ManifestConfig> = serde_json::from_slice(&manifests_data)?;
    Ok(
        manifests.into_iter().map(|manifest_config| {
            let mut full_name = manifest_config.name;
            full_name.push_str("_manifest.txt");
            Manifest {
                name: OsString::from(full_name),
                prefix: manifest_config.path,
            }
        }).collect()
    )
}

fn append_extension(extension: impl AsRef<OsStr>, path: &std::path::Path) -> PathBuf {
    let mut os_string: OsString = path.into();
    os_string.push(".");
    os_string.push(extension.as_ref());
    os_string.into()
}

async fn list_files(root_dir: &std::path::Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let mut directories = VecDeque::new();
    directories.push_back(root_dir.to_path_buf());

    while let Some(dir) = directories.pop_front() {
        if dir.is_dir() {
            let mut entries = read_dir(dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let entry = entry;
                let path = entry.path();
                if path.is_dir() {
                    directories.push_back(path);
                } else {
                    files.push(path);
                }
            }
        }
    }

    Ok(files)
}

fn forward_slash_path(path: &std::path::Path) -> OsString {
    let mut os_string = OsString::new();
    for (index, component) in path.iter().enumerate() {
        if index > 0 {
            os_string.push("/");
        }

        os_string.push(component);
    }

    os_string
}

fn compressed_asset_name(asset_path: &std::path::Path, assets_path: &std::path::Path) -> PathBuf {
    append_extension(
        COMPRESSED_EXTENSION,
        asset_path.strip_prefix(&assets_path).expect("Asset entry path was not in the assets folder")
    )
}

async fn write_to_cache(uncompressed_contents: &[u8], compressed_asset_name: &std::path::Path,
                  assets_cache_path: &std::path::Path) -> io::Result<usize> {
    let mut compressed_contents = Vec::new();
    compressed_contents.write_u32(MAGIC).await?;
    compressed_contents.write_u32(uncompressed_contents.len() as u32).await?;
    compressed_contents.append(&mut compress_to_vec_zlib(&uncompressed_contents, ZLIB_COMPRESSION_LEVEL));

    let cached_asset_path = assets_cache_path.join(&compressed_asset_name);
    if let Some(parent) = cached_asset_path.parent() {
        create_dir_all(parent).await?;
    }
    write(&cached_asset_path, &compressed_contents).await?;
    Ok(compressed_contents.len())
}

async fn prepare_asset_cache(assets_path: &std::path::Path, assets_cache_path: &std::path::Path,
                       manifests: &[Manifest]) -> io::Result<()> {
    remove_dir_all(assets_cache_path).await?;
    create_dir_all(assets_cache_path).await?;
    let mut asset_paths = list_files(assets_path).await?;
    asset_paths.sort();

    for asset_path in asset_paths {
        let contents = read(&asset_path).await?;
        let compressed_asset_name = compressed_asset_name(&asset_path, assets_path);
        let bytes_written = write_to_cache(&contents, &compressed_asset_name, assets_cache_path).await?;

        // Determine which manifest this file belongs to, if any
        let manifest = manifests.iter().fold(
            (None, 0),
            |(current_manifest, current_depth), manifest| {
                if compressed_asset_name.starts_with(&manifest.prefix) {
                    let new_depth = &manifest.prefix.ancestors().count() - 1;
                    if new_depth >= current_depth {
                        return (Some(manifest), new_depth);
                    }
                }

                (current_manifest, current_depth)
            }
        );

        // Add this file to a manifest if necessary
        if let (Some(manifest), _) = manifest {
            let manifest_path = assets_cache_path.join(&manifest.prefix).join(&manifest.name);
            let crc = crc32fast::hash(&contents);
            let slash_asset_name = forward_slash_path(&compressed_asset_name);

            let mut manifest_entry = Vec::new();
            manifest_entry.append(&mut slash_asset_name.into_encoded_bytes());
            manifest_entry.push(b',');
            manifest_entry.write_all(crc.to_string().as_bytes()).await?;
            manifest_entry.push(b',');
            manifest_entry.write_all(bytes_written.to_string().as_bytes()).await?;
            manifest_entry.push(b'\n');

            let mut manifest_file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(manifest_path).await?;
            manifest_file.write_all(&manifest_entry).await?;
        }

    }

    // Compress manifest and create CRC file
    for manifest in manifests {
        create_dir_all(assets_cache_path.join(&manifest.prefix)).await?;
        let manifest_asset_name = &manifest.prefix.join(&manifest.name);
        let mut manifest_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(assets_cache_path.join(&manifest_asset_name)).await?;
        let mut manifest_contents = Vec::new();
        manifest_file.read_to_end(&mut manifest_contents).await?;

        let manifest_compressed_asset_name = append_extension(
            COMPRESSED_EXTENSION,
            &manifest_asset_name
        );
        write_to_cache(&manifest_contents, &manifest_compressed_asset_name, assets_cache_path).await?;

        let manifest_crc = crc32fast::hash(&manifest_contents).to_string();
        let manifest_crc_contents = manifest_crc.as_bytes();
        write_to_cache(manifest_crc_contents, &&manifest.prefix.join("manifest.crc.z"), assets_cache_path).await?;
    }

    Ok(())
}

async fn asset_handler(Path(asset_name): Path<PathBuf>, State(assets_cache_path): State<PathBuf>, request: Request) -> Result<Vec<u8>, StatusCode> {

    // SECURITY: Ensure that the path is within the assets cache before returning any data.
    // Reject all paths containing anything other than normal folder names (e.g. paths containing
    // the parent directory or the root directory).
    let is_invalid_path = asset_name.components().any(|component| {
        if let Component::Normal(_) = component {
            false
        } else {
            true
        }
    });
    if is_invalid_path {
        return Err(StatusCode::BAD_REQUEST);
    }

    let file_data = read(assets_cache_path.join(asset_name)).await.map_err(|_| StatusCode::NOT_FOUND)?;
    let crc = crc32fast::hash(&file_data);
    let queried_crc: u32 = if let Some(query) = request.uri().query() {
        str::parse(query).map_err(|_| StatusCode::BAD_REQUEST)?
    } else {
        crc
    };

    if crc == queried_crc {
        Ok(file_data)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn try_start(port: u16, config_dir: &std::path::Path, assets_path: &std::path::Path, assets_cache_path: PathBuf) -> io::Result<()> {
    let manifests = read_manifests_config(config_dir).await?;
    prepare_asset_cache(assets_path, &assets_cache_path, &manifests).await?;

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    let app: Router<()> = Router::new()
        .route("/assets/*asset", get(asset_handler))
        .with_state(assets_cache_path);

    serve(listener, app).await
}

pub async fn start(port: u16, config_dir: &std::path::Path, assets_path: &std::path::Path, assets_cache_path: PathBuf) {
    try_start(port, config_dir, assets_path, assets_cache_path).await.expect("Unable to start HTTP server");
}
