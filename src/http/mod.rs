use std::collections::{BTreeMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::path::{Component, PathBuf};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{serve, Router};
use miniz_oxide::deflate::compress_to_vec_zlib;
use miniz_oxide::inflate::decompress_to_vec_zlib;
use tokio::fs::{create_dir_all, read, read_dir, remove_dir_all, try_exists, write, OpenOptions};
use tokio::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const COMPRESSED_MAGIC: u32 = 0xa1b2c3d4;
const ZLIB_COMPRESSION_LEVEL: u8 = 6;
const COMPRESSED_EXTENSION: &str = "z";
const CRC_EXTENSION_SEPARATOR: &str = "_";
const MANIFEST_NAME: &str = "manifest.txt";

struct Manifest {
    name: OsString,
    prefix: PathBuf,
}

async fn read_manifests_config(config_dir: &std::path::Path) -> io::Result<Vec<Manifest>> {
    let manifests_data = read(config_dir.join("manifests.json")).await?;
    let manifests: Vec<PathBuf> = serde_json::from_slice(&manifests_data)?;
    Ok(manifests
        .into_iter()
        .map(|manifest_path| Manifest {
            name: OsString::from(MANIFEST_NAME),
            prefix: manifest_path,
        })
        .collect())
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
        asset_path
            .strip_prefix(assets_path)
            .expect("Asset entry path was not in the assets folder"),
    )
}

async fn write_to_cache(
    uncompressed_contents: &[u8],
    compressed_asset_name: &std::path::Path,
    assets_cache_path: &std::path::Path,
    crc_map: &mut CrcMap,
) -> io::Result<usize> {
    crc_map.insert(
        compressed_asset_name.to_path_buf(),
        crc32fast::hash(uncompressed_contents),
    );

    let mut compressed_contents = Vec::new();
    compressed_contents.write_u32(COMPRESSED_MAGIC).await?;
    compressed_contents
        .write_u32(uncompressed_contents.len() as u32)
        .await?;
    compressed_contents.append(&mut compress_to_vec_zlib(
        uncompressed_contents,
        ZLIB_COMPRESSION_LEVEL,
    ));

    let cached_asset_path = assets_cache_path.join(compressed_asset_name);
    if let Some(parent) = cached_asset_path.parent() {
        create_dir_all(parent).await?;
    }
    write(&cached_asset_path, &compressed_contents).await?;
    Ok(compressed_contents.len())
}

type CrcMap = BTreeMap<PathBuf, u32>;
async fn prepare_asset_cache(
    assets_path: &std::path::Path,
    assets_cache_path: &std::path::Path,
    manifests: &[Manifest],
) -> io::Result<CrcMap> {
    if try_exists(assets_cache_path).await? {
        remove_dir_all(assets_cache_path).await?;
    }
    create_dir_all(assets_cache_path).await?;
    let mut asset_paths = list_files(assets_path).await?;
    asset_paths.sort();

    let mut crc_map = CrcMap::new();

    for asset_path in asset_paths {
        let contents = read(&asset_path).await?;
        let compressed_asset_name = compressed_asset_name(&asset_path, assets_path);
        let bytes_written = write_to_cache(
            &contents,
            &compressed_asset_name,
            assets_cache_path,
            &mut crc_map,
        )
        .await?;

        // Determine which manifest this file belongs to, if any
        let manifest =
            manifests
                .iter()
                .fold((None, 0), |(current_manifest, current_depth), manifest| {
                    if compressed_asset_name.starts_with(&manifest.prefix) {
                        let new_depth = &manifest.prefix.ancestors().count() - 1;
                        if new_depth >= current_depth {
                            return (Some(manifest), new_depth);
                        }
                    }

                    (current_manifest, current_depth)
                });

        // Add this file to a manifest if necessary
        if let (Some(manifest), _) = manifest {
            let manifest_path = assets_cache_path
                .join(&manifest.prefix)
                .join(&manifest.name);
            let crc = crc32fast::hash(&contents);
            let slash_asset_name = forward_slash_path(&compressed_asset_name);

            let mut manifest_entry = Vec::new();
            manifest_entry.append(&mut slash_asset_name.into_encoded_bytes());
            manifest_entry.push(b',');
            manifest_entry.write_all(crc.to_string().as_bytes()).await?;
            manifest_entry.push(b',');
            manifest_entry
                .write_all(bytes_written.to_string().as_bytes())
                .await?;
            manifest_entry.push(b'\n');

            let mut manifest_file = OpenOptions::new()
                .create(true)
                .truncate(false)
                .append(true)
                .open(manifest_path)
                .await?;
            manifest_file.write_all(&manifest_entry).await?;
        }
    }

    // Compress manifest and create CRC file
    for manifest in manifests {
        create_dir_all(assets_cache_path.join(&manifest.prefix)).await?;
        let manifest_asset_name = &manifest.prefix.join(&manifest.name);
        let mut manifest_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(assets_cache_path.join(manifest_asset_name))
            .await?;
        let mut manifest_contents = Vec::new();
        manifest_file.read_to_end(&mut manifest_contents).await?;
        let manifest_crc = crc32fast::hash(&manifest_contents);
        crc_map.insert(manifest_asset_name.clone(), manifest_crc);

        let manifest_compressed_asset_name =
            append_extension(COMPRESSED_EXTENSION, manifest_asset_name);
        write_to_cache(
            &manifest_contents,
            &manifest_compressed_asset_name,
            assets_cache_path,
            &mut crc_map,
        )
        .await?;

        let manifest_crc_string = manifest_crc.to_string();
        let manifest_crc_contents = manifest_crc_string.as_bytes();
        write_to_cache(
            manifest_crc_contents,
            &manifest.prefix.join("manifest.crc.z"),
            assets_cache_path,
            &mut crc_map,
        )
        .await?;
    }

    Ok(crc_map)
}

