mod pack;

pub use pack::*;
use walkdir::WalkDir;

use std::{
    collections::{HashMap, VecDeque},
    iter,
    path::{Path, PathBuf},
};

use tokio::{
    fs::{read_dir, File, OpenOptions},
    task::JoinSet,
};

pub trait DeserializeAsset: Sized {
    fn deserialize(
        path: PathBuf,
        file: &mut File,
    ) -> impl std::future::Future<Output = Result<Self, tokio::io::Error>> + Send;
}

pub struct Asset {
    path: PathBuf,
    offset: u64,
}

async fn list_assets_in_file(path: PathBuf, mut file: File) -> HashMap<String, Asset> {
    let is_pack = path
        .extension()
        .map(|ext| ext.to_ascii_lowercase() == "pack")
        .unwrap_or(false);
    match is_pack {
        true => {
            let Ok(pack) = Pack::deserialize(path.clone(), &mut file).await else {
                return HashMap::new();
            };

            pack.flatten()
        }
        false => {
            let Some(Ok(name)) = path
                .file_name()
                .map(|name| name.to_os_string().into_string())
            else {
                return HashMap::new();
            };

            let mut results: HashMap<_, _> = HashMap::new();
            results.insert(
                name,
                Asset {
                    path: path.clone(),
                    offset: 0,
                },
            );

            results
        }
    }
}

pub async fn list_assets<P: AsRef<Path>>(
    directory_path: P,
    follow_links: bool,
    mut predicate: impl FnMut(&Path) -> bool,
) -> Result<HashMap<String, Asset>, tokio::io::Error> {
    let mut futures = JoinSet::new();

    let walker = WalkDir::new(&directory_path)
        .follow_links(follow_links)
        .into_iter();
    for entry in walker.filter_map(|err| err.ok()) {
        if predicate(entry.path()) {
            // Per PathBuf#isFile():
            // When the goal is simply to read from (or write to) the source, the most reliable way
            // to test the source can be read (or written to) is to open it.
            if let Ok(file) = OpenOptions::new().read(true).open(entry.path()).await {
                futures.spawn(list_assets_in_file(entry.into_path(), file));
            }
        }
    }

    let mut final_result = HashMap::new();
    futures
        .join_all()
        .await
        .into_iter()
        .for_each(|result| final_result.extend(result.into_iter()));
    Ok(final_result)
}
