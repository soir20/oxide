mod pack;

pub use pack::*;

use std::{
    collections::{HashMap, VecDeque},
    iter,
    path::PathBuf,
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

async fn list_assets_in_file(path: PathBuf) -> HashMap<String, Asset> {
    let is_pack = path
        .extension()
        .map(|ext| ext.to_ascii_lowercase() == "pack")
        .unwrap_or(false);
    match is_pack {
        true => {
            let Ok(mut file) = OpenOptions::new().read(true).open(&path).await else {
                return HashMap::new();
            };
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

pub async fn list_assets(
    directory_path: &PathBuf,
    mut predicate: impl FnMut(&PathBuf) -> bool,
) -> Result<HashMap<String, Asset>, tokio::io::Error> {
    let mut queue = VecDeque::new();
    queue.push_back(directory_path.clone());
    let mut futures = JoinSet::new();

    while let Some(directory_path) = queue.pop_front() {
        let Ok(mut directory) = read_dir(directory_path).await else {
            continue;
        };

        while let Ok(Some(entry)) = directory.next_entry().await {
            let file_path = entry.path();
            match entry
                .file_type()
                .await
                .is_ok_and(|file_type| file_type.is_dir())
            {
                true => queue.push_back(file_path),
                false => {
                    if predicate(&file_path) {
                        futures.spawn(list_assets_in_file(file_path));
                    }
                }
            }
        }
    }

    let results = futures.join_all().await;
    Ok(HashMap::new())
}