fn decompose_extension(asset_name: &std::path::Path) -> (PathBuf, bool, Option<u32>) {
    let possible_extension_str = asset_name
        .extension()
        .map(|extension| extension.to_os_string().into_string().ok())
        .unwrap_or(None);
    let (non_crc_asset_name, crc) = if let Some(extension_str) = possible_extension_str {
        let extension_split = extension_str.rsplit_once(CRC_EXTENSION_SEPARATOR);

        if let Some((real_extension, crc_str)) = extension_split {
            (
                asset_name.with_extension(real_extension),
                crc_str.parse::<u32>().ok(),
            )
        } else {
            (asset_name.to_path_buf(), None)
        }
    } else {
        (asset_name.to_path_buf(), None)
    };

    let compressed = non_crc_asset_name
        .extension()
        .map(|extension| extension == COMPRESSED_EXTENSION)
        .unwrap_or(false);
    let compressed_asset_name = if compressed {
        non_crc_asset_name.to_path_buf()
    } else {
        append_extension(COMPRESSED_EXTENSION, &non_crc_asset_name)
    };

    (compressed_asset_name, compressed, crc)
}

async fn retrieve_asset(
    asset_name: PathBuf,
    assets_cache_path: Arc<PathBuf>,
    crc_map: Arc<CrcMap>,
) -> Result<Vec<u8>, StatusCode> {
    // SECURITY: Ensure that the path is within the assets cache before returning any data.
    // Reject all paths containing anything other than normal folder names (e.g. paths containing
    // the parent directory or the root directory).
    let is_invalid_path = asset_name
        .components()
        .any(|component| !matches!(component, Component::Normal(_)));
    if is_invalid_path {
        return Err(StatusCode::BAD_REQUEST);
    }

    let (compressed_asset_name, compress, queried_crc) = decompose_extension(&asset_name);

    // Do CRC checks first since that is faster than checking the file system
    let crc = *crc_map
        .get(&compressed_asset_name)
        .ok_or(StatusCode::NOT_FOUND)?;
    if crc != queried_crc.unwrap_or(crc) {
        return Err(StatusCode::NOT_FOUND);
    }

    let asset_path = assets_cache_path.join(&compressed_asset_name);
    let compressed_data = read(asset_path).await.map_err(|_| StatusCode::NOT_FOUND)?;
    if compress {
        Ok(compressed_data)
    } else {
        // Skip the 4-byte magic number and 4-byte length comprising the compressed header
        decompress_to_vec_zlib(&compressed_data[8..]).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }
}

fn is_name_hash(component: &OsStr) -> bool {
    let is_hash_length = component.len() == 3;
    is_hash_length
        && if let Ok(comp_str) = component.to_os_string().into_string() {
            comp_str.parse::<u16>().is_ok()
        } else {
            false
        }
}

async fn asset_handler(
    Path(asset): Path<PathBuf>,
    State((assets_cache_path, crc_map)): State<(Arc<PathBuf>, Arc<CrcMap>)>,
) -> Result<Vec<u8>, StatusCode> {
    let is_first_component_name_hash = asset.iter().next().map(is_name_hash).unwrap_or(false);

    // Ignore the name hash if it is included
    let asset_name = if is_first_component_name_hash {
        let mut components = asset.components();
        components.next();
        components.as_path().to_path_buf()
    } else {
        asset
    };

    retrieve_asset(asset_name, assets_cache_path, crc_map).await
}

async fn try_start(
    port: u16,
    config_dir: &std::path::Path,
    assets_path: &std::path::Path,
    assets_cache_path: PathBuf,
) -> io::Result<()> {
    let manifests = read_manifests_config(config_dir).await?;
    let crc_map = prepare_asset_cache(assets_path, &assets_cache_path, &manifests).await?;

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    let app: Router<()> = Router::new()
        .route("/assets/*asset", get(asset_handler))
        .with_state((Arc::new(assets_cache_path), Arc::new(crc_map)));

    serve(listener, app).await
}

pub async fn start(
    port: u16,
    config_dir: &std::path::Path,
    assets_path: &std::path::Path,
    assets_cache_path: PathBuf,
) {
    try_start(port, config_dir, assets_path, assets_cache_path)
        .await
        .expect("Unable to start HTTP server");
}
